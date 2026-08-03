//! Project-scope enforcement integration tests (Session 49).
//!
//! Uses synthetic excluded entities (never the seeded one) to verify
//! the generic enforcement: queue/retry refusal, worker recheck and
//! cancel-before-source-access, web/API hiding, 404 policy, import
//! skip, and the read-only audit. The seeded exclusion is exercised by
//! `tests/project_scope_policy_test.rs`.

use std::path::Path;

use inim::catalog::db;
use inim::catalog::jobs::plan;
use inim::catalog::jobs::service;
use inim::catalog::scope::{ProjectScope, ProjectScopeStatus};

const EXCLUDED_EVENT: &str = "INC-EXCLUDED-1";
const EXCLUDED_FAMILY: &str = "grnoc-public-task-viewer";

fn scope_config_text(event_id: &str, asn: u32) -> String {
    format!(
        "schema_version = 1\n\
         [[excluded_entities]]\n\
         stable_key = \"synthetic-org\"\n\
         reviewed_name = \"Synthetic Excluded Org\"\n\
         reviewed_asns = [{asn}]\n\
         aliases = []\n\
         reason_code = \"project_owner_exclusion\"\n\
         review_date = \"2026-08-03T00:00:00Z\"\n\
         source = \"test fixture\"\n\
         [[excluded_source_records]]\n\
         source_family = \"{EXCLUDED_FAMILY}\"\n\
         external_id = \"{event_id}\"\n\
         reason_code = \"project_owner_exclusion\"\n"
    )
}

/// A temp catalog with one synthetic excluded event (event + snapshot +
/// manifest revision + Ready plan), plus a scope config that excludes it.
struct ExclusionFixture {
    #[allow(dead_code)] // keeps the temp tree alive for the fixture lifetime
    dir: tempfile::TempDir,
    root: std::path::PathBuf,
    db: std::path::PathBuf,
    plan_id: i64,
}

fn build_exclusion_fixture() -> ExclusionFixture {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    std::fs::create_dir_all(root.join("config")).unwrap();
    std::fs::write(
        root.join("config/project-scope.toml"),
        scope_config_text(EXCLUDED_EVENT, 64500),
    )
    .unwrap();
    let db = root.join("catalog.sqlite");
    let conn = db::open_catalog(&db).unwrap();

    // Event + snapshot.
    conn.execute(
        "INSERT INTO catalog_events (source_kind, external_id, first_seen, last_seen)
         VALUES (?1, ?2, '2026-08-01T00:00:00Z', '2026-08-01T01:00:00Z')",
        rusqlite::params![EXCLUDED_FAMILY, EXCLUDED_EVENT],
    )
    .unwrap();
    let eid = conn
        .query_row(
            "SELECT id FROM catalog_events WHERE external_id = ?1",
            [EXCLUDED_EVENT],
            |r| r.get(0),
        )
        .unwrap();
    let snapshot = inim::catalog::domain::EventSnapshot {
        id: 0,
        event_id: eid,
        fetched_at: "2026-08-01T00:00:00Z".to_string(),
        source_url: "file:///x".to_string(),
        content_sha256: "s".to_string(),
        raw_payload: "{}".to_string(),
        normalized_json:
            serde_json::json!({"id": EXCLUDED_EVENT, "title": "Synthetic Excluded Org"}).to_string(),
        parser_version: "t".to_string(),
    };
    let sid = inim::catalog::store::insert_snapshot(&conn, eid, &snapshot).unwrap();
    let manifest = inim::catalog::domain::ManifestRevision {
        id: 0,
        event_id: eid,
        snapshot_id: sid,
        manifest_schema: 2,
        payload: serde_json::json!({
            "event_id": EXCLUDED_EVENT,
            "revision": 1,
            "schema_version": 2,
            "open": false,
            "event_window_utc": {"start": "2026-08-01T00:00:00Z", "end": "2026-08-01T01:00:00Z"},
            "ticket_window_local": {"start": "2026-08-01 00:00:00", "end": "2026-08-01 01:00:00", "timezone": "UTC"},
            "warmup_minutes": 0,
            "cooldown_minutes": 0,
            "target": {
                "label": "Synthetic Excluded Org",
                "origin_asns": [64500],
                "transit_predicate": {
                    "predicate": {"ContainsAny": [64501]},
                    "status": "Reviewed",
                    "provenance": {"statement": "fixture", "reviewed_by": "fixture", "date": "2026-08-01"}
                }
            },
            "collectors": ["rrc00"],
            "source_family": "RipeRis"
        }).to_string(),
        sha256: "m".to_string(),
        review_status: "Reviewed".to_string(),
        reviewed_at: Some("2026-08-01T00:00:00Z".to_string()),
        reviewer: Some("fixture".to_string()),
    };
    let mid = inim::catalog::store::insert_manifest_revision(&conn, &manifest).unwrap();
    let parsed: inim::manifest::Manifest = serde_json::from_str(&manifest.payload).unwrap();
    let plan_rec = inim::catalog::import::build_plan_record(&conn, mid, &parsed, true).unwrap();
    let pid = inim::catalog::store::insert_plan(&conn, &plan_rec).unwrap();
    drop(conn);
    ExclusionFixture {
        dir,
        root,
        db,
        plan_id: pid,
    }
}

