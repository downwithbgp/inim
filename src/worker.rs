//! The separate analysis worker process (`inim worker`).
//!
//! The worker is the ONLY component that performs source access and
//! analysis. The web server enqueues jobs; the worker claims them
//! transactionally, executes them through the shared execution service,
//! stages and validates artifacts, and publishes completed runs
//! atomically. On shutdown it stops claiming, requests cooperative
//! cancellation, writes a final heartbeat, and leaves the job in a
//! recoverable state — it never marks a partially executing job
//! Completed and never silently abandons a lease.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use rusqlite::Connection;

use crate::catalog::jobs::plan::{canonical_plan_hash, manifest_payload_for_plan};
use crate::catalog::jobs::publish::{
    enter_publishing, fail_job as fail_job_db, publish_staged_run, validate_staged,
    write_execution_metadata, ExecutionMetadata, PublishInputs, EXECUTION_METADATA_SCHEMA_VERSION,
};

/// Fail a job from any non-terminal state, preserving the error record.
fn fail_job(
    conn: &Arc<Mutex<Connection>>,
    job_id: &str,
    code: &str,
    summary: &str,
) -> Result<(), String> {
    let conn = conn.lock().unwrap();
    fail_job_db(&conn, job_id, code, summary)
}
use crate::catalog::jobs::service::{self, JobClaim, WorkerHeartbeat};
use crate::catalog::jobs::{error_code, JobState};
use crate::execution::{self, ExecutionConfig, ExecutionError, ProgressEvent, ProgressSink};

/// Worker configuration. All defaults are bounded: one job at a time,
/// conservative download/parse budgets.
#[derive(Debug, Clone)]
pub struct WorkerConfig {
    pub db_path: PathBuf,
    /// Catalog root; all job paths are stored root-relative.
    pub root: PathBuf,
    pub worker_id: Option<String>,
    pub poll_interval: Duration,
    pub max_jobs: usize,
    pub download_jobs: usize,
    pub parse_jobs: usize,
    pub once: bool,
    pub offline: bool,
    pub lease_secs: i64,
    pub heartbeat_secs: i64,
    pub keep_failed_workdir: bool,
    pub show_execution_plan: bool,
}

impl Default for WorkerConfig {
    fn default() -> Self {
        WorkerConfig {
            db_path: PathBuf::from("data/inim.sqlite"),
            root: PathBuf::from("."),
            worker_id: None,
            poll_interval: Duration::from_secs(2),
            max_jobs: 1,
            download_jobs: 2,
            parse_jobs: 8,
            once: false,
            offline: false,
            lease_secs: service::DEFAULT_LEASE_SECS,
            heartbeat_secs: service::DEFAULT_HEARTBEAT_SECS,
            keep_failed_workdir: false,
            show_execution_plan: false,
        }
    }
}

/// Reject unsafe topologies: `max_jobs * parse_jobs` must not exceed the
/// documented safety bound silently. Alpha behavior: max_jobs > 1
/// requires an explicit parse budget (per-job), and the product is
/// capped at the host's available parallelism.
pub fn validate_topology(config: &WorkerConfig) -> Result<(), String> {
    if config.max_jobs == 0 {
        return Err("max_jobs must be >= 1".to_string());
    }
    if config.parse_jobs == 0 {
        return Err("parse_jobs must be >= 1".to_string());
    }
    if config.download_jobs == 0 {
        return Err("download_jobs must be >= 1".to_string());
    }
    // The documented safety bound governs the MULTI-JOB product: two
    // 8-parser jobs running concurrently would oversubscribe a small
    // host. A single job keeps the established per-job parse default
    // (the same budget as direct `inim analyze`), which the pipeline
    // caps by archive count at runtime.
    if config.max_jobs > 1 {
        let cpus = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1);
        let total_parsers = config.max_jobs.saturating_mul(config.parse_jobs);
        if config.parse_jobs > 8 || total_parsers > cpus {
            return Err(format!(
                "unsafe worker topology: max_jobs {} x parse_jobs {} = {total_parsers} parser threads exceeds the documented safety bound ({cpus} logical CPUs, max 8 per job); reduce max_jobs or parse_jobs",
                config.max_jobs, config.parse_jobs
            ));
        }
    }
    Ok(())
}

/// The execution plan a worker would use (for --show-execution-plan).
pub fn execution_plan(config: &WorkerConfig) -> serde_json::Value {
    let cpus = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1);
    serde_json::json!({
        "logical_cpus": cpus,
        "max_concurrent_jobs": config.max_jobs,
        "download_workers": config.download_jobs,
        "parse_workers": config.parse_jobs,
        "expected_max_parser_threads": config.max_jobs.saturating_mul(config.parse_jobs),
        "offline": config.offline,
    })
}

/// Validate a job id before it is used in filesystem paths. Job ids
/// are generated as 32 lowercase hex chars; anything else (path
/// separators, dots, slashes) is rejected so a crafted id can never
/// escape the job staging/run roots.
pub fn valid_job_id(id: &str) -> bool {
    id.len() == 32
        && id
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}

/// Generate a stable process-lifetime worker id (not hostname-derived).
pub fn generate_worker_id(conn: &Connection) -> Result<String, String> {
    conn.query_row("SELECT 'w-' || lower(hex(randomblob(8)))", [], |r| r.get(0))
        .map_err(|e| format!("cannot generate worker id: {e}"))
}

