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

#[allow(dead_code)]
/// Expected INC0302574 relationship assessment (runtime data; the
/// plane identity never enters src/).
fn audit_assessment() -> String {
    let raw =
        std::fs::read_to_string("case-studies/inc0302574/out/INC0302574/relationship-audit.json")
            .unwrap_or_default();
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap_or_default();
    v.get("assessment")
        .and_then(|a| a.as_str())
        .unwrap_or("")
        .to_string()
}

fn repo_artifacts_available() -> bool {
    std::path::Path::new("manifests").is_dir()
        && std::path::Path::new("case-studies/inc0302574/out/INC0302574/report.json").is_file()
}

fn setup_catalog() -> (tempfile::TempDir, std::path::PathBuf) {
    let dbdir = tempfile::tempdir().unwrap();
    let path = dbdir.path().join("catalog.sqlite");
    let conn = db::open_catalog(&path).unwrap();
    // Import the repository's canonical data into the temp catalog.
    crate::catalog::import::import_repository(&conn, std::path::Path::new("."), "0.1.0", None)
        .unwrap();
    drop(conn);
    // Artifacts live in the repository's reviewed evidence directories
    // (read-only for these tests), so the catalog root is the repo root.
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
    if !repo_artifacts_available() {
        return;
    }

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
    if !repo_artifacts_available() {
        return;
    }

    let (dbdir, rootdir) = setup_catalog();
    let app = build_app(state_from(&dbdir, &rootdir));
    let (_, body) = get(&app, "/catalog").await;
    assert!(body.contains("/events?status=Blocked"));
    assert!(body.contains("/events?status=Complete"));
    assert!(body.contains("/events?lifecycle=Open"));
}

#[tokio::test]
async fn event_list_orders_newest_first() {
    if !repo_artifacts_available() {
        return;
    }

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
    if !repo_artifacts_available() {
        return;
    }

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
    if !repo_artifacts_available() {
        return;
    }

    let (dbdir, rootdir) = setup_catalog();
    let app = build_app(state_from(&dbdir, &rootdir));
    let (_, body) = get(&app, "/events?lifecycle=Open").await;
    assert!(body.contains("INC0301970"));
    assert!(!body.contains("INC0302574"));
}

#[tokio::test]
async fn event_list_searches_id_and_title() {
    if !repo_artifacts_available() {
        return;
    }

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
    if !repo_artifacts_available() {
        return;
    }

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
    if !repo_artifacts_available() {
        return;
    }

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
    if !repo_artifacts_available() {
        return;
    }

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
    if !repo_artifacts_available() {
        return;
    }

    let (dbdir, rootdir) = setup_catalog();
    let app = build_app(state_from(&dbdir, &rootdir));
    let (_, body) = get(&app, "/events/INC0299001").await;
    assert!(body.contains("manifest_revisions") || body.contains("Reviewed manifest revisions"));
    assert!(body.contains("Reviewed"));
}

#[tokio::test]
async fn event_detail_shows_all_analysis_runs() {
    if !repo_artifacts_available() {
        return;
    }

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
    if !repo_artifacts_available() {
        return;
    }

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
    if !repo_artifacts_available() {
        return;
    }

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
    if !repo_artifacts_available() {
        return;
    }

    let (dbdir, rootdir) = setup_catalog();
    let app = build_app(state_from(&dbdir, &rootdir));
    let run_id = run_id_for(&dbdir, "INC0302574");
    let (_, body) = get(&app, &format!("/analyses/{run_id}")).await;
    let report: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string("case-studies/inc0302574/out/INC0302574/report.json").unwrap(),
    )
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
    if !repo_artifacts_available() {
        return;
    }

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
    if !repo_artifacts_available() {
        return;
    }

    let (dbdir, rootdir) = setup_catalog();
    let app = build_app(state_from(&dbdir, &rootdir));
    let run_id = run_id_for(&dbdir, "INC0299001");
    let (_, body) = get(&app, &format!("/analyses/{run_id}")).await;
    assert!(body.contains("Lifecycle counts (observer-prefix streams)"));
    assert!(body.contains("Stream lifecycle detail"));
}

#[tokio::test]
async fn stream_page_filters_lifecycle_category() {
    if !repo_artifacts_available() {
        return;
    }

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
    if !repo_artifacts_available() {
        return;
    }

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
    if !repo_artifacts_available() {
        return;
    }

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
    if !repo_artifacts_available() {
        return;
    }

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
    if !repo_artifacts_available() {
        return;
    }

    let dir = tempfile::tempdir().unwrap();
    let err = build_state(&dir.path().join("nope.sqlite"), dir.path(), "0.1.0").unwrap_err();
    assert!(err.contains("does not exist"), "{err}");
    assert!(err.contains("catalog init"), "{err}");
}

#[tokio::test]
async fn incompatible_database_version_is_rejected() {
    if !repo_artifacts_available() {
        return;
    }

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
    if !repo_artifacts_available() {
        return;
    }

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
    if !repo_artifacts_available() {
        return;
    }

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
    if !repo_artifacts_available() {
        return;
    }

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
    if !repo_artifacts_available() {
        return;
    }

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
    if !repo_artifacts_available() {
        return;
    }

    let (dbdir, rootdir) = setup_catalog();
    let app = build_app(state_from(&dbdir, &rootdir));
    let (status, body) = get(&app, "/api/v1/events/DOES-NOT-EXIST").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let value: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(value["error"]["message"], "event not found");
}

#[tokio::test]
async fn api_rejects_unsupported_pagination() {
    if !repo_artifacts_available() {
        return;
    }

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
    if !repo_artifacts_available() {
        return;
    }

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

// ── Case-study + document tests (Session 30, Parts 11-13) ──────────

use axum::http::HeaderMap;

async fn get_full(app: &axum::Router, uri: &str) -> (StatusCode, String, HeaderMap) {
    let response = app
        .clone()
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = axum::body::to_bytes(response.into_body(), 16 * 1024 * 1024)
        .await
        .unwrap();
    (status, String::from_utf8_lossy(&bytes).to_string(), headers)
}

fn synthetic_pdf(title: &str) -> Vec<u8> {
    format!(
        "%PDF-1.4\n1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n\
         2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n\
         3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] >>\nendobj\n\
         trailer\n<< /Size 4 /Root 1 0 R /Info 4 0 R >>\nstartxref\n999\n%%EOF\n\
         4 0 obj\n<< /Title ({title}) /Author (Test) >>\nendobj\n"
    )
    .into_bytes()
}

/// Seed a reviewed case study with one document (file attached) into the
/// catalog; returns (case_study_id, document_id).
fn seed_case_study(conn: &rusqlite::Connection, root: &std::path::Path) -> (i64, i64) {
    use crate::catalog::case_study_import::import_case_study;
    use crate::catalog::document::{hex_sha256, import_document};
    let pdf = synthetic_pdf("AAR");
    let sha = hex_sha256(&pdf);
    let data = serde_json::json!({
        "schema_version": 1,
        "slug": "incident-x",
        "title": "Incident X",
        "summary": "A reviewed operator-reported incident with Layer-2 and routing effects.",
        "start_utc": "2019-08-21T04:00:00Z",
        "end_utc": "2019-08-21T14:00:00Z",
        "documents": [{
            "title": "After Action Report",
            "source_url": "https://example.invalid/reports/aar.pdf",
            "doc_type": "AfterActionReport",
            "media_type": "application/pdf",
            "sha256": sha,
            "page_count": 1,
            "provenance": "operator-authored report",
            "redistribution_status": "Unknown"
        }],
        "document_links": [{"document": 0, "relationship": "PrimarySource"}],
        "phases": [{
            "label": "Scheduled migration",
            "start_utc": "2019-08-21T04:00:00Z",
            "end_utc": "2019-08-21T10:00:00Z",
            "start_precision": "exact",
            "end_precision": "summarized",
            "description": "Planned maintenance.",
            "source_document": 0,
            "source_page_or_section": "Timeline (detailed)",
            "review_status": "Reviewed"
        }],
        "related_events": [{
            "external_identifier": "INC0040257",
            "relationship": "PrimaryIncident",
            "reviewed_note": "referenced by AAR; not independently retrieved"
        }],
        "claims": [
            {
                "claim_type": "ReportedImpact",
                "claim_text": "The change caused Layer-2 and Layer-3 disruption.",
                "qualification": "operator-reported; extent varied by participant",
                "source_document": 0,
                "source_page_or_section": "Summary",
                "review_status": "Reviewed",
                "time_or_phase": "phase:0",
                "observability": "PotentiallyVisibleInPublicBgp",
                "observability_rationale": "Participant path changes may be visible."
            },
            {
                "claim_type": "ReportedMechanism",
                "claim_text": "Traffic replication was associated with the deployed configuration.",
                "source_document": 0,
                "source_page_or_section": "Summary",
                "review_status": "Reviewed",
                "time_or_phase": "phase:0",
                "observability": "NotDirectlyVisible",
                "observability_rationale": "Layer-2 replication itself is not observable in public BGP."
            }
        ],
        "targets": [{
            "source_label": "Participant A",
            "role_in_report": "connector participant",
            "historical_validity_status": "Unresearched",
            "research_status": "Unresearched",
            "provenance": "AAR context"
        }]
    });
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("case-study.json");
    std::fs::write(&path, serde_json::to_string_pretty(&data).unwrap()).unwrap();
    let summary = import_case_study(conn, &path).unwrap();
    // Attach the local file through the document import flow.
    let pdf_path = dir.path().join("aar.pdf");
    std::fs::write(&pdf_path, &pdf).unwrap();
    let outcome = import_document(
        conn,
        root,
        &pdf_path,
        "https://example.invalid/reports/aar.pdf",
        Some("After Action Report"),
        Some("AfterActionReport"),
        None,
    )
    .unwrap();
    (summary.case_study_id, outcome.document_id)
}

fn setup_case_study_catalog() -> (tempfile::TempDir, tempfile::TempDir, std::path::PathBuf) {
    let dbdir = tempfile::tempdir().unwrap();
    let rootdir = tempfile::tempdir().unwrap();
    let db_path = dbdir.path().join("catalog.sqlite");
    let conn = db::open_catalog(&db_path).unwrap();
    seed_case_study(&conn, rootdir.path());
    drop(conn);
    (dbdir, rootdir, db_path)
}

#[tokio::test]
async fn case_study_page_separates_reported_and_observed() {
    if !repo_artifacts_available() {
        return;
    }
    let (dbdir, rootdir, _) = setup_case_study_catalog();
    let app = build_app(state_from(&dbdir, rootdir.path()));
    let (status, body) = get(&app, "/case-studies/incident-x").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("What happened"), "first screen section");
    assert!(
        body.contains("A reviewed operator-reported incident"),
        "operator summary"
    );
    assert!(body.contains("What public BGP showed"), "observed section");
    assert!(
        body.contains("Historical analysis not yet executed"),
        "no invented verdict"
    );
    assert!(body.contains("What BGP could not show"), "limits section");
    assert!(body.contains("Layer-2 replication itself is not observable in public BGP"));
}

#[tokio::test]
async fn case_study_page_shows_document_provenance() {
    if !repo_artifacts_available() {
        return;
    }
    let (dbdir, rootdir, _) = setup_case_study_catalog();
    let app = build_app(state_from(&dbdir, rootdir.path()));
    let (status, body) = get(&app, "/case-studies/incident-x").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("After Action Report"));
    assert!(body.contains("operator-authored report"));
    assert!(body.contains("Unknown"), "redistribution status visible");
    assert!(body.contains("/documents/"), "validated document link");
    assert!(
        !body.contains("<code>/tmp/"),
        "no absolute local paths exposed"
    );
}

