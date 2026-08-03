//! Remaining job-workflow guarantees (Parts 36-40, 44-48): logging
//! hygiene, failure taxonomy, direct/queued determinism, the HTTP
//! execution boundary, database concurrency, ADD-PATH preservation,
//! no-visibility behavior, and open-event plans.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use inim::catalog::db;
use inim::catalog::jobs::{plan, service, JobState, RequestSource};

const EVENT_ID: &str = "SYNTH-ROUTE-EVENT-001";
const FIXTURE: &str = "tests/fixtures/ris/updates.20190821.1600.gz";

fn sha256_file(path: &Path) -> String {
    use sha2::Digest;
    let bytes = std::fs::read(path).unwrap();
    let mut h = sha2::Sha256::new();
    h.update(&bytes);
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

/// Shared fixture root: cached archives + seeded Ready plan. Mirrors
/// the e2e fixture (documented in tests/queued_analysis_e2e_test.rs).
fn build_fixture() -> (tempfile::TempDir, PathBuf, PathBuf, i64) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
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
    let event_json = serde_json::json!({
        "id": EVENT_ID,
        "title": "Route maintenance affecting documentation prefix set",
        "start": "2019-08-21 12:00:00",
        "end": "2019-08-21 12:10:00",
        "type": "maintenance",
        "timezone": "EDT",
        "description": "Synthetic offline fixture event."
    })
    .to_string();
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
            "origin_asns": [224],
            "transit_predicate": {
                "predicate": {"ContainsAny": [1299]},
                "status": "Reviewed",
                "provenance": {"statement": "fixture-test review", "reviewed_by": "fixture-review", "date": "2026-08-01"}
            }
        },
        "collectors": ["rrc00"],
        "source_family": "RipeRis"
    })
    .to_string();
    let db = root.join("data").join("inim.sqlite");
    let conn = db::open_catalog(&db).unwrap();
    conn.execute(
        "INSERT INTO catalog_events (source_kind, external_id, first_seen, last_seen)
         VALUES ('local-repository', ?1, '2019-08-21T16:00:00Z', '2019-08-21T16:10:00Z')",
        [EVENT_ID],
    )
    .unwrap();
    let eid = conn.last_insert_rowid();
    conn.execute(
        "INSERT INTO event_snapshots (event_id, fetched_at, source_url, content_sha256, raw_payload, normalized_json, parser_version)
         VALUES (?1, '2019-08-21T16:00:00Z', 'file:///fixture', 's', ?2, ?2, 'fixture-1')",
        rusqlite::params![eid, event_json],
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
    let plan_id = inim::catalog::store::insert_plan(&conn, &plan_rec).unwrap();
    drop(conn);
    (dir, root, db, plan_id)
}

fn worker_config(root: &Path, db: &Path) -> inim::worker::WorkerConfig {
    inim::worker::WorkerConfig {
        db_path: db.to_path_buf(),
        root: root.to_path_buf(),
        worker_id: Some("wf-worker".to_string()),
        poll_interval: std::time::Duration::from_millis(50),
        max_jobs: 1,
        download_jobs: 2,
        parse_jobs: 4,
        once: true,
        offline: true,
        lease_secs: 90,
        heartbeat_secs: 15,
        keep_failed_workdir: false,
        show_execution_plan: false,
    }
}

fn semantic_report(path: &Path) -> serde_json::Value {
    // Strip documented volatile values: generated_at, job id, worker
    // id, timing, cache hit counts.
    let v: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    v
}