/// Local-only discovery for `--offline`: scans the cache directory for
/// archives within the requested window. A job that needs uncached
/// acquisition fails with `archive_not_cached`.
pub struct CacheScanDiscovery {
    cache_dir: PathBuf,
}

impl CacheScanDiscovery {
    pub fn new(cache_dir: PathBuf) -> Self {
        CacheScanDiscovery { cache_dir }
    }
}

impl crate::discover::ArchiveDiscovery for CacheScanDiscovery {
    fn query(
        &self,
        _project: &str,
        collectors: &[&str],
        ts_start: chrono::DateTime<chrono::Utc>,
        ts_end: chrono::DateTime<chrono::Utc>,
        data_type: &str,
    ) -> Result<Vec<crate::discover::ArchiveItem>, crate::discover::InimArchiveError> {
        let mut items = Vec::new();
        for collector in collectors {
            let dir = self.cache_dir.join(collector).join(data_type);
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue; // no cached material for this collector/type
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_file() || path.extension().is_none() {
                    continue;
                }
                let name = path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                let Some(ts) = crate::discover::filename_timestamp(&name) else {
                    continue;
                };
                if ts < ts_start || ts > ts_end {
                    continue;
                }
                let size = path.metadata().map(|m| m.len()).unwrap_or(0);
                // Archive coverage mirrors the live conventions:
                // UPDATE files cover the next 15 minutes, RIB/bview
                // files cover the full day. The pipeline uses ts_start
                // for selection and ts_end only for gap validation;
                // setting ts_end = ts_start would fabricate a gap
                // between every consecutive 15-minute UPDATE archive.
                let ts_end = if data_type == "updates" {
                    ts + chrono::Duration::minutes(15)
                } else {
                    ts + chrono::Duration::hours(24)
                };
                items.push(crate::discover::ArchiveItem {
                    project: "local".to_string(),
                    collector_id: collector.to_string(),
                    data_type: data_type.to_string(),
                    ts_start: ts,
                    ts_end,
                    url: format!("file://{}", path.display()),
                    size,
                });
            }
        }
        Ok(items)
    }
}

/// Database-backed progress sink. Stage events drive legal job state
/// transitions; counts update the bounded progress columns. Updates are
/// throttled: stage boundaries always emit, count updates at most every
/// `throttle` seconds.
pub struct DbSink {
    conn: Arc<Mutex<Connection>>,
    job_id: String,
    state: Mutex<JobState>,
    last_emit: Mutex<Instant>,
    throttle: Duration,
}

impl DbSink {
    pub fn new(conn: Arc<Mutex<Connection>>, job_id: String, throttle: Duration) -> Self {
        DbSink {
            conn,
            job_id,
            state: Mutex::new(JobState::Claimed),
            last_emit: Mutex::new(Instant::now() - throttle),
            throttle,
        }
    }

    /// Advance the DB job state along the linear stage chain. Only
    /// forward steps are legal; a stage regression (e.g. UPDATE
    /// discovery re-entering DiscoveringArchives) updates the stage
    /// column but never moves the state backwards.
    fn transition_to(&self, to: JobState) {
        let mut state = self.state.lock().unwrap();
        if *state == to {
            return;
        }
        if crate::catalog::jobs::legal_transition(*state, to) {
            let conn = self.conn.lock().unwrap();
            if service::transition(&conn, &self.job_id, *state, to, None, "").is_ok() {
                *state = to;
            }
        }
    }
}

impl ProgressSink for DbSink {
    fn emit(&self, ev: &ProgressEvent) {
        let desired = JobState::parse_state(ev.stage).ok();
        let mut omitted_stage: Option<JobState> = None;
        if let Some(desired) = desired {
            self.transition_to(desired);
            let after = *self.state.lock().unwrap();
            // A stage emitted behind the current execution state is a
            // non-applicable skip (the pipeline re-emitted a stage the
            // job already passed): the event log states why instead of
            // pretending it ran.
            if after != desired {
                omitted_stage = Some(desired);
            }
        }
        let state = *self.state.lock().unwrap();
        if let Some(skipped) = omitted_stage {
            let conn = self.conn.lock().unwrap();
            let _ = service::append_event(
                &conn,
                &crate::catalog::jobs::JobEvent {
                    job_id: self.job_id.clone(),
                    sequence: 0,
                    occurred_at: String::new(),
                    state,
                    stage: Some(skipped.as_str().to_string()),
                    message_code: Some("stage_omitted".to_string()),
                    human_message: format!(
                        "stage {} omitted: not applicable after {}",
                        skipped.as_str(),
                        state.as_str()
                    ),
                    progress_current: None,
                    progress_total: None,
                    progress_unit: None,
                    structured_detail: None,
                },
            );
        }
        let now = Instant::now();
        let mut last = self.last_emit.lock().unwrap();
        if now.duration_since(*last) < self.throttle && ev.current.is_none() {
            return;
        }
        *last = now;
        let conn = self.conn.lock().unwrap();
        let _ = service::update_progress(
            &conn,
            &self.job_id,
            ev.stage,
            ev.current.map(|c| c as i64),
            ev.total.map(|t| t as i64),
            ev.unit,
        );
        let _ = service::append_event(
            &conn,
            &crate::catalog::jobs::JobEvent {
                job_id: self.job_id.clone(),
                sequence: 0,
                occurred_at: String::new(),
                state,
                stage: Some(ev.stage.to_string()),
                message_code: Some(ev.stage.to_string()),
                human_message: ev.message.clone(),
                progress_current: ev.current.map(|c| c as i64),
                progress_total: ev.total.map(|t| t as i64),
                progress_unit: ev.unit.map(|u| u.to_string()),
                structured_detail: None,
            },
        );
    }
}