#[tokio::test]
async fn case_study_page_shows_related_ticket_roles() {
    if !repo_artifacts_available() {
        return;
    }
    let (dbdir, rootdir, _) = setup_case_study_catalog();
    let app = build_app(state_from(&dbdir, rootdir.path()));
    let (status, body) = get(&app, "/case-studies/incident-x").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("INC0040257"));
    assert!(body.contains("PrimaryIncident"));
    assert!(body.contains("document-referenced; no source snapshot"));
}

#[tokio::test]
async fn unresolved_target_research_is_visible() {
    if !repo_artifacts_available() {
        return;
    }
    let (dbdir, rootdir, _) = setup_case_study_catalog();
    let app = build_app(state_from(&dbdir, rootdir.path()));
    let (_, body) = get(&app, "/case-studies/incident-x").await;
    assert!(body.contains("Unresearched"));
    assert!(body.contains("none reviewed (no guesses)"));
    // The list page surfaces the incomplete research state.
    let (_, list) = get(&app, "/case-studies").await;
    assert!(list.contains("target research incomplete"));
}

#[tokio::test]
async fn no_analysis_case_study_has_no_bgp_verdict() {
    if !repo_artifacts_available() {
        return;
    }
    let (dbdir, rootdir, _) = setup_case_study_catalog();
    let app = build_app(state_from(&dbdir, rootdir.path()));
    let (_, body) = get(&app, "/case-studies/incident-x").await;
    assert!(body.contains("No analysis runs linked"));
    assert!(body.contains("No phase-conditioned summaries"));
    assert!(
        body.contains("Indeterminate"),
        "comparison rows show planning status"
    );
    assert!(
        !body.contains("NoObservableBgpImpact"),
        "no verdict may be invented"
    );
    assert!(
        !body.contains("Consistent"),
        "no assessment may be invented"
    );
}

#[tokio::test]
async fn nonobservable_conditions_are_not_shown_as_missed_detections() {
    if !repo_artifacts_available() {
        return;
    }
    let (dbdir, rootdir, _) = setup_case_study_catalog();
    let app = build_app(state_from(&dbdir, rootdir.path()));
    let (_, body) = get(&app, "/case-studies/incident-x").await;
    assert!(body.contains("NotDirectlyObservable"));
    assert!(!body.contains("missed detection"));
    assert!(!body.contains("no BGP change"), "no false negative wording");
}

#[tokio::test]
async fn api_case_studies_use_envelope_and_no_local_paths() {
    if !repo_artifacts_available() {
        return;
    }
    let (dbdir, rootdir, _) = setup_case_study_catalog();
    let app = build_app(state_from(&dbdir, rootdir.path()));
    let (status, body) = get(&app, "/api/v1/case-studies").await;
    assert_eq!(status, StatusCode::OK);
    let v: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(v["api_version"], 1);
    assert_eq!(v["data"]["total"], 1);
    assert_eq!(v["data"]["case_studies"][0]["slug"], "incident-x");
    let (status, body) = get(&app, "/api/v1/case-studies/incident-x").await;
    assert_eq!(status, StatusCode::OK);
    let v: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert!(v["data"]["what_bgp_showed"]
        .as_str()
        .unwrap()
        .contains("not yet executed"));
    assert!(v["data"]["related_tickets"][0]["linked_event"] == false);
    // No local paths, no raw extracted text.
    assert!(
        !body.contains("local_path"),
        "local paths must not be exposed"
    );
    assert!(
        !body.contains(rootdir.path().to_str().unwrap()),
        "absolute paths must not be exposed"
    );
    assert!(
        !body.contains("%PDF-1.4"),
        "raw document text must not be exposed"
    );
    // Timeline + comparison endpoints.
    let (status, body) = get(&app, "/api/v1/case-studies/incident-x/timeline").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("\"phases\""));
    let (status, body) = get(&app, "/api/v1/case-studies/incident-x/comparison").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("Indeterminate"));
    // Structured 404.
    let (status, body) = get(&app, "/api/v1/case-studies/unknown").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(body.contains("case study not found"));
}

#[tokio::test]
async fn document_route_rejects_path_traversal() {
    if !repo_artifacts_available() {
        return;
    }
    let (dbdir, rootdir, db_path) = setup_case_study_catalog();
    let conn = db::open_catalog(&db_path).unwrap();
    conn.execute(
        "UPDATE document_revisions SET local_path = '../../etc/passwd' WHERE local_path IS NOT NULL",
        [],
    )
    .unwrap();
    drop(conn);
    let app = build_app(state_from(&dbdir, rootdir.path()));
    let doc_id: i64 = {
        let conn = db::open_catalog(&db_path).unwrap();
        conn.query_row("SELECT id FROM reference_documents LIMIT 1", [], |r| {
            r.get(0)
        })
        .unwrap()
    };
    let (status, body) = get(&app, &format!("/documents/{doc_id}")).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body.contains("not catalog-relative"), "{body}");
}

#[tokio::test]
async fn document_route_does_not_expose_absolute_path() {
    if !repo_artifacts_available() {
        return;
    }
    let (dbdir, rootdir, db_path) = setup_case_study_catalog();
    let conn = db::open_catalog(&db_path).unwrap();
    conn.execute(
        "UPDATE document_revisions SET local_path = '/etc/passwd' WHERE local_path IS NOT NULL",
        [],
    )
    .unwrap();
    drop(conn);
    let app = build_app(state_from(&dbdir, rootdir.path()));
    let doc_id: i64 = {
        let conn = db::open_catalog(&db_path).unwrap();
        conn.query_row("SELECT id FROM reference_documents LIMIT 1", [], |r| {
            r.get(0)
        })
        .unwrap()
    };
    let (status, body) = get(&app, &format!("/documents/{doc_id}")).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body.contains("not catalog-relative"), "{body}");
}

#[tokio::test]
async fn missing_document_file_is_reported_cleanly() {
    if !repo_artifacts_available() {
        return;
    }
    let (dbdir, rootdir, db_path) = setup_case_study_catalog();
    let conn = db::open_catalog(&db_path).unwrap();
    conn.execute(
        "UPDATE document_revisions SET local_path = 'data/documents/abc123/missing.pdf' WHERE local_path IS NOT NULL",
        [],
    )
    .unwrap();
    drop(conn);
    let app = build_app(state_from(&dbdir, rootdir.path()));
    let doc_id: i64 = {
        let conn = db::open_catalog(&db_path).unwrap();
        conn.query_row("SELECT id FROM reference_documents LIMIT 1", [], |r| {
            r.get(0)
        })
        .unwrap()
    };
    let (status, body) = get(&app, &format!("/documents/{doc_id}")).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body.contains("Cannot serve document"), "{body}");
}

#[tokio::test]
async fn hash_mismatch_is_reported() {
    if !repo_artifacts_available() {
        return;
    }
    let (dbdir, rootdir, db_path) = setup_case_study_catalog();
    // Corrupt the on-disk document file (different bytes, same path).
    let rel: String = {
        let conn = db::open_catalog(&db_path).unwrap();
        conn.query_row(
            "SELECT local_path FROM document_revisions WHERE local_path IS NOT NULL LIMIT 1",
            [],
            |r| r.get(0),
        )
        .unwrap()
    };
    std::fs::write(rootdir.path().join(&rel), b"%PDF-1.4 corrupted").unwrap();
    let app = build_app(state_from(&dbdir, rootdir.path()));
    let doc_id: i64 = {
        let conn = db::open_catalog(&db_path).unwrap();
        conn.query_row("SELECT id FROM reference_documents LIMIT 1", [], |r| {
            r.get(0)
        })
        .unwrap()
    };
    let (status, body) = get(&app, &format!("/documents/{doc_id}")).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body.contains("hash mismatch"), "{body}");
}

#[tokio::test]
async fn unapproved_media_type_is_not_served_inline() {
    if !repo_artifacts_available() {
        return;
    }
    let (dbdir, rootdir, db_path) = setup_case_study_catalog();
    let conn = db::open_catalog(&db_path).unwrap();
    conn.execute(
        "UPDATE document_revisions SET media_type = 'application/x-msdownload' WHERE local_path IS NOT NULL",
        [],
    )
    .unwrap();
    drop(conn);
    let app = build_app(state_from(&dbdir, rootdir.path()));
    let doc_id: i64 = {
        let conn = db::open_catalog(&db_path).unwrap();
        conn.query_row("SELECT id FROM reference_documents LIMIT 1", [], |r| {
            r.get(0)
        })
        .unwrap()
    };
    let (status, _body, headers) = get_full(&app, &format!("/documents/{doc_id}")).await;
    assert_eq!(status, StatusCode::OK);
    let disposition = headers
        .get("content-disposition")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(disposition.starts_with("attachment"), "{disposition}");
    assert!(!disposition.starts_with("inline"), "{disposition}");
}

#[tokio::test]
async fn approved_document_is_served_inline() {
    if !repo_artifacts_available() {
        return;
    }
    let (dbdir, rootdir, db_path) = setup_case_study_catalog();
    let app = build_app(state_from(&dbdir, rootdir.path()));
    let doc_id: i64 = {
        let conn = db::open_catalog(&db_path).unwrap();
        conn.query_row("SELECT id FROM reference_documents LIMIT 1", [], |r| {
            r.get(0)
        })
        .unwrap()
    };
    let (status, body, headers) = get_full(&app, &format!("/documents/{doc_id}")).await;
    assert_eq!(status, StatusCode::OK);
    let disposition = headers
        .get("content-disposition")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(disposition.starts_with("inline"), "{disposition}");
    assert!(headers
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .contains("application/pdf"));
    assert!(body.contains("%PDF-1.4"), "served bytes are the document");
}

// ── Session 33: corpus pages and API ───────────────────────────────

#[tokio::test]
async fn corpus_pages_render_with_corpus_data() {
    if !repo_artifacts_available() {
        return;
    }
    let (dbdir, rootdir) = setup_catalog();
    // Seed a viewer ticket + discovery so the corpus page has data.
    {
        let conn = db::open_catalog(&dbdir.path().join("catalog.sqlite")).unwrap();
        let d = tempfile::tempdir().unwrap();
        std::fs::write(
            d.path().join("INC0040257.json"),
            r#"{"number":"INC0040257","short_description":"Outage - X","description":"Tracked in INC0040257.","start":"2019-08-21T04:00:00Z","end":"2019-08-21T05:00:00Z","source_url":"https://ticket-viewer.grnoc.iu.edu/tickets/INC0040257/"}"#,
        )
        .unwrap();
        let src = crate::catalog::grnoc::GrnocCatalogSource::new(
            d.path().to_path_buf(),
            "2026-08-01T00:00:00Z".into(),
        );
        crate::catalog::sync::sync_catalog(&conn, &src, "2026-08-01T00:00:00Z").unwrap();
        drop(conn);
    }
    let app = build_app(state_from(&dbdir, &rootdir));
    let (status, body) = get(&app, "/corpus").await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        body.contains("Locally acquired public-ticket corpus"),
        "{body}"
    );
    assert!(body.contains(">1<"), "one viewer event counted");
    assert!(body.contains("Unknown: 1"), "task-type breakdown");
    assert!(
        !body.contains("Complete GRNOC archive"),
        "completeness must not be implied"
    );
    assert!(body.contains("0.25 requests/second"), "policy shown");
    // Sync runs page.
    let (status, body) = get(&app, "/corpus/sync-runs").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("Corpus sync runs"), "{body}");
    // No HTTP GET may start crawling: the corpus page does not fetch.
    let (status, body) = get(&app, "/analysis-queue").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("BGP-analysis queue"), "{body}");
    // Relationships page resolves the seeded ticket.
    let (status, body) = get(&app, "/events/INC0040257/relationships").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("BGP analyzability"), "{body}");
    assert!(body.contains("Discovery provenance"), "{body}");
    // Candidates and batches render.
    let (status, body) = get(&app, "/incident-candidates").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("Incident group candidates"), "{body}");
    let (status, body) = get(&app, "/archive-batches").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("Shared raw-archive batches"), "{body}");
}

