//! JSON API handlers — versioned, read-only.

use axum::extract::{Path as AxumPath, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Deserialize;

use super::handlers::json_error;
use super::SharedState;

pub const API_VERSION: u32 = 1;

/// Stable envelope: `{ api_version, schema_version, data }`.
fn envelope(payload: serde_json::Value) -> Response {
    Json(serde_json::json!({
        "api_version": API_VERSION,
        "data": payload
    }))
    .into_response()
}

#[allow(clippy::result_large_err)]
fn parse_page(params: &PageParams) -> Result<(usize, usize), Response> {
    let page = params.page.unwrap_or(0);
    let per_page = params.per_page.unwrap_or(25);
    if per_page == 0 || per_page > 200 {
        return Err(json_error(
            StatusCode::BAD_REQUEST,
            "per_page must be between 1 and 200",
        ));
    }
    Ok((page, per_page))
}

#[derive(Debug, Default, Deserialize)]
pub struct PageParams {
    pub page: Option<usize>,
    pub per_page: Option<usize>,
}

#[derive(Debug, Default, Deserialize)]
pub struct StreamQueryParams {
    pub page: Option<usize>,
    pub per_page: Option<usize>,
    pub category: Option<String>,
    pub collector: Option<String>,
}

pub async fn api_events(
    State(state): State<SharedState>,
    Query(params): Query<PageParams>,
) -> Response {
    let (page, per_page) = match parse_page(&params) {
        Ok(p) => p,
        Err(r) => return r,
    };
    let db = state.db.lock().unwrap();
    match super::view::load_event_list_json(&db, page, per_page) {
        Ok(events) => envelope(events),
        Err(e) => json_error(StatusCode::INTERNAL_SERVER_ERROR, &e),
    }
}

pub async fn api_event_detail(
    State(state): State<SharedState>,
    AxumPath(event_id): AxumPath<String>,
) -> Response {
    let db = state.db.lock().unwrap();
    match super::view::load_event_detail_json(&db, &event_id) {
        Ok(Some(v)) => envelope(v),
        Ok(None) => json_error(StatusCode::NOT_FOUND, "event not found"),
        Err(e) => json_error(StatusCode::INTERNAL_SERVER_ERROR, &e),
    }
}

pub async fn api_analysis(
    State(state): State<SharedState>,
    AxumPath(run_id): AxumPath<i64>,
) -> Response {
    let db = state.db.lock().unwrap();
    match super::view::load_run_json(&db, run_id) {
        Ok(Some(v)) => envelope(v),
        Ok(None) => json_error(StatusCode::NOT_FOUND, "analysis run not found"),
        Err(e) => json_error(StatusCode::INTERNAL_SERVER_ERROR, &e),
    }
}

pub async fn api_analysis_streams(
    State(state): State<SharedState>,
    AxumPath(run_id): AxumPath<i64>,
    Query(params): Query<StreamQueryParams>,
) -> Response {
    let (page, per_page) = match parse_page(&PageParams {
        page: params.page,
        per_page: params.per_page,
    }) {
        Ok(p) => p,
        Err(r) => return r,
    };
    let db = state.db.lock().unwrap();
    match super::view::load_streams_json(
        &db,
        run_id,
        page,
        per_page,
        params.category.as_deref(),
        params.collector.as_deref(),
    ) {
        Ok(Some(v)) => envelope(v),
        Ok(None) => json_error(StatusCode::NOT_FOUND, "analysis run not found"),
        Err(e) => json_error(StatusCode::INTERNAL_SERVER_ERROR, &e),
    }
}

pub async fn api_catalog_status(State(state): State<SharedState>) -> Response {
    let db = state.db.lock().unwrap();
    match super::view::load_catalog_status_json(&db) {
        Ok(v) => envelope(v),
        Err(e) => json_error(StatusCode::INTERNAL_SERVER_ERROR, &e),
    }
}

// ── Case-study API (Session 30, Part 12) ───────────────────────────

/// GET /api/v1/case-studies — paginated list.
pub async fn api_case_studies(
    State(state): State<SharedState>,
    Query(params): Query<PageParams>,
) -> Response {
    let (page, per_page) = match parse_page(&params) {
        Ok(p) => p,
        Err(r) => return r,
    };
    let db = state.db.lock().unwrap();
    match super::view::load_case_studies(&db) {
        Ok(list) => {
            let all = list
                .rows
                .iter()
                .map(|r| {
                    serde_json::json!({
                        "slug": r.slug,
                        "title": r.title,
                        "date_utc": r.date,
                        "status": r.status,
                        "documents": r.documents,
                        "linked_events": r.events,
                        "linked_analyses": r.runs,
                        "target_research": r.research_state,
                        "latest_result": r.latest_result,
                    })
                })
                .collect::<Vec<_>>();
            let start = (page * per_page).min(all.len());
            let end = (start + per_page).min(all.len());
            envelope(serde_json::json!({
                "total": all.len(),
                "page": page,
                "per_page": per_page,
                "case_studies": &all[start..end],
            }))
        }
        Err(e) => super::handlers::json_error(StatusCode::INTERNAL_SERVER_ERROR, &e),
    }
}

/// GET /api/v1/case-studies/:slug — full metadata (no local paths, no raw
/// extracted text, no internal notes).
pub async fn api_case_study(
    State(state): State<SharedState>,
    AxumPath(slug): AxumPath<String>,
) -> Response {
    let db = state.db.lock().unwrap();
    match super::view::load_case_study(&db, &slug) {
        Ok(Some(v)) => envelope(serde_json::json!({
            "slug": v.slug,
            "title": v.title,
            "date_utc": v.date,
            "status": v.status,
            "summary": v.summary,
            "what_happened": v.what_happened,
            "what_bgp_showed": v.what_bgp_showed,
            "what_bgp_could_not_show": v.what_bgp_could_not_show,
            "phases": v.phases.iter().map(|p| serde_json::json!({
                "label": p.label, "start_utc": p.start_utc, "end_utc": p.end_utc,
                "start_precision": p.start_precision, "end_precision": p.end_precision,
                "description": p.description, "source_section": p.source_section,
                "review_status": p.review_status,
            })).collect::<Vec<_>>(),
            "related_tickets": v.related_tickets.iter().map(|t| serde_json::json!({
                "external_identifier": t.external_id, "relationship": t.relationship,
                "reviewed_note": t.reviewed_note, "linked_event": t.event_href.is_some(),
            })).collect::<Vec<_>>(),
            "documents": v.documents.iter().map(|d| serde_json::json!({
                "id": d.id, "title": d.title, "source_url": d.source_url,
                "doc_type": d.doc_type, "media_type": d.media_type, "sha256": d.sha256,
                "pages": d.pages, "redistribution_status": d.redistribution_status,
                "provenance": d.provenance, "local_copy": d.href.is_some(),
            })).collect::<Vec<_>>(),
            "targets": v.targets.iter().map(|t| serde_json::json!({
                "source_label": t.source_label, "role_in_report": t.role_in_report,
                "candidate_org": t.candidate_org, "candidate_origin_asns": t.candidate_asns,
                "historical_validity_status": t.historical_validity_status,
                "research_status": t.research_status, "provenance": t.provenance,
            })).collect::<Vec<_>>(),
            "analysis_plan": v.plan.as_ref().map(|p| serde_json::json!({
                "status": p.status, "warmup_start_utc": p.warmup_start,
                "incident_start_utc": p.incident_start, "incident_end_utc": p.incident_end,
                "cooldown_end_utc": p.cooldown_end, "collectors": p.collectors,
                "estimated_bytes": p.estimated_bytes, "blocked_targets": p.blocked_targets,
                "notes": p.notes,
            })),
            "runs": v.runs.iter().map(|r| serde_json::json!({
                "id": r.id, "started_at": r.started_at, "verdict": r.verdict,
                "assessment": r.assessment,
            })).collect::<Vec<_>>(),
            "observability": {
                "potentially_visible": v.observability_potentially_visible,
                "indirectly_visible": v.observability_indirectly_visible,
                "not_directly_visible": v.observability_not_directly_visible,
                "unknown": v.observability_unknown,
            },
        })),
        Ok(None) => super::handlers::json_error(StatusCode::NOT_FOUND, "case study not found"),
        Err(e) => super::handlers::json_error(StatusCode::INTERNAL_SERVER_ERROR, &e),
    }
}

/// GET /api/v1/case-studies/:slug/timeline — reviewed phases plus
/// phase-conditioned run summaries.
pub async fn api_case_study_timeline(
    State(state): State<SharedState>,
    AxumPath(slug): AxumPath<String>,
) -> Response {
    let db = state.db.lock().unwrap();
    match super::view::load_case_study(&db, &slug) {
        Ok(Some(v)) => envelope(serde_json::json!({
            "slug": v.slug,
            "phases": v.phases.iter().map(|p| serde_json::json!({
                "label": p.label, "start_utc": p.start_utc, "end_utc": p.end_utc,
                "start_precision": p.start_precision, "end_precision": p.end_precision,
                "description": p.description, "source_section": p.source_section,
            })).collect::<Vec<_>>(),
            "phase_summaries": v.phase_summaries.iter().map(|p| serde_json::json!({
                "run_id": p.run_id, "phase": p.phase_label,
                "active_streams_entering": p.active_streams_entering,
                "announcements": p.announcements, "withdrawals": p.withdrawals,
                "path_changes": p.path_changes, "transit_departures": p.transit_departures,
                "restorations": p.restorations, "semantic_waves": p.semantic_waves,
                "first_evidence_utc": p.first_evidence_utc,
                "last_evidence_utc": p.last_evidence_utc,
                "evidence_observation_ids": p.evidence_observation_ids,
            })).collect::<Vec<_>>(),
        })),
        Ok(None) => super::handlers::json_error(StatusCode::NOT_FOUND, "case study not found"),
        Err(e) => super::handlers::json_error(StatusCode::INTERNAL_SERVER_ERROR, &e),
    }
}

/// GET /api/v1/case-studies/:slug/comparison — reviewed comparison rows.
pub async fn api_case_study_comparison(
    State(state): State<SharedState>,
    AxumPath(slug): AxumPath<String>,
) -> Response {
    let db = state.db.lock().unwrap();
    match super::view::load_case_study(&db, &slug) {
        Ok(Some(v)) => envelope(serde_json::json!({
            "slug": v.slug,
            "rows": v.comparison.iter().map(|c| serde_json::json!({
                "operator_report": c.operator_report,
                "operator_time": c.operator_time,
                "bgp_observation": c.bgp_observation,
                "interpretation": c.interpretation,
                "temporal_detail": c.temporal_detail,
                "limitation": c.limitation,
            })).collect::<Vec<_>>(),
        })),
        Ok(None) => super::handlers::json_error(StatusCode::NOT_FOUND, "case study not found"),
        Err(e) => super::handlers::json_error(StatusCode::INTERNAL_SERVER_ERROR, &e),
    }
}

/// GET /api/v1/events/:event_id/workbench — full workbench view model.
pub async fn api_event_workbench(
    State(state): State<SharedState>,
    AxumPath(event_id): AxumPath<String>,
) -> Response {
    let db = state.db.lock().unwrap();
    match super::view::load_event_workbench(
        &db,
        &event_id,
        &state.catalog_root,
        &crate::catalog::web::handlers::WorkbenchQuery::default(),
    ) {
        Ok(Some(v)) => envelope(serde_json::to_value(v.vm).unwrap_or_default()),
        Ok(None) => super::handlers::json_error(StatusCode::NOT_FOUND, "event not found"),
        Err(e) => super::handlers::json_error(StatusCode::INTERNAL_SERVER_ERROR, &e),
    }
}

/// GET /api/v1/analyses/:run_id/observer-episodes — episodes of one run.
///
/// No absolute paths are exposed; every field is presentation data.
pub async fn api_run_observer_episodes(
    State(state): State<SharedState>,
    AxumPath(run_id): AxumPath<i64>,
) -> Response {
    let db = state.db.lock().unwrap();
    match super::view::load_run_workbench_slice(&db, run_id) {
        Ok(Some(v)) => envelope(serde_json::json!({
            "run_id": run_id,
            "episodes": v,
        })),
        Ok(None) => super::handlers::json_error(StatusCode::NOT_FOUND, "analysis run not found"),
        Err(e) => super::handlers::json_error(StatusCode::INTERNAL_SERVER_ERROR, &e),
    }
}

/// GET /api/v1/analyses/:run_id/regional-breadth — breadth of one run.
pub async fn api_run_regional_breadth(
    State(state): State<SharedState>,
    AxumPath(run_id): AxumPath<i64>,
) -> Response {
    let db = state.db.lock().unwrap();
    match super::view::load_run_breadth_slice(&db, run_id) {
        Ok(Some(v)) => envelope(serde_json::json!({
            "run_id": run_id,
            "regional_breadth": v,
        })),
        Ok(None) => super::handlers::json_error(StatusCode::NOT_FOUND, "analysis run not found"),
        Err(e) => super::handlers::json_error(StatusCode::INTERNAL_SERVER_ERROR, &e),
    }
}

// ── Session 33: corpus API (read-only) ─────────────────────────────

pub async fn api_corpus_status(State(state): State<SharedState>) -> Response {
    let db = state.db.lock().unwrap();
    match super::view::load_corpus(&db) {
        Ok(v) => envelope(serde_json::to_value(v).unwrap_or_default()),
        Err(e) => json_error(StatusCode::INTERNAL_SERVER_ERROR, &e),
    }
}

pub async fn api_corpus_sync_runs(State(state): State<SharedState>) -> Response {
    let db = state.db.lock().unwrap();
    match super::view::load_sync_runs(&db) {
        Ok(v) => envelope(serde_json::to_value(v).unwrap_or_default()),
        Err(e) => json_error(StatusCode::INTERNAL_SERVER_ERROR, &e),
    }
}

pub async fn api_event_relationships(
    State(state): State<SharedState>,
    AxumPath(event_id): AxumPath<String>,
) -> Response {
    let db = state.db.lock().unwrap();
    match super::view::load_event_relationships(&db, &event_id) {
        Ok(Some(v)) => envelope(serde_json::to_value(v).unwrap_or_default()),
        Ok(None) => json_error(StatusCode::NOT_FOUND, "event not found"),
        Err(e) => json_error(StatusCode::INTERNAL_SERVER_ERROR, &e),
    }
}

pub async fn api_analysis_queue(State(state): State<SharedState>) -> Response {
    let db = state.db.lock().unwrap();
    let filters = super::view::QueueFilters::default();
    match super::view::load_analysis_queue(&db, &filters) {
        Ok(v) => envelope(serde_json::to_value(v).unwrap_or_default()),
        Err(e) => json_error(StatusCode::INTERNAL_SERVER_ERROR, &e),
    }
}

pub async fn api_incident_candidates(State(state): State<SharedState>) -> Response {
    let db = state.db.lock().unwrap();
    match super::view::load_incident_candidates(&db, false) {
        Ok(v) => envelope(serde_json::to_value(v).unwrap_or_default()),
        Err(e) => json_error(StatusCode::INTERNAL_SERVER_ERROR, &e),
    }
}

pub async fn api_archive_batches(State(state): State<SharedState>) -> Response {
    let db = state.db.lock().unwrap();
    match super::view::load_archive_batches(&db) {
        Ok(v) => envelope(serde_json::to_value(v).unwrap_or_default()),
        Err(e) => json_error(StatusCode::INTERNAL_SERVER_ERROR, &e),
    }
}