struct HeartbeatThread {
    stop: Arc<AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl Drop for HeartbeatThread {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

/// Run the worker loop. Returns a process exit code.
pub fn run_worker(config: &WorkerConfig) -> i32 {
    if let Err(e) = validate_topology(config) {
        eprintln!("worker: {e}");
        return 2;
    }
    if config.show_execution_plan {
        println!(
            "{}",
            serde_json::to_string_pretty(&execution_plan(config)).unwrap_or_default()
        );
        return 0;
    }
    let conn = match crate::catalog::db::open_catalog(&config.db_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("worker: cannot open catalog database: {e}");
            return 1;
        }
    };
    let worker_id = match &config.worker_id {
        Some(id) => id.clone(),
        None => match generate_worker_id(&conn) {
            Ok(id) => id,
            Err(e) => {
                eprintln!("worker: {e}");
                return 1;
            }
        },
    };
    let conn = Arc::new(Mutex::new(conn));
    register_heartbeat(&conn, &worker_id, config);
    eprintln!(
        "worker {worker_id}: online (max_jobs={}, download_jobs={}, parse_jobs={}, offline={})",
        config.max_jobs, config.download_jobs, config.parse_jobs, config.offline
    );

    let mut processed = 0usize;
    let mut last_success = true;
    loop {
        // Conservative stale-lease policy: expired leases become Failed
        // with worker_lease_expired; staging is preserved; retry is
        // explicit. Never auto-resume.
        {
            let conn = conn.lock().unwrap();
            if let Err(e) = service::mark_stale_leases(&conn, &now_utc()) {
                eprintln!("worker: stale-lease scan failed: {e}");
            }
        }
        let claimed = {
            let mut guard = conn.lock().unwrap();
            service::claim_next(&mut guard, &worker_id, config.lease_secs)
        };
        match claimed {
            Ok(Some(claim)) => {
                last_success = execute_one(&conn, config, &worker_id, claim);
                processed += 1;
                if config.once {
                    break;
                }
            }
            Ok(None) => {
                if config.once {
                    break;
                }
                std::thread::sleep(config.poll_interval);
            }
            Err(e) => {
                eprintln!("worker: claim failed: {e}");
                if config.once {
                    return 1;
                }
                std::thread::sleep(config.poll_interval);
            }
        }
    }
    eprintln!("worker {worker_id}: shutting down after {processed} job(s)");
    if processed > 0 && !last_success {
        return 1;
    }
    0
}

fn now_utc() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

fn register_heartbeat(conn: &Arc<Mutex<Connection>>, worker_id: &str, config: &WorkerConfig) {
    let hb = WorkerHeartbeat {
        worker_id: worker_id.to_string(),
        started_at: now_utc(),
        last_heartbeat: now_utc(),
        process_version: env!("CARGO_PKG_VERSION").to_string(),
        source_families: vec!["RouteViews".to_string(), "RipeRis".to_string()],
        download_jobs: config.download_jobs as i64,
        parse_jobs: config.parse_jobs as i64,
        offline_mode: config.offline,
    };
    let conn = conn.lock().unwrap();
    if let Err(e) = service::heartbeat(&conn, &hb) {
        eprintln!("worker: cannot register heartbeat: {e}");
    }
}

/// Execute one claimed job end to end (execute -> stage -> publish).
/// Returns true when the job reached a terminal success state.
fn execute_one(
    conn: &Arc<Mutex<Connection>>,
    config: &WorkerConfig,
    worker_id: &str,
    claim: JobClaim,
) -> bool {
    let job = &claim.job;
    let job_id = job.id.clone();
    if !valid_job_id(&job_id) {
        eprintln!("worker: refusing malformed job id {job_id}");
        return false;
    }
    eprintln!(
        "worker: claimed job {job_id} (plan revision {})",
        job.plan_revision_id
    );

    // Materialize the immutable plan inputs from the catalog.
    let materialized = match materialize_inputs(conn, job) {
        Ok(m) => m,
        Err(e) => {
            let _ = fail_job(conn, &job_id, error_code::INVALID_PLAN, &e);
            return false;
        }
    };
    let _materialized_dir = materialized.0; // tempdir lifetime
    let (event_path, manifest_path, plan_hash) = materialized.1;

    // Staging root is catalog-root-relative and never absolute.
    let staging_rel = format!("data/jobs/{job_id}/staging");
    let staging_abs = config.root.join(&staging_rel);
    let event_id = match event_id_from_job(conn, job.plan_revision_id) {
        Ok(e) => e,
        Err(e) => {
            let _ = fail_job(conn, &job_id, error_code::INVALID_PLAN, &e);
            return false;
        }
    };
    let event_out = staging_abs.join(&event_id);
    if let Err(e) = std::fs::create_dir_all(&event_out) {
        let _ = fail_job(
            conn,
            &job_id,
            error_code::ARTIFACT_PUBLICATION_FAILED,
            &e.to_string(),
        );
        return false;
    }
    {
        let conn = conn.lock().unwrap();
        let _ = conn.execute(
            "UPDATE analysis_jobs SET staged_artifact_root = ?1 WHERE id = ?2",
            rusqlite::params![staging_rel, job_id],
        );
    }

    let cancel = Arc::new(AtomicBool::new(false));
    let sink = DbSink::new(conn.clone(), job_id.clone(), Duration::from_secs(2));
    let heartbeat = spawn_heartbeat(conn, config, worker_id, &job_id, &cancel);

    let exec_config = ExecutionConfig {
        cache_dir: config.root.join("cache"),
        jobs: 1,
        parse_jobs: config.parse_jobs,
        download_jobs: config.download_jobs,
        no_derived_cache: false,
        rebuild_derived_cache: false,
        rebuild_update_caches: false,
        offline: config.offline,
    };
    let discovery: Box<dyn crate::discover::ArchiveDiscovery> = if config.offline {
        Box::new(CacheScanDiscovery::new(exec_config.cache_dir.clone()))
    } else {
        Box::new(crate::discover::LiveArchiveDiscovery)
    };
    let t_start = Instant::now();
    let started_at = now_utc();

    let result = execution::execute_analysis(
        &event_path,
        &manifest_path,
        &exec_config,
        discovery.as_ref(),
        &event_out,
        &cancel,
        &sink,
    );

    let finished_at = now_utc();
    let wall_secs = t_start.elapsed().as_secs_f64();
    drop(heartbeat);

    match result {
        Ok(staged) => {
            if cancel.load(Ordering::Relaxed) {
                let _ = cancel_job(conn, &job_id);
                cleanup_staging(&staging_abs, false);
                return false;
            }
            publish(
                conn,
                config,
                worker_id,
                job,
                &staged.outcome,
                &event_out,
                &plan_hash,
                &started_at,
                &finished_at,
                wall_secs,
            )
        }
        Err(ExecutionError::Cancelled) => {
            let _ = cancel_job(conn, &job_id);
            cleanup_staging(&staging_abs, false);
            false
        }
        Err(ExecutionError::Failed { code, summary }) => {
            let _ = fail_job(conn, &job_id, code, &summary);
            cleanup_staging(&staging_abs, config.keep_failed_workdir);
            false
        }
    }
}

fn event_id_from_job(
    conn: &Arc<Mutex<Connection>>,
    plan_revision_id: i64,
) -> Result<String, String> {
    let conn = conn.lock().unwrap();
    conn.query_row(
        "SELECT e.external_id FROM catalog_events e
         JOIN manifest_revisions m ON m.event_id = e.id
         JOIN analysis_plans p ON p.manifest_revision_id = m.id
         WHERE p.id = ?1",
        rusqlite::params![plan_revision_id],
        |r| r.get(0),
    )
    .map_err(|e| format!("cannot resolve event for plan {plan_revision_id}: {e}"))
}

/// Materialize the event snapshot + manifest payload into temporary
/// files, and compute the canonical plan hash. Returns (tempdir, paths).
fn materialize_inputs(
    conn: &Arc<Mutex<Connection>>,
    job: &crate::catalog::jobs::AnalysisJob,
) -> Result<(TempDirGuard, (PathBuf, PathBuf, String)), String> {
    let dir = unique_temp_dir()?;
    let manifest_payload = {
        let conn = conn.lock().unwrap();
        manifest_payload_for_plan(&conn, job.plan_revision_id)?
    };
    let event_id = event_id_from_job(conn, job.plan_revision_id)?;
    let snapshot_payload: Option<String> = {
        let conn = conn.lock().unwrap();
        conn.query_row(
            "SELECT s.raw_payload FROM event_snapshots s
             JOIN manifest_revisions m ON m.snapshot_id = s.id
             JOIN analysis_plans p ON p.manifest_revision_id = m.id
             WHERE p.id = ?1",
            rusqlite::params![job.plan_revision_id],
            |r| r.get(0),
        )
        .ok()
    };
    let snapshot_payload =
        snapshot_payload.ok_or_else(|| "invalid_plan: event snapshot missing".to_string())?;
    // The pipeline consumes an event ticket input. Ticket-fixture-
    // shaped snapshots pass through unchanged; any other catalog event
    // (e.g. GRNOC viewer records) is translated generically from the
    // snapshot's normalized model — never entity-specific.
    let event_file_content = generic_ticket_input(&snapshot_payload, conn, job.plan_revision_id)?;
    let plan_hash = canonical_plan_hash(&manifest_payload)?;
    let event_path = dir.path().join(format!("{event_id}.json"));
    let manifest_path = dir.path().join(format!("{event_id}.manifest.json"));
    std::fs::write(&event_path, event_file_content)
        .map_err(|e| format!("cannot write event temp file: {e}"))?;
    std::fs::write(&manifest_path, manifest_payload)
        .map_err(|e| format!("cannot write manifest temp file: {e}"))?;
    Ok((dir, (event_path, manifest_path, plan_hash)))
}

fn spawn_heartbeat(
    conn: &Arc<Mutex<Connection>>,
    config: &WorkerConfig,
    worker_id: &str,
    job_id: &str,
    cancel: &Arc<AtomicBool>,
) -> HeartbeatThread {
    let stop = Arc::new(AtomicBool::new(false));
    let stop_thread = stop.clone();
    let conn = conn.clone();
    let cancel = cancel.clone();
    let job_id = job_id.to_string();
    let worker_id = worker_id.to_string();
    let lease_secs = config.lease_secs;
    let heartbeat_secs = config.heartbeat_secs;
    let download_jobs = config.download_jobs;
    let parse_jobs = config.parse_jobs;
    let offline = config.offline;
    let started_at = now_utc();
    let handle = std::thread::spawn(move || {
        while !stop_thread.load(Ordering::Relaxed) {
            let conn = conn.lock().unwrap();
            // Renew our lease while active.
            if service::renew_lease(&conn, &job_id, &worker_id, lease_secs).is_err() {
                // Lease lost (stolen or expired): the stale scan will
                // fail the job; stop running.
                cancel.store(true, Ordering::Relaxed);
                drop(conn);
                break;
            }
            // Observe cancellation requests from the web/CLI.
            if let Ok(job) = service::get(&conn, &job_id) {
                if job.state == JobState::CancelRequested {
                    cancel.store(true, Ordering::Relaxed);
                }
            }
            let hb = WorkerHeartbeat {
                worker_id: worker_id.clone(),
                started_at: started_at.clone(),
                last_heartbeat: now_utc(),
                process_version: env!("CARGO_PKG_VERSION").to_string(),
                source_families: vec!["RouteViews".to_string(), "RipeRis".to_string()],
                download_jobs: download_jobs as i64,
                parse_jobs: parse_jobs as i64,
                offline_mode: offline,
            };
            let _ = service::heartbeat(&conn, &hb);
            drop(conn);
            std::thread::sleep(Duration::from_secs(heartbeat_secs.max(1) as u64));
        }
    });
    HeartbeatThread {
        stop,
        handle: Some(handle),
    }
}

#[allow(clippy::too_many_arguments)] // one publish operation; inputs are distinct concerns
fn publish(
    conn: &Arc<Mutex<Connection>>,
    config: &WorkerConfig,
    worker_id: &str,
    job: &crate::catalog::jobs::AnalysisJob,
    outcome: &crate::outcome::AnalysisOutcome,
    event_out: &Path,
    plan_hash: &str,
    started_at: &str,
    finished_at: &str,
    wall_secs: f64,
) -> bool {
    let job_id = job.id.clone();
    let event_id = match event_id_from_job(conn, job.plan_revision_id) {
        Ok(e) => e,
        Err(e) => {
            let _ = fail_job(conn, &job_id, error_code::INTERNAL, &e);
            return false;
        }
    };
    let stage_durations = read_stage_durations(event_out);
    let meta = ExecutionMetadata {
        metadata_schema_version: EXECUTION_METADATA_SCHEMA_VERSION,
        plan_hash: plan_hash.to_string(),
        job_id: job_id.clone(),
        attempt: job.attempt,
        original_job_id: job.original_job_id.clone(),
        worker_id: worker_id.to_string(),
        requested_by: job.requested_by.clone(),
        requested_at: job.requested_at.clone(),
        started_at: started_at.to_string(),
        finished_at: finished_at.to_string(),
        wall_secs,
        stage_durations_secs: stage_durations,
        offline: config.offline,
        cache_hits: None,
        bytes_downloaded: None,
        bytes_read_local: None,
    };
    if let Err(e) = write_execution_metadata(event_out, &meta) {
        let _ = fail_job(conn, &job_id, error_code::ARTIFACT_VALIDATION_FAILED, &e);
        return false;
    }

    // Deterministic cancellation race policy: once the job is in
    // PublishingRun the publication wins; cancellation observed before
    // that point cancels before import. The DB state transition enforces
    // exactly one of the two outcomes.
    {
        let guard = conn.lock().unwrap();
        if let Err(e) = enter_publishing(&guard, &job_id) {
            drop(guard);
            // Cancellation was accepted before publication began.
            if e.contains("CancelRequested") || e.contains("not Claimed") {
                let _ = cancel_job(conn, &job_id);
            } else {
                let _ = fail_job(conn, &job_id, error_code::ARTIFACT_PUBLICATION_FAILED, &e);
            }
            return false;
        }
    }

    if let Err(e) = validate_staged(event_out, plan_hash) {
        let _ = fail_job(conn, &job_id, error_code::ARTIFACT_VALIDATION_FAILED, &e);
        cleanup_staging(
            event_out.parent().unwrap_or(&config.root),
            config.keep_failed_workdir,
        );
        return false;
    }

    let conn_guard = conn.lock().unwrap();
    let inputs = PublishInputs {
        catalog_root: &config.root,
        job_id: &job_id,
        plan_revision_id: job.plan_revision_id,
        event_id: &event_id,
        software_version: env!("CARGO_PKG_VERSION"),
        git_revision: None,
        run_started_at: started_at,
        run_completed_at: finished_at,
        runtime_secs: wall_secs,
    };
    match publish_staged_run(&conn_guard, event_out, &inputs) {
        Ok(run_id) => {
            match service::complete(&conn_guard, &job_id, run_id) {
                Ok(()) => {
                    eprintln!("worker: job {job_id} completed (run {run_id})");
                    if let crate::outcome::AnalysisOutcome::Incomplete { .. } = outcome {
                        eprintln!("worker: warning: job completed but run outcome is incomplete");
                    }
                    // The event directory was renamed into the final
                    // run location; remove the now-empty staging
                    // parent after verified publication.
                    cleanup_staging(event_out.parent().unwrap_or(&config.root), false);
                    true
                }
                Err(e) => {
                    // The run is published and immutable; the job
                    // linkage failed. Report precisely; do not re-import.
                    eprintln!("worker: run {run_id} published but job completion failed: {e}");
                    false
                }
            }
        }
        Err(e) => {
            let _ = fail_job_db(&conn_guard, &job_id, error_code::CATALOG_IMPORT_FAILED, &e);
            eprintln!("worker: job {job_id} failed during publication: {e}");
            false
        }
    }
}

fn cancel_job(conn: &Arc<Mutex<Connection>>, job_id: &str) -> Result<(), String> {
    let conn = conn.lock().unwrap();
    let job = service::get(&conn, job_id)?;
    match job.state {
        JobState::CancelRequested => service::observe_cancel(&conn, job_id),
        JobState::Queued => {
            let _ = service::request_cancel(&conn, job_id);
            Ok(())
        }
        _ => {
            let _ = service::request_cancel(&conn, job_id);
            Ok(())
        }
    }
}

fn cleanup_staging(staging_abs: &Path, keep: bool) {
    if keep {
        return;
    }
    if let Err(e) = std::fs::remove_dir_all(staging_abs) {
        if e.kind() != std::io::ErrorKind::NotFound {
            eprintln!(
                "worker: cannot clean staging {}: {e}",
                staging_abs.display()
            );
        }
    }
}

/// Read stage durations from the staged performance.json when present.
fn read_stage_durations(event_out: &Path) -> Vec<(String, f64)> {
    let path = event_out.join("performance.json");
    let Ok(content) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&content) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    if let Some(stages) = v.get("stages").and_then(|s| s.as_array()) {
        for s in stages {
            let name = s
                .get("stage")
                .and_then(|n| n.as_str())
                .unwrap_or("")
                .to_string();
            let secs = s.get("wall_secs").and_then(|t| t.as_f64()).unwrap_or(0.0);
            if !name.is_empty() {
                out.push((name, secs));
            }
        }
    }
    out
}