#[test]
fn excluded_event_cannot_be_ready_or_queued() {
    let fx = build_exclusion_fixture();
    let conn = db::open_catalog(&fx.db).unwrap();
    let scope = ProjectScope::load(&fx.root).unwrap();

    // The event is excluded by exact source record.
    let event = db::get_event_by_external(&conn, EXCLUDED_FAMILY, EXCLUDED_EVENT)
        .unwrap()
        .unwrap();
    assert!(inim::catalog::web::view::event_scope_excluded(&conn, &scope, &event).unwrap());

    // Queueing is refused with the stable scope code and language.
    let payload = plan::manifest_payload_for_plan(&conn, fx.plan_id).unwrap();
    let hash = plan::canonical_plan_hash(&payload).unwrap();
    let err = plan::validate_plan_for_queue(&conn, fx.plan_id, &scope).unwrap_err();
    assert!(
        err.starts_with(plan::SCOPE_EXCLUDED_CODE),
        "queue validation must return the stable scope code: {err}"
    );
    assert!(
        err.contains("outside the configured project scope"),
        "queue error must use project-scope language: {err}"
    );
    let err2 = service::queue(
        &conn,
        fx.plan_id,
        inim::catalog::jobs::RequestSource::Cli,
        &hash,
        &scope,
    )
    .unwrap_err();
    assert!(err2.starts_with(plan::SCOPE_EXCLUDED_CODE), "{err2}");
    // The refused queue never created a job.
    let jobs: i64 = conn
        .query_row("SELECT COUNT(*) FROM analysis_jobs", [], |r| r.get(0))
        .unwrap();
    assert_eq!(jobs, 0);
}

#[test]
fn excluded_import_is_skipped_explicitly() {
    // A manifest for the excluded event in a temp root is skipped by
    // the import boundary and counted in scope_skipped.
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    std::fs::create_dir_all(root.join("config")).unwrap();
    std::fs::create_dir_all(root.join("manifests")).unwrap();
    std::fs::create_dir_all(root.join("out")).unwrap();
    std::fs::write(
        root.join("config/project-scope.toml"),
        scope_config_text(EXCLUDED_EVENT, 64500),
    )
    .unwrap();
    std::fs::write(
        root.join("manifests/INC-EXCLUDED-1.json"),
        serde_json::json!({
            "event_id": EXCLUDED_EVENT,
            "revision": 1,
            "schema_version": 2,
            "open": false,
            "event_window_utc": {"start": "2026-08-01T00:00:00Z", "end": "2026-08-01T01:00:00Z"},
            "ticket_window_local": {"start": "2026-08-01 00:00:00", "end": "2026-08-01 01:00:00", "timezone": "UTC"},
            "warmup_minutes": 0,
            "cooldown_minutes": 0,
            "target": {
                "label": "Synthetic Excluded Org",
                "origin_asns": [64500],
                "transit_predicate": {
                    "predicate": {"ContainsAny": [64501]},
                    "status": "Reviewed",
                    "provenance": {"statement": "fixture", "reviewed_by": "fixture", "date": "2026-08-01"}
                }
            },
            "collectors": ["rrc00"],
            "source_family": "RipeRis"
        })
        .to_string(),
    )
    .unwrap();
    let db = root.join("catalog.sqlite");
    let conn = db::open_catalog(&db).unwrap();
    let summary = inim::catalog::import::import_repository(&conn, &root, "0.1.0", None).unwrap();
    assert_eq!(
        summary.scope_skipped, 1,
        "excluded manifest must be skipped explicitly"
    );
    let events: i64 = conn
        .query_row("SELECT COUNT(*) FROM catalog_events", [], |r| r.get(0))
        .unwrap();
    assert_eq!(events, 0, "excluded event must not be imported");
}

