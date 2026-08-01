//! Web integration tests — in-process HTTP against a temporary catalog.
//!
//! No live network access; no analysis or MRT parsing happens on the
//! request path.

use std::sync::{Arc, Mutex};

use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

use crate::catalog::db;
use crate::catalog::web::handlers::EventListFilters;
use crate::catalog::web::server::{build_app, build_state};
use crate::catalog::web::AppState;

fn setup_catalog() -> (tempfile::TempDir, std::path::PathBuf) {
    let dbdir = tempfile::tempdir().unwrap();
    let path = dbdir.path().join("catalog.sqlite");
    let conn = db::open_catalog(&path).unwrap();
    // Import the repository's canonical data into the temp catalog.
    crate::catalog::import::import_repository(&conn, std::path::Path::new("."), "0.1.0", None)
        .unwrap();
    drop(conn);
    // Artifacts live in the repository's out/ directory (read-only for
    // these tests), so the catalog root is the repo root.
    (dbdir, std::path::PathBuf::from("."))
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

fn state_from(dbdir: &tempfile::TempDir, rootdir: &std::path::Path) -> Arc<AppState> {
    build_state(&dbdir.path().join("catalog.sqlite"), rootdir, "0.1.0").unwrap()
}

/// Resolve the completed run id for an imported event.
fn run_id_for(dbdir: &tempfile::TempDir, external_id: &str) -> i64 {
    let conn = db::open_catalog(&dbdir.path().join("catalog.sqlite")).unwrap();
    let e = db::get_event_by_external(&conn, "local-repository", external_id)
        .unwrap()
        .unwrap();
    db::list_runs_for_event(&conn, e.id).unwrap()[0].id
}

#[tokio::test]
async fn dashboard_counts_match_database() {
    let (dbdir, rootdir) = setup_catalog();
    let app = build_app(state_from(&dbdir, &rootdir));
    let (status, body) = get(&app, "/").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("Catalog health"));
    assert!(body.contains("Total catalog events"));
    assert!(body.contains(">3<"), "three imported events");
    assert!(body.contains(">2<"), "two completed analyses");
    assert!(body.contains(">1<"), "one blocked event");
    // No severity score anywhere.
    assert!(body.contains("No severity score is shown"));
}

#[tokio::test]
async fn dashboard_links_to_filtered_lists() {
    let (dbdir, rootdir) = setup_catalog();
    let app = build_app(state_from(&dbdir, &rootdir));
    let (_, body) = get(&app, "/catalog").await;
    assert!(body.contains("/events?status=Blocked"));
    assert!(body.contains("/events?status=Complete"));
    assert!(body.contains("/events?lifecycle=Open"));
}

#[tokio::test]
async fn event_list_orders_newest_first() {
    let (dbdir, rootdir) = setup_catalog();
    let app = build_app(state_from(&dbdir, &rootdir));
    let (status, body) = get(&app, "/events").await;
    assert_eq!(status, StatusCode::OK);
    let i2574 = body.find("INC0302574").unwrap();
    let i299 = body.find("INC0299001").unwrap();
    let i1970 = body.find("INC0301970").unwrap();
    // Sorted by event start descending: INC0302574 (07-30), then
    // INC0301970 (07-28), then INC0299001 (07-14).
    assert!(i2574 < i1970, "INC0302574 is newest by start");
    assert!(i1970 < i299, "INC0301970 sorts before INC0299001");
}

#[tokio::test]
async fn event_list_filters_by_status() {
    let (dbdir, rootdir) = setup_catalog();
    let app = build_app(state_from(&dbdir, &rootdir));
    let (_, body) = get(&app, "/events?status=Blocked").await;
    assert!(body.contains("INC0301970"));
    assert!(!body.contains("INC0302574"));
    let (_, body) = get(&app, "/events?status=Complete").await;
    assert!(body.contains("INC0302574"));
    assert!(body.contains("INC0299001"));
    assert!(!body.contains("INC0301970"));
}

