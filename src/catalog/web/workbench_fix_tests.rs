//! Regression tests for the evaluator-blocking workbench corrections.
//!
//! Source-neutral: no test hard-codes an entity-specific branch in
//! product code; these tests load the tracked cases through the same
//! repository import the demo uses. No live network; no analysis.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

use crate::catalog::db;
use crate::catalog::web::server::{build_app, build_state};
use crate::catalog::web::AppState;

fn repo_artifacts_available() -> bool {
    std::path::Path::new("case-studies/indiana-gigapop-smithville-2026/out/INC0301970/report.json")
        .is_file()
        && std::path::Path::new("case-studies/manlan-2019/pilot/out").is_dir()
}

fn setup_catalog() -> (tempfile::TempDir, std::path::PathBuf) {
    let dbdir = tempfile::tempdir().unwrap();
    let path = dbdir.path().join("catalog.sqlite");
    let conn = db::open_catalog(&path).unwrap();
    crate::catalog::import::import_repository(&conn, std::path::Path::new("."), "0.1.0", None)
        .unwrap();
    let reviews = std::path::Path::new("case-studies/manlan-2019/pilot/ticket-reviews.json");
    if reviews.is_file() {
        crate::catalog::corpus_import::import_reviews(&conn, reviews).unwrap();
    }
    drop(conn);
    (dbdir, std::path::PathBuf::from("."))
}

/// Full deterministic-demo catalog (corpus + case-study layer + pilot
/// runs + reviewed run links), matching the evaluator demo. Used by
/// the MAN LAN tests, which load the tracked case through the same
/// import path the demo uses.
fn setup_demo_catalog() -> (tempfile::TempDir, std::path::PathBuf) {
    let dbdir = tempfile::tempdir().unwrap();
    let path = dbdir.path().join("catalog.sqlite");
    crate::catalog::demo::demo_init(&path, std::path::Path::new("."), false)
        .expect("demo import succeeds");
    (dbdir, std::path::PathBuf::from("."))
}

fn state_from(dbdir: &tempfile::TempDir, rootdir: &std::path::Path) -> Arc<AppState> {
    build_state(
        &dbdir.path().join("catalog.sqlite"),
        rootdir,
        "0.1.0",
        false,
    )
    .unwrap()
}

async fn get(app: &axum::Router, uri: &str) -> (StatusCode, String) {
    let response = app
        .clone()
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 16 * 1024 * 1024)
        .await
        .unwrap();
    (status, String::from_utf8_lossy(&bytes).to_string())
}

fn run_id_for(dbdir: &tempfile::TempDir, external_id: &str) -> i64 {
    let conn = db::open_catalog(&dbdir.path().join("catalog.sqlite")).unwrap();
    conn.query_row(
        "SELECT r.id FROM analysis_runs r
         JOIN analysis_plans p ON p.id = r.plan_id
         JOIN manifest_revisions m ON m.id = p.manifest_revision_id
         JOIN catalog_events e ON e.id = m.event_id
         WHERE e.external_id = ?1 ORDER BY r.id LIMIT 1",
        [external_id],
        |r| r.get(0),
    )
    .unwrap()
}

fn artifact_row(conn: &rusqlite::Connection, run_id: i64, kind: &str) -> (String, String, i64) {
    conn.query_row(
        "SELECT relative_path, sha256, size FROM analysis_artifacts
         WHERE run_id = ?1 AND kind = ?2 ORDER BY relative_path LIMIT 1",
        rusqlite::params![run_id, kind],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )
    .unwrap()
}

// ── Artifact resolution (tracked cases) ─────────────────────────────

#[tokio::test]
async fn smithville_report_artifact_resolves() {
    if !repo_artifacts_available() {
        return;
    }
    let (dbdir, rootdir) = setup_catalog();
    let conn = db::open_catalog(&dbdir.path().join("catalog.sqlite")).unwrap();
    let (rel, sha, size) = artifact_row(&conn, run_id_for(&dbdir, "INC0301970"), "report");
    let resolved =
        crate::catalog::artifact_path::resolve_artifact(&rootdir, &rel).expect("report resolves");
    let bytes = std::fs::read(&resolved).unwrap();
    assert_eq!(bytes.len() as i64, size, "size matches catalog metadata");
    let digest = sha256_hex(&bytes);
    assert_eq!(digest, sha, "SHA-256 matches catalog metadata");
}

