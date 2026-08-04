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
pub(crate) fn render_view<T: askama::Template>(template: T) -> Response {
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

pub(crate) fn not_found_view(kind: &str) -> Response {
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
    match super::view::load_dashboard(&db, &state.scope) {
        Ok(mut view) => {
            view.writes_enabled = state.writes_enabled;
            render_view(view)
        }
        Err(e) => server_error(&e),
    }
}

pub async fn event_list(
    State(state): State<SharedState>,
    Query(filters): Query<EventListFilters>,
) -> Response {
    let db = state.db.lock().unwrap();
    match super::view::load_event_list(&db, &filters, &state.scope) {
        Ok(view) => render_view(view),
        Err(e) => server_error(&e),
    }
}

#[derive(Debug, Default, Deserialize)]
pub struct WorkbenchQuery {
    /// Render all episode detail rows open (deterministic screenshots).
    #[serde(default, deserialize_with = "de_bool_loose")]
    pub expand: bool,
    /// Show only changed episodes (Part 6 filter).
    #[serde(default, deserialize_with = "de_bool_loose")]
    pub changed: bool,
    /// Region filter: AMER | EMEA | APAC | Unknown.
    pub region: Option<String>,
    /// Relationship filter: direct | indirect.
    pub rel: Option<String>,
    /// Effect-kind filter (human slug: absent | withdrawn | path |
    /// plane | prepend | mixed | unchanged).
    pub kind: Option<String>,
    /// Open one episode's detail row (index into the ORDERED episode
    /// list as rendered on the page: changed episodes first by time,
    /// then unchanged; index 0 is the earliest changed episode).
    pub episode: Option<usize>,
    /// Open one episode's prefix drill-down table.
    pub prefixes: Option<usize>,
    /// Focus mode: "timeline" collapses the episode table.
    pub view: Option<String>,
}

/// Accept "1", "true", "yes", "on" as true for boolean query flags
/// (serde_urlencoded parses raw strings, so `?changed=1` must work).
fn de_bool_loose<'de, D>(deserializer: D) -> Result<bool, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    Ok(matches!(
        s.trim().to_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    ))
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
        Ok(Some(view)) => {
            // Direct access to an excluded event is consistently 404:
            // it is not an active project result.
            // Fail closed: if the scope check errors, do not serve the
            // item as an active project result.
            if super::view::event_scope_excluded(&db, &state.scope, &view.event).unwrap_or(true) {
                return not_found_view("event");
            }
            render_view(view)
        }
        Ok(None) => not_found_view("event"),
        Err(e) => server_error(&e),
    }
}

pub async fn case_studies(State(state): State<SharedState>) -> Response {
    let db = state.db.lock().unwrap();
    match super::view::load_case_studies(&db) {
        Ok(view) => render_view(view),
        Err(e) => server_error(&e),
    }
}

pub async fn case_study_detail(
    State(state): State<SharedState>,
    AxumPath(slug): AxumPath<String>,
) -> Response {
    let db = state.db.lock().unwrap();
    match super::view::load_case_study(&db, &slug, &state.catalog_root) {
        Ok(Some(view)) => render_view(view),
        Ok(None) => not_found_view("case study"),
        Err(e) => server_error(&e),
    }
}

/// Event incident workbench — the dense NOC view for one event.
///
/// No analysis or MRT parsing happens on this request path: the view
/// reads catalog tables (indexed), reviewed data files, and immutable
/// report artifacts only. Per-request timing (query count, DB time,
/// model time, render time) is captured for the performance review.
pub async fn event_workbench(
    State(state): State<SharedState>,
    AxumPath(event_id): AxumPath<String>,
    Query(params): Query<WorkbenchQuery>,
) -> Response {
    let mut db = state.db.lock().unwrap();
    let started = std::time::Instant::now();
    QUERY_COUNT.store(0, std::sync::atomic::Ordering::Relaxed);
    #[allow(deprecated)] // connection-local tracer; replaced by trace_v2 in later rusqlite
    db.trace(Some(trace_counter));
    let mut view =
        match super::view::load_event_workbench(&db, &event_id, &state.catalog_root, &params) {
            Ok(Some(view)) => view,
            Ok(None) => return not_found_view("event"),
            Err(e) => return server_error(&e),
        };
    view.timing.sql_query_count = QUERY_COUNT.load(std::sync::atomic::Ordering::Relaxed);
    view.timing.model_time_ms = started.elapsed().as_secs_f64() * 1000.0;
    render_view(view)
}