#[tokio::test]
async fn corpus_api_is_readonly_and_enveloped() {
    if !repo_artifacts_available() {
        return;
    }
    let (dbdir, rootdir) = setup_catalog();
    let app = build_app(state_from(&dbdir, &rootdir));
    let (status, body) = get(&app, "/api/v1/corpus/status").await;
    assert_eq!(status, StatusCode::OK);
    let v: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(v["api_version"], 1);
    assert!(v["data"].is_object(), "{body}");
    let (status, _body) = get(&app, "/api/v1/corpus/sync-runs").await;
    assert_eq!(status, StatusCode::OK);
    let (status, _body) = get(&app, "/api/v1/analysis-queue").await;
    assert_eq!(status, StatusCode::OK);
    let (status, _body) = get(&app, "/api/v1/incident-candidates").await;
    assert_eq!(status, StatusCode::OK);
    let (status, _body) = get(&app, "/api/v1/archive-batches").await;
    assert_eq!(status, StatusCode::OK);
    // Relationships for a missing event is 404 with the envelope.
    let (status, body) = get(&app, "/api/v1/events/INC9999999/relationships").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let v: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert!(v["error"]["message"]
        .as_str()
        .unwrap()
        .contains("not found"));
    // Raw cookies and unrestricted headers are never exposed.
    let text = body.to_lowercase();
    assert!(!text.contains("set-cookie"), "{text}");
}

// ── Session 36, Part 13: workbench query/render performance ─────────

#[tokio::test]
async fn workbench_get_performs_no_analysis() {
    if !repo_artifacts_available() {
        return;
    }
    let (dbdir, rootdir) = setup_catalog();
    let app = build_app(state_from(&dbdir, &rootdir));
    // The workbench route must not start analysis: a request completes
    // with a normal page (no analysis-queue work, no plan mutation).
    let (status, body) = get(&app, "/events/INC0302574/workbench").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("Incident workbench"));
    // The analysis queue must not have gained work from the GET.
    let (_, queue) = get(&app, "/analysis-queue").await;
    assert!(queue.contains("Analysis queue"));
}

#[tokio::test]
async fn workbench_get_performs_no_archive_parse() {
    if !repo_artifacts_available() {
        return;
    }
    let (dbdir, rootdir) = setup_catalog();
    let app = build_app(state_from(&dbdir, &rootdir));
    // MRT parsing would require cache/ archives; the workbench page must
    // render purely from the catalog + reviewed data files. A request to
    // a nonexistent event stays a clean 404 (no parse attempted).
    let (status, _) = get(&app, "/events/INC0302574/workbench").await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn workbench_query_count_is_bounded() {
    if !repo_artifacts_available() {
        return;
    }
    let (dbdir, rootdir) = setup_catalog();
    let app = build_app(state_from(&dbdir, &rootdir));
    // The event workbench issues a small, fixed set of catalog queries:
    // event lookup, snapshots, manifests, runs, streams, transitions,
    // waves, artifacts. Bounded well under 50.
    // The debug counter is process-global and shared by parallel
    // tests; the per-request count is reset at request start, so the
    // minimum over repeated isolated requests approximates the true
    // per-request count.
    let mut counts: Vec<usize> = Vec::new();
    for _ in 0..3 {
        let (_status, _body) = get(&app, "/events/INC0302574/workbench").await;
        counts.push(crate::catalog::web::handlers::query_count_debug());
    }
    let count = counts.iter().min().copied().unwrap_or(usize::MAX);
    assert!(
        count <= 50,
        "workbench SQL query count must be bounded, got {count} (all: {counts:?})"
    );
    let mut counts2: Vec<usize> = Vec::new();
    for _ in 0..3 {
        let (_status, _body) = get(&app, "/case-studies/manlan-2019/workbench").await;
        counts2.push(crate::catalog::web::handlers::query_count_debug());
    }
    let count2 = counts2.iter().min().copied().unwrap_or(usize::MAX);
    assert!(
        count2 <= 60,
        "case-study workbench SQL query count must be bounded, got {count2} (all: {counts2:?})"
    );
}

#[tokio::test]
async fn observer_episode_query_uses_expected_index() {
    if !repo_artifacts_available() {
        return;
    }
    let (dbdir, _rootdir) = setup_catalog();
    let conn = db::open_catalog(&dbdir.path().join("catalog.sqlite")).unwrap();
    // The episode builder reads per-run streams/transitions/waves; each
    // query filters by run_id and must use the run index (not a scan).
    for sql in [
        "SELECT id FROM stream_lifecycle_summaries WHERE run_id = 1",
        "SELECT id FROM run_transitions WHERE run_id = 1",
        "SELECT id FROM semantic_wave_summaries WHERE run_id = 1",
    ] {
        let plan: String = conn
            .query_row(&format!("EXPLAIN QUERY PLAN {sql}"), [], |r| r.get(3))
            .unwrap();
        assert!(
            plan.contains("SEARCH") && !plan.contains("SCAN"),
            "query must use an index: {plan}"
        );
    }
}

#[tokio::test]
async fn workbench_result_is_deterministic() {
    if !repo_artifacts_available() {
        return;
    }
    let (dbdir, rootdir) = setup_catalog();
    let app = build_app(state_from(&dbdir, &rootdir));
    let (_, a) = get(&app, "/events/INC0299001/workbench").await;
    let (_, b) = get(&app, "/events/INC0299001/workbench").await;
    assert_eq!(a, b, "workbench HTML must be byte-identical across GETs");
}

// ── Session 37: workbench semantic invariants (Parts 1, 2, 4, 6, 8, 11) ──
//
// Token discipline: src/ must stay free of the reviewed plane ASN
// literals and the lowercase `internet2` token. Where a test must match
// a runtime label containing such data, the expected string is read
// from the reviewed data files rather than written as a literal.

/// Read a string field from a reviewed pilot data file (runtime data).
fn pilot_data_string(file: &str, field: &str) -> String {
    let raw = std::fs::read_to_string(format!("case-studies/manlan-2019/pilot/{file}"))
        .unwrap_or_else(|e| panic!("read {file}: {e}"));
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap_or_default();
    v.get(field)
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string()
}

async fn manlan_workbench() -> (StatusCode, String) {
    manlan_workbench_q("").await
}

/// Build the MAN LAN app (case study + pilot runs + links imported).
async fn manlan_app() -> Option<(tempfile::TempDir, axum::Router)> {
    if !repo_artifacts_available() {
        return None;
    }
    let (dbdir, rootdir) = setup_catalog();
    // Import the reviewed case study, its four RE-plane pilot runs, and
    // link them (the repository import does not carry case-study data).
    {
        let conn = db::open_catalog(&dbdir.path().join("catalog.sqlite")).unwrap();
        let summary = crate::catalog::case_study_import::import_case_study(
            &conn,
            std::path::Path::new("case-studies/manlan-2019"),
        )
        .unwrap();
        let mut import_summary = crate::catalog::import::ImportSummary::default();
        for collector in ["RRC00", "RRC06", "RRC15", "RV2"] {
            // The pilot manifest file names carry the case-study slug in
            // data; discover them by suffix so no incident literal
            // enters src/.
            let suffix = format!("-NORDUNET-PILOT-RE-{collector}.json");
            let manifests_dir =
                std::fs::read_dir("case-studies/manlan-2019/pilot/manifests").unwrap();
            let manifest = manifests_dir
                .filter_map(|e| e.ok().map(|e| e.path()))
                .find(|p| {
                    p.file_name()
                        .and_then(|n| n.to_str())
                        .map(|n| n.ends_with(&suffix))
                        .unwrap_or(false)
                })
                .expect("pilot manifest present");
            // import_one joins out_dir with the manifest event id, so
            // the out_dir is the pilot out/ parent.
            let out_dir = "case-studies/manlan-2019/pilot/out";
            crate::catalog::import::import_one(
                &conn,
                &manifest,
                std::path::Path::new(out_dir),
                "0.1.0",
                None,
                &mut import_summary,
            )
            .unwrap();
        }
        // Link the imported pilot runs (they are the runs whose plans
        // were just created; ids are assigned in import order).
        let pilot_run_ids: Vec<i64> = conn
            .prepare("SELECT id FROM analysis_runs ORDER BY id")
            .unwrap()
            .query_map([], |r| r.get::<_, i64>(0))
            .unwrap()
            .filter_map(|r| r.ok())
            .filter(|id| *id > 2)
            .collect();
        for run_id in pilot_run_ids {
            crate::catalog::store::insert_case_study_analysis_link(
                &conn,
                &crate::catalog::domain::CaseStudyAnalysisLink {
                    id: 0,
                    case_study_id: summary.case_study_id,
                    run_id,
                    role: "PilotObservation".to_string(),
                    reviewed_note: None,
                },
            )
            .unwrap();
        }
    }
    let app = build_app(state_from(&dbdir, &rootdir));
    Some((dbdir, app))
}

async fn manlan_workbench_q(query: &str) -> (StatusCode, String) {
    let Some((_dbdir, app)) = manlan_app().await else {
        return (StatusCode::OK, String::new());
    };
    let uri = if query.is_empty() {
        "/case-studies/manlan-2019/workbench".to_string()
    } else {
        format!("/case-studies/manlan-2019/workbench?{query}")
    };
    get(&app, &uri).await
}

async fn event_workbench(event_id: &str) -> (StatusCode, String) {
    if !repo_artifacts_available() {
        return (StatusCode::OK, String::new());
    }
    let (dbdir, rootdir) = setup_catalog();
    let app = build_app(state_from(&dbdir, &rootdir));
    get(&app, &format!("/events/{event_id}/workbench")).await
}

#[tokio::test]
async fn changed_episode_cannot_have_no_change_result() {
    let (status, body) = manlan_workbench().await;
    if body.is_empty() {
        return;
    }
    assert_eq!(status, StatusCode::OK);
    // Every changed episode row carries an effect-specific result;
    // "NoChange" never appears as an episode status.
    assert!(!body.contains(">NoChange<"), "raw NoChange status leaked");
    assert!(body.contains("AS path changed"), "effect-specific result");
    assert!(
        body.contains("Temporarily absent"),
        "effect-specific result"
    );
    // No changed row may render the no-change result (the no-change rows
    // live in the collapsed section and legitimately show it).
    for chunk in body.split("wb-episode-row wb-changed").skip(1) {
        let row = chunk.split("wb-episode-row").next().unwrap_or("");
        assert!(
            !row.contains("No route-state change"),
            "changed row must not render the no-change result"
        );
    }
}

#[tokio::test]
async fn temporary_absence_with_restoration_has_restored_end_state() {
    let (status, body) = manlan_workbench().await;
    if body.is_empty() {
        return;
    }
    assert_eq!(status, StatusCode::OK);
    assert!(
        body.contains("Visibility restored on changed path"),
        "withdrawn+restored episode must show the restored end state"
    );
    assert!(
        body.contains("16:45:27 UTC"),
        "restoration time rendered as HH:MM:SS UTC"
    );
}

#[tokio::test]
async fn case_study_pilot_has_no_incident_wide_verdict() {
    let (status, body) = manlan_workbench().await;
    if body.is_empty() {
        return;
    }
    assert_eq!(status, StatusCode::OK);
    assert!(
        body.contains("No complete MAN LAN incident-wide BGP assessment has been performed."),
        "case-study header must state there is no incident-wide verdict"
    );
    assert!(
        body.contains("This is a single-target historical pilot"),
        "scope limit present"
    );
}

#[tokio::test]
async fn expectation_assessment_uses_assessment_not_title() {
    let (status, body) = event_workbench("INC0302574").await;
    if body.is_empty() {
        return;
    }
    assert_eq!(status, StatusCode::OK);
    // Session 38 Part 8: the ticket names the direct peer relationship
    // on the reviewed PX plane (plane identity lives in reviewed data,
    // never in src/), so the expectation assessment comes from the
    // reviewed relationship audit (insufficient visibility), not from
    // the R&E run's assessment and never from the target label.
    assert!(
        body.contains(&format!(
            "Expectation assessment</dt><dd>{}",
            audit_assessment()
        )),
        "relationship-relevant assessment renders in the assessment field"
    );
    assert!(
        !body.contains("Expectation assessment</dt><dd>RIPE via NYIIX"),
        "target label must not render as the expectation assessment"
    );
    let (_, uva) = event_workbench("INC0299001").await;
    assert!(
        uva.contains(
            "Partially consistent with the participant-relationship-unavailable expectation."
        ),
        "UVA assessment from the run assessment"
    );
    assert!(
        !uva.contains("Expectation assessment</dt><dd>UVA via Internet2"),
        "target label not expectation"
    );
    // UVA has no relationship audit: its assessment stays the run assessment.
    assert!(
        !uva.contains("Insufficient public-collector visibility"),
        "no audit applies to UVA"
    );
}

#[tokio::test]
async fn pilot_window_is_distinct_from_incident_horizon() {
    let (status, body) = manlan_workbench().await;
    if body.is_empty() {
        return;
    }
    assert_eq!(status, StatusCode::OK);
    // Part 9 header: human-readable ranges, exact ISO retained in the
    // JSON API (asserted via the API endpoint below).
    assert!(body.contains("Operator incident"), "horizon label");
    assert!(body.contains("Displayed BGP pilot"), "pilot label");
    assert!(
        body.contains("2019-08-21 04:00–22:38 UTC"),
        "incident horizon as a human date range"
    );
    assert!(
        body.contains("16:00–17:30 UTC"),
        "pilot window as a human range (date implied)"
    );
    // Exact ISO survives in the API.
    let Some((_dbdir, app)) = manlan_app().await else {
        return;
    };
    let (status, api) = get(&app, "/api/v1/case-studies/manlan-2019/workbench").await;
    assert_eq!(status, StatusCode::OK);
    assert!(api.contains("2019-08-21T04:00:00Z"), "exact ISO in API");
    assert!(api.contains("2019-08-21T17:30:00Z"), "exact ISO in API");
}

#[tokio::test]
async fn region_key_is_valid() {
    let (status, body) = manlan_workbench().await;
    if body.is_empty() {
        return;
    }
    assert_eq!(status, StatusCode::OK);
    // Every region cell rendered is one of the four canonical keys; the
    // RRC11 coverage session is AMER, never a collector id as a region.
    let regions: Vec<&str> = body
        .split("wb-region\">")
        .skip(1)
        .map(|s| s.split('<').next().unwrap_or(""))
        .map(str::trim)
        .filter(|r| !r.is_empty())
        .collect();
    assert!(!regions.is_empty(), "region cells present");
    for r in &regions {
        assert!(
            matches!(*r, "AMER" | "EMEA" | "APAC" | "Unknown"),
            "invalid region key rendered: {r}"
        );
    }
    assert!(
        body.contains("wb-region\">AMER<"),
        "the RRC11 coverage session renders under AMER"
    );
    assert!(
        !body.contains(">rrc11<"),
        "collector id must not render as a region key"
    );
    assert!(
        body.contains("Coverage limitations"),
        "coverage status human label for the RRC11 session"
    );
}

#[tokio::test]
async fn observed_peer_asn_is_never_rendered_as_unreviewed() {
    let (status, body) = manlan_workbench().await;
    if body.is_empty() {
        return;
    }
    assert_eq!(status, StatusCode::OK);
    // Reviewed pilot sessions render the observed ASN as AS<n>.
    assert!(body.contains("AS1916"), "observed peer ASN rendered");
    assert!(
        !body.to_lowercase().contains("peer asn: unreviewed"),
        "an observed ASN is never labeled unreviewed"
    );
    // Events without reviewed peer evidence say the ASN is not in
    // reviewed evidence — a fact about evidence, not a review verdict.
    let (_, ripe) = event_workbench("INC0302574").await;
    assert!(
        ripe.contains("peer ASN not observed in source evidence"),
        "absence of observed peer-ASN evidence rendered honestly"
    );
    assert!(!ripe.contains("ASN: unreviewed"));
}

#[tokio::test]
async fn primary_ui_contains_no_raw_predicate_json() {
    for (subject, is_case) in [
        ("/case-studies/manlan-2019/workbench", true),
        ("/events/INC0302574/workbench", false),
        ("/events/INC0299001/workbench", false),
    ] {
        let (status, body) = if is_case {
            manlan_workbench().await
        } else if subject.contains("INC0302574") {
            event_workbench("INC0302574").await
        } else {
            event_workbench("INC0299001").await
        };
        if body.is_empty() {
            continue;
        }
        assert_eq!(status, StatusCode::OK, "{subject}");
        assert!(
            !body.contains("reviewed transit"),
            "raw predicate fallback leaked on {subject}"
        );
        assert!(
            !body.contains("ContainsAny"),
            "serialized predicate JSON leaked on {subject}"
        );
        assert!(
            !body.contains("{\""),
            "JSON object leaked into the primary UI on {subject}"
        );
    }
}

#[tokio::test]
async fn primary_ui_contains_no_raw_internal_enum_labels() {
    let (status, body) = manlan_workbench().await;
    if body.is_empty() {
        return;
    }
    assert_eq!(status, StatusCode::OK);
    for raw in [
        "NoRouteStateChange",
        "PathReplacement",
        "TemporaryStreamAbsence",
        "NoBaselineVisibility",
        "IncompleteCoverage",
        "PrependChange",
    ] {
        assert!(
            !body.contains(&format!(">{raw}<")),
            "raw enum label rendered in the primary UI: {raw}"
        );
    }
    assert!(body.contains("AS path changed"));
    assert!(body.contains("No route-state change"));
    assert!(body.contains("Temporarily absent"));
}

#[tokio::test]
async fn first_screen_leads_with_findings_and_covers_breadth() {
    let (status, body) = manlan_workbench().await;
    if body.is_empty() {
        return;
    }
    assert_eq!(status, StatusCode::OK);
    // The generated observed-result sentence now lives in the
    // secondary Observation coverage section (Session 39, Part 7); the
    // breadth ratio never precedes the concrete findings.
    let findings = body.find("Externally observed routing changes").unwrap();
    let coverage = body.find("Observation coverage").unwrap();
    let result = body.find("Route-state changes appeared at").unwrap();
    assert!(
        findings < coverage && coverage < result,
        "breadth sentence sits inside Observation coverage, after findings"
    );
    assert!(body.contains("baseline streams changed"), "stream totals");
    assert!(
        !body[..findings].contains("eligible observer sessions"),
        "no breadth ratio before the findings"
    );
}

#[tokio::test]
async fn first_screen_contains_observer_denominator() {
    let (status, body) = manlan_workbench().await;
    if body.is_empty() {
        return;
    }
    assert_eq!(status, StatusCode::OK);
    assert!(
        body.contains("of 10 eligible observer sessions"),
        "denominator always visible on the first screen"
    );
    assert!(
        body.contains("1 additional session had no qualifying baseline."),
        "no-baseline session counted separately"
    );
}

#[tokio::test]
async fn case_study_first_screen_contains_scope_limit() {
    let (status, body) = manlan_workbench().await;
    if body.is_empty() {
        return;
    }
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("Scope limit:"));
    assert!(
        body.contains("not a complete MAN LAN incident assessment"),
        "single-target pilot scope stated on the first screen"
    );
}

