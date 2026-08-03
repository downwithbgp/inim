//! Job-workflow web handlers: plan review/edit, queue, cancel, retry,
//! and the job pages.
//!
//! Execution boundary: these handlers NEVER parse MRT, download
//! archives, discover sources, or run analysis. POST handlers only
//! validate the mutation request and call the shared job service. GET
//! handlers are database-read-only.

use axum::extract::{Path as AxumPath, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Redirect, Response};
use axum::Form;
use serde::Deserialize;

use super::handlers::{json_error, server_error};
use super::{SharedState, MAX_MUTATION_BODY_BYTES};

/// 404 for mutation routes when writes are disabled (one consistent
/// policy: the route does not exist for read-only servers).
fn writes_disabled() -> Response {
    (
        StatusCode::NOT_FOUND,
        "writes are disabled on this server (start with --enable-writes for local mutations)",
    )
        .into_response()
}

/// Validate the mutation gate: writes enabled + CSRF token present and
/// correct. Returns an error Response when the request is rejected.
fn check_mutation(state: &SharedState, csrf: Option<&str>) -> Result<(), Box<Response>> {
    if !state.writes_enabled {
        return Err(Box::new(writes_disabled()));
    }
    let supplied = csrf.unwrap_or("");
    if supplied.is_empty() || supplied != state.csrf_token {
        return Err(Box::new(
            (
                StatusCode::FORBIDDEN,
                "invalid or missing CSRF token; reload the page and retry",
            )
                .into_response(),
        ));
    }
    Ok(())
}

/// Read the CSRF token from the X-Inim-CSRF header or the `_csrf` form
/// field. The token is never logged and never stored.
fn csrf_from(headers: &axum::http::HeaderMap, form: Option<&str>) -> Option<String> {
    headers
        .get("x-inim-csrf")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .or_else(|| form.map(|s| s.to_string()))
}

// ── Plan review (GET, read-only) ────────────────────────────────────

pub async fn plan_review(
    State(state): State<SharedState>,
    AxumPath(event_id): AxumPath<String>,
) -> Response {
    let db = state.db.lock().unwrap();
    match super::view::load_plan_review(&db, &event_id, state.writes_enabled) {
        Ok(Some(mut view)) => {
            view.csrf_token = state.csrf_token.clone();
            super::handlers::render_view(view)
        }
        Ok(None) => super::handlers::not_found_view("event"),
        Err(e) => server_error(&e),
    }
}

// ── Plan editing (POST, write mode only) ────────────────────────────

#[derive(Debug, Default, Deserialize)]
pub struct PlanEditForm {
    #[serde(default)]
    pub _csrf: String,
    #[serde(default)]
    pub source_family: String,
    #[serde(default)]
    pub collectors: String,
    #[serde(default)]
    pub warmup_minutes: Option<i64>,
    #[serde(default)]
    pub cooldown_minutes: Option<i64>,
    #[serde(default)]
    pub analysis_start: String,
    #[serde(default)]
    pub analysis_end: String,
    #[serde(default)]
    pub analyst_note: String,
    /// Free-form origin ASN entry is NEVER silently reviewed: it marks
    /// the plan NeedsReview until an explicit review happens.
    #[serde(default)]
    pub free_form_origin_asns: String,
}

pub async fn plan_edit(
    State(state): State<SharedState>,
    AxumPath(event_id): AxumPath<String>,
    headers: axum::http::HeaderMap,
    Form(form): Form<PlanEditForm>,
) -> Response {
    if let Err(resp) = check_mutation(&state, csrf_from(&headers, Some(&form._csrf)).as_deref()) {
        return *resp;
    }
    let db = state.db.lock().unwrap();
    match super::view::edit_plan_revision(&db, &event_id, &form) {
        Ok(Some(redirect)) => Redirect::to(&redirect).into_response(),
        Ok(None) => super::handlers::not_found_view("event"),
        Err(e) => {
            let msg = e.replace('<', "&lt;").replace('>', "&gt;");
            (StatusCode::BAD_REQUEST, html_err(&msg)).into_response()
        }
    }
}

fn html_err(msg: &str) -> axum::response::Html<String> {
    axum::response::Html(format!(
        "<h1>Plan edit rejected</h1><p>{msg}</p><p><a href=\"/events\">Back</a></p>"
    ))
}

// ── Queue (POST, write mode only) ───────────────────────────────────