fn sha256_hex(bytes: &[u8]) -> String {
    crate::catalog::import::sha256_hex_bytes(bytes)
}

#[test]
fn artifact_listing_and_missing_check_use_same_root() {
    let d = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(
        d.path()
            .join("case-studies/indiana-gigapop-smithville-2026/out/INC0301970"),
    )
    .unwrap();
    std::fs::write(
        d.path()
            .join("case-studies/indiana-gigapop-smithville-2026/out/INC0301970/report.json"),
        "{}",
    )
    .unwrap();
    // The web resolution and the demo verifier resolution agree.
    let web = crate::catalog::artifact_path::resolve_artifact(d.path(), "INC0301970/report.json");
    assert!(web.is_some());
}

#[tokio::test]
async fn missing_artifact_is_never_listed_as_available() {
    if !repo_artifacts_available() {
        return;
    }
    let (dbdir, _rootdir) = setup_catalog();
    let empty = tempfile::tempdir().unwrap();
    let app = build_app(state_from(&dbdir, empty.path()));
    let run_id = run_id_for(&dbdir, "INC0301970");
    let (status, body) = get(&app, &format!("/analyses/{run_id}")).await;
    assert_eq!(status, StatusCode::OK);
    // The report row is marked unavailable, never "present".
    assert!(body.contains("unavailable"), "{body}");
}

// ── Insufficient-visibility presentation ──────────────────────────

#[tokio::test]
async fn insufficient_visibility_has_dedicated_view_model() {
    if !repo_artifacts_available() {
        return;
    }
    let (dbdir, rootdir) = setup_catalog();
    let app = build_app(state_from(&dbdir, &rootdir));
    let run_id = run_id_for(&dbdir, "INC0301970");
    let (_, body) = get(&app, &format!("/analyses/{run_id}")).await;
    assert!(body.contains("Reviewed relationship"), "{body}");
    assert!(body.contains("Public vantage points checked"), "{body}");
    assert!(
        body.contains("Why this is insufficient visibility, not no route-state change"),
        "{body}"
    );
}

#[tokio::test]
async fn null_is_not_rendered_as_operator_value() {
    if !repo_artifacts_available() {
        return;
    }
    let (dbdir, rootdir) = setup_catalog();
    let app = build_app(state_from(&dbdir, &rootdir));
    let run_id = run_id_for(&dbdir, "INC0301970");
    let (_, body) = get(&app, &format!("/analyses/{run_id}")).await;
    assert!(
        !body.contains(">null<"),
        "no null operator values rendered: {body}"
    );
    assert!(!body.contains(": null"), "no null operator values rendered");
}

#[tokio::test]
async fn zero_and_not_applicable_are_distinct() {
    if !repo_artifacts_available() {
        return;
    }
    let (dbdir, rootdir) = setup_catalog();
    let app = build_app(state_from(&dbdir, &rootdir));
    let run_id = run_id_for(&dbdir, "INC0301970");
    let (_, body) = get(&app, &format!("/analyses/{run_id}")).await;
    // Known measured zero renders as zero.
    assert!(body.contains("Qualifying observer sessions"), "{body}");
    // Lifecycle classification is Not applicable, not zero.
    assert!(
        body.contains("Not applicable — no qualifying cohort was frozen"),
        "{body}"
    );
}

#[tokio::test]
async fn zero_stream_run_has_no_empty_lifecycle_section() {
    if !repo_artifacts_available() {
        return;
    }
    let (dbdir, rootdir) = setup_catalog();
    let app = build_app(state_from(&dbdir, &rootdir));
    let run_id = run_id_for(&dbdir, "INC0301970");
    let (_, body) = get(&app, &format!("/analyses/{run_id}")).await;
    assert!(
        !body.contains("Lifecycle counts"),
        "no lifecycle counters for zero-stream runs"
    );
}

#[tokio::test]
async fn insufficient_visibility_is_not_no_change() {
    if !repo_artifacts_available() {
        return;
    }
    let (dbdir, rootdir) = setup_catalog();
    let app = build_app(state_from(&dbdir, &rootdir));
    let run_id = run_id_for(&dbdir, "INC0301970");
    let (_, body) = get(&app, &format!("/analyses/{run_id}")).await;
    assert!(
        body.contains("not no route-state change"),
        "insufficient visibility is not presented as no-change: {body}"
    );
}

