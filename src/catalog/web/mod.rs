//! Catalog web interface — read-only localhost server.
//!
//! HTTP requests never perform Broker discovery, downloads, MRT parsing,
//! or analysis. The server only reads the catalog database.

pub mod api;
pub mod handlers;
pub mod server;
pub mod session_context;
#[cfg(test)]
pub mod tests;
pub mod view;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use axum::Router;
use rusqlite::Connection;

/// Application state shared by handlers.
#[derive(Debug)]
pub struct AppState {
    pub db: Mutex<Connection>,
    pub catalog_root: PathBuf,
    pub software_version: String,
}

pub type SharedState = Arc<AppState>;

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
            "/api/v1/incident-candidates",
            axum::routing::get(api::api_incident_candidates),
        )
        .route(
            "/api/v1/archive-batches",
            axum::routing::get(api::api_archive_batches),
        )
        .with_state(state)
}