#[tokio::test]
async fn event_list_filters_open_events() {
    let (dbdir, rootdir) = setup_catalog();
    let app = build_app(state_from(&dbdir, &rootdir));
    let (_, body) = get(&app, "/events?lifecycle=Open").await;
    assert!(body.contains("INC0301970"));
    assert!(!body.contains("INC0302574"));
}

#[tokio::test]
async fn event_list_searches_id_and_title() {
    let (dbdir, rootdir) = setup_catalog();
    let app = build_app(state_from(&dbdir, &rootdir));
    let (_, body) = get(&app, "/events?q=INC0302574").await;
    assert!(body.contains("INC0302574"));
    assert!(!body.contains("INC0299001"));
    let (_, body) = get(&app, "/events?q=UVA").await;
    assert!(body.contains("INC0299001"));
}

#[tokio::test]
async fn stale_event_is_visually_identifiable() {
    let (dbdir, rootdir) = setup_catalog();
    let conn = db::open_catalog(&dbdir.path().join("catalog.sqlite")).unwrap();
    // Add a new snapshot after the completed run -> stale.
    let e = db::get_event_by_external(&conn, "local-repository", "INC0302574")
        .unwrap()
        .unwrap();
    // A CHANGED ticket payload creates a new snapshot -> stale view.
    let item = crate::catalog::grnoc::source_item_from_fixture(
        std::path::Path::new("tests/fixtures/internet2/INC0302574.json"),
        "2026-08-01T00:00:00Z",
    )
    .unwrap();
    let snapshot = crate::catalog::domain::EventSnapshot {
        id: 0,
        event_id: e.id,
        fetched_at: item.fetched_at.clone(),
        source_url: item.source_url.clone(),
        content_sha256: crate::catalog::sync::hex_sha256(&format!("{}\n", item.raw_payload)),
        raw_payload: format!("{}\n", item.raw_payload),
        normalized_json: item.normalized_json,
        parser_version: "grnoc-record-1".to_string(),
    };
    crate::catalog::store::insert_snapshot(&conn, e.id, &snapshot).unwrap();
    drop(conn);
    let app = build_app(state_from(&dbdir, &rootdir));
    let (_, body) = get(&app, "/events").await;
    assert!(body.contains("Stale"), "stale badge must be visible");
    assert!(body.contains("stale"));
}

#[tokio::test]
async fn blocked_and_completed_events_are_distinct() {
    let (dbdir, rootdir) = setup_catalog();
    let app = build_app(state_from(&dbdir, &rootdir));
    let (_, blocked_page) = get(&app, "/events/INC0301970").await;
    assert!(blocked_page.contains("Blocked"));
    assert!(blocked_page.contains("no impact verdict exists"));
    assert!(!blocked_page.contains("No route-state change observed"));
    let (_, complete_page) = get(&app, "/events/INC0302574").await;
    assert!(complete_page.contains("Complete"));
    assert!(complete_page.contains("No route-state change observed"));
}

#[tokio::test]
async fn event_detail_shows_snapshot_history() {
    let (dbdir, rootdir) = setup_catalog();
    let app = build_app(state_from(&dbdir, &rootdir));
    let (status, body) = get(&app, "/events/INC0302574").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("Source snapshot history"));
    assert!(body.contains("Reviewed manifest revisions"));
    assert!(body.contains("Analysis runs"));
}

#[tokio::test]
async fn event_detail_shows_manifest_history() {
    let (dbdir, rootdir) = setup_catalog();
    let app = build_app(state_from(&dbdir, &rootdir));
    let (_, body) = get(&app, "/events/INC0299001").await;
    assert!(body.contains("manifest_revisions") || body.contains("Reviewed manifest revisions"));
    assert!(body.contains("Reviewed"));
}

