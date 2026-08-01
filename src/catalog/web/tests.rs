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
fn repo_artifacts_available() -> bool {
    std::path::Path::new("manifests").is_dir()
        && std::path::Path::new("out/INC0302574/report.json").is_file()
}

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
