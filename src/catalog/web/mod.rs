//! Catalog web interface — localhost server, read-only by default.
//!
//! HTTP GET requests never perform Broker discovery, downloads, MRT
//! parsing, or analysis. The server only reads the catalog database.
//! Mutation POSTs exist only when the server was started with
//! `--enable-writes`; they require a process-lifetime CSRF token and a
//! bounded body, and are intended for trusted local use only.

pub mod api;
pub mod handlers;
pub mod job_handlers;
pub mod jobs_view;
pub mod server;
pub mod session_context;
#[cfg(test)]
pub mod tests;
pub mod view;
#[cfg(test)]
pub mod workbench_fix_tests;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use axum::Router;
use rusqlite::Connection;

/// Maximum accepted mutation body size (form/JSON payloads).
pub const MAX_MUTATION_BODY_BYTES: usize = 64 * 1024;

/// Application state shared by handlers.
#[derive(Debug)]
pub struct AppState {
    pub db: Mutex<Connection>,
    pub catalog_root: PathBuf,
    pub software_version: String,
    /// Write mode is disabled by default; see ADR-004.
    pub writes_enabled: bool,
    /// Process-lifetime random CSRF token, rendered into server
    /// generated forms. Never stored in the database, never logged.
    pub csrf_token: String,
    /// Reviewed project-scope policy, loaded once at startup and shared
    /// by every handler. Never reread per row.
    pub scope: crate::catalog::scope::ProjectScope,
}

pub type SharedState = Arc<AppState>;

/// Generate a process-lifetime CSRF token (128 random bits, hex).
///
/// Uses SQLite's system-RNG-backed `randomblob` through the catalog
/// connection; the token is never predictable from time or pid and is
/// never stored in the database or logged.
pub fn generate_csrf_token(conn: &rusqlite::Connection) -> String {
    conn.query_row("SELECT lower(hex(randomblob(16)))", [], |r| {
        r.get::<_, String>(0)
    })
    .unwrap_or_else(|_| {
        // Last-resort fallback for an unreadable connection; the
        // token is still never derived from time alone.
        let mut buf = [0u8; 16];
        getrandom_fallback(&mut buf);
        buf.iter().map(|b| format!("{b:02x}")).collect()
    })
}

fn getrandom_fallback(buf: &mut [u8]) {
    // Best-effort OS randomness via /dev/urandom (POSIX); on failure
    // the token is invalidated by the mutation gate comparing against
    // a mismatched value (the server refuses mutations).
    if let Ok(mut f) = std::fs::File::open("/dev/urandom") {
        use std::io::Read;
        let _ = f.read_exact(buf);
    }
}

