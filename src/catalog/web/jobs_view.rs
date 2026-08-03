//! View models + loaders for the plan-review and job pages.
//!
//! GET loaders are database-read-only; they never touch the network,
//! never parse MRT, and never run analysis. The plan-review page
//! clearly separates Reviewed input, Derived execution plan, and
//! Unresolved requirements.

use askama::Template;
use rusqlite::{Connection, OptionalExtension};
use serde::Serialize;

use crate::catalog::jobs::plan::{canonical_plan_hash, manifest_payload_for_plan};
use crate::catalog::jobs::service::{self, JobFilter, WorkerFreshness};
use crate::catalog::jobs::{AnalysisJob, JobState, RequestSource};
use crate::manifest::Manifest;

// ── Plan review ─────────────────────────────────────────────────────

/// A labeled value row for the plan page (no raw JSON in the principal
/// view).
#[derive(Debug, Clone)]
pub struct PlanRow {
    pub label: String,
    pub value: String,
}

#[derive(Template)]
#[template(path = "analysis_plan.html")]
pub struct PlanReviewView {
    pub event_id: String,
    pub title: String,
    pub lifecycle: String,
    pub plan_status: String,
    pub block_reason: String,
    pub reviewed: Vec<PlanRow>,
    pub derived: Vec<PlanRow>,
    pub unresolved: Vec<String>,
    pub warnings: Vec<String>,
    pub plan_revision_id: Option<i64>,
    pub manifest_revision_id: Option<i64>,
    pub plan_sha256: Option<String>,
    pub plan_hash: Option<String>,
    pub ready_to_queue: bool,
    pub writes_enabled: bool,
    pub csrf_token: String,
    pub editable: EditablePlanValues,
}

/// Current editable values rendered into the form.
#[derive(Debug, Clone, Default)]
pub struct EditablePlanValues {
    pub source_family: String,
    pub collectors: String,
    pub warmup_minutes: i64,
    pub cooldown_minutes: i64,
    pub analysis_start: String,
    pub analysis_end: String,
    pub analyst_notes: Vec<String>,
}

