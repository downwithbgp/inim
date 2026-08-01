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