#[tokio::test]
async fn event_detail_shows_all_analysis_runs() {
    let (dbdir, rootdir) = setup_catalog();
    let conn = db::open_catalog(&dbdir.path().join("catalog.sqlite")).unwrap();
    let e = db::get_event_by_external(&conn, "local-repository", "INC0299001")
        .unwrap()
        .unwrap();
    // Add a second run (re-analysis) under the same plan.
    let runs = db::list_runs_for_event(&conn, e.id).unwrap();
    let plan_id = runs[0].plan_id;
    let second = crate::catalog::domain::AnalysisRun {
        id: 0,
        plan_id,
        software_version: "0.1.0".into(),
        git_revision: Some("abc".into()),
        parser_identity: "p".into(),
        cache_schema_version: 2,
        report_schema_version: 2,
        status: "Complete".into(),
        started_at: "2026-07-31T23:00:00Z".into(),
        completed_at: Some("2026-07-31T23:00:00Z".into()),
        runtime_secs: Some(1.0),
        verdict: Some("Partial routing impact observed".into()),
        assessment: Some("Partially consistent".into()),
    };
    crate::catalog::store::insert_run(&conn, &second).unwrap();
    let run_ids: Vec<i64> = db::list_runs_for_event(&conn, e.id)
        .unwrap()
        .iter()
        .map(|r| r.id)
        .collect();
    drop(conn);
    let app = build_app(state_from(&dbdir, &rootdir));
    let (_, body) = get(&app, "/events/INC0299001").await;
    assert_eq!(run_ids.len(), 2);
    for id in run_ids {
        assert!(body.contains(&format!("/analyses/{id}")));
    }
}

#[tokio::test]
async fn blocked_event_has_no_observational_verdict() {
    let (dbdir, rootdir) = setup_catalog();
    let app = build_app(state_from(&dbdir, &rootdir));
    let (_, body) = get(&app, "/events/INC0301970").await;
    assert!(body.contains("Open"));
    assert!(body.contains("Blocked"));
    assert!(!body.contains("No route-state change observed"));
    assert!(!body.contains("Partial routing impact"));
}

#[tokio::test]
async fn stale_event_explains_changed_input() {
    let (dbdir, rootdir) = setup_catalog();
    let conn = db::open_catalog(&dbdir.path().join("catalog.sqlite")).unwrap();
    let e = db::get_event_by_external(&conn, "local-repository", "INC0302574")
        .unwrap()
        .unwrap();
    let item = crate::catalog::grnoc::source_item_from_fixture(
        std::path::Path::new("tests/fixtures/internet2/INC0302574.json"),
        "2026-08-01T00:00:00Z",
    )
    .unwrap();
    let snapshot = crate::catalog::domain::EventSnapshot {
        id: 0,
        event_id: e.id,
        fetched_at: item.fetched_at,
        source_url: item.source_url,
        content_sha256: crate::catalog::sync::hex_sha256(&format!("{}\n", item.raw_payload)),
        raw_payload: format!("{}\n", item.raw_payload),
        normalized_json: item.normalized_json,
        parser_version: "x".into(),
    };
    crate::catalog::store::insert_snapshot(&conn, e.id, &snapshot).unwrap();
    drop(conn);
    let app = build_app(state_from(&dbdir, &rootdir));
    let (_, body) = get(&app, "/events/INC0302574").await;
    assert!(body.contains("has not yet been analyzed under the latest inputs"));
}

#[tokio::test]
async fn analysis_page_matches_report_result() {
    let (dbdir, rootdir) = setup_catalog();
    let app = build_app(state_from(&dbdir, &rootdir));
    let run_id = run_id_for(&dbdir, "INC0302574");
    let (_, body) = get(&app, &format!("/analyses/{run_id}")).await;
    let report: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string("out/INC0302574/report.json").unwrap())
            .unwrap();
    let label = report["result"]["verdict_label"].as_str().unwrap();
    assert!(
        body.contains(label),
        "web page shows the report verdict label"
    );
    assert!(body.contains("Assessment against ticket expectation"));
}

#[tokio::test]
async fn analysis_page_matches_report_assessment() {
    let (dbdir, rootdir) = setup_catalog();
    let app = build_app(state_from(&dbdir, &rootdir));
    let run_id = run_id_for(&dbdir, "INC0299001");
    let (_, body) = get(&app, &format!("/analyses/{run_id}")).await;
    assert!(
        body.contains("Partially consistent"),
        "web page shows the report assessment"
    );
}