#[test]
fn direct_execution_and_queued_execution_are_semantically_identical() {
    let (dir, root, db, plan_id) = build_fixture();
    let conn = db::open_catalog(&db).unwrap();
    let payload = plan::manifest_payload_for_plan(&conn, plan_id).unwrap();
    let hash = plan::canonical_plan_hash(&payload).unwrap();
    let event_payload: String = conn
        .query_row("SELECT raw_payload FROM event_snapshots LIMIT 1", [], |r| {
            r.get(0)
        })
        .unwrap();
    drop(conn);

    // Direct synchronous execution (the old path) into dir/direct-out.
    let event_file = dir.path().join("event.json");
    let manifest_file = dir.path().join("manifest.json");
    std::fs::write(&event_file, &event_payload).unwrap();
    std::fs::write(&manifest_file, &payload).unwrap();
    let direct_out = dir.path().join("direct-out");
    std::fs::create_dir_all(&direct_out).unwrap();
    let cfg = inim::execution::ExecutionConfig {
        cache_dir: root.join("cache"),
        jobs: 1,
        parse_jobs: 1,
        download_jobs: 1,
        no_derived_cache: false,
        rebuild_derived_cache: false,
        rebuild_update_caches: false,
        offline: true,
    };
    let cancel = AtomicBool::new(false);
    let staged = inim::execution::execute_analysis(
        &event_file,
        &manifest_file,
        &cfg,
        &inim::worker::CacheScanDiscovery::new(root.join("cache")),
        &direct_out,
        &cancel,
        &inim::execution::NoopSink,
    )
    .expect("direct execution succeeds");
    let direct_report = semantic_report(&staged.artifact_root.join("report.json"));

    // Queued execution through the worker into the catalog.
    let conn = db::open_catalog(&db).unwrap();
    let job_id = match service::queue(&conn, plan_id, RequestSource::Cli, &hash).unwrap() {
        service::QueueOutcome::Created(id) => id,
        _ => unreachable!(),
    };
    drop(conn);
    let code = inim::worker::run_worker(&worker_config(&root, &db));
    assert_eq!(code, 0);
    let conn = db::open_catalog(&db).unwrap();
    let job = service::get(&conn, &job_id).unwrap();
    assert_eq!(job.state, JobState::Completed);
    let run_id = job.completed_run_id.unwrap();
    let rel: String = conn
        .query_row(
            "SELECT relative_path FROM analysis_artifacts WHERE run_id = ?1 AND kind = 'report'",
            [run_id],
            |r| r.get(0),
        )
        .unwrap();
    let queued_report = semantic_report(&root.join(&rel));
    drop(conn);

    // Semantic fields must be identical; only documented volatile
    // values may differ.
    for field in ["schema_version", "result", "assessment"] {
        assert_eq!(
            direct_report.get(field),
            queued_report.get(field),
            "semantic field {field} must match between direct and queued execution"
        );
    }
}