/// Load the plan-review page for an event (latest manifest revision +
/// latest plan). Returns None when the event is unknown.
pub fn load_plan_review(
    conn: &Connection,
    event_id: &str,
    writes_enabled: bool,
) -> Result<Option<PlanReviewView>, String> {
    let event: Option<(i64, String, String)> = conn
        .query_row(
            "SELECT e.id, e.external_id, e.first_seen FROM catalog_events e WHERE e.external_id = ?1",
            rusqlite::params![event_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()
        .map_err(|e| format!("cannot load event: {e}"))?;
    let Some((event_row, _, _)) = event else {
        return Ok(None);
    };
    let manifest = service::latest_manifest_revision(conn, event_row)?;
    let plan = service::latest_plan(conn, event_row)?;
    let mut view = PlanReviewView {
        event_id: event_id.to_string(),
        title: String::new(),
        lifecycle: String::new(),
        plan_status: String::new(),
        block_reason: String::new(),
        reviewed: Vec::new(),
        derived: Vec::new(),
        unresolved: Vec::new(),
        warnings: Vec::new(),
        plan_revision_id: None,
        manifest_revision_id: None,
        plan_sha256: None,
        plan_hash: None,
        ready_to_queue: false,
        writes_enabled,
        csrf_token: String::new(),
        editable: EditablePlanValues::default(),
    };
    let mut manifest_value: Option<Manifest> = None;
    if let Some(mr) = &manifest {
        view.manifest_revision_id = Some(mr.id);
        match serde_json::from_str::<Manifest>(&mr.payload) {
            Ok(m) => {
                view.title = m.target.label.clone();
                view.lifecycle = if m.open { "Open" } else { "Closed" }.to_string();
                // ── Reviewed input ────────────────────────────────
                view.reviewed.push(PlanRow {
                    label: "Event role".to_string(),
                    value: event_role(&m).to_string(),
                });
                view.reviewed.push(PlanRow {
                    label: "Target label".to_string(),
                    value: m.target.label.clone(),
                });
                view.reviewed.push(PlanRow {
                    label: "Target origin ASNs".to_string(),
                    value: format!("{:?}", m.target.origin_asns),
                });
                let predicate = &m.target.transit_predicate;
                view.reviewed.push(PlanRow {
                    label: "Named service plane / transit predicate".to_string(),
                    value: format!(
                        "{:?} (review status: {:?})",
                        predicate.predicate, predicate.status
                    ),
                });
                view.reviewed.push(PlanRow {
                    label: "Predicate provenance".to_string(),
                    value: predicate
                        .provenance
                        .as_ref()
                        .map(|p| format!("{} — {}", p.reviewed_by, p.date))
                        .unwrap_or_else(|| "none".to_string()),
                });
                // ── Derived execution plan ────────────────────────
                view.derived.push(PlanRow {
                    label: "Analysis window".to_string(),
                    value: format!("{} → {}", m.event_window_utc.start, m.event_window_utc.end),
                });
                view.derived.push(PlanRow {
                    label: "Warmup".to_string(),
                    value: format!("{} min", m.warmup_minutes),
                });
                view.derived.push(PlanRow {
                    label: "Cooldown".to_string(),
                    value: format!("{} min", m.cooldown_minutes),
                });
                view.derived.push(PlanRow {
                    label: "Source family".to_string(),
                    value: m.source_family.clone(),
                });
                view.derived.push(PlanRow {
                    label: "Collectors".to_string(),
                    value: m.collectors.join(", "),
                });
                view.derived.push(PlanRow {
                    label: "Expected archives".to_string(),
                    value: "estimate after worker discovery (label: estimate)".to_string(),
                });
                if let Ok(h) = canonical_plan_hash(&mr.payload) {
                    view.plan_hash = Some(h);
                }
                view.editable = EditablePlanValues {
                    source_family: m.source_family.clone(),
                    collectors: m.collectors.join(", "),
                    warmup_minutes: m.warmup_minutes,
                    cooldown_minutes: m.cooldown_minutes,
                    analysis_start: m.event_window_utc.start.clone(),
                    analysis_end: m.event_window_utc.end.clone(),
                    analyst_notes: m.analyst_notes.clone(),
                };
                manifest_value = Some(m);
            }
            Err(e) => {
                view.unresolved
                    .push(format!("manifest payload unreadable: {e}"));
            }
        }
    } else {
        view.unresolved.push("no manifest revision".to_string());
    }

    if let Some(p) = &plan {
        view.plan_revision_id = Some(p.id);
        view.plan_sha256 = Some(p.sha256.clone());
        view.plan_status = p.status.clone();
        view.block_reason = p.block_reason.clone().unwrap_or_default();
        if p.status == "Ready" {
            view.ready_to_queue = true;
        }
    } else {
        view.plan_status = "none".to_string();
    }

    // ── Unresolved requirements (exact reasons) ───────────────────
    if let Some(m) = &manifest_value {
        if m.target.origin_asns.is_empty() {
            view.unresolved
                .push("target origin mapping not reviewed".to_string());
            view.ready_to_queue = false;
        }
        if !m.target.transit_predicate.is_ready() {
            view.unresolved
                .push("transit predicate not reviewed".to_string());
            view.ready_to_queue = false;
        }
        if m.event_window_utc.end.is_empty() && !m.open {
            view.unresolved.push("event end unavailable".to_string());
        }
        if !matches!(
            m.source_family.to_lowercase().as_str(),
            "routeviews" | "riperis"
        ) {
            view.unresolved
                .push(format!("source family unsupported: {}", m.source_family));
            view.ready_to_queue = false;
        }
        if m.collectors.is_empty() {
            view.unresolved
                .push("collector selection empty".to_string());
            view.ready_to_queue = false;
        }
        if view.block_reason.contains("OriginMappingNeedsReview") {
            view.unresolved.push(
                "origin ASNs entered free-form are NOT reviewed; review the mapping before queueing"
                    .to_string(),
            );
            view.ready_to_queue = false;
        }
    }
    if view.unresolved.is_empty() && !view.ready_to_queue {
        view.unresolved
            .push("plan is not Ready; see plan status".to_string());
    }

    // Warnings (never hard blockers).
    if let Some(m) = &manifest_value {
        if m.open {
            view.warnings.push(
                "open event: analysis is provisional through the explicit cutoff".to_string(),
            );
        }
        if m.collectors.len() == 1 {
            view.warnings.push("single collector only".to_string());
        }
    }
    Ok(Some(view))
}

fn event_role(m: &Manifest) -> &'static str {
    if m.open {
        "open-event"
    } else {
        "closed-event"
    }
}

/// Apply a bounded plan-edit form to the LATEST manifest revision,
/// creating a new manifest revision + plan revision. Returns the
/// redirect target. Free-form origin ASNs mark the plan NeedsReview.
pub fn edit_plan_revision(
    conn: &Connection,
    event_id: &str,
    form: &super::job_handlers::PlanEditForm,
) -> Result<Option<String>, String> {
    let event: Option<i64> = conn
        .query_row(
            "SELECT id FROM catalog_events WHERE external_id = ?1",
            rusqlite::params![event_id],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| format!("cannot load event: {e}"))?;
    let Some(event_row) = event else {
        return Ok(None);
    };
    let mr = service::latest_manifest_revision(conn, event_row)?
        .ok_or_else(|| "no manifest revision to edit".to_string())?;

    // Queued plans are immutable: if any ACTIVE job references the
    // latest plan, editing is rejected (editing creates a new revision
    // and would bypass the queued exact revision).
    if let Some(plan) = service::latest_plan(conn, event_row)? {
        let active = service::list(
            conn,
            &JobFilter {
                state: None,
                plan_revision_id: Some(plan.id),
            },
        )?
        .into_iter()
        .any(|j| j.state.is_active());
        if active {
            return Err(format!(
                "plan revision {} has an active job; queued plans are immutable — wait or cancel the job",
                plan.id
            ));
        }
    }

    let mut manifest: Manifest = serde_json::from_str(&mr.payload)
        .map_err(|e| format!("manifest payload unreadable: {e}"))?;
    let mut free_form = false;

    if !form.source_family.is_empty() && form.source_family != manifest.source_family {
        manifest.source_family = form.source_family.trim().to_string();
    }
    if !form.collectors.is_empty() {
        let collectors: Vec<String> = form
            .collectors
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        manifest.collectors = collectors;
    }
    if let Some(w) = form.warmup_minutes {
        manifest.warmup_minutes = w;
    }
    if let Some(c) = form.cooldown_minutes {
        manifest.cooldown_minutes = c;
    }
    if !form.analysis_start.is_empty() {
        normalize_rfc3339(&form.analysis_start)?;
        manifest.event_window_utc.start = form.analysis_start.trim().to_string();
    }
    if !form.analysis_end.is_empty() {
        normalize_rfc3339(&form.analysis_end)?;
        manifest.event_window_utc.end = form.analysis_end.trim().to_string();
    }
    if !form.free_form_origin_asns.trim().is_empty() {
        // Free-form ASN entry is NEVER silently reviewed.
        let asns: Vec<u32> = form
            .free_form_origin_asns
            .split(|c: char| c == ',' || c.is_whitespace())
            .filter(|s| !s.is_empty())
            .map(|s| {
                s.parse::<u32>()
                    .map_err(|_| format!("invalid origin ASN: {s}"))
            })
            .collect::<Result<_, _>>()?;
        manifest.target.origin_asns = asns;
        free_form = true;
    }
    if !form.analyst_note.trim().is_empty() {
        manifest
            .analyst_notes
            .push(form.analyst_note.trim().to_string());
    }
    manifest.revision += 1;
    manifest
        .validate()
        .map_err(|e| format!("edited manifest invalid: {e}"))?;

    let payload = serde_json::to_string_pretty(&manifest)
        .map_err(|e| format!("cannot serialize edited manifest: {e}"))?;
    let payload_sha = crate::catalog::document::hex_sha256(payload.as_bytes());
    let review_status = if free_form {
        // The mapping changed without review.
        "NeedsReview"
    } else if manifest.target.transit_predicate.is_ready() {
        "Reviewed"
    } else {
        "Unresolved"
    };
    let revision = crate::catalog::domain::ManifestRevision {
        id: 0,
        event_id: event_row,
        snapshot_id: mr.snapshot_id,
        manifest_schema: crate::manifest::MANIFEST_SCHEMA_VERSION,
        payload: payload.clone(),
        sha256: payload_sha,
        review_status: review_status.to_string(),
        reviewed_at: Some(crate::catalog::jobs::service::now_utc_public()),
        reviewer: manifest
            .target
            .transit_predicate
            .provenance
            .as_ref()
            .map(|p| p.reviewed_by.clone())
            .or_else(|| Some("local-review".to_string())),
    };
    let mr_id = crate::catalog::store::insert_manifest_revision(conn, &revision)?;
    let plan = crate::catalog::import::build_plan_record(conn, mr_id, &manifest, !free_form)?;
    let plan_id = crate::catalog::store::insert_plan(conn, &plan)?;
    Ok(Some(format!(
        "/events/{event_id}/analysis-plan?revised={plan_id}"
    )))
}

fn normalize_rfc3339(s: &str) -> Result<(), String> {
    chrono::DateTime::parse_from_rfc3339(s.trim())
        .map(|_| ())
        .map_err(|e| format!("invalid RFC3339 timestamp '{s}': {e}"))
}

// ── Jobs index ──────────────────────────────────────────────────────

#[derive(Template)]
#[template(path = "analysis_jobs.html")]
pub struct JobsIndexView {
    pub writes_enabled: bool,
    pub csrf_token: String,
    pub active: Vec<JobRowView>,
    pub queued: Vec<JobRowView>,
    pub failed: Vec<JobRowView>,
    pub recent_completed: Vec<JobRowView>,
}

#[derive(Debug, Clone)]
pub struct JobRowView {
    pub job_id: String,
    pub event_id: String,
    pub title: String,
    pub source_family: String,
    pub state: String,
    pub stage: String,
    pub progress: String,
    pub requested_at: String,
    pub elapsed: String,
    pub worker_id: String,
    pub error_code: String,
    pub run_link: Option<String>,
}

pub fn load_jobs_index(
    conn: &Connection,
    writes_enabled: bool,
    scope: &crate::catalog::scope::ProjectScope,
) -> Result<JobsIndexView, String> {
    let all = service::list(conn, &JobFilter::default())?;
    let mut view = JobsIndexView {
        writes_enabled,
        csrf_token: String::new(),
        active: Vec::new(),
        queued: Vec::new(),
        failed: Vec::new(),
        recent_completed: Vec::new(),
    };
    for job in all {
        // Omit jobs whose event is outside the configured project scope.
        if let Ok(Some(event)) = event_for_job_event(conn, &job) {
            if super::view::event_scope_excluded(conn, scope, &event).unwrap_or(false) {
                continue;
            }
        }
        let row = job_row(conn, &job)?;
        match job.state {
            JobState::Queued => view.queued.push(row),
            JobState::Failed => view.failed.push(row),
            JobState::Completed => {
                if view.recent_completed.len() < 20 {
                    view.recent_completed.push(row);
                }
            }
            s if s.is_active() => view.active.push(row),
            _ => {}
        }
    }
    Ok(view)
}

fn job_row(conn: &Connection, job: &AnalysisJob) -> Result<JobRowView, String> {
    let event: Option<(String, String)> = conn
        .query_row(
            "SELECT e.external_id, COALESCE(s.normalized_json, '{}') FROM analysis_plans p
             JOIN manifest_revisions m ON m.id = p.manifest_revision_id
             JOIN catalog_events e ON e.id = m.event_id
             JOIN event_snapshots s ON s.id = m.snapshot_id
             WHERE p.id = ?1",
            rusqlite::params![job.plan_revision_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .map_err(|e| format!("cannot load job event: {e}"))?;
    let (event_id, snapshot_json) = event.unwrap_or_else(|| ("?".to_string(), "{}".to_string()));
    let title = serde_json::from_str::<serde_json::Value>(&snapshot_json)
        .ok()
        .and_then(|v| {
            v.get("title")
                .and_then(|t| t.as_str())
                .map(|s| s.to_string())
        })
        .unwrap_or_default();
    let family = source_family_of(conn, job.plan_revision_id).unwrap_or_default();
    let progress = progress_text(job);
    let run_link = job
        .completed_run_id
        .map(|run_id| format!("/analyses/{run_id}"));
    Ok(JobRowView {
        job_id: job.id.clone(),
        event_id,
        title,
        source_family: family,
        state: job.state.as_str().to_string(),
        stage: job.stage.clone().unwrap_or_default(),
        progress,
        requested_at: job.requested_at.clone(),
        elapsed: elapsed_text(job),
        worker_id: job.worker_id.clone().unwrap_or_default(),
        error_code: job.error_code.clone().unwrap_or_default(),
        run_link,
    })
}

fn source_family_of(conn: &Connection, plan_revision_id: i64) -> Result<String, String> {
    let payload = manifest_payload_for_plan(conn, plan_revision_id)?;
    Ok(serde_json::from_str::<Manifest>(&payload)
        .map(|m| m.source_family)
        .unwrap_or_default())
}

fn progress_text(job: &AnalysisJob) -> String {
    match (job.progress_current, job.progress_total) {
        (Some(c), Some(t)) if t > 0 => format!("{c} / {t}{}", unit_suffix(job)),
        (Some(c), None) => format!("{c}{}", unit_suffix(job)),
        _ => String::new(),
    }
}

fn unit_suffix(job: &AnalysisJob) -> String {
    job.progress_unit
        .as_deref()
        .map(|u| format!(" {u}"))
        .unwrap_or_default()
}

fn elapsed_text(job: &AnalysisJob) -> String {
    let start = match &job.started_at {
        Some(s) => s,
        None => return String::new(),
    };
    let end = job.finished_at.as_deref().unwrap_or(start);
    let s = chrono::DateTime::parse_from_rfc3339(start)
        .map(|t| t.with_timezone(&chrono::Utc))
        .ok();
    let e = chrono::DateTime::parse_from_rfc3339(end)
        .map(|t| t.with_timezone(&chrono::Utc))
        .ok();
    match (s, e) {
        (Some(s), Some(e)) => {
            let secs = (e - s).num_seconds().max(0);
            format!("{secs}s")
        }
        _ => String::new(),
    }
}

// ── Job detail ──────────────────────────────────────────────────────

#[derive(Template)]
#[template(path = "analysis_job.html")]
pub struct JobDetailView {
    pub job: JobDetailRow,
    pub events: Vec<JobEventRow>,
    pub writes_enabled: bool,
    pub csrf_token: String,
    pub already_queued: bool,
    pub cancellable: bool,
    pub retryable: bool,
    pub auto_refresh: bool,
    pub worker: Vec<WorkerRowView>,
}

#[derive(Debug, Clone)]
pub struct JobDetailRow {
    pub job_id: String,
    pub event_id: String,
    pub title: String,
    pub plan_revision_id: i64,
    pub plan_hash: String,
    pub requested_by: String,
    pub requested_at: String,
    pub attempt: i64,
    pub original_job_id: Option<String>,
    pub state: String,
    pub stage: String,
    pub progress: String,
    pub elapsed: String,
    pub worker_id: String,
    pub error_code: String,
    pub error_summary: String,
    pub completed_run_id: Option<i64>,
    pub source_access: String,
}

#[derive(Debug, Clone)]
pub struct JobEventRow {
    pub sequence: i64,
    pub occurred_at: String,
    pub state: String,
    pub message: String,
    pub progress: String,
}

#[derive(Debug, Clone)]
pub struct WorkerRowView {
    pub worker_id: String,
    pub last_heartbeat: String,
    pub freshness: String,
    pub process_version: String,
    pub download_jobs: i64,
    pub parse_jobs: i64,
    pub offline: bool,
}

pub fn load_job_detail(
    conn: &Connection,
    job_id: &str,
    writes_enabled: bool,
) -> Result<Option<JobDetailView>, String> {
    let job = match service::get(conn, job_id) {
        Ok(j) => j,
        Err(e) if e.contains("not found") => return Ok(None),
        Err(e) => return Err(e),
    };
    let (event_id, title) = event_for_job(conn, &job)?;
    let row = JobDetailRow {
        job_id: job.id.clone(),
        event_id,
        title,
        plan_revision_id: job.plan_revision_id,
        plan_hash: job.plan_hash.clone(),
        requested_by: RequestSource::parse_source(&job.requested_by)
            .as_str()
            .to_string(),
        requested_at: job.requested_at.clone(),
        attempt: job.attempt,
        original_job_id: job.original_job_id.clone(),
        state: job.state.as_str().to_string(),
        stage: job.stage.clone().unwrap_or_default(),
        progress: progress_text(&job),
        elapsed: elapsed_text(&job),
        worker_id: job.worker_id.clone().unwrap_or_default(),
        error_code: job.error_code.clone().unwrap_or_default(),
        error_summary: job.error_summary.clone().unwrap_or_default(),
        completed_run_id: job.completed_run_id,
        source_access: source_access_label(&job),
    };
    let evs = service::events(conn, job_id, 50)?;
    let events = evs
        .iter()
        .map(|e| JobEventRow {
            sequence: e.sequence,
            occurred_at: e.occurred_at.clone(),
            state: e.state.as_str().to_string(),
            message: e.human_message.clone(),
            progress: match (e.progress_current, e.progress_total) {
                (Some(c), Some(t)) if t > 0 => format!("{c}/{t}"),
                _ => String::new(),
            },
        })
        .collect();
    let workers: Vec<WorkerRowView> = service::list_workers(conn, 60)?
        .iter()
        .map(|(hb, f)| WorkerRowView {
            worker_id: hb.worker_id.clone(),
            last_heartbeat: hb.last_heartbeat.clone(),
            freshness: match f {
                WorkerFreshness::Online => "online".to_string(),
                WorkerFreshness::Stale => "stale".to_string(),
            },
            process_version: hb.process_version.clone(),
            download_jobs: hb.download_jobs,
            parse_jobs: hb.parse_jobs,
            offline: hb.offline_mode,
        })
        .collect();
    let cancellable = job.state.is_cancellable();
    let retryable = job.state.is_retryable();
    let auto_refresh = job.state.is_active();
    Ok(Some(JobDetailView {
        job: row,
        events,
        writes_enabled,
        csrf_token: String::new(),
        already_queued: false,
        cancellable,
        retryable,
        auto_refresh,
        worker: workers,
    }))
}

/// Resolve the catalog event for a job (used by project-scope checks).
fn event_for_job_event(
    conn: &Connection,
    job: &AnalysisJob,
) -> Result<Option<crate::catalog::domain::CatalogEvent>, String> {
    conn.query_row(
        "SELECT e.id, e.source_kind, e.external_id, e.first_seen, e.last_seen
         FROM analysis_plans p
         JOIN manifest_revisions m ON m.id = p.manifest_revision_id
         JOIN catalog_events e ON e.id = m.event_id
         WHERE p.id = ?1",
        rusqlite::params![job.plan_revision_id],
        |r| {
            Ok(crate::catalog::domain::CatalogEvent {
                id: r.get(0)?,
                source_kind: r.get(1)?,
                external_id: r.get(2)?,
                first_seen: r.get(3)?,
                last_seen: r.get(4)?,
            })
        },
    )
    .map(Some)
    .or_else(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => Ok(None),
        other => Err(format!("cannot load event for job: {other}")),
    })
}

fn event_for_job(conn: &Connection, job: &AnalysisJob) -> Result<(String, String), String> {
    let row: Option<(String, String)> = conn
        .query_row(
            "SELECT e.external_id, COALESCE(s.normalized_json, '{}') FROM analysis_plans p
             JOIN manifest_revisions m ON m.id = p.manifest_revision_id
             JOIN catalog_events e ON e.id = m.event_id
             JOIN event_snapshots s ON s.id = m.snapshot_id
             WHERE p.id = ?1",
            rusqlite::params![job.plan_revision_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .map_err(|e| format!("cannot load event for job: {e}"))?;
    let (event_id, snapshot_json) = row.unwrap_or_else(|| ("?".to_string(), "{}".to_string()));
    let title = serde_json::from_str::<serde_json::Value>(&snapshot_json)
        .ok()
        .and_then(|v| {
            v.get("title")
                .and_then(|t| t.as_str())
                .map(|s| s.to_string())
        })
        .unwrap_or_default();
    Ok((event_id, title))
}

fn source_access_label(job: &AnalysisJob) -> String {
    match job.state {
        JobState::Queued => "none until worker claims".to_string(),
        JobState::Completed => "per published archive manifest".to_string(),
        JobState::Failed => job
            .error_code
            .as_deref()
            .map(|c| format!("failed ({c})"))
            .unwrap_or_else(|| "failed".to_string()),
        _ => "disclosed by plan; worker performs acquisition".to_string(),
    }
}

// ── API views ───────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct ApiJobSummary {
    pub job_id: String,
    pub state: String,
    pub stage: Option<String>,
    pub progress_current: Option<i64>,
    pub progress_total: Option<i64>,
    pub progress_unit: Option<String>,
    pub plan_revision_id: i64,
    pub plan_hash: String,
    pub requested_by: String,
    pub requested_at: String,
    pub attempt: i64,
    pub original_job_id: Option<String>,
    pub worker_id: Option<String>,
    pub error_code: Option<String>,
    pub completed_run_id: Option<i64>,
}

fn api_job_summary(job: &AnalysisJob) -> ApiJobSummary {
    ApiJobSummary {
        job_id: job.id.clone(),
        state: job.state.as_str().to_string(),
        stage: job.stage.clone(),
        progress_current: job.progress_current,
        progress_total: job.progress_total,
        progress_unit: job.progress_unit.clone(),
        plan_revision_id: job.plan_revision_id,
        plan_hash: job.plan_hash.clone(),
        requested_by: job.requested_by.clone(),
        requested_at: job.requested_at.clone(),
        attempt: job.attempt,
        original_job_id: job.original_job_id.clone(),
        worker_id: job.worker_id.clone(),
        error_code: job.error_code.clone(),
        completed_run_id: job.completed_run_id,
    }
}

pub fn api_jobs_index(conn: &Connection) -> Result<serde_json::Value, String> {
    let jobs = service::list(conn, &JobFilter::default())?;
    let summaries: Vec<ApiJobSummary> = jobs.iter().map(api_job_summary).collect();
    Ok(serde_json::json!({
        "api_version": 1,
        "jobs": summaries,
    }))
}

pub fn api_job_detail(
    conn: &Connection,
    job_id: &str,
) -> Result<Option<serde_json::Value>, String> {
    let job = match service::get(conn, job_id) {
        Ok(j) => j,
        Err(e) if e.contains("not found") => return Ok(None),
        Err(e) => return Err(e),
    };
    let events: Vec<serde_json::Value> = service::events(conn, job_id, 50)?
        .iter()
        .map(|e| {
            serde_json::json!({
                "sequence": e.sequence,
                "occurred_at": e.occurred_at,
                "state": e.state.as_str(),
                "stage": e.stage,
                "message_code": e.message_code,
                "message": e.human_message,
                "progress_current": e.progress_current,
                "progress_total": e.progress_total,
                "progress_unit": e.progress_unit,
            })
        })
        .collect();
    Ok(Some(serde_json::json!({
        "api_version": 1,
        "job": api_job_summary(&job),
        "events": events,
    })))
}

pub fn api_plan_detail(
    conn: &Connection,
    plan_revision_id: i64,
) -> Result<Option<serde_json::Value>, String> {
    let plan = crate::catalog::db::get_plan(conn, plan_revision_id)
        .map_err(|e| format!("cannot load plan: {e}"))?;
    let Some(plan) = plan else {
        return Ok(None);
    };
    Ok(Some(serde_json::json!({
        "api_version": 1,
        "plan_revision_id": plan.id,
        "status": plan.status,
        "block_reason": plan.block_reason,
        "plan_schema": plan.plan_schema,
        "sha256": plan.sha256,
        "manifest_revision_id": plan.manifest_revision_id,
    })))
}