/// Translate a snapshot's payload into the pipeline's ticket input.
///
/// The raw payload is used verbatim when it already parses as a ticket
/// fixture (legacy fixture-shaped snapshots keep their exact
/// semantics, including timezone handling). Otherwise a generic ticket
/// input is derived from the snapshot's NORMALIZED model (id, title,
/// RFC3339 start/end, task type, description) — this covers any source
/// kind, including GRNOC Public Task Viewer records, without any
/// source-specific branch in the pipeline.
fn generic_ticket_input(
    raw_payload: &str,
    conn: &Arc<Mutex<Connection>>,
    plan_revision_id: i64,
) -> Result<String, String> {
    if crate::sources::internet2::ticket::parse_ticket_fixture(raw_payload).is_ok() {
        return Ok(raw_payload.to_string());
    }
    let normalized: String = {
        let conn = conn.lock().unwrap();
        conn.query_row(
            "SELECT s.normalized_json FROM event_snapshots s
             JOIN manifest_revisions m ON m.snapshot_id = s.id
             JOIN analysis_plans p ON p.manifest_revision_id = m.id
             WHERE p.id = ?1",
            rusqlite::params![plan_revision_id],
            |r| r.get(0),
        )
        .map_err(|e| format!("cannot load normalized event: {e}"))?
    };
    let v: serde_json::Value = serde_json::from_str(&normalized)
        .map_err(|e| format!("normalized event unreadable: {e}"))?;
    let id = v
        .get("id")
        .and_then(|x| x.as_str())
        .ok_or_else(|| "invalid_plan: normalized event missing id".to_string())?;
    let title = v.get("title").and_then(|x| x.as_str()).unwrap_or(id);
    let start = v
        .get("start")
        .and_then(|x| x.as_str())
        .ok_or_else(|| "invalid_plan: normalized event missing start".to_string())?;
    let end = v.get("end").and_then(|x| x.as_str()).unwrap_or("");
    let task_type = v
        .get("task_type")
        .and_then(|x| x.as_str())
        .unwrap_or("incident");
    let description = v.get("description").and_then(|x| x.as_str()).unwrap_or("");
    let fmt = |ts: &str| -> String {
        chrono::DateTime::parse_from_rfc3339(ts)
            .map(|t| t.format("%Y-%m-%d %H:%M:%S").to_string())
            .unwrap_or_else(|_| ts.to_string())
    };
    // The ticket parser treats an absent timezone as UTC (backward
    // compatible), so no timezone field is emitted.
    serde_json::to_string(&serde_json::json!({
        "id": id,
        "title": title,
        "start": fmt(start),
        "end": fmt(end),
        "type": task_type,
        "description": description,
    }))
    .map_err(|e| format!("cannot build generic ticket input: {e}"))
}