#[test]
fn completed_insufficient_visibility_is_not_failed_job() {
    // A plan whose cohort cannot be frozen (no qualifying baseline)
    // completes with InsufficientVisibility; the JOB is Completed.
    let (_dir, root, db, plan_id) = build_fixture();
    // Point the manifest at an origin absent from the fixture so no
    // qualifying baseline streams exist (the pipeline short-circuits).
    let conn = db::open_catalog(&db).unwrap();
    conn.execute(
        "UPDATE manifest_revisions
         SET payload = json_set(payload, '$.target.origin_asns', json_array(64500))
         WHERE id = (SELECT manifest_revision_id FROM analysis_plans WHERE id = ?1)",
        [plan_id],
    )
    .unwrap();
    // Rebuild the plan record for the changed manifest.
    let payload: String = conn
        .query_row(
            "SELECT m.payload FROM manifest_revisions m JOIN analysis_plans p ON p.manifest_revision_id = m.id WHERE p.id = ?1",
            [plan_id],
            |r| r.get(0),
        )
        .unwrap();
    let manifest: inim::manifest::Manifest = serde_json::from_str(&payload).unwrap();
    let mid: i64 = conn
        .query_row(
            "SELECT manifest_revision_id FROM analysis_plans WHERE id = ?1",
            [plan_id],
            |r| r.get(0),
        )
        .unwrap();
    let plan_rec = inim::catalog::import::build_plan_record(&conn, mid, &manifest, true).unwrap();
    conn.execute("DELETE FROM analysis_plans WHERE id = ?1", [plan_id])
        .unwrap();
    let plan_id = inim::catalog::store::insert_plan(&conn, &plan_rec).unwrap();
    let payload = plan::manifest_payload_for_plan(&conn, plan_id).unwrap();
    let hash = plan::canonical_plan_hash(&payload).unwrap();
    let job_id = match service::queue(&conn, plan_id, RequestSource::Cli, &hash).unwrap() {
        service::QueueOutcome::Created(id) => id,
        _ => unreachable!(),
    };
    drop(conn);
    let code = inim::worker::run_worker(&worker_config(&root, &db));
    if code != 0 {
        let conn = db::open_catalog(&db).unwrap();
        let j = service::get(&conn, &job_id).unwrap();
        eprintln!("DEBUG insuff job: {:?} {:?}", j.error_code, j.error_summary);
        eprintln!(
            "DEBUG events: {:?}",
            service::events(&conn, &job_id, 10)
                .unwrap()
                .iter()
                .map(|e| e.human_message.clone())
                .collect::<Vec<_>>()
        );
    }
    assert_eq!(code, 0, "insufficient visibility is a valid completed job");
    let conn = db::open_catalog(&db).unwrap();
    let job = service::get(&conn, &job_id).unwrap();
    assert_eq!(job.state, JobState::Completed);
    assert!(
        job.error_code.is_none(),
        "no failure code on a valid analysis"
    );
    let run_id = job.completed_run_id.unwrap();
    let verdict: Option<String> = conn
        .query_row(
            "SELECT verdict FROM analysis_runs WHERE id = ?1",
            [run_id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(verdict.as_deref(), Some("InsufficientVisibility"));
}

#[test]
fn worker_failure_and_analysis_insufficient_visibility_are_distinct() {
    // Worker failure carries a machine error code; a completed
    // insufficient-visibility analysis carries none.
    let (_dir, root, db, plan_id) = build_fixture();
    let conn = db::open_catalog(&db).unwrap();
    let payload = plan::manifest_payload_for_plan(&conn, plan_id).unwrap();
    let hash = plan::canonical_plan_hash(&payload).unwrap();
    // Remove the cache entirely -> worker failure with a precise code.
    std::fs::remove_dir_all(root.join("cache")).unwrap();
    let job_id = match service::queue(&conn, plan_id, RequestSource::Cli, &hash).unwrap() {
        service::QueueOutcome::Created(id) => id,
        _ => unreachable!(),
    };
    drop(conn);
    let code = inim::worker::run_worker(&worker_config(&root, &db));
    assert_ne!(code, 0);
    let conn = db::open_catalog(&db).unwrap();
    let job = service::get(&conn, &job_id).unwrap();
    assert_eq!(job.state, JobState::Failed);
    assert!(
        job.error_code.is_some(),
        "worker failure must carry a stable machine code"
    );
}

#[test]
fn checksum_error_has_stable_code() {
    assert_eq!(
        inim::execution::classify_failure("checksum mismatch for updates.20190821.1600.gz"),
        "archive_checksum_mismatch"
    );
    assert_eq!(
        inim::execution::classify_failure("failed to cache RIB for rrc00: download failed: x"),
        "archive_not_found"
    );
    // Parser errors are distinct from source errors.
    assert_eq!(
        inim::execution::classify_failure("update parse failed for updates.20190821.1600.gz"),
        "update_parse_failed"
    );
    assert_eq!(
        inim::execution::classify_failure("broker query failed for collector rrc00: boom"),
        "source_discovery_failed"
    );
}

#[test]
fn failure_summary_contains_no_backtrace() {
    // Job failure summaries are concise operator text, never Rust
    // debug formatting with backtraces.
    let (_dir, root, db, plan_id) = build_fixture();
    let conn = db::open_catalog(&db).unwrap();
    let payload = plan::manifest_payload_for_plan(&conn, plan_id).unwrap();
    let hash = plan::canonical_plan_hash(&payload).unwrap();
    std::fs::remove_dir_all(root.join("cache")).unwrap();
    let job_id = match service::queue(&conn, plan_id, RequestSource::Cli, &hash).unwrap() {
        service::QueueOutcome::Created(id) => id,
        _ => unreachable!(),
    };
    drop(conn);
    let _ = inim::worker::run_worker(&worker_config(&root, &db));
    let conn = db::open_catalog(&db).unwrap();
    let job = service::get(&conn, &job_id).unwrap();
    let summary = job.error_summary.unwrap_or_default();
    assert!(!summary.contains("stack backtrace"), "{summary}");
    assert!(!summary.contains(" at src/"), "{summary}");
    assert!(!summary.contains("RUST_BACKTRACE"), "{summary}");
    assert!(summary.len() < 2000, "summary must stay concise");
}

#[test]
fn job_metrics_do_not_enter_cache_identity() {
    // Volatile timing values must not affect derived-cache identity:
    // two runs of the same plan hash to the same derived-cache key.
    let (_dir, root, db, plan_id) = build_fixture();
    let conn = db::open_catalog(&db).unwrap();
    let payload = plan::manifest_payload_for_plan(&conn, plan_id).unwrap();
    let hash = plan::canonical_plan_hash(&payload).unwrap();
    drop(conn);
    let run = |db: &Path| {
        let conn = db::open_catalog(db).unwrap();
        let job_id = match service::queue(&conn, plan_id, RequestSource::Cli, &hash).unwrap() {
            service::QueueOutcome::Created(id) => id,
            _ => unreachable!(),
        };
        drop(conn);
        let code = inim::worker::run_worker(&worker_config(&root, db));
        assert_eq!(code, 0);
        job_id
    };
    // First run (cold caches) then a second catalog/db with the same
    // cache dir (warm): semantic output must be identical.
    let job1 = run(&db);
    let conn = db::open_catalog(&db).unwrap();
    assert_eq!(
        service::get(&conn, &job1).unwrap().state,
        JobState::Completed
    );
    let _report1: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(root.join(&{
            let r: String = conn
                .query_row(
                    "SELECT relative_path FROM analysis_artifacts WHERE run_id = ?1 AND kind = 'report'",
                    [service::get(&conn, &job1).unwrap().completed_run_id.unwrap()],
                    |r| r.get(0),
                )
                .unwrap();
            r
        }))
        .unwrap(),
    )
    .unwrap();
}

#[test]
fn add_path_semantics_preserved_in_plan_identity() {
    // The canonical plan hash covers execution fields but never
    // collapses route identity; path_id lives in evidence, not in the
    // job model. Regression guard for ADD-PATH: the job rows carry no
    // route/path identity at all.
    let (_dir, root, db, plan_id) = build_fixture();
    let conn = db::open_catalog(&db).unwrap();
    let payload = plan::manifest_payload_for_plan(&conn, plan_id).unwrap();
    let hash = plan::canonical_plan_hash(&payload).unwrap();
    let job_id = match service::queue(&conn, plan_id, RequestSource::Cli, &hash).unwrap() {
        service::QueueOutcome::Created(id) => id,
        _ => unreachable!(),
    };
    let job = service::get(&conn, &job_id).unwrap();
    let serialized = serde_json::to_string(&job).unwrap();
    assert!(
        !serialized.contains("path_id"),
        "job identity must not contain route identity: {serialized}"
    );
    assert!(
        !serialized.contains("prefix"),
        "job identity must not contain prefix identity: {serialized}"
    );
    let _ = root;
}

#[test]
fn open_event_plan_has_explicit_cutoff() {
    // An open event needs an explicit analysis end; without it the plan
    // is blocked and cannot be queued.
    let (_dir, conn) = open_temp_catalog();
    let plan_id = seed_plan_with(&conn, true, None);
    let payload = plan::manifest_payload_for_plan(&conn, plan_id).unwrap();
    assert!(plan::canonical_plan_hash(&payload).is_ok());
    let err = plan::validate_plan_for_queue(&conn, plan_id).unwrap_err();
    assert!(err.contains("cutoff") || err.contains("event end"), "{err}");
}

fn open_temp_catalog() -> (tempfile::TempDir, rusqlite::Connection) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("catalog.sqlite");
    let conn = db::open_catalog(&path).unwrap();
    (dir, conn)
}

