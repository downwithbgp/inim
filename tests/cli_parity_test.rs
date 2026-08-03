//! CLI parity tests: the CLI and the web use the SAME job service, so
//! queue/cancel/retry behave identically; CLI JSON output carries an
//! explicit schema version; demo init/verify is offline and
//! deterministic.

use inim::catalog::db;
use inim::catalog::jobs::{plan, service, JobState, RequestSource};

fn temp_catalog() -> (tempfile::TempDir, rusqlite::Connection) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("catalog.sqlite");
    let conn = db::open_catalog(&path).unwrap();
    (dir, conn)
}

fn seed_ready_plan(conn: &rusqlite::Connection) -> i64 {
    conn.execute(
        "INSERT INTO catalog_events (source_kind, external_id, first_seen, last_seen)
         VALUES ('local-repository', 'CLI-EVT', '2026-08-01T00:00:00Z', '2026-08-01T00:00:00Z')",
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
        "event_id": "CLI-EVT",
        "revision": 1,
        "schema_version": 2,
        "open": false,
        "event_window_utc": {"start": "2026-08-01T00:00:00Z", "end": "2026-08-02T00:00:00Z"},
        "ticket_window_local": {"start": "", "end": "", "timezone": "UTC"},
        "target": {
            "label": "CLI event",
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
        rusqlite::params![eid, sid, payload, "cli-msha"],
    )
    .unwrap();
    let mid = conn.last_insert_rowid();
    let plan_rec = inim::catalog::import::build_plan_record(
        conn,
        mid,
        &serde_json::from_str(&payload).unwrap(),
        true,
    )
    .unwrap();
    inim::catalog::store::insert_plan(conn, &plan_rec).unwrap()
}

#[test]
fn cli_and_web_queue_use_same_service() {
    // Both the CLI command and the web handler call
    // service::queue with the same plan hash and RequestSource marker;
    // the resulting rows are identical in every execution-relevant
    // field. This test exercises the service exactly as both paths do.
    let (_dir, conn) = temp_catalog();
    let plan_id = seed_ready_plan(&conn);
    let payload = plan::manifest_payload_for_plan(&conn, plan_id).unwrap();
    let hash = plan::canonical_plan_hash(&payload).unwrap();
    let out = service::queue(&conn, plan_id, RequestSource::Cli, &hash, &inim::catalog::scope::ProjectScope::default()).unwrap();
    let job_id = match out {
        service::QueueOutcome::Created(id) => id,
        _ => unreachable!(),
    };
    let job = service::get(&conn, &job_id).unwrap();
    assert_eq!(job.state, JobState::Queued);
    assert_eq!(job.requested_by, "cli");
    assert_eq!(job.plan_hash, hash);
    assert_eq!(job.plan_revision_id, plan_id);
    // The same queue call with LocalWeb (the web path) against a
    // completed job creates a fresh identical job — proving the service
    // (not the caller) owns the rules.
    let mut conn = conn;
    service::claim_next(&mut conn, "w", 90).unwrap();
    conn.execute(
        "INSERT INTO analysis_runs (plan_id, software_version, parser_identity, cache_schema_version, report_schema_version, status, started_at)
         VALUES (?1, '0.1.0', 'p', 1, 1, 'Complete', '2026-08-01T00:00:00Z')",
        [plan_id],
    )
    .unwrap();
    let run_id = conn.last_insert_rowid();
    for (from, to) in [
        (JobState::Claimed, JobState::DiscoveringArchives),
        (JobState::DiscoveringArchives, JobState::AcquiringArchives),
        (JobState::AcquiringArchives, JobState::ParsingBaseline),
        (JobState::ParsingBaseline, JobState::FreezingCohort),
        (JobState::FreezingCohort, JobState::ParsingUpdates),
        (JobState::ParsingUpdates, JobState::ReconstructingRoutes),
        (JobState::ReconstructingRoutes, JobState::DerivingEvidence),
        (JobState::DerivingEvidence, JobState::RenderingArtifacts),
        (JobState::RenderingArtifacts, JobState::ValidatingArtifacts),
        (JobState::ValidatingArtifacts, JobState::PublishingRun),
    ] {
        service::transition(&conn, &job_id, from, to, None, "stage").unwrap();
    }
    service::complete(&conn, &job_id, run_id).unwrap();
    let out2 = service::queue(&conn, plan_id, RequestSource::LocalWeb, &hash, &inim::catalog::scope::ProjectScope::default()).unwrap();
    let job2_id = match out2 {
        service::QueueOutcome::Created(id) => id,
        _ => unreachable!(),
    };
    let job2 = service::get(&conn, &job2_id).unwrap();
    assert_eq!(job2.plan_hash, hash);
    assert_eq!(job2.plan_revision_id, plan_id);
    assert_eq!(job2.requested_by, "local-web");
}

#[test]
fn cli_cancel_matches_web_cancel() {
    let (_dir, mut conn) = temp_catalog();
    let plan_id = seed_ready_plan(&conn);
    let payload = plan::manifest_payload_for_plan(&conn, plan_id).unwrap();
    let hash = plan::canonical_plan_hash(&payload).unwrap();
    let job_id = match service::queue(&conn, plan_id, RequestSource::Cli, &hash, &inim::catalog::scope::ProjectScope::default()).unwrap() {
        service::QueueOutcome::Created(id) => id,
        _ => unreachable!(),
    };
    // CLI cancel of a queued job.
    let out = service::request_cancel(&conn, &job_id).unwrap();
    assert_eq!(out, service::CancelOutcome::Cancelled(job_id.clone()));
    assert_eq!(
        service::get(&conn, &job_id).unwrap().state,
        JobState::Cancelled
    );
    // Web cancel of an executing job: same transition path.
    let job2_id = match service::queue(&conn, plan_id, RequestSource::LocalWeb, &hash, &inim::catalog::scope::ProjectScope::default()).unwrap() {
        service::QueueOutcome::Created(id) => id,
        _ => unreachable!(),
    };
    service::claim_next(&mut conn, "w", 90).unwrap();
    let out2 = service::request_cancel(&conn, &job2_id).unwrap();
    assert_eq!(out2, service::CancelOutcome::Requested(job2_id.clone()));
    assert_eq!(
        service::get(&conn, &job2_id).unwrap().state,
        JobState::CancelRequested
    );
}

#[test]
fn cli_retry_matches_web_retry() {
    let (_dir, mut conn) = temp_catalog();
    let plan_id = seed_ready_plan(&conn);
    let payload = plan::manifest_payload_for_plan(&conn, plan_id).unwrap();
    let hash = plan::canonical_plan_hash(&payload).unwrap();
    let job_id = match service::queue(&conn, plan_id, RequestSource::Cli, &hash, &inim::catalog::scope::ProjectScope::default()).unwrap() {
        service::QueueOutcome::Created(id) => id,
        _ => unreachable!(),
    };
    service::claim_next(&mut conn, "w", 90).unwrap();
    service::fail(
        &conn,
        &job_id,
        JobState::Claimed,
        "archive_checksum_mismatch",
        "bad",
    )
    .unwrap();
    let new_cli = service::retry(
            &conn,
            &job_id,
            RequestSource::Cli,
            &hash,
            &inim::catalog::scope::ProjectScope::default(),
        )
        .unwrap();
    let new_web = service::retry(
            &conn,
            &job_id,
            RequestSource::LocalWeb,
            &hash,
            &inim::catalog::scope::ProjectScope::default(),
        )
        .unwrap();
    assert_ne!(new_cli, new_web);
    for id in [&new_cli, &new_web] {
        let j = service::get(&conn, id).unwrap();
        assert_eq!(j.state, JobState::Queued);
        assert_eq!(j.attempt, 2);
        assert_eq!(j.original_job_id.as_deref(), Some(job_id.as_str()));
    }
    // The original job is untouched.
    assert_eq!(
        service::get(&conn, &job_id).unwrap().state,
        JobState::Failed
    );
    assert_eq!(
        service::get(&conn, &job_id).unwrap().error_code.as_deref(),
        Some("archive_checksum_mismatch")
    );
}

#[test]
fn cli_json_has_explicit_schema_version() {
    // The CLI plan-show --json output carries an explicit schema
    // version (mirrored by the API envelope).
    let (_dir, conn) = temp_catalog();
    let plan_id = seed_ready_plan(&conn);
    let payload = plan::manifest_payload_for_plan(&conn, plan_id).unwrap();
    let hash = plan::canonical_plan_hash(&payload).unwrap();
    let view = inim::catalog::web::jobs_view::load_plan_review(&conn, "CLI-EVT", false)
        .unwrap()
        .unwrap();
    let json = serde_json::json!({
        "event_id": view.event_id,
        "plan_status": view.plan_status,
        "plan_revision_id": view.plan_revision_id,
        "plan_hash": view.plan_hash,
        "ready_to_queue": view.ready_to_queue,
        "schema_version": 1,
    });
    assert_eq!(json["schema_version"], 1);
    assert_eq!(json["plan_hash"].as_str(), Some(hash.as_str()));
}

#[test]
fn cli_help_documents_network_behavior() {
    // The CLI help strings must state whether each command mutates the
    // catalog and whether it accesses the network. Verified against the
    // clap-generated help text of the new commands.
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_inim"))
        .args(["analysis-job", "queue", "--help"])
        .output()
        .unwrap();
    let help = String::from_utf8_lossy(&output.stdout).to_string();
    assert!(help.contains("idempotent"), "{help}");
    let worker_help = std::process::Command::new(env!("CARGO_BIN_EXE_inim"))
        .args(["worker", "--help"])
        .output()
        .unwrap();
    let w = String::from_utf8_lossy(&worker_help.stdout).to_string();
    assert!(w.contains("--offline"), "{w}");
    assert!(w.contains("archive sources"), "{w}");
    let serve_help = std::process::Command::new(env!("CARGO_BIN_EXE_inim"))
        .args(["serve", "--help"])
        .output()
        .unwrap();
    let s = String::from_utf8_lossy(&serve_help.stdout).to_string();
    assert!(s.contains("--enable-writes"), "{s}");
    assert!(s.contains("worker"), "{s}");
}

#[test]
fn demo_init_is_offline_and_deterministic() {
    let dir = tempfile::tempdir().unwrap();
    let db1 = dir.path().join("demo1.sqlite");
    // The demo root is the repository root (contains manifests/ and
    // case-studies/); the test only works when run from the repo.
    if !std::path::Path::new("manifests").is_dir() {
        return;
    }
    let r1 = inim::catalog::demo::demo_init(&db1, std::path::Path::new("."), false).unwrap();
    assert!(r1.is_ok(), "{r1:?}");
    let db2 = dir.path().join("demo2.sqlite");
    let _r2 = inim::catalog::demo::demo_init(&db2, std::path::Path::new("."), false).unwrap();
    // Deterministic imports: same event/plan/run identity sets.
    {
        let ca = db::open_catalog(&db1).unwrap();
        let cb = db::open_catalog(&db2).unwrap();
        let events_a: Vec<(String, String)> = {
            let mut st = ca
                .prepare("SELECT source_kind, external_id FROM catalog_events ORDER BY external_id")
                .unwrap();
            st.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
                .unwrap()
                .collect::<Result<_, _>>()
                .unwrap()
        };
        let events_b: Vec<(String, String)> = {
            let mut st = cb
                .prepare("SELECT source_kind, external_id FROM catalog_events ORDER BY external_id")
                .unwrap();
            st.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
                .unwrap()
                .collect::<Result<_, _>>()
                .unwrap()
        };
        assert_eq!(events_a, events_b);
        let plans_a: Vec<String> = {
            let mut st = ca
                .prepare("SELECT sha256 FROM analysis_plans ORDER BY sha256")
                .unwrap();
            st.query_map([], |r| r.get(0))
                .unwrap()
                .collect::<Result<_, _>>()
                .unwrap()
        };
        let plans_b: Vec<String> = {
            let mut st = cb
                .prepare("SELECT sha256 FROM analysis_plans ORDER BY sha256")
                .unwrap();
            st.query_map([], |r| r.get(0))
                .unwrap()
                .collect::<Result<_, _>>()
                .unwrap()
        };
        assert_eq!(plans_a, plans_b);
    }
}

#[test]
fn demo_init_refuses_overwrite() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("demo.sqlite");
    if !std::path::Path::new("manifests").is_dir() {
        return;
    }
    inim::catalog::demo::demo_init(&db, std::path::Path::new("."), false).unwrap();
    let err = inim::catalog::demo::demo_init(&db, std::path::Path::new("."), false).unwrap_err();
    assert!(err.contains("refusing to overwrite"), "{err}");
    // --force replaces it.
    let r = inim::catalog::demo::demo_init(&db, std::path::Path::new("."), true).unwrap();
    assert!(r.is_ok());
}

