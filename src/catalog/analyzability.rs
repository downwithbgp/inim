//! Corpus-level BGP-analysis readiness (Session 33, Part 8).
//!
//! For every catalog event a readiness record is DERIVED from reviewed
//! inputs (manifests, plans, runs, snapshots) — never stored as a single
//! mutable truth field, mirroring `CatalogStatus`. The analyzability
//! state is separate from ticket lifecycle, catalog synchronization
//! status, and the final BGP verdict: a closed ticket can be
//! `AnalysisComplete`, an open ticket can be `NotReviewed`.
//!
//! States (the readiness vocabulary):
//! NotReviewed, NeedsEntityMapping, NeedsTransitPredicate,
//! NeedsAnalysisWindow, NotApplicableToPublicBgp,
//! ReadyForArchivePlanning, ArchivePlanReady, InsufficientBaselineVisibility,
//! AnalysisComplete, AnalysisStale, AnalysisFailed, AnalysisRunning.

use rusqlite::Connection;

use super::domain::*;
use super::status::{self, CatalogStatus};

/// Readiness states (stable strings, not enums, so storage/API are
/// forward-compatible).
pub mod state {
    pub const NOT_REVIEWED: &str = "NotReviewed";
    pub const NEEDS_ENTITY_MAPPING: &str = "NeedsEntityMapping";
    pub const NEEDS_TRANSIT_PREDICATE: &str = "NeedsTransitPredicate";
    pub const NEEDS_ANALYSIS_WINDOW: &str = "NeedsAnalysisWindow";
    pub const NOT_APPLICABLE: &str = "NotApplicableToPublicBgp";
    pub const READY_FOR_ARCHIVE_PLANNING: &str = "ReadyForArchivePlanning";
    pub const ARCHIVE_PLAN_READY: &str = "ArchivePlanReady";
    pub const INSUFFICIENT_BASELINE_VISIBILITY: &str = "InsufficientBaselineVisibility";
    pub const ANALYSIS_COMPLETE: &str = "AnalysisComplete";
    pub const ANALYSIS_STALE: &str = "AnalysisStale";
    pub const ANALYSIS_FAILED: &str = "AnalysisFailed";
    pub const ANALYSIS_RUNNING: &str = "AnalysisRunning";
}

/// The derived readiness record for one event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Analyzability {
    pub event_id: i64,
    pub external_id: String,
    pub readiness: String,
    /// Why the event is not ready (or what keeps it from advancing).
    pub reason: String,
}