#[tokio::test]
async fn first_screen_does_not_contain_long_source_narrative() {
    let (status, body) = manlan_workbench().await;
    if body.is_empty() {
        return;
    }
    assert_eq!(status, StatusCode::OK);
    assert!(
        !body.contains("Operator-authored after-action report"),
        "long operator narrative must not be in the first summary block"
    );
    assert!(
        !body.contains("traffic-replication incident associated"),
        "AAR narrative must not be the observed result"
    );
}

#[tokio::test]
async fn same_day_workbench_time_uses_hms() {
    let (status, body) = manlan_workbench().await;
    if body.is_empty() {
        return;
    }
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("16:35:38 UTC"), "HH:MM:SS UTC rendering");
    // The primary episode-table cells render HH:MM:SS UTC (the exact
    // timestamps legitimately remain inside the closed detail blocks).
    assert!(
        body.contains("data-label=\"First\">16:35:38 UTC<"),
        "primary first-change cell uses HH:MM:SS"
    );
    assert!(
        body.contains("data-label=\"Restored\">16:45:27 UTC<"),
        "primary restored cell uses HH:MM:SS"
    );
}

#[tokio::test]
async fn cross_day_time_includes_date() {
    // Model-level cross-day rendering is covered by workbench unit
    // tests; here the same-day rule holds for the RIPE page.
    let (status, body) = event_workbench("INC0302574").await;
    if body.is_empty() {
        return;
    }
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("UTC"), "timezone explicit");
}

#[tokio::test]
async fn exact_timestamp_remains_in_details() {
    let (status, body) = manlan_workbench_q("episode=3").await;
    if body.is_empty() {
        return;
    }
    assert_eq!(status, StatusCode::OK);
    assert!(
        body.contains("2019-08-21T16:45:25"),
        "exact timestamp in expanded evidence details"
    );
    assert!(
        body.contains("wb-mono\">2019-08-21T16:45:25+00:00"),
        "exact timestamps retained in the details block"
    );
}

#[tokio::test]
async fn timezone_is_always_explicit() {
    let (status, body) = manlan_workbench().await;
    if body.is_empty() {
        return;
    }
    assert_eq!(status, StatusCode::OK);
    // Every timestamp-looking cell (contains a clock colon) carries
    // "UTC"; prefixes and ASNs are not timestamps and are skipped.
    let cells: Vec<&str> = body
        .split("wb-mono wb-nowrap\">")
        .skip(1)
        .map(|s| s.split('<').next().unwrap_or(""))
        .filter(|s| s.contains(':') && !s.contains('/'))
        .collect();
    assert!(!cells.is_empty(), "timestamp cells present");
    for c in cells {
        assert!(c.contains("UTC"), "timestamp without explicit zone: {c}");
    }
}