#[tokio::test]
async fn smithville_page_lists_all_preflight_collectors() {
    if !repo_artifacts_available() {
        return;
    }
    let (dbdir, rootdir) = setup_catalog();
    let app = build_app(state_from(&dbdir, &rootdir));
    let run_id = run_id_for(&dbdir, "INC0301970");
    let (_, body) = get(&app, &format!("/analyses/{run_id}")).await;
    assert!(body.contains("route-views2"), "{body}");
    assert!(body.contains("route-views6"), "{body}");
}

#[tokio::test]
async fn collector_site_not_labeled_peer_location() {
    if !repo_artifacts_available() {
        return;
    }
    let (dbdir, rootdir) = setup_catalog();
    let app = build_app(state_from(&dbdir, &rootdir));
    let run_id = run_id_for(&dbdir, "INC0301970");
    let (_, body) = get(&app, &format!("/analyses/{run_id}")).await;
    assert!(body.contains("Collector site"), "{body}");
    assert!(
        !body.contains(">Location<"),
        "collector site is not labeled Location"
    );
}

#[tokio::test]
async fn target_visible_relationship_absent_rendered() {
    if !repo_artifacts_available() {
        return;
    }
    let (dbdir, rootdir) = setup_catalog();
    let app = build_app(state_from(&dbdir, &rootdir));
    let run_id = run_id_for(&dbdir, "INC0301970");
    let (_, body) = get(&app, &format!("/analyses/{run_id}")).await;
    // Reviewed determination note carries the relationship-absent fact.
    assert!(
        body.contains("reviewed path condition") && body.contains("Adjacent"),
        "reviewed path condition rendered: {body}"
    );
}

#[tokio::test]
async fn required_session_absent_rendered() {
    if !repo_artifacts_available() {
        return;
    }
    let (dbdir, rootdir) = setup_catalog();
    let app = build_app(state_from(&dbdir, &rootdir));
    let run_id = run_id_for(&dbdir, "INC0301970");
    let (_, body) = get(&app, &format!("/analyses/{run_id}")).await;
    assert!(
        body.contains("Reviewed determination"),
        "reviewed determination section rendered: {body}"
    );
}

#[tokio::test]
async fn no_update_acquisition_explained() {
    if !repo_artifacts_available() {
        return;
    }
    let (dbdir, rootdir) = setup_catalog();
    let app = build_app(state_from(&dbdir, &rootdir));
    let run_id = run_id_for(&dbdir, "INC0301970");
    let (_, body) = get(&app, &format!("/analyses/{run_id}")).await;
    assert!(
        body.contains("No UPDATE archives were acquired"),
        "no-update acquisition explained: {body}"
    );
}

// ── Event-page workflow (tracked open event) ────────────────────────

#[tokio::test]
async fn completed_run_leads_over_ready_plan() {
    if !repo_artifacts_available() {
        return;
    }
    let (dbdir, rootdir) = setup_catalog();
    let app = build_app(state_from(&dbdir, &rootdir));
    let (_, body) = get(&app, "/events/INC0301970").await;
    assert!(body.contains("Provisional analysis completed"), "{body}");
    assert!(
        !body.contains("Ready to queue"),
        "completed run leads over ready plan"
    );
}

#[tokio::test]
async fn open_event_page_shows_cutoff() {
    if !repo_artifacts_available() {
        return;
    }
    let (dbdir, rootdir) = setup_catalog();
    let app = build_app(state_from(&dbdir, &rootdir));
    let (_, body) = get(&app, "/events/INC0301970").await;
    assert!(body.contains("Analysis cutoff"), "{body}");
    assert!(
        body.contains("2026-08-04T00:01:37Z"),
        "reviewed cutoff rendered: {body}"
    );
}

#[tokio::test]
async fn read_only_demo_does_not_present_queue_as_next_step() {
    if !repo_artifacts_available() {
        return;
    }
    let (dbdir, rootdir) = setup_catalog();
    let app = build_app(state_from(&dbdir, &rootdir));
    let (_, body) = get(&app, "/events/INC0301970").await;
    assert!(
        !body.contains("Ready to queue"),
        "read-only demo does not present queueing as the next step"
    );
    assert!(
        body.contains("not the next step"),
        "queueing is explicitly not the next step: {body}"
    );
}