/// Minimal unique temp directory (tempfile is a dev-dependency only).
/// Removes itself on drop.
struct TempDirGuard(PathBuf);

impl TempDirGuard {
    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        // Remove the unique directory AND its per-process base so the
        // next worker run can create a fresh, unforgeable base.
        let _ = std::fs::remove_dir_all(&self.0);
        if let Some(base) = self.0.parent() {
            let _ = std::fs::remove_dir(base);
        }
    }
}

fn unique_temp_dir() -> Result<TempDirGuard, String> {
    // The base is created with create_dir (fails if a local attacker
    // pre-created it) and restricted to the owner; the unique
    // subdirectory then cannot be predicted or planted. The name
    // includes the thread id so parallel workers (e.g. tests) never
    // collide on one base.
    let thread_id = format!("{:?}", std::thread::current().id())
        .replace(|c: char| !c.is_ascii_alphanumeric(), "");
    let base = std::env::temp_dir().join(format!("inim-worker-{}-{thread_id}", std::process::id()));
    if base.exists() {
        return Err(format!(
            "temp dir {} already exists; refusing to reuse a possibly planted directory",
            base.display()
        ));
    }
    std::fs::create_dir(&base).map_err(|e| format!("cannot create temp dir: {e}"))?;
    let _ = std::fs::set_permissions(&base, std::fs::Permissions::from_mode(0o700));
    let unique = base.join(format!(
        "{}",
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
    ));
    std::fs::create_dir(&unique).map_err(|e| format!("cannot create temp dir: {e}"))?;
    Ok(TempDirGuard(unique))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> WorkerConfig {
        WorkerConfig::default()
    }

    #[test]
    fn unsafe_worker_topology_is_rejected() {
        let mut c = cfg();
        c.max_jobs = 4;
        c.parse_jobs = 8;
        // 4 x 8 = 32 parsers exceeds any reasonable host budget and the
        // documented safety bound.
        assert!(validate_topology(&c).is_err());
        let mut c = cfg();
        c.max_jobs = 2;
        c.parse_jobs = 9;
        assert!(validate_topology(&c).is_err());
        let mut c = cfg();
        c.parse_jobs = 0;
        assert!(validate_topology(&c).is_err());
    }

    #[test]
    fn default_topology_is_bounded() {
        let c = cfg();
        assert_eq!(c.max_jobs, 1);
        assert_eq!(c.download_jobs, 2);
        assert_eq!(c.parse_jobs, 8);
        assert!(validate_topology(&c).is_ok());
        // A single job keeps the per-job parse default on any host.
        assert!(c.parse_jobs <= 8);
        assert!(c.max_jobs == 1);
    }

    #[test]
    fn max_jobs_one_preserves_existing_parse_default() {
        // With max_jobs = 1 the parse budget stays at the established
        // practical default (8), matching direct execution.
        let c = cfg();
        assert_eq!(c.parse_jobs, 8);
        let plan = execution_plan(&c);
        assert_eq!(plan["max_concurrent_jobs"], 1);
        assert_eq!(plan["parse_workers"], 8);
    }

    #[test]
    fn execution_plan_matches_effective_configuration() {
        let mut c = cfg();
        c.download_jobs = 3;
        c.parse_jobs = 6;
        c.offline = true;
        let plan = execution_plan(&c);
        assert_eq!(plan["download_workers"], 3);
        assert_eq!(plan["parse_workers"], 6);
        assert_eq!(plan["expected_max_parser_threads"], 6);
        assert_eq!(plan["offline"], true);
    }

    #[test]
    fn cache_scan_discovery_finds_only_cached_material() {
        use crate::discover::ArchiveDiscovery;
        let dir = tempfile::tempdir().unwrap();
        let cache = dir.path().join("cache");
        let collector = cache.join("route-views2").join("updates");
        std::fs::create_dir_all(&collector).unwrap();
        std::fs::write(collector.join("updates.20190821.1600.bz2"), "x").unwrap();
        std::fs::write(collector.join("updates.20190821.1800.bz2"), "y").unwrap();
        std::fs::write(collector.join("updates.20190820.0000.bz2"), "z").unwrap();
        std::fs::write(collector.join("not-an-archive.txt"), "n").unwrap();
        let d = CacheScanDiscovery::new(cache);
        let start = chrono::DateTime::parse_from_rfc3339("2019-08-21T15:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        let end = chrono::DateTime::parse_from_rfc3339("2019-08-21T17:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        let items = d
            .query("route-views", &["route-views2"], start, end, "updates")
            .unwrap();
        let names: Vec<String> = items
            .iter()
            .map(|i| i.url.rsplit('/').next().unwrap().to_string())
            .collect();
        assert_eq!(names, vec!["updates.20190821.1600.bz2"]);
        // Unsupported collector with no cache dir yields nothing.
        let items = d
            .query("route-views", &["route-views8"], start, end, "updates")
            .unwrap();
        assert!(items.is_empty());
    }

    #[test]
    fn offline_scan_produces_file_urls_consumable_by_cache() {
        use crate::discover::ArchiveDiscovery;
        let dir = tempfile::tempdir().unwrap();
        let cache = dir.path().join("cache");
        let rib_dir = cache.join("route-views2").join("rib");
        std::fs::create_dir_all(&rib_dir).unwrap();
        std::fs::write(rib_dir.join("rib.20190821.0000.bz2"), "r").unwrap();
        let d = CacheScanDiscovery::new(cache);
        let start = chrono::DateTime::parse_from_rfc3339("2019-08-21T00:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        let end = chrono::DateTime::parse_from_rfc3339("2019-08-21T01:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        let items = d
            .query("route-views", &["route-views2"], start, end, "rib")
            .unwrap();
        assert_eq!(items.len(), 1);
        // The URL is file:// so cache_archive would treat it as cached
        // content only if the file+sidecar are present; in offline mode
        // a missing sidecar is a NotCached error, never a download.
        let err = crate::discover::cache_archive(&items[0], dir.path(), true).unwrap_err();
        assert!(matches!(
            err,
            crate::discover::InimArchiveError::NotCached { .. }
        ));
    }

    #[test]
    fn worker_ids_are_stable_process_lifetime_values() {
        let dir = tempfile::tempdir().unwrap();
        let conn = crate::catalog::db::open_catalog(&dir.path().join("c.sqlite")).unwrap();
        let a = generate_worker_id(&conn).unwrap();
        let b = generate_worker_id(&conn).unwrap();
        assert!(a.starts_with("w-"));
        assert!(a.len() >= 18);
        assert_ne!(a, b);
        // Not hostname-derived: never contains a machine name.
        let host = std::env::var("HOSTNAME").unwrap_or_default();
        assert!(!a.contains(&host) || host.is_empty());
    }
}

#[cfg(test)]
mod sink_tests {
    use super::*;
    use crate::catalog::jobs::service as jobs;

    fn sink_job() -> (tempfile::TempDir, String, DbSink) {
        let dir = tempfile::tempdir().unwrap();
        let conn = crate::catalog::db::open_catalog(&dir.path().join("c.sqlite")).unwrap();
        conn.execute(
            "INSERT INTO catalog_events (source_kind, external_id, first_seen, last_seen)
             VALUES ('local-repository', 'SINK-EVT', '2026-08-01T00:00:00Z', '2026-08-01T00:00:00Z')",
            [],
        )
        .unwrap();
        let eid = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO event_snapshots (event_id, fetched_at, source_url, content_sha256, raw_payload, normalized_json, parser_version)
             VALUES (?1, '2026-08-01T00:00:00Z', 'file:///x', 's', '{}', '{}', 't')",
            [eid],
        )
        .unwrap();
        let sid = conn.last_insert_rowid();
        let payload = serde_json::json!({
            "event_id": "SINK-EVT", "revision": 1, "schema_version": 2, "open": false,
            "event_window_utc": {"start": "2026-08-01T00:00:00Z", "end": "2026-08-02T00:00:00Z"},
            "ticket_window_local": {"start": "", "end": "", "timezone": "UTC"},
            "target": {"label": "Sink event", "origin_asns": [64500],
                "transit_predicate": {"predicate": {"ContainsAny": [64501]}, "status": "Reviewed",
                    "provenance": {"statement": "r", "reviewed_by": "local-review", "date": "2026-08-01"}}},
            "collectors": ["route-views2"], "source_family": "RouteViews"
        })
        .to_string();
        conn.execute(
            "INSERT INTO manifest_revisions (event_id, snapshot_id, manifest_schema, payload, sha256, review_status)
             VALUES (?1, ?2, 2, ?3, ?4, 'Reviewed')",
            rusqlite::params![eid, sid, payload, "sink-msha"],
        )
        .unwrap();
        let mid = conn.last_insert_rowid();
        let manifest: crate::manifest::Manifest = serde_json::from_str(
            &conn
                .query_row(
                    "SELECT payload FROM manifest_revisions WHERE id = ?1",
                    [mid],
                    |r| r.get::<_, String>(0),
                )
                .unwrap(),
        )
        .unwrap();
        let plan = crate::catalog::import::build_plan_record(&conn, mid, &manifest, true).unwrap();
        let pid = crate::catalog::store::insert_plan(&conn, &plan).unwrap();
        let id =
            match jobs::queue(&conn, pid, crate::catalog::jobs::RequestSource::Cli, "h").unwrap() {
                jobs::QueueOutcome::Created(id) => id,
                _ => unreachable!(),
            };
        drop(conn);
        let conn = crate::catalog::db::open_catalog(&dir.path().join("c.sqlite")).unwrap();
        let conn = Arc::new(Mutex::new(conn));
        let sink = DbSink::new(conn.clone(), id.clone(), Duration::from_secs(0));
        (dir, id, sink)
    }

    #[test]
    fn skipped_stage_is_not_logged_as_completed() {
        let (dir, job_id, sink) = sink_job();
        // Claim first, then emit stages with the pipeline's re-entrant
        // DiscoveringArchives and skipped FreezingCohort.
        {
            let conn = sink.conn.lock().unwrap();
            let mut c = conn;
            jobs::claim_next(&mut c, "w", 90).unwrap();
        }
        for (stage, msg) in [
            ("DiscoveringArchives", "discovering"),
            ("AcquiringArchives", "acquiring"),
            ("DiscoveringArchives", "updates discovery"), // re-entrant: behind
            ("ParsingUpdates", "parsing"),
            ("ReconstructingRoutes", "reconstructing"),
        ] {
            sink.emit(&ProgressEvent {
                stage,
                message: msg.to_string(),
                current: None,
                total: None,
                unit: None,
            });
        }
        let conn = sink.conn.lock().unwrap();
        let job = jobs::get(&conn, &job_id).unwrap();
        assert_eq!(job.state, JobState::ReconstructingRoutes);
        // The re-entrant DiscoveringArchives was NOT recorded as a
        // completed stage: no event carries it as the job state, and
        // the omitted stage is explained.
        let evs = jobs::events(&conn, &job_id, 100).unwrap();
        // The re-entrant "updates discovery" emit was recorded under
        // the ACTUAL state (AcquiringArchives), never as a completed
        // DiscoveringArchives stage.
        let reentrant = evs
            .iter()
            .find(|e| e.human_message == "updates discovery")
            .expect("re-entrant emit event present");
        // The re-entrant stage was recorded under the ACTUAL state,
        // not as a completed DiscoveringArchives stage.
        assert_eq!(reentrant.state, JobState::AcquiringArchives);
        assert_eq!(
            reentrant.stage.as_deref(),
            Some("DiscoveringArchives"),
            "the stage label is retained but the state reflects reality"
        );
        assert!(
            evs.iter()
                .any(|e| e.message_code.as_deref() == Some("stage_omitted")),
            "the skip must be explained in the event log: {:?}",
            evs.iter()
                .map(|e| e.human_message.clone())
                .collect::<Vec<_>>()
        );
        drop(dir);
    }
}