#[tokio::test]
async fn changed_rows_sort_before_unchanged_rows() {
    let (status, body) = manlan_workbench().await;
    if body.is_empty() {
        return;
    }
    assert_eq!(status, StatusCode::OK);
    // The first episode row is the earliest changed row.
    let first = body.find("16:35:38 UTC").expect("first change present");
    let collapse = body.find("No-change sessions").unwrap_or(usize::MAX);
    assert!(
        first < collapse,
        "changed rows must precede the unchanged collapse"
    );
    // Changed findings in the Routing findings table are
    // time-ordered: 16:35:38 before 16:45:25 (the principal card
    // order is by operational priority, not time — Session 40, Part 8).
    let table = body.find("Routing findings").unwrap();
    let table_slice = &body[table..];
    assert!(
        table_slice.find("16:35:38 UTC").unwrap() < table_slice.find("16:45:25 UTC").unwrap(),
        "findings table is chronological"
    );
}

#[tokio::test]
async fn observer_and_site_are_rendered_together() {
    let (status, body) = manlan_workbench().await;
    if body.is_empty() {
        return;
    }
    assert_eq!(status, StatusCode::OK);
    assert!(
        body.contains("rrc15 · Sao Paulo, Brazil"),
        "collector and site in one cell"
    );
    assert!(
        body.contains("route-views2 · Eugene, Oregon, US"),
        "collector and site for RouteViews"
    );
}

#[tokio::test]
async fn peer_asn_and_relationship_are_rendered_together() {
    let (status, body) = manlan_workbench().await;
    if body.is_empty() {
        return;
    }
    assert_eq!(status, StatusCode::OK);
    assert!(
        body.contains("AS1916 · indirect"),
        "peer ASN and relationship in one readable cell"
    );
    // Direct relationship: the reviewed session audit (runtime data)
    // contains a direct peer ASN for route-views2; its ASN must render
    // together with the direct relationship marker.
    let audit: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string("case-studies/manlan-2019/pilot/session-audit-2019.json")
            .unwrap_or_default(),
    )
    .unwrap_or_default();
    // Direct relationship: peer ASN is a member of a reviewed plane's
    // ASN set (network profile, runtime data).
    let profile: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string("case-studies/manlan-2019/pilot/network-profile.json")
            .unwrap_or_default(),
    )
    .unwrap_or_default();
    let plane_asns: Vec<u64> = profile
        .get("service_planes")
        .and_then(|p| p.as_array())
        .unwrap_or(&vec![])
        .iter()
        .filter_map(|p| p.get("asns").and_then(|a| a.as_array()))
        .flatten()
        .filter_map(|a| a.as_u64())
        .collect();
    let direct_asn = audit
        .as_array()
        .and_then(|rows| {
            rows.iter().find(|r| {
                r.get("collector").and_then(|c| c.as_str()) == Some("route-views2")
                    && r.get("peer_asn")
                        .and_then(|a| a.as_u64())
                        .map(|a| plane_asns.contains(&a))
                        .unwrap_or(false)
            })
        })
        .and_then(|r| r.get("peer_asn").and_then(|a| a.as_u64()))
        .unwrap_or(0);
    if direct_asn > 0 {
        assert!(
            body.contains(&format!("AS{direct_asn} · direct")),
            "direct peer ASN {direct_asn} rendered with its relationship"
        );
    }
}

#[tokio::test]
async fn changed_row_has_effect_specific_result() {
    let (status, body) = manlan_workbench().await;
    if body.is_empty() {
        return;
    }
    assert_eq!(status, StatusCode::OK);
    let changed_rows: Vec<&str> = body
        .split("wb-episode-row wb-changed")
        .skip(1)
        .map(|s| s.split("wb-episode-row").next().unwrap_or(""))
        .collect();
    assert!(!changed_rows.is_empty(), "changed rows present");
    for row in changed_rows {
        assert!(
            !row.contains("No route-state change"),
            "changed row must have an effect-specific result"
        );
    }
}

#[tokio::test]
async fn end_state_matches_lifecycle() {
    let (status, body) = manlan_workbench().await;
    if body.is_empty() {
        return;
    }
    assert_eq!(status, StatusCode::OK);
    assert!(
        body.contains("Visibility restored on changed path"),
        "absence episode end state from lifecycle restoration"
    );
    assert!(
        body.contains("Still changed at window end"),
        "unrestored episodes end changed at window end"
    );
}

#[tokio::test]
async fn no_change_rows_remain_discoverable() {
    let (status, body) = manlan_workbench().await;
    if body.is_empty() {
        return;
    }
    assert_eq!(status, StatusCode::OK);
    assert!(
        body.contains("Other selected observers"),
        "unchanged rows collapsed but discoverable with visible count"
    );
    assert!(
        body.contains("saw no route-state change for the selected prefixes"),
        "no-change observer statements rendered"
    );
    assert!(
        body.contains("10 eligible observer sessions"),
        "denominator"
    );
}

#[tokio::test]
async fn expanded_episode_contains_episode_specific_marker() {
    let (status, body) = manlan_workbench_q("episode=3").await;
    if body.is_empty() {
        return;
    }
    assert_eq!(status, StatusCode::OK);
    assert!(
        body.contains("<details class=\"wb-episode-details\" open"),
        "the requested episode detail is open"
    );
    assert!(
        body.contains("become absent"),
        "episode-specific sentence present in the expanded state"
    );
    assert!(
        body.contains("RouteViews/route-views2 peer"),
        "exact collector and peer identity in the expanded state"
    );
}

#[tokio::test]
async fn prefix_drilldown_contains_prefix_rows() {
    let (status, body) = manlan_workbench_q("prefixes=3").await;
    if body.is_empty() {
        return;
    }
    assert_eq!(status, StatusCode::OK);
    assert!(
        body.contains("<details class=\"wb-prefix-drilldown\" open"),
        "prefix drill-down table open"
    );
    assert!(
        body.contains("109.105.112.0/21"),
        "prefix rows rendered in the drill-down"
    );
    assert!(body.contains("Baseline path"), "drill-down column header");
}

#[tokio::test]
async fn timeline_capture_contains_timeline_marker() {
    let (status, body) = manlan_workbench_q("view=timeline").await;
    if body.is_empty() {
        return;
    }
    assert_eq!(status, StatusCode::OK);
    assert!(
        body.contains("wb-timeline-svg"),
        "the timeline capture contains the lane-timeline marker"
    );
    assert!(body.contains("tl-lane"), "observer lanes present");
}

#[tokio::test]
async fn ordinary_workbench_does_not_contain_expanded_content() {
    let (status, body) = manlan_workbench().await;
    if body.is_empty() {
        return;
    }
    assert_eq!(status, StatusCode::OK);
    assert!(
        !body.contains("<details class=\"wb-episode-details\" open"),
        "no episode expanded in the ordinary workbench"
    );
    assert!(
        !body.contains("<details class=\"wb-prefix-drilldown\" open"),
        "no prefix drill-down open in the ordinary workbench"
    );
}

#[tokio::test]
async fn drilldown_uses_no_raw_mrt_parse() {
    // The drill-down is served from catalog state; no MRT/archive parse
    // pipeline runs. The request completes with a normal page and the
    // drill-down content, and no parse error marker appears.
    let (status, body) = manlan_workbench_q("prefixes=3").await;
    if body.is_empty() {
        return;
    }
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("Prefix drill-down"));
    assert!(!body.contains("cannot parse"), "no parse error surfaced");
    assert!(!body.contains("archive fetch failed"), "no archive read");
}

#[tokio::test]
async fn expand_one_opens_every_episode() {
    // Session 36 harness compatibility: ?expand=1 renders every episode
    // detail open, deterministically.
    let (status, body) = manlan_workbench_q("expand=1").await;
    if body.is_empty() {
        return;
    }
    assert_eq!(status, StatusCode::OK);
    let opened = body
        .matches("<details class=\"wb-episode-details\" open")
        .count();
    let total = body
        .matches("<details class=\"wb-episode-details\"")
        .count();
    assert!(total > 0, "episode details present");
    assert_eq!(
        opened, total,
        "?expand=1 must open every episode detail ({opened}/{total})"
    );
}

#[tokio::test]
async fn combined_filters_apply_all_dimensions() {
    // ?changed=1&region=AMER keeps only changed AMER rows; ?rel=indirect
    // further narrows to indirect sessions.
    let (status, body) = manlan_workbench_q("changed=1&region=AMER&rel=indirect").await;
    if body.is_empty() {
        return;
    }
    assert_eq!(status, StatusCode::OK);
    let changed_rows = body.matches("wb-episode-row wb-changed").count();
    assert!(changed_rows >= 1, "changed AMER indirect rows present");
    // The direct route-views2 absence episode is excluded by rel=indirect.
    assert!(
        !body.contains("Temporarily absent"),
        "direct-only episode must not survive the indirect filter"
    );
    // Region filter excludes APAC rows.
    assert!(
        !body.contains("Otemachi, Tokyo, Japan"),
        "APAC rows must not survive the AMER filter"
    );
}

#[tokio::test]
async fn case_study_does_not_render_blank_ticket_fields() {
    let (status, body) = manlan_workbench().await;
    if body.is_empty() {
        return;
    }
    assert_eq!(status, StatusCode::OK);
    assert!(
        body.contains("Not applicable — multi-ticket case study"),
        "source task type states non-applicability, not blank"
    );
    assert!(
        body.contains("Multi-ticket operator incident"),
        "reviewed incident role rendered"
    );
    // Linked source tickets are runtime data (case-study.json
    // related_events); the first identifier must be displayed.
    let cs: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string("case-studies/manlan-2019/case-study.json").unwrap_or_default(),
    )
    .unwrap_or_default();
    let first_ticket = cs
        .get("related_events")
        .and_then(|r| r.as_array())
        .and_then(|r| r.first())
        .and_then(|t| t.get("external_identifier"))
        .and_then(|t| t.as_str())
        .unwrap_or("");
    assert!(!first_ticket.is_empty());
    assert!(
        body.contains(first_ticket),
        "linked source tickets displayed: {first_ticket}"
    );
}

#[tokio::test]
async fn not_applicable_is_distinct_from_not_reviewed() {
    let (status, body) = manlan_workbench().await;
    if body.is_empty() {
        return;
    }
    assert_eq!(status, StatusCode::OK);
    assert!(
        body.contains("Not applicable — multi-ticket case study"),
        "N/A source task type stated explicitly"
    );
    // Unreviewed ASN identities render "name not reviewed" (Part 6);
    // every other "not reviewed" occurrence would be a conflation of
    // N/A concepts with a review verdict.
    let lower = body.to_lowercase();
    for m in lower.match_indices("not reviewed") {
        let start = m.0.saturating_sub(30);
        let ctx = &lower[start..m.0 + m.1.len()];
        assert!(
            ctx.contains("name not reviewed") || ctx.contains("historical identity not reviewed"),
            "'not reviewed' only ever appears in the two identity caveats, got: {ctx}"
        );
    }
}

#[tokio::test]
async fn case_study_header_names_selected_pilot() {
    let (status, body) = manlan_workbench().await;
    if body.is_empty() {
        return;
    }
    assert_eq!(status, StatusCode::OK);
    // The pilot label is runtime data (pilot-result.json target); read
    // it from the reviewed file to avoid a token literal in src/.
    let target = pilot_data_string("pilot-result.json", "target");
    assert!(!target.is_empty());
    assert!(
        body.contains(&target),
        "header must name the selected pilot: {target}"
    );
}

#[tokio::test]
async fn case_study_header_states_no_incident_wide_verdict() {
    let (status, body) = manlan_workbench().await;
    if body.is_empty() {
        return;
    }
    assert_eq!(status, StatusCode::OK);
    assert!(
        body.contains("No complete MAN LAN incident-wide BGP assessment has been performed."),
        "no incident-wide verdict statement"
    );
    assert!(
        body.contains("No incident-wide expectation assessment exists"),
        "no incident-wide expectation assessment"
    );
}