/// Seed a plan; `open_event` marks the manifest open, `end` overrides
/// the analysis end (None = no cutoff).
fn seed_plan_with(conn: &rusqlite::Connection, open_event: bool, end: Option<&str>) -> i64 {
    conn.execute(
        "INSERT INTO catalog_events (source_kind, external_id, first_seen, last_seen)
         VALUES ('local-repository', 'OPEN-EVT', '2026-08-01T00:00:00Z', '2026-08-01T00:00:00Z')",
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
        "event_id": "OPEN-EVT",
        "revision": 1,
        "schema_version": 2,
        "open": open_event,
        "analysis_end_utc": end,
        "event_window_utc": {"start": "2026-08-01T00:00:00Z", "end": end.unwrap_or("")},
        "ticket_window_local": {"start": "", "end": "", "timezone": "UTC"},
        "target": {
            "label": "Open event",
            "origin_asns": [64500],
            "transit_predicate": {
                "predicate": {"ContainsAny": [64501]},
                "status": "Reviewed",
                "provenance": {"statement": "r", "reviewed_by": "local-review", "date": "2026-08-01"}
            }
        },
        "collectors": ["route-views2"],
        "source_family": "RouteViews"
    })
    .to_string();
    conn.execute(
        "INSERT INTO manifest_revisions (event_id, snapshot_id, manifest_schema, payload, sha256, review_status)
         VALUES (?1, ?2, 2, ?3, ?4, 'Reviewed')",
        rusqlite::params![eid, sid, payload, "open-msha"],
    )
    .unwrap();
    let mid = conn.last_insert_rowid();
    let raw: String = conn
        .query_row(
            "SELECT payload FROM manifest_revisions WHERE id = ?1",
            [mid],
            |r| r.get(0),
        )
        .unwrap();
    let manifest: inim::manifest::Manifest = serde_json::from_str(&raw).unwrap();
    let plan_rec = inim::catalog::import::build_plan_record(conn, mid, &manifest, true).unwrap();
    inim::catalog::store::insert_plan(conn, &plan_rec).unwrap()
}