/// Derive the BGP-analysis readiness for one event.
pub fn derive_analyzability(
    conn: &Connection,
    event: &CatalogEvent,
) -> Result<Analyzability, String> {
    let status = status::derive_status(conn, event.id)?;
    let manifests = super::db::list_manifest_revisions(conn, event.id)?;
    let plans: Vec<AnalysisPlanRecord> = {
        let mut out = Vec::new();
        for m in &manifests {
            out.extend(super::db::list_plans_for_manifest(conn, m.id)?);
        }
        out
    };
    let runs = super::db::list_runs_for_event(conn, event.id)?;
    let latest_plan = plans.first();
    let latest_run = runs.first();

    let readiness = match status {
        CatalogStatus::Running => {
            (state::ANALYSIS_RUNNING, "an analysis run is in progress".to_string())
        }
        CatalogStatus::Failed => (
            state::ANALYSIS_FAILED,
            latest_run
                .map(|r| format!("latest run {} failed", r.id))
                .unwrap_or_else(|| "the latest attempted analysis failed".to_string()),
        ),
        CatalogStatus::Stale => (
            state::ANALYSIS_STALE,
            "the latest source snapshot or reviewed manifest changed after the latest completed analysis"
                .to_string(),
        ),
        CatalogStatus::Complete => (
            state::ANALYSIS_COMPLETE,
            "the latest reviewed manifest produced a ready plan and a completed run exists"
                .to_string(),
        ),
        CatalogStatus::Blocked => match latest_plan.map(|p| p.block_reason.as_deref()) {
            Some(Some(reason)) if reason.contains("NotApplicable") || reason.contains("not applicable") => (
                state::NOT_APPLICABLE,
                "reviewed as not applicable to public BGP".to_string(),
            ),
            Some(Some(reason)) if reason.contains("window") || reason.contains("Window") => (
                state::NEEDS_ANALYSIS_WINDOW,
                format!("plan blocked: {reason}"),
            ),
            Some(Some(reason)) if reason.contains("insufficient") || reason.contains("Insufficient") => (
                state::INSUFFICIENT_BASELINE_VISIBILITY,
                format!("plan blocked: {reason}"),
            ),
            Some(Some(reason)) => (
                state::NEEDS_TRANSIT_PREDICATE,
                format!("plan blocked: {reason}"),
            ),
            _ => (
                state::NEEDS_TRANSIT_PREDICATE,
                "the latest plan is blocked".to_string(),
            ),
        },
        CatalogStatus::NeedsReview => {
            let manifest = manifests.first();
            match manifest {
                None => (
                    state::NOT_REVIEWED,
                    "no reviewed manifest exists; nothing has been mapped".to_string(),
                ),
                Some(manifest) => {
                    let payload: serde_json::Value =
                        serde_json::from_str(&manifest.payload).unwrap_or_default();
                    let target = payload.get("target");
                    let has_mapping = target
                        .and_then(|t| t.get("origin_asns"))
                        .map(|v| !v.is_null() && v.as_array().map(|a| !a.is_empty()).unwrap_or(false))
                        .unwrap_or(false);
                    let predicate = target
                        .and_then(|t| t.get("transit_predicate"))
                        .and_then(|p| p.get("status"))
                        .and_then(|s| s.as_str())
                        .unwrap_or("Unresolved");
                    if !has_mapping {
                        (
                            state::NEEDS_ENTITY_MAPPING,
                            "a manifest exists but no reviewed entity/ASN mapping".to_string(),
                        )
                    } else if predicate != "Reviewed" {
                        (
                            state::NEEDS_TRANSIT_PREDICATE,
                            "entity mapping exists but the transit predicate is not Reviewed"
                                .to_string(),
                        )
                    } else {
                        (
                            state::NEEDS_ANALYSIS_WINDOW,
                            "mapping and predicate reviewed; no analysis window planned".to_string(),
                        )
                    }
                }
            }
        }
        CatalogStatus::Ready => {
            // Ready means the latest reviewed manifest produces a ready
            // plan. If the event's case study has a stored archive plan,
            // the archive plan is ready too.
            let has_case_study_plan = {
                let mut stmt = conn
                    .prepare(
                        "SELECT COUNT(*) FROM case_study_analysis_plans p
                         JOIN case_study_event_links l ON l.case_study_id = p.case_study_id
                         WHERE l.catalog_event_id = ?1",
                    )
                    .map_err(|e| format!("catalog read failed: {e}"))?;
                let count: i64 = stmt
                    .query_row([event.id], |r| r.get(0))
                    .map_err(|e| format!("catalog read failed: {e}"))?;
                count > 0
            };
            if has_case_study_plan {
                (
                    state::ARCHIVE_PLAN_READY,
                    "a ready plan exists and the case study archive plan is stored".to_string(),
                )
            } else {
                (
                    state::READY_FOR_ARCHIVE_PLANNING,
                    "the latest reviewed manifest produces a ready plan".to_string(),
                )
            }
        }
        CatalogStatus::Discovered => (
            state::NOT_REVIEWED,
            "the source event exists but has never been reviewed".to_string(),
        ),
    };

    Ok(Analyzability {
        event_id: event.id,
        external_id: event.external_id.clone(),
        readiness: readiness.0.to_string(),
        reason: readiness.1,
    })
}