pub async fn queue_plan(
    State(state): State<SharedState>,
    AxumPath(plan_revision_id): AxumPath<i64>,
    headers: axum::http::HeaderMap,
    Form(form): Form<QueueForm>,
) -> Response {
    if let Err(resp) = check_mutation(&state, csrf_from(&headers, Some(&form._csrf)).as_deref()) {
        return *resp;
    }
    let db = state.db.lock().unwrap();
    let manifest_payload =
        match crate::catalog::jobs::plan::manifest_payload_for_plan(&db, plan_revision_id) {
            Ok(p) => p,
            Err(e) => return (StatusCode::BAD_REQUEST, e).into_response(),
        };
    let plan_hash =
        match crate::catalog::jobs::plan::validate_plan_for_queue(&db, plan_revision_id, &state.scope)
        {
            Ok(h) => h,
            Err(e) => return (StatusCode::BAD_REQUEST, e).into_response(),
        };
    let outcome = crate::catalog::jobs::service::queue(
        &db,
        plan_revision_id,
        crate::catalog::jobs::RequestSource::LocalWeb,
        &plan_hash,
        &state.scope,
    );
    let _ = manifest_payload;
    match outcome {
        Ok(crate::catalog::jobs::service::QueueOutcome::Created(job_id)) => {
            Redirect::to(&format!("/analysis-jobs/{job_id}")).into_response()
        }
        Ok(crate::catalog::jobs::service::QueueOutcome::Duplicate(job_id)) => {
            // Idempotent: redirect to the existing active job. The job
            // page states "job already queued".
            Redirect::to(&format!("/analysis-jobs/{job_id}?already-queued=1")).into_response()
        }
        Err(e) => (StatusCode::BAD_REQUEST, e).into_response(),
    }
}

/// Silence an unused-symbol lint for the body-limit constant (the
/// router-level limit is applied in `build_router`).
#[allow(dead_code)]
pub fn body_limit() -> usize {
    MAX_MUTATION_BODY_BYTES
}

#[derive(Debug, Default, Deserialize)]
pub struct QueueForm {
    #[serde(default)]
    pub _csrf: String,
}

// ── Cancel / retry (POST, write mode only) ──────────────────────────

pub async fn job_cancel(
    State(state): State<SharedState>,
    AxumPath(job_id): AxumPath<String>,
    headers: axum::http::HeaderMap,
    Form(form): Form<QueueForm>,
) -> Response {
    if let Err(resp) = check_mutation(&state, csrf_from(&headers, Some(&form._csrf)).as_deref()) {
        return *resp;
    }
    let db = state.db.lock().unwrap();
    match crate::catalog::jobs::service::request_cancel(&db, &job_id) {
        Ok(_) => Redirect::to(&format!("/analysis-jobs/{job_id}")).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, e).into_response(),
    }
}

pub async fn job_retry(
    State(state): State<SharedState>,
    AxumPath(job_id): AxumPath<String>,
    headers: axum::http::HeaderMap,
    Form(form): Form<QueueForm>,
) -> Response {
    if let Err(resp) = check_mutation(&state, csrf_from(&headers, Some(&form._csrf)).as_deref()) {
        return *resp;
    }
    let db = state.db.lock().unwrap();
    let plan_hash = match crate::catalog::jobs::service::get(&db, &job_id) {
        Ok(job) => job.plan_hash,
        Err(e) => return (StatusCode::BAD_REQUEST, e).into_response(),
    };
    match crate::catalog::jobs::service::retry(
        &db,
        &job_id,
        crate::catalog::jobs::RequestSource::LocalWeb,
        &plan_hash,
        &state.scope,
    ) {
        Ok(new_id) => Redirect::to(&format!("/analysis-jobs/{new_id}")).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, e).into_response(),
    }
}

// ── Job pages (GET, read-only) ──────────────────────────────────────

pub async fn jobs_index(State(state): State<SharedState>) -> Response {
    let db = state.db.lock().unwrap();
    match super::view::load_jobs_index(&db, state.writes_enabled, &state.scope) {
        Ok(mut view) => {
            view.csrf_token = state.csrf_token.clone();
            super::handlers::render_view(view)
        }
        Err(e) => server_error(&e),
    }
}

pub async fn job_detail(
    State(state): State<SharedState>,
    AxumPath(job_id): AxumPath<String>,
) -> Response {
    let db = state.db.lock().unwrap();
    match super::view::load_job_detail(&db, &job_id, state.writes_enabled) {
        Ok(Some(mut view)) => {
            view.csrf_token = state.csrf_token.clone();
            super::handlers::render_view(view)
        }
        Ok(None) => super::handlers::not_found_view("analysis job"),
        Err(e) => server_error(&e),
    }
}

// ── API twins ───────────────────────────────────────────────────────

pub async fn api_jobs_index(State(state): State<SharedState>) -> Response {
    let db = state.db.lock().unwrap();
    match super::view::api_jobs_index(&db) {
        Ok(v) => axum::Json(v).into_response(),
        Err(e) => json_error(StatusCode::INTERNAL_SERVER_ERROR, &e),
    }
}