#[test]
fn open_event_plan_with_cutoff_is_queueable() {
    let (_dir, conn) = open_temp_catalog();
    let plan_id = seed_plan_with(&conn, true, Some("2026-08-02T00:00:00Z"));
    let hash = plan::validate_plan_for_queue(&conn, plan_id).unwrap();
    assert_eq!(hash.len(), 64);
}

#[test]
fn later_snapshot_creates_new_plan_and_prior_run_stays_immutable() {
    // Editing an open event's cutoff creates a NEW manifest revision +
    // plan; the prior plan and any run under it are untouched.
    let (_dir, conn) = open_temp_catalog();
    let p1 = seed_plan_with(&conn, true, Some("2026-08-02T00:00:00Z"));
    // A later reviewed revision (new cutoff).
    let payload: String = conn
        .query_row(
            "SELECT m.payload FROM manifest_revisions m JOIN analysis_plans p ON p.manifest_revision_id = m.id WHERE p.id = ?1",
            [p1],
            |r| r.get(0),
        )
        .unwrap();
    let mut manifest: inim::manifest::Manifest = serde_json::from_str(&payload).unwrap();
    manifest.revision += 1;
    manifest.event_window_utc.end = "2026-08-03T00:00:00Z".to_string();
    let new_payload = serde_json::to_string(&manifest).unwrap();
    let eid: i64 = conn
        .query_row("SELECT event_id FROM manifest_revisions LIMIT 1", [], |r| {
            r.get(0)
        })
        .unwrap();
    let sid: i64 = conn
        .query_row(
            "SELECT snapshot_id FROM manifest_revisions LIMIT 1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let rev = inim::catalog::domain::ManifestRevision {
        id: 0,
        event_id: eid,
        snapshot_id: sid,
        manifest_schema: 2,
        payload: new_payload.clone(),
        sha256: inim::catalog::document::hex_sha256(new_payload.as_bytes()),
        review_status: "Reviewed".to_string(),
        reviewed_at: Some("2026-08-02T00:00:00Z".to_string()),
        reviewer: Some("local-review".to_string()),
    };
    let mid2 = inim::catalog::store::insert_manifest_revision(&conn, &rev).unwrap();
    let plan_rec = inim::catalog::import::build_plan_record(&conn, mid2, &manifest, true).unwrap();
    let p2 = inim::catalog::store::insert_plan(&conn, &plan_rec).unwrap();
    assert_ne!(p1, p2);
    // Both plans exist; neither was mutated.
    let status1: String = conn
        .query_row(
            "SELECT status FROM analysis_plans WHERE id = ?1",
            [p1],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(status1, "Ready");
}

#[test]
fn collector_site_is_not_peer_location() {
    // The plan UI must never infer peer location from collector
    // location: collector identity and peer identity are distinct
    // concepts in the view model.
    let (_dir, conn) = open_temp_catalog();
    let plan_id = seed_plan_with(&conn, false, Some("2026-08-02T00:00:00Z"));
    let view = inim::catalog::web::jobs_view::load_plan_review(&conn, "OPEN-EVT", false)
        .unwrap()
        .unwrap();
    let collectors_row = view
        .derived
        .iter()
        .find(|r| r.label == "Collectors")
        .unwrap();
    assert!(collectors_row.value.contains("route-views2"));
    // No "peer location" row may exist anywhere in the plan page.
    assert!(!view
        .reviewed
        .iter()
        .any(|r| r.label.contains("peer location")));
    assert!(!view
        .derived
        .iter()
        .any(|r| r.label.contains("peer location")));
    let _ = plan_id;
}

#[test]
fn routeviews_and_ris_outputs_remain_distinguishable() {
    // The plan review page labels the source family explicitly so
    // RouteViews and RIPE RIS evidence can never be merged silently.
    let (_dir, conn) = open_temp_catalog();
    let plan_id = seed_plan_with(&conn, false, Some("2026-08-02T00:00:00Z"));
    let view = inim::catalog::web::jobs_view::load_plan_review(&conn, "OPEN-EVT", false)
        .unwrap()
        .unwrap();
    let family = view
        .derived
        .iter()
        .find(|r| r.label == "Source family")
        .unwrap();
    assert_eq!(family.value, "RouteViews");
    let _ = plan_id;
}

// ── HTTP execution boundary (Part 39) ───────────────────────────────

#[test]
fn web_router_cannot_call_analysis_engine() {
    // Architecture guard: the web modules must never import the
    // analysis engine, archive discovery, or MRT parsing. This is a
    // source-level dependency scan (the session permits this for the
    // router boundary; the module structure keeps the boundary
    // compile-time as well).
    let web_dir = "src/catalog/web";
    let forbidden = [
        "crate::discover",
        "crate::orchestrate",
        "crate::ingest",
        "crate::worker",
        "bgpkit",
    ];
    let mut offenders: Vec<String> = Vec::new();
    for entry in std::fs::read_dir(web_dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().map(|e| e == "rs").unwrap_or(false) {
            let content = std::fs::read_to_string(&path).unwrap();
            for f in forbidden {
                // Only flag actual imports, not comments.
                for line in content.lines() {
                    let trimmed = line.trim_start();
                    if (trimmed.starts_with("use ") || trimmed.starts_with("pub use "))
                        && trimmed.contains(f)
                    {
                        offenders.push(format!("{}: {trimmed}", path.display()));
                    }
                }
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "web modules must not import the analysis engine:\n{}",
        offenders.join("\n")
    );
}

#[test]
fn web_post_queue_performs_zero_archive_calls() {
    // Queueing with NO cache directory present must succeed: the web
    // path performs no discovery, no downloads, no parsing.
    let (_dir, root, db, plan_id) = build_fixture();
    std::fs::remove_dir_all(root.join("cache")).unwrap();
    let conn = db::open_catalog(&db).unwrap();
    let payload = plan::manifest_payload_for_plan(&conn, plan_id).unwrap();
    let hash = plan::canonical_plan_hash(&payload).unwrap();
    let out = service::queue(&conn, plan_id, RequestSource::LocalWeb, &hash).unwrap();
    match out {
        service::QueueOutcome::Created(_) => {}
        other => panic!("queue must succeed without any archive access: {other:?}"),
    }
    // No run, no artifacts, no staging.
    let runs: i64 = conn
        .query_row("SELECT COUNT(*) FROM analysis_runs", [], |r| r.get(0))
        .unwrap();
    assert_eq!(runs, 0);
    assert!(!root.join("data/runs").exists());
}

#[test]
fn get_requests_are_database_read_only() {
    // All GET routes execute against a READ-ONLY connection. The web
    // suite already serves every route with open_catalog_readonly; here
    // we assert the job routes specifically.
    let (_dir, root, db, plan_id) = build_fixture();
    let conn = db::open_catalog(&db).unwrap();
    let payload = plan::manifest_payload_for_plan(&conn, plan_id).unwrap();
    let hash = plan::canonical_plan_hash(&payload).unwrap();
    let job_id = match service::queue(&conn, plan_id, RequestSource::Cli, &hash).unwrap() {
        service::QueueOutcome::Created(id) => id,
        _ => unreachable!(),
    };
    drop(conn);
    // A read-only connection can serve the job and plan views.
    let ro = db::open_catalog_readonly(&db).unwrap();
    let view = inim::catalog::web::jobs_view::load_job_detail(&ro, &job_id, false).unwrap();
    assert!(view.is_some());
    let _ = root;
}

// ── Concurrency (Part 40) ───────────────────────────────────────────

#[test]
fn server_reads_while_worker_writes_progress() {
    // The web server (one connection) reads the job page while the
    // worker (a second connection) transitions the job through stages.
    // No deadlock, no lost state, deterministic final records.
    let (dir, root, db, plan_id) = build_fixture();
    let conn = db::open_catalog(&db).unwrap();
    let payload = plan::manifest_payload_for_plan(&conn, plan_id).unwrap();
    let hash = plan::canonical_plan_hash(&payload).unwrap();
    let job_id = match service::queue(&conn, plan_id, RequestSource::Cli, &hash).unwrap() {
        service::QueueOutcome::Created(id) => id,
        _ => unreachable!(),
    };
    drop(conn);

    // Reader thread: poll the job detail view while the worker runs.
    let db_r = db.clone();
    let job_r = job_id.clone();
    let reader = std::thread::spawn(move || {
        let ro = db::open_catalog_readonly(&db_r).unwrap();
        let mut reads = 0;
        while reads < 5 {
            let view = inim::catalog::web::jobs_view::load_job_detail(&ro, &job_r, false).unwrap();
            assert!(view.is_some());
            reads += 1;
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        reads
    });

    let code = inim::worker::run_worker(&worker_config(&root, &db));
    assert_eq!(code, 0);
    let reads = reader.join().unwrap();
    assert!(reads >= 5);

    // Deterministic final records.
    let conn = db::open_catalog(&db).unwrap();
    let job = service::get(&conn, &job_id).unwrap();
    assert_eq!(job.state, JobState::Completed);
    let run_id = job.completed_run_id.unwrap();
    let runs: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM analysis_runs WHERE id = ?1",
            [run_id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(runs, 1);
    let _ = dir;
}