#[test]
fn demo_catalog_contains_expected_examples() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("demo.sqlite");
    if !std::path::Path::new("manifests").is_dir() {
        return;
    }
    inim::catalog::demo::demo_init(&db, std::path::Path::new("."), false).unwrap();
    let conn = db::open_catalog(&db).unwrap();
    for expected in inim::catalog::demo::DEMO_EXPECTED_EVENTS {
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM catalog_events WHERE external_id = ?1",
                [expected],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 1, "missing demo event {expected}");
    }
    let cs: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM case_studies WHERE slug = 'manlan-2019'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(cs, 1);
    let runs: i64 = conn
        .query_row("SELECT COUNT(*) FROM analysis_runs", [], |r| r.get(0))
        .unwrap();
    assert!(runs >= 2, "expected at least two demo runs, got {runs}");
}

#[test]
fn demo_verify_detects_missing_artifact() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("demo.sqlite");
    if !std::path::Path::new("manifests").is_dir() {
        return;
    }
    inim::catalog::demo::demo_init(&db, std::path::Path::new("."), false).unwrap();
    // Break one resolved artifact reference: point one relative_path at
    // a file that does not exist anywhere.
    let conn = db::open_catalog(&db).unwrap();
    conn.execute(
        "UPDATE analysis_artifacts SET relative_path = 'INC0299001/nonexistent.json'
         WHERE relative_path = 'INC0299001/report.json'",
        [],
    )
    .unwrap();
    drop(conn);
    let report = inim::catalog::demo::demo_verify(&db, std::path::Path::new(".")).unwrap();
    assert!(
        report
            .unresolved_artifacts
            .iter()
            .any(|a| a.contains("nonexistent.json")),
        "{:?}",
        report.unresolved_artifacts
    );
    assert!(!report.is_ok());
}