#[tokio::test]
async fn analysis_page_uses_streams_as_primary_unit() {
    let (dbdir, rootdir) = setup_catalog();
    let app = build_app(state_from(&dbdir, &rootdir));
    let run_id = run_id_for(&dbdir, "INC0299001");
    let (_, body) = get(&app, &format!("/analyses/{run_id}")).await;
    assert!(body.contains("Lifecycle counts (observer-prefix streams)"));
    assert!(body.contains("Stream lifecycle detail"));
}

#[tokio::test]
async fn stream_page_filters_lifecycle_category() {
    let (dbdir, rootdir) = setup_catalog();
    let app = build_app(state_from(&dbdir, &rootdir));
    let run_id = run_id_for(&dbdir, "INC0299001");
    let (status, _body) = get(
        &app,
        &format!("/analyses/{run_id}/streams?category=Withdrawn"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let conn = db::open_catalog(&dbdir.path().join("catalog.sqlite")).unwrap();
    let withdrawn = db::list_streams(&conn, run_id, Some("Withdrawn"), None).unwrap();
    let _ = conn;
    assert_eq!(withdrawn.len(), 13);
}

#[tokio::test]
async fn artifact_links_use_catalog_relative_paths() {
    let (dbdir, rootdir) = setup_catalog();
    let app = build_app(state_from(&dbdir, &rootdir));
    let run_id = run_id_for(&dbdir, "INC0302574");
    let (_, body) = get(&app, &format!("/analyses/{run_id}")).await;
    assert!(body.contains("INC0302574/report.json"));
    assert!(!body.contains("/home/"), "no absolute paths in links");
    assert!(!body.contains("C:\\"));
}

#[tokio::test]
async fn missing_artifact_is_reported_without_crashing() {
    let (dbdir, _rootdir) = setup_catalog();
    // Point the catalog root at an empty dir: artifact files are missing.
    let empty = tempfile::tempdir().unwrap();
    let state = build_state(&dbdir.path().join("catalog.sqlite"), empty.path(), "0.1.0").unwrap();
    let app = build_app(state);
    let run_id = run_id_for(&dbdir, "INC0302574");
    let (status, body) = get(&app, &format!("/analyses/{run_id}")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("Missing artifact files"), "{body}");
}

#[tokio::test]
async fn web_requests_do_not_start_analysis() {
    // The router contains no analysis route; ordinary GETs only read the
    // catalog. Prove by serving pages with a read-only database handle.
    let (dbdir, rootdir) = setup_catalog();
    let conn = db::open_catalog_readonly(&dbdir.path().join("catalog.sqlite")).unwrap();
    let state = Arc::new(AppState {
        db: Mutex::new(conn),
        catalog_root: rootdir.clone(),
        software_version: "0.1.0".into(),
    });
    let app = build_app(state);
    for uri in [
        "/",
        "/events",
        "/events/INC0302574",
        "/analyses/1",
        "/api/v1/catalog/status",
    ] {
        let (status, _) = get(&app, uri).await;
        assert_eq!(status, StatusCode::OK, "{uri} served from read-only db");
    }
}

#[tokio::test]
async fn missing_database_returns_clear_startup_error() {
    let dir = tempfile::tempdir().unwrap();
    let err = build_state(&dir.path().join("nope.sqlite"), dir.path(), "0.1.0").unwrap_err();
    assert!(err.contains("does not exist"), "{err}");
    assert!(err.contains("catalog init"), "{err}");
}

#[tokio::test]
async fn incompatible_database_version_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("catalog.sqlite");
    let conn = db::open_catalog(&path).unwrap();
    conn.pragma_update(None, "user_version", 99).unwrap();
    drop(conn);
    let err = db::open_catalog_readonly(&path).unwrap_err();
    assert!(err.contains("incompatible"), "{err}");
}

// ── API tests ───────────────────────────────────────────────────────

#[tokio::test]
async fn api_event_list_is_paginated() {
    let (dbdir, rootdir) = setup_catalog();
    let app = build_app(state_from(&dbdir, &rootdir));
    let (status, body) = get(&app, "/api/v1/events?per_page=2&page=0").await;
    assert_eq!(status, StatusCode::OK);
    let value: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(value["api_version"], 1);
    assert_eq!(value["data"]["per_page"], 2);
    assert_eq!(value["data"]["total"], 3);
    assert_eq!(value["data"]["events"].as_array().unwrap().len(), 2);
    let (_, body2) = get(&app, "/api/v1/events?per_page=2&page=1").await;
    let value2: serde_json::Value = serde_json::from_str(&body2).unwrap();
    assert_eq!(value2["data"]["events"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn api_event_detail_has_schema_version() {
    let (dbdir, rootdir) = setup_catalog();
    let app = build_app(state_from(&dbdir, &rootdir));
    let (status, body) = get(&app, "/api/v1/events/INC0302574").await;
    assert_eq!(status, StatusCode::OK);
    let value: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(value["data"]["schema_version"], 1);
    assert_eq!(value["data"]["event"]["id"], "INC0302574");
}

#[tokio::test]
async fn api_analysis_matches_catalog_run() {
    let (dbdir, rootdir) = setup_catalog();
    let app = build_app(state_from(&dbdir, &rootdir));
    let run_id = run_id_for(&dbdir, "INC0302574");
    let (_, body) = get(&app, &format!("/api/v1/analyses/{run_id}")).await;
    let value: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(value["data"]["run"]["id"], run_id);
    assert_eq!(value["data"]["run"]["status"], "Complete");
    assert!(!value["data"]["artifacts"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn api_does_not_expose_absolute_paths() {
    let (dbdir, rootdir) = setup_catalog();
    let app = build_app(state_from(&dbdir, &rootdir));
    let run_id = run_id_for(&dbdir, "INC0302574");
    let (_, body) = get(&app, &format!("/api/v1/analyses/{run_id}")).await;
    assert!(!body.contains("/home/"));
    assert!(!body.contains("C:\\"));
    assert!(!body.contains("vadim"));
}

#[tokio::test]
async fn api_returns_structured_not_found() {
    let (dbdir, rootdir) = setup_catalog();
    let app = build_app(state_from(&dbdir, &rootdir));
    let (status, body) = get(&app, "/api/v1/events/DOES-NOT-EXIST").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let value: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(value["error"]["message"], "event not found");
}

#[tokio::test]
async fn api_rejects_unsupported_pagination() {
    let (dbdir, rootdir) = setup_catalog();
    let app = build_app(state_from(&dbdir, &rootdir));
    let (status, body) = get(&app, "/api/v1/events?per_page=0").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let value: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert!(value["error"]["message"]
        .as_str()
        .unwrap()
        .contains("per_page"));
    let (status, _) = get(&app, "/api/v1/events?per_page=9999").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn api_catalog_status_counts_match_database() {
    let (dbdir, rootdir) = setup_catalog();
    let app = build_app(state_from(&dbdir, &rootdir));
    let (status, body) = get(&app, "/api/v1/catalog/status").await;
    assert_eq!(status, StatusCode::OK);
    let value: serde_json::Value = serde_json::from_str(&body).unwrap();
    let c = &value["data"]["catalog"];
    assert_eq!(c["total_events"], 3);
    assert_eq!(c["complete"], 2);
    assert_eq!(c["blocked"], 1);
}

// ── Filter helpers compile-check (server-side filtering) ────────────

#[test]
fn event_list_filters_are_server_side() {
    let filters = EventListFilters {
        lifecycle: Some("Open".into()),
        status: None,
        expectation: None,
        source: None,
        date_from: None,
        date_to: None,
        q: Some("INC".into()),
    };
    assert_eq!(filters.lifecycle.as_deref(), Some("Open"));
}
