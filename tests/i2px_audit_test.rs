//! Session 38, Part 8 — INC0302574 I2PX relationship audit tests.
//!
//! The ticket names an I2PX peer relationship; the historical analysis
//! observed the R&E plane. These tests verify that the ticket assessment
//! uses ONLY relationship-relevant evidence (the reviewed audit), that
//! the R&E run is classified as supporting, that eligibility uses the
//! event-date peer evidence, and that insufficient visibility never
//! becomes "no impact". Test names contain the reviewed plane identity;
//! they live in tests/ because the src/ token gate forbids the plane
//! identity inside the library.

use std::sync::Arc;

use axum::http::{Request, StatusCode};
use tower::ServiceExt;

fn repo_artifacts_available() -> bool {
    std::path::Path::new("manifests").is_dir()
        && std::path::Path::new("out/INC0302574/report.json").is_file()
}

fn setup_app() -> Option<(tempfile::TempDir, axum::Router)> {
    if !repo_artifacts_available() {
        return None;
    }
    let dbdir = tempfile::tempdir().unwrap();
    let path = dbdir.path().join("catalog.sqlite");
    let conn = inim::catalog::db::open_catalog(&path).unwrap();
    inim::catalog::import::import_repository(&conn, std::path::Path::new("."), "0.1.0", None)
        .unwrap();
    drop(conn);
    let state: Arc<inim::catalog::web::AppState> =
        inim::catalog::web::server::build_state(&path, std::path::Path::new("."), "0.1.0").unwrap();
    Some((dbdir, inim::catalog::web::server::build_app(state)))
}

async fn get(app: &axum::Router, uri: &str) -> (StatusCode, String) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(uri)
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 16 * 1024 * 1024)
        .await
        .unwrap();
    (status, String::from_utf8_lossy(&bytes).to_string())
}

#[tokio::test]
async fn i2px_ticket_does_not_use_re_plane_as_primary_evidence() {
    let Some((_dbdir, app)) = setup_app() else {
        return;
    };
    let (status, body) = get(&app, "/events/INC0302574/workbench").await;
    assert_eq!(status, StatusCode::OK);
    // The primary ticket assessment comes from the I2PX relationship
    // audit (insufficient visibility), NOT from the R&E run's verdict.
    assert!(
        body.contains("Insufficient public-collector visibility for the named I2PX relationship"),
        "relationship-relevant assessment is primary"
    );
    assert!(
        !body.contains(
            "Expectation assessment</dt><dd>Consistent with the redundant-attachment expectation."
        ),
        "the R&E run assessment must not be the primary ticket assessment"
    );
}

#[tokio::test]
async fn supporting_plane_run_is_labeled_supporting() {
    let Some((_dbdir, app)) = setup_app() else {
        return;
    };
    let (status, body) = get(&app, "/events/INC0302574/workbench").await;
    assert_eq!(status, StatusCode::OK);
    // The existing AS11537 run is classified as a supporting R&E-plane
    // observation in the analysis history.
    assert!(
        body.contains("supporting-re-plane"),
        "R&E run classified as supporting"
    );
    assert!(
        body.contains("supporting R&#38;E-plane observation"),
        "supporting classification described in the note"
    );
}

#[tokio::test]
async fn direct_i2px_eligibility_uses_event_date_peer_asn() {
    // The reviewed audit records the event-date (2026-07-30) bview peer
    // evidence: direct AS11164 sessions existed at RRC11/RRC14 with
    // zero AS3333-origin routes — eligibility is established by the
    // event-date peer ASN, not by current peer lists alone.
    let raw = std::fs::read_to_string("out/INC0302574/relationship-audit.json")
        .expect("audit artifact present");
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    let direct = v["direct_i2px_sessions"].as_array().unwrap();
    assert_eq!(direct.len(), 4, "two collectors x two address families");
    for row in direct {
        assert_eq!(row["peer_asn"].as_u64(), Some(11164));
        assert_eq!(row["as3333_origin_route_count"].as_u64(), Some(0));
    }
    assert_eq!(v["decision"].as_str(), Some("insufficient-visibility"));
    let bviews = v["baseline_bviews"].as_array().unwrap();
    assert!(bviews.iter().any(|b| b["collector"] == "rrc11"));
    assert!(bviews.iter().any(|b| b["collector"] == "rrc14"));
}

#[tokio::test]
async fn no_relevant_visibility_does_not_become_no_impact() {
    let Some((_dbdir, app)) = setup_app() else {
        return;
    };
    let (status, body) = get(&app, "/events/INC0302574/workbench").await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        body.contains("Insufficient public-collector visibility"),
        "insufficient visibility stated"
    );
    // The audit explicitly says the relationship CANNOT be assessed —
    // it never claims the I2PX relationship had no impact.
    assert!(
        body.contains("the relationship cannot be assessed"),
        "no impact is not claimed for the unobservable relationship"
    );
}

#[tokio::test]
async fn ticket_assessment_uses_relationship_relevant_runs() {
    let Some((_dbdir, app)) = setup_app() else {
        return;
    };
    // Page and API agree: both carry the audit assessment and the
    // supporting classification.
    let (status, body) = get(&app, "/events/INC0302574/workbench").await;
    assert_eq!(status, StatusCode::OK);
    let (status, api) = get(&app, "/api/v1/events/INC0302574/workbench").await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        api.contains("Insufficient public-collector visibility for the named I2PX relationship"),
        "API carries the relationship assessment"
    );
    assert!(
        api.contains("supporting-re-plane"),
        "API carries the supporting classification"
    );
    assert!(body.contains("supporting-re-plane"));
}

#[tokio::test]
async fn i2px_primary_assessment_uses_relationship_relevant_runs() {
    // Golden (Part 10): the primary assessment for INC0302574 uses only
    // relationship-relevant evidence.
    let Some((_dbdir, app)) = setup_app() else {
        return;
    };
    let (status, body) = get(&app, "/events/INC0302574/workbench").await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        body.contains("Ticket relationship assessment:"),
        "audit block present"
    );
    // The observed result still reports the supporting R&E observation
    // but the expectation/assessment fields come from the audit.
    assert!(
        body.contains("No route-state change at 4 of 4 eligible observer sessions"),
        "supporting R&E breadth shown"
    );
}
