//! Catalog view layer — the shared presentation model.
//!
//! Web pages, the JSON API, and the text reports all draw from the same
//! report model (report.json schema v2 fields plus domain label
//! functions), so the CLI report and web UI cannot disagree about the
//! analyst-facing result, assessment, or scope.

use serde::Serialize;

use crate::catalog::db;
use crate::catalog::domain::*;
use crate::catalog::status::{self, CatalogStatus};
use crate::catalog::web::handlers::EventListFilters;
use crate::catalog::web::SharedState;

// ── CSS ─────────────────────────────────────────────────────────────

pub const APP_CSS: &str = r#"
:root { --ink: #1a1a1a; --muted: #555; --line: #ddd; --bg: #fafafa; }
* { box-sizing: border-box; }
body { font-family: ui-sans-serif, system-ui, sans-serif; margin: 0; color: var(--ink); background: var(--bg); }
header { padding: 0.75rem 1rem; border-bottom: 1px solid var(--line); background: #fff; }
header h1 { font-size: 1.1rem; margin: 0; }
nav a { margin-right: 1rem; }
main { padding: 1rem; max-width: 1100px; }
table { border-collapse: collapse; width: 100%; background: #fff; }
th, td { text-align: left; padding: 0.4rem 0.6rem; border-bottom: 1px solid var(--line); vertical-align: top; }
th { font-size: 0.8rem; text-transform: uppercase; letter-spacing: 0.04em; color: var(--muted); }
.tag { display: inline-block; padding: 0.1rem 0.45rem; border-radius: 3px; font-size: 0.78rem; border: 1px solid var(--line); }
.tag.stale { background: #fff3cd; border-color: #e0c060; }
.tag.blocked { background: #fdecea; border-color: #e0a0a0; }
.tag.complete { background: #e7f4e7; border-color: #a0cfa0; }
.muted { color: var(--muted); }
.panel { background: #fff; border: 1px solid var(--line); border-radius: 4px; padding: 0.75rem 1rem; margin-bottom: 1rem; }
.panel h2 { font-size: 1rem; margin: 0 0 0.5rem; }
code { font-size: 0.85em; }
.error { color: #a00; }
form.filter { margin-bottom: 0.75rem; }
form.filter input, form.filter select { margin-right: 0.5rem; }
ul.flat { margin: 0.25rem 0; padding-left: 1.2rem; }
"#;

// ── Templates ───────────────────────────────────────────────────────

use askama::Template;

#[derive(Template)]
#[template(path = "dashboard.html")]
pub struct DashboardView {
    pub total_events: usize,
    pub open_events: usize,
    pub needs_review: usize,
    pub ready: usize,
    pub blocked: usize,
    pub complete: usize,
    pub stale: usize,
    pub failed_runs: usize,
    pub latest_sync: String,
    pub latest_completed: String,
    pub by_status: Vec<(String, usize)>,
}

#[derive(Template)]
#[template(path = "event_list.html")]
pub struct EventListView {
    pub rows: Vec<EventRowView>,
    pub filters: EventListFilters,
}

#[derive(Serialize)]
pub struct EventRowView {
    pub external_id: String,
    pub title: String,
    pub source: String,
    pub lifecycle: String,
    pub start: String,
    pub end: String,
    pub status: String,
    pub expectation: String,
    pub result: String,
    pub last_seen: String,
    pub stale: bool,
}

#[derive(Template)]
#[template(path = "event_detail.html")]
pub struct EventDetailView {
    pub event: CatalogEvent,
    pub status: String,
    pub lifecycle: String,
    pub title: String,
    pub start: String,
    pub end: String,
    pub expectation: String,
    pub latest_result: String,
    pub assessment: String,
    pub stale_reason: String,
    pub snapshots: Vec<SnapshotView>,
    pub manifests: Vec<ManifestView>,
    pub runs: Vec<RunRowView>,
}

pub struct SnapshotView {
    pub id: i64,
    pub fetched_at: String,
    pub source_url: String,
    pub sha256: String,
    pub raw_preview: String,
}

pub struct ManifestView {
    pub id: i64,
    pub sha256: String,
    pub review_status: String,
    pub schema: u32,
    pub snapshot_id: i64,
    pub reviewed_at: String,
}

#[derive(Template)]
#[template(path = "analysis.html")]
pub struct RunView {
    pub run_id: i64,
    pub event_id: String,
    pub title: String,
    pub result_label: String,
    pub finding: String,
    pub assessment: String,
    pub scope: serde_json::Value,
    pub lifecycle: serde_json::Value,
    pub waves: serde_json::Value,
    pub artifacts: Vec<ArtifactView>,
    pub missing_artifacts: Vec<String>,
    pub status: String,
    pub started_at: String,
}

pub struct ArtifactView {
    pub kind: String,
    pub relative_path: String,
    pub sha256: String,
    pub size: i64,
}

#[derive(Template)]
#[template(path = "streams.html")]
pub struct StreamsView {
    pub run_id: i64,
    pub streams: Vec<StreamRowView>,
    pub category_filter: Option<String>,
}

#[derive(Serialize)]
pub struct StreamRowView {
    pub collector: String,
    pub peer_ip: String,
    pub prefix: String,
    pub category: String,
    pub baseline_instances: i64,
    pub max_active: i64,
    pub transitions: i64,
    pub withdrawn: bool,
    pub restored: bool,
    pub transit_state: String,
    pub ambiguous: bool,
}

// ── Loaders ─────────────────────────────────────────────────────────

fn lifecycle_of(snapshot: &EventSnapshot) -> String {
    serde_json::from_str::<serde_json::Value>(&snapshot.normalized_json)
        .ok()
        .map(|v| match v.get("end") {
            None => "Open".to_string(),
            Some(end) => {
                if end.is_null() || end.as_str().map(|s| s.is_empty()).unwrap_or(true) {
                    "Open".to_string()
                } else {
                    "Closed".to_string()
                }
            }
        })
        .unwrap_or_else(|| "Closed".to_string())
}

pub fn load_dashboard(conn: &rusqlite::Connection) -> Result<DashboardView, String> {
    let statuses = status::derive_all_statuses(conn)?;
    let total = statuses.len();
    let open = statuses
        .iter()
        .filter(|(e, _)| {
            db::list_snapshots(conn, e.id)
                .ok()
                .and_then(|s| s.first().map(lifecycle_of))
                .map(|l| l == "Open")
                .unwrap_or(false)
        })
        .count();
    let count = |s: CatalogStatus| statuses.iter().filter(|(_, st)| *st == s).count();
    let failed_runs = conn
        .query_row(
            "SELECT COUNT(*) FROM analysis_runs WHERE status IN ('Failed','Incomplete')",
            [],
            |r| r.get::<_, i64>(0),
        )
        .map(|c| c as usize)
        .unwrap_or(0);
    let latest_sync = db::latest_sync(conn, "grnoc-public-task-viewer")?
        .map(|s| format!("{} ({})", s.started_at, s.status))
        .unwrap_or_default();
    let latest_completed = conn
        .query_row(
            "SELECT MAX(started_at) FROM analysis_runs WHERE status = 'Complete'",
            [],
            |r| r.get::<_, Option<String>>(0),
        )
        .map_err(|e| format!("catalog query failed: {e}"))?
        .unwrap_or_default();
    let by_status = vec![
        ("Discovered".to_string(), count(CatalogStatus::Discovered)),
        ("NeedsReview".to_string(), count(CatalogStatus::NeedsReview)),
        ("Ready".to_string(), count(CatalogStatus::Ready)),
        ("Blocked".to_string(), count(CatalogStatus::Blocked)),
        ("Running".to_string(), count(CatalogStatus::Running)),
        ("Complete".to_string(), count(CatalogStatus::Complete)),
        ("Failed".to_string(), count(CatalogStatus::Failed)),
        ("Stale".to_string(), count(CatalogStatus::Stale)),
    ];
    Ok(DashboardView {
        total_events: total,
        open_events: open,
        needs_review: count(CatalogStatus::NeedsReview),
        ready: count(CatalogStatus::Ready),
        blocked: count(CatalogStatus::Blocked),
        complete: count(CatalogStatus::Complete),
        stale: count(CatalogStatus::Stale),
        failed_runs,
        latest_sync,
        latest_completed,
        by_status,
    })
}

pub fn load_event_list(
    conn: &rusqlite::Connection,
    filters: &EventListFilters,
) -> Result<EventListView, String> {
    let mut rows = Vec::new();
    for (event, st) in status::derive_all_statuses(conn)? {
        let snapshots = db::list_snapshots(conn, event.id)?;
        let latest = snapshots.first();
        let lifecycle = latest
            .map(lifecycle_of)
            .unwrap_or_else(|| "Closed".to_string());
        let normalized: serde_json::Value = latest
            .and_then(|s| serde_json::from_str(&s.normalized_json).ok())
            .unwrap_or_else(|| serde_json::json!({}));
        let title = normalized
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let start = normalized
            .get("start")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let end = normalized
            .get("end")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let expectation = latest_expectation(conn, event.id)?;
        let (result, _assessment) = latest_result(conn, event.id)?;

        if let Some(f) = &filters.lifecycle {
            if lifecycle != *f {
                continue;
            }
        }
        if let Some(f) = &filters.status {
            if st.as_str() != f {
                continue;
            }
        }
        if let Some(f) = &filters.expectation {
            if expectation.as_deref() != Some(f.as_str()) {
                continue;
            }
        }
        if let Some(f) = &filters.source {
            if event.source_kind != *f {
                continue;
            }
        }
        if let Some(f) = &filters.q {
            let hay = format!("{} {title}", event.external_id).to_lowercase();
            if !hay.contains(&f.to_lowercase()) {
                continue;
            }
        }
        let stale = st == CatalogStatus::Stale;
        rows.push(EventRowView {
            external_id: event.external_id.clone(),
            title,
            source: event.source_kind.clone(),
            lifecycle,
            start,
            end: end.unwrap_or_else(|| "Open".to_string()),
            status: st.as_str().to_string(),
            expectation: expectation.unwrap_or_default(),
            result: result.unwrap_or_default(),
            last_seen: event.last_seen.clone(),
            stale,
        });
    }
    rows.sort_by(|a, b| {
        b.start
            .cmp(&a.start)
            .then_with(|| a.external_id.cmp(&b.external_id))
    });
    Ok(EventListView {
        rows,
        filters: EventListFilters {
            lifecycle: filters.lifecycle.clone(),
            status: filters.status.clone(),
            expectation: filters.expectation.clone(),
            source: filters.source.clone(),
            date_from: filters.date_from.clone(),
            date_to: filters.date_to.clone(),
            q: filters.q.clone(),
        },
    })
}

fn latest_expectation(
    conn: &rusqlite::Connection,
    event_id: i64,
) -> Result<Option<String>, String> {
    let manifests = db::list_manifest_revisions(conn, event_id)?;
    let Some(manifest) = manifests.first() else {
        return Ok(None);
    };
    let value: serde_json::Value = serde_json::from_str(&manifest.payload).unwrap_or_default();
    Ok(value
        .get("target")
        .and_then(|t| t.get("label"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string()))
}

fn latest_result(
    conn: &rusqlite::Connection,
    event_id: i64,
) -> Result<(Option<String>, Option<String>), String> {
    let runs = db::list_runs_for_event(conn, event_id)?;
    let latest = runs.into_iter().find(|r| r.status == "Complete");
    Ok((
        latest.as_ref().and_then(|r| r.verdict.clone()),
        latest.as_ref().and_then(|r| r.assessment.clone()),
    ))
}

pub fn load_event_detail(
    conn: &rusqlite::Connection,
    external_id: &str,
) -> Result<Option<EventDetailView>, String> {
    let Some(event) = db::get_event_by_external(conn, "local-repository", external_id)?.or(
        db::get_event_by_external(conn, "grnoc-public-task-viewer", external_id)?,
    ) else {
        return Ok(None);
    };
    let st = status::derive_status(conn, event.id)?;
    let snapshots = db::list_snapshots(conn, event.id)?;
    let latest = snapshots.first();
    let normalized: serde_json::Value = latest
        .and_then(|s| serde_json::from_str(&s.normalized_json).ok())
        .unwrap_or_else(|| serde_json::json!({}));
    let title = normalized
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let start = normalized
        .get("start")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let end = normalized
        .get("end")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let lifecycle = if end.as_deref().map(|s| s.is_empty()).unwrap_or(true) {
        "Open"
    } else {
        "Closed"
    }
    .to_string();
    let expectation = latest_expectation(conn, event.id)?;
    let (result, assessment) = latest_result(conn, event.id)?;
    let stale_reason = if st == CatalogStatus::Stale {
        "The latest source snapshot or reviewed manifest changed after the latest completed analysis. The current event state has not yet been analyzed under the latest inputs; the historical run remains valid.".to_string()
    } else {
        String::new()
    };
    let snapshot_views = snapshots
        .iter()
        .map(|s| SnapshotView {
            id: s.id,
            fetched_at: s.fetched_at.clone(),
            source_url: s.source_url.clone(),
            sha256: s.content_sha256.clone(),
            raw_preview: s.raw_payload.chars().take(220).collect(),
        })
        .collect();
    let manifest_views = db::list_manifest_revisions(conn, event.id)?
        .iter()
        .map(|m| ManifestView {
            id: m.id,
            sha256: m.sha256.clone(),
            review_status: m.review_status.clone(),
            schema: m.manifest_schema,
            snapshot_id: m.snapshot_id,
            reviewed_at: m.reviewed_at.clone().unwrap_or_default(),
        })
        .collect();
    let run_rows = db::list_runs_for_event(conn, event.id)?
        .iter()
        .map(|r| RunRowView {
            id: r.id,
            status: r.status.clone(),
            started_at: r.started_at.clone(),
            verdict: r.verdict.clone().unwrap_or_default(),
            assessment: r.assessment.clone().unwrap_or_default(),
        })
        .collect();

    Ok(Some(EventDetailView {
        event,
        status: st.as_str().to_string(),
        lifecycle,
        title,
        start,
        end: end.unwrap_or_else(|| "Open".to_string()),
        expectation: expectation.unwrap_or_default(),
        latest_result: result.unwrap_or_default(),
        assessment: assessment.unwrap_or_default(),
        stale_reason,
        snapshots: snapshot_views,
        manifests: manifest_views,
        runs: run_rows,
    }))
}

#[derive(Serialize)]
pub struct RunRowView {
    pub id: i64,
    pub status: String,
    pub started_at: String,
    pub verdict: String,
    pub assessment: String,
}

/// Load the analyst-facing run view from the stored report model.
pub fn load_run(
    conn: &rusqlite::Connection,
    run_id: i64,
    state: &SharedState,
) -> Result<Option<RunView>, String> {
    let Some(run) = db::get_run(conn, run_id)? else {
        return Ok(None);
    };
    let plan = db::get_plan(conn, run.plan_id)?;
    let Some(plan) = plan else {
        return Ok(None);
    };
    let manifest = db::get_manifest_revision(conn, plan.manifest_revision_id)?;
    let Some(manifest) = manifest else {
        return Ok(None);
    };
    let event = db::get_event(conn, manifest.event_id)?;
    let Some(event) = event else {
        return Ok(None);
    };

    // Report model: read the run's report.json artifact (relative path).
    let artifacts = db::list_artifacts(conn, run_id)?;
    let mut missing_artifacts = Vec::new();
    let mut report_value = serde_json::Value::Null;
    let mut report_available = false;
    for a in &artifacts {
        if a.kind == "report" && a.relative_path.ends_with("report.json") {
            let full = state.catalog_root.join(&a.relative_path);
            match std::fs::read_to_string(&full) {
                Ok(content) => {
                    report_value = serde_json::from_str(&content).unwrap_or_default();
                    report_available = true;
                }
                Err(_) => missing_artifacts.push(a.relative_path.clone()),
            }
        }
    }

    let result_label = if report_available {
        report_value
            .get("result")
            .and_then(|r| r.get("verdict_label"))
            .and_then(|v| v.as_str())
            .unwrap_or(run.verdict.as_deref().unwrap_or("Unknown"))
            .to_string()
    } else {
        run.verdict.clone().unwrap_or_else(|| "Unknown".to_string())
    };
    let finding = if report_available {
        report_value
            .get("result")
            .and_then(|r| r.get("finding"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string()
    } else {
        String::new()
    };
    let assessment = if report_available {
        report_value
            .get("assessment")
            .and_then(|a| a.get("statement"))
            .and_then(|v| v.as_str())
            .unwrap_or(run.assessment.as_deref().unwrap_or(""))
            .to_string()
    } else {
        run.assessment.clone().unwrap_or_default()
    };
    let scope = report_value
        .get("observed_event_signature")
        .and_then(|s| s.get("observer_scope"))
        .cloned()
        .unwrap_or_default();
    let lifecycle = report_value
        .get("observed_event_signature")
        .and_then(|s| s.get("stream_lifecycle"))
        .cloned()
        .unwrap_or_default();
    let waves = report_value
        .get("observed_event_signature")
        .and_then(|s| s.get("semantic_waves"))
        .cloned()
        .unwrap_or_default();

    let title = serde_json::from_str::<serde_json::Value>(&manifest.payload)
        .ok()
        .and_then(|m| {
            m.get("target")
                .and_then(|t| t.get("label"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        })
        .unwrap_or_else(|| event.external_id.clone());

    Ok(Some(RunView {
        run_id: run.id,
        event_id: event.external_id.clone(),
        title,
        result_label,
        finding,
        assessment,
        scope,
        lifecycle,
        waves,
        artifacts: artifacts
            .iter()
            .map(|a| ArtifactView {
                kind: a.kind.clone(),
                relative_path: a.relative_path.clone(),
                sha256: a.sha256.clone(),
                size: a.size,
            })
            .collect(),
        missing_artifacts,
        status: run.status.clone(),
        started_at: run.started_at.clone(),
    }))
}

pub fn load_run_streams(
    conn: &rusqlite::Connection,
    run_id: i64,
    filters: &crate::catalog::web::handlers::StreamFilters,
) -> Result<Option<StreamsView>, String> {
    if db::get_run(conn, run_id)?.is_none() {
        return Ok(None);
    }
    let streams = db::list_streams(
        conn,
        run_id,
        filters.category.as_deref(),
        filters.collector.as_deref(),
    )?
    .into_iter()
    .filter(|s| {
        if let Some(w) = &filters.withdrawn {
            if (w == "1") != s.withdrawn {
                return false;
            }
        }
        if let Some(t) = &filters.transit_departed {
            let departed = s.transit_state == "DepartedTransitPath";
            if (t == "1") != departed {
                return false;
            }
        }
        if let Some(r) = &filters.restored {
            if (r == "1") != s.restored {
                return false;
            }
        }
        if let Some(a) = &filters.ambiguous {
            if (a == "1") != s.add_path_ambiguous {
                return false;
            }
        }
        true
    })
    .map(|s| StreamRowView {
        collector: s.collector,
        peer_ip: s.peer_ip,
        prefix: s.prefix,
        category: s.category,
        baseline_instances: s.baseline_instances,
        max_active: s.max_active_instances,
        transitions: s.transition_count,
        withdrawn: s.withdrawn,
        restored: s.restored,
        transit_state: s.transit_state,
        ambiguous: s.add_path_ambiguous,
    })
    .collect();
    Ok(Some(StreamsView {
        run_id,
        streams,
        category_filter: filters.category.clone(),
    }))
}

// ── JSON loaders ────────────────────────────────────────────────────

pub fn load_event_list_json(
    conn: &rusqlite::Connection,
    page: usize,
    per_page: usize,
) -> Result<serde_json::Value, String> {
    let mut all = Vec::new();
    for (event, st) in status::derive_all_statuses(conn)? {
        let (result, assessment) = latest_result(conn, event.id)?;
        all.push(serde_json::json!({
            "event_id": event.external_id,
            "source": event.source_kind,
            "status": st.as_str(),
            "last_seen": event.last_seen,
            "latest_result": result,
            "assessment": assessment,
        }));
    }
    all.sort_by(|a, b| {
        b["last_seen"]
            .as_str()
            .cmp(&a["last_seen"].as_str())
            .then_with(|| a["event_id"].as_str().cmp(&b["event_id"].as_str()))
    });
    let total = all.len();
    let start = page.saturating_mul(per_page);
    let items: Vec<_> = all.into_iter().skip(start).take(per_page).collect();
    Ok(serde_json::json!({
        "schema_version": 1,
        "total": total,
        "page": page,
        "per_page": per_page,
        "events": items,
    }))
}

pub fn load_event_detail_json(
    conn: &rusqlite::Connection,
    external_id: &str,
) -> Result<Option<serde_json::Value>, String> {
    let Some(view) = load_event_detail(conn, external_id)? else {
        return Ok(None);
    };
    Ok(Some(serde_json::json!({
        "schema_version": 1,
        "event": {
            "id": view.event.external_id,
            "source": view.event.source_kind,
            "status": view.status,
            "lifecycle": view.lifecycle,
            "title": view.title,
            "start": view.start,
            "end": view.end,
            "expectation": if view.expectation.is_empty() { serde_json::Value::Null } else { serde_json::Value::String(view.expectation.clone()) },
            "latest_result": if view.latest_result.is_empty() { serde_json::Value::Null } else { serde_json::Value::String(view.latest_result.clone()) },
            "assessment": if view.assessment.is_empty() { serde_json::Value::Null } else { serde_json::Value::String(view.assessment.clone()) },
            "snapshot_count": view.snapshots.len(),
            "manifest_count": view.manifests.len(),
            "run_count": view.runs.len(),
        },
        "snapshots": view.snapshots.iter().map(|s| serde_json::json!({
            "id": s.id, "fetched_at": s.fetched_at, "source_url": s.source_url, "sha256": s.sha256,
        })).collect::<Vec<_>>(),
        "runs": view.runs.iter().map(|r| serde_json::json!({
            "id": r.id, "status": r.status, "started_at": r.started_at,
            "verdict": if r.verdict.is_empty() { serde_json::Value::Null } else { serde_json::Value::String(r.verdict.clone()) },
            "assessment": if r.assessment.is_empty() { serde_json::Value::Null } else { serde_json::Value::String(r.assessment.clone()) },
        })).collect::<Vec<_>>(),
    })))
}

pub fn load_run_json(
    conn: &rusqlite::Connection,
    run_id: i64,
) -> Result<Option<serde_json::Value>, String> {
    let Some(run) = db::get_run(conn, run_id)? else {
        return Ok(None);
    };
    let artifacts = db::list_artifacts(conn, run_id)?
        .iter()
        .map(|a| {
            serde_json::json!({
                "kind": a.kind,
                "relative_path": a.relative_path,
                "media_type": a.media_type,
                "sha256": a.sha256,
                "size": a.size,
            })
        })
        .collect::<Vec<_>>();
    Ok(Some(serde_json::json!({
        "schema_version": 1,
        "run": {
            "id": run.id,
            "plan_id": run.plan_id,
            "software_version": run.software_version,
            "status": run.status,
            "started_at": run.started_at,
            "verdict": run.verdict,
            "assessment": run.assessment,
        },
        "artifacts": artifacts,
    })))
}

pub fn load_streams_json(
    conn: &rusqlite::Connection,
    run_id: i64,
    page: usize,
    per_page: usize,
    category: Option<&str>,
    collector: Option<&str>,
) -> Result<Option<serde_json::Value>, String> {
    if db::get_run(conn, run_id)?.is_none() {
        return Ok(None);
    }
    let all = db::list_streams(conn, run_id, category, collector)?;
    let total = all.len();
    let start = page.saturating_mul(per_page);
    let items = all
        .into_iter()
        .skip(start)
        .take(per_page)
        .map(|s| {
            serde_json::json!({
                "collector": s.collector,
                "peer_ip": s.peer_ip,
                "prefix": s.prefix,
                "category": s.category,
                "baseline_instances": s.baseline_instances,
                "max_active_instances": s.max_active_instances,
                "transition_count": s.transition_count,
                "withdrawn": s.withdrawn,
                "restored": s.restored,
                "transit_state": s.transit_state,
                "add_path_ambiguous": s.add_path_ambiguous,
            })
        })
        .collect::<Vec<_>>();
    Ok(Some(serde_json::json!({
        "schema_version": 1,
        "run_id": run_id,
        "total": total,
        "page": page,
        "per_page": per_page,
        "streams": items,
    })))
}

pub fn load_catalog_status_json(conn: &rusqlite::Connection) -> Result<serde_json::Value, String> {
    let dashboard = load_dashboard(conn)?;
    Ok(serde_json::json!({
        "schema_version": 1,
        "catalog": {
            "total_events": dashboard.total_events,
            "open_events": dashboard.open_events,
            "needs_review": dashboard.needs_review,
            "ready": dashboard.ready,
            "blocked": dashboard.blocked,
            "complete": dashboard.complete,
            "stale": dashboard.stale,
            "failed_runs": dashboard.failed_runs,
        },
        "latest_sync": dashboard.latest_sync,
        "latest_completed_analysis": dashboard.latest_completed,
    }))
}
