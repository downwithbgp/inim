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
}

pub type SharedState = Arc<AppState>;

/// Generate a process-lifetime CSRF token (128 random bits, hex).
pub fn generate_csrf_token() -> String {
    use sha2::Digest;
    let mut hasher = sha2::Sha256::new();
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    hasher.update(nonce.to_le_bytes());
    hasher.update(std::process::id().to_le_bytes());
    let bytes = hasher.finalize();
    bytes.iter().map(|b| format!("{b:02x}")).collect()
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