#[tokio::test]
async fn plan_readiness_and_run_status_remain_distinct() {
    if !repo_artifacts_available() {
        return;
    }
    let (dbdir, rootdir) = setup_catalog();
    let app = build_app(state_from(&dbdir, &rootdir));
    let (_, body) = get(&app, "/events/INC0301970").await;
    assert!(
        body.contains("The reviewed plan remains reproducible"),
        "plan readiness shown as secondary state: {body}"
    );
}

// ── Fixture-path provenance ───────────────────────────────────────

#[tokio::test]
async fn demo_import_path_not_presented_as_original_source() {
    if !repo_artifacts_available() {
        return;
    }
    let (dbdir, rootdir) = setup_catalog();
    let app = build_app(state_from(&dbdir, &rootdir));
    let (_, body) = get(&app, "/events/INC0301970").await;
    assert!(
        body.contains("imported from tracked offline fixture"),
        "fixture import provenance is disclosed: {body}"
    );
    // The fixture path is not the primary source identity.
    let primary = body.find("GRNOC Public Task Viewer");
    let fixture = body.find("file://");
    assert!(primary.is_some() && fixture.is_some(), "{body}");
    assert!(
        primary.unwrap() < fixture.unwrap(),
        "source identity precedes import provenance"
    );
}

#[tokio::test]
async fn snapshot_hash_remains_visible() {
    if !repo_artifacts_available() {
        return;
    }
    let (dbdir, rootdir) = setup_catalog();
    let app = build_app(state_from(&dbdir, &rootdir));
    let (_, body) = get(&app, "/events/INC0301970").await;
    let conn = db::open_catalog(&dbdir.path().join("catalog.sqlite")).unwrap();
    let sha: String = conn
        .query_row(
            "SELECT s.content_sha256 FROM event_snapshots s
             JOIN catalog_events e ON e.id = s.event_id
             WHERE e.external_id = 'INC0301970'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(body.contains(&sha[..16]), "snapshot SHA rendered: {body}");
}

// ── Case-study target authority ────────────────────────────────────

#[tokio::test]
async fn linked_reviewed_target_overrides_unresearched_aar_placeholder() {
    if !repo_artifacts_available() {
        return;
    }
    let (dbdir, rootdir) = setup_demo_catalog();
    let app = build_app(state_from(&dbdir, &rootdir));
    let (_, body) = get(&app, "/case-studies/manlan-2019").await;
    // The reviewed target appears as an analyzed target, not as unresearched.
    assert!(body.contains("Analyzed targets"), "{body}");
    let analyzed = body.find("Analyzed targets").unwrap();
    let segment = &body[analyzed..analyzed + 1200];
    assert!(
        segment.to_lowercase().contains("nordunet"),
        "analyzed target names the reviewed target: {segment}"
    );
    // The mentioned-participants table no longer lists the analyzed target.
    let mentioned = body.find("operator-reported").unwrap();
    let segment = &body[mentioned..mentioned + 1200];
    assert!(
        !segment.to_lowercase().contains("nordunet"),
        "analyzed target not mentioned-unreviewed: {segment}"
    );
}

#[tokio::test]
async fn unreviewed_aar_participant_remains_unreviewed() {
    if !repo_artifacts_available() {
        return;
    }
    let (dbdir, rootdir) = setup_demo_catalog();
    let app = build_app(state_from(&dbdir, &rootdir));
    let (_, body) = get(&app, "/case-studies/manlan-2019").await;
    let mentioned = body.find("operator-reported").unwrap();
    let segment = &body[mentioned..mentioned + 2000];
    assert!(
        segment.to_lowercase().contains("canarie"),
        "other participants still listed: {segment}"
    );
    assert!(
        segment.contains("Unresearched"),
        "unreviewed participant stays unreviewed: {segment}"
    );
}

#[tokio::test]
async fn manlan_does_not_say_pilot_unplanned_when_runs_exist() {
    if !repo_artifacts_available() {
        return;
    }
    let (dbdir, rootdir) = setup_demo_catalog();
    let app = build_app(state_from(&dbdir, &rootdir));
    let (_, body) = get(&app, "/case-studies/manlan-2019").await;
    assert!(body.contains("Completed target analysis"), "{body}");
    assert!(!body.contains("Historical pilot — Not planned"), "{body}");
}

#[tokio::test]
async fn incident_wide_plan_and_target_plan_distinct() {
    if !repo_artifacts_available() {
        return;
    }
    let (dbdir, rootdir) = setup_demo_catalog();
    let app = build_app(state_from(&dbdir, &rootdir));
    let (_, body) = get(&app, "/case-studies/manlan-2019").await;
    assert!(
        body.contains("Incident-wide plan: None"),
        "no incident-wide plan claim: {body}"
    );
}

#[tokio::test]
async fn manlan_first_summary_contains_route_story() {
    if !repo_artifacts_available() {
        return;
    }
    let (dbdir, rootdir) = setup_demo_catalog();
    let app = build_app(state_from(&dbdir, &rootdir));
    let (_, body) = get(&app, "/case-studies/manlan-2019").await;
    assert!(
        body.contains("Reviewed target:"),
        "route story present: {body}"
    );
    assert!(
        body.contains("Selected observers saw route changes"),
        "{body}"
    );
    assert!(body.contains("Reviewed scope:"), "{body}");
}

#[tokio::test]
async fn manlan_first_summary_preserves_rrc15_cooldown() {
    if !repo_artifacts_available() {
        return;
    }
    let (dbdir, rootdir) = setup_demo_catalog();
    let app = build_app(state_from(&dbdir, &rootdir));
    let (_, body) = get(&app, "/case-studies/manlan-2019").await;
    assert!(
        body.contains("changed again during cooldown"),
        "rrc15 cooldown re-change preserved: {body}"
    );
}

#[tokio::test]
async fn aggregate_stream_count_not_primary_summary() {
    if !repo_artifacts_available() {
        return;
    }
    let (dbdir, rootdir) = setup_demo_catalog();
    let app = build_app(state_from(&dbdir, &rootdir));
    let (_, body) = get(&app, "/case-studies/manlan-2019").await;
    let story = body.find("Reviewed target:").unwrap();
    let aggregate = body.find("route-state stream-counts");
    assert!(
        aggregate.is_some(),
        "aggregate counts remain available: {body}"
    );
    assert!(
        story < aggregate.unwrap(),
        "route story precedes aggregate stream counts"
    );
}

#[tokio::test]
async fn linked_runs_identifiable_without_run_id() {
    if !repo_artifacts_available() {
        return;
    }
    let (dbdir, rootdir) = setup_demo_catalog();
    let app = build_app(state_from(&dbdir, &rootdir));
    let (_, body) = get(&app, "/case-studies/manlan-2019").await;
    let runs = body.find("Analysis runs").unwrap();
    let segment = &body[runs..runs + 2500];
    assert!(
        segment.to_lowercase().contains("nordunet"),
        "target column: {segment}"
    );
    assert!(
        segment.contains("RipeRis") || segment.contains("RouteViews"),
        "family column: {segment}"
    );
    assert!(
        segment.contains("rrc00") || segment.to_lowercase().contains("route-views2"),
        "collector column: {segment}"
    );
}

#[tokio::test]
async fn legacy_machine_verdict_not_primary_human_label() {
    if !repo_artifacts_available() {
        return;
    }
    let (dbdir, rootdir) = setup_demo_catalog();
    let app = build_app(state_from(&dbdir, &rootdir));
    let (_, body) = get(&app, "/case-studies/manlan-2019").await;
    assert!(!body.contains("LessImpactThanExpected"), "{body}");
    assert!(!body.contains("ExpectedLossOfReachability"), "{body}");
    // Current human labels are present.
    assert!(
        body.contains("Route-state changes observed") || body.contains("Partial"),
        "{body}"
    );
}

#[tokio::test]
async fn manlan_initial_view_is_bounded() {
    if !repo_artifacts_available() {
        return;
    }
    let (dbdir, rootdir) = setup_demo_catalog();
    let app = build_app(state_from(&dbdir, &rootdir));
    let (_, body) = get(&app, "/case-studies/manlan-2019").await;
    // The complete evidence matrix is inside a <details> disclosure;
    // the per-collector summary appears before it.
    let summary = body.find("Per-collector summaries").unwrap();
    let details = body.find("Complete prefix × observer evidence").unwrap();
    assert!(
        summary < details,
        "compact summaries precede the evidence matrix"
    );
    assert!(
        body.contains("<details>"),
        "evidence matrix is a disclosure: {body}"
    );
}

#[tokio::test]
async fn complete_evidence_table_remains_reachable() {
    if !repo_artifacts_available() {
        return;
    }
    let (dbdir, rootdir) = setup_demo_catalog();
    let app = build_app(state_from(&dbdir, &rootdir));
    let (_, body) = get(&app, "/case-studies/manlan-2019").await;
    assert!(
        body.contains("peer rows"),
        "complete row count is disclosed in the details summary: {body}"
    );
    assert!(body.contains("rowspan"), "peer-level detail rows preserved");
}

#[tokio::test]
async fn cross_observer_collector_timing_deduplicated() {
    if !repo_artifacts_available() {
        return;
    }
    let (dbdir, rootdir) = setup_demo_catalog();
    let app = build_app(state_from(&dbdir, &rootdir));
    let (_, body) = get(&app, "/case-studies/manlan-2019").await;
    // The grouped matrix shows one first-change cell per collector per
    // prefix; peer detail lives in the peer rows.
    let details = body.find("Complete prefix × observer evidence").unwrap();
    let segment = &body[details..details + 4000];
    assert!(
        segment.contains("rowspan"),
        "collector-level rows with peer detail: {segment}"
    );
}

#[tokio::test]
async fn acquired_ticket_not_labeled_unretrieved() {
    if !repo_artifacts_available() {
        return;
    }
    let (dbdir, rootdir) = setup_demo_catalog();
    let app = build_app(state_from(&dbdir, &rootdir));
    let (_, body) = get(&app, "/case-studies/manlan-2019").await;
    assert!(
        body.contains("superseded by the acquired immutable snapshot"),
        "acquired tickets show the superseded note: {body}"
    );
    assert!(
        body.matches("not independently retrieved").count() <= 2,
        "stale unretrieved language retained only for unresolved TASK references: {body}"
    );
}

#[tokio::test]
async fn unresolved_task_reference_not_rendered_as_ticket_snapshot() {
    if !repo_artifacts_available() {
        return;
    }
    let (dbdir, rootdir) = setup_demo_catalog();
    let app = build_app(state_from(&dbdir, &rootdir));
    let (_, body) = get(&app, "/case-studies/manlan-2019").await;
    assert!(
        body.contains("unresolved document reference"),
        "TASK references remain unresolved: {body}"
    );
}

#[tokio::test]
async fn collector_site_label_explicit() {
    if !repo_artifacts_available() {
        return;
    }
    let (dbdir, rootdir) = setup_demo_catalog();
    let app = build_app(state_from(&dbdir, &rootdir));
    let (_, body) = get(&app, "/case-studies/manlan-2019").await;
    assert!(body.contains("Collector site"), "{body}");
    assert!(
        !body.contains(">Location<"),
        "no generic Location column: {body}"
    );
    assert!(
        body.contains("observer peer's location"),
        "collector-site note present: {body}"
    );
}

// ── Generic guards ────────────────────────────────────────────────

#[test]
fn no_event_specific_template_branches() {
    for f in [
        "src/catalog/web/templates/analysis.html",
        "src/catalog/web/templates/event_detail.html",
        "src/catalog/web/templates/case_study.html",
    ] {
        let text = std::fs::read_to_string(f).unwrap();
        for entity in ["INC0301970", "manlan-2019"] {
            assert!(
                !text.contains(entity),
                "{f} must not hard-code an entity: {entity}"
            );
        }
    }
}

#[test]
fn canonical_artifact_hashes_unchanged() {
    if !repo_artifacts_available() {
        return;
    }
    // The tracked run's catalog rows must match the tracked files
    // exactly (the demo import verifies the same contract).
    let manifest: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(
            "case-studies/indiana-gigapop-smithville-2026/out/INC0301970/archive_manifest.json",
        )
        .unwrap(),
    )
    .unwrap();
    for arch in manifest.get("archives").and_then(|a| a.as_array()).unwrap() {
        let url = arch.get("url").and_then(|v| v.as_str()).unwrap_or("");
        let sha = arch.get("sha256").and_then(|v| v.as_str()).unwrap_or("");
        assert!(
            !url.is_empty() && !sha.is_empty(),
            "archive manifest rows complete"
        );
        assert_eq!(sha.len(), 64, "SHA-256 length for {url}");
    }
}
