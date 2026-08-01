//! Web handlers — server-rendered HTML pages.
//!
//! Read-only: no analysis or MRT parsing happens on any request path.

use axum::extract::{Path as AxumPath, Query, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};
use axum::Json;
use serde::Deserialize;

use super::view::RunView;
use super::SharedState;

/// Render an Askama template or a stable error page.
fn render<T: askama::Template>(template: T) -> Response {
    match template.render() {
        Ok(html) => Html(html).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Html(format!(
                "<h1>Internal error</h1><p>Template rendering failed.</p><p>{e}</p>"
            )),
        )
            .into_response(),
    }
}

fn not_found(kind: &str) -> Response {
    (
        StatusCode::NOT_FOUND,
        Html(format!(
            "<h1>Not found</h1><p>No {kind} matches the requested identifier.</p>"
        )),
    )
        .into_response()
}

// ── Pages ───────────────────────────────────────────────────────────

pub async fn dashboard(State(state): State<SharedState>) -> Response {
    let db = state.db.lock().unwrap();
    match super::view::load_dashboard(&db) {
        Ok(view) => render(view),
        Err(e) => server_error(&e),
    }
}

pub async fn event_list(
    State(state): State<SharedState>,
    Query(filters): Query<EventListFilters>,
) -> Response {
    let db = state.db.lock().unwrap();
    match super::view::load_event_list(&db, &filters) {
        Ok(view) => render(view),
        Err(e) => server_error(&e),
    }
}

#[derive(Debug, Default, Deserialize)]
pub struct EventListFilters {
    pub lifecycle: Option<String>,
    pub status: Option<String>,
    pub expectation: Option<String>,
    pub source: Option<String>,
    pub date_from: Option<String>,
    pub date_to: Option<String>,
    pub q: Option<String>,
}

pub async fn event_detail(
    State(state): State<SharedState>,
    AxumPath(event_id): AxumPath<String>,
) -> Response {
    let db = state.db.lock().unwrap();
    match super::view::load_event_detail(&db, &event_id) {
        Ok(Some(view)) => render(view),
        Ok(None) => not_found("event"),
        Err(e) => server_error(&e),
    }
}

pub async fn analysis_detail(
    State(state): State<SharedState>,
    AxumPath(run_id): AxumPath<i64>,
) -> Response {
    let db = state.db.lock().unwrap();
    match super::view::load_run(&db, run_id, &state) {
        Ok(Some(view)) => render(view),
        Ok(None) => not_found("analysis run"),
        Err(e) => server_error(&e),
    }
}

pub async fn analysis_streams(
    State(state): State<SharedState>,
    AxumPath(run_id): AxumPath<i64>,
    Query(filters): Query<StreamFilters>,
) -> Response {
    let db = state.db.lock().unwrap();
    match super::view::load_run_streams(&db, run_id, &filters) {
        Ok(Some(view)) => render(view),
        Ok(None) => not_found("analysis run"),
        Err(e) => server_error(&e),
    }
}

#[derive(Debug, Default, Deserialize)]
pub struct StreamFilters {
    pub category: Option<String>,
    pub collector: Option<String>,
    pub withdrawn: Option<String>,
    pub transit_departed: Option<String>,
    pub restored: Option<String>,
    pub ambiguous: Option<String>,
}

pub async fn app_css() -> impl IntoResponse {
    (
        [(axum::http::header::CONTENT_TYPE, "text/css; charset=utf-8")],
        super::view::APP_CSS,
    )
}

fn server_error(e: &str) -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Html(format!(
            "<h1>Internal error</h1><p>The catalog query failed.</p><p>{e}</p>"
        )),
    )
        .into_response()
}

/// JSON error envelope for API handlers.
pub fn json_error(status: StatusCode, message: &str) -> Response {
    (
        status,
        Json(serde_json::json!({
            "api_version": 1,
            "error": {"message": message}
        })),
    )
        .into_response()
}

/// Read a run view for both pages and API (shared presentation model).
pub fn run_view(state: &SharedState, run_id: i64) -> Result<Option<RunView>, String> {
    let db = state.db.lock().unwrap();
    super::view::load_run(&db, run_id, state)
}