/// Derive readiness for every event, newest first.
pub fn derive_all_analyzability(conn: &Connection) -> Result<Vec<Analyzability>, String> {
    let mut out = Vec::new();
    for event in super::db::list_events(conn)? {
        out.push(derive_analyzability(conn, &event)?);
    }
    out.sort_by(|a, b| b.external_id.cmp(&a.external_id));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::db;
    use crate::catalog::grnoc::source_item_from_fixture;
    use crate::catalog::sync::sync_catalog;
    use std::path::Path;

    fn open_temp_db() -> (tempfile::TempDir, Connection) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("catalog.sqlite");
        let conn = db::open_catalog(&path).unwrap();
        (dir, conn)
    }

    fn seed_event(conn: &Connection, external_id: &str) -> i64 {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("e.json"),
            serde_json::json!({
                "number": external_id,
                "short_description": "Outage - Test",
                "start": "2026-07-28T04:35:00Z",
                "end": "2026-07-28T05:00:00Z",
            })
            .to_string(),
        )
        .unwrap();
        let source = crate::catalog::grnoc::GrnocCatalogSource::new(
            dir.path().to_path_buf(),
            "2026-08-01T00:00:00Z".into(),
        );
        sync_catalog(conn, &source, "2026-08-01T00:00:00Z").unwrap();
        let event = db::get_event_by_external(conn, "grnoc-public-task-viewer", external_id)
            .unwrap()
            .unwrap();
        event.id
    }

    #[test]
    fn acquired_ticket_without_review_is_not_ready() {
        let (_dir, conn) = open_temp_db();
        let event_id = seed_event(&conn, "INC0099901");
        let event = db::get_event(&conn, event_id).unwrap().unwrap();
        let a = derive_analyzability(&conn, &event).unwrap();
        assert_eq!(a.readiness, state::NOT_REVIEWED);
        assert!(a.reason.contains("never been reviewed"));
    }

    #[test]
    fn reviewed_mapping_without_predicate_needs_predicate() {
        let (_dir, conn) = open_temp_db();
        let event_id = seed_event(&conn, "INC0099902");
        let snapshots = db::list_snapshots(&conn, event_id).unwrap();
        let payload = serde_json::json!({
            "target": {
                "label": "Test Target",
                "origin_asns": [11550],
                "transit_predicate": {"status": "Unresolved"}
            }
        })
        .to_string();
        let manifest =
            crate::catalog::tests::sample_manifest_revision(event_id, snapshots[0].id, &payload);
        super::super::store::insert_manifest_revision(&conn, &manifest).unwrap();
        let event = db::get_event(&conn, event_id).unwrap().unwrap();
        let a = derive_analyzability(&conn, &event).unwrap();
        assert_eq!(a.readiness, state::NEEDS_TRANSIT_PREDICATE);
    }

    #[test]
    fn non_bgp_service_can_be_marked_not_applicable() {
        let (_dir, conn) = open_temp_db();
        let event_id = seed_event(&conn, "INC0099903");
        let snapshots = db::list_snapshots(&conn, event_id).unwrap();
        let payload = r#"{"target":{"label":"L2 only","origin_asns":[1],"transit_predicate":{"status":"Reviewed"}}}"#;
        let manifest =
            crate::catalog::tests::sample_manifest_revision(event_id, snapshots[0].id, payload);
        let mid = super::super::store::insert_manifest_revision(&conn, &manifest).unwrap();
        let plan = crate::catalog::tests::sample_plan(mid, "Blocked");
        let plan = crate::catalog::domain::AnalysisPlanRecord {
            block_reason: Some("not applicable to public BGP (reviewed)".to_string()),
            ..plan
        };
        super::super::store::insert_plan(&conn, &plan).unwrap();
        let event = db::get_event(&conn, event_id).unwrap().unwrap();
        let a = derive_analyzability(&conn, &event).unwrap();
        assert_eq!(a.readiness, state::NOT_APPLICABLE);
    }

    #[test]
    fn completed_analysis_is_distinct_from_ticket_closed_state() {
        let (_dir, conn) = open_temp_db();
        let event_id = seed_event(&conn, "INC0099904");
        let snapshots = db::list_snapshots(&conn, event_id).unwrap();
        let payload = r#"{"target":{"label":"T","origin_asns":[1],"transit_predicate":{"status":"Reviewed"}}}"#;
        let manifest =
            crate::catalog::tests::sample_manifest_revision(event_id, snapshots[0].id, payload);
        let mid = super::super::store::insert_manifest_revision(&conn, &manifest).unwrap();
        let pid = super::super::store::insert_plan(
            &conn,
            &crate::catalog::tests::sample_plan(mid, "Ready"),
        )
        .unwrap();
        let run = crate::catalog::tests::sample_run(pid, "2026-08-01T00:00:00Z");
        let run = crate::catalog::domain::AnalysisRun {
            status: "Complete".to_string(),
            verdict: Some("NoObservableBgpImpact".to_string()),
            ..run
        };
        super::super::store::insert_run(&conn, &run).unwrap();
        let event = db::get_event(&conn, event_id).unwrap().unwrap();
        let a = derive_analyzability(&conn, &event).unwrap();
        // The ticket lifecycle (closed) is irrelevant to readiness:
        // a completed analysis is AnalysisComplete.
        assert_eq!(a.readiness, state::ANALYSIS_COMPLETE);
    }

    #[test]
    fn changed_snapshot_marks_analysis_stale() {
        let (_dir, conn) = open_temp_db();
        let event_id = seed_event(&conn, "INC0099905");
        let snapshots = db::list_snapshots(&conn, event_id).unwrap();
        let payload = r#"{"target":{"label":"T","origin_asns":[1],"transit_predicate":{"status":"Reviewed"}}}"#;
        let manifest =
            crate::catalog::tests::sample_manifest_revision(event_id, snapshots[0].id, payload);
        let mid = super::super::store::insert_manifest_revision(&conn, &manifest).unwrap();
        let pid = super::super::store::insert_plan(
            &conn,
            &crate::catalog::tests::sample_plan(mid, "Ready"),
        )
        .unwrap();
        let run = crate::catalog::tests::sample_run(pid, "2026-08-01T00:00:00Z");
        super::super::store::insert_run(&conn, &run).unwrap();
        // The source snapshot changes after the completed run.
        let item = source_item_from_fixture(
            Path::new("tests/fixtures/grnoc/INC0301970.json"),
            "2026-08-01T02:00:00Z",
        )
        .unwrap();
        let mut n: serde_json::Value = serde_json::from_str(&item.normalized_json).unwrap();
        n["id"] = serde_json::json!("INC0099905");
        n["title"] = serde_json::json!("Outage - Test (changed)");
        let item = crate::catalog::domain::CatalogSourceItem {
            external_id: "INC0099905".to_string(),
            normalized_json: n.to_string(),
            ..item
        };
        super::super::sync::record_fetch(
            &conn,
            1,
            None,
            Some(&item),
            &crate::catalog::sync::FetchMetadata {
                source_url: "https://ticket-viewer.grnoc.iu.edu/tickets/INC0099905/".to_string(),
                http_status: 200,
                content_type: Some("application/json".to_string()),
                etag: None,
                last_modified: None,
                acquisition_method: "grnoc-viewer-api".to_string(),
                retry_count: 0,
                conditional_requested: false,
            },
            "2026-08-01T02:00:00Z",
        )
        .unwrap();
        let event = db::get_event(&conn, event_id).unwrap().unwrap();
        let a = derive_analyzability(&conn, &event).unwrap();
        assert_eq!(a.readiness, state::ANALYSIS_STALE);
    }

    #[test]
    fn inferred_entity_candidate_is_not_reviewed_mapping() {
        let (_dir, conn) = open_temp_db();
        let event_id = seed_event(&conn, "INC0099906");
        let snapshots = db::list_snapshots(&conn, event_id).unwrap();
        // A manifest whose target has NO origin_asns (only a candidate
        // suggestion in the label) is not a reviewed mapping.
        let payload = r#"{"target":{"label":"Candidate: SampleNet (UNREVIEWED)","transit_predicate":{"status":"Unresolved"}}}"#;
        let manifest =
            crate::catalog::tests::sample_manifest_revision(event_id, snapshots[0].id, payload);
        super::super::store::insert_manifest_revision(&conn, &manifest).unwrap();
        let event = db::get_event(&conn, event_id).unwrap().unwrap();
        let a = derive_analyzability(&conn, &event).unwrap();
        assert_eq!(a.readiness, state::NEEDS_ENTITY_MAPPING);
        assert!(a.reason.contains("no reviewed entity/ASN mapping"));
    }
}
