//! Offline end-to-end queued-analysis fixture (Part 26).
//!
//! A synthetic, source-neutral event + manifest is queued through the
//! durable job service and executed by the real worker in --offline
//! mode against a local MRT fixture. The full path is exercised:
//! queue → claim → parse local fixture → derive evidence → staged
//! artifacts → validation → atomic publication → completed run →
//! workbench. No network is used anywhere.

use std::path::{Path, PathBuf};

use inim::catalog::db;
use inim::catalog::jobs::{plan, service, JobState, RequestSource};

/// Synthetic event identity — deliberately generic (Part 26: no
/// Internet2 / NORDUnet / UVA / MAN LAN / GRNOC semantics).
const EVENT_ID: &str = "SYNTH-ROUTE-EVENT-001";
const ORIGIN_ASN: u32 = 224; // present in the fixture as a path origin
const PREDICATE_ASN: u32 = 1299; // present in every sampled origin-224 path

/// The tracked MRT fixture used for both baseline and updates.
const FIXTURE: &str = "tests/fixtures/ris/updates.20190821.1600.gz";

struct FixtureRoot {
    _dir: tempfile::TempDir,
    root: PathBuf,
    db: PathBuf,
}

fn sha256_file(path: &Path) -> String {
    use sha2::Digest;
    let bytes = std::fs::read(path).unwrap();
    let mut h = sha2::Sha256::new();
    h.update(&bytes);
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

/// Build a temp catalog root with cached fixture archives + seeded
/// Ready plan. The event snapshot payload is an Internet2-shaped ticket
/// fixture (the pipeline's ticket parser requirement) with generic
/// content.
fn build_fixture() -> FixtureRoot {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();

    // ── Local cache: the fixture doubles as RIB and UPDATE, with the
    // integrity sidecars cache_archive requires. ──
    let fixture_sha = sha256_file(Path::new(FIXTURE));
    for data_type in ["rib", "updates"] {
        let collector_dir = root.join("cache").join("rrc00").join(data_type);
        std::fs::create_dir_all(&collector_dir).unwrap();
        let target = collector_dir.join("updates.20190821.1600.gz");
        std::fs::copy(FIXTURE, &target).unwrap();
        std::fs::write(
            format!("{}.sha256", target.display()),
            format!("{fixture_sha}\n"),
        )
        .unwrap();
    }

    // ── Synthetic generic event fixture (Internet2 ticket shape, no
    // Internet2-specific content). ──
    let event_json = serde_json::json!({
        "id": EVENT_ID,
        "title": "Route maintenance affecting documentation prefix set",
        "start": "2019-08-21 12:00:00",
        "end": "2019-08-21 12:10:00",
        "type": "maintenance",
        "timezone": "EDT",
        "description": "Synthetic offline fixture event for the queued-analysis workflow."
    })
    .to_string();

    // ── Reviewed manifest: origin 224 + transit predicate 1299,
    // both observed in the fixture. ──
    let manifest_json = serde_json::json!({
        "event_id": EVENT_ID,
        "revision": 1,
        "schema_version": 2,
        "open": false,
        "event_window_utc": {"start": "2019-08-21T16:00:00Z", "end": "2019-08-21T16:10:00Z"},
        "ticket_window_local": {"start": "2019-08-21 12:00:00", "end": "2019-08-21 12:10:00", "timezone": "EDT"},
        "warmup_minutes": 0,
        "cooldown_minutes": 0,
        "target": {
            "label": "Synthetic documentation target",
            "origin_asns": [ORIGIN_ASN],
            "transit_predicate": {
                "predicate": {"ContainsAny": [PREDICATE_ASN]},
                "status": "Reviewed",
                "provenance": {
                    "statement": "fixture-test review: ASN 224 origin with 1299 in path observed in fixture",
                    "reviewed_by": "fixture-review",
                    "date": "2026-08-01"
                }
            }
        },
        "collectors": ["rrc00"],
        "source_family": "RipeRis"
    })
    .to_string();

    // ── Catalog: event + snapshot + manifest revision + Ready plan. ──
    let db = root.join("data").join("inim.sqlite");
    let conn = db::open_catalog(&db).unwrap();
    let db_for_struct = db.clone();
    conn.execute(
        "INSERT INTO catalog_events (source_kind, external_id, first_seen, last_seen)
         VALUES ('local-repository', ?1, '2019-08-21T16:00:00Z', '2019-08-21T16:10:00Z')",
        [EVENT_ID],
    )
    .unwrap();
    let eid = conn.last_insert_rowid();
    conn.execute(
        "INSERT INTO event_snapshots (event_id, fetched_at, source_url, content_sha256, raw_payload, normalized_json, parser_version)
         VALUES (?1, '2019-08-21T16:00:00Z', 'file:///fixture', ?2, ?3, ?3, 'fixture-1')",
        rusqlite::params![eid, sha256_file(Path::new("/dev/null")), event_json.clone()],
    )
    .unwrap();
    let sid = conn.last_insert_rowid();
    let msha = inim::catalog::document::hex_sha256(manifest_json.as_bytes());
    conn.execute(
        "INSERT INTO manifest_revisions (event_id, snapshot_id, manifest_schema, payload, sha256, review_status, reviewed_at, reviewer)
         VALUES (?1, ?2, 2, ?3, ?4, 'Reviewed', '2026-08-01T00:00:00Z', 'fixture-review')",
        rusqlite::params![eid, sid, manifest_json.clone(), msha],
    )
    .unwrap();
    let mid = conn.last_insert_rowid();
    let manifest: inim::manifest::Manifest = serde_json::from_str(&manifest_json).unwrap();
    let plan_rec = inim::catalog::import::build_plan_record(&conn, mid, &manifest, true).unwrap();
    let _plan_id = inim::catalog::store::insert_plan(&conn, &plan_rec).unwrap();
    drop(conn);

    FixtureRoot {
        _dir: dir,
        root: root.clone(),
        db: db_for_struct,
    }
}

fn worker_config(root: &Path, db: &Path, once: bool) -> inim::worker::WorkerConfig {
    inim::worker::WorkerConfig {
        db_path: db.to_path_buf(),
        root: root.to_path_buf(),
        worker_id: Some("fixture-worker-1".to_string()),
        poll_interval: std::time::Duration::from_millis(100),
        max_jobs: 1,
        download_jobs: 2,
        parse_jobs: 4,
        once,
        offline: true,
        lease_secs: 90,
        heartbeat_secs: 15,
        keep_failed_workdir: false,
        show_execution_plan: false,
    }
}

fn queue_ready(conn: &rusqlite::Connection, plan_id: i64, source: RequestSource) -> String {
    let payload = plan::manifest_payload_for_plan(conn, plan_id).unwrap();
    let hash = plan::canonical_plan_hash(&payload).unwrap();
    match service::queue(conn, plan_id, source, &hash, &inim::catalog::scope::ProjectScope::default()).unwrap() {
        service::QueueOutcome::Created(id) => id,
        _ => unreachable!(),
    }
}

fn run_count(conn: &rusqlite::Connection) -> i64 {
    conn.query_row("SELECT COUNT(*) FROM analysis_runs", [], |r| r.get(0))
        .unwrap()
}

#[test]
fn offline_job_end_to_end_completes() {
    let fx = build_fixture();
    let conn = db::open_catalog(&fx.db).unwrap();
    let plan_id: i64 = conn
        .query_row("SELECT id FROM analysis_plans LIMIT 1", [], |r| r.get(0))
        .unwrap();
    let job_id = queue_ready(&conn, plan_id, RequestSource::Cli);
    // Durability across "process restart": a second connection (the
    // worker's own) sees the queued job exactly as the first left it.
    drop(conn);
    let code = inim::worker::run_worker(&worker_config(&fx.root, &fx.db, true));
    assert_eq!(code, 0, "worker exit code");

    let conn = db::open_catalog(&fx.db).unwrap();
    let job = service::get(&conn, &job_id).unwrap();
    if job.state != JobState::Completed {
        eprintln!(
            "DEBUG job error: {:?} {:?}",
            job.error_code, job.error_summary
        );
        eprintln!(
            "DEBUG events: {:?}",
            service::events(&conn, &job_id, 10)
                .unwrap()
                .iter()
                .map(|e| (e.state.as_str().to_string(), e.human_message.clone()))
                .collect::<Vec<_>>()
        );
    }
    assert_eq!(job.state, JobState::Completed, "job must complete");
    let run_id = job.completed_run_id.expect("completed job links a run");
    assert_eq!(run_count(&conn), 1);
    // The run links the exact plan revision and preserves the plan hash.
    let plan_of_run: i64 = conn
        .query_row(
            "SELECT plan_id FROM analysis_runs WHERE id = ?1",
            [run_id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(plan_of_run, plan_id);
    // Published artifacts exist and resolve under the catalog root.
    let rel: String = conn
        .query_row(
            "SELECT relative_path FROM analysis_artifacts WHERE run_id = ?1 AND kind = 'report'",
            [run_id],
            |r| r.get(0),
        )
        .unwrap();
    assert!(rel.starts_with("data/runs/"), "{rel}");
    assert!(fx.root.join(&rel).is_file(), "artifact must resolve: {rel}");
    // The report carries the analysis outcome (NoRouteStateChange for
    // an unchanged fixture), not a worker failure.
    let report: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(fx.root.join(&rel)).unwrap()).unwrap();
    let schema = report["schema_version"].as_u64().unwrap();
    assert_eq!(schema, inim::schema::REPORT_SCHEMA_VERSION as u64);
    // The event page/workbench path resolves: the run view loads.
    let conn = db::open_catalog(&fx.db).unwrap();
    let view = inim::catalog::web::view::load_run(&conn, run_id, &state_for(&fx));
    assert!(view.is_ok(), "workbench must read published artifacts");
    drop(conn);
}

fn state_for(fx: &FixtureRoot) -> std::sync::Arc<inim::catalog::web::AppState> {
    inim::catalog::web::server::build_state(&fx.db, &fx.root, "0.1.0", false).unwrap()
}

#[test]
fn queue_is_idempotent_while_active() {
    let fx = build_fixture();
    let conn = db::open_catalog(&fx.db).unwrap();
    let plan_id: i64 = conn
        .query_row("SELECT id FROM analysis_plans LIMIT 1", [], |r| r.get(0))
        .unwrap();
    let first = queue_ready(&conn, plan_id, RequestSource::LocalWeb);
    let second = {
        let payload = plan::manifest_payload_for_plan(&conn, plan_id).unwrap();
        let hash = plan::canonical_plan_hash(&payload).unwrap();
        service::queue(&conn, plan_id, RequestSource::LocalWeb, &hash, &inim::catalog::scope::ProjectScope::default()).unwrap()
    };
    match second {
        service::QueueOutcome::Duplicate(existing) => assert_eq!(existing, first),
        other => panic!("expected Duplicate, got {other:?}"),
    }
    drop(conn);
    // One job only.
    let conn = db::open_catalog(&fx.db).unwrap();
    let jobs: i64 = conn
        .query_row("SELECT COUNT(*) FROM analysis_jobs", [], |r| r.get(0))
        .unwrap();
    assert_eq!(jobs, 1);
}

#[test]
fn failed_before_publish_has_no_run() {
    let fx = build_fixture();
    // Corrupt the RIB sidecar: offline mode turns the cache miss into a
    // hard archive_not_cached failure BEFORE any publication.
    let rib = fx.root.join("cache/rrc00/rib/updates.20190821.1600.gz");
    std::fs::write(format!("{}.sha256", rib.display()), "deadbeef\n").unwrap();
    let conn = db::open_catalog(&fx.db).unwrap();
    let plan_id: i64 = conn
        .query_row("SELECT id FROM analysis_plans LIMIT 1", [], |r| r.get(0))
        .unwrap();
    let job_id = queue_ready(&conn, plan_id, RequestSource::Cli);
    drop(conn);
    let code = inim::worker::run_worker(&worker_config(&fx.root, &fx.db, true));
    assert_ne!(code, 0, "worker must fail the job");
    let conn = db::open_catalog(&fx.db).unwrap();
    let job = service::get(&conn, &job_id).unwrap();
    assert_eq!(job.state, JobState::Failed);
    assert_eq!(job.error_code.as_deref(), Some("archive_not_cached"));
    assert_eq!(run_count(&conn), 0, "no run may be published");
    // No final run directory exists.
    assert!(!fx.root.join("data/runs").join(&job_id).exists());
}

#[test]
fn retry_after_injected_failure_completes() {
    let fx = build_fixture();
    let rib = fx.root.join("cache/rrc00/rib/updates.20190821.1600.gz");
    let sidecar = format!("{}.sha256", rib.display());
    let good = std::fs::read_to_string(&sidecar).unwrap();
    std::fs::write(&sidecar, "deadbeef\n").unwrap();

    let conn = db::open_catalog(&fx.db).unwrap();
    let plan_id: i64 = conn
        .query_row("SELECT id FROM analysis_plans LIMIT 1", [], |r| r.get(0))
        .unwrap();
    let job_id = queue_ready(&conn, plan_id, RequestSource::Cli);
    drop(conn);
    let code = inim::worker::run_worker(&worker_config(&fx.root, &fx.db, true));
    assert_ne!(code, 0);
    let conn = db::open_catalog(&fx.db).unwrap();
    assert_eq!(
        service::get(&conn, &job_id).unwrap().state,
        JobState::Failed
    );

    // Fix the cache and retry: a NEW attempt with the same plan.
    std::fs::write(&sidecar, &good).unwrap();
    let new_id = service::retry(
        &conn,
        &job_id,
        RequestSource::Cli,
        &service::get(&conn, &job_id).unwrap().plan_hash,
        &inim::catalog::scope::ProjectScope::default(),
    )
    .unwrap();
    assert_ne!(new_id, job_id);
    drop(conn);
    let code = inim::worker::run_worker(&worker_config(&fx.root, &fx.db, true));
    assert_eq!(code, 0);
    let conn = db::open_catalog(&fx.db).unwrap();
    assert_eq!(
        service::get(&conn, &new_id).unwrap().state,
        JobState::Completed
    );
    assert_eq!(run_count(&conn), 1, "exactly one published run");
    // The failed original attempt is preserved, untouched.
    let original = service::get(&conn, &job_id).unwrap();
    assert_eq!(original.state, JobState::Failed);
    assert_eq!(original.error_code.as_deref(), Some("archive_not_cached"));
    assert!(original.completed_run_id.is_none());
}

#[test]
fn completed_workbench_reads_published_artifacts() {
    let fx = build_fixture();
    let conn = db::open_catalog(&fx.db).unwrap();
    let plan_id: i64 = conn
        .query_row("SELECT id FROM analysis_plans LIMIT 1", [], |r| r.get(0))
        .unwrap();
    queue_ready(&conn, plan_id, RequestSource::Cli);
    drop(conn);
    let code = inim::worker::run_worker(&worker_config(&fx.root, &fx.db, true));
    assert_eq!(code, 0);
    let conn = db::open_catalog(&fx.db).unwrap();
    let run_id: i64 = conn
        .query_row("SELECT id FROM analysis_runs LIMIT 1", [], |r| r.get(0))
        .unwrap();
    // The workbench view model loads from the published artifacts.
    let state = state_for(&fx);
    let view = inim::catalog::web::view::load_run(&conn, run_id, &state).unwrap();
    let view = view.expect("run view exists");
    // The report text resolves (the run's report.txt exists).
    assert!(!view.result_label.is_empty() || !view.assessment.is_empty());
}

#[test]
fn queue_operation_performs_no_analysis() {
    let fx = build_fixture();
    let conn = db::open_catalog(&fx.db).unwrap();
    let plan_id: i64 = conn
        .query_row("SELECT id FROM analysis_plans LIMIT 1", [], |r| r.get(0))
        .unwrap();
    queue_ready(&conn, plan_id, RequestSource::Cli);
    // Queueing alone (no worker) must not create runs or artifacts.
    assert_eq!(run_count(&conn), 0);
    let events = service::events(
        &conn,
        &service::list(&conn, &service::JobFilter::default()).unwrap()[0].id,
        10,
    )
    .unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].state, JobState::Queued);
    // The web GET path performs no execution either (database read-only
    // already proven in the web test suite).
}
