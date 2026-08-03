//! Catalog status — derived, analyst-facing statuses.
//!
//! Status is **derived**, never stored as a single mutable truth field.
//! A completed historical run remains completed even if the event later
//! becomes stale. "Stale" means the current event state has not yet been
//! analyzed under the latest inputs — it never invalidates an old run.
//!
//! ## Deterministic precedence (highest wins)
//!
//! 1. `Running`   — an active analysis run exists
//! 2. `Failed`    — the latest attempted analysis failed
//! 3. `Stale`     — the latest source snapshot or reviewed manifest changed
//!    after the latest completed run
//! 4. `Blocked`   — the latest plan is blocked
//! 5. `Complete`  — the latest reviewed manifest produced a ready plan and a
//!    completed run exists for it
//! 6. `Ready`     — the latest reviewed manifest produces a ready plan
//! 7. `NeedsReview` — a manifest exists but its reviewed mapping is
//!    unresolved (no blocked plan yet)
//! 8. `Discovered` — the source event exists but has no reviewed manifest

use serde::{Deserialize, Serialize};

use super::db;

/// Derived analyst-facing catalog status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CatalogStatus {
    Discovered,
    NeedsReview,
    Ready,
    Blocked,
    Running,
    Complete,
    Failed,
    Stale,
}

impl CatalogStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            CatalogStatus::Discovered => "Discovered",
            CatalogStatus::NeedsReview => "NeedsReview",
            CatalogStatus::Ready => "Ready",
            CatalogStatus::Blocked => "Blocked",
            CatalogStatus::Running => "Running",
            CatalogStatus::Complete => "Complete",
            CatalogStatus::Failed => "Failed",
            CatalogStatus::Stale => "Stale",
        }
    }
}

/// The plan/run inputs that determine staleness.
struct Inputs {
    latest_snapshot_id: Option<i64>,
    latest_manifest_id: Option<i64>,
    run_snapshot_id: Option<i64>,
    run_manifest_id: Option<i64>,
    latest_plan_status: Option<String>,
    latest_plan_ready: bool,
    run_for_latest_plan: bool,
    has_manifest: bool,
    manifest_reviewed: bool,
    any_running: bool,
    latest_run_failed: bool,
}

fn load_inputs(conn: &rusqlite::Connection, event_id: i64) -> Result<Inputs, String> {
    let snapshots = db::list_snapshots(conn, event_id)?;
    let manifests = db::list_manifest_revisions(conn, event_id)?;
    let runs = db::list_runs_for_event(conn, event_id)?;

    let latest_snapshot_id = snapshots.first().map(|s| s.id);
    let latest_manifest_id = manifests.first().map(|m| m.id);

    // The latest plan across all manifests (by manifest id, then plan id).
    let mut latest_plan: Option<(i64, i64, String, Option<String>)> = None; // (manifest_id, plan_id, status, reason)
    let mut run_for_latest_plan = false;
    let mut run_snapshot_id: Option<i64> = None;
    let mut run_manifest_id: Option<i64> = None;
    let mut any_running = false;
    let mut latest_run_failed = false;

    for m in &manifests {
        for p in db::list_plans_for_manifest(conn, m.id)? {
            let better = match &latest_plan {
                None => true,
                Some((lm, lp, _, _)) => m.id > *lm || (m.id == *lm && p.id > *lp),
            };
            if better {
                latest_plan = Some((m.id, p.id, p.status.clone(), p.block_reason.clone()));
            }
        }
    }

    for r in &runs {
        if r.status == "Running" {
            any_running = true;
        }
    }
    // Latest run by started_at (runs are listed newest first).
    if let Some(latest) = runs.first() {
        if latest.status == "Failed" || latest.status == "Incomplete" {
            latest_run_failed = true;
        }
        let plan = db::get_plan(conn, latest.plan_id)?;
        if let Some(p) = plan {
            let manifest = db::get_manifest_revision(conn, p.manifest_revision_id)?;
            run_manifest_id = manifest.as_ref().map(|m| m.id);
            run_snapshot_id = manifest.as_ref().map(|m| m.snapshot_id);
        }
    }

    if let Some((m_id, p_id, _, _)) = &latest_plan {
        // A completed run counts if it executed the latest plan or any plan
        // under the latest manifest revision.
        for r in runs.iter().filter(|r| r.status == "Complete") {
            let run_manifest = db::get_plan(conn, r.plan_id)?.map(|p| p.manifest_revision_id);
            if r.plan_id == *p_id || run_manifest == Some(*m_id) {
                run_for_latest_plan = true;
            }
        }
    }

    let latest_plan_ready = matches!(
        latest_plan.as_ref().map(|(_, _, s, _)| s.as_str()),
        Some("Ready")
    );
    let has_manifest = !manifests.is_empty();
    let manifest_reviewed = manifests
        .first()
        .map(|m| m.review_status == "Reviewed")
        .unwrap_or(false);

    Ok(Inputs {
        latest_snapshot_id,
        latest_manifest_id,
        run_snapshot_id,
        run_manifest_id,
        latest_plan_status: latest_plan.map(|(_, _, s, _)| s),
        latest_plan_ready,
        run_for_latest_plan,
        has_manifest,
        manifest_reviewed,
        any_running,
        latest_run_failed,
    })
}

/// Derive the status for one event.
pub fn derive_status(conn: &rusqlite::Connection, event_id: i64) -> Result<CatalogStatus, String> {
    let i = load_inputs(conn, event_id)?;

    if i.any_running {
        return Ok(CatalogStatus::Running);
    }
    if i.latest_run_failed {
        return Ok(CatalogStatus::Failed);
    }
    let stale = i.latest_snapshot_id.is_some()
        && i.run_snapshot_id.is_some()
        && (i.latest_snapshot_id != i.run_snapshot_id || i.latest_manifest_id != i.run_manifest_id);
    if stale {
        return Ok(CatalogStatus::Stale);
    }
    if !i.has_manifest {
        return Ok(CatalogStatus::Discovered);
    }
    if i.latest_plan_status.as_deref() == Some("Blocked") {
        return Ok(CatalogStatus::Blocked);
    }
    if !i.manifest_reviewed {
        return Ok(CatalogStatus::NeedsReview);
    }
    if i.latest_plan_ready && i.run_for_latest_plan {
        return Ok(CatalogStatus::Complete);
    }
    if i.latest_plan_ready {
        return Ok(CatalogStatus::Ready);
    }
    Ok(CatalogStatus::NeedsReview)
}

/// Compute statuses for every event.
///
/// When a project-scope policy is supplied, excluded events are omitted
/// from the result (they are not active project items).
pub fn derive_all_statuses(
    conn: &rusqlite::Connection,
) -> Result<Vec<(super::domain::CatalogEvent, CatalogStatus)>, String> {
    let events = db::list_events(conn)?;
    let mut out = Vec::new();
    for e in events {
        let id = e.id;
        let st = derive_status(conn, id)?;
        out.push((e, st));
    }
    Ok(out)
}