/// Static SQL statement counter for workbench requests.
static QUERY_COUNT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

fn trace_counter(_sql: &str) {
    QUERY_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

/// Test/debug accessor for the per-request SQL statement counter.
pub fn query_count_debug() -> usize {
    QUERY_COUNT.load(std::sync::atomic::Ordering::Relaxed)
}

/// Case-study incident workbench — the multi-observer view over the
/// case study's linked runs (same reusable view model).
pub async fn case_study_workbench(
    State(state): State<SharedState>,
    AxumPath(slug): AxumPath<String>,
    Query(params): Query<WorkbenchQuery>,
) -> Response {
    let mut db = state.db.lock().unwrap();
    let started = std::time::Instant::now();
    QUERY_COUNT.store(0, std::sync::atomic::Ordering::Relaxed);
    #[allow(deprecated)] // connection-local tracer
    db.trace(Some(trace_counter));
    let mut view =
        match super::view::load_case_study_workbench(&db, &slug, &state.catalog_root, &params) {
            Ok(Some(view)) => view,
            Ok(None) => return not_found_view("case study"),
            Err(e) => return server_error(&e),
        };
    view.timing.sql_query_count = QUERY_COUNT.load(std::sync::atomic::Ordering::Relaxed);
    view.timing.model_time_ms = started.elapsed().as_secs_f64() * 1000.0;
    render_view(view)
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
        Ok(None) => not_found_view("document"),
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
        Ok(Some(view)) => render_view(view),
        Ok(None) => not_found_view("analysis run"),
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
        Ok(Some(view)) => render_view(view),
        Ok(None) => not_found_view("analysis run"),
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

pub(crate) fn server_error(e: &str) -> Response {
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

// ── corpus pages (read-only; no crawling on GET) ───────

pub async fn corpus(State(state): State<SharedState>) -> Response {
    let db = state.db.lock().unwrap();
    match super::view::load_corpus(&db) {
        Ok(view) => render_view(view),
        Err(e) => server_error(&e),
    }
}

pub async fn corpus_sync_runs(State(state): State<SharedState>) -> Response {
    let db = state.db.lock().unwrap();
    match super::view::load_sync_runs(&db) {
        Ok(view) => render_view(view),
        Err(e) => server_error(&e),
    }
}

pub async fn event_relationships(
    State(state): State<SharedState>,
    AxumPath(event_id): AxumPath<String>,
) -> Response {
    let db = state.db.lock().unwrap();
    match super::view::load_event_relationships(&db, &event_id) {
        Ok(Some(view)) => render_view(view),
        Ok(None) => not_found_view("event"),
        Err(e) => server_error(&e),
    }
}

pub async fn analysis_queue(
    State(state): State<SharedState>,
    Query(filters): Query<super::view::QueueFilters>,
) -> Response {
    let db = state.db.lock().unwrap();
    match super::view::load_analysis_queue(&db, &filters, &state.scope) {
        Ok(view) => render_view(view),
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
        Ok(view) => render_view(view),
        Err(e) => server_error(&e),
    }
}
pub async fn corpus_relationships(State(state): State<SharedState>) -> Response {
    let db = state.db.lock().unwrap();
    match super::view::load_corpus_relationships(&db) {
        Ok(view) => render_view(view),
        Err(e) => server_error(&e),
    }
}

pub async fn archive_batches(State(state): State<SharedState>) -> Response {
    let db = state.db.lock().unwrap();
    match super::view::load_archive_batches(&db) {
        Ok(view) => render_view(view),
        Err(e) => server_error(&e),
    }
}