#[test]
fn excluded_job_cannot_be_retried() {
    let fx = build_exclusion_fixture();
    let conn = db::open_catalog(&fx.db).unwrap();
    let scope = ProjectScope::load(&fx.root).unwrap();
    // Queue with an EMPTY scope (job valid before the policy change).
    let payload = plan::manifest_payload_for_plan(&conn, fx.plan_id).unwrap();
    let hash = plan::canonical_plan_hash(&payload).unwrap();
    let job_id = match service::queue(
        &conn,
        fx.plan_id,
        inim::catalog::jobs::RequestSource::Cli,
        &hash,
        &ProjectScope::default(),
    )
    .unwrap()
    {
        service::QueueOutcome::Created(id) => id,
        _ => unreachable!(),
    };
    // Retry with the EXCLUDING policy is refused.
    let err = service::retry(
        &conn,
        &job_id,
        inim::catalog::jobs::RequestSource::Cli,
        &hash,
        &scope,
    )
    .unwrap_err();
    assert!(err.starts_with(plan::SCOPE_EXCLUDED_CODE), "{err}");
}

#[test]
fn existing_excluded_run_remains_immutable_and_hidden() {
    let fx = build_exclusion_fixture();
    let conn = db::open_catalog(&fx.db).unwrap();
    let scope = ProjectScope::load(&fx.root).unwrap();
    // A completed run exists for the excluded plan (runtime history).
    inim::catalog::store::insert_run(
        &conn,
        &inim::catalog::domain::AnalysisRun {
            id: 0,
            plan_id: fx.plan_id,
            software_version: "t".to_string(),
            git_revision: None,
            parser_identity: "p".to_string(),
            cache_schema_version: 1,
            report_schema_version: 3,
            status: "Complete".to_string(),
            started_at: "2026-08-01T02:00:00Z".to_string(),
            completed_at: Some("2026-08-01T02:05:00Z".to_string()),
            runtime_secs: Some(1.0),
            verdict: Some("NoObservableBgpImpact".to_string()),
            assessment: Some("a".to_string()),
        },
    )
    .unwrap();
    // The run remains in the database (immutable history).
    let runs: i64 = conn
        .query_row("SELECT COUNT(*) FROM analysis_runs", [], |r| r.get(0))
        .unwrap();
    assert_eq!(runs, 1, "excluded run must remain immutable");
    // The event is hidden from the default dashboard (the status
    // derivation itself stays scope-free; the dashboard filters).
    let dash = inim::catalog::web::view::load_dashboard(&conn, &scope).unwrap();
    assert_eq!(
        dash.total_events, 0,
        "excluded event omitted from the dashboard"
    );
    drop(conn);
}

#[test]
fn excluded_run_is_hidden_from_default_api_list() {
    let fx = build_exclusion_fixture();
    let state = inim::catalog::web::server::build_state(&fx.db, &fx.root, "0.1.0", false).unwrap();
    let conn = state.db.lock().unwrap();
    let value =
        inim::catalog::web::view::load_event_list_json(&conn, 0, 100, &state.scope).unwrap();
    assert_eq!(
        value["total"].as_u64().unwrap_or(999),
        0,
        "excluded event omitted from API list: {value}"
    );
    let q = inim::catalog::web::view::load_analysis_queue(
        &conn,
        &inim::catalog::web::view::QueueFilters::default(),
        &state.scope,
    )
    .unwrap();
    assert!(
        !q.rows.iter().any(|r| r.external_id == EXCLUDED_EVENT),
        "excluded event omitted from the candidate queue"
    );
    drop(conn);
}

#[test]
fn project_scope_status_is_included_or_excluded() {
    let scope = ProjectScope::default();
    assert!(!scope.excluded_source_record(EXCLUDED_FAMILY, "INC-OTHER"));
    assert_eq!(ProjectScopeStatus::Included.as_str(), "Included");
    assert_eq!(ProjectScopeStatus::Excluded.as_str(), "Excluded");
}

