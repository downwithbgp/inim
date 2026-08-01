//! Catalog web interface — read-only localhost server.
//!
//! HTTP requests never perform Broker discovery, downloads, MRT parsing,
//! or analysis. The server only reads the catalog database.

pub mod api;
pub mod handlers;
pub mod server;
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
            "/analyses/{run_id}",
            axum::routing::get(handlers::analysis_detail),
        )
        .route(
            "/analyses/{run_id}/streams",
            axum::routing::get(handlers::analysis_streams),
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
            "/api/v1/catalog/status",
            axum::routing::get(api::api_catalog_status),
        )
        .with_state(state)
}