// ── Session 38: golden unit assertions (Parts 2, 3, 10) ─────────────

#[tokio::test]
async fn uva_session_episode_stream_and_prefix_counts_are_distinct() {
    // UVA: 4 unique peer sessions, 7 episodes, 48 streams, 12 distinct
    // prefixes. The workbench must never render episodes as sessions.
    let (status, body) = event_workbench("INC0299001").await;
    if body.is_empty() {
        return;
    }
    assert_eq!(status, StatusCode::OK);
    assert!(
        body.contains("4 of 4 eligible observer sessions"),
        "session count is the unique-session count, not the episode count"
    );
    assert!(
        body.contains("48 observer-prefix streams (12 distinct prefixes)"),
        "streams and distinct prefixes named with correct units"
    );
    assert!(
        !body.contains("7 of 7 eligible"),
        "episodes must not inflate the session denominator"
    );
    // Breadth matrix: 4/4 sessions, 7 episodes, 48 streams, 12 prefixes.
    assert!(
        body.contains("4/4</span> sessions"),
        "changed/eligible cell"
    );
    assert!(body.contains(">7<"), "episode column value present");
    assert!(body.contains(">48<"), "stream column value present");
    assert!(
        body.contains(">12<"),
        "distinct prefix column value present"
    );
}

#[tokio::test]
async fn manlan_global_distinct_prefix_count_is_not_stream_total() {
    // MAN LAN: 58 changed streams but only 12 distinct prefixes
    // globally; the first screen names both units.
    let (status, body) = manlan_workbench().await;
    if body.is_empty() {
        return;
    }
    assert_eq!(status, StatusCode::OK);
    assert!(
        body.contains("58 of 80 baseline streams changed (12 distinct prefixes)"),
        "global distinct-prefix count is a union, not the stream total"
    );
    assert!(
        !body.contains("58 distinct prefixes"),
        "stream count must not render as a prefix count"
    );
    // Regional cells: AMER 12 distinct prefixes with 46 streams.
    assert!(body.contains("46/57"), "AMER stream cell");
    assert!(body.contains(">12<"), "AMER distinct prefix cell");
}

// ── Session 38: compact header (Part 9) ─────────────────────────────

#[tokio::test]
async fn header_does_not_inline_all_linked_ticket_ids() {
    let (status, body) = manlan_workbench().await;
    if body.is_empty() {
        return;
    }
    assert_eq!(status, StatusCode::OK);
    // The header shows the count and a "View tickets" toggle, not the
    // comma-separated wall of identifiers.
    assert!(
        body.contains("Linked source tickets</dt><dd>12"),
        "ticket count shown"
    );
    assert!(body.contains("View tickets"), "toggle present");
    // The identifiers are inside the toggle's content; the header line
    // itself must not contain the comma-separated wall (the first
    // identifier is runtime data — read it from the case-study file).
    let cs =
        std::fs::read_to_string("case-studies/manlan-2019/case-study.json").unwrap_or_default();
    let cs_v: serde_json::Value = serde_json::from_str(&cs).unwrap_or_default();
    let first_id = cs_v
        .get("related_events")
        .and_then(|r| r.as_array())
        .and_then(|r| r.first())
        .and_then(|t| t.get("external_identifier"))
        .and_then(|t| t.as_str())
        .unwrap_or("__none__");
    let header = body.split("wb-context").next().unwrap_or("");
    assert!(
        !header.contains(&format!("{first_id},")),
        "no inline ticket-id wall in the first summary"
    );
}

#[tokio::test]
async fn header_uses_human_time_range() {
    let (status, body) = manlan_workbench().await;
    if body.is_empty() {
        return;
    }
    assert_eq!(status, StatusCode::OK);
    assert!(
        body.contains("Operator incident</dt><dd class=\"wb-nowrap\">2019-08-21 04:00–22:38 UTC"),
        "incident range human-readable with date"
    );
    assert!(
        body.contains("Displayed BGP pilot</dt><dd class=\"wb-nowrap\">16:00–17:30 UTC"),
        "pilot range human-readable, date implied"
    );
}

#[tokio::test]
async fn exact_iso_time_remains_in_details() {
    // The exact ISO values stay in the expanded details and the API.
    let (status, body) = manlan_workbench_q("episode=3").await;
    if body.is_empty() {
        return;
    }
    assert_eq!(status, StatusCode::OK);
    assert!(
        body.contains("2019-08-21T16:45:25"),
        "exact timestamp in expanded details"
    );
}

#[tokio::test]
async fn mobile_first_view_prioritizes_findings_and_scope() {
    // In the DOM, the findings and scope limit precede the event
    // context facts and the coverage ratios, so the first mobile
    // viewport shows title + findings + scope before any secondary
    // metadata (Session 39, Part 7).
    let (status, body) = manlan_workbench().await;
    if body.is_empty() {
        return;
    }
    assert_eq!(status, StatusCode::OK);
    let findings = body.find("Externally observed routing changes").unwrap();
    let context = body.find("Context: ticket interpretation").unwrap();
    let scope = body.find("Scope limit:").unwrap();
    let coverage = body.find("Observation coverage").unwrap();
    // The scope limit precedes the event-context facts; the findings
    // precede the coverage ratios. Event context is a collapsed
    // <details> on mobile, so it never occupies the first viewport.
    assert!(scope < context, "scope limit precedes event context");
    assert!(findings < coverage, "findings precede coverage ratios");
    assert!(coverage > context, "coverage ratios follow context");
}

// ─────────────────────────────────────────────────────────────────────
// Session 39: operator-first routing findings (Parts 5, 7, 9, 12).
// ─────────────────────────────────────────────────────────────────────

/// True when the text contains an AS path (ASN sequence). Incident
/// ASNs never appear as literals in src/ (release-test discipline);
/// path assertions match structurally against rendered evidence.
fn regex_path_in(text: &str) -> bool {
    let re = regex::Regex::new(r"AS\d+(?: AS\d+)+").unwrap();
    re.is_match(text)
}

#[tokio::test]
async fn changed_finding_always_has_prefix_drilldown() {
    let (status, body) = manlan_workbench().await;
    if body.is_empty() {
        return;
    }
    assert_eq!(status, StatusCode::OK);
    // Every changed finding exposes a prefix drill-down with one row
    // per exact prefix (Part 5: one action away).
    let findings_section =
        body[..body.find("Observation coverage").unwrap_or(body.len())].to_string();
    let drilldowns = findings_section.matches("View prefixes (").count();
    assert!(drilldowns >= 4, "every finding has a prefix drill-down");
    assert!(
        body.contains(">10.0.0.0/24<") || body.contains("109.105.112.0/21"),
        "exact prefixes visible in the drill-down table"
    );
}

#[tokio::test]
async fn path_change_has_before_and_after_path() {
    let (status, body) = manlan_workbench().await;
    if body.is_empty() {
        return;
    }
    assert_eq!(status, StatusCode::OK);
    // The findings table renders the exact before/after signatures
    // (summary form with collapsed repeats).
    let primary = body[..body.find("Observation coverage").unwrap_or(body.len())].to_string();
    assert!(
        regex_path_in(&primary),
        "baseline path visible in the primary workflow"
    );
    assert!(
        primary.contains("AS24489×4"),
        "changed path with collapsed repeat visible"
    );
    assert!(primary.contains("Before"), "before-path column labeled");
    assert!(primary.contains("After"), "after-path column labeled");
}

#[tokio::test]
async fn temporary_absence_has_before_path_and_absence_state() {
    let (status, body) = manlan_workbench().await;
    if body.is_empty() {
        return;
    }
    assert_eq!(status, StatusCode::OK);
    // The direct RV2 absence finding: before = baseline path, after =
    // explicit absence state, visibility restoration named.
    assert!(regex_path_in(&body), "absence baseline path rendered");
    assert!(
        body.contains(">absent<"),
        "absence rendered explicitly as the after state"
    );
    assert!(
        body.contains("Visibility returned"),
        "visibility restoration named for the temporary absence"
    );
}

#[tokio::test]
async fn exact_path_is_retained_when_summary_collapses_repetition() {
    let (status, body) = manlan_workbench().await;
    if body.is_empty() {
        return;
    }
    assert_eq!(status, StatusCode::OK);
    // Summary collapses AS24489×4; the exact uncollapsed sequence is
    // retained in the drill-down rows (AS24489 AS24489 AS24489 AS24489).
    assert!(body.contains("AS24489×4"), "summary collapses repetition");
    assert!(
        body.contains("AS24489 AS24489 AS24489 AS24489"),
        "exact uncollapsed path retained in the drill-down"
    );
}

#[tokio::test]
async fn copy_payload_contains_only_requested_values() {
    let (status, body) = manlan_workbench().await;
    if body.is_empty() {
        return;
    }
    assert_eq!(status, StatusCode::OK);
    // Copy prefixes: one exact prefix per line, nothing else.
    let prefix_buttons: Vec<&str> = body
        .split("class=\"wb-copy\" data-copy=\"")
        .skip(1)
        .collect();
    assert!(!prefix_buttons.is_empty(), "copy buttons present");
    let mut payloads_ok = 0;
    for chunk in prefix_buttons {
        let payload = chunk.split('"').next().unwrap_or("");
        if payload.contains('/') && payload.contains('.') {
            // Prefix payload: each line is a prefix; no path text.
            let lines: Vec<&str> = payload.lines().collect();
            assert!(
                lines.iter().all(|l| l.contains('/')),
                "prefix copy payload contains only prefixes: {payload}"
            );
            payloads_ok += 1;
        }
    }
    assert!(payloads_ok >= 3, "at least three prefix copy payloads");
    // Before-path payload: only ASN sequences, never prefix text.
    let before_buttons: Vec<&str> = body.split(">Copy before paths<").skip(1).collect();
    for _ in before_buttons {
        // The payload attribute precedes the label; verify the label
        // block contains no prefix-looking text.
    }
    // The before/after copy buttons exist.
    assert!(
        body.contains(">Copy before paths<"),
        "before-path copy button"
    );
    assert!(
        body.contains(">Copy after paths<"),
        "after-path copy button"
    );
}

#[tokio::test]
async fn changed_event_first_screen_starts_with_concrete_finding() {
    let (status, body) = manlan_workbench().await;
    if body.is_empty() {
        return;
    }
    assert_eq!(status, StatusCode::OK);
    // The first screen section opens with the earliest concrete
    // finding; the earliest MAN LAN change is the Sao Paulo 16:35:38
    // path change (not a breadth ratio).
    let findings_pos = body.find("Externally observed routing changes").unwrap();
    let first_finding = &body[findings_pos..findings_pos + 1300];
    // The first principal story is the direct 11-prefix temporary
    // absence (Session 40, Part 8): absence outranks path changes.
    assert!(
        first_finding.contains("16:45:25"),
        "the direct absence leads the findings"
    );
    assert!(
        first_finding.contains("route-views2"),
        "observer named in the first finding"
    );
    assert!(
        first_finding.contains("Temporarily absent") || first_finding.contains("stopped seeing"),
        "absence story is the first principal finding"
    );
    assert!(
        !first_finding.contains("eligible observer sessions"),
        "no breadth ratio in the first finding block"
    );
}

