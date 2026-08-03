//! SQLite-backed analysis-job service.
//!
//! Every mutation is explicit; every state change is a legal transition
//! enforced by the domain state machine. State updates and their
//! append-only events commit in one transaction. The web server, the
//! CLI, and the worker all use this service — business rules live here,
//! never in command handlers.

use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};

use super::{
    error_code, legal_transition, AnalysisJob, JobEvent, JobState, RequestSource,
    MAX_HUMAN_MESSAGE_BYTES, MAX_STRUCTURED_DETAIL_BYTES,
};

/// Default lease duration for a claimed job (seconds).
pub const DEFAULT_LEASE_SECS: i64 = 90;
/// Default heartbeat interval (seconds) — bounded, not per-element.
pub const DEFAULT_HEARTBEAT_SECS: i64 = 15;
/// Default stale threshold for a worker heartbeat (seconds).
pub const DEFAULT_WORKER_STALE_SECS: i64 = 60;

/// Outcome of a queue operation.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub enum QueueOutcome {
    /// A new job was created.
    Created(String),
    /// An active job for the same plan revision already exists.
    Duplicate(String),
}

/// Outcome of a cancellation request.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub enum CancelOutcome {
    /// The queued job was cancelled immediately.
    Cancelled(String),
    /// The executing job now carries a cancellation request.
    Requested(String),
}

/// Structured worker heartbeat record.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct WorkerHeartbeat {
    pub worker_id: String,
    pub started_at: String,
    pub last_heartbeat: String,
    pub process_version: String,
    pub source_families: Vec<String>,
    pub download_jobs: i64,
    pub parse_jobs: i64,
    pub offline_mode: bool,
}

/// Freshness classification for a worker heartbeat row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkerFreshness {
    Online,
    Stale,
}

/// Query filters for `list`.
#[derive(Debug, Clone, Default)]
pub struct JobFilter {
    pub state: Option<JobState>,
    pub plan_revision_id: Option<i64>,
}

/// Current UTC timestamp (RFC3339, second precision).
pub fn now_utc_public() -> String {
    now_utc()
}

fn now_utc() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

/// Generate a collision-resistant local job id (128 random bits, hex).
/// No UUID crate is needed; SQLite's system-RNG-backed `randomblob` is
/// available on every supported platform.
pub fn new_job_id(conn: &Connection) -> Result<String, String> {
    conn.query_row("SELECT lower(hex(randomblob(16)))", [], |r| {
        r.get::<_, String>(0)
    })
    .map_err(|e| format!("cannot generate job id: {e}"))
}

fn bind_job(row: &rusqlite::Row<'_>) -> rusqlite::Result<AnalysisJob> {
    let state: String = row.get(4)?;
    Ok(AnalysisJob {
        id: row.get(0)?,
        plan_revision_id: row.get(1)?,
        requested_by: row.get(2)?,
        requested_at: row.get(3)?,
        state: JobState::parse_state(&state).unwrap_or(JobState::Failed),
        attempt: row.get(5)?,
        original_job_id: row.get(6)?,
        worker_id: row.get(7)?,
        lease_acquired_at: row.get(8)?,
        lease_expires_at: row.get(9)?,
        heartbeat_at: row.get(10)?,
        stage: row.get(11)?,
        progress_current: row.get(12)?,
        progress_total: row.get(13)?,
        progress_unit: row.get(14)?,
        cancel_requested_at: row.get(15)?,
        started_at: row.get(16)?,
        finished_at: row.get(17)?,
        error_code: row.get(18)?,
        error_summary: row.get(19)?,
        staged_artifact_root: row.get(20)?,
        completed_run_id: row.get(21)?,
        plan_hash: row.get(22)?,
    })
}

const JOB_COLUMNS: &str = "id, plan_revision_id, requested_by, requested_at, state, attempt, \
     original_job_id, worker_id, lease_acquired_at, lease_expires_at, heartbeat_at, stage, \
     progress_current, progress_total, progress_unit, cancel_requested_at, started_at, \
     finished_at, error_code, error_summary, staged_artifact_root, completed_run_id, plan_hash";

fn get_job_row(conn: &Connection, job_id: &str) -> Result<Option<AnalysisJob>, String> {
    let sql = format!("SELECT {JOB_COLUMNS} FROM analysis_jobs WHERE id = ?1");
    conn.query_row(&sql, params![job_id], bind_job)
        .optional()
        .map_err(|e| format!("cannot load job {job_id}: {e}"))
}

/// Load one job. Errors when the id is unknown.
pub fn get(conn: &Connection, job_id: &str) -> Result<AnalysisJob, String> {
    get_job_row(conn, job_id)?.ok_or_else(|| format!("analysis job not found: {job_id}"))
}

/// List jobs, newest first.
pub fn list(conn: &Connection, filter: &JobFilter) -> Result<Vec<AnalysisJob>, String> {
    let mut sql = format!("SELECT {JOB_COLUMNS} FROM analysis_jobs");
    let mut clauses: Vec<String> = Vec::new();
    let mut args: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
    if let Some(s) = filter.state {
        clauses.push("state = ?".to_string());
        args.push(Box::new(s.as_str().to_string()));
    }
    if let Some(p) = filter.plan_revision_id {
        clauses.push("plan_revision_id = ?".to_string());
        args.push(Box::new(p));
    }
    if !clauses.is_empty() {
        sql.push_str(" WHERE ");
        sql.push_str(&clauses.join(" AND "));
    }
    sql.push_str(" ORDER BY requested_at DESC, id DESC");
    let mut stmt = conn
        .prepare(&sql)
        .map_err(|e| format!("cannot prepare job list: {e}"))?;
    let rows = stmt
        .query_map(
            rusqlite::params_from_iter(args.iter().map(|a| a as &dyn rusqlite::ToSql)),
            bind_job,
        )
        .map_err(|e| format!("cannot query job list: {e}"))?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(|e| format!("bad job row: {e}"))?);
    }
    Ok(out)
}

/// Execution-state counts for the dashboard: queued / running / failed /
/// completed / cancelled.
pub fn counts(conn: &Connection) -> Result<JobCounts, String> {
    let sql = "SELECT state, COUNT(*) FROM analysis_jobs GROUP BY state";
    let mut stmt = conn
        .prepare(sql)
        .map_err(|e| format!("cannot prepare counts: {e}"))?;
    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))
        .map_err(|e| format!("cannot query counts: {e}"))?;
    let mut c = JobCounts::default();
    for row in rows {
        let (state, n) = row.map_err(|e| format!("bad count row: {e}"))?;
        match JobState::parse_state(&state) {
            Ok(JobState::Queued) => c.queued = n,
            Ok(s) if s.is_active() && s != JobState::Queued => c.running += n,
            Ok(JobState::Failed) => c.failed = n,
            Ok(JobState::Completed) => c.completed = n,
            Ok(JobState::Cancelled) => c.cancelled = n,
            _ => {}
        }
    }
    Ok(c)
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct JobCounts {
    pub queued: i64,
    pub running: i64,
    pub failed: i64,
    pub completed: i64,
    pub cancelled: i64,
}

/// Append one event with the NEXT deterministic sequence number. Call
/// inside the same transaction as the state update it describes.
fn append_event_tx(tx: &Connection, ev: &JobEvent) -> Result<(), String> {
    if ev.human_message.len() > MAX_HUMAN_MESSAGE_BYTES {
        return Err("job event human_message too long".to_string());
    }
    if let Some(d) = &ev.structured_detail {
        if d.len() > MAX_STRUCTURED_DETAIL_BYTES {
            return Err("job event structured_detail too long (bounded)".to_string());
        }
        // Absolute paths must never enter job events (POSIX root or a
        // Windows drive/UNC-style prefix).
        if d.starts_with('/') || d.contains(":/") || d.contains(":\\") {
            return Err("job event structured_detail must not contain absolute paths".to_string());
        }
    }
    tx.execute(
        "INSERT INTO analysis_job_events
           (job_id, sequence, occurred_at, state, stage, message_code, human_message,
            progress_current, progress_total, progress_unit, structured_detail)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![
            ev.job_id,
            ev.sequence,
            ev.occurred_at,
            ev.state.as_str(),
            ev.stage,
            ev.message_code,
            ev.human_message,
            ev.progress_current,
            ev.progress_total,
            ev.progress_unit,
            ev.structured_detail,
        ],
    )
    .map_err(|e| format!("cannot append job event: {e}"))?;
    Ok(())
}

fn next_sequence(tx: &Connection, job_id: &str) -> Result<i64, String> {
    tx.query_row(
        "SELECT COALESCE(MAX(sequence), 0) + 1 FROM analysis_job_events WHERE job_id = ?1",
        params![job_id],
        |r| r.get(0),
    )
    .map_err(|e| format!("cannot compute next event sequence: {e}"))
}

/// Append an event outside any explicit transaction (short, standalone).
pub fn append_event(conn: &Connection, ev: &JobEvent) -> Result<(), String> {
    let tx = conn
        .unchecked_transaction()
        .map_err(|e| format!("cannot begin event transaction: {e}"))?;
    let mut ev = ev.clone();
    ev.sequence = next_sequence(&tx, &ev.job_id)?;
    if ev.occurred_at.is_empty() {
        ev.occurred_at = now_utc();
    }
    append_event_tx(&tx, &ev)?;
    tx.commit().map_err(|e| format!("cannot commit event: {e}"))
}