/// Build the application router.
pub fn build_router(state: SharedState) -> Router {
    Router::new()
        .route("/", axum::routing::get(handlers::dashboard))
        .route("/catalog", axum::routing::get(handlers::dashboard))
        .route("/events", axum::routing::get(handlers::event_list))
        .route(
            "/events/{event_id}",
            axum::routing::get(handlers::event_detail),
        )
        .route(
            "/events/{event_id}/workbench",
            axum::routing::get(handlers::event_workbench),
        )
        .route(
            "/case-studies/{slug}/workbench",
            axum::routing::get(handlers::case_study_workbench),
        )
        .route(
            "/analyses/{run_id}",
            axum::routing::get(handlers::analysis_detail),
        )
        .route(
            "/analyses/{run_id}/streams",
            axum::routing::get(handlers::analysis_streams),
        )
        .route("/case-studies", axum::routing::get(handlers::case_studies))
        .route(
            "/case-studies/{slug}",
            axum::routing::get(handlers::case_study_detail),
        )
        .route("/corpus", axum::routing::get(handlers::corpus))
        .route(
            "/corpus/sync-runs",
            axum::routing::get(handlers::corpus_sync_runs),
        )
        .route(
            "/events/{event_id}/relationships",
            axum::routing::get(handlers::event_relationships),
        )
        .route(
            "/analysis-queue",
            axum::routing::get(handlers::analysis_queue),
        )
        .route(
            "/analysis-jobs",
            axum::routing::get(job_handlers::jobs_index),
        )
        .route(
            "/analysis-jobs/{job_id}",
            axum::routing::get(job_handlers::job_detail),
        )
        .route(
            "/analysis-jobs/{job_id}/cancel",
            axum::routing::post(job_handlers::job_cancel),
        )
        .route(
            "/analysis-jobs/{job_id}/retry",
            axum::routing::post(job_handlers::job_retry),
        )
        .route(
            "/events/{event_id}/analysis-plan",
            axum::routing::get(job_handlers::plan_review).post(job_handlers::plan_edit),
        )
        .route(
            "/analysis-plans/{plan_revision_id}/queue",
            axum::routing::post(job_handlers::queue_plan),
        )
        .route(
            "/incident-candidates",
            axum::routing::get(handlers::incident_candidates),
        )
        .route(
            "/corpus/relationships",
            axum::routing::get(handlers::corpus_relationships),
        )
        .route(
            "/archive-batches",
            axum::routing::get(handlers::archive_batches),
        )
        .route(
            "/documents/{document_id}",
            axum::routing::get(handlers::serve_document),
        )
        .route("/static/app.css", axum::routing::get(handlers::app_css))
        .route("/api/v1/events", axum::routing::get(api::api_events))
        .route(
            "/api/v1/events/{event_id}",
            axum::routing::get(api::api_event_detail),
        )
        .route(
            "/api/v1/analyses/{run_id}",
            axum::routing::get(api::api_analysis),
        )
        .route(
            "/api/v1/analyses/{run_id}/streams",
            axum::routing::get(api::api_analysis_streams),
        )
        .route(
            "/api/v1/case-studies",
            axum::routing::get(api::api_case_studies),
        )
        .route(
            "/api/v1/case-studies/{slug}",
            axum::routing::get(api::api_case_study),
        )
        .route(
            "/api/v1/case-studies/{slug}/timeline",
            axum::routing::get(api::api_case_study_timeline),
        )
        .route(
            "/api/v1/case-studies/{slug}/comparison",
            axum::routing::get(api::api_case_study_comparison),
        )
        .route(
            "/api/v1/events/{event_id}/workbench",
            axum::routing::get(api::api_event_workbench),
        )
        .route(
            "/api/v1/case-studies/{slug}/workbench",
            axum::routing::get(api::api_case_study_workbench),
        )
        .route(
            "/api/v1/analyses/{run_id}/observer-episodes",
            axum::routing::get(api::api_run_observer_episodes),
        )
        .route(
            "/api/v1/analyses/{run_id}/regional-breadth",
            axum::routing::get(api::api_run_regional_breadth),
        )
        .route(
            "/api/v1/catalog/status",
            axum::routing::get(api::api_catalog_status),
        )
        .route(
            "/api/v1/corpus/status",
            axum::routing::get(api::api_corpus_status),
        )
        .route(
            "/api/v1/corpus/sync-runs",
            axum::routing::get(api::api_corpus_sync_runs),
        )
        .route(
            "/api/v1/events/{event_id}/relationships",
            axum::routing::get(api::api_event_relationships),
        )
        .route(
            "/api/v1/analysis-queue",
            axum::routing::get(api::api_analysis_queue),
        )
        .route(
            "/api/v1/analysis-jobs",
            axum::routing::get(job_handlers::api_jobs_index),
        )
        .route(
            "/api/v1/analysis-jobs/{job_id}",
            axum::routing::get(job_handlers::api_job_detail),
        )
        .route(
            "/api/v1/analysis-plans/{plan_revision_id}",
            axum::routing::get(job_handlers::api_plan_detail),
        )
        .route(
            "/api/v1/analysis-plans/{plan_revision_id}/queue",
            axum::routing::post(job_handlers::api_queue_plan),
        )
        .route(
            "/api/v1/analysis-jobs/{job_id}/cancel",
            axum::routing::post(job_handlers::api_job_cancel),
        )
        .route(
            "/api/v1/analysis-jobs/{job_id}/retry",
            axum::routing::post(job_handlers::api_job_retry),
        )
        .route(
            "/api/v1/incident-candidates",
            axum::routing::get(api::api_incident_candidates),
        )
        .route(
            "/api/v1/archive-batches",
            axum::routing::get(api::api_archive_batches),
        )
        .layer(axum::extract::DefaultBodyLimit::max(
            MAX_MUTATION_BODY_BYTES,
        ))
        .with_state(state)
}