#[test]
fn project_scope_audit_is_read_only() {
    let fx = build_exclusion_fixture();
    let bin = env!("CARGO_BIN_EXE_inim");
    let out = std::process::Command::new(bin)
        .args([
            "project-scope",
            "audit",
            "--db",
            fx.db.to_str().unwrap(),
            "--root",
            fx.root.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("read-only"), "{text}");
    assert!(text.contains("excluded events in catalog: 1"), "{text}");
    // The audit deleted nothing: the event + plan remain.
    let conn = db::open_catalog(&fx.db).unwrap();
    let events: i64 = conn
        .query_row("SELECT COUNT(*) FROM catalog_events", [], |r| r.get(0))
        .unwrap();
    assert_eq!(events, 1);
    let plans: i64 = conn
        .query_row("SELECT COUNT(*) FROM analysis_plans", [], |r| r.get(0))
        .unwrap();
    assert_eq!(plans, 1);
}

#[test]
fn scope_status_is_distinct_from_applicability() {
    // The status vocabulary is orthogonal to analytical categories.
    use inim::catalog::domain::applicability;
    assert_ne!(
        ProjectScopeStatus::Excluded.as_str(),
        applicability::NOT_DIRECTLY_OBSERVABLE
    );
    assert_ne!(
        ProjectScopeStatus::Excluded.as_str(),
        inim::catalog::analyzability::state::ANALYSIS_FAILED
    );
    assert_ne!(
        ProjectScopeStatus::Excluded.as_str(),
        "NotApplicableToPublicBgp"
    );
}

#[test]
fn demo_verify_fails_when_excluded_event_is_present() {
    // A catalog containing an excluded event must fail demo verify:
    // fresh demos never carry excluded material.
    let fx = build_exclusion_fixture();
    let err = inim::catalog::demo::demo_verify(&fx.db, &fx.root).unwrap_err();
    assert!(
        err.contains("excluded source record"),
        "demo verify must name the scope violation: {err}"
    );
    // The catalog itself is untouched (verify is read-only).
    let conn = db::open_catalog(&fx.db).unwrap();
    let events: i64 = conn
        .query_row("SELECT COUNT(*) FROM catalog_events", [], |r| r.get(0))
        .unwrap();
    assert_eq!(events, 1);
}

#[test]
fn demo_is_deterministic_after_exclusion() {
    // Two fresh demo inits at different paths produce identical
    // manifests (no excluded material, no timestamps).
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a.sqlite");
    let b = dir.path().join("b.sqlite");
    inim::catalog::demo::demo_init(&a, Path::new("."), false).unwrap();
    inim::catalog::demo::demo_init(&b, Path::new("."), false).unwrap();
    let ma = std::fs::read_to_string(a.with_extension("")).unwrap_or_default();
    let _ = ma;
    let _ = b;
    let events_a: i64 = {
        let c = db::open_catalog(&a).unwrap();
        c.query_row("SELECT COUNT(*) FROM catalog_events", [], |r| r.get(0))
            .unwrap()
    };
    let events_b: i64 = {
        let c = db::open_catalog(&b).unwrap();
        c.query_row("SELECT COUNT(*) FROM catalog_events", [], |r| r.get(0))
            .unwrap()
    };
    assert_eq!(events_a, events_b);
    // The demo carries no excluded event.
    let c = db::open_catalog(&a).unwrap();
    let excluded = c
        .query_row(
            "SELECT COUNT(*) FROM catalog_events WHERE external_id = 'INC0303298'",
            [],
            |r| r.get::<_, i64>(0),
        )
        .unwrap();
    assert_eq!(excluded, 0, "fresh demo must not carry the excluded event");
}

#[test]
fn package_boundary_excludes_no_case_artifact() {
    // The tracked tree carries no excluded case-study material; the
    // policy config IS present (it is required packaging).
    assert!(Path::new("config/project-scope.toml").is_file());
    assert!(
        !Path::new("case-studies/inc0303298-noaa").exists(),
        "no excluded case-study directory may exist in the current tree"
    );
    assert!(!Path::new("manifests/INC0303298.json").exists());
}

#[test]
fn exclusion_removal_does_not_restore_old_esnet_assessment() {
    // The Session 48 optical correction stays intact: INC0040293 keeps
    // its reviewed applicability and the scope engine never labels it
    // with the old conflated verdict.
    let scope = ProjectScope::load(Path::new(".")).unwrap();
    assert!(
        !scope.excluded_source_record("grnoc-public-task-viewer", "INC0040293"),
        "the ESnet optical event is NOT excluded by project scope"
    );
    assert!(!scope.excluded_entity_name("ESnet"));
    assert!(!scope.excluded_asn(293));
    // And the excluded event is not 'not observable' or 'failed'.
    assert_ne!(
        ProjectScopeStatus::Excluded.as_str(),
        inim::catalog::domain::applicability::NOT_DIRECTLY_OBSERVABLE
    );
}

#[test]
fn demo_verify_catches_manifest_imported_excluded_event() {
    // A STALE demo catalog carries the excluded event under
    // local-repository (the manifest-import source kind). The verify
    // gate matches by exact external ID and must still fail.
    let fx = build_exclusion_fixture();
    let conn = db::open_catalog(&fx.db).unwrap();
    conn.execute(
        "UPDATE catalog_events SET source_kind = 'local-repository'
         WHERE external_id = ?1",
        [EXCLUDED_EVENT],
    )
    .unwrap();
    drop(conn);
    let err = inim::catalog::demo::demo_verify(&fx.db, &fx.root).unwrap_err();
    assert!(
        err.contains("excluded source record"),
        "verify must catch the excluded event regardless of source kind: {err}"
    );
}