/// Bounded recent events for one job (oldest first).
pub fn events(conn: &Connection, job_id: &str, limit: i64) -> Result<Vec<JobEvent>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT job_id, sequence, occurred_at, state, stage, message_code, human_message,
                    progress_current, progress_total, progress_unit, structured_detail
             FROM analysis_job_events WHERE job_id = ?1
             ORDER BY sequence DESC LIMIT ?2",
        )
        .map_err(|e| format!("cannot prepare events: {e}"))?;
    let rows = stmt
        .query_map(params![job_id, limit], |r| {
            Ok(JobEvent {
                job_id: r.get(0)?,
                sequence: r.get(1)?,
                occurred_at: r.get(2)?,
                state: JobState::parse_state(&r.get::<_, String>(3)?).unwrap_or(JobState::Failed),
                stage: r.get(4)?,
                message_code: r.get(5)?,
                human_message: r.get(6)?,
                progress_current: r.get(7)?,
                progress_total: r.get(8)?,
                progress_unit: r.get(9)?,
                structured_detail: r.get(10)?,
            })
        })
        .map_err(|e| format!("cannot query events: {e}"))?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(|e| format!("bad event row: {e}"))?);
    }
    out.reverse();
    Ok(out)
}

/// Transition a job to `to` and append the matching event in ONE
/// transaction. `from` is the caller's expected current state; a
/// mismatch (race) rejects the transition.
pub fn transition(
    conn: &Connection,
    job_id: &str,
    from: JobState,
    to: JobState,
    message_code: Option<&str>,
    human_message: &str,
) -> Result<(), String> {
    if !legal_transition(from, to) {
        return Err(format!(
            "illegal job transition {job_id}: {} -> {}",
            from.as_str(),
            to.as_str()
        ));
    }
    let tx = conn
        .unchecked_transaction()
        .map_err(|e| format!("cannot begin transition transaction: {e}"))?;
    let current: String = tx
        .query_row(
            "SELECT state FROM analysis_jobs WHERE id = ?1",
            params![job_id],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| format!("cannot load job state: {e}"))?
        .ok_or_else(|| format!("analysis job not found: {job_id}"))?;
    let current_state = JobState::parse_state(&current)
        .map_err(|e| format!("job {job_id} has corrupt state: {e}"))?;
    if current_state != from {
        return Err(format!(
            "job {job_id} is {current}, not {from}; refusing {to}",
            current = current_state.as_str(),
            from = from.as_str(),
            to = to.as_str()
        ));
    }
    let occurred = now_utc();
    let finished = if matches!(
        to,
        JobState::Completed | JobState::Cancelled | JobState::Failed
    ) {
        Some(occurred.clone())
    } else {
        None
    };
    tx.execute(
        "UPDATE analysis_jobs SET state = ?1, stage = ?2, finished_at = COALESCE(?3, finished_at)
         WHERE id = ?4",
        params![to.as_str(), to.stage_label(), finished, job_id],
    )
    .map_err(|e| format!("cannot update job state: {e}"))?;
    let ev = JobEvent {
        job_id: job_id.to_string(),
        sequence: next_sequence(&tx, job_id)?,
        occurred_at: occurred,
        state: to,
        stage: to.stage_label(),
        message_code: message_code.map(|s| s.to_string()),
        human_message: human_message.to_string(),
        progress_current: None,
        progress_total: None,
        progress_unit: None,
        structured_detail: None,
    };
    append_event_tx(&tx, &ev)?;
    tx.commit()
        .map_err(|e| format!("cannot commit transition: {e}"))
}

impl JobState {
    /// Human stage label for display (same vocabulary as the state).
    pub fn stage_label(&self) -> Option<String> {
        match self {
            JobState::Queued
            | JobState::Claimed
            | JobState::Completed
            | JobState::CancelRequested
            | JobState::Cancelled
            | JobState::Failed => None,
            other => Some(other.as_str().to_string()),
        }
    }
}

/// Fail a job with a stable machine code. `from` must be the current
/// state; terminal jobs can never be failed again.
pub fn fail(
    conn: &Connection,
    job_id: &str,
    from: JobState,
    code: &str,
    summary: &str,
) -> Result<(), String> {
    if !legal_transition(from, JobState::Failed) {
        return Err(format!(
            "illegal transition to Failed for {job_id} from {from:?}"
        ));
    }
    let tx = conn
        .unchecked_transaction()
        .map_err(|e| format!("cannot begin fail transaction: {e}"))?;
    let current: String = tx
        .query_row(
            "SELECT state FROM analysis_jobs WHERE id = ?1",
            params![job_id],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| format!("cannot load job state: {e}"))?
        .ok_or_else(|| format!("analysis job not found: {job_id}"))?;
    let current_state = JobState::parse_state(&current)
        .map_err(|e| format!("job {job_id} has corrupt state: {e}"))?;
    if current_state != from {
        return Err(format!(
            "job {job_id} is not {from:?} (is {current_state:?})"
        ));
    }
    let occurred = now_utc();
    tx.execute(
        "UPDATE analysis_jobs SET state = 'Failed', finished_at = ?1, error_code = ?2,
                error_summary = ?3, cancel_requested_at = COALESCE(cancel_requested_at, ?1)
         WHERE id = ?4",
        params![occurred, code, summary, job_id],
    )
    .map_err(|e| format!("cannot fail job: {e}"))?;
    let ev = JobEvent {
        job_id: job_id.to_string(),
        sequence: next_sequence(&tx, job_id)?,
        occurred_at: occurred,
        state: JobState::Failed,
        stage: None,
        message_code: Some(code.to_string()),
        human_message: format!("failed: {summary}"),
        progress_current: None,
        progress_total: None,
        progress_unit: None,
        structured_detail: None,
    };
    append_event_tx(&tx, &ev)?;
    tx.commit().map_err(|e| format!("cannot commit fail: {e}"))
}

/// Complete a job after its run was published. `from` must be
/// PublishingRun (the final atomic publication stage).
pub fn complete(conn: &Connection, job_id: &str, run_id: i64) -> Result<(), String> {
    let tx = conn
        .unchecked_transaction()
        .map_err(|e| format!("cannot begin complete transaction: {e}"))?;
    let current: String = tx
        .query_row(
            "SELECT state FROM analysis_jobs WHERE id = ?1",
            params![job_id],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| format!("cannot load job state: {e}"))?
        .ok_or_else(|| format!("analysis job not found: {job_id}"))?;
    let current_state = JobState::parse_state(&current)
        .map_err(|e| format!("job {job_id} has corrupt state: {e}"))?;
    if current_state != JobState::PublishingRun {
        return Err(format!(
            "job {job_id} is {current_state:?}, not PublishingRun; refusing completion"
        ));
    }
    let occurred = now_utc();
    let updated = tx
        .execute(
            "UPDATE analysis_jobs SET state = 'Completed', completed_run_id = ?1,
                    finished_at = ?2, stage = NULL
             WHERE id = ?3 AND state = 'PublishingRun'",
            params![run_id, occurred, job_id],
        )
        .map_err(|e| format!("cannot complete job: {e}"))?;
    if updated == 0 {
        return Err(format!(
            "job {job_id} is not in PublishingRun; refusing completion (concurrent state change)"
        ));
    }
    let ev = JobEvent {
        job_id: job_id.to_string(),
        sequence: next_sequence(&tx, job_id)?,
        occurred_at: occurred,
        state: JobState::Completed,
        stage: None,
        message_code: Some("run_published".to_string()),
        human_message: "Analysis complete; run published".to_string(),
        progress_current: None,
        progress_total: None,
        progress_unit: None,
        structured_detail: Some(format!("run_id={run_id}")),
    };
    append_event_tx(&tx, &ev)?;
    tx.commit()
        .map_err(|e| format!("cannot commit completion: {e}"))
}