pub async fn api_job_detail(
    State(state): State<SharedState>,
    AxumPath(job_id): AxumPath<String>,
) -> Response {
    let db = state.db.lock().unwrap();
    match super::view::api_job_detail(&db, &job_id) {
        Ok(Some(v)) => axum::Json(v).into_response(),
        Ok(None) => super::handlers::json_error(StatusCode::NOT_FOUND, "analysis job not found"),
        Err(e) => json_error(StatusCode::INTERNAL_SERVER_ERROR, &e),
    }
}

pub async fn api_plan_detail(
    State(state): State<SharedState>,
    AxumPath(plan_revision_id): AxumPath<i64>,
) -> Response {
    let db = state.db.lock().unwrap();
    match super::view::api_plan_detail(&db, plan_revision_id) {
        Ok(Some(v)) => axum::Json(v).into_response(),
        Ok(None) => super::handlers::json_error(StatusCode::NOT_FOUND, "plan revision not found"),
        Err(e) => json_error(StatusCode::INTERNAL_SERVER_ERROR, &e),
    }
}

/// API write endpoint wrapper: write mode + CSRF header + bounded body.
fn api_check_mutation(
    state: &SharedState,
    headers: &axum::http::HeaderMap,
) -> Result<(), Box<Response>> {
    if !state.writes_enabled {
        return Err(Box::new(
            super::handlers::json_error(
                StatusCode::NOT_FOUND,
                "writes are disabled on this server",
            )
            .into_response(),
        ));
    }
    let supplied = headers
        .get("x-inim-csrf")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if supplied.is_empty() || supplied != state.csrf_token {
        return Err(Box::new(
            super::handlers::json_error(
                StatusCode::FORBIDDEN,
                "invalid or missing X-Inim-CSRF header",
            )
            .into_response(),
        ));
    }
    Ok(())
}

pub async fn api_queue_plan(
    State(state): State<SharedState>,
    AxumPath(plan_revision_id): AxumPath<i64>,
    headers: axum::http::HeaderMap,
) -> Response {
    if let Err(resp) = api_check_mutation(&state, &headers) {
        return *resp;
    }
    let db = state.db.lock().unwrap();
    let plan_hash =
        match crate::catalog::jobs::plan::validate_plan_for_queue(&db, plan_revision_id, &state.scope)
        {
        Ok(h) => h,
        Err(e) => return super::handlers::json_error(StatusCode::BAD_REQUEST, &e),
    };
    match crate::catalog::jobs::service::queue(
        &db,
        plan_revision_id,
        crate::catalog::jobs::RequestSource::LocalWeb,
        &plan_hash,
        &state.scope,
    ) {
        Ok(crate::catalog::jobs::service::QueueOutcome::Created(job_id)) => {
            axum::Json(serde_json::json!({
                "api_version": 1,
                "result": "queued",
                "job_id": job_id,
            }))
            .into_response()
        }
        Ok(crate::catalog::jobs::service::QueueOutcome::Duplicate(job_id)) => {
            axum::Json(serde_json::json!({
                "api_version": 1,
                "result": "already_queued",
                "job_id": job_id,
            }))
            .into_response()
        }
        Err(e) => json_error(StatusCode::BAD_REQUEST, &e),
    }
}

pub async fn api_job_cancel(
    State(state): State<SharedState>,
    AxumPath(job_id): AxumPath<String>,
    headers: axum::http::HeaderMap,
) -> Response {
    if let Err(resp) = api_check_mutation(&state, &headers) {
        return *resp;
    }
    let db = state.db.lock().unwrap();
    match crate::catalog::jobs::service::request_cancel(&db, &job_id) {
        Ok(outcome) => {
            let result = match outcome {
                crate::catalog::jobs::service::CancelOutcome::Cancelled(_) => "cancelled",
                crate::catalog::jobs::service::CancelOutcome::Requested(_) => "cancel_requested",
            };
            axum::Json(serde_json::json!({"api_version": 1, "result": result, "job_id": job_id}))
                .into_response()
        }
        Err(e) => json_error(StatusCode::BAD_REQUEST, &e),
    }
}

pub async fn api_job_retry(
    State(state): State<SharedState>,
    AxumPath(job_id): AxumPath<String>,
    headers: axum::http::HeaderMap,
) -> Response {
    if let Err(resp) = api_check_mutation(&state, &headers) {
        return *resp;
    }
    let db = state.db.lock().unwrap();
    let plan_hash = match crate::catalog::jobs::service::get(&db, &job_id) {
        Ok(job) => job.plan_hash,
        Err(e) => return super::handlers::json_error(StatusCode::BAD_REQUEST, &e),
    };
    match crate::catalog::jobs::service::retry(
        &db,
        &job_id,
        crate::catalog::jobs::RequestSource::LocalWeb,
        &plan_hash,
        &state.scope,
    ) {
        Ok(new_id) => axum::Json(serde_json::json!({
            "api_version": 1,
            "result": "retry_created",
            "job_id": new_id,
        }))
        .into_response(),
        Err(e) => json_error(StatusCode::BAD_REQUEST, &e),
    }
}