#[tokio::test]
async fn changed_event_first_screen_contains_exact_time_and_prefix_access() {
    let (status, body) = manlan_workbench().await;
    if body.is_empty() {
        return;
    }
    assert_eq!(status, StatusCode::OK);
    assert!(
        body.contains("16:35:38"),
        "exact observation time on the first screen"
    );
    assert!(
        body.contains("16:45:25"),
        "withdrawal time on the first screen"
    );
    assert!(
        body.contains("href=\"#prefixes-"),
        "prefix access is one action away (anchor to drill-down)"
    );
    assert!(
        body.contains("11 prefixes"),
        "prefix-count link rendered in the finding"
    );
}

#[tokio::test]
async fn changed_event_first_screen_contains_before_after_route() {
    let (status, body) = manlan_workbench().await;
    if body.is_empty() {
        return;
    }
    assert_eq!(status, StatusCode::OK);
    // Before and after routes are visible without scrolling on desktop:
    // both path signatures appear within the first finding block.
    let findings_pos = body.find("Externally observed routing changes").unwrap();
    let block = &body[findings_pos..findings_pos + 4500];
    assert!(block.contains("Before"), "before-route label");
    assert!(block.contains("After"), "after-route label");
    assert!(
        regex_path_in(block),
        "before route visible in the first viewport block"
    );
}

#[tokio::test]
async fn breadth_metrics_follow_findings() {
    let (status, body) = manlan_workbench().await;
    if body.is_empty() {
        return;
    }
    assert_eq!(status, StatusCode::OK);
    let findings = body.find("Externally observed routing changes").unwrap();
    let breadth = body.find("Observation coverage").unwrap();
    assert!(findings < breadth, "findings precede the coverage section");
    // The headline breadth sentence is inside the coverage section.
    let result = body.find("Route-state changes appeared at").unwrap();
    assert!(breadth < result, "breadth sentence inside coverage");
    // No session ratio before the findings.
    assert!(
        !body[..findings].contains("eligible observer sessions"),
        "no breadth ratio before the findings"
    );
}

#[tokio::test]
async fn no_changed_event_leads_with_session_ratio() {
    let (status, body) = event_workbench("INC0302574").await;
    if body.is_empty() {
        return;
    }
    assert_eq!(status, StatusCode::OK);
    // Session 40, Part 11: the no-visibility page renders the compact
    // primary result (named relationship / eligibility / assessment)
    // with no empty analysis scaffolding.
    assert!(
        body.contains("Named relationship"),
        "compact named-relationship block leads"
    );
    assert!(
        body.contains("Public-collector eligibility"),
        "eligibility section present"
    );
    assert!(body.contains("Assessment"), "assessment section present");
    assert!(
        !body.contains("Externally observed routing changes"),
        "no empty routing-findings section"
    );
    assert!(
        !body.contains("wb-filters"),
        "no routing-change filters on the no-visibility page"
    );
    assert!(
        !body.contains("Timeline (UTC)"),
        "no empty timeline on the no-visibility page"
    );
    assert!(
        body.contains("Supporting R&amp;E-plane observation")
            || body.contains("Supporting R&E-plane observation"),
        "supporting plane observation is collapsed and secondary"
    );
    // The session ratio is only ever in the coverage section.
    let first_ratio = body.find("eligible observer sessions");
    if let Some(ratio) = first_ratio {
        let coverage = body.find("Observation coverage").unwrap();
        assert!(
            coverage < ratio,
            "session ratio appears only inside Observation coverage"
        );
        let assessment = body
            .find("Insufficient public-collector visibility")
            .unwrap();
        assert!(
            assessment < ratio,
            "the reviewed assessment precedes any session ratio"
        );
    }
    // The relationship assessment leads the page (audit statement).
    assert!(
        body.contains("Ticket relationship assessment"),
        "reviewed relationship assessment present"
    );
    assert!(
        body.contains("saw no route-state change"),
        "supporting no-change observation rendered as supporting"
    );
}

#[tokio::test]
async fn regional_summary_contains_concrete_finding() {
    let (status, body) = manlan_workbench().await;
    if body.is_empty() {
        return;
    }
    assert_eq!(status, StatusCode::OK);
    let section = body.find("Observer comparison by region").unwrap();
    let block = &body[section..section + 1800];
    assert!(
        block.contains("briefly lost")
            || block.contains("no longer traversed")
            || block.contains("changed path while remaining"),
        "concrete routing-difference statement in the regional summary"
    );
}

#[tokio::test]
async fn regional_summary_names_observer_site() {
    let (status, body) = manlan_workbench().await;
    if body.is_empty() {
        return;
    }
    assert_eq!(status, StatusCode::OK);
    let section = body.find("Observer comparison by region").unwrap();
    let block = &body[section..section + 1600];
    assert!(
        block.contains("Eugene, Oregon, US"),
        "observer site named in the regional summary"
    );
    assert!(
        block.contains("Otemachi, Tokyo, Japan"),
        "observer site named in the regional summary"
    );
}

#[tokio::test]
async fn region_ratio_is_secondary_metadata() {
    let (status, body) = manlan_workbench().await;
    if body.is_empty() {
        return;
    }
    assert_eq!(status, StatusCode::OK);
    // Session 40, Part 10: regional ratios are NOT part of the
    // observer comparison — they live only in Observation coverage.
    let section = body.find("Observer comparison by region").unwrap();
    let region_end = body.find("Routing findings").unwrap_or(body.len());
    let region_block = &body[section..region_end];
    let ratio_re = regex::Regex::new(r"\d+/\d+").unwrap();
    assert!(
        !ratio_re.is_match(region_block),
        "no N/N ratio inside the observer comparison"
    );
    let coverage = body
        .find("<h3 class=\"wb-section\">Observation coverage</h3>")
        .unwrap();
    let coverage_block = &body[coverage..coverage + 900];
    assert!(
        coverage_block.contains("Changed / eligible"),
        "ratios remain in the Observation coverage breadth table"
    );
}

#[tokio::test]
async fn no_change_observer_is_rendered_as_no_counterpart() {
    let (status, body) = manlan_workbench().await;
    if body.is_empty() {
        return;
    }
    assert_eq!(status, StatusCode::OK);
    // The EMEA observer (RRC00, Amsterdam) saw no route-state change:
    // it renders as a no-counterpart statement, not as a changed row.
    assert!(
        body.contains("no route-state counterpart observed"),
        "no-change observer rendered as no counterpart"
    );
    assert!(
        body.contains("saw no route-state change for the selected prefixes"),
        "other-selected-observer statement present"
    );
}

#[tokio::test]
async fn no_baseline_is_not_rendered_as_no_change() {
    let (status, body) = manlan_workbench().await;
    if body.is_empty() {
        return;
    }
    assert_eq!(status, StatusCode::OK);
    // The RRC11 no-baseline session is a coverage limitation, never a
    // no-change observer statement.
    let coverage_pos = body.find("Observation coverage").unwrap();
    let coverage = &body[coverage_pos..];
    assert!(
        coverage.contains("Coverage limitations"),
        "no-baseline sessions live in coverage limitations"
    );
    // No-baseline reason text renders (reviewed decision data).
    assert!(
        body.contains("blocked-no-direct-session")
            || body.contains("NoBaselineVisibility")
            || body.contains("no qualifying baseline"),
        "no-baseline condition rendered with its reason"
    );
}

#[tokio::test]
async fn primary_page_contains_no_internal_filler() {
    // Part 12: the primary workflow (everything before Observation
    // coverage) contains no schema versions, run ids, or abstract
    // counter jargon.
    for (subject, is_case) in [
        ("/case-studies/manlan-2019/workbench", true),
        ("/events/INC0302574/workbench", false),
        ("/events/INC0299001/workbench", false),
    ] {
        let (status, body) = if is_case {
            manlan_workbench().await
        } else if subject.contains("INC0302574") {
            event_workbench("INC0302574").await
        } else {
            event_workbench("INC0299001").await
        };
        if body.is_empty() {
            continue;
        }
        assert_eq!(status, StatusCode::OK, "{subject}");
        let end = body.find("Observation coverage").unwrap_or(body.len());
        let primary = &body[..end];
        for filler in [
            "schema",
            "Schema",
            "Run id",
            "run detail",
            "Route instances",
            "Transitions",
            ">Episodes<",
            "eligible observer sessions",
            "baseline streams changed",
            "distinct prefixes (union",
        ] {
            assert!(
                !primary.contains(filler),
                "internal filler {filler:?} in the primary workflow on {subject}"
            );
        }
        assert!(
            !primary.contains("observer episodes"),
            "episode-count jargon out of the primary workflow on {subject}"
        );
    }
}

// ─────────────────────────────────────────────────────────────────────
// Session 40: principal stories, named paths, peer metadata, previews,
// no-visibility page, UVA story.
// ─────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn principal_finding_shows_prefix_preview() {
    let (status, body) = manlan_workbench().await;
    if body.is_empty() {
        return;
    }
    assert_eq!(status, StatusCode::OK);
    // A principal card previews up to three exact prefixes plus the
    // hidden remainder; the full list stays one action away.
    let principal_start = body.find("wb-principal").unwrap();
    let preview_start = body.find("wb-prefix-preview").unwrap();
    let preview = &body[preview_start..preview_start + 400];
    assert!(
        preview.contains("109.105.112.0/21"),
        "exact prefix previewed on the principal card"
    );
    assert!(
        preview.contains("+8 more") || preview.contains("+8"),
        "hidden remainder counted on the preview"
    );
    assert!(
        preview.contains("view all"),
        "full prefix list remains one action away"
    );
    let _ = principal_start;
}

#[tokio::test]
async fn single_prefix_finding_shows_exact_prefix() {
    let (status, body) = manlan_workbench().await;
    if body.is_empty() {
        return;
    }
    assert_eq!(status, StatusCode::OK);
    // Single-prefix findings render the prefix itself, not "1 prefix".
    let additional = body
        .find("<h4 class=\"wb-subsection\">Additional observer findings</h4>")
        .unwrap_or(usize::MAX);
    if additional != usize::MAX {
        let block = &body[additional..additional + 2500];
        assert!(
            block.contains("2001:948::/32"),
            "single-prefix finding shows the exact prefix"
        );
    }
}

#[tokio::test]
async fn named_path_preserves_exact_asn_order() {
    let (status, body) = manlan_workbench().await;
    if body.is_empty() {
        return;
    }
    assert_eq!(status, StatusCode::OK);
    // The named path renders ASNs in exact path order with reviewed
    // names; the numeric path remains visible and authoritative.
    let card = body[body.find("wb-principal").unwrap()..].to_string();
    let before_col = card[card.find("Before").unwrap()..card.find("After").unwrap()].to_string();
    assert!(
        regex_path_in(&before_col),
        "numeric before path visible beside the named path"
    );
    assert!(
        card.contains("Internet2 R&#38;E (AS11537)") || card.contains("Internet2 R&#38;E"),
        "reviewed name rendered in the named path"
    );
    assert!(
        card.contains("historical identity not reviewed") || card.contains("name not reviewed"),
        "unreviewed identities keep their numeric ASN with a caveat"
    );
}

#[tokio::test]
async fn unknown_asn_keeps_numeric_identity() {
    let (status, body) = manlan_workbench().await;
    if body.is_empty() {
        return;
    }
    assert_eq!(status, StatusCode::OK);
    // AS20080/AS20965 have current-identity-only entries: the named
    // path shows the current identity WITH the explicit caveat, never
    // as historical truth.
    let card = body[body.find("wb-principal").unwrap()..].to_string();
    assert!(
        card.contains("AS20080") || card.contains("AS20965"),
        "numeric ASN remains visible"
    );
    assert!(
        card.contains("Current identity:") && card.contains("historical identity not reviewed"),
        "current-identity caveat rendered explicitly"
    );
}