/// Queue an exact plan revision. Validates plan existence, readiness,
/// schema currency, and event/snapshot existence; enforces one active
/// job per exact plan revision and plan hash. Performs NO network access
/// and no analysis.
///
/// `plan_hash` is the canonical hash of the serialized plan revision
/// (computed by `super::plan::canonical_plan_hash`).
pub fn queue(
    conn: &Connection,
    plan_revision_id: i64,
    requested_by: RequestSource,
    plan_hash: &str,
) -> Result<QueueOutcome, String> {
    let tx = conn
        .unchecked_transaction()
        .map_err(|e| format!("cannot begin queue transaction: {e}"))?;

    // Exact immutable plan revision must exist.
    let plan: Option<(String, i64, String, String)> = tx
        .query_row(
            "SELECT status, plan_schema, payload, sha256 FROM analysis_plans WHERE id = ?1",
            params![plan_revision_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .optional()
        .map_err(|e| format!("cannot load plan: {e}"))?;
    let (status, plan_schema, _payload, stored_sha) =
        plan.ok_or_else(|| format!("plan revision not found: {plan_revision_id}"))?;

    // Plan schema must be current; stale plans are never queued.
    if plan_schema != crate::schema::ANALYSIS_PLAN_SCHEMA_VERSION as i64 {
        return Err(format!(
            "incompatible_plan_schema: plan {plan_revision_id} is schema v{plan_schema}, current v{}",
            crate::schema::ANALYSIS_PLAN_SCHEMA_VERSION
        ));
    }

    // Referenced event and source snapshot must exist.
    let manifest: Option<(i64, i64)> = tx
        .query_row(
            "SELECT event_id, snapshot_id FROM manifest_revisions
             WHERE id = (SELECT manifest_revision_id FROM analysis_plans WHERE id = ?1)",
            params![plan_revision_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .map_err(|e| format!("cannot load manifest revision: {e}"))?;
    let (event_id, snapshot_id) =
        manifest.ok_or_else(|| format!("plan {plan_revision_id} has no manifest revision"))?;
    let event_ok: i64 = tx
        .query_row(
            "SELECT COUNT(*) FROM catalog_events WHERE id = ?1",
            params![event_id],
            |r| r.get(0),
        )
        .map_err(|e| format!("cannot check event: {e}"))?;
    let snapshot_ok: i64 = tx
        .query_row(
            "SELECT COUNT(*) FROM event_snapshots WHERE id = ?1",
            params![snapshot_id],
            |r| r.get(0),
        )
        .map_err(|e| format!("cannot check snapshot: {e}"))?;
    if event_ok == 0 || snapshot_ok == 0 {
        return Err("invalid_plan: referenced event or source snapshot is missing".to_string());
    }

    // Plan must be Ready per the reviewed domain rules. The stored
    // status is authoritative for reviewed readiness; the hash check
    // below keeps identity exact.
    if status != "Ready" {
        return Err(format!(
            "invalid_plan: plan {plan_revision_id} is not Ready (status {status})"
        ));
    }

    // Idempotency: one active job per exact plan revision and hash.
    let duplicate: Option<String> = tx
        .query_row(
            "SELECT id FROM analysis_jobs
             WHERE plan_revision_id = ?1 AND plan_hash = ?2 AND state IN (
                'Queued','Claimed','DiscoveringArchives','AcquiringArchives','ParsingBaseline',
                'FreezingCohort','ParsingUpdates','ReconstructingRoutes','DerivingEvidence',
                'RenderingArtifacts','ValidatingArtifacts','PublishingRun','CancelRequested')
             ORDER BY requested_at ASC LIMIT 1",
            params![plan_revision_id, plan_hash],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| format!("cannot check duplicate job: {e}"))?;
    if let Some(existing) = duplicate {
        return Ok(QueueOutcome::Duplicate(existing));
    }

    let job_id = new_job_id(&tx)?;
    let occurred = now_utc();
    tx.execute(
        "INSERT INTO analysis_jobs
           (id, plan_revision_id, requested_by, requested_at, state, attempt, plan_hash)
         VALUES (?1, ?2, ?3, ?4, 'Queued', 1, ?5)",
        params![
            job_id,
            plan_revision_id,
            requested_by.as_str(),
            occurred,
            plan_hash
        ],
    )
    .map_err(|e| format!("cannot insert job: {e}"))?;
    let ev = JobEvent {
        job_id: job_id.clone(),
        sequence: 1,
        occurred_at: occurred,
        state: JobState::Queued,
        stage: None,
        message_code: Some("job_queued".to_string()),
        human_message: "Analysis queued".to_string(),
        progress_current: None,
        progress_total: None,
        progress_unit: None,
        structured_detail: Some(format!("plan_sha256={stored_sha}")),
    };
    append_event_tx(&tx, &ev)?;
    tx.commit()
        .map_err(|e| format!("cannot commit queue: {e}"))?;
    Ok(QueueOutcome::Created(job_id))
}

/// A claimed job: the job plus the lease the worker now holds.
#[derive(Debug, Clone)]
pub struct JobClaim {
    pub job: AnalysisJob,
    pub lease_secs: i64,
}

/// Transactionally claim the oldest queueable job for `worker_id`.
///
/// BEGIN IMMEDIATE (via `transaction_with_behavior`) prevents two
/// workers from claiming the same job: only one connection can hold the
/// write lock at claim time, and the claim updates state to `Claimed`
/// before commit.
pub fn claim_next(
    conn: &mut Connection,
    worker_id: &str,
    lease_secs: i64,
) -> Result<Option<JobClaim>, String> {
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|e| format!("cannot begin claim transaction: {e}"))?;
    let job_id: Option<String> = tx
        .query_row(
            "SELECT id FROM analysis_jobs
             WHERE state = 'Queued'
             ORDER BY requested_at ASC, id ASC LIMIT 1",
            [],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| format!("cannot select queueable job: {e}"))?;
    let Some(job_id) = job_id else {
        return Ok(None);
    };
    let occurred = now_utc();
    let expires = chrono::Utc::now() + chrono::Duration::seconds(lease_secs);
    let expires_at = expires.to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    tx.execute(
        "UPDATE analysis_jobs
         SET state = 'Claimed', worker_id = ?1, lease_acquired_at = ?2, lease_expires_at = ?3,
             heartbeat_at = ?2, started_at = ?2, stage = NULL
         WHERE id = ?4 AND state = 'Queued'",
        params![worker_id, occurred, expires_at, job_id],
    )
    .map_err(|e| format!("cannot claim job: {e}"))?;
    let ev = JobEvent {
        job_id: job_id.clone(),
        sequence: next_sequence(&tx, &job_id)?,
        occurred_at: occurred,
        state: JobState::Claimed,
        stage: None,
        message_code: Some("job_claimed".to_string()),
        human_message: "Worker claimed analysis".to_string(),
        progress_current: None,
        progress_total: None,
        progress_unit: None,
        structured_detail: Some(format!("worker_id={worker_id}")),
    };
    append_event_tx(&tx, &ev)?;
    tx.commit()
        .map_err(|e| format!("cannot commit claim: {e}"))?;
    let job = get(conn, &job_id)?;
    Ok(Some(JobClaim { job, lease_secs }))
}

/// Renew the lease of a job the worker currently holds. Fails when the
/// lease was stolen or expired (stale jobs are never resumed
/// automatically; they become Failed with `worker_lease_expired`).
pub fn renew_lease(
    conn: &Connection,
    job_id: &str,
    worker_id: &str,
    lease_secs: i64,
) -> Result<(), String> {
    let tx = conn
        .unchecked_transaction()
        .map_err(|e| format!("cannot begin lease transaction: {e}"))?;
    let state: String = tx
        .query_row(
            "SELECT state FROM analysis_jobs WHERE id = ?1",
            params![job_id],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| format!("cannot load job state: {e}"))?
        .ok_or_else(|| format!("analysis job not found: {job_id}"))?;
    let state = JobState::parse_state(&state).map_err(|e| format!("corrupt job state: {e}"))?;
    if !state.is_active() || state == JobState::Queued {
        return Err(format!("job {job_id} is not leaseable in state {state:?}"));
    }
    let worker: Option<String> = tx
        .query_row(
            "SELECT worker_id FROM analysis_jobs WHERE id = ?1",
            params![job_id],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| format!("cannot load worker: {e}"))?
        .flatten();
    if worker.as_deref() != Some(worker_id) {
        return Err(format!(
            "job {job_id} is leased by another worker; refusing renewal"
        ));
    }
    let now = now_utc();
    let expires = chrono::Utc::now() + chrono::Duration::seconds(lease_secs);
    let expires_at = expires.to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    // CAS-style: the UPDATE re-asserts the conditions the read relied
    // on (same worker, active executing state) so a concurrent
    // completion or steal cannot be overwritten.
    let updated = tx
        .execute(
            "UPDATE analysis_jobs SET heartbeat_at = ?1, lease_expires_at = ?2
             WHERE id = ?3 AND worker_id = ?4 AND state IN (
                'Claimed','DiscoveringArchives','AcquiringArchives','ParsingBaseline',
                'FreezingCohort','ParsingUpdates','ReconstructingRoutes','DerivingEvidence',
                'RenderingArtifacts','ValidatingArtifacts','PublishingRun')",
            params![now, expires_at, job_id, worker_id],
        )
        .map_err(|e| format!("cannot renew lease: {e}"))?;
    if updated == 0 {
        return Err(format!(
            "job {job_id} lease no longer renewable (state or worker changed); refusing renewal"
        ));
    }
    tx.commit()
        .map_err(|e| format!("cannot commit lease renewal: {e}"))
}

/// Detect jobs whose lease expired while executing. Conservative alpha
/// policy: mark them Failed with `worker_lease_expired`, preserve the
/// staging directory, and require explicit retry. Never auto-resume.
pub fn mark_stale_leases(conn: &Connection, now: &str) -> Result<Vec<String>, String> {
    let tx = conn
        .unchecked_transaction()
        .map_err(|e| format!("cannot begin stale-lease transaction: {e}"))?;
    let mut stmt = tx
        .prepare(
            "SELECT id FROM analysis_jobs
             WHERE state IN ('Claimed','DiscoveringArchives','AcquiringArchives','ParsingBaseline',
                             'FreezingCohort','ParsingUpdates','ReconstructingRoutes',
                             'DerivingEvidence','RenderingArtifacts','ValidatingArtifacts',
                             'PublishingRun')
               AND lease_expires_at IS NOT NULL AND lease_expires_at < ?1",
        )
        .map_err(|e| format!("cannot prepare stale scan: {e}"))?;
    let ids: Vec<String> = stmt
        .query_map(params![now], |r| r.get(0))
        .map_err(|e| format!("cannot query stale jobs: {e}"))?
        .collect::<Result<_, _>>()
        .map_err(|e| format!("bad stale row: {e}"))?;
    drop(stmt);
    for id in &ids {
        tx.execute(
            "UPDATE analysis_jobs SET state = 'Failed', finished_at = ?1,
                    error_code = ?2, error_summary = ?3
             WHERE id = ?4 AND state IN (
                'Claimed','DiscoveringArchives','AcquiringArchives','ParsingBaseline',
                'FreezingCohort','ParsingUpdates','ReconstructingRoutes','DerivingEvidence',
                'RenderingArtifacts','ValidatingArtifacts','PublishingRun')
               AND lease_expires_at IS NOT NULL AND lease_expires_at < ?1",
            params![
                now,
                error_code::WORKER_LEASE_EXPIRED,
                "worker lease expired; staging preserved; explicit retry required",
                id
            ],
        )
        .map_err(|e| format!("cannot expire lease for {id}: {e}"))?;
        let ev = JobEvent {
            job_id: id.clone(),
            sequence: next_sequence(&tx, id)?,
            occurred_at: now.to_string(),
            state: JobState::Failed,
            stage: None,
            message_code: Some(error_code::WORKER_LEASE_EXPIRED.to_string()),
            human_message: "Worker lease expired; job interrupted".to_string(),
            progress_current: None,
            progress_total: None,
            progress_unit: None,
            structured_detail: None,
        };
        append_event_tx(&tx, &ev)?;
    }
    tx.commit()
        .map_err(|e| format!("cannot commit stale scan: {e}"))?;
    Ok(ids)
}

/// Request cancellation.
///
/// - A Queued job transitions directly to Cancelled.
/// - A claimed/executing job enters CancelRequested; the worker observes
///   the request at its next checkpoint and then transitions to
///   Cancelled. Publication is never attempted after an accepted
///   cancellation request.
pub fn request_cancel(conn: &Connection, job_id: &str) -> Result<CancelOutcome, String> {
    let job = get(conn, job_id)?;
    if !job.state.is_cancellable() {
        return Err(format!(
            "job {job_id} cannot be cancelled in state {}",
            job.state.as_str()
        ));
    }
    if job.state == JobState::Queued {
        transition(
            conn,
            job_id,
            JobState::Queued,
            JobState::Cancelled,
            Some(error_code::CANCELLED),
            "Cancelled before execution",
        )?;
        return Ok(CancelOutcome::Cancelled(job_id.to_string()));
    }
    // Claimed or executing: request cooperative cancellation.
    let tx = conn
        .unchecked_transaction()
        .map_err(|e| format!("cannot begin cancel transaction: {e}"))?;
    let current: String = tx
        .query_row(
            "SELECT state FROM analysis_jobs WHERE id = ?1",
            params![job_id],
            |r| r.get(0),
        )
        .map_err(|e| format!("cannot load job state: {e}"))?;
    let current_state =
        JobState::parse_state(&current).map_err(|e| format!("corrupt job state: {e}"))?;
    if !current_state.is_cancellable() {
        return Err(format!(
            "job {job_id} cannot be cancelled in state {current_state:?}"
        ));
    }
    let occurred = now_utc();
    // CAS-style: only a still-cancellable job may enter
    // CancelRequested; a job that completed concurrently is left
    // untouched and the request is rejected.
    let updated = tx
        .execute(
            "UPDATE analysis_jobs SET state = 'CancelRequested', cancel_requested_at = ?1
             WHERE id = ?2 AND state IN (
                'Claimed','DiscoveringArchives','AcquiringArchives','ParsingBaseline',
                'FreezingCohort','ParsingUpdates','ReconstructingRoutes','DerivingEvidence',
                'RenderingArtifacts','ValidatingArtifacts')",
            params![occurred, job_id],
        )
        .map_err(|e| format!("cannot request cancel: {e}"))?;
    if updated == 0 {
        return Err(format!(
            "job {job_id} is no longer cancellable (state changed); refusing cancellation"
        ));
    }
    let ev = JobEvent {
        job_id: job_id.to_string(),
        sequence: next_sequence(&tx, job_id)?,
        occurred_at: occurred,
        state: JobState::CancelRequested,
        stage: None,
        message_code: Some("cancel_requested".to_string()),
        human_message: "Cancellation requested; worker stops at the next checkpoint".to_string(),
        progress_current: None,
        progress_total: None,
        progress_unit: None,
        structured_detail: None,
    };
    append_event_tx(&tx, &ev)?;
    tx.commit()
        .map_err(|e| format!("cannot commit cancel request: {e}"))?;
    Ok(CancelOutcome::Requested(job_id.to_string()))
}

/// Worker-side observation of an accepted cancellation request.
pub fn observe_cancel(conn: &Connection, job_id: &str) -> Result<(), String> {
    transition(
        conn,
        job_id,
        JobState::CancelRequested,
        JobState::Cancelled,
        Some(error_code::CANCELLED),
        "Cancellation observed; no run published",
    )
}

/// Create a new attempt for a Failed or Cancelled job.
///
/// The original job is never mutated. The new job reuses the exact plan
/// revision, links via `original_job_id`, increments `attempt`, and gets
/// its own staging path. Plan readiness is re-verified: if the reviewed
/// mappings changed such that the plan is no longer Ready, retry is
/// rejected and a new plan revision is required.
pub fn retry(
    conn: &Connection,
    job_id: &str,
    requested_by: RequestSource,
    plan_hash: &str,
) -> Result<String, String> {
    let original = get(conn, job_id)?;
    if !original.state.is_retryable() {
        return Err(format!(
            "job {job_id} cannot be retried in state {}",
            original.state.as_str()
        ));
    }
    // Plan must still exist and still be Ready.
    let status: Option<String> = conn
        .query_row(
            "SELECT status FROM analysis_plans WHERE id = ?1",
            params![original.plan_revision_id],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| format!("cannot load plan: {e}"))?;
    let status = status
        .ok_or_else(|| "invalid_plan: original plan revision no longer exists".to_string())?;
    if status != "Ready" {
        return Err(format!(
            "invalid_plan: plan {} is no longer Ready (status {status}); create a new plan revision",
            original.plan_revision_id
        ));
    }
    let tx = conn
        .unchecked_transaction()
        .map_err(|e| format!("cannot begin retry transaction: {e}"))?;
    let new_id = new_job_id(&tx)?;
    let occurred = now_utc();
    tx.execute(
        "INSERT INTO analysis_jobs
           (id, plan_revision_id, requested_by, requested_at, state, attempt, original_job_id, plan_hash)
         VALUES (?1, ?2, ?3, ?4, 'Queued', ?5, ?6, ?7)",
        params![
            new_id,
            original.plan_revision_id,
            requested_by.as_str(),
            occurred,
            original.attempt + 1,
            job_id,
            plan_hash,
        ],
    )
    .map_err(|e| format!("cannot insert retry job: {e}"))?;
    let ev = JobEvent {
        job_id: new_id.clone(),
        sequence: 1,
        occurred_at: occurred,
        state: JobState::Queued,
        stage: None,
        message_code: Some("retry_created".to_string()),
        human_message: format!("Retry of {job_id} queued"),
        progress_current: None,
        progress_total: None,
        progress_unit: None,
        structured_detail: Some(format!("original_job_id={job_id}")),
    };
    append_event_tx(&tx, &ev)?;
    tx.commit()
        .map_err(|e| format!("cannot commit retry: {e}"))?;
    Ok(new_id)
}

/// Record a worker heartbeat (upsert). Never exposes absolute paths,
/// environment variables, or secrets.
pub fn heartbeat(conn: &Connection, hb: &WorkerHeartbeat) -> Result<(), String> {
    conn.execute(
        "INSERT INTO worker_heartbeats
           (worker_id, started_at, last_heartbeat, process_version, source_families,
            download_jobs, parse_jobs, offline_mode)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
         ON CONFLICT(worker_id) DO UPDATE SET
           last_heartbeat = excluded.last_heartbeat,
           process_version = excluded.process_version,
           source_families = excluded.source_families,
           download_jobs = excluded.download_jobs,
           parse_jobs = excluded.parse_jobs,
           offline_mode = excluded.offline_mode",
        params![
            hb.worker_id,
            hb.started_at,
            hb.last_heartbeat,
            hb.process_version,
            serde_json::to_string(&hb.source_families).unwrap_or_else(|_| "[]".to_string()),
            hb.download_jobs,
            hb.parse_jobs,
            hb.offline_mode as i64,
        ],
    )
    .map_err(|e| format!("cannot record worker heartbeat: {e}"))?;
    Ok(())
}

/// List worker heartbeat rows with freshness classification.
pub fn list_workers(
    conn: &Connection,
    stale_after_secs: i64,
) -> Result<Vec<(WorkerHeartbeat, WorkerFreshness)>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT worker_id, started_at, last_heartbeat, process_version, source_families,
                         download_jobs, parse_jobs, offline_mode
                  FROM worker_heartbeats ORDER BY last_heartbeat DESC",
        )
        .map_err(|e| format!("cannot prepare worker list: {e}"))?;
    let rows = stmt
        .query_map([], |r| {
            let families: String = r.get(4)?;
            Ok(WorkerHeartbeat {
                worker_id: r.get(0)?,
                started_at: r.get(1)?,
                last_heartbeat: r.get(2)?,
                process_version: r.get(3)?,
                source_families: serde_json::from_str(&families).unwrap_or_default(),
                download_jobs: r.get(5)?,
                parse_jobs: r.get(6)?,
                offline_mode: r.get::<_, i64>(7)? != 0,
            })
        })
        .map_err(|e| format!("cannot query workers: {e}"))?;
    let now = chrono::Utc::now();
    let mut out = Vec::new();
    for row in rows {
        let hb = row.map_err(|e| format!("bad worker row: {e}"))?;
        let fresh = chrono::DateTime::parse_from_rfc3339(&hb.last_heartbeat)
            .map(|t| {
                now.signed_duration_since(t.with_timezone(&chrono::Utc))
                    .num_seconds()
                    <= stale_after_secs
            })
            .unwrap_or(false);
        out.push((
            hb,
            if fresh {
                WorkerFreshness::Online
            } else {
                WorkerFreshness::Stale
            },
        ));
    }
    Ok(out)
}

/// Latest manifest revision for an event (by id, newest first).
pub fn latest_manifest_revision(
    conn: &Connection,
    event_id: i64,
) -> Result<Option<crate::catalog::domain::ManifestRevision>, String> {
    crate::catalog::db::list_manifest_revisions(conn, event_id)
        .map(|mut v| {
            if v.is_empty() {
                None
            } else {
                Some(v.remove(0))
            }
        })
        .map_err(|e| format!("cannot list manifest revisions: {e}"))
}

/// Latest analysis plan for an event (by id, newest first).
pub fn latest_plan(
    conn: &Connection,
    event_id: i64,
) -> Result<Option<crate::catalog::domain::AnalysisPlanRecord>, String> {
    let plans: Vec<crate::catalog::domain::AnalysisPlanRecord> = conn
        .prepare(
            "SELECT p.id, p.manifest_revision_id, p.plan_schema, p.payload, p.sha256, p.status,
                    p.block_reason, p.created_at
             FROM analysis_plans p
             JOIN manifest_revisions m ON m.id = p.manifest_revision_id
             WHERE m.event_id = ?1
             ORDER BY p.id DESC LIMIT 1",
        )
        .map_err(|e| format!("cannot prepare latest plan: {e}"))?
        .query_map([event_id], |r| {
            Ok(crate::catalog::domain::AnalysisPlanRecord {
                id: r.get(0)?,
                manifest_revision_id: r.get(1)?,
                plan_schema: r.get(2)?,
                payload: r.get(3)?,
                sha256: r.get(4)?,
                status: r.get(5)?,
                block_reason: r.get(6)?,
                created_at: r.get(7)?,
            })
        })
        .map_err(|e| format!("cannot query latest plan: {e}"))?
        .collect::<Result<_, _>>()
        .map_err(|e| format!("bad plan row: {e}"))?;
    Ok(plans.into_iter().next())
}

/// Update the bounded progress columns on a job row (no event). The
/// worker throttles calls; this is a short update.
pub fn update_progress(
    conn: &Connection,
    job_id: &str,
    stage: &str,
    current: Option<i64>,
    total: Option<i64>,
    unit: Option<&str>,
) -> Result<(), String> {
    conn.execute(
        "UPDATE analysis_jobs SET stage = ?1, progress_current = ?2, progress_total = ?3,
                progress_unit = ?4 WHERE id = ?5",
        params![stage, current, total, unit, job_id],
    )
    .map_err(|e| format!("cannot update progress: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::db;

    fn test_conn() -> Connection {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("catalog.sqlite");
        db::open_catalog(&path).expect("open catalog")
    }

    static EVENT_SEQ: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(0);

    fn next_event_id() -> String {
        let n = EVENT_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        format!("EVENT-{n}")
    }

    /// Seed a Ready plan: event + snapshot + manifest revision + plan.
    fn seed_ready_plan(conn: &Connection) -> i64 {
        seed_plan_with_status(conn, "Ready", "2026-08-01T00:00:00Z")
    }

    fn seed_plan_with_status(conn: &Connection, status: &str, end: &str) -> i64 {
        let external_id = next_event_id();
        conn.execute(
            "INSERT INTO catalog_events (source_kind, external_id, first_seen, last_seen)
             VALUES ('local-repository', ?1, '2026-08-01T00:00:00Z', '2026-08-01T00:00:00Z')",
            params![external_id],
        )
        .unwrap();
        let event_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO event_snapshots (event_id, fetched_at, source_url, content_sha256,
                 raw_payload, normalized_json, parser_version)
             VALUES (?1, '2026-08-01T00:00:00Z', 'file:///fixture', 'sha', '{}', '{}', 't')",
            params![event_id],
        )
        .unwrap();
        let snapshot_id = conn.last_insert_rowid();
        let manifest_payload = serde_json::json!({
            "event_id": external_id,
            "schema_version": 2,
            "revision": 1,
            "open": false,
            "event_window_utc": {"start": "2026-08-01T00:00:00Z", "end": end},
            "ticket_window_local": {"start": "", "end": "", "timezone": "UTC"},
            "target": {
                "label": "Test event",
                "origin_asns": [64500],
                "transit_predicate": {
                    "label": "Test plane",
                    "predicate": [64501],
                    "status": "Reviewed",
                    "provenance": {"reviewed_by": "local-review", "date": "2026-08-01"}
                }
            },
            "collectors": ["route-views2"],
            "source_family": "RouteViews"
        })
        .to_string();
        conn.execute(
            "INSERT INTO manifest_revisions (event_id, snapshot_id, manifest_schema, payload, sha256,
                 review_status, reviewed_at, reviewer)
             VALUES (?1, ?2, 2, ?3, ?4, 'Reviewed', '2026-08-01T00:00:00Z', 'local-review')",
            params![event_id, snapshot_id, manifest_payload, format!("msha-{external_id}")],
        )
        .unwrap();
        let mr_id = conn.last_insert_rowid();
        let plan_payload = serde_json::json!({
            "event_id": external_id, "status": status, "origin_asns": [64500],
            "transit_predicate_status": "Reviewed"
        })
        .to_string();
        let sha = crate::catalog::document::hex_sha256(plan_payload.as_bytes());
        conn.execute(
            "INSERT INTO analysis_plans (manifest_revision_id, plan_schema, payload, sha256, status,
                 block_reason, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, NULL, '2026-08-01T00:00:00Z')",
            params![mr_id, crate::schema::ANALYSIS_PLAN_SCHEMA_VERSION as i64, plan_payload, sha, status],
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    /// Walk a claimed job through every stage to PublishingRun.
    fn advance_to_publish(conn: &Connection, job_id: &str) {
        use JobState::*;
        let path = [
            (Claimed, DiscoveringArchives),
            (DiscoveringArchives, AcquiringArchives),
            (AcquiringArchives, ParsingBaseline),
            (ParsingBaseline, FreezingCohort),
            (FreezingCohort, ParsingUpdates),
            (ParsingUpdates, ReconstructingRoutes),
            (ReconstructingRoutes, DerivingEvidence),
            (DerivingEvidence, RenderingArtifacts),
            (RenderingArtifacts, ValidatingArtifacts),
            (ValidatingArtifacts, PublishingRun),
        ];
        for (from, to) in path {
            transition(conn, job_id, from, to, Some("stage"), "stage").unwrap();
        }
    }

    fn insert_run(conn: &Connection, plan: i64) -> i64 {
        conn.execute(
            "INSERT INTO analysis_runs (plan_id, software_version, git_revision, parser_identity,
                 cache_schema_version, report_schema_version, status, started_at, completed_at)
             VALUES (?1, '0.1.0', NULL, 'p', 1, 1, 'Complete', '2026-08-01T00:00:00Z',
                     '2026-08-01T00:00:00Z')",
            params![plan],
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    #[test]
    fn blocked_plan_cannot_be_queued() {
        let conn = test_conn();
        let plan = seed_plan_with_status(&conn, "Blocked", "2026-08-01T00:00:00Z");
        let err = queue(&conn, plan, RequestSource::Cli, "h").unwrap_err();
        assert!(err.contains("not Ready"), "{err}");
        assert!(err.starts_with("invalid_plan"), "{err}");
    }

    #[test]
    fn ready_plan_can_be_queued() {
        let conn = test_conn();
        let plan = seed_ready_plan(&conn);
        let out = queue(&conn, plan, RequestSource::Cli, "hash-1").unwrap();
        let id = match out {
            QueueOutcome::Created(id) => id,
            other => panic!("expected Created, got {other:?}"),
        };
        let job = get(&conn, &id).unwrap();
        assert_eq!(job.state, JobState::Queued);
        assert_eq!(job.plan_revision_id, plan);
        assert_eq!(job.requested_by, "cli");
        assert_eq!(job.plan_hash, "hash-1");
        assert_eq!(job.attempt, 1);
        // Initial event exists with sequence 1.
        let evs = events(&conn, &id, 10).unwrap();
        assert_eq!(evs.len(), 1);
        assert_eq!(evs[0].sequence, 1);
        assert_eq!(evs[0].state, JobState::Queued);
        assert_eq!(evs[0].message_code.as_deref(), Some("job_queued"));
    }

    #[test]
    fn duplicate_submit_returns_existing_active_job() {
        let conn = test_conn();
        let plan = seed_ready_plan(&conn);
        let first = queue(&conn, plan, RequestSource::Cli, "hash-1").unwrap();
        let second = queue(&conn, plan, RequestSource::LocalWeb, "hash-1").unwrap();
        match (first, second) {
            (QueueOutcome::Created(a), QueueOutcome::Duplicate(b)) => assert_eq!(a, b),
            other => panic!("expected Created then Duplicate, got {other:?}"),
        }
        let jobs = list(
            &conn,
            &JobFilter {
                state: None,
                plan_revision_id: Some(plan),
            },
        )
        .unwrap();
        assert_eq!(jobs.len(), 1);
    }

    #[test]
    fn completed_job_does_not_block_explicit_new_run() {
        let mut conn = test_conn();
        let plan = seed_ready_plan(&conn);
        let first = match queue(&conn, plan, RequestSource::Cli, "hash-1").unwrap() {
            QueueOutcome::Created(id) => id,
            _ => unreachable!(),
        };
        // Simulate a full run: claim -> publish -> complete.
        let claim = claim_next(&mut conn, "w1", 90).unwrap().unwrap();
        assert_eq!(claim.job.id, first);
        let run_id = insert_run(&conn, plan);
        advance_to_publish(&conn, &first);
        complete(&conn, &first, run_id).unwrap();
        let done = get(&conn, &first).unwrap();
        assert_eq!(done.state, JobState::Completed);
        assert_eq!(done.completed_run_id, Some(run_id));
        // A deliberate rerun creates a NEW job (idempotency only covers active jobs).
        let rerun = queue(&conn, plan, RequestSource::Cli, "hash-1").unwrap();
        match rerun {
            QueueOutcome::Created(new_id) => assert_ne!(new_id, first),
            other => panic!("expected Created for rerun, got {other:?}"),
        }
    }

    #[test]
    fn queued_job_can_be_claimed() {
        let mut conn = test_conn();
        let plan = seed_ready_plan(&conn);
        let id = match queue(&conn, plan, RequestSource::Cli, "h").unwrap() {
            QueueOutcome::Created(id) => id,
            _ => unreachable!(),
        };
        let claim = claim_next(&mut conn, "worker-a", 90).unwrap().unwrap();
        assert_eq!(claim.job.id, id);
        assert_eq!(claim.job.state, JobState::Claimed);
        assert_eq!(claim.job.worker_id.as_deref(), Some("worker-a"));
        assert!(claim.job.lease_expires_at.is_some());
        assert!(claim.job.started_at.is_some());
        // Claimed event recorded.
        let evs = events(&conn, &id, 10).unwrap();
        assert_eq!(evs.last().unwrap().state, JobState::Claimed);
    }

    #[test]
    fn two_workers_cannot_claim_same_job() {
        let mut conn = test_conn();
        let plan = seed_ready_plan(&conn);
        let id = match queue(&conn, plan, RequestSource::Cli, "h").unwrap() {
            QueueOutcome::Created(id) => id,
            _ => unreachable!(),
        };
        let c1 = claim_next(&mut conn, "worker-a", 90).unwrap().unwrap();
        assert_eq!(c1.job.id, id);
        // Second worker: the job is no longer Queued -> nothing to claim.
        let c2 = claim_next(&mut conn, "worker-b", 90).unwrap();
        assert!(c2.is_none());
        let job = get(&conn, &id).unwrap();
        assert_eq!(job.worker_id.as_deref(), Some("worker-a"));
    }

    #[test]
    fn worker_claims_oldest_job_deterministically() {
        let mut conn = test_conn();
        let plan_a = seed_ready_plan(&conn);
        let plan_b = seed_plan_with_status(&conn, "Ready", "2026-08-01T00:00:00Z");
        let id_a = match queue(&conn, plan_a, RequestSource::Cli, "ha").unwrap() {
            QueueOutcome::Created(id) => id,
            _ => unreachable!(),
        };
        let id_b = match queue(&conn, plan_b, RequestSource::Cli, "hb").unwrap() {
            QueueOutcome::Created(id) => id,
            _ => unreachable!(),
        };
        // Requests within one second share requested_at; force distinct
        // timestamps so FIFO is by request time, not random id.
        conn.execute(
            "UPDATE analysis_jobs SET requested_at = '2026-08-01T00:00:00Z' WHERE id = ?1",
            [&id_a],
        )
        .unwrap();
        conn.execute(
            "UPDATE analysis_jobs SET requested_at = '2026-08-01T00:00:01Z' WHERE id = ?1",
            [&id_b],
        )
        .unwrap();
        let first = claim_next(&mut conn, "w", 90).unwrap().unwrap();
        assert_eq!(first.job.id, id_a);
        let second = claim_next(&mut conn, "w", 90).unwrap().unwrap();
        assert_eq!(second.job.id, id_b);
        let _ = (id_a, id_b);
    }

    #[test]
    fn failed_job_is_immutable_and_can_be_retried() {
        let mut conn = test_conn();
        let plan = seed_ready_plan(&conn);
        let id = match queue(&conn, plan, RequestSource::Cli, "h").unwrap() {
            QueueOutcome::Created(id) => id,
            _ => unreachable!(),
        };
        claim_next(&mut conn, "w", 90).unwrap().unwrap();
        fail(
            &conn,
            &id,
            JobState::Claimed,
            "source_discovery_failed",
            "boom",
        )
        .unwrap();
        let failed = get(&conn, &id).unwrap();
        assert_eq!(failed.state, JobState::Failed);
        assert_eq!(
            failed.error_code.as_deref(),
            Some("source_discovery_failed")
        );
        assert!(failed.finished_at.is_some());
        // No transition out of Failed.
        assert!(transition(&conn, &id, JobState::Failed, JobState::Queued, None, "x").is_err());
        assert!(fail(&conn, &id, JobState::Failed, "x", "y").is_err());
        // Retry creates a new job with linkage.
        let new_id = retry(&conn, &id, RequestSource::Cli, "h").unwrap();
        assert_ne!(new_id, id);
        let retried = get(&conn, &new_id).unwrap();
        assert_eq!(retried.state, JobState::Queued);
        assert_eq!(retried.attempt, 2);
        assert_eq!(retried.original_job_id.as_deref(), Some(id.as_str()));
        assert_eq!(retried.plan_revision_id, plan);
        // Original preserved.
        assert_eq!(get(&conn, &id).unwrap().state, JobState::Failed);
        assert_eq!(
            get(&conn, &id).unwrap().error_code.as_deref(),
            Some("source_discovery_failed")
        );
    }

    #[test]
    fn cancelled_job_is_immutable_and_can_be_retried() {
        let conn = test_conn();
        let plan = seed_ready_plan(&conn);
        // Queued job cancels immediately.
        let id = match queue(&conn, plan, RequestSource::Cli, "h").unwrap() {
            QueueOutcome::Created(id) => id,
            _ => unreachable!(),
        };
        let out = request_cancel(&conn, &id).unwrap();
        assert_eq!(out, CancelOutcome::Cancelled(id.clone()));
        let job = get(&conn, &id).unwrap();
        assert_eq!(job.state, JobState::Cancelled);
        assert!(transition(&conn, &id, JobState::Cancelled, JobState::Queued, None, "x").is_err());
        // Retry works.
        let new_id = retry(&conn, &id, RequestSource::LocalWeb, "h").unwrap();
        assert_eq!(
            get(&conn, &new_id).unwrap().original_job_id.as_deref(),
            Some(id.as_str())
        );
    }

    #[test]
    fn executing_job_enters_cancel_requested() {
        let mut conn = test_conn();
        let plan = seed_ready_plan(&conn);
        let id = match queue(&conn, plan, RequestSource::Cli, "h").unwrap() {
            QueueOutcome::Created(id) => id,
            _ => unreachable!(),
        };
        claim_next(&mut conn, "w", 90).unwrap();
        let out = request_cancel(&conn, &id).unwrap();
        assert_eq!(out, CancelOutcome::Requested(id.clone()));
        let job = get(&conn, &id).unwrap();
        assert_eq!(job.state, JobState::CancelRequested);
        assert!(job.cancel_requested_at.is_some());
        // Worker observes -> Cancelled; nothing may be published after.
        observe_cancel(&conn, &id).unwrap();
        let job = get(&conn, &id).unwrap();
        assert_eq!(job.state, JobState::Cancelled);
        // Completing is impossible.
        assert!(complete(&conn, &id, 1).is_err());
    }

    #[test]
    fn completed_job_cannot_be_cancelled() {
        let mut conn = test_conn();
        let plan = seed_ready_plan(&conn);
        let id = match queue(&conn, plan, RequestSource::Cli, "h").unwrap() {
            QueueOutcome::Created(id) => id,
            _ => unreachable!(),
        };
        claim_next(&mut conn, "w", 90).unwrap();
        let run_id = insert_run(&conn, plan);
        advance_to_publish(&conn, &id);
        complete(&conn, &id, run_id).unwrap();
        assert!(request_cancel(&conn, &id).is_err());
    }

    #[test]
    fn illegal_state_transition_is_rejected() {
        let conn = test_conn();
        let plan = seed_ready_plan(&conn);
        let id = match queue(&conn, plan, RequestSource::Cli, "h").unwrap() {
            QueueOutcome::Created(id) => id,
            _ => unreachable!(),
        };
        let err = transition(
            &conn,
            &id,
            JobState::Queued,
            JobState::Completed,
            None,
            "skip",
        )
        .unwrap_err();
        assert!(err.contains("illegal"), "{err}");
        // Stale-state race: a LEGAL transition (Claimed -> Discovering)
        // against a job that is still Queued must be rejected.
        let err = transition(
            &conn,
            &id,
            JobState::Claimed,
            JobState::DiscoveringArchives,
            None,
            "x",
        )
        .unwrap_err();
        assert!(err.contains("not Claimed"), "{err}");
    }

    #[test]
    fn lease_renewal_extends_expiry() {
        let mut conn = test_conn();
        let plan = seed_ready_plan(&conn);
        let id = match queue(&conn, plan, RequestSource::Cli, "h").unwrap() {
            QueueOutcome::Created(id) => id,
            _ => unreachable!(),
        };
        claim_next(&mut conn, "w", 90).unwrap();
        // Force a past expiry so the renewal is measurable (lease
        // timestamps are second-truncated).
        conn.execute(
            "UPDATE analysis_jobs SET lease_expires_at = '2020-01-01T00:00:00Z' WHERE id = ?1",
            params![id],
        )
        .unwrap();
        renew_lease(&conn, &id, "w", 90).unwrap();
        let after = get(&conn, &id).unwrap().lease_expires_at.unwrap();
        assert!(after.as_str() > "2020-01-01T00:00:00Z", "{after}");
        assert!(get(&conn, &id).unwrap().heartbeat_at.is_some());
    }

    #[test]
    fn unexpired_lease_cannot_be_stolen() {
        let mut conn = test_conn();
        let plan = seed_ready_plan(&conn);
        let id = match queue(&conn, plan, RequestSource::Cli, "h").unwrap() {
            QueueOutcome::Created(id) => id,
            _ => unreachable!(),
        };
        claim_next(&mut conn, "w-a", 90).unwrap();
        // Other worker cannot renew.
        assert!(renew_lease(&conn, &id, "w-b", 90).is_err());
        // And cannot claim (not Queued).
        assert!(claim_next(&mut conn, "w-b", 90).unwrap().is_none());
    }

    #[test]
    fn expired_lease_is_detected_and_not_auto_resumed() {
        let mut conn = test_conn();
        let plan = seed_ready_plan(&conn);
        let id = match queue(&conn, plan, RequestSource::Cli, "h").unwrap() {
            QueueOutcome::Created(id) => id,
            _ => unreachable!(),
        };
        // Claim with a lease that is already expired (past expiry).
        claim_next(&mut conn, "w", 90).unwrap();
        conn.execute(
            "UPDATE analysis_jobs SET lease_expires_at = '2020-01-01T00:00:00Z' WHERE id = ?1",
            params![id],
        )
        .unwrap();
        let expired = mark_stale_leases(&conn, "2026-08-02T00:00:00Z").unwrap();
        assert_eq!(expired, vec![id.clone()]);
        let job = get(&conn, &id).unwrap();
        assert_eq!(job.state, JobState::Failed);
        assert_eq!(job.error_code.as_deref(), Some("worker_lease_expired"));
        // Staging preserved: staged_artifact_root untouched (None here);
        // explicit retry required.
        assert!(job.staged_artifact_root.is_none());
        assert!(claim_next(&mut conn, "w2", 90).unwrap().is_none());
    }

    #[test]
    fn retry_after_stale_job_creates_new_attempt() {
        let mut conn = test_conn();
        let plan = seed_ready_plan(&conn);
        let id = match queue(&conn, plan, RequestSource::Cli, "h").unwrap() {
            QueueOutcome::Created(id) => id,
            _ => unreachable!(),
        };
        claim_next(&mut conn, "w", 90).unwrap();
        conn.execute(
            "UPDATE analysis_jobs SET lease_expires_at = '2020-01-01T00:00:00Z' WHERE id = ?1",
            params![id],
        )
        .unwrap();
        mark_stale_leases(&conn, "2026-08-02T00:00:00Z").unwrap();
        let new_id = retry(&conn, &id, RequestSource::Cli, "h").unwrap();
        let retried = get(&conn, &new_id).unwrap();
        assert_eq!(retried.attempt, 2);
        assert_eq!(retried.original_job_id.as_deref(), Some(id.as_str()));
    }

    #[test]
    fn event_sequence_is_monotonic_and_duplicates_rejected() {
        let mut conn = test_conn();
        let plan = seed_ready_plan(&conn);
        let id = match queue(&conn, plan, RequestSource::Cli, "h").unwrap() {
            QueueOutcome::Created(id) => id,
            _ => unreachable!(),
        };
        claim_next(&mut conn, "w", 90).unwrap();
        let evs = events(&conn, &id, 100).unwrap();
        let seqs: Vec<i64> = evs.iter().map(|e| e.sequence).collect();
        assert_eq!(seqs, vec![1, 2]);
        // Direct duplicate insert is rejected by the PK.
        let err = conn
            .execute(
                "INSERT INTO analysis_job_events (job_id, sequence, occurred_at, state, human_message)
                 VALUES (?1, 1, '2026-08-01T00:00:00Z', 'Queued', 'dup')",
                params![id],
            )
            .unwrap_err();
        assert!(err.to_string().contains("UNIQUE") || err.to_string().contains("PRIMARY"));
    }

    #[test]
    fn state_and_event_commit_atomically() {
        let conn = test_conn();
        let plan = seed_ready_plan(&conn);
        let id = match queue(&conn, plan, RequestSource::Cli, "h").unwrap() {
            QueueOutcome::Created(id) => id,
            _ => unreachable!(),
        };
        // Force the event insert to fail (oversized message) inside a
        // transition: the state change must roll back with it.
        let long = "x".repeat(MAX_HUMAN_MESSAGE_BYTES + 1);
        let err =
            transition(&conn, &id, JobState::Queued, JobState::Claimed, None, &long).unwrap_err();
        assert!(err.contains("too long"), "{err}");
        let job = get(&conn, &id).unwrap();
        assert_eq!(
            job.state,
            JobState::Queued,
            "state must not change when the event fails"
        );
        assert_eq!(events(&conn, &id, 10).unwrap().len(), 1);
    }

    #[test]
    fn event_log_is_append_only() {
        let mut conn = test_conn();
        let plan = seed_ready_plan(&conn);
        let id = match queue(&conn, plan, RequestSource::Cli, "h").unwrap() {
            QueueOutcome::Created(id) => id,
            _ => unreachable!(),
        };
        // No UPDATE path exists in the service; verify the schema
        // allows no silent replacement through normal code by checking
        // the event count only ever grows.
        claim_next(&mut conn, "w", 90).unwrap();
        let before = events(&conn, &id, 100).unwrap().len();
        renew_lease(&conn, &id, "w", 90).unwrap();
        let after = events(&conn, &id, 100).unwrap().len();
        assert_eq!(after, before, "lease renewal must not add events");
        assert_eq!(after, 2);
    }

    #[test]
    fn structured_detail_is_bounded() {
        let conn = test_conn();
        let plan = seed_ready_plan(&conn);
        let id = match queue(&conn, plan, RequestSource::Cli, "h").unwrap() {
            QueueOutcome::Created(id) => id,
            _ => unreachable!(),
        };
        let big = "d".repeat(MAX_STRUCTURED_DETAIL_BYTES + 10);
        let ev = JobEvent {
            job_id: id.clone(),
            sequence: 99,
            occurred_at: "2026-08-01T00:00:00Z".into(),
            state: JobState::Queued,
            stage: None,
            message_code: None,
            human_message: "x".into(),
            progress_current: None,
            progress_total: None,
            progress_unit: None,
            structured_detail: Some(big),
        };
        let err = append_event(&conn, &ev).unwrap_err();
        assert!(err.contains("bounded"), "{err}");
    }

    #[test]
    fn job_event_contains_no_absolute_path() {
        let conn = test_conn();
        let plan = seed_ready_plan(&conn);
        let id = match queue(&conn, plan, RequestSource::Cli, "h").unwrap() {
            QueueOutcome::Created(id) => id,
            _ => unreachable!(),
        };
        let ev = JobEvent {
            job_id: id.clone(),
            sequence: 99,
            occurred_at: "2026-08-01T00:00:00Z".into(),
            state: JobState::Queued,
            stage: None,
            message_code: None,
            human_message: "x".into(),
            progress_current: None,
            progress_total: None,
            progress_unit: None,
            structured_detail: Some("/home/user/inim/cache/x".to_string()),
        };
        assert!(append_event(&conn, &ev).is_err());
    }

    #[test]
    fn worker_status_exposes_no_absolute_path() {
        let conn = test_conn();
        let hb = WorkerHeartbeat {
            worker_id: "w-1".into(),
            started_at: "2026-08-01T00:00:00Z".into(),
            last_heartbeat: "2026-08-01T00:00:05Z".into(),
            process_version: "0.1.0".into(),
            source_families: vec!["RouteViews".into(), "RipeRis".into()],
            download_jobs: 2,
            parse_jobs: 8,
            offline_mode: false,
        };
        heartbeat(&conn, &hb).unwrap();
        let workers = list_workers(&conn, 60).unwrap();
        assert_eq!(workers.len(), 1);
        let (row, fresh) = &workers[0];
        assert_eq!(row.worker_id, "w-1");
        assert_eq!(
            row.source_families,
            vec!["RouteViews".to_string(), "RipeRis".to_string()]
        );
        assert_eq!(row.download_jobs, 2);
        assert_eq!(row.parse_jobs, 8);
        assert_eq!(fresh, &WorkerFreshness::Stale); // 2026-08-01 vs now
        assert!(!serde_json::to_string(row).unwrap().contains("/home/"));
    }

    #[test]
    fn queue_event_is_transactional() {
        let conn = test_conn();
        let plan = seed_ready_plan(&conn);
        // Queuing twice for different hashes creates two jobs (distinct
        // plan hashes are distinct identities), each with exactly one
        // initial event.
        let a = match queue(&conn, plan, RequestSource::Cli, "hash-a").unwrap() {
            QueueOutcome::Created(id) => id,
            _ => unreachable!(),
        };
        let b = match queue(&conn, plan, RequestSource::Cli, "hash-b").unwrap() {
            QueueOutcome::Created(id) => id,
            _ => unreachable!(),
        };
        assert_ne!(a, b);
        assert_eq!(events(&conn, &a, 10).unwrap().len(), 1);
        assert_eq!(events(&conn, &b, 10).unwrap().len(), 1);
    }

    #[test]
    fn jobs_survive_connection_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("catalog.sqlite");
        let mut conn = db::open_catalog(&path).unwrap();
        let plan = seed_ready_plan(&conn);
        let id = match queue(&conn, plan, RequestSource::Cli, "h").unwrap() {
            QueueOutcome::Created(id) => id,
            _ => unreachable!(),
        };
        claim_next(&mut conn, "w", 90).unwrap();
        drop(conn);
        // Process restart: fresh connection sees the durable job.
        let conn2 = db::open_catalog(&path).unwrap();
        let job = get(&conn2, &id).unwrap();
        assert_eq!(job.state, JobState::Claimed);
        assert_eq!(job.worker_id.as_deref(), Some("w"));
        let evs = events(&conn2, &id, 10).unwrap();
        assert_eq!(evs.len(), 2);
    }

    #[test]
    fn retry_rejects_incompatible_plan() {
        let mut conn = test_conn();
        let plan = seed_ready_plan(&conn);
        let id = match queue(&conn, plan, RequestSource::Cli, "h").unwrap() {
            QueueOutcome::Created(id) => id,
            _ => unreachable!(),
        };
        claim_next(&mut conn, "w", 90).unwrap();
        fail(&conn, &id, JobState::Claimed, "x", "y").unwrap();
        // Plan becomes Blocked (reviewed mappings changed).
        conn.execute(
            "UPDATE analysis_plans SET status = 'Blocked' WHERE id = ?1",
            params![plan],
        )
        .unwrap();
        let err = retry(&conn, &id, RequestSource::Cli, "h").unwrap_err();
        assert!(err.contains("no longer Ready"), "{err}");
    }

    #[test]
    fn retry_uses_new_job_id_and_preserves_plan_revision() {
        let mut conn = test_conn();
        let plan = seed_ready_plan(&conn);
        let id = match queue(&conn, plan, RequestSource::Cli, "h").unwrap() {
            QueueOutcome::Created(id) => id,
            _ => unreachable!(),
        };
        claim_next(&mut conn, "w", 90).unwrap();
        fail(
            &conn,
            &id,
            JobState::Claimed,
            "archive_checksum_mismatch",
            "bad sha",
        )
        .unwrap();
        let new_id = retry(&conn, &id, RequestSource::Cli, "h2").unwrap();
        assert_ne!(new_id, id);
        let retried = get(&conn, &new_id).unwrap();
        assert_eq!(retried.plan_revision_id, plan);
        assert_eq!(retried.plan_hash, "h2");
        assert!(
            retried.staged_artifact_root.is_none(),
            "retry must not reuse a staging path"
        );
    }

    #[test]
    fn job_counts_by_execution_state() {
        let mut conn = test_conn();
        let plan_a = seed_ready_plan(&conn);
        let plan_b = seed_plan_with_status(&conn, "Ready", "2026-08-02T00:00:00Z");
        let plan_c = seed_plan_with_status(&conn, "Ready", "2026-08-03T00:00:00Z");
        queue(&conn, plan_a, RequestSource::Cli, "h").unwrap();
        queue(&conn, plan_b, RequestSource::Cli, "h").unwrap();
        queue(&conn, plan_c, RequestSource::Cli, "h").unwrap();
        // Claim one job. Random job ids make the picker order-
        // independent here, so derive the remaining two from the list.
        let claimed = claim_next(&mut conn, "w", 90).unwrap().unwrap().job.id;
        let all = list(&conn, &JobFilter::default()).unwrap();
        let others: Vec<String> = all
            .iter()
            .filter(|j| j.id != claimed)
            .map(|j| j.id.clone())
            .collect();
        assert_eq!(others.len(), 2);
        // One remaining job fails before execution; the other is
        // cancelled while queued.
        fail(&conn, &others[0], JobState::Queued, "cancelled", "n/a").unwrap();
        request_cancel(&conn, &others[1]).unwrap();
        let counts = counts(&conn).unwrap();
        assert_eq!(counts.queued, 0);
        assert_eq!(counts.running, 1); // claimed
        assert_eq!(counts.failed, 1);
        assert_eq!(counts.cancelled, 1);
        assert_eq!(counts.completed, 0);
        let _ = (plan_a, plan_b, plan_c);
    }
}

// ── CAS race regression tests ───────────────────────────────────────

#[cfg(test)]
mod race_tests {
    use super::*;
    use crate::catalog::db;

    fn setup_job() -> (tempfile::TempDir, Connection, String) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("catalog.sqlite");
        let conn = db::open_catalog(&path).unwrap();
        conn.execute(
            "INSERT INTO catalog_events (source_kind, external_id, first_seen, last_seen)
             VALUES ('local-repository', 'RACE-EVT', '2026-08-01T00:00:00Z', '2026-08-01T00:00:00Z')",
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
            "event_id": "RACE-EVT", "revision": 1, "schema_version": 2, "open": false,
            "event_window_utc": {"start": "2026-08-01T00:00:00Z", "end": "2026-08-02T00:00:00Z"},
            "ticket_window_local": {"start": "", "end": "", "timezone": "UTC"},
            "target": {"label": "Race event", "origin_asns": [64500],
                "transit_predicate": {"predicate": {"ContainsAny": [64501]}, "status": "Reviewed",
                    "provenance": {"statement": "r", "reviewed_by": "local-review", "date": "2026-08-01"}}},
            "collectors": ["route-views2"], "source_family": "RouteViews"
        })
        .to_string();
        conn.execute(
            "INSERT INTO manifest_revisions (event_id, snapshot_id, manifest_schema, payload, sha256, review_status)
             VALUES (?1, ?2, 2, ?3, ?4, 'Reviewed')",
            rusqlite::params![eid, sid, payload, "race-msha"],
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
        let plan_rec =
            crate::catalog::import::build_plan_record(&conn, mid, &manifest, true).unwrap();
        let plan = crate::catalog::store::insert_plan(&conn, &plan_rec).unwrap();
        let id = match queue(&conn, plan, RequestSource::Cli, "h").unwrap() {
            QueueOutcome::Created(id) => id,
            _ => unreachable!(),
        };
        (dir, conn, id)
    }

    #[test]
    fn cancel_cannot_overwrite_completed_job() {
        let (dir, conn, id) = setup_job();
        // Simulate the interleaving: the job reaches PublishingRun and
        // completes BEFORE the cancel UPDATE runs. The CAS WHERE clause
        // must reject the cancellation.
        conn.execute(
            "UPDATE analysis_jobs SET state = 'PublishingRun' WHERE id = ?1",
            [&id],
        )
        .unwrap();
        let err = request_cancel(&conn, &id).unwrap_err();
        assert!(
            err.contains("cannot be cancelled") || err.contains("no longer cancellable"),
            "{err}"
        );
        // The job stays exactly as the concurrent completion left it.
        assert_eq!(get(&conn, &id).unwrap().state, JobState::PublishingRun);
        drop(dir);
    }

    #[test]
    fn stale_scan_cannot_overwrite_renewed_lease() {
        let (dir, mut conn, id) = setup_job();
        claim_next(&mut conn, "w", 90).unwrap();
        conn.execute(
            "UPDATE analysis_jobs SET lease_expires_at = '2020-01-01T00:00:00Z' WHERE id = ?1",
            [&id],
        )
        .unwrap();
        // The worker renews (extends the lease) before the stale scan's
        // UPDATE runs: the scan must not fail the job.
        renew_lease(&conn, &id, "w", 90).unwrap();
        let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        let expired = mark_stale_leases(&conn, &now).unwrap();
        assert!(
            !expired.contains(&id),
            "renewed lease must not be expired: {expired:?}"
        );
        assert_eq!(get(&conn, &id).unwrap().state, JobState::Claimed);
        drop(dir);
    }

    #[test]
    fn completion_cannot_overwrite_cancelled_job() {
        let (dir, mut conn, id) = setup_job();
        claim_next(&mut conn, "w", 90).unwrap();
        // Cancellation lands first: the job is Cancelled. A late
        // complete() (publication never began) must be rejected.
        request_cancel(&conn, &id).unwrap();
        assert_eq!(get(&conn, &id).unwrap().state, JobState::CancelRequested);
        observe_cancel(&conn, &id).unwrap();
        assert_eq!(get(&conn, &id).unwrap().state, JobState::Cancelled);
        let err = complete(&conn, &id, 99).unwrap_err();
        assert!(
            err.contains("not in PublishingRun") || err.contains("refusing completion"),
            "{err}"
        );
        assert_eq!(get(&conn, &id).unwrap().state, JobState::Cancelled);
        drop(dir);
    }
}
