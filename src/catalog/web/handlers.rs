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

pub async fn case_studies(State(state): State<SharedState>) -> Response {
    let db = state.db.lock().unwrap();
    match super::view::load_case_studies(&db) {
        Ok(view) => render(view),
        Err(e) => server_error(&e),
    }
}

pub async fn case_study_detail(
    State(state): State<SharedState>,
    AxumPath(slug): AxumPath<String>,
) -> Response {
    let db = state.db.lock().unwrap();
    match super::view::load_case_study(&db, &slug) {
        Ok(Some(view)) => render(view),
        Ok(None) => not_found("case study"),
        Err(e) => server_error(&e),
    }
}

/// Serve a validated document file.
///
/// The record must exist, the stored path must stay under the catalog root
/// (canonical containment), the file must exist, its SHA-256 must match the
/// recorded revision, and only allowlisted media types are served inline.
pub async fn serve_document(
    State(state): State<SharedState>,
    AxumPath(document_id): AxumPath<i64>,
) -> Response {
    let db = state.db.lock().unwrap();
    let resolved = super::view::resolve_document_file(&db, &state.catalog_root, document_id);
    match resolved {
        Ok(Some(serve)) => {
            let bytes = match std::fs::read(&serve.path) {
                Ok(b) => b,
                Err(e) => {
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        format!("cannot read document file: {e}"),
                    )
                        .into_response()
                }
            };
            let disposition = if serve.inline { "inline" } else { "attachment" };
            let name = serve
                .path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "document".to_string());
            axum::response::Response::builder()
                .status(StatusCode::OK)
                .header(axum::http::header::CONTENT_TYPE, serve.media_type.clone())
                .header(
                    axum::http::header::CONTENT_DISPOSITION,
                    format!("{disposition}; filename=\"{name}\""),
                )
                .body(axum::body::Body::from(bytes))
                .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
        }
        Ok(None) => not_found("document"),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Html(format!("<h1>Cannot serve document</h1><p>{e}</p>")),
        )
            .into_response(),
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

// ── Session 33: corpus pages (read-only; no crawling on GET) ───────

pub async fn corpus(State(state): State<SharedState>) -> Response {
    let db = state.db.lock().unwrap();
    match super::view::load_corpus(&db) {
        Ok(view) => render(view),
        Err(e) => server_error(&e),
    }
}

pub async fn corpus_sync_runs(State(state): State<SharedState>) -> Response {
    let db = state.db.lock().unwrap();
    match super::view::load_sync_runs(&db) {
        Ok(view) => render(view),
        Err(e) => server_error(&e),
    }
}

pub async fn event_relationships(
    State(state): State<SharedState>,
    AxumPath(event_id): AxumPath<String>,
) -> Response {
    let db = state.db.lock().unwrap();
    match super::view::load_event_relationships(&db, &event_id) {
        Ok(Some(view)) => render(view),
        Ok(None) => not_found("event"),
        Err(e) => server_error(&e),
    }
}

pub async fn analysis_queue(
    State(state): State<SharedState>,
    Query(filters): Query<super::view::QueueFilters>,
) -> Response {
    let db = state.db.lock().unwrap();
    match super::view::load_analysis_queue(&db, &filters) {
        Ok(view) => render(view),
        Err(e) => server_error(&e),
    }
}

pub async fn incident_candidates(
    State(state): State<SharedState>,
    query: axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Response {
    let include_temporal = query
        .get("include")
        .map(|v| v == "temporal")
        .unwrap_or(false);
    let db = state.db.lock().unwrap();
    match super::view::load_incident_candidates(&db, include_temporal) {
        Ok(view) => render(view),
        Err(e) => server_error(&e),
    }
}
pub async fn corpus_relationships(State(state): State<SharedState>) -> Response {
    let db = state.db.lock().unwrap();
    match super::view::load_corpus_relationships(&db) {
        Ok(view) => render(view),
        Err(e) => server_error(&e),
    }
}

pub async fn archive_batches(State(state): State<SharedState>) -> Response {
    let db = state.db.lock().unwrap();
    match super::view::load_archive_batches(&db) {
        Ok(view) => render(view),
        Err(e) => server_error(&e),
    }
}