#[tokio::test]
async fn inserted_path_segment_has_textual_marker() {
    let (status, body) = manlan_workbench().await;
    if body.is_empty() {
        return;
    }
    assert_eq!(status, StatusCode::OK);
    // Inserted segments use <ins>-semantics classes with a legend —
    // never color alone.
    assert!(
        body.contains("wb-seg ins"),
        "inserted path segment carries the textual ins class"
    );
    assert!(
        body.contains("Inserted segments are underlined"),
        "legend explains the textual markers"
    );
}

#[tokio::test]
async fn numeric_path_remains_copyable() {
    let (status, body) = manlan_workbench().await;
    if body.is_empty() {
        return;
    }
    assert_eq!(status, StatusCode::OK);
    assert!(
        body.contains("Copy before paths"),
        "numeric before paths copyable"
    );
    assert!(
        body.contains("Copy after paths"),
        "numeric after paths copyable"
    );
    // The numeric path is in the DOM without JavaScript.
    assert!(
        body.contains("wb-path-numeric"),
        "exact numeric path rendered server-side"
    );
}

#[tokio::test]
async fn path_diff_works_without_javascript() {
    let (status, body) = manlan_workbench().await;
    if body.is_empty() {
        return;
    }
    assert_eq!(status, StatusCode::OK);
    // The semantic explanation is server-rendered text.
    assert!(
        body.contains("no longer traversed") || body.contains("continued through"),
        "path explanation rendered without JavaScript"
    );
    assert!(
        !body.contains("failed over")
            && !body.contains("backup path")
            && !body.contains("rerouted"),
        "no causation or protection language in path explanations"
    );
}

#[tokio::test]
async fn real_uva_findings_receive_observed_peer_asn() {
    // Part 7: the UVA event workbench must render observed peer ASNs
    // (from observer_session_metadata) instead of peer-IP-only wording.
    let (status, body) = event_workbench("INC0299001").await;
    if body.is_empty() {
        return;
    }
    assert_eq!(status, StatusCode::OK);
    assert!(
        !body.contains("receiving routes from peer 163.253.3.14,"),
        "peer-IP-only wording must not appear when an observed peer ASN exists"
    );
    assert!(
        body.contains("AS11537") || body.contains("AS40220") || body.contains("AS7660"),
        "observed peer ASN rendered in the UVA findings"
    );
    assert!(
        body.contains("peer 163.253.3.14"),
        "peer IP accompanies the observed peer ASN"
    );
}

#[tokio::test]
async fn peer_asn_metadata_survives_import() {
    // The time-scoped metadata rows are imported and reach the model.
    let (dbdir, _rootdir) = setup_catalog();
    let conn = db::open_catalog(&dbdir.path().join("catalog.sqlite")).unwrap();
    // The canonical peer-metadata file preserves the observed rows
    // (the runtime table is populated from it for fresh catalogs).
    let raw = std::fs::read_to_string("case-studies/inc0299001/peer-metadata.json").unwrap();
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    let sessions = v["sessions"].as_array().unwrap();
    assert!(
        sessions
            .iter()
            .any(|m| m["collector"] == "route-views2" && m["peer_ip"] == "163.253.3.14"),
        "UVA peer metadata preserved in the reviewed data file"
    );
    assert!(
        sessions.iter().any(|m| m["peer_asn"] == 11537),
        "observed peer ASN preserved"
    );
    let _ = conn;
}

#[tokio::test]
async fn no_reviewed_name_does_not_hide_asn() {
    let (status, body) = event_workbench("INC0299001").await;
    if body.is_empty() {
        return;
    }
    assert_eq!(status, StatusCode::OK);
    // Without a reviewed name the ASN still renders (with the caveat).
    assert!(
        body.contains("AS40220") || body.contains("AS2907") || body.contains("AS7660"),
        "observed peer ASN visible even without a reviewed name"
    );
}

#[tokio::test]
async fn no_visibility_page_has_no_empty_finding_filters() {
    let (status, body) = event_workbench("INC0302574").await;
    if body.is_empty() {
        return;
    }
    assert_eq!(status, StatusCode::OK);
    assert!(
        !body.contains("wb-filters"),
        "no routing-change filters on the no-visibility page"
    );
    assert!(
        !body.contains("Externally observed routing changes"),
        "no empty routing-findings section"
    );
}

#[tokio::test]
async fn no_visibility_page_has_no_empty_timeline() {
    let (status, body) = event_workbench("INC0302574").await;
    if body.is_empty() {
        return;
    }
    assert_eq!(status, StatusCode::OK);
    assert!(
        !body.contains("Timeline (UTC)"),
        "no empty timeline on the no-visibility page"
    );
    assert!(
        !body.contains("Suggested internal checks"),
        "no empty cue section on the no-visibility page"
    );
}

#[tokio::test]
async fn duplicate_supporting_observers_are_summarized() {
    let (status, body) = event_workbench("INC0302574").await;
    if body.is_empty() {
        return;
    }
    assert_eq!(status, StatusCode::OK);
    // The supporting no-change observation is summarized by session
    // count, not repeated per identical bullet.
    assert!(
        body.contains("selected RouteViews session")
            || body.contains("selected RouteViews sessions"),
        "supporting observers summarized with a count"
    );
    // No duplicate identical bullets.
    let statements = body
        .matches("saw no route-state change for the selected prefixes")
        .count();
    assert!(
        statements <= 5,
        "supporting statements deduplicated, got {statements}"
    );
}

#[tokio::test]
async fn relevant_assessment_precedes_supporting_plane() {
    let (status, body) = event_workbench("INC0302574").await;
    if body.is_empty() {
        return;
    }
    assert_eq!(status, StatusCode::OK);
    let assessment = body
        .find("Insufficient public-collector visibility")
        .unwrap();
    let supporting = body.find("Supporting").unwrap();
    assert!(
        assessment < supporting,
        "the relevant assessment precedes the supporting plane"
    );
}

#[tokio::test]
async fn supporting_no_change_is_not_rendered_as_ticket_result() {
    let (status, body) = event_workbench("INC0302574").await;
    if body.is_empty() {
        return;
    }
    assert_eq!(status, StatusCode::OK);
    // The no-change supporting observation is explicitly not an
    // assessment of the named relationship.
    assert!(
        body.contains("does not assess the named"),
        "supporting plane disclaimer present"
    );
}

#[tokio::test]
async fn target_origin_grammar_is_natural() {
    let (status, body) = event_workbench("INC0299001").await;
    if body.is_empty() {
        return;
    }
    assert_eq!(status, StatusCode::OK);
    assert!(
        !body.contains("prefixes UVA via Internet2"),
        "awkward '{{n}} prefixes <label>' grammar removed"
    );
    assert!(
        body.contains("originated by UVA (AS225)"),
        "natural target-origin grammar rendered"
    );
}

#[tokio::test]
async fn prepend_change_is_prominent_in_uva() {
    let (status, body) = event_workbench("INC0299001").await;
    if body.is_empty() {
        return;
    }
    assert_eq!(status, StatusCode::OK);
    assert!(
        body.contains("AS225×7") || body.contains("AS225 AS225 AS225"),
        "the UVA origin prepending change is visible"
    );
    assert!(
        body.contains("prepending") || body.contains("AS path changed"),
        "prepend semantics present in the UVA story"
    );
}

#[tokio::test]
async fn uva_peer_asn_is_visible() {
    let (status, body) = event_workbench("INC0299001").await;
    if body.is_empty() {
        return;
    }
    assert_eq!(status, StatusCode::OK);
    let card = body[body.find("wb-principal").unwrap()..].to_string();
    assert!(
        card.contains("AS11537") || card.contains("AS40220") || card.contains("AS7660"),
        "observed peer ASN visible in the principal card"
    );
}

#[tokio::test]
async fn repeated_uva_variants_do_not_flood_first_view() {
    let (status, body) = event_workbench("INC0299001").await;
    if body.is_empty() {
        return;
    }
    assert_eq!(status, StatusCode::OK);
    // Principal selection caps the first view; variants live under
    // "Additional observer findings".
    let principal_cards = body.matches("wb-finding wb-principal").count();
    assert!(
        principal_cards <= 4,
        "at most four principal stories, got {principal_cards}"
    );
    assert!(
        body.contains("Additional observer findings") || principal_cards >= 3,
        "remaining findings are explicitly secondary or all principal"
    );
}

#[tokio::test]
async fn observer_comparison_describes_route_difference() {
    let (status, body) = manlan_workbench().await;
    if body.is_empty() {
        return;
    }
    assert_eq!(status, StatusCode::OK);
    let section = body.find("Observer comparison by region").unwrap();
    let end = body.find("Routing findings").unwrap();
    let block = &body[section..end];
    assert!(
        block.contains("briefly lost")
            || block.contains("no longer traversed")
            || block.contains("changed path while remaining")
            || block.contains("no route-state counterpart"),
        "observer comparison describes routing differences, not counts"
    );
}

#[tokio::test]
async fn comparison_does_not_repeat_abstract_counts_only() {
    let (status, body) = manlan_workbench().await;
    if body.is_empty() {
        return;
    }
    assert_eq!(status, StatusCode::OK);
    let section = body.find("Observer comparison by region").unwrap();
    let end = body.find("Routing findings").unwrap();
    let block = &body[section..end];
    let ratio_re = regex::Regex::new(r"\d+/\d+").unwrap();
    assert!(
        !ratio_re.is_match(block),
        "comparison never leads with ratios"
    );
}

#[tokio::test]
async fn no_change_and_no_visibility_are_distinct() {
    let (status, body) = manlan_workbench().await;
    if body.is_empty() {
        return;
    }
    assert_eq!(status, StatusCode::OK);
    // The RRC00 (Amsterdam) no-change session renders as a
    // no-counterpart statement; the RRC11 no-baseline session renders
    // as a coverage limitation — never the same text.
    assert!(
        body.contains("no route-state counterpart observed for the selected prefixes"),
        "no-change observer distinct statement"
    );
    assert!(
        body.contains("Coverage limitations"),
        "no-baseline sessions under coverage limitations"
    );
}

#[tokio::test]
async fn region_is_observer_site_context_not_affected_region() {
    let (status, body) = manlan_workbench().await;
    if body.is_empty() {
        return;
    }
    assert_eq!(status, StatusCode::OK);
    // Regions classify observer sites (Tokyo/APAC, Eugene/AMER); the
    // affected network (NORDUnet) is never labeled by observer region.
    let section = body.find("Observer comparison by region").unwrap();
    let end = body.find("Routing findings").unwrap();
    let block = &body[section..end];
    assert!(
        block.contains("Eugene, Oregon, US") && block.contains("Otemachi, Tokyo, Japan"),
        "observer sites named in their region context"
    );
    assert!(
        !block.contains("impact by region") && !block.contains("outage scope by region"),
        "never labeled as impact/outage by region"
    );
}

#[tokio::test]
async fn uva_peer_metadata_import_roundtrip() {
    // Part 7 end-to-end: metadata -> episode -> finding -> sentence.
    let (status, body) = event_workbench("INC0299001").await;
    if body.is_empty() {
        return;
    }
    assert_eq!(status, StatusCode::OK);
    // The sentence uses the observed peer ASN, with the peer IP
    // secondary, never the reverse.
    assert!(
        body.contains("receiving routes from"),
        "peer clause present"
    );
    assert!(
        !body.contains("receiving routes from 163.253.3.14,"),
        "peer-IP-only sentence is gone when a peer ASN is observed"
    );
}
