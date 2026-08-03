//! Catalog view layer — the shared presentation model.
//!
//! Web pages, the JSON API, and the text reports all draw from the same
//! report model (report.json schema v2 fields plus domain label
//! functions), so the CLI report and web UI cannot disagree about the
//! analyst-facing result, assessment, or scope.

use serde::Serialize;

use crate::catalog::db;
use crate::catalog::domain::*;
use crate::catalog::jobs::JobState;
use crate::catalog::status::{self, CatalogStatus};
use crate::catalog::web::handlers::EventListFilters;
use crate::catalog::web::SharedState;

// ── CSS ─────────────────────────────────────────────────────────────

pub const APP_CSS: &str = r#"
:root { --ink: #1a1a1a; --muted: #555; --line: #ddd; --bg: #fafafa; --link: #1a4f8b; }
* { box-sizing: border-box; }
body { font-family: ui-sans-serif, system-ui, sans-serif; margin: 0; color: var(--ink); background: var(--bg); font-size: 14px; line-height: 1.4; }
header { padding: 0.6rem 1rem; border-bottom: 1px solid var(--line); background: #fff; }
header h1 { font-size: 1.05rem; margin: 0; }
nav { margin-top: 0.25rem; }
nav a { margin-right: 1rem; font-size: 0.85rem; }
nav a.active { font-weight: 700; text-decoration: underline; }
main { padding: 0.75rem 16px 2rem; max-width: 1440px; margin: 0 auto; }
table { border-collapse: collapse; width: 100%; background: #fff; }
th, td { text-align: left; padding: 0.35rem 0.5rem; border-bottom: 1px solid var(--line); vertical-align: top; }
th { font-size: 0.75rem; text-transform: uppercase; letter-spacing: 0.04em; color: var(--muted); }
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

/* ── NOC incident workbench (Sessions 36–37) ────────────────────────
   Old-school operations-console HCI: square corners, thin rules,
   conventional blue underlined links, restrained grey headers, compact
   vertical rhythm, tabular/monospaced technical values, no decorative
   animation, no external fonts, no CDN assets, no SPA. Color is always
   an ADDITIONAL signal — every state carries explicit text. */
.wb-title { font-size: 1.15rem; margin: 0.5rem 0 0.1rem; }
.wb-subtitle { margin: 0 0 0.75rem; font-size: 0.85rem; }
.wb-section { font-size: 1rem; margin: 1.25rem 0 0.4rem; border-bottom: 1px solid #444; padding-bottom: 0.2rem; }
.wb-subsection { font-size: 0.9rem; margin: 0.6rem 0 0.3rem; }
.wb-panel { border: 1px solid var(--line); border-radius: 0; background: #fff; padding: 0.6rem 0.8rem; }
.wb-panel h3 { margin: 0 0 0.3rem; font-size: 0.8rem; text-transform: uppercase; letter-spacing: 0.06em; }
.wb-facts { display: flex; flex-wrap: wrap; gap: 0.1rem 1.2rem; margin: 0 0 0.5rem; }
.wb-facts > div { display: flex; gap: 0.5rem; min-width: 240px; }
.wb-facts dt { color: var(--muted); font-size: 0.75rem; text-transform: uppercase; letter-spacing: 0.04em; min-width: 10rem; }
.wb-facts dd { margin: 0; font-size: 0.85rem; }
.wb-observed { border-top: 1px solid var(--line); padding-top: 0.4rem; }
.wb-result { font-size: 0.95rem; margin: 0.2rem 0; }
.wb-scope { margin: 0.2rem 0; color: #7a4a00; border-left: 3px solid #c8962e; padding-left: 0.5rem; }
.wb-links { margin: 0.4rem 0 0; font-size: 0.85rem; }
.wb-links a { color: var(--link); text-decoration: underline; }
.wb-note { font-size: 0.78rem; }
.wb-table { border-collapse: collapse; width: 100%; background: #fff; font-size: 0.8rem; line-height: 1.3; }
.wb-table th { text-align: left; padding: 0.3rem 0.5rem; border-bottom: 2px solid #444; font-size: 0.68rem; text-transform: uppercase; letter-spacing: 0.04em; color: #333; background: #f0f0f0; white-space: nowrap; position: sticky; top: 0; z-index: 1; }
.wb-table td { padding: 0.28rem 0.5rem; border-bottom: 1px solid var(--line); vertical-align: top; }
.wb-table.wb-narrow { width: auto; min-width: 60%; }
.wb-mono { font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace; font-size: 0.76rem; }
.wb-nowrap { white-space: nowrap; }
.wb-num { text-align: right; font-variant-numeric: tabular-nums; }
.wb-region { font-weight: 600; }
.wb-peer { font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace; font-size: 0.76rem; }
.wb-change { font-weight: 600; }
.wb-denominator { font-size: 0.85rem; margin: 0.2rem 0; }
.wb-filters { font-size: 0.8rem; margin: 0.2rem 0 0.5rem; }
.wb-filters a { margin-right: 0.5rem; color: var(--link); text-decoration: underline; }
.wb-filters a.wb-filter-active { background: #e8eef6; font-weight: 700; }
.wb-sentence { font-size: 0.85rem; margin: 0.3rem 0; }
.wb-episode-row td { background: #fff; }
.wb-episode-row.wb-changed td { background: #fdf6ec; border-left: 3px solid #c8962e; }
.wb-episode-row.wb-changed:hover td { background: #f9edd6; }
.wb-episode-row.wb-unchanged td { background: #f7f7f5; color: #444; }
.wb-episode-row.wb-unchanged:hover td { background: #efefec; }
.wb-episode-row:focus-within td { outline: 2px solid var(--link); outline-offset: -2px; }
.wb-episode-details summary, .wb-prefix-drilldown summary, .wb-collapsed summary, .wb-analysis-history summary { cursor: pointer; font-size: 0.8rem; color: var(--link); text-decoration: underline; }
.wb-episode-details[open] summary { font-weight: 700; }
.wb-episode-expanded { border: 1px solid var(--line); background: #fff; padding: 0.5rem 0.6rem; margin: 0.4rem 0; }
.wb-end { font-size: 0.78rem; }
.wb-end-restored { color: #1d5c1d; }
.wb-end-changed { color: #8a4a00; font-weight: 600; }
.wb-end-unresolved { color: #8a5a00; }
.wb-end-plain { color: var(--muted); }
.wb-status { font-size: 0.75rem; text-transform: uppercase; letter-spacing: 0.03em; }
.wb-unresolved { color: #8a5a00; }
.wb-breadth-changed { background: #fdf3e3; color: #7a4a00; font-weight: 700; }
.wb-breadth-unchanged { background: #eef4ee; color: #3d6b3d; }
.wb-breadth-none { background: repeating-linear-gradient(45deg, #f0f0f0, #f0f0f0 4px, #e6e6e6 4px, #e6e6e6 8px); color: #666; }
.wb-change-tag { padding: 0.05rem 0.35rem; border: 1px solid currentColor; }
.wb-nobaseline-row td { background: repeating-linear-gradient(45deg, #f4f4f2, #f4f4f2 4px, #ececea 4px, #ececea 8px); }
.wb-cue-groups { padding-left: 0; list-style: none; }
.wb-cue-group { margin: 0.35rem 0; padding-left: 0.8rem; border-left: 3px solid #888; }
.wb-cue-title { font-weight: 700; font-size: 0.85rem; display: block; }
.wb-cue-text { font-size: 0.83rem; }
.wb-cue-group a { color: var(--link); text-decoration: underline; font-size: 0.8rem; }
.wb-collapsed, .wb-analysis-history { margin: 0.5rem 0; }
.wb-scroll { overflow-x: auto; -webkit-overflow-scrolling: touch; border: 1px solid var(--line); }
.wb-scroll::-webkit-scrollbar { height: 8px; }
.wb-scroll::-webkit-scrollbar-thumb { background: #bbb; }
.wb-timeline-svg { width: 100%; height: auto; display: block; background: #fff; border: 1px solid var(--line); }
.tl-lane-label { font-size: 12px; fill: #333; }
.tl-lane-line { stroke: #ddd; stroke-width: 1; }
.tl-axis line { stroke: #555; stroke-width: 1; }
.tl-tick { stroke: #999; stroke-width: 1; }
.tl-tick-label { font-size: 10px; fill: #555; }
.tl-absence { fill: #c0392b; opacity: 0.85; }
.tl-path { fill: #c8962e; opacity: 0.85; }
.tl-restore { fill: #1d7a1d; }
.tl-bgp { fill: #1a4f8b; }
.tl-op-marker { fill: none; stroke: #1a4f8b; stroke-width: 1.5; }
.tl-op-label { font-size: 10px; fill: #1a4f8b; }
.tl-changed-end { stroke: #8a4a00; stroke-width: 1.5; }
.tl-legend-item { font-size: 10px; fill: #555; }
a { color: var(--link); text-decoration: underline; }
a:focus-visible, button:focus-visible, summary:focus-visible { outline: 2px solid var(--link); outline-offset: 1px; }
.wb-sortable th { cursor: pointer; user-select: none; }
.wb-sortable th.sorted-asc::after { content: " ▲"; }
.wb-sortable th.sorted-desc::after { content: " ▼"; }

/* ── Routing findings : operator-first cards + table. */
.wb-pilot-range { font-size: 0.9rem; margin: 0.2rem 0; }
.wb-audit { margin: 0.3rem 0; color: #7a4a00; }
.wb-nofinding { font-size: 0.95rem; margin: 0.3rem 0; }
.wb-finding { border: 1px solid var(--line); background: #fff; padding: 0.5rem 0.7rem; margin: 0.5rem 0; }
.wb-finding-head { display: flex; flex-wrap: wrap; gap: 0.3rem 1rem; align-items: baseline; font-size: 0.85rem; }
.wb-finding-head .wb-prefix-link { font-weight: 700; color: var(--link); text-decoration: underline; }
.wb-finding-statement { font-size: 0.88rem; margin: 0.4rem 0 0.3rem; line-height: 1.45; }
.wb-finding-paths { display: flex; flex-wrap: wrap; gap: 0.25rem 0.6rem; align-items: baseline; font-size: 0.82rem; margin: 0.2rem 0; }
.wb-path-label { color: var(--muted); font-size: 0.68rem; text-transform: uppercase; letter-spacing: 0.04em; }
.wb-path { background: #f5f5f3; padding: 0.05rem 0.35rem; }
.wb-finding-outcome { font-size: 0.82rem; margin: 0.2rem 0; }
.wb-copy-actions { margin: 0.4rem 0 0; font-size: 0.8rem; }
.wb-copy { font-size: 0.75rem; padding: 0.15rem 0.5rem; margin-right: 0.4rem; border: 1px solid var(--line); background: #f0f0f0; cursor: pointer; }
.wb-region { margin: 0.4rem 0; }
.wb-region-statements { margin: 0.2rem 0 0.4rem; font-size: 0.85rem; }
.wb-region-ratio { font-size: 0.75rem; }
.wb-finding-row td { background: #fff; }
.wb-finding-row .wb-change { font-weight: 700; }
.wb-nochange-statements { margin: 0.2rem 0 0.4rem; font-size: 0.85rem; }
.wb-episode-detail { margin: 0.5rem 0; }

/* principal stories, named paths, no-visibility. */
.wb-scope-line { font-size: 0.82rem; color: #555; margin: 0 0 0.5rem; }
.wb-principal { border-left: 3px solid #2e6da4; }
.wb-secondary { border-left: 3px solid #bbb; }
.wb-path-explanation { font-size: 0.85rem; margin: 0.3rem 0; color: #222; }
.wb-finding-paths { display: flex; flex-wrap: wrap; gap: 0.4rem 2rem; margin: 0.3rem 0; }
.wb-path-col { min-width: 260px; }
.wb-named-path { list-style: none; margin: 0.2rem 0; padding: 0; font-size: 0.82rem; }
.wb-named-path li { position: relative; padding: 0.1rem 0 0.1rem 1.1rem; white-space: nowrap; }
.wb-named-path li::before { content: "\2192"; position: absolute; left: 0.2rem; color: #777; }
.wb-named-path li:first-child::before { content: "\25CF"; font-size: 0.6rem; top: 0.35rem; left: 0.3rem; }
.wb-seg-mark { display: inline-block; min-width: 3.2rem; color: #555; font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace; font-size: 0.7rem; }
.wb-seg.ins { text-decoration: underline; text-decoration-thickness: 2px; background: #eef6ee; }
.wb-seg.del { text-decoration: line-through; text-decoration-thickness: 1px; color: #666; background: #f6eeee; }
.wb-path-numeric { display: block; margin-top: 0.15rem; color: #444; }
.wb-path-legend { margin: 0.15rem 0 0.3rem; }
.wb-prefix-preview { font-size: 0.82rem; margin: 0.3rem 0; }
.wb-prefix-preview a { color: var(--link); text-decoration: underline; }
.wb-context { margin: 0.6rem 0; }
.wb-context summary { cursor: pointer; color: var(--link); text-decoration: underline; font-size: 0.8rem; }
.wb-novis { border: 1px solid var(--line); background: #fff; padding: 0.6rem 0.8rem; }
.wb-novis-statement { font-size: 0.95rem; font-weight: 700; margin: 0.2rem 0; }

/* compact principal cards + progressive disclosure. */
.wb-finding-meaning { font-size: 0.84rem; margin: 0.2rem 0 0.1rem; line-height: 1.32; }
.wb-finding-final { font-size: 0.8rem; margin: 0.05rem 0 0.1rem; color: #333; line-height: 1.3; }
.wb-finding-links { margin: 0.1rem 0; font-size: 0.78rem; display: flex; flex-wrap: wrap; gap: 0.15rem 1rem; }
.wb-finding-links summary { cursor: pointer; color: var(--link); text-decoration: underline; display: inline; }
.wb-finding-links a { color: var(--link); text-decoration: underline; }
.wb-finding-links details { display: inline; }
.wb-route-sequence, .wb-identity-notes, .wb-evidence { margin: 0.15rem 0; }
.wb-route-sequence summary, .wb-identity-notes summary, .wb-evidence summary { color: var(--link); text-decoration: underline; font-size: 0.8rem; cursor: pointer; }
.wb-chronology { font-size: 0.78rem; }
.wb-identity-notes-list { margin: 0.3rem 0 0.2rem; font-size: 0.8rem; }
.wb-identity-notes-list li { margin: 0.12rem 0; }
.wb-filters-wrap { margin: 0.2rem 0; }
.wb-filters-wrap > summary { display: none; } /* desktop: always open */
.wb-principal { padding: 0.3rem 0.5rem; margin: 0.35rem 0; }
.wb-principal .wb-finding-head { font-size: 0.8rem; }
.wb-principal .wb-prefix-preview { font-size: 0.78rem; margin: 0.15rem 0; }
.wb-novis-relationship { font-size: 0.95rem; margin: 0.2rem 0; }
.wb-novis-eligibility { margin: 0.2rem 0 0.5rem; font-size: 0.85rem; }
.wb-novis-assessment { font-size: 0.88rem; margin: 0.2rem 0; line-height: 1.45; }

/* ── Narrow / mobile (Part 12): stacked header, scrollable tables,
   definition-list episode rows. Text never shrinks below readable size;
   timestamps, ASNs, and prefixes never break. */
@media (max-width: 640px) {
  body { font-size: 13px; }
  main { padding: 0.5rem 8px 1.5rem; }
  .wb-event-context summary { cursor: pointer; color: var(--link); text-decoration: underline; font-size: 0.8rem; }
  .wb-facts { display: block; }
  .wb-facts > div { display: block; }
  .wb-facts dt { margin-top: 0.2rem; }
  .wb-facts dd { font-size: 12px; }
  .wb-table { font-size: 12px; }
  .wb-table th { font-size: 11px; }
  .wb-named-path li { white-space: normal; }
  .wb-seg-mark { display: block; }
  .wb-finding-head { font-size: 13px; }
  .wb-path-numeric { word-break: break-word; }
  /* compact first viewport — title, concise
     scope line, then the first principal story within ~300-350px. */
  .wb-filters-wrap { margin: 0.1rem 0; }
  .wb-filters-wrap > summary { display: inline; cursor: pointer; color: var(--link); text-decoration: underline; font-size: 0.78rem; }
  .wb-filters { white-space: nowrap; overflow-x: auto; font-size: 0.75rem; }
  .wb-finding-meaning { font-size: 0.8rem; }
  .wb-finding-final { font-size: 0.78rem; }
  .wb-title { font-size: 1rem; margin: 0.3rem 0 0.1rem; }
  .wb-subtitle { font-size: 0.72rem; margin: 0 0 0.4rem; }
  .wb-scope-line { font-size: 0.75rem; margin: 0 0 0.35rem; }
  .wb-header { padding: 0.3rem 0.5rem; }
  .wb-header .wb-scope { display: none; } /* the scope line carries it */
  .wb-section { margin: 0.7rem 0 0.3rem; font-size: 0.95rem; }
  .wb-finding { margin: 0.35rem 0; padding: 0.4rem 0.5rem; }
  .wb-finding-statement { font-size: 0.82rem; }
  .wb-path-explanation { font-size: 0.8rem; }
  table.wb-episodes, table.wb-episodes tbody, table.wb-episodes tr, table.wb-episodes td { display: block; width: 100%; }
  table.wb-episodes thead { display: none; }
  table.wb-episodes tr { border: 1px solid var(--line); margin-bottom: 0.5rem; padding: 0.25rem; }
  table.wb-episodes td { border: none; padding: 0.15rem 0.3rem 0.15rem 8.5rem; position: relative; min-height: 1.3em; }
  table.wb-episodes td::before { content: attr(data-label); position: absolute; left: 0.3rem; top: 0.15rem; font-size: 10px; text-transform: uppercase; letter-spacing: 0.03em; color: var(--muted); }
  table.wb-episodes td:last-child { padding-left: 0.3rem; }
  .wb-timeline-svg { min-width: 0; }
  .wb-scroll { border: none; }
  .wb-timeline-scroll .wb-timeline-svg { width: 640px; }
}
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
    pub jobs_queued: i64,
    pub jobs_running: i64,
    pub jobs_failed: i64,
    pub jobs_completed: i64,
    pub workers_online: i64,
    pub workers_stale: i64,
    pub writes_enabled: bool,
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
    /// Workflow status line: Prepare analysis / Analysis planning
    /// blocked / Ready to queue / Analysis running / Open workbench /
    /// Latest execution failed.
    pub workflow_status: String,
    pub workflow_link: String,
    pub workflow_detail: String,
    pub active_job_id: Option<String>,
    pub completed_run_id: Option<i64>,
    /// Reviewed relationship is not directly observable in public BGP;
    /// any BGP run is a scope-mismatched supporting observation.
    pub supporting_only: bool,
    /// Reviewed BGP applicability (ticket_reviews), empty when none.
    pub applicability: String,
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
    let job_counts = crate::catalog::jobs::service::counts(conn).unwrap_or_default();
    let workers = crate::catalog::jobs::service::list_workers(conn, 60).unwrap_or_default();
    let (workers_online, workers_stale) =
        workers
            .iter()
            .fold((0i64, 0i64), |(on, st), (_, f)| match f {
                crate::catalog::jobs::service::WorkerFreshness::Online => (on + 1, st),
                crate::catalog::jobs::service::WorkerFreshness::Stale => (on, st + 1),
            });
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
        jobs_queued: job_counts.queued,
        jobs_running: job_counts.running,
        jobs_failed: job_counts.failed,
        jobs_completed: job_counts.completed,
        workers_online,
        workers_stale,
        writes_enabled: false,
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

/// Present a stored verdict string (machine name or frozen legacy
/// label) through the source-neutral observed-result vocabulary.
/// Returns (observed_result_label, expectation_assessment_label).
fn present_run_verdict(stored: &str) -> (String, String) {
    use crate::domain::assessment::Verdict;
    match Verdict::from_stored(stored) {
        Some(v) => (
            v.observed_result_kind().human_label().to_string(),
            v.expectation_assessment_kind().human_label().to_string(),
        ),
        None => (stored.to_string(), String::new()),
    }
}

/// Extract the expectation name from a frozen legacy assessment
/// statement ("Consistent with the redundant-attachment expectation."
/// -> "redundant-attachment") so current presentation can keep the
/// parenthetical without reinterpreting evidence.
fn expectation_name_from_legacy(statement: &str) -> Option<String> {
    for prefix in [
        "Consistent with the ",
        "Inconsistent with the ",
        "Partially consistent with the ",
        "Indeterminate relative to the ",
        "Not assessable from the ",
    ] {
        if let Some(rest) = statement.strip_prefix(prefix) {
            for suffix in [" expectation.", " expectation"] {
                if let Some(name) = rest.strip_suffix(suffix) {
                    return Some(name.to_string());
                }
            }
        }
    }
    None
}

/// The reviewed BGP applicability of an event (ticket_reviews), if any.
fn reviewed_applicability(
    conn: &rusqlite::Connection,
    event_id: i64,
) -> Result<Option<String>, String> {
    use rusqlite::OptionalExtension;
    conn.query_row(
        "SELECT analysis_applicability FROM ticket_reviews
         WHERE catalog_event_id = ?1
         ORDER BY reviewed_at DESC, id DESC LIMIT 1",
        [event_id],
        |r| r.get(0),
    )
    .optional()
    .map_err(|e| format!("catalog read failed: {e}"))
}

/// The ticket-level result line for an event whose reviewed
/// relationship is not directly observable in public BGP.
fn not_directly_observable_line() -> &'static str {
    "The named relationship is not directly assessable with public BGP."
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
    let applicability = reviewed_applicability(conn, event.id)?;
    let supporting_only = applicability.as_deref()
        == Some(crate::catalog::domain::applicability::NOT_DIRECTLY_OBSERVABLE);
    // For a relationship not directly observable in public BGP, the
    // ticket-level result is the observability statement; any BGP run
    // is a scope-mismatched supporting observation and carries NO
    // expectation assessment against the ticket.
    let (result, assessment) = if supporting_only {
        (
            Some(not_directly_observable_line().to_string()),
            Some(String::new()),
        )
    } else {
        (result, assessment)
    };
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
        .map(|r| {
            let stored = r.verdict.clone().unwrap_or_default();
            let (observed, expectation) = present_run_verdict(&stored);
            let mut assessment = r.assessment.clone().unwrap_or_default();
            if supporting_only {
                // Scope-mismatched supporting observation: the run row
                // names the observed result but carries no expectation
                // assessment against the optical relationship.
                assessment = String::new();
            } else if !expectation.is_empty() {
                if let Some(name) = expectation_name_from_legacy(&assessment) {
                    assessment = format!("{expectation} ({name}).");
                } else {
                    assessment = expectation;
                }
            }
            RunRowView {
                id: r.id,
                status: r.status.clone(),
                started_at: r.started_at.clone(),
                verdict: if supporting_only {
                    format!("{observed} (supporting observation; scope mismatch)")
                } else {
                    observed
                },
                assessment,
            }
        })
        .collect();

    // ── Job-workflow status (Part 22) ─────────────────────────────
    // Plan status, job status, and run status stay visually distinct.
    let (workflow_status, workflow_link, workflow_detail, active_job_id, completed_run_id) =
        workflow_status_for(conn, event.id, &st, &result)?;

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
        supporting_only,
        applicability: applicability.unwrap_or_default(),
        stale_reason,
        snapshots: snapshot_views,
        manifests: manifest_views,
        runs: run_rows,
        workflow_status,
        workflow_link,
        workflow_detail,
        active_job_id,
        completed_run_id,
    }))
}

/// The event-page workflow line: status text, link, detail, active job
/// id (when any), completed run id (when any).
type WorkflowStatus = (String, String, String, Option<String>, Option<i64>);

/// Compute the event-page workflow line. Plan status, job status, and
/// run status are separate concepts; the line shows the LATEST relevant
/// state without hiding history (multiple plan revisions, jobs, and
/// runs remain visible in their own sections).
fn workflow_status_for(
    conn: &rusqlite::Connection,
    event_id: i64,
    _st: &CatalogStatus,
    latest_verdict: &Option<String>,
) -> Result<WorkflowStatus, String> {
    use crate::catalog::jobs::service::{latest_manifest_revision, latest_plan};
    let plan = latest_plan(conn, event_id)?;
    let jobs = match &plan {
        Some(p) => crate::catalog::jobs::service::list(
            conn,
            &crate::catalog::jobs::service::JobFilter {
                state: None,
                plan_revision_id: Some(p.id),
            },
        )?,
        None => Vec::new(),
    };
    let active = jobs.iter().find(|j| j.state.is_active());
    let latest = jobs.first();
    let completed = jobs.iter().find(|j| j.completed_run_id.is_some());
    if let Some(job) = active {
        return Ok((
            format!("Analysis running ({})", job.state.as_str()),
            format!("/analysis-jobs/{}", job.id),
            "execution state; the workbench is derived from completed runs only".to_string(),
            Some(job.id.clone()),
            completed.and_then(|j| j.completed_run_id),
        ));
    }
    if let Some(run_id) = completed.and_then(|j| j.completed_run_id) {
        return Ok((
            "Open workbench".to_string(),
            format!("/events/{event_id}/workbench"),
            format!(
                "completed run {run_id}; verdict: {}",
                latest_verdict.as_deref().unwrap_or("—")
            ),
            None,
            Some(run_id),
        ));
    }
    if let Some(job) = latest {
        if job.state == JobState::Failed {
            return Ok((
                "Latest execution failed".to_string(),
                format!("/analysis-jobs/{}", job.id),
                format!(
                    "{}: {}",
                    job.error_code
                        .clone()
                        .unwrap_or_else(|| "error".to_string()),
                    job.error_summary.clone().unwrap_or_default()
                ),
                None,
                None,
            ));
        }
    }
    let _ = latest_manifest_revision(conn, event_id)?;
    let (status, link, detail) = match plan {
        Some(p) if p.status == "Ready" => (
            "Ready to queue".to_string(),
            format!("/events/{event_id}/analysis-plan"),
            "reviewed plan is ready; queue it for the worker".to_string(),
        ),
        Some(_) => (
            "Analysis planning blocked".to_string(),
            format!("/events/{event_id}/analysis-plan"),
            "review the plan to see the exact blocker reasons".to_string(),
        ),
        None => (
            "Prepare analysis".to_string(),
            format!("/events/{event_id}/analysis-plan"),
            "no plan revision exists yet".to_string(),
        ),
    };
    Ok((status, link, detail, None, None))
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

    // Current presentation: the stored machine verdict (or frozen
    // legacy label) is mapped through the source-neutral observed-result
    // vocabulary. The report's stored verdict_label is never rendered
    // verbatim when it conflates observation with expectation.
    let stored = run.verdict.clone().unwrap_or_else(|| "Unknown".to_string());
    let (observed, expectation) = present_run_verdict(&stored);
    let result_label = if report_available {
        report_value
            .get("result")
            .and_then(|r| r.get("observed_result"))
            .and_then(|o| o.get("label"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or(observed)
    } else {
        observed
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
        // Prefer the structured v3 kind; for v2 reports map the machine
        // verdict and keep the legacy statement's expectation name.
        if let Some(kind) = report_value
            .get("assessment")
            .and_then(|a| a.get("kind"))
            .and_then(|v| v.as_str())
        {
            crate::domain::assessment::ExpectationAssessmentKind::from_label(kind)
                .map(|k| format!("{}.", k.human_label()))
                .unwrap_or_default()
        } else {
            let statement = report_value
                .get("assessment")
                .and_then(|a| a.get("statement"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if let Some(name) = expectation_name_from_legacy(statement) {
                format!("{expectation} ({name}).")
            } else {
                expectation
            }
        }
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

// ── Case-study views  ─────────────────────

#[derive(Template)]
#[template(path = "case_studies.html")]
pub struct CaseStudyListView {
    pub rows: Vec<CaseStudyRowView>,
}

#[derive(Serialize)]
pub struct CaseStudyRowView {
    pub slug: String,
    pub title: String,
    pub date: String,
    pub status: String,
    pub documents: usize,
    pub events: usize,
    pub runs: usize,
    pub research_state: String,
    pub latest_result: String,
}

#[derive(Template)]
#[template(path = "case_study.html")]
pub struct CaseStudyView {
    pub slug: String,
    pub title: String,
    pub date: String,
    pub status: String,
    pub summary: String,
    pub what_happened: String,
    pub what_bgp_showed: String,
    pub what_bgp_could_not_show: Vec<String>,
    pub phases: Vec<PhaseView>,
    pub related_tickets: Vec<RelatedTicketView>,
    pub documents: Vec<DocumentView>,
    pub targets: Vec<TargetView>,
    pub plan: Option<PlanView>,
    pub runs: Vec<RunLinkView>,
    pub phase_summaries: Vec<PhaseSummaryView>,
    pub comparison: Vec<ComparisonRowView>,
    pub observability_potentially_visible: usize,
    pub observability_indirectly_visible: usize,
    pub observability_not_directly_visible: usize,
    pub observability_unknown: usize,
    /// Reviewed corpus tickets related to the case study.
    pub public_tickets: Vec<PublicTicketView>,
    /// Cross-observer comparison over linked runs.
    pub observer_comparison: ObserverComparisonView,
}

/// One related public corpus ticket with its reviewed interpretation.
#[derive(Serialize)]
pub struct PublicTicketView {
    pub external_id: String,
    pub title: String,
    pub task_type: String,
    pub reviewed_roles: String,
    pub source_window: String,
    pub relationship_evidence: String,
    pub readiness: String,
    pub next_action: String,
}

/// Cross-observer comparison (rows + statements + conclusion wording).
#[derive(Serialize, Default)]
pub struct ObserverComparisonView {
    pub rows: Vec<ObserverComparisonRowView>,
    pub statements: Vec<ObserverStatementView>,
    /// Narrow conclusion wording per the reviewed session brief.
    pub conclusion: String,
    /// Explainer lines for the first screen (distinct routing planes).
    pub planes_explainer: Vec<String>,
}

#[derive(Serialize)]
pub struct ObserverComparisonRowView {
    pub prefix: String,
    pub collector: String,
    pub family: String,
    pub peer: String,
    /// Historically correct collector location (reviewed metadata).
    pub location: String,
    /// Peer ASN from the historical RIB MRT header (session audit).
    pub peer_asn: String,
    /// Direct vs indirect relationship to the named planes (audit data).
    pub relationship: String,
    /// Named service plane of the run's cohort.
    pub plane: String,
    /// The reviewed cohort predicate that selected this run's streams.
    pub cohort_predicate: String,
    pub first_change_utc: String,
    pub temporary_absence: String,
    pub path_replacement: String,
    pub transit_departure: String,
    pub restoration_utc: String,
    pub baseline_visibility: String,
}

#[derive(Serialize)]
pub struct ObserverStatementView {
    pub prefix: String,
    pub visible_at: String,
    pub changed_at: String,
    pub statement: String,
    pub timing_note: String,
}

#[derive(Serialize)]
pub struct PhaseView {
    pub label: String,
    pub start_utc: String,
    pub end_utc: String,
    pub start_precision: String,
    pub end_precision: String,
    pub description: String,
    pub source_section: String,
    pub review_status: String,
}

#[derive(Serialize)]
pub struct RelatedTicketView {
    pub external_id: String,
    pub relationship: String,
    pub reviewed_note: String,
    /// Link to the catalog event page when a snapshot-backed event exists.
    pub event_href: Option<String>,
}

#[derive(Serialize)]
pub struct DocumentView {
    pub id: i64,
    pub title: String,
    pub source_url: String,
    pub doc_type: String,
    pub media_type: String,
    pub sha256: String,
    pub pages: String,
    pub redistribution_status: String,
    pub provenance: String,
    /// A validated `/documents/<id>` href when a local copy exists.
    pub href: Option<String>,
}

#[derive(Serialize)]
pub struct TargetView {
    pub source_label: String,
    pub role_in_report: String,
    pub candidate_org: String,
    pub candidate_asns: String,
    pub historical_validity_status: String,
    pub research_status: String,
    pub path_predicate_status: String,
    pub provenance: String,
    pub reviewed_note: String,
    pub research_updated: String,
}

#[derive(Serialize)]
pub struct PlanView {
    pub status: String,
    pub warmup_start: String,
    pub incident_start: String,
    pub incident_end: String,
    pub cooldown_end: String,
    pub collectors: Vec<String>,
    pub estimated_bytes: i64,
    pub estimated_uncompressed_bytes: i64,
    pub blocked_targets: Vec<String>,
    pub skipped_targets: Vec<String>,
    pub notes: Vec<String>,
    pub baseline_ribs: Vec<String>,
    pub validation_ribs: Vec<String>,
    pub update_ranges: Vec<String>,
    /// "none" when no pilot has been planned.
    pub pilot_status: String,
    pub pilot_target: String,
    pub pilot_collector: String,
    pub pilot_window: String,
    pub pilot_run_id: String,
    pub pilot_baseline_streams: usize,
    pub pilot_operator_evidence: String,
    pub pilot_bgp_observation: String,
    pub pilot_temporal_relationship: String,
    pub pilot_interpretation: String,
    pub pilot_limitation: String,
    pub pilot_finding: String,
}

#[derive(Serialize)]
pub struct RunLinkView {
    pub id: i64,
    pub started_at: String,
    pub verdict: String,
    pub assessment: String,
}

#[derive(Serialize)]
pub struct PhaseSummaryView {
    pub run_id: i64,
    pub phase_label: String,
    pub phase_start: String,
    pub phase_end: String,
    pub active_streams_entering: usize,
    pub announcements: usize,
    pub withdrawals: usize,
    pub path_changes: usize,
    pub transit_departures: usize,
    pub restorations: usize,
    pub semantic_waves: Vec<String>,
    pub first_evidence_utc: String,
    pub last_evidence_utc: String,
    pub evidence_observation_ids: Vec<i64>,
}

#[derive(Serialize)]
pub struct ComparisonRowView {
    pub operator_report: String,
    pub operator_time: String,
    pub bgp_observation: String,
    pub interpretation: String,
    pub temporal_detail: String,
    pub limitation: String,
}

/// Load the case-study list (deterministic order by slug).
pub fn load_case_studies(conn: &rusqlite::Connection) -> Result<CaseStudyListView, String> {
    let mut stmt = conn
        .prepare(
            "SELECT cs.id, cs.slug, cs.title, cs.start_utc, cs.status,
                    (SELECT COUNT(*) FROM case_study_event_links l WHERE l.case_study_id = cs.id
                      AND l.catalog_event_id IS NOT NULL),
                    (SELECT COUNT(*) FROM case_study_analysis_links a WHERE a.case_study_id = cs.id),
                    (SELECT COUNT(*) FROM case_study_document_links d WHERE d.case_study_id = cs.id)
             FROM case_studies cs ORDER BY cs.slug",
        )
        .map_err(|e| format!("catalog read failed: {e}"))?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, Option<String>>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, i64>(5)?,
                r.get::<_, i64>(6)?,
                r.get::<_, i64>(7)?,
            ))
        })
        .map_err(|e| format!("catalog read failed: {e}"))?;
    let mut out = Vec::new();
    for row in rows {
        let (slug, title, start, status, events, runs, documents) =
            row.map_err(|e| format!("catalog read failed: {e}"))?;
        let research_state = research_state_for(conn, &slug)?;
        let latest_result = if runs == 0 {
            "no analysis runs; no BGP verdict".to_string()
        } else {
            "analysis runs linked (see detail)".to_string()
        };
        out.push(CaseStudyRowView {
            slug,
            title,
            date: start.unwrap_or_default(),
            status,
            documents: documents as usize,
            events: events as usize,
            runs: runs as usize,
            research_state,
            latest_result,
        });
    }
    Ok(CaseStudyListView { rows: out })
}

/// Research/readiness state: all targets reviewed vs pending.
fn research_state_for(conn: &rusqlite::Connection, slug: &str) -> Result<String, String> {
    let total: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM case_study_targets t
             JOIN case_studies cs ON cs.id = t.case_study_id WHERE cs.slug = ?1",
            [slug],
            |r| r.get(0),
        )
        .unwrap_or(0);
    let unresolved: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM case_study_targets t
             JOIN case_studies cs ON cs.id = t.case_study_id
             WHERE cs.slug = ?1 AND t.research_status != 'HistoricallyReviewed'",
            [slug],
            |r| r.get(0),
        )
        .unwrap_or(0);
    Ok(if total == 0 {
        "no analysis targets".to_string()
    } else if unresolved == 0 {
        "target research complete".to_string()
    } else {
        format!("target research incomplete ({unresolved}/{total} unresolved)")
    })
}

/// Load the case-study detail view; None when the slug is unknown.
/// Narrow conclusion wording for the observer comparison
/// . Multi-observer corresponding changes use the reviewed
/// sentence; single-observer observations say so directly; disagreement
/// is never hidden.
/// Narrow conclusion wording for the observer comparison
/// . The reviewed target label comes from the case-study plan
/// data (never hard-coded in source); multi-observer corresponding
/// changes use the reviewed sentence; a single-observer signature says
/// so directly; disagreement is never hidden.
fn observer_conclusion(
    c: &crate::catalog::observer_compare::ObserverComparison,
    target_label: &str,
) -> String {
    let target_sentence = if target_label.is_empty() {
        "the reviewed target".to_string()
    } else {
        format!("the reviewed {target_label} target")
    };
    let multi = c
        .statements
        .iter()
        .any(|s| s.statement.starts_with("Observed at") && s.changed_at.len() >= 2);
    let single = c
        .statements
        .iter()
        .any(|s| s.statement == "Observed only at one selected collector");
    if multi {
        format!(
            "Similar transient route-state disruption was observed at multiple selected public collectors for {target_sentence}. This does not establish traffic loss, the Layer-2 mechanism, or a complete incident impact."
        )
    } else if single {
        format!(
            "Route-state change was observed at one selected public collector for {target_sentence}; other selected collectors did not show a corresponding change. This does not establish traffic loss, the Layer-2 mechanism, or a complete incident impact."
        )
    } else if c.statements.is_empty() {
        format!(
            "No selected observer had baseline visibility for {target_sentence}; no cross-observer comparison is possible."
        )
    } else {
        format!(
            "No route-state disruption was observed at the selected public collectors for {target_sentence}."
        )
    }
}

pub fn load_case_study(
    conn: &rusqlite::Connection,
    slug: &str,
) -> Result<Option<CaseStudyView>, String> {
    let Some(cs) = crate::catalog::archive_plan::find_case_study(conn, slug) else {
        return Ok(None);
    };
    let cs_id = cs.id;

    // Phases.
    let mut phases = Vec::new();
    {
        let mut stmt = conn
            .prepare(
                "SELECT label, start_utc, end_utc, start_precision, end_precision,
                        description, source_page_or_section, review_status
                 FROM case_study_phases WHERE case_study_id = ?1 ORDER BY sort_order",
            )
            .map_err(|e| format!("catalog read failed: {e}"))?;
        let rows = stmt
            .query_map([cs_id], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, String>(5)?,
                    r.get::<_, String>(6)?,
                    r.get::<_, String>(7)?,
                ))
            })
            .map_err(|e| format!("catalog read failed: {e}"))?;
        for row in rows {
            let (label, start, end, sp, ep, desc, section, review) =
                row.map_err(|e| format!("catalog read failed: {e}"))?;
            phases.push(PhaseView {
                label,
                start_utc: start,
                end_utc: end,
                start_precision: sp,
                end_precision: ep,
                description: desc,
                source_section: section,
                review_status: review,
            });
        }
    }

    // Related tickets.
    let mut related_tickets = Vec::new();
    {
        let mut stmt = conn
            .prepare(
                "SELECT l.external_identifier, l.relationship,
                        COALESCE(l.reviewed_note, ''), l.catalog_event_id
                 FROM case_study_event_links l
                 WHERE l.case_study_id = ?1 ORDER BY l.sort_order",
            )
            .map_err(|e| format!("catalog read failed: {e}"))?;
        let rows = stmt
            .query_map([cs_id], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, Option<i64>>(3)?,
                ))
            })
            .map_err(|e| format!("catalog read failed: {e}"))?;
        for row in rows {
            let (external_id, relationship, note, event_id) =
                row.map_err(|e| format!("catalog read failed: {e}"))?;
            let event_href = event_id.map(|_| format!("/events/{external_id}"));
            related_tickets.push(RelatedTicketView {
                external_id,
                relationship,
                reviewed_note: note,
                event_href,
            });
        }
    }

    // Documents (latest revision per document).
    let mut documents = Vec::new();
    {
        let mut stmt = conn
            .prepare(
                "SELECT d.id, d.title, COALESCE(d.source_url, ''), d.doc_type,
                        d.redistribution_status, d.provenance, r.media_type, r.sha256,
                        r.page_count, r.local_path
                 FROM reference_documents d
                 JOIN case_study_document_links l ON l.document_id = d.id
                 LEFT JOIN document_revisions r ON r.id = (
                     SELECT id FROM document_revisions r2
                     WHERE r2.document_id = d.id ORDER BY r2.revision DESC LIMIT 1)
                 WHERE l.case_study_id = ?1
                 ORDER BY d.title",
            )
            .map_err(|e| format!("catalog read failed: {e}"))?;
        let rows = stmt
            .query_map([cs_id], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, String>(5)?,
                    r.get::<_, Option<String>>(6)?,
                    r.get::<_, Option<String>>(7)?,
                    r.get::<_, Option<i64>>(8)?,
                    r.get::<_, Option<String>>(9)?,
                ))
            })
            .map_err(|e| format!("catalog read failed: {e}"))?;
        for row in rows {
            let (id, title, url, doc_type, redist, prov, media, sha, pages, local) =
                row.map_err(|e| format!("catalog read failed: {e}"))?;
            documents.push(DocumentView {
                id,
                title,
                source_url: url,
                doc_type,
                media_type: media.unwrap_or_default(),
                sha256: sha.unwrap_or_default(),
                pages: pages
                    .map(|p| p.to_string())
                    .unwrap_or_else(|| "n/a".to_string()),
                redistribution_status: redist,
                provenance: prov,
                href: local
                    .filter(|p| !p.is_empty())
                    .map(|_| format!("/documents/{id}")),
            });
        }
    }

    // Targets.
    let mut targets = Vec::new();
    {
        let mut stmt = conn
            .prepare(
                "SELECT source_label, role_in_report, COALESCE(candidate_org_identity, ''),
                        COALESCE(candidate_origin_asns_json, '[]'),
                        COALESCE(candidate_predicate, ''),
                        historical_validity_status, research_status,
                        COALESCE(path_predicate_status, ''),
                        COALESCE(provenance, ''), COALESCE(reviewed_note, ''),
                        COALESCE(research_updated_utc, '')
                 FROM case_study_targets WHERE case_study_id = ?1 ORDER BY sort_order",
            )
            .map_err(|e| format!("catalog read failed: {e}"))?;
        let rows = stmt
            .query_map([cs_id], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, String>(5)?,
                    r.get::<_, String>(6)?,
                    r.get::<_, String>(7)?,
                    r.get::<_, String>(8)?,
                    r.get::<_, String>(9)?,
                    r.get::<_, String>(10)?,
                ))
            })
            .map_err(|e| format!("catalog read failed: {e}"))?;
        for row in rows {
            let (label, role, org, asns, pred, hv, rs, pstatus, prov, note, updated) =
                row.map_err(|e| format!("catalog read failed: {e}"))?;
            targets.push(TargetView {
                source_label: label,
                role_in_report: role,
                candidate_org: org,
                candidate_asns: if asns == "[]" || asns.is_empty() {
                    "none reviewed (no guesses)".to_string()
                } else {
                    asns
                },
                historical_validity_status: hv,
                research_status: rs,
                path_predicate_status: if pred.is_empty() {
                    pstatus
                } else {
                    format!("{pred} ({pstatus})")
                },
                provenance: prov,
                reviewed_note: note,
                research_updated: updated,
            });
        }
    }

    // Analysis plan.
    let plan = crate::catalog::archive_plan::load_plan(conn, cs_id).map(|p| {
        let horizon: crate::catalog::archive_plan::AnalysisHorizon = serde_json::from_str(
            &p.horizon_json,
        )
        .unwrap_or(crate::catalog::archive_plan::AnalysisHorizon {
            warmup_start_utc: String::new(),
            incident_start_utc: String::new(),
            incident_end_utc: String::new(),
            cooldown_end_utc: String::new(),
            warmup_hours: 0,
            cooldown_hours: 0,
            review_required: true,
        });
        let ap: crate::catalog::archive_plan::ArchivePlan = serde_json::from_str(&p.plan_json)
            .unwrap_or(crate::catalog::archive_plan::ArchivePlan {
                collectors: Vec::new(),
                blocked_targets: Vec::new(),
                skipped_targets: Vec::new(),
                estimated_total_bytes: 0,
                estimated_total_uncompressed_bytes: 0,
                estimated_total_is_estimate: true,
                notes: Vec::new(),
                pilot: None,
            });
        let baseline_ribs: Vec<String> = ap
            .collectors
            .iter()
            .map(|c| {
                format!(
                    "{}: {}",
                    c.collector,
                    c.baseline_rib.url.rsplit('/').next().unwrap_or_default()
                )
            })
            .collect();
        let validation_ribs: Vec<String> = ap
            .collectors
            .iter()
            .map(|c| {
                format!(
                    "{}: {}",
                    c.collector,
                    c.validation_rib
                        .as_ref()
                        .map(|r| r.url.rsplit('/').next().unwrap_or_default().to_string())
                        .unwrap_or_else(|| "none".to_string())
                )
            })
            .collect();
        let update_ranges: Vec<String> = ap
            .collectors
            .iter()
            .map(|c| {
                format!(
                    "{}: {} → {} ({} files)",
                    c.collector,
                    c.first_update_utc,
                    c.last_update_utc,
                    c.updates.len()
                )
            })
            .collect();
        let (
            pilot_status,
            pilot_target,
            pilot_collector,
            pilot_window,
            pilot_run_id,
            pilot_baseline_streams,
            pilot_operator_evidence,
            pilot_bgp_observation,
            pilot_temporal_relationship,
            pilot_interpretation,
            pilot_limitation,
            pilot_finding,
        ) = match &ap.pilot {
            Some(pl) => (
                pl.status.clone(),
                pl.target.clone(),
                pl.collector.clone(),
                format!("{} → {}", pl.window_start_utc, pl.window_end_utc),
                pl.run_id.map(|r| r.to_string()).unwrap_or_default(),
                pl.baseline_streams,
                pl.operator_evidence.clone(),
                pl.bgp_observation.clone(),
                pl.temporal_relationship.clone(),
                pl.interpretation.clone(),
                pl.limitation.clone(),
                pl.finding.clone(),
            ),
            None => (
                "Not planned".to_string(),
                String::new(),
                String::new(),
                String::new(),
                String::new(),
                0,
                String::new(),
                String::new(),
                String::new(),
                String::new(),
                String::new(),
                String::new(),
            ),
        };
        PlanView {
            status: p.status,
            warmup_start: horizon.warmup_start_utc,
            incident_start: horizon.incident_start_utc,
            incident_end: horizon.incident_end_utc,
            cooldown_end: horizon.cooldown_end_utc,
            collectors: ap
                .collectors
                .iter()
                .map(|c| {
                    format!(
                        "{} (baseline {} + validation {}; {} updates)",
                        c.collector,
                        c.baseline_rib.url.rsplit('/').next().unwrap_or_default(),
                        c.validation_rib
                            .as_ref()
                            .map(|r| r.url.rsplit('/').next().unwrap_or_default().to_string())
                            .unwrap_or_else(|| "none".to_string()),
                        c.updates.len()
                    )
                })
                .collect(),
            estimated_bytes: ap.estimated_total_bytes,
            estimated_uncompressed_bytes: ap.estimated_total_uncompressed_bytes,
            baseline_ribs,
            validation_ribs,
            update_ranges,
            pilot_status,
            pilot_target,
            pilot_collector,
            pilot_window,
            pilot_run_id,
            pilot_baseline_streams,
            pilot_operator_evidence,
            pilot_bgp_observation,
            pilot_temporal_relationship,
            pilot_interpretation,
            pilot_limitation,
            pilot_finding,
            blocked_targets: ap
                .blocked_targets
                .iter()
                .map(|b| format!("{} — {}", b.source_label, b.reason))
                .collect(),
            skipped_targets: ap
                .skipped_targets
                .iter()
                .map(|b| format!("{} — {}", b.source_label, b.reason))
                .collect(),
            notes: ap.notes,
        }
    });

    // Linked runs.
    let mut runs = Vec::new();
    {
        let mut stmt = conn
            .prepare(
                "SELECT r.id, r.started_at, COALESCE(r.verdict, ''), COALESCE(r.assessment, '')
                 FROM analysis_runs r
                 JOIN case_study_analysis_links l ON l.run_id = r.id
                 WHERE l.case_study_id = ?1 ORDER BY r.id",
            )
            .map_err(|e| format!("catalog read failed: {e}"))?;
        let rows = stmt
            .query_map([cs_id], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                ))
            })
            .map_err(|e| format!("catalog read failed: {e}"))?;
        for row in rows {
            let (id, started, verdict, assessment) =
                row.map_err(|e| format!("catalog read failed: {e}"))?;
            runs.push(RunLinkView {
                id,
                started_at: started,
                verdict,
                assessment,
            });
        }
    }

    // Phase-conditioned BGP summaries (per linked run).
    let mut phase_summaries = Vec::new();
    for run in &runs {
        let rs = crate::catalog::phase_summary::summarize_run(conn, run.id, cs_id)?;
        for p in &rs.phases {
            phase_summaries.push(PhaseSummaryView {
                run_id: run.id,
                phase_label: p.label.clone(),
                phase_start: p.start_utc.clone(),
                phase_end: p.end_utc.clone(),
                active_streams_entering: p.active_streams_entering,
                announcements: p.announcements,
                withdrawals: p.withdrawals,
                path_changes: p.path_changes,
                transit_departures: p.transit_departures,
                restorations: p.restorations,
                semantic_waves: p.semantic_waves.clone(),
                first_evidence_utc: p.first_evidence_utc.clone().unwrap_or_default(),
                last_evidence_utc: p.last_evidence_utc.clone().unwrap_or_default(),
                evidence_observation_ids: p.evidence_observation_ids.clone(),
            });
        }
    }

    // Comparison matrix.
    let mut comparison = Vec::new();
    for row in crate::catalog::case_study_compare::build_comparison(conn, cs_id)? {
        comparison.push(ComparisonRowView {
            operator_report: row.operator_report,
            operator_time: row.operator_time.unwrap_or_default(),
            bgp_observation: row.bgp_observation,
            interpretation: row.interpretation,
            temporal_detail: row.temporal_detail,
            limitation: row.limitation,
        });
    }

    // Observability classification counts.
    let obs = |o: &str| -> usize {
        conn.query_row(
            "SELECT COUNT(*) FROM case_study_claims c WHERE c.case_study_id = ?1 AND c.observability = ?2",
            rusqlite::params![cs_id, o],
            |r| r.get(0),
        )
        .unwrap_or(0) as usize
    };

    // First-screen derived summaries.
    let what_happened = format!(
        "{} ({} – {} UTC)",
        cs.summary,
        cs.start_utc.as_deref().unwrap_or("unknown"),
        cs.end_utc.as_deref().unwrap_or("unknown")
    );
    let what_bgp_showed = if runs.is_empty() {
        "Historical analysis not yet executed. No event-conditioned AnalysisRun is linked to this case study. No public-BGP conclusion is produced until historical target mappings and the archive plan are reviewed.".to_string()
    } else {
        let total_transitions: usize = phase_summaries
            .iter()
            .map(|p| p.announcements + p.withdrawals + p.path_changes + p.restorations)
            .sum();
        format!(
            "{total_transitions} route-state stream-counts observed across {} linked run(s); see the phase-conditioned summaries and comparison matrix below.",
            runs.len()
        )
    };
    let mut what_bgp_could_not_show = Vec::new();
    {
        let mut stmt = conn
            .prepare(
                "SELECT claim_text, observability_rationale FROM case_study_claims
                 WHERE case_study_id = ?1 AND observability IN ('NotDirectlyVisible', 'IndirectlyVisible')
                 ORDER BY sort_order",
            )
            .map_err(|e| format!("catalog read failed: {e}"))?;
        let rows = stmt
            .query_map([cs_id], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })
            .map_err(|e| format!("catalog read failed: {e}"))?;
        for row in rows {
            let (text, rationale) = row.map_err(|e| format!("catalog read failed: {e}"))?;
            what_bgp_could_not_show.push(format!("{text} — {rationale}"));
        }
    }

    // ── reviewed public tickets + observer comparison ────
    let mut public_tickets = Vec::new();
    {
        let mut stmt = conn
            .prepare(
                "SELECT l.catalog_event_id FROM case_study_event_links l
                 JOIN catalog_events e ON e.id = l.catalog_event_id
                 WHERE l.case_study_id = ?1 AND l.catalog_event_id IS NOT NULL
                   AND e.source_kind = 'grnoc-public-task-viewer'
                 ORDER BY e.external_id",
            )
            .map_err(|e| format!("catalog read failed: {e}"))?;
        let event_ids: Vec<i64> = stmt
            .query_map([cs_id], |r| r.get::<_, i64>(0))
            .map_err(|e| format!("catalog read failed: {e}"))?
            .flatten()
            .collect();
        for event_id in event_ids {
            let event = crate::catalog::db::get_event(conn, event_id)?
                .ok_or_else(|| "linked event missing".to_string())?;
            let review = crate::catalog::store::get_ticket_review(conn, event_id)?;
            let snap = crate::catalog::db::list_snapshots(conn, event_id)?
                .first()
                .cloned();
            let mut title = String::new();
            let mut task_type = String::new();
            let mut source_window = String::new();
            if let Some(s) = &snap {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&s.normalized_json) {
                    title = v
                        .get("title")
                        .and_then(|x| x.as_str())
                        .unwrap_or("")
                        .to_string();
                    task_type = v
                        .get("task_type")
                        .and_then(|x| x.as_str())
                        .unwrap_or("")
                        .to_string();
                    let start = v
                        .get("start")
                        .and_then(|x| x.as_str())
                        .unwrap_or("")
                        .to_string();
                    let end = v
                        .get("end")
                        .and_then(|x| x.as_str())
                        .unwrap_or("")
                        .to_string();
                    source_window = if start.is_empty() && end.is_empty() {
                        "no source window (AAR-dated)".to_string()
                    } else {
                        format!("{start} → {end}")
                    };
                }
            }
            // Relationship evidence: first reviewed or explicit edge from
            // this ticket.
            let mut relationship_evidence = String::new();
            for edge in crate::catalog::store::list_relationships(conn, Some(event_id))? {
                if edge.evidence_kind == crate::catalog::domain::EVIDENCE_DERIVED_TEMPORAL_OVERLAP {
                    continue;
                }
                relationship_evidence =
                    format!("{} ({})", edge.relationship_kind, edge.evidence_kind);
                break;
            }
            let analyzability = crate::catalog::analyzability::derive_analyzability(conn, &event)?;
            let applicability = review
                .as_ref()
                .map(|r| r.analysis_applicability.as_str())
                .unwrap_or("");
            let next_action = crate::catalog::analyzability::next_analyst_action(
                &analyzability.readiness,
                applicability,
            )
            .to_string();
            public_tickets.push(PublicTicketView {
                external_id: event.external_id,
                title,
                task_type,
                reviewed_roles: review
                    .map(|r| r.reviewed_roles.join(", "))
                    .unwrap_or_default(),
                source_window,
                relationship_evidence,
                readiness: analyzability.readiness,
                next_action,
            });
        }
    }

    let comparison_data = crate::catalog::observer_compare::build_observer_comparison(conn, cs_id)?;
    let target_label = plan
        .as_ref()
        .map(|p| p.pilot_target.clone())
        .filter(|t| !t.is_empty())
        .unwrap_or_default();
    let conclusion = observer_conclusion(&comparison_data, &target_label);
    // Reviewed session context (data files; absent for other case
    // studies — the comparison then shows the plain columns only).
    let session_context =
        crate::catalog::web::session_context::SessionContext::load_for_slug(&cs.slug);
    let observer_rows = comparison_data
        .rows
        .iter()
        .map(|r| {
            let ctx = session_context
                .as_ref()
                .and_then(|c| c.lookup(&r.family, &r.collector, &r.peer));
            let (location, peer_asn, relationship, plane, cohort_predicate) = match ctx {
                Some((loc, asn, rel, plane_label, pred)) => (
                    loc.to_string(),
                    asn.to_string(),
                    rel.to_string(),
                    plane_label.to_string(),
                    pred.to_string(),
                ),
                None => (
                    String::new(),
                    String::new(),
                    String::new(),
                    String::new(),
                    String::new(),
                ),
            };
            ObserverComparisonRowView {
                prefix: r.prefix.clone(),
                collector: r.collector.clone(),
                family: r.family.clone(),
                peer: r.peer.clone(),
                location,
                peer_asn,
                relationship,
                plane,
                cohort_predicate,
                first_change_utc: r.first_change_utc.clone().unwrap_or_default(),
                temporary_absence: r.temporary_absence.clone().unwrap_or_default(),
                path_replacement: if r.path_replacement {
                    "yes".into()
                } else {
                    "no".into()
                },
                transit_departure: if r.transit_departure {
                    "yes".into()
                } else {
                    "no".into()
                },
                restoration_utc: r.restoration_utc.clone().unwrap_or_default(),
                baseline_visibility: if r.baseline_visibility {
                    "yes".into()
                } else {
                    "no".into()
                },
            }
        })
        .collect();
    let observer_statements = comparison_data
        .statements
        .iter()
        .map(|s| ObserverStatementView {
            prefix: s.prefix.clone(),
            visible_at: s.visible_at.join(", "),
            changed_at: s.changed_at.join(", "),
            statement: s.statement.clone(),
            timing_note: s.timing_note.clone(),
        })
        .collect();
    let observer_comparison = ObserverComparisonView {
        rows: observer_rows,
        statements: observer_statements,
        conclusion: session_context
            .as_ref()
            .map(|c| c.conclusion(conclusion.clone()))
            .unwrap_or(conclusion),
        planes_explainer: session_context
            .as_ref()
            .map(|c| c.planes_explainer())
            .unwrap_or_default(),
    };

    Ok(Some(CaseStudyView {
        slug: cs.slug,
        title: cs.title,
        date: cs.start_utc.unwrap_or_default(),
        status: cs.status,
        summary: cs.summary,
        what_happened,
        what_bgp_showed,
        what_bgp_could_not_show,
        phases,
        related_tickets,
        documents,
        targets,
        plan,
        runs,
        phase_summaries,
        comparison,
        public_tickets,
        observer_comparison,
        observability_potentially_visible: obs(
            crate::catalog::domain::OBSERVABILITY_POTENTIALLY_VISIBLE,
        ),
        observability_indirectly_visible: obs(
            crate::catalog::domain::OBSERVABILITY_INDIRECTLY_VISIBLE,
        ),
        observability_not_directly_visible: obs(
            crate::catalog::domain::OBSERVABILITY_NOT_DIRECTLY_VISIBLE,
        ),
        observability_unknown: obs(crate::catalog::domain::OBSERVABILITY_UNKNOWN),
    }))
}

/// A resolved document file ready to serve.
pub struct DocumentServe {
    pub path: std::path::PathBuf,
    pub media_type: String,
    /// Inline only for approved media types; everything else is an attachment.
    pub inline: bool,
}

/// Resolve and validate a document file for serving.
///
/// Security: the record must exist, the stored path must be catalog-relative
/// (no absolute paths, no `..`), the resolved path must remain under the
/// catalog root (canonical containment), the file must exist, and its
/// SHA-256 must match the recorded revision.
pub fn resolve_document_file(
    conn: &rusqlite::Connection,
    catalog_root: &std::path::Path,
    document_id: i64,
) -> Result<Option<DocumentServe>, String> {
    let row: Option<(String, String, Option<String>)> = conn
        .query_row(
            "SELECT r.media_type, r.sha256, r.local_path
             FROM document_revisions r
             JOIN reference_documents d ON d.id = r.document_id
             WHERE d.id = ?1 AND r.local_path IS NOT NULL
             ORDER BY r.revision DESC LIMIT 1",
            [document_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .ok();
    let Some((media_type, sha256, Some(rel))) = row else {
        return Ok(None);
    };
    if rel.is_empty() || rel.starts_with('/') || rel.contains("..") || rel.contains('\\') {
        return Err(format!("document path is not catalog-relative: {rel}"));
    }
    let resolved = catalog_root.join(&rel);
    let root_canon = catalog_root
        .canonicalize()
        .map_err(|e| format!("cannot resolve catalog root: {e}"))?;
    let file_canon = resolved
        .canonicalize()
        .map_err(|e| format!("cannot resolve document file: {e}"))?;
    if !file_canon.starts_with(&root_canon) {
        return Err("document file resolves outside the catalog root".to_string());
    }
    if !file_canon.is_file() {
        return Err("document file is missing".to_string());
    }
    let bytes =
        std::fs::read(&file_canon).map_err(|e| format!("cannot read document file: {e}"))?;
    let actual = crate::catalog::document::hex_sha256(&bytes);
    if actual != sha256 {
        return Err(format!(
            "document hash mismatch: recorded {sha256}, file {actual}"
        ));
    }
    let inline = crate::catalog::document::MEDIA_TYPE_ALLOWLIST
        .iter()
        .any(|(_, mt)| *mt == media_type);
    Ok(Some(DocumentServe {
        path: file_canon,
        media_type,
        inline,
    }))
}

// ── corpus workspace views ─────────────────────────────

#[derive(Template, Serialize)]
#[template(path = "corpus.html")]
pub struct CorpusView {
    pub total_events: usize,
    pub source_snapshots: usize,
    pub oldest_event: String,
    pub newest_event: String,
    pub open_tickets: usize,
    pub closed_tickets: usize,
    pub by_task_type: Vec<(String, usize)>,
    pub not_reviewed: usize,
    pub ready_for_planning: usize,
    pub completed_analyses: usize,
    pub stale_analyses: usize,
    pub latest_sync: String,
    pub sync_status: String,
    pub sync_failures: i64,
    pub policy: String,
}

#[derive(Template, Serialize)]
#[template(path = "sync_runs.html")]
pub struct SyncRunsView {
    pub runs: Vec<SyncRunRowView>,
}

#[derive(Serialize)]
pub struct SyncRunRowView {
    pub id: i64,
    pub started_at: String,
    pub status: String,
    pub examined: i64,
    pub new_events: i64,
    pub changed: i64,
    pub unchanged: i64,
    pub failures: i64,
}

#[derive(Template, Serialize)]
#[template(path = "event_relationships.html")]
pub struct EventRelationshipsView {
    pub event_id: String,
    pub title: String,
    pub analyzability: String,
    pub analyzability_reason: String,
    pub outgoing: Vec<RelationshipRowView>,
    pub incoming: Vec<RelationshipRowView>,
    pub derived: Vec<RelationshipRowView>,
    pub discoveries: Vec<DiscoveryRowView>,
    pub groups: Vec<GroupRowView>,
    pub fetches: Vec<FetchRowView>,
}

#[derive(Serialize)]
pub struct RelationshipRowView {
    pub from: String,
    pub to: String,
    pub kind: String,
    pub evidence: String,
    pub snapshot_id: Option<i64>,
    pub reviewed: String,
    pub note: String,
}

#[derive(Serialize)]
pub struct DiscoveryRowView {
    pub external_id: String,
    pub provenance: String,
    pub status: String,
    pub discovered_at: String,
}

#[derive(Serialize)]
pub struct GroupRowView {
    pub id: i64,
    pub label: String,
    pub confidence: String,
    pub review_status: String,
    pub members: String,
}

#[derive(Serialize)]
pub struct FetchRowView {
    pub fetched_at: String,
    pub http_status: i64,
    pub method: String,
    pub retries: i64,
    pub snapshot_id: Option<i64>,
}

#[derive(Template, Serialize)]
#[template(path = "analysis_queue.html")]
pub struct AnalysisQueueView {
    pub rows: Vec<QueueRowView>,
    pub filters: QueueFilters,
}

#[derive(Serialize, Clone, Default, serde::Deserialize)]
pub struct QueueFilters {
    pub state: Option<String>,
    pub lifecycle: Option<String>,
    pub task_type: Option<String>,
    pub date_from: Option<String>,
    pub date_to: Option<String>,
    pub expectation: Option<String>,
    pub case_study: Option<String>,
    pub q: Option<String>,
}

#[derive(Serialize)]
pub struct QueueRowView {
    pub external_id: String,
    pub title: String,
    pub task_type: String,
    pub lifecycle: String,
    /// Reviewed per-ticket role (ticket_reviews), e.g. PrimaryIncident.
    pub reviewed_role: String,
    /// Reviewed analysis applicability (ticket_reviews).
    pub applicability: String,
    /// Route-selection status: reviewed plane / predicate or none.
    pub selection_status: String,
    pub start: String,
    pub readiness: String,
    pub reason: String,
    pub expectation: String,
    pub case_studies: String,
    /// Reviewed case-study role(s), when a reviewed interpretation exists.
    pub reviewed_roles: String,
    /// Archive-plan status (none / Draft / Ready).
    pub archive_plan_status: String,
    /// Existing analysis runs for this event.
    pub runs: usize,
    /// Derived next analyst action (never executed automatically).
    pub next_action: String,
}

#[derive(Template, Serialize)]
#[template(path = "incident_candidates.html")]
pub struct IncidentCandidatesView {
    pub groups: Vec<IncidentGroupView>,
    pub explicit: usize,
    pub strong: usize,
    pub weak: usize,
    pub coincidence: usize,
    pub rejected: usize,
}

#[derive(Serialize)]
pub struct IncidentGroupView {
    pub id: i64,
    pub label: String,
    pub confidence: String,
    pub review_status: String,
    pub members: Vec<String>,
    pub evidence: Vec<GroupEvidenceView>,
}

#[derive(Serialize)]
pub struct GroupEvidenceView {
    pub signal: String,
    pub detail: String,
}

// ── Reviewed relationship graph  ────────────

#[derive(Template, Serialize)]
#[template(path = "corpus_relationships.html")]
pub struct CorpusRelationshipsView {
    pub reviews: Vec<ReviewRowView>,
    pub audit: Vec<AuditRowView>,
    pub unresolved: usize,
}

#[derive(Serialize)]
pub struct ReviewRowView {
    pub external_id: String,
    pub task_type: String,
    pub roles: String,
    pub entity_labels: String,
    pub linked_changes: String,
    pub applicability: String,
    pub review_status: String,
    pub reviewer: String,
}

#[derive(Serialize)]
pub struct AuditRowView {
    pub from: String,
    pub to: String,
    pub unresolved: bool,
    pub kind: String,
    pub evidence: String,
    pub source: String,
    pub review_status: String,
}

/// Load the reviewed interpretations + full graph audit for the corpus.
pub fn load_corpus_relationships(
    conn: &rusqlite::Connection,
) -> Result<CorpusRelationshipsView, String> {
    use crate::catalog::review::graph_audit;
    const SOURCE_KIND: &str = "grnoc-public-task-viewer";

    let reviews = crate::catalog::store::list_ticket_reviews(conn)?;
    let mut review_rows = Vec::new();
    for r in &reviews {
        let mut task_type = String::new();
        if let Ok(Some(snap)) =
            crate::catalog::db::list_snapshots(conn, r.catalog_event_id).map(|s| s.first().cloned())
        {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&snap.normalized_json) {
                task_type = v
                    .get("task_type")
                    .and_then(|x| x.as_str())
                    .unwrap_or("")
                    .to_string();
            }
        }
        review_rows.push(ReviewRowView {
            external_id: r.external_id.clone(),
            task_type,
            roles: r.reviewed_roles.join(", "),
            entity_labels: r.entity_labels.join(", "),
            linked_changes: r.linked_change_ids.join(", "),
            applicability: r.analysis_applicability.clone(),
            review_status: r.review_status.clone(),
            reviewer: r.reviewer.clone(),
        });
    }

    let audit = graph_audit(conn, SOURCE_KIND)?;
    let unresolved = audit.iter().filter(|r| !r.to_resolved).count();
    let audit_rows = audit
        .into_iter()
        .map(|r| AuditRowView {
            from: r.from_external,
            to: r.to_external,
            unresolved: !r.to_resolved,
            kind: r.relationship_kind,
            evidence: r.evidence_kind,
            source: r.exact_source,
            review_status: r.review_status,
        })
        .collect();

    Ok(CorpusRelationshipsView {
        reviews: review_rows,
        audit: audit_rows,
        unresolved,
    })
}

#[derive(Template, Serialize)]
#[template(path = "archive_batches.html")]
pub struct ArchiveBatchesView {
    pub batches: Vec<ArchiveBatchView>,
    pub note: String,
}

#[derive(Serialize)]
pub struct ArchiveBatchView {
    pub case_study: String,
    pub batch_id: String,
    pub events: usize,
    pub unique_archives: usize,
    pub archives_avoided: usize,
    pub estimated_bytes: i64,
    pub parse_operations: usize,
    pub families: String,
    pub cohorts: Vec<String>,
}

// ── Corpus loaders ─────────────────────────────────────────────────

pub fn load_corpus(conn: &rusqlite::Connection) -> Result<CorpusView, String> {
    use crate::catalog::analyzability::{derive_analyzability, state as astate};
    let events = crate::catalog::db::list_events(conn)?;
    let grnoc: Vec<_> = events
        .iter()
        .filter(|e| e.source_kind == "grnoc-public-task-viewer")
        .collect();
    let mut snapshots = 0usize;
    let mut open = 0usize;
    let mut closed = 0usize;
    let mut by_task_type: std::collections::BTreeMap<String, usize> = Default::default();
    let mut dates: Vec<String> = Vec::new();
    let mut not_reviewed = 0usize;
    let mut ready_for_planning = 0usize;
    let mut completed = 0usize;
    let mut stale = 0usize;
    for e in &grnoc {
        let snaps = crate::catalog::db::list_snapshots(conn, e.id)?;
        snapshots += snaps.len();
        if let Some(latest) = snaps.first() {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&latest.normalized_json) {
                let start = v
                    .get("start")
                    .and_then(|x| x.as_str())
                    .unwrap_or("")
                    .to_string();
                if !start.is_empty() {
                    dates.push(start);
                }
                let end = v
                    .get("end")
                    .and_then(|x| x.as_str())
                    .unwrap_or("")
                    .to_string();
                if end.is_empty() {
                    open += 1;
                } else {
                    closed += 1;
                }
                let tt = v
                    .get("task_type")
                    .and_then(|x| x.as_str())
                    .filter(|s| !s.is_empty())
                    .unwrap_or("Unknown")
                    .to_string();
                *by_task_type.entry(tt).or_insert(0) += 1;
            }
        }
        let a = derive_analyzability(conn, e)?;
        match a.readiness.as_str() {
            astate::NOT_REVIEWED => not_reviewed += 1,
            astate::READY_FOR_ARCHIVE_PLANNING | astate::ARCHIVE_PLAN_READY => {
                ready_for_planning += 1
            }
            astate::ANALYSIS_COMPLETE => completed += 1,
            astate::ANALYSIS_STALE => stale += 1,
            _ => {}
        }
    }
    dates.sort();
    let latest_sync = crate::catalog::db::latest_sync(conn, "grnoc-public-task-viewer")?;
    let (latest_sync, sync_status, sync_failures) = match &latest_sync {
        Some(s) => (s.started_at.clone(), s.status.clone(), s.failures),
        None => (String::new(), String::new(), 0),
    };
    let policy = "default ceiling 5 requests/second; burst 2; max 5 in-flight; budget 100 requests per sync; adaptive to 429/Retry-After".to_string();
    Ok(CorpusView {
        total_events: grnoc.len(),
        source_snapshots: snapshots,
        oldest_event: dates.first().cloned().unwrap_or_default(),
        newest_event: dates.last().cloned().unwrap_or_default(),
        open_tickets: open,
        closed_tickets: closed,
        by_task_type: by_task_type.into_iter().collect(),
        not_reviewed,
        ready_for_planning,
        completed_analyses: completed,
        stale_analyses: stale,
        latest_sync,
        sync_status,
        sync_failures,
        policy: policy.to_string(),
    })
}

pub fn load_sync_runs(conn: &rusqlite::Connection) -> Result<SyncRunsView, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, started_at, status, events_examined, new_events, changed_events,
                    unchanged_events, failures
             FROM catalog_sync_runs WHERE source = ?1 ORDER BY id DESC",
        )
        .map_err(|e| format!("catalog read failed: {e}"))?;
    let rows = stmt
        .query_map(["grnoc-public-task-viewer"], |r| {
            Ok(SyncRunRowView {
                id: r.get(0)?,
                started_at: r.get(1)?,
                status: r.get(2)?,
                examined: r.get(3)?,
                new_events: r.get(4)?,
                changed: r.get(5)?,
                unchanged: r.get(6)?,
                failures: r.get(7)?,
            })
        })
        .map_err(|e| format!("catalog read failed: {e}"))?;
    let mut runs = Vec::new();
    for r in rows {
        runs.push(r.map_err(|e| format!("catalog read failed: {e}"))?);
    }
    Ok(SyncRunsView { runs })
}

/// Resolve an event by either repository or viewer identity.
fn find_event(
    conn: &rusqlite::Connection,
    external_id: &str,
) -> Option<crate::catalog::domain::CatalogEvent> {
    crate::catalog::db::get_event_by_external(conn, "local-repository", external_id)
        .ok()
        .flatten()
        .or_else(|| {
            crate::catalog::db::get_event_by_external(conn, "grnoc-public-task-viewer", external_id)
                .ok()
                .flatten()
        })
}

pub fn load_event_relationships(
    conn: &rusqlite::Connection,
    external_id: &str,
) -> Result<Option<EventRelationshipsView>, String> {
    let Some(event) = find_event(conn, external_id) else {
        return Ok(None);
    };
    let snaps = crate::catalog::db::list_snapshots(conn, event.id)?;
    let title = snaps
        .first()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s.normalized_json).ok())
        .and_then(|v| {
            v.get("title")
                .and_then(|t| t.as_str())
                .map(|s| s.to_string())
        })
        .unwrap_or_default();
    let analyzability = crate::catalog::analyzability::derive_analyzability(conn, &event)?;

    let edges = crate::catalog::store::list_relationships(conn, None)?;
    let mut outgoing = Vec::new();
    let mut incoming = Vec::new();
    let mut derived = Vec::new();
    for e in edges {
        let is_derived = e.evidence_kind
            == crate::catalog::domain::EVIDENCE_DERIVED_TEMPORAL_OVERLAP
            || e.evidence_kind == crate::catalog::domain::EVIDENCE_DERIVED_ENTITY_OVERLAP;
        let row = RelationshipRowView {
            from: event_external(conn, e.from_event_id),
            to: e.to_external_id.clone(),
            kind: e.relationship_kind.clone(),
            evidence: e.evidence_kind.clone(),
            snapshot_id: e.source_snapshot_id,
            reviewed: e.reviewed_status.clone(),
            note: e.note.clone().unwrap_or_default(),
        };
        if e.from_event_id == event.id && !is_derived {
            outgoing.push(row);
        } else if e.to_event_id == Some(event.id) && !is_derived {
            incoming.push(row);
        } else if is_derived && (e.from_event_id == event.id || e.to_event_id == Some(event.id)) {
            derived.push(row);
        }
    }

    let discoveries: Vec<DiscoveryRowView> =
        crate::catalog::store::list_discoveries(conn, "grnoc-public-task-viewer", None)?
            .into_iter()
            .filter(|d| d.external_id == event.external_id)
            .map(|d| DiscoveryRowView {
                external_id: d.external_id,
                provenance: d.provenance,
                status: d.status,
                discovered_at: d.discovered_at,
            })
            .collect();

    let fetches: Vec<FetchRowView> = crate::catalog::store::list_snapshot_fetches(conn, event.id)?
        .into_iter()
        .map(|f| FetchRowView {
            fetched_at: f.fetched_at,
            http_status: f.http_status,
            method: f.acquisition_method,
            retries: f.retry_count,
            snapshot_id: f.snapshot_id,
        })
        .collect();

    // Candidate groups containing this event.
    let mut groups = Vec::new();
    for g in crate::catalog::grouping::list_candidates(conn)? {
        if g.member_event_ids.contains(&event.id) {
            let members: Vec<String> = g
                .member_event_ids
                .iter()
                .map(|id| {
                    crate::catalog::db::get_event(conn, *id)
                        .ok()
                        .flatten()
                        .map(|e| e.external_id)
                        .unwrap_or_else(|| id.to_string())
                })
                .collect();
            groups.push(GroupRowView {
                id: g.id,
                label: g.label,
                confidence: g.confidence,
                review_status: g.review_status,
                members: members.join(", "),
            });
        }
    }

    Ok(Some(EventRelationshipsView {
        event_id: event.external_id.clone(),
        title,
        analyzability: analyzability.readiness,
        analyzability_reason: analyzability.reason,
        outgoing,
        incoming,
        derived,
        discoveries,
        groups,
        fetches,
    }))
}

fn event_external(conn: &rusqlite::Connection, event_id: i64) -> String {
    crate::catalog::db::get_event(conn, event_id)
        .ok()
        .flatten()
        .map(|e| e.external_id)
        .unwrap_or_else(|| format!("#{event_id}"))
}

pub fn load_analysis_queue(
    conn: &rusqlite::Connection,
    filters: &QueueFilters,
) -> Result<AnalysisQueueView, String> {
    use crate::catalog::analyzability::derive_all_analyzability;
    let events = crate::catalog::db::list_events(conn)?;
    let readiness: std::collections::HashMap<i64, (String, String)> =
        derive_all_analyzability(conn)?
            .into_iter()
            .map(|a| (a.event_id, (a.readiness, a.reason)))
            .collect();
    let mut rows = Vec::new();
    for e in &events {
        let snaps = crate::catalog::db::list_snapshots(conn, e.id)?;
        let Some(latest) = snaps.first() else {
            continue;
        };
        let v: serde_json::Value =
            serde_json::from_str(&latest.normalized_json).unwrap_or_default();
        let title = v
            .get("title")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string();
        let task_type = v
            .get("task_type")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string();
        let start = v
            .get("start")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string();
        let end = v
            .get("end")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string();
        let lifecycle = if end.is_empty() { "Open" } else { "Closed" }.to_string();
        let (readiness, reason) = readiness
            .get(&e.id)
            .cloned()
            .unwrap_or_else(|| (String::new(), String::new()));
        let reviewed_role = {
            let mut stmt = conn
                .prepare(
                    "SELECT reviewed_roles_json FROM ticket_reviews
                     WHERE catalog_event_id = ?1 ORDER BY id DESC LIMIT 1",
                )
                .map_err(|e| format!("catalog read failed: {e}"))?;
            let mut rows = stmt
                .query_map([e.id], |r| r.get::<_, String>(0))
                .map_err(|e| format!("catalog read failed: {e}"))?;
            match rows.next() {
                Some(Ok(roles)) => {
                    let roles: serde_json::Value = serde_json::from_str(&roles).unwrap_or_default();
                    roles
                        .as_array()
                        .map(|a| {
                            a.iter()
                                .filter_map(|v| v.as_str().map(|x| x.to_string()))
                                .collect::<Vec<_>>()
                                .join(", ")
                        })
                        .unwrap_or_default()
                }
                _ => String::new(),
            }
        };
        // Route-selection status: the reviewed plan's predicate, or a
        // reviewed-requirement statement when absent.
        let selection_status = {
            let mut stmt = conn
                .prepare(
                    "SELECT p.payload FROM analysis_plans p
                     JOIN manifest_revisions m ON m.id = p.manifest_revision_id
                     WHERE m.event_id = ?1 ORDER BY p.id DESC LIMIT 1",
                )
                .map_err(|e| format!("catalog read failed: {e}"))?;
            let mut rows = stmt
                .query_map([e.id], |r| r.get::<_, String>(0))
                .map_err(|e| format!("catalog read failed: {e}"))?;
            match rows.next() {
                Some(Ok(payload)) => {
                    let v: serde_json::Value = serde_json::from_str(&payload).unwrap_or_default();
                    let status = v
                        .get("transit_predicate_status")
                        .and_then(|x| x.as_str())
                        .unwrap_or("");
                    let origin = v
                        .get("origin_asns")
                        .and_then(|x| x.as_array())
                        .map(|a| a.len())
                        .unwrap_or(0);
                    if status == "Reviewed" && origin > 0 {
                        "reviewed plane + origin".to_string()
                    } else if origin > 0 {
                        "origin reviewed; predicate pending".to_string()
                    } else {
                        "no reviewed mapping".to_string()
                    }
                }
                _ => "no plan".to_string(),
            }
        };
        let expectation = latest_expectation(conn, e.id)?.unwrap_or_default();
        let case_studies = {
            let mut stmt = conn
                .prepare(
                    "SELECT c.slug FROM case_studies c
                     JOIN case_study_event_links l ON l.case_study_id = c.id
                     WHERE l.catalog_event_id = ?1 ORDER BY c.slug",
                )
                .map_err(|e| format!("catalog read failed: {e}"))?;
            let slugs: Vec<String> = stmt
                .query_map([e.id], |r| r.get::<_, String>(0))
                .map_err(|e| format!("catalog read failed: {e}"))?
                .flatten()
                .collect();
            slugs.join(", ")
        };
        if let Some(f) = &filters.state {
            if readiness != *f {
                continue;
            }
        }
        if let Some(f) = &filters.lifecycle {
            if lifecycle != *f {
                continue;
            }
        }
        if let Some(f) = &filters.task_type {
            if !task_type.to_lowercase().contains(&f.to_lowercase()) {
                continue;
            }
        }
        if let Some(f) = &filters.expectation {
            if !expectation.to_lowercase().contains(&f.to_lowercase()) {
                continue;
            }
        }
        if let Some(f) = &filters.case_study {
            if !case_studies.split(',').any(|s| s.trim() == f) {
                continue;
            }
        }
        if let Some(f) = &filters.date_from {
            if start < *f {
                continue;
            }
        }
        if let Some(f) = &filters.date_to {
            if !start.is_empty() && start > *f {
                continue;
            }
        }
        if let Some(f) = &filters.q {
            let hay = format!("{} {title}", e.external_id).to_lowercase();
            if !hay.contains(&f.to_lowercase()) {
                continue;
            }
        }
        // reviewed roles, archive-plan status, existing runs,
        // and the derived next analyst action.
        let review = crate::catalog::store::get_ticket_review(conn, e.id)?;
        let reviewed_roles = review
            .as_ref()
            .map(|r| r.reviewed_roles.join(", "))
            .unwrap_or_default();
        let applicability = review
            .as_ref()
            .map(|r| r.analysis_applicability.as_str())
            .unwrap_or("");
        let archive_plan_status = {
            let mut stmt = conn
                .prepare(
                    "SELECT p.status FROM analysis_plans p
                     JOIN manifest_revisions m ON m.id = p.manifest_revision_id
                     WHERE m.event_id = ?1 ORDER BY p.id DESC LIMIT 1",
                )
                .map_err(|e| format!("catalog read failed: {e}"))?;
            let status: Option<String> = stmt.query_row([e.id], |r| r.get::<_, String>(0)).ok();
            match status {
                Some(s) if s == "Ready" => "Ready".to_string(),
                Some(_) => "Draft".to_string(),
                None => "none".to_string(),
            }
        };
        let runs = crate::catalog::db::list_runs_for_event(conn, e.id)?.len();
        let next_action =
            crate::catalog::analyzability::next_analyst_action(&readiness, applicability)
                .to_string();
        rows.push(QueueRowView {
            external_id: e.external_id.clone(),
            reviewed_role: reviewed_role.clone(),
            applicability: applicability.to_string(),
            next_action: next_action.clone(),
            selection_status: selection_status.clone(),
            title,
            task_type,
            lifecycle,
            start,
            reason,
            readiness,
            expectation,
            case_studies,
            reviewed_roles,
            archive_plan_status,
            runs,
        });
    }
    rows.sort_by(|a, b| {
        b.start
            .cmp(&a.start)
            .then_with(|| a.external_id.cmp(&b.external_id))
    });
    Ok(AnalysisQueueView {
        rows,
        filters: filters.clone(),
    })
}

pub fn load_incident_candidates(
    conn: &rusqlite::Connection,
    include_temporal: bool,
) -> Result<IncidentCandidatesView, String> {
    use crate::catalog::grouping::{confidence, default_queue_candidates};
    let all = crate::catalog::grouping::list_candidates(conn)?;
    let mut explicit = 0usize;
    let mut strong = 0usize;
    let mut weak = 0usize;
    let mut coincidence = 0usize;
    let mut rejected = 0usize;
    for g in &all {
        match g.confidence.as_str() {
            c if c == confidence::EXPLICITLY_LINKED => explicit += 1,
            c if c == confidence::STRONG_CANDIDATE => strong += 1,
            c if c == confidence::WEAK_CANDIDATE => weak += 1,
            c if c == confidence::TEMPORAL_COINCIDENCE => coincidence += 1,
            c if c == confidence::REJECTED => rejected += 1,
            _ => {}
        }
    }
    // Default analyst view: temporal-only coincidence is hidden; it
    // remains queryable (?include=temporal).
    let mut shown: Vec<&crate::catalog::domain::IncidentGroupCandidate> =
        default_queue_candidates(&all);
    if include_temporal {
        shown = all.iter().collect();
    }
    let mut groups = Vec::new();
    for g in shown {
        let members: Vec<String> = g
            .member_event_ids
            .iter()
            .map(|id| event_external(conn, *id))
            .collect();
        let evidence: Vec<GroupEvidenceView> = g
            .evidence
            .iter()
            .map(|e| GroupEvidenceView {
                signal: e.signal.clone(),
                detail: e.detail.clone(),
            })
            .collect();
        groups.push(IncidentGroupView {
            id: g.id,
            label: g.label.clone(),
            confidence: g.confidence.clone(),
            review_status: g.review_status.clone(),
            members,
            evidence,
        });
    }
    Ok(IncidentCandidatesView {
        groups,
        explicit,
        strong,
        weak,
        coincidence,
        rejected,
    })
}

pub fn load_archive_batches(conn: &rusqlite::Connection) -> Result<ArchiveBatchesView, String> {
    use crate::catalog::archive_plan::{AnalysisHorizon, ArchivePlan};
    use crate::catalog::batch::{plan_batch, EventPlanInput};
    let mut batches = Vec::new();
    let mut stmt = conn
        .prepare(
            "SELECT p.id, p.horizon_json, p.plan_json, c.slug FROM case_study_analysis_plans p
             JOIN case_studies c ON c.id = p.case_study_id ORDER BY c.slug",
        )
        .map_err(|e| format!("catalog read failed: {e}"))?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
            ))
        })
        .map_err(|e| format!("catalog read failed: {e}"))?;
    for row in rows {
        let (plan_id, horizon_json, plan_json, slug) =
            row.map_err(|e| format!("catalog read failed: {e}"))?;
        let Ok(plan) = serde_json::from_str::<ArchivePlan>(&plan_json) else {
            continue;
        };
        let Ok(horizon) = serde_json::from_str::<AnalysisHorizon>(&horizon_json) else {
            continue;
        };
        let mut stmt2 = conn
            .prepare(
                "SELECT external_identifier FROM case_study_event_links
                 WHERE case_study_id = ?1 AND catalog_event_id IS NOT NULL ORDER BY sort_order",
            )
            .map_err(|e| format!("catalog read failed: {e}"))?;
        let ids: Vec<String> = stmt2
            .query_map([plan_id], |r| r.get::<_, String>(0))
            .map_err(|e| format!("catalog read failed: {e}"))?
            .flatten()
            .collect();
        if ids.is_empty() {
            continue;
        }
        let inputs: Vec<EventPlanInput> = ids
            .iter()
            .map(|id| EventPlanInput {
                event_id: id.clone(),
                horizon: horizon.clone(),
                plan: plan.clone(),
            })
            .collect();
        let batch = plan_batch(&inputs);
        batches.push(ArchiveBatchView {
            case_study: slug,
            batch_id: batch.batch_id,
            events: batch.events.len(),
            unique_archives: batch.unique_archives.len(),
            archives_avoided: batch.archives_avoided_through_reuse,
            estimated_bytes: batch.estimated_compressed_bytes,
            parse_operations: batch.expected_parse_operations,
            families: batch.source_families.join(", "),
            cohorts: batch.events.iter().map(|e| e.event_id.clone()).collect(),
        });
    }
    Ok(ArchiveBatchesView {
        batches,
        note: "Batch plans are deterministic groupings of per-event raw archive requirements; archive reuse never merges event evidence. Nothing is downloaded to produce these plans.".to_string(),
    })
}

// ── Incident workbench views  ──────────

/// Event workbench: the event's own runs through the shared view model.
pub fn load_event_workbench(
    conn: &rusqlite::Connection,
    event_id: &str,
    catalog_root: &std::path::Path,
    query: &crate::catalog::web::handlers::WorkbenchQuery,
) -> Result<Option<WorkbenchView>, String> {
    let event = db::get_event_by_external(conn, "local-repository", event_id)?.or(
        db::get_event_by_external(conn, "grnoc-public-task-viewer", event_id)?,
    );
    let Some(event) = event else { return Ok(None) };
    // Collector sites are stable reviewed facts; the pilot-scoped session
    // audit and peering-plane decision are NOT applied to unrelated events.
    let mut context = crate::catalog::workbench::WorkbenchContext::load_registry_only(
        std::path::Path::new("case-studies/manlan-2019/pilot"),
    );
    // Event-scoped ASN identities: a registry for
    // this event, when one exists, augments the shared pilot registry.
    let event_registry = std::path::Path::new("case-studies")
        .join(event_id.to_lowercase())
        .join("asn-identities.json");
    if event_registry.is_file() {
        let event_identities =
            crate::catalog::workbench::AsnIdentityRegistry::load(&event_registry);
        context.asn_identities.merge(&event_identities);
    }
    crate::catalog::workbench::WorkbenchContext::load_session_metadata(conn, &mut context);
    // Reviewed peer-session metadata for this event
    // : the canonical data file preserves the observed peer
    // ASNs when the runtime catalog lacks the backfilled rows.
    let metadata_file = std::path::Path::new("case-studies")
        .join(event_id.to_lowercase())
        .join("peer-metadata.json");
    if metadata_file.is_file() {
        if let Ok(raw) = std::fs::read_to_string(&metadata_file) {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&raw) {
                if let Some(sessions) = v.get("sessions").and_then(|x| x.as_array()) {
                    for row in sessions {
                        if let Ok(m) = serde_json::from_value::<
                            crate::catalog::domain::ObserverSessionMetadata,
                        >(row.clone())
                        {
                            context.session_metadata.push(m);
                        }
                    }
                }
            }
        }
    }
    crate::catalog::workbench::WorkbenchContext::load_relationship_audit(&mut context, event_id);
    let Some(vm) = crate::catalog::workbench::IncidentWorkbenchViewModel::for_event(
        conn,
        event_id,
        &context,
        catalog_root,
    )?
    else {
        return Ok(None);
    };
    // Fill event header fields from the catalog event row.
    let mut vm = vm;
    vm.subject_kind = event.source_kind.clone();
    vm.lifecycle = event_lifecycle(conn, event.id)?;
    // The expectation assessment comes from the first completed run's
    // assessment (model); the manifest target label is a title, not an
    // assessment, and is never rendered here.
    let title = latest_title(conn, event.id)?;
    if !title.is_empty() {
        vm.title = title;
    }
    // Source task type from the immutable snapshot (e.g. INC/CHG/TASK);
    // fixtures without a task_type field keep the source kind.
    if let Some(s) = db::list_snapshots(conn, event.id)?.first() {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&s.normalized_json) {
            if let Some(tt) = v.get("task_type").and_then(|x| x.as_str()) {
                if !tt.is_empty() {
                    vm.source_task_type = tt.to_string();
                }
            }
        }
    }
    Ok(Some(WorkbenchView::from_vm(vm, query)))
}

/// Case-study workbench: linked runs through the same shared model.
pub fn load_case_study_workbench(
    conn: &rusqlite::Connection,
    slug: &str,
    catalog_root: &std::path::Path,
    query: &crate::catalog::web::handlers::WorkbenchQuery,
) -> Result<Option<WorkbenchView>, String> {
    let Some(cs) = crate::catalog::archive_plan::find_case_study(conn, slug) else {
        return Ok(None);
    };
    let pilot_dir = std::path::Path::new("case-studies")
        .join(slug)
        .join("pilot");
    let mut context = crate::catalog::workbench::WorkbenchContext::load_from_pilot_dir(&pilot_dir);
    crate::catalog::workbench::WorkbenchContext::load_session_metadata(conn, &mut context);
    let Some(mut vm) = crate::catalog::workbench::IncidentWorkbenchViewModel::for_case_study(
        conn,
        slug,
        &context,
        catalog_root,
    )?
    else {
        return Ok(None);
    };
    vm.title = cs.title.clone();
    vm.lifecycle = if cs.end_utc.is_some() {
        "Closed"
    } else {
        "Open"
    }
    .to_string();
    // No incident-wide expectation assessment exists for a case study;
    // the model already states this.
    Ok(Some(WorkbenchView::from_vm(vm, query)))
}

fn event_lifecycle(conn: &rusqlite::Connection, event_id: i64) -> Result<String, String> {
    let snapshots = db::list_snapshots(conn, event_id)?;
    let latest = snapshots.first();
    let normalized: serde_json::Value = latest
        .and_then(|s| serde_json::from_str(&s.normalized_json).ok())
        .unwrap_or_else(|| serde_json::json!({}));
    let end = normalized.get("end").and_then(|v| v.as_str()).unwrap_or("");
    Ok(if end.is_empty() { "Open" } else { "Closed" }.to_string())
}

fn latest_title(conn: &rusqlite::Connection, event_id: i64) -> Result<String, String> {
    let snapshots = db::list_snapshots(conn, event_id)?;
    let latest = snapshots.first();
    let normalized: serde_json::Value = latest
        .and_then(|s| serde_json::from_str(&s.normalized_json).ok())
        .unwrap_or_else(|| serde_json::json!({}));
    Ok(normalized
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string())
}

/// Askama wrapper for the shared workbench view model.
#[derive(Template)]
#[template(path = "workbench.html")]
pub struct WorkbenchView {
    pub vm: crate::catalog::workbench::IncidentWorkbenchViewModel,
    pub episodes: Vec<WorkbenchEpisodeRow>,
    /// No-change rows, rendered collapsed but discoverable (Part 6).
    pub unchanged_episodes: Vec<WorkbenchEpisodeRow>,
    /// Operator-facing routing findings.
    pub findings: Vec<WorkbenchFindingRow>,
    /// Secondary findings under "Additional observer findings"
    ///  fully accessible, not on the first screen.
    pub additional_findings: Vec<WorkbenchFindingRow>,
    /// All findings in chronological model order (the Routing findings
    /// table); principal card order is by operational priority.
    pub findings_table: Vec<WorkbenchFindingRow>,
    /// True when the named relationship has no qualifying public
    /// visibility: renders the compact primary
    /// result and hides empty analysis scaffolding.
    pub no_visibility_page: bool,
    /// Compact case-study scope line for the mobile first viewport
    ///  "`<pilot label>` · `<pilot range>` UTC ·
    /// not incident-wide".
    pub compact_scope_line: String,
    /// Named target origin for the no-visibility eligibility text
    ///  "RIPE" + 3333 from reviewed data.
    pub origin_label: String,
    pub origin_asn: String,
    /// Concrete per-region observer comparison (Part 9).
    pub region_comparison: Vec<WorkbenchRegionComparisonRow>,
    pub breadth: Vec<WorkbenchBreadthRow>,
    pub timeline: Vec<WorkbenchTimelineRow>,
    pub timeline_svg: String,
    pub anchors: Vec<WorkbenchAnchorRow>,
    pub coverage: Vec<WorkbenchCoverageRow>,
    pub cues: Vec<crate::catalog::workbench::InvestigationCue>,
    pub grouped_cues: Vec<crate::catalog::workbench::GroupedCue>,
    pub runs: Vec<crate::catalog::workbench::WorkbenchRunView>,
    /// No-change observer statements: short
    /// per-session sentences, e.g. "RRC00 in Amsterdam saw no
    /// route-state change for the selected prefixes."
    pub no_change_statements: Vec<String>,
    /// Denominator line: "10 eligible observer sessions: 8 changed,
    /// 2 unchanged, 1 no qualifying baseline."
    pub denominator: String,
    /// Header human ranges (Part 9): exact ISO stays in the model/API.
    pub header_incident_range: String,
    pub header_pilot_range: String,
    /// Linked-ticket count for the compact header.
    pub linked_ticket_count: usize,
    /// Active filter state (for filter links and title).
    pub filters: WorkbenchFilters,
    /// Per-request performance measurement (Part 13). All timings are
    /// wall-clock; the model is deterministic regardless of timing.
    pub timing: WorkbenchTiming,
    /// Focus mode: timeline (episode table collapsed).
    pub timeline_focus: bool,
}

/// Active workbench filter state (Part 6). Filter values are plain
/// strings so the template can compare them with literals.
#[derive(Debug, Clone, Default, Serialize)]
pub struct WorkbenchFilters {
    pub changed: bool,
    pub region: String,
    pub rel: String,
    pub kind: String,
    /// Render every episode detail open (?expand=1).
    pub expand_all: bool,
    /// Opened episode index (server-rendered drill-down, Part 8).
    pub episode: Option<usize>,
    pub prefixes: Option<usize>,
}

impl WorkbenchFilters {
    fn from_query(q: &crate::catalog::web::handlers::WorkbenchQuery) -> Self {
        WorkbenchFilters {
            changed: q.changed,
            region: q.region.clone().unwrap_or_default(),
            rel: q.rel.clone().unwrap_or_default(),
            kind: q.kind.clone().unwrap_or_default(),
            episode: q.episode,
            prefixes: q.prefixes,
            expand_all: false,
        }
    }

    /// Whether a filter (other than drill-down) is active.
    pub fn active(&self) -> bool {
        self.changed || !self.region.is_empty() || !self.rel.is_empty() || !self.kind.is_empty()
    }
}

/// Performance measurement for one workbench request.
///
/// The workbench performs NO BGP parsing and NO archive reads on the
/// request path: it reads catalog tables (indexed), reviewed data files,
/// and immutable report artifacts only. `sql_query_count` is bounded and
/// asserted by tests.
#[derive(Serialize, Default)]
pub struct WorkbenchTiming {
    pub sql_query_count: usize,
    pub db_time_ms: f64,
    pub model_time_ms: f64,
    pub render_time_ms: f64,
    pub response_size_bytes: usize,
}

impl WorkbenchView {
    fn from_vm(
        vm: crate::catalog::workbench::IncidentWorkbenchViewModel,
        filters: &crate::catalog::web::handlers::WorkbenchQuery,
    ) -> Self {
        let start = std::time::Instant::now();
        let mut f = WorkbenchFilters::from_query(filters);
        f.expand_all = filters.expand;
        let (episodes, unchanged, denominator) = episode_rows(&vm, &f);
        let mut findings_table = finding_rows(&vm, &f);
        // finding_rows returns selector priority order (principal
        // first, then additional). Record it, then re-sort the table
        // chronologically (exact ISO) while keeping the principal
        // cards in operational-priority order.
        let priority: std::collections::HashMap<String, usize> = findings_table
            .iter()
            .enumerate()
            .map(|(i, r)| (r.stable_id.clone(), i))
            .collect();
        findings_table.sort_by(|a, b| {
            a.first_exact
                .cmp(&b.first_exact)
                .then_with(|| a.stable_id.cmp(&b.stable_id))
        });
        let (mut findings, additional_findings): (Vec<_>, Vec<_>) = findings_table
            .clone()
            .into_iter()
            .partition(|r| r.principal);
        findings.sort_by_key(|r| priority.get(&r.stable_id).copied().unwrap_or(usize::MAX));
        let no_visibility_page = findings.is_empty() && vm.relationship_audit.is_some();
        let region_comparison = region_comparison_rows(&vm, &f);

        let no_change_statements = no_change_statements(&vm);
        let breadth = breadth_rows(&vm);
        let timeline = timeline_rows(&vm, &f);
        let timeline_svg =
            crate::catalog::workbench::render_timeline_svg(&vm.timeline, &vm.operator_anchors);
        let anchors = anchor_rows(&vm);
        let coverage = coverage_rows(&vm);
        let cues = vm.cues.clone();
        let grouped_cues = vm.grouped_cues.clone();
        // Run history is a secondary provenance section, but timestamps
        // still render timezone-explicit (Part 4).
        let runs: Vec<crate::catalog::workbench::WorkbenchRunView> = vm
            .runs
            .iter()
            .map(|r| {
                let mut r = r.clone();
                if !r.completed_at.is_empty() {
                    r.completed_at = crate::catalog::workbench::workbench_time(
                        &r.completed_at,
                        &vm.window_start,
                    );
                }
                r
            })
            .collect();
        let render_time_ms = start.elapsed().as_secs_f64() * 1000.0;
        WorkbenchView {
            episodes,
            unchanged_episodes: unchanged,
            findings,
            additional_findings,
            findings_table,
            no_visibility_page,
            region_comparison,
            breadth,
            timeline,
            timeline_svg,
            anchors,
            coverage,
            cues,
            grouped_cues,
            runs,
            no_change_statements,
            denominator,
            header_incident_range: if vm.incident_horizon_start.is_empty() {
                String::new()
            } else {
                human_date_range(&vm.incident_horizon_start, &vm.incident_horizon_end)
            },
            header_pilot_range: human_pilot_range(&vm.window_start, &vm.window_end),
            origin_label: crate::catalog::workbench::target_name_from_label(&vm.target_label),
            origin_asn: vm
                .target_origin_asns
                .first()
                .map(|a| a.to_string())
                .unwrap_or_default(),
            compact_scope_line: if vm.subject_kind == "case-study" && !vm.pilot_label.is_empty() {
                format!(
                    "{} · {} · not incident-wide",
                    vm.pilot_label,
                    human_pilot_range(&vm.window_start, &vm.window_end)
                )
            } else {
                String::new()
            },
            linked_ticket_count: vm.linked_tickets.len(),
            filters: f,
            timing: WorkbenchTiming {
                sql_query_count: 0,
                db_time_ms: 0.0,
                model_time_ms: 0.0,
                render_time_ms,
                response_size_bytes: 0,
            },
            timeline_focus: filters.view.as_deref() == Some("timeline"),
            vm,
        }
    }
}

/// One run's observer episodes (API slice; same presentation model).
pub fn load_run_workbench_slice(
    conn: &rusqlite::Connection,
    run_id: i64,
) -> Result<Option<serde_json::Value>, String> {
    let run = db::get_run(conn, run_id)?;
    let Some(_run) = run else { return Ok(None) };
    let evidence = crate::catalog::workbench::RunEvidence::load(conn, &[run_id])?;
    let registry = crate::catalog::netprofile::CollectorLocationRegistry::default();
    let streams: Vec<crate::catalog::domain::StreamLifecycleSummary> = evidence.streams;
    let transitions: Vec<crate::catalog::domain::RunTransitionRecord> = evidence.transitions;
    let peers = std::collections::BTreeMap::new();
    let mut eps = crate::catalog::workbench::build_episodes(
        run_id,
        "",
        &streams,
        &transitions,
        &registry,
        &peers,
        "no reviewed plane",
    );
    for ep in eps.iter_mut() {
        ep.representative_evidence = crate::catalog::workbench::render_episode_sentence(ep);
    }
    Ok(Some(serde_json::to_value(eps).unwrap_or_default()))
}

/// One run's regional breadth (API slice; same presentation model).
pub fn load_run_breadth_slice(
    conn: &rusqlite::Connection,
    run_id: i64,
) -> Result<Option<serde_json::Value>, String> {
    let run = db::get_run(conn, run_id)?;
    let Some(_run) = run else { return Ok(None) };
    let evidence = crate::catalog::workbench::RunEvidence::load(conn, &[run_id])?;
    let registry = crate::catalog::netprofile::CollectorLocationRegistry::default();
    let peers = std::collections::BTreeMap::new();
    let eps = crate::catalog::workbench::build_episodes(
        run_id,
        "",
        &evidence.streams,
        &evidence.transitions,
        &registry,
        &peers,
        "no reviewed plane",
    );
    let breadth = crate::catalog::workbench::regional_breadth(&eps, &[], &[]);
    Ok(Some(serde_json::to_value(breadth).unwrap_or_default()))
}

/// Pre-rendered episode row for the workbench template (Part 6).
///
/// All presentation logic (human labels, HH:MM:SS times, combined
/// PEER+VIEW cell, grouped signatures) lives here — the template only
/// places strings. Exact timestamps and raw labels remain available for
/// the expanded drill-down.
#[derive(Serialize)]
pub struct WorkbenchEpisodeRow {
    pub first: String,
    pub region: String,
    pub observer: String,
    pub peer_view: String,
    pub change: String,
    pub streams: String,
    pub prefixes: String,
    pub restored: String,
    pub end_state: String,
    pub end_state_class: String,
    pub cooldown: String,
    pub changed: bool,
    pub session: String,
    pub sentence: String,
    pub family: String,
    pub relationship: String,
    pub plane: String,
    pub baseline_plane_state: String,
    pub changed_plane_state: String,
    pub first_exact: String,
    pub last_exact: String,
    pub restoration_exact: String,
    pub baseline_streams: String,
    pub route_instances: String,
    pub unresolved: String,
    pub signatures: Vec<WorkbenchSignatureRow>,
    pub evidence_refs: String,
    pub expanded: bool,
    pub prefixes_open: bool,
    pub stream_rows: Vec<WorkbenchStreamRow>,
}

/// One grouped prefix signature (category → human label + prefixes).
#[derive(Serialize)]
pub struct WorkbenchSignatureRow {
    pub category: String,
    pub human: String,
    pub count: usize,
    pub prefixes: String,
}

#[derive(Serialize)]
pub struct WorkbenchStreamRow {
    pub prefix: String,
    pub baseline: String,
    pub change: String,
    pub first: String,
    pub restoration: String,
    pub end_state: String,
    pub evidence: String,
}

/// Convert shared-model episodes into pre-rendered template rows,
/// applying the active filters (Part 6). Returns the rows and the
/// denominator line. Changed episodes sort first (model already orders
/// them); unchanged/no-baseline rows are discoverable below.
fn episode_rows(
    vm: &crate::catalog::workbench::IncidentWorkbenchViewModel,
    f: &WorkbenchFilters,
) -> (Vec<WorkbenchEpisodeRow>, Vec<WorkbenchEpisodeRow>, String) {
    let _window = &vm.window_start;
    let all: Vec<WorkbenchEpisodeRow> = vm
        .episodes
        .iter()
        .enumerate()
        .filter(|(_, e)| matches_episode_filter(e, f))
        .map(|(idx, e)| episode_row(vm, e, idx, f))
        .collect();
    let total_changed = vm
        .episodes
        .iter()
        .filter(|e| e.effect_kind != crate::catalog::workbench::EffectKind::NoRouteStateChange)
        .count();
    let total_unchanged = vm
        .episodes
        .iter()
        .filter(|e| e.effect_kind == crate::catalog::workbench::EffectKind::NoRouteStateChange)
        .count();
    let no_baseline = vm.no_baseline_sessions.len();
    let incomplete = vm.incomplete_sessions.len();
    let eligible = total_changed + total_unchanged;
    let mut denominator = format!(
        "{eligible} eligible observer session{}: {total_changed} changed, {total_unchanged} unchanged",
        if eligible == 1 { "" } else { "s" }
    );
    if no_baseline > 0 {
        denominator.push_str(&format!(
            ", {no_baseline} no qualifying baseline{}",
            if no_baseline == 1 { "" } else { "s" }
        ));
    }
    if incomplete > 0 {
        denominator.push_str(&format!(", {incomplete} incomplete coverage"));
    }
    denominator.push('.');
    // Split changed and unchanged lists. The unchanged rows stay
    // discoverable (collapsed section) and their denominator remains
    // visible on the page (Part 6).
    let (mut changed_rows, mut unchanged_rows): (Vec<_>, Vec<_>) =
        all.into_iter().partition(|r| r.changed);
    if f.changed {
        unchanged_rows.clear();
    }
    if !f.kind.is_empty() {
        if f.kind == "unchanged" {
            changed_rows.clear();
        } else {
            unchanged_rows.clear();
        }
    }
    (changed_rows, unchanged_rows, denominator)
}

fn matches_episode_filter(
    e: &crate::catalog::workbench::ObserverEpisode,
    f: &WorkbenchFilters,
) -> bool {
    if !f.kind.is_empty() && effect_slug(&e.effect_kind) != f.kind {
        return false;
    }
    if f.changed && e.effect_kind == crate::catalog::workbench::EffectKind::NoRouteStateChange {
        return false;
    }
    if !f.region.is_empty() && e.observer_region != f.region {
        return false;
    }
    if !f.rel.is_empty() && e.relationship.label().to_lowercase() != f.rel {
        return false;
    }
    true
}

/// Human slug for effect-kind filters.
fn effect_slug(kind: &crate::catalog::workbench::EffectKind) -> &'static str {
    use crate::catalog::workbench::EffectKind as K;
    match kind {
        K::TemporaryStreamAbsence => "absent",
        K::RouteWithdrawal => "withdrawn",
        K::PathReplacement => "path",
        K::NamedPlaneDeparture | K::NamedPlaneReturn => "plane",
        K::PrependChange => "prepend",
        K::MixedRouteChange => "mixed",
        K::NoRouteStateChange => "unchanged",
    }
}

/// Filter a routing finding: the same dimensions as the
/// episode filters, mapped onto the finding's effect vocabulary.
fn matches_finding_filter(
    f: &crate::catalog::workbench::RoutingFinding,
    wf: &WorkbenchFilters,
) -> bool {
    use crate::catalog::workbench::RoutingEffect as RE;
    let slug = match f.effect {
        RE::PrefixesTemporarilyAbsent => "absent",
        RE::PrefixesWithdrawn => "withdrawn",
        RE::AsPathChanged => "path",
        RE::PrependingChanged => "prepend",
        RE::NamedPlaneDeparted | RE::NamedPlaneReturned => "plane",
        RE::MixedChange => "mixed",
        RE::VisibilityRestored | RE::BaselinePathRestored => "path",
    };
    if !wf.kind.is_empty() && slug != wf.kind {
        return false;
    }
    if !wf.region.is_empty() && f.observer_region != wf.region {
        return false;
    }
    if !wf.rel.is_empty() && f.relationship.label().to_lowercase() != wf.rel {
        return false;
    }
    true
}

/// Human peer-identity label from OBSERVED peer ASNs (Part 5).
///
/// The observed ASN is a protocol fact and renders even without a
/// reviewed organization label; conflicting observations render as
/// ambiguous instead of silently choosing one.
pub fn peer_identity_label(observed_asns: Vec<u32>) -> String {
    let mut asns = observed_asns;
    asns.sort_unstable();
    asns.dedup();
    match asns.len() {
        0 => "peer ASN not observed in source evidence".to_string(),
        1 => format!(
            "AS{} · organization unclassified · role unclassified",
            asns[0]
        ),
        _ => {
            let list: Vec<String> = asns.iter().map(|a| format!("AS{a}")).collect();
            format!("peer ASN ambiguous ({})", list.join(" / "))
        }
    }
}

fn episode_row(
    vm: &crate::catalog::workbench::IncidentWorkbenchViewModel,
    e: &crate::catalog::workbench::ObserverEpisode,
    idx: usize,
    f: &WorkbenchFilters,
) -> WorkbenchEpisodeRow {
    use crate::catalog::workbench::{EffectKind as K, EndState as ES};
    let window = &vm.window_start;
    let changed = e.effect_kind != K::NoRouteStateChange;
    let collector =
        crate::catalog::workbench::collector_from_session(&e.observer_session).to_string();
    let observer = if e.observer_site == collector {
        collector.clone()
    } else {
        format!("{collector} · {}", e.observer_site)
    };
    let relationship = e.relationship.label().to_lowercase();
    // Observed peer ASN (protocol fact from RIB evidence, Part 5) takes
    // precedence; reviewed organization labels are separate concepts.
    let (peer, org_role) = if !e.observed_peer_asns.is_empty() {
        let label = peer_identity_label(e.observed_peer_asns.clone());
        if label.starts_with("peer ASN ambiguous") {
            (label, String::new())
        } else {
            // label = "ASn · organization unclassified · role unclassified"
            let mut parts = label.splitn(2, " · ");
            (
                parts.next().unwrap_or(&label).to_string(),
                parts.next().unwrap_or("").to_string(),
            )
        }
    } else {
        match e.peer_asn {
            Some(asn) => (format!("AS{asn}"), String::new()),
            None => (
                "peer ASN not observed in source evidence".to_string(),
                String::new(),
            ),
        }
    };
    let peer_view = if org_role.is_empty() {
        format!("{peer} · {relationship} {}", e.named_path_plane)
    } else {
        format!(
            "{peer} · {org_role} · {relationship} {}",
            e.named_path_plane
        )
    };
    let restored = if changed {
        match (&e.restoration_start, &e.restoration_end) {
            (Some(a), Some(b)) if a != b => {
                format!("{}–{}", wb_time(a, window), wb_time(b, window))
            }
            (Some(a), _) => wb_time(a, window),
            _ => "—".to_string(),
        }
    } else {
        "—".to_string()
    };
    use crate::catalog::workbench::CooldownOutcome as CO;
    let cooldown = match &e.cooldown_outcome {
        CO::None => "—".to_string(),
        CO::RestoredAt(t) => format!("Restored {} in cooldown", wb_time(t, window)),
        CO::StillChangingBeforeAnalysisEnd(t) => {
            format!(
                "Still changing at {}; no restoration before analysis end",
                wb_time(t, window)
            )
        }
        CO::NoRestorationBeforeAnalysisEnd(end) => {
            format!(
                "No restoration before analysis end ({})",
                wb_time(end, window)
            )
        }
    };
    let (end_state, end_state_class) = match e.end_state {
        ES::BaselineRestored => ("Exact baseline restored".to_string(), "wb-end-restored"),
        ES::VisibilityRestored => (
            "Visibility restored on changed path".to_string(),
            "wb-end-restored",
        ),
        ES::StillChangedAtWindowEnd => {
            ("Still changed at window end".to_string(), "wb-end-changed")
        }
        ES::AbsentAtWindowEnd => ("Absent at window end".to_string(), "wb-end-changed"),
        ES::NoRouteStateChange => ("No route-state change".to_string(), "wb-end-plain"),
        ES::Unresolved => ("Unresolved".to_string(), "wb-end-unresolved"),
    };

    // Baseline and changed path-plane state for the expanded view.
    let baseline_plane_state = format!(
        "{} baseline stream{} via {}",
        e.baseline_stream_count,
        if e.baseline_stream_count == 1 {
            ""
        } else {
            "s"
        },
        e.named_path_plane
    );
    let mut changed_parts: Vec<String> = Vec::new();
    let mut by_cat: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
    for s in &e.streams {
        *by_cat.entry(s.category.as_str()).or_default() += 1;
    }
    for (cat, count) in &by_cat {
        changed_parts.push(format!(
            "{count} {}",
            stream_category_human(cat, &e.named_path_plane)
        ));
    }
    let changed_plane_state = if changed_parts.is_empty() {
        "no route-state change".to_string()
    } else {
        changed_parts.join("; ")
    };

    // Grouped prefix signatures (category → prefix list).
    let mut signatures: Vec<WorkbenchSignatureRow> = Vec::new();
    for (cat, count) in &by_cat {
        let prefixes: Vec<&str> = e
            .streams
            .iter()
            .filter(|s| s.category == *cat)
            .map(|s| s.prefix.as_str())
            .collect();
        signatures.push(WorkbenchSignatureRow {
            category: cat.to_string(),
            human: stream_category_human(cat, &e.named_path_plane),
            count: *count,
            prefixes: prefixes.join(", "),
        });
    }

    // Representative evidence references (unique, bounded).
    let mut refs: Vec<String> = Vec::new();
    for s in &e.streams {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&s.evidence_refs) {
            if let Some(items) = v.as_array() {
                for it in items {
                    if let Some(r) = it.as_str() {
                        if !refs.contains(&r.to_string()) && refs.len() < 6 {
                            refs.push(r.to_string());
                        }
                    }
                }
            }
        }
    }

    let streams: Vec<WorkbenchStreamRow> = e
        .streams
        .iter()
        .map(|s| {
            let first = s
                .first_change_utc
                .as_ref()
                .map(|t| wb_time(t, window))
                .unwrap_or_else(|| "—".to_string());
            let restoration = s
                .restoration_time_utc
                .as_ref()
                .map(|t| wb_time(t, window))
                .unwrap_or_else(|| "—".to_string());
            let end_state = stream_end_state_human(s);
            WorkbenchStreamRow {
                prefix: s.prefix.clone(),
                baseline: format!("via {}", e.named_path_plane),
                change: stream_category_human(&s.category, &e.named_path_plane),
                first,
                restoration,
                end_state,
                evidence: if s.evidence_refs.is_empty() || s.evidence_refs == "[]" {
                    "—".to_string()
                } else {
                    s.evidence_refs.clone()
                },
            }
        })
        .collect();

    WorkbenchEpisodeRow {
        first: e
            .first_change
            .as_ref()
            .map(|t| wb_time(t, window))
            .unwrap_or_else(|| "—".to_string()),
        region: e.observer_region.clone(),
        observer,
        peer_view,
        change: e.effect_kind.human_label().to_string(),
        streams: e.changed_stream_count.to_string(),
        prefixes: e.distinct_prefix_count.to_string(),
        restored,
        end_state,
        end_state_class: end_state_class.to_string(),
        cooldown,
        changed,
        session: e.observer_session.clone(),
        sentence: e.representative_evidence.clone(),
        family: crate::catalog::workbench::family_from_session(&e.observer_session).to_string(),
        relationship,
        plane: e.named_path_plane.clone(),
        baseline_plane_state,
        changed_plane_state,
        first_exact: e.first_change.clone().unwrap_or_default(),
        last_exact: e.last_change.clone().unwrap_or_default(),
        restoration_exact: e
            .restoration_end
            .clone()
            .or_else(|| e.restoration_start.clone())
            .unwrap_or_default(),
        baseline_streams: e.baseline_stream_count.to_string(),
        route_instances: e.route_instance_count.to_string(),
        unresolved: e.unresolved_count.to_string(),
        signatures,
        evidence_refs: refs.join(", "),
        // The prefix drill-down is nested inside the episode details,
        // so opening prefixes opens the parent episode too; ?expand=1
        // opens every episode.
        expanded: f.episode == Some(idx) || f.prefixes == Some(idx) || f.expand_all,
        prefixes_open: f.prefixes == Some(idx),
        stream_rows: streams,
    }
}

/// Human label for one stream category (Part 1.8 / 8).
fn stream_category_human(category: &str, plane: &str) -> String {
    match category {
        "Withdrawn" => "withdrawn (absent)".to_string(),
        "DepartedTransitPath" => format!("departed the {plane} path"),
        "PathChangedStillViaTransit" => format!("path changed via {plane}"),
        "PrependOnly" => "AS-path prepending changed".to_string(),
        "ReturnedToTransitPath" => format!("returned to the {plane} path"),
        "Unchanged" => "no change".to_string(),
        other => other.to_string(),
    }
}

/// Per-stream end state at the analysis-window end (Part 8).
fn stream_end_state_human(s: &crate::catalog::workbench::EpisodeStream) -> String {
    if s.withdrawn && !s.restored {
        return "Absent at window end".to_string();
    }
    if s.withdrawn && s.restored {
        return "Visibility restored".to_string();
    }
    if s.restoration_time_utc.is_some() {
        return "Exact baseline restored".to_string();
    }
    if s.category != "Unchanged" {
        return "Still changed at window end".to_string();
    }
    "No change".to_string()
}

/// Earlier-change link data: the separate
/// related finding at the same session.
#[derive(Debug, Clone, Serialize)]
pub struct WorkbenchEarlierChange {
    pub stable_id: String,
    pub label: String,
}

/// One operator-facing routing finding row.
#[derive(Debug, Clone, Serialize)]
pub struct WorkbenchFindingRow {
    pub stable_id: String,
    /// HH:MM:SS UTC (exact ISO in `first_exact` and the JSON API).
    pub time: String,
    /// "RRC15 · São Paulo" (region in `region`).
    pub observer: String,
    pub region: String,
    /// "RNP (AS1916)" or "AS1916 — name not reviewed"; ambiguity and
    /// peer-IP-only fallbacks are explicit.
    pub peer: String,
    pub peer_ip: String,
    pub relationship: String,
    /// "11 prefixes" (exact list in `exact_prefixes`); single-prefix
    /// findings label with the exact prefix itself.
    pub prefixes: String,
    pub prefix_count: usize,
    /// Principal story: selected for operational
    /// meaning; secondary findings render under "Additional observer
    /// findings".
    pub principal: bool,
    /// Observed route change label (effect vocabulary, Part 4).
    pub change: String,
    /// Compact before-path signature (space-joined ASN sequence).
    pub before: String,
    /// Compact after-path signature, or "absent" for pure absences.
    pub after: String,
    /// Exact numeric paths (authoritative; names never replace them).
    pub numeric_before: String,
    pub numeric_after: String,
    /// Named path segments with textual inserted/removed markers
    ///
    pub named_before: Vec<PathSegmentRow>,
    pub named_after: Vec<PathSegmentRow>,
    /// Factual semantic explanation of the before/after pair
    ///  — never causation.
    pub path_explanation: String,
    /// Outcome text: restoration(s) or "still changed" state.
    pub outcome: String,
    /// Compact two-line route meaning: the
    /// default card body; temporally qualified for absences.
    pub compact_meaning: String,
    /// Restoration/final-state summary line:
    /// the exact-baseline reappearance and the FINAL observed state,
    /// never conflated.
    pub final_state_line: String,
    /// Ordered route chronology , rendered in
    /// the Route sequence expansion.
    pub chronology: crate::catalog::workbench::RouteChronology,
    /// Per-distinct-ASN identity notes: rendered
    /// once per ASN under the identity expansion.
    pub identity_notes: Vec<IdentityNoteRow>,
    /// One concise operational statement (Part 4).
    pub statement: String,
    pub exact_prefixes: Vec<String>,
    /// Prefix preview: up to three prefixes plus
    /// the hidden remainder count; the full list stays one action away.
    pub preview_prefixes: Vec<String>,
    pub hidden_prefix_count: usize,
    /// Copy payloads (progressive enhancement only; the exact data is
    /// visible without JavaScript). Each line is one prefix/path.
    pub copy_prefixes: String,
    pub copy_before_paths: String,
    pub copy_after_paths: String,
    /// Exact per-prefix rows for the drill-down (Part 5).
    pub stream_rows: Vec<WorkbenchFindingStreamRow>,
    pub evidence_refs: Vec<String>,
    pub scope_limit: String,
    /// Earlier-change link.
    pub earlier_change: Option<WorkbenchEarlierChange>,
    /// Exact ISO timestamps (details/API only).
    pub first_exact: String,
    pub visibility_restored_exact: String,
    pub baseline_restored_exact: String,
    pub expanded: bool,
    pub prefixes_open: bool,
}

/// One named path segment with a textual diff marker
/// : `same`, `ins` (appeared), or `del` (removed). Markers are
/// textual classes, never color alone.
#[derive(Debug, Clone, Serialize)]
pub struct PathSegmentRow {
    pub asn: u32,
    pub label: String,
    pub mark: &'static str,
}

/// One distinct ASN's identity note: rendered
/// once per ASN, never once per occurrence.
#[derive(Debug, Clone, Serialize)]
pub struct IdentityNoteRow {
    pub asn: u32,
    /// Primary label for historical paths: reviewed name+ASN, or the
    /// bare ASN when the identity is current-only or unresolved.
    pub label: String,
    /// Current registry identity (CurrentIdentityOnly), for the note.
    pub current_identity: Option<String>,
    /// Provenance note text.
    pub note: String,
}

/// One prefix row of a finding drill-down (exact paths, Part 5).
#[derive(Debug, Clone, Serialize)]
pub struct WorkbenchFindingStreamRow {
    pub prefix: String,
    /// Exact baseline AS path (ASN sequence, uncollapsed).
    pub baseline: String,
    /// Exact changed AS path, or "absent" for a withdrawal.
    pub after: String,
    /// Exact final observed AS path, or "—".
    pub final_path: String,
    pub first: String,
    pub vis_restored: String,
    pub base_restored: String,
    pub evidence: String,
}

/// Short per-session statements for observers with no route-state
/// change: "RRC00 in Amsterdam saw no route-state
/// change for the selected prefixes." No-change evidence only — never
/// presented as proof about the named relationship.
fn no_change_statements(vm: &crate::catalog::workbench::IncidentWorkbenchViewModel) -> Vec<String> {
    let mut statements: Vec<String> = Vec::new();
    for e in &vm.episodes {
        if e.effect_kind != crate::catalog::workbench::EffectKind::NoRouteStateChange {
            continue;
        }
        let collector = crate::catalog::workbench::collector_from_session(&e.observer_session);
        statements.push(format!(
            "{} in {} saw no route-state change for the selected prefixes.",
            collector, e.observer_site
        ));
    }
    statements.sort();
    statements.dedup();
    statements
}

/// One region of the observer comparison:
/// concrete per-site statements first, ratio as compact metadata.
#[derive(Debug, Clone, Serialize)]
pub struct WorkbenchRegionComparisonRow {
    pub region: String,
    /// Concrete finding statements for this region's observer sites.
    pub statements: Vec<String>,
    /// Compact coverage ratio, e.g. "2/2" (secondary metadata).
    pub ratio: String,
}

fn finding_rows(
    vm: &crate::catalog::workbench::IncidentWorkbenchViewModel,
    f: &WorkbenchFilters,
) -> Vec<WorkbenchFindingRow> {
    use crate::catalog::workbench::{select_principal_findings, RoutingEffect as RE};
    let window = &vm.window_start;
    let event_date = crate::catalog::workbench::window_date(&vm.window_start);
    // Principal stories first ; secondary findings
    // remain fully accessible under "Additional observer findings".
    let (principal, additional) = select_principal_findings(vm.findings.clone(), &vm.plane_asns, 4);
    let ordered: Vec<&crate::catalog::workbench::RoutingFinding> = principal
        .iter()
        .chain(additional.iter())
        .filter(|finding| matches_finding_filter(finding, f))
        .collect();
    let principal_ids: std::collections::BTreeSet<&str> =
        principal.iter().map(|f| f.stable_id.as_str()).collect();
    ordered
        .iter()
        .enumerate()
        .map(|(idx, finding)| {
            let peer = match finding.peer_asn {
                Some(asn) => {
                    if finding.peer_asn_ambiguous {
                        format!("peer ASN ambiguous (observed {})", finding.peer_ip)
                    } else if finding.peer_name == "name not reviewed" {
                        format!("AS{asn} — name not reviewed")
                    } else {
                        format!("{} (AS{asn})", finding.peer_name)
                    }
                }
                None => "peer ASN not observed in source evidence".to_string(),
            };
            let relationship = finding.relationship.label().to_lowercase();
            let before = if finding.baseline_path_signature == "—" {
                "—".to_string()
            } else {
                finding.baseline_path_signature.clone()
            };
            // Pure absences have no changed path: render the absence.
            let after = match (&finding.changed_path_signature, finding.effect) {
                (s, _) if s != "—" => s.clone(),
                (_, RE::PrefixesTemporarilyAbsent | RE::PrefixesWithdrawn) => "absent".to_string(),
                _ => "—".to_string(),
            };
            let outcome = finding_outcome(finding);
            let time = finding
                .first_observed
                .as_deref()
                .map(|t| wb_time(t, window))
                .unwrap_or_else(|| "—".to_string());
            // Exact numeric paths from the most frequent member paths.
            let (baseline_exact, changed_exact) = finding_path_pair(finding);
            // Identity policy: primary historical
            // path rendering uses reviewed names or bare ASNs; current
            // registry identities appear only in the identity notes.
            let identity_at = |asn: u32| -> Option<&crate::catalog::workbench::AsnIdentity> {
                vm.asn_identities
                    .iter()
                    .find(|i| i.asn == asn && i.valid_at(&event_date) && i.has_display_name())
            };
            let name_for = |asn: u32| -> String {
                match identity_at(asn) {
                    Some(i) if i.review_status == "HistoricallyReviewed" => {
                        format!("{} (AS{})", i.display_name, asn)
                    }
                    Some(i) => format!("AS{asn} — current identity: {}", i.display_name),
                    None => format!("AS{asn}"),
                }
            };
            let plane = vm
                .plane_asns
                .first()
                .map(|_| finding.named_path_plane.clone())
                .unwrap_or_default();
            let diff = crate::catalog::workbench::diff_paths(
                &baseline_exact,
                &changed_exact,
                &vm.plane_asns,
            );
            let segment_name = |asn: u32| -> String {
                match identity_at(asn) {
                    Some(i) if i.review_status == "HistoricallyReviewed" => {
                        format!("{} (AS{})", i.display_name, asn)
                    }
                    _ => format!("AS{asn}"),
                }
            };
            let named_before = named_segments(&baseline_exact, &diff, false, &segment_name);
            let named_after = named_segments(&changed_exact, &diff, true, &segment_name);
            let path_explanation = crate::catalog::workbench::explain_path_diff_with_origins(
                &baseline_exact,
                &changed_exact,
                &vm.plane_asns,
                &plane,
                &finding.target_origin_asns,
                &|asn| name_for(asn),
            );
            let preview: Vec<String> = finding.exact_prefixes.iter().take(3).cloned().collect();
            let hidden = finding.exact_prefixes.len().saturating_sub(preview.len());
            // Compact route meaning + final-state line (Part 4).
            let compact_meaning = compact_finding_meaning(
                finding,
                &baseline_exact,
                &changed_exact,
                &diff,
                &plane,
                &vm.plane_asns,
                &|asn| name_for(asn),
            );
            let final_state_line = finding_final_state_line(finding);
            let chronology = crate::catalog::workbench::route_chronology(finding, &vm.window_end);
            // Per-distinct-ASN identity notes (Part 6): once per ASN
            // across the baseline + changed paths.
            let mut identity_asns: Vec<u32> = baseline_exact
                .iter()
                .chain(changed_exact.iter())
                .copied()
                .collect();
            identity_asns.sort_unstable();
            identity_asns.dedup();
            let identity_notes: Vec<IdentityNoteRow> = identity_asns
                .into_iter()
                .map(|asn| {
                    let label = match identity_at(asn) {
                        Some(i) if i.review_status == "HistoricallyReviewed" => {
                            format!("{} (AS{})", i.display_name, asn)
                        }
                        _ => format!("AS{asn}"),
                    };
                    let current_identity = identity_at(asn)
                        .filter(|i| i.review_status == "CurrentIdentityOnly")
                        .map(|i| i.display_name.clone());
                    let note = match identity_at(asn) {
                        Some(i) if i.review_status == "HistoricallyReviewed" => {
                            format!("Reviewed identity ({})", i.provenance)
                        }
                        Some(i) if i.review_status == "CurrentIdentityOnly" => format!(
                            "Current registry identity: {}; historical {} identity not reviewed",
                            i.display_name,
                            finding_date_year(&event_date)
                        ),
                        _ => "Name not reviewed".to_string(),
                    };
                    IdentityNoteRow {
                        asn,
                        label,
                        current_identity,
                        note,
                    }
                })
                .collect();
            let stream_rows: Vec<WorkbenchFindingStreamRow> = finding
                .streams
                .iter()
                .map(|s| WorkbenchFindingStreamRow {
                    prefix: s.prefix.clone(),
                    baseline: exact_path_display(&s.baseline_path),
                    after: match &s.changed_path {
                        Some(p) => exact_path_display(p),
                        None => {
                            if s.withdrawn {
                                "absent".to_string()
                            } else {
                                "—".to_string()
                            }
                        }
                    },
                    final_path: s
                        .final_path
                        .as_ref()
                        .map(|p| exact_path_display(p))
                        .unwrap_or_else(|| "—".to_string()),
                    first: s
                        .first_change_utc
                        .as_deref()
                        .map(|t| wb_time(t, window))
                        .unwrap_or_else(|| "—".to_string()),
                    vis_restored: s
                        .visibility_restored_at
                        .as_deref()
                        .map(|t| wb_time(t, window))
                        .unwrap_or_else(|| "—".to_string()),
                    base_restored: s
                        .exact_baseline_restored_at
                        .as_deref()
                        .map(|t| wb_time(t, window))
                        .unwrap_or_else(|| "—".to_string()),
                    evidence: s.evidence_refs.clone(),
                })
                .collect();
            WorkbenchFindingRow {
                stable_id: finding.stable_id.clone(),
                time,
                observer: format!("{} · {}", finding.collector, finding.observer_site),
                region: finding.observer_region.clone(),
                peer,
                peer_ip: finding.peer_ip.clone(),
                relationship,
                prefixes: if finding.distinct_prefixes == 1 {
                    finding
                        .exact_prefixes
                        .first()
                        .cloned()
                        .unwrap_or_else(|| "1 prefix".to_string())
                } else {
                    format!("{} prefixes", finding.distinct_prefixes)
                },
                prefix_count: finding.distinct_prefixes,
                principal: principal_ids.contains(finding.stable_id.as_str()),
                change: finding.effect.label().to_string(),
                before,
                after,
                compact_meaning,
                final_state_line,
                chronology,
                identity_notes,
                numeric_before: exact_path_display(&baseline_exact),
                numeric_after: if changed_exact.is_empty() {
                    if matches!(
                        finding.effect,
                        RE::PrefixesTemporarilyAbsent | RE::PrefixesWithdrawn
                    ) {
                        "absent".to_string()
                    } else {
                        "—".to_string()
                    }
                } else {
                    exact_path_display(&changed_exact)
                },
                named_before,
                named_after,
                path_explanation,
                outcome,
                statement: crate::catalog::workbench::finding_statement(finding),
                exact_prefixes: finding.exact_prefixes.clone(),
                preview_prefixes: preview,
                hidden_prefix_count: hidden,
                copy_prefixes: finding.exact_prefixes.join("\n"),
                copy_before_paths: finding
                    .streams
                    .iter()
                    .map(|s| exact_path_display(&s.baseline_path))
                    .collect::<Vec<_>>()
                    .join("\n"),
                copy_after_paths: finding
                    .streams
                    .iter()
                    .map(|s| match &s.changed_path {
                        Some(p) => exact_path_display(p),
                        None => {
                            if s.withdrawn {
                                "absent".to_string()
                            } else {
                                "—".to_string()
                            }
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("\n"),
                stream_rows,
                evidence_refs: finding.evidence_refs.clone(),
                earlier_change: finding
                    .earlier_change
                    .as_ref()
                    .map(|ec| WorkbenchEarlierChange {
                        stable_id: ec.stable_id.clone(),
                        label: ec.label.clone(),
                    }),
                scope_limit: finding.scope_limit.clone(),
                first_exact: finding.first_observed.clone().unwrap_or_default(),
                visibility_restored_exact: finding
                    .visibility_restored_at
                    .clone()
                    .unwrap_or_default(),
                baseline_restored_exact: finding
                    .exact_baseline_restored_at
                    .clone()
                    .unwrap_or_default(),
                expanded: f.expand_all || f.episode == Some(idx),
                prefixes_open: f.prefixes == Some(idx),
            }
        })
        .collect()
}

/// Most frequent baseline/changed exact path pair across a finding's
/// member streams (the summary signatures' exact counterparts).
fn finding_path_pair(f: &crate::catalog::workbench::RoutingFinding) -> (Vec<u32>, Vec<u32>) {
    let most_frequent = |pick: &dyn Fn(
        &crate::catalog::workbench::FindingStream,
    ) -> Option<Vec<u32>>|
     -> Vec<u32> {
        let mut counts: std::collections::BTreeMap<Vec<u32>, usize> =
            std::collections::BTreeMap::new();
        for s in &f.streams {
            if let Some(p) = pick(s) {
                *counts.entry(p).or_default() += 1;
            }
        }
        counts
            .iter()
            .max_by_key(|(p, c)| (*c, p.len()))
            .map(|(p, _)| p.clone())
            .unwrap_or_default()
    };
    (
        most_frequent(&|s| {
            if s.baseline_path.is_empty() {
                None
            } else {
                Some(s.baseline_path.clone())
            }
        }),
        most_frequent(&|s| s.changed_path.clone()),
    )
}

/// Named path segments with textual diff markers.
fn named_segments(
    path: &[u32],
    diff: &crate::catalog::workbench::PathDiff,
    is_after: bool,
    name_for: &dyn Fn(u32) -> String,
) -> Vec<PathSegmentRow> {
    let mut rows: Vec<PathSegmentRow> = Vec::new();
    if path.is_empty() {
        return rows;
    }
    let mut i = 0;
    while i < path.len() {
        let asn = path[i];
        let mut run = 1;
        while i + run < path.len() && path[i + run] == asn {
            run += 1;
        }
        let mark = if is_after {
            if diff.inserted.contains(&asn) {
                "ins"
            } else {
                "same"
            }
        } else if diff.removed.contains(&asn) {
            "del"
        } else {
            "same"
        };
        let label = if run > 1 {
            format!("{} ×{run}", name_for(asn))
        } else {
            name_for(asn)
        };
        rows.push(PathSegmentRow { asn, label, mark });
        i += run;
    }
    rows
}
/// Exact AS path display: plain space-joined ASN sequence. The compact
/// ×N collapse is only used in the summary signature, never here.
fn exact_path_display(path: &[u32]) -> String {
    if path.is_empty() {
        return "—".to_string();
    }
    path.iter()
        .map(|asn| format!("AS{asn}"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Outcome text for a finding: restoration(s) or still-changed state.
fn finding_outcome(f: &crate::catalog::workbench::RoutingFinding) -> String {
    use crate::catalog::workbench::EndState as ES;
    let window = f.first_observed.as_deref().unwrap_or("");
    let time = |t: &str| wb_time(t, window);
    match (
        &f.visibility_restored_at,
        &f.exact_baseline_restored_at,
        &f.state_at_window_end,
    ) {
        (Some(v), Some(b), _) => format!("Visibility {} · baseline {}", time(v), time(b)),
        (Some(v), None, _) => format!("Visibility {}", time(v)),
        (None, Some(b), _) => format!("Baseline {}", time(b)),
        (None, None, ES::StillChangedAtWindowEnd) => "Still changed at window end".to_string(),
        (None, None, ES::AbsentAtWindowEnd) => "Absent at window end".to_string(),
        (None, None, ES::Unresolved) => "Unresolved".to_string(),
        _ => "—".to_string(),
    }
}

/// Year of an ISO date ("2019-08-21" -> "2019") for identity notes.
fn finding_date_year(date: &str) -> String {
    date.chars().take(4).collect()
}

/// Compact two-line route meaning for a principal card
/// . Absence findings are temporally qualified: nothing is said
/// to "remain visible" across the absence interval.
fn compact_finding_meaning(
    f: &crate::catalog::workbench::RoutingFinding,
    baseline_exact: &[u32],
    changed_exact: &[u32],
    diff: &crate::catalog::workbench::PathDiff,
    plane: &str,
    plane_asns: &[u32],
    name_for: &dyn Fn(u32) -> String,
) -> String {
    use crate::catalog::workbench::RoutingEffect as RE;
    let n = f.distinct_prefixes;
    // "<count> <TARGET> prefixes": the reviewed target name precedes
    // the noun.
    let unit = if n == 1 {
        if f.target_name.is_empty() {
            "1 prefix".to_string()
        } else {
            format!("1 {} prefix", f.target_name)
        }
    } else if f.target_name.is_empty() {
        format!("{n} prefixes")
    } else {
        format!("{n} {} prefixes", f.target_name)
    };
    let target = String::new();
    let plane_text = if plane.is_empty() {
        "the reviewed plane".to_string()
    } else {
        // The plane label may already carry the ASN (e.g. "Plane
        // R&E path (AS64512)"); never duplicate it.
        let asn = if plane.contains("(AS") {
            String::new()
        } else {
            plane_asns
                .first()
                .map(|a| format!(" AS{a}"))
                .unwrap_or_default()
        };
        format!("{plane}{asn}")
    };
    // Inserted ASNs with multiplicity (AS24489x4), plus a positional
    // clause for the first inserted ASN ("AS2907 appeared between
    // AS7660 and the reviewed plane AS64512").
    let count = |asn: u32| changed_exact.iter().filter(|a| **a == asn).count();
    let inserted_compact: Vec<String> = diff
        .inserted
        .iter()
        .map(|a| {
            let c = count(*a);
            if c > 1 {
                format!("AS{a}×{c}")
            } else {
                format!("AS{a}")
            }
        })
        .collect();
    let insert_text = if inserted_compact.is_empty() {
        String::new()
    } else {
        format!(" and included {}", inserted_compact.join(", "))
    };
    let positional = diff.inserted.first().and_then(|first| {
        let pos = changed_exact.iter().position(|a| a == first)?;
        let prev = if pos > 0 {
            changed_exact.get(pos - 1).copied()
        } else {
            None
        };
        let next = changed_exact.get(pos + 1).copied();
        match (prev, next) {
            (Some(p), Some(n)) if p != *first && n != *first => Some(format!(
                "{} appeared between {} and {}",
                name_for(*first),
                name_for(p),
                name_for(n)
            )),
            _ => None,
        }
    });
    let prepend_text = prepend_delta_text(
        baseline_exact,
        changed_exact,
        diff,
        f.target_origin_asns.first().copied(),
        name_for,
    );
    let length_text = if diff.longer {
        " on a longer path"
    } else if diff.shorter {
        " on a shorter path"
    } else {
        ""
    };

    match f.effect {
        RE::PrefixesTemporarilyAbsent | RE::PrefixesWithdrawn => {
            // Precise absence wording: the
            // duration is the actual Withdrawal-to-return interval
            // (54 ms for the UVA group), stated in ms when
            // sub-second. Never a vague "temporarily disappeared"
            // and never a traffic-interruption claim.
            let duration = absence_duration_seconds(f);
            let dur = match duration {
                Some(secs) if secs >= 2 => format!(" for {}", human_duration(secs)),
                Some(1) => " for one second".to_string(),
                _ => match absence_duration_millis(f) {
                    Some(ms) => format!(" for {ms} ms"),
                    None => String::new(),
                },
            };
            let verb = if n == 1 { "was" } else { "were" };
            let mut s = format!("{} {verb} withdrawn from this observer{}.", unit, dur);
            if f.visibility_restored_at.is_some() {
                // Ordered withdrawal story:
                // the route that returned and the event-baseline
                // relation. The pre-finding route is never called the
                // baseline; the "exact baseline" claim requires
                // EventBaseline equality.
                if let Some(story) = crate::catalog::workbench::withdrawal_story(f) {
                    let origin = f.target_origin_asns.first().copied();
                    let event_sig = if f.event_baseline_path_signature.is_empty()
                        || f.event_baseline_path_signature == "—"
                    {
                        f.baseline_path_signature.clone()
                    } else {
                        f.event_baseline_path_signature.clone()
                    };
                    let returned_on_event_baseline =
                        crate::catalog::workbench::collapse_as_path(&story.return_path)
                            == event_sig;
                    let ret_diff = crate::catalog::workbench::diff_paths(
                        &story.before_path,
                        &story.return_path,
                        plane_asns,
                    );
                    if returned_on_event_baseline {
                        let return_n = origin
                            .map(|o| story.return_path.iter().filter(|a| **a == o).count())
                            .unwrap_or(0);
                        if return_n > 0 {
                            s.push_str(&format!(
                                " They returned on the event-baseline path containing AS{}×{return_n}.",
                                origin.unwrap()
                            ));
                        } else {
                            s.push_str(" They returned on the event-baseline path.");
                        }
                    } else if ret_diff.plane_retained && !ret_diff.inserted.is_empty() {
                        // One concise semantic sentence about the
                        // return path: repetition
                        // already expressed by the ×N notation is
                        // never restated.
                        let only_origin = ret_diff.inserted.iter().all(|a| Some(*a) == origin);
                        if !only_origin {
                            let count_in =
                                |asn: u32| story.return_path.iter().filter(|a| **a == asn).count();
                            let added: Vec<String> = ret_diff
                                .inserted
                                .iter()
                                .map(|a| {
                                    let c = count_in(*a);
                                    if c > 1 {
                                        format!("AS{a}×{c}")
                                    } else {
                                        format!("AS{a}")
                                    }
                                })
                                .collect();
                            let shape = if ret_diff.longer {
                                "longer"
                            } else if ret_diff.shorter {
                                "shorter"
                            } else {
                                "different-length"
                            };
                            let joined = if added.len() > 1 {
                                let (head, last) = added.split_at(added.len() - 1);
                                format!("{}, and {}", head.join(", "), last[0])
                            } else {
                                added.join(", ")
                            };
                            s.push_str(&format!(
                                " They returned on a {shape} path that still traversed {}, adding {}.",
                                plane_text, joined
                            ));
                        } else {
                            let return_n = origin
                                .map(|o| story.return_path.iter().filter(|a| **a == o).count())
                                .unwrap_or(0);
                            s.push_str(&format!(
                                " They returned on a path containing AS{}×{return_n}.",
                                origin.unwrap_or(0)
                            ));
                        }
                    } else if ret_diff.plane_departed {
                        s.push_str(&format!(
                            " They returned on a path that no longer traversed {}.",
                            plane_text
                        ));
                    } else {
                        s.push_str(" They returned to visibility.");
                    }
                    // The later settle: "By 07:36:30 UTC, the selected
                    // paths again contained AS225×1, matching the
                    // pre-withdrawal state but not the event baseline."
                    if let (Some(o), Some(last_t)) = (
                        origin,
                        f.streams
                            .iter()
                            .filter_map(|s| s.transitions.iter().last())
                            .max_by_key(|t| t.timestamp.clone()),
                    ) {
                        let last_n = last_t.after_path.iter().filter(|a| **a == o).count();
                        let return_n = story.return_path.iter().filter(|a| **a == o).count();
                        if last_n != return_n && last_n > 0 {
                            let lt = crate::catalog::workbench::finding_time(&last_t.timestamp);
                            let settled_on_pre = last_t.after_path == story.before_path;
                            let matches_event =
                                crate::catalog::workbench::collapse_as_path(&last_t.after_path)
                                    == event_sig;
                            let clause = if settled_on_pre && !matches_event {
                                ", matching the pre-withdrawal state but not the event baseline"
                            } else if matches_event {
                                ", returning to the event baseline"
                            } else {
                                ""
                            };
                            s.push_str(&format!(
                                " By {lt} UTC, the selected paths again contained AS{o}×{last_n}{clause}."
                            ));
                        }
                    }
                } else {
                    // No per-prefix transition evidence: fall back to
                    // the episode-level path facts (still exact).
                    let after = if diff.plane_retained {
                        format!(
                            " After visibility returned, the selected route continued through {}{}.",
                            plane_text, insert_text
                        )
                    } else if diff.plane_departed {
                        format!(
                            " After visibility returned, the selected route no longer traversed {}.",
                            plane_text
                        )
                    } else {
                        " After visibility returned, a different route was observed.".to_string()
                    };
                    s.push_str(&after);
                }
            } else {
                s.push_str(" The prefixes remained absent at the event-window end.");
            }
            let _ = length_text;
            s
        }
        RE::AsPathChanged | RE::PrependingChanged => {
            let mut s = if diff.plane_departed {
                format!(
                    "The {} changed path; the new path no longer traversed {}.",
                    unit, plane_text
                )
            } else {
                format!(
                    "The {} changed path while remaining visible through {}{}.",
                    unit, plane_text, length_text
                )
            };
            if !inserted_compact.is_empty() && !diff.plane_departed {
                match &positional {
                    Some(p) => s.push_str(&format!(" {p}.")),
                    None => s.push_str(&format!(
                        " {} appeared in the selected path.",
                        inserted_compact.join(", ")
                    )),
                }
            }
            // A later transition that inserts an ASN not in the first
            // changed path (e.g. AS2907 in the UVA 7660 story): state
            // it with its exact time and position.
            if let Some(clause) = later_insertion_clause(f, &|asn| name_for(asn)) {
                s.push(' ');
                s.push_str(&clause);
            }
            if !prepend_text.is_empty() {
                s.push(' ');
                s.push_str(&prepend_text);
            }
            s
        }
        _ => format!("The {}{} showed a route-state change.", n, target),
    }
}

/// Prepending deltas with exact from/to counts. "origin-AS prepending"
/// is used ONLY when the repeated ASN is the finding's target origin
/// ; intermediate repetition reads
/// "AS24489 appeared four consecutive times in the selected path".
fn prepend_delta_text(
    before: &[u32],
    after: &[u32],
    diff: &crate::catalog::workbench::PathDiff,
    target_origin: Option<u32>,
    _name_for: &dyn Fn(u32) -> String,
) -> String {
    let count = |path: &[u32], asn: u32| path.iter().filter(|a| **a == asn).count();
    let mut parts: Vec<String> = Vec::new();
    for (asn, delta) in &diff.count_deltas {
        let before_count = count(before, *asn);
        let after_count = count(after, *asn);
        if target_origin == Some(*asn) {
            let direction = if *delta > 0 { "increased" } else { "decreased" };
            parts.push(format!(
                "origin-AS prepending {} from {} to {} AS{} occurrences",
                direction, before_count, after_count, asn
            ));
        } else if *delta > 0 {
            parts.push(format!(
                "AS{} appeared {} consecutive times in the selected path",
                asn, after_count
            ));
        } else {
            parts.push(format!(
                "AS{} repetition decreased from {} to {}",
                asn, before_count, after_count
            ));
        }
    }
    parts.join("; ")
}

/// A later transition that inserts an ASN absent from both the
/// baseline and the first changed path: renders
/// "At 07:33:59, AS2907 appeared between AS7660 and AS64512." using
/// short labels (reviewed names or bare ASNs).
fn later_insertion_clause(
    f: &crate::catalog::workbench::RoutingFinding,
    name_for: &dyn Fn(u32) -> String,
) -> Option<String> {
    let short = |asn: u32| -> String {
        let n = name_for(asn);
        // name_for may append " — current identity: ..."; keep the
        // leading ASn token for short labels.
        n.split(" — ").next().unwrap_or(&n).to_string()
    };
    let baseline = f
        .streams
        .iter()
        .find(|s| !s.baseline_path.is_empty())
        .map(|s| s.baseline_path.clone());
    let first_changed = f.streams.iter().find_map(|s| s.changed_path.clone());
    let mut candidate: Option<(String, u32, u32, u32)> = None; // (time, asn, prev, next)
    for stream in &f.streams {
        for t in &stream.transitions {
            if t.after_path.is_empty() {
                continue;
            }
            let inserted: Vec<u32> = t
                .after_path
                .iter()
                .copied()
                .filter(|a| {
                    !baseline.as_deref().map(|b| b.contains(a)).unwrap_or(false)
                        && !first_changed
                            .as_deref()
                            .map(|c| c.contains(a))
                            .unwrap_or(false)
                })
                .collect();
            if let Some(asn) = inserted.first() {
                let pos = t.after_path.iter().position(|a| a == asn)?;
                let prev = if pos > 0 {
                    t.after_path.get(pos - 1).copied()
                } else {
                    None
                };
                let next = t.after_path.get(pos + 1).copied();
                if let (Some(p), Some(n2)) = (prev, next) {
                    let better = match &candidate {
                        Some((ct, _, _, _)) => t.timestamp < *ct,
                        None => true,
                    };
                    if better {
                        candidate = Some((t.timestamp.clone(), *asn, p, n2));
                    }
                }
            }
        }
    }
    let (ts, asn, prev, next) = candidate?;
    Some(format!(
        "At {}, {} appeared between {} and {}.",
        crate::catalog::workbench::finding_time(&ts),
        short(asn),
        short(prev),
        short(next)
    ))
}

/// Human duration: "2 seconds", "11 minutes and 13 seconds".
fn human_duration(secs: u64) -> String {
    let unit = |n: u64, word: &str| -> String {
        if n == 1 {
            format!("1 {word}")
        } else {
            format!("{n} {word}s")
        }
    };
    if secs < 60 {
        return unit(secs, "second");
    }
    let m = secs / 60;
    let s = secs % 60;
    if s == 0 {
        unit(m, "minute")
    } else {
        format!("{} and {}", unit(m, "minute"), unit(s, "second"))
    }
}

/// Absence duration in whole seconds between withdrawal and the
/// visibility restoration, when both are exact.
/// Sub-second absence duration in milliseconds from the ordered
/// Withdrawal-to-return timestamps: the exact
/// 54 ms interval for the UVA group, never a vague duration.
fn absence_duration_millis(f: &crate::catalog::workbench::RoutingFinding) -> Option<u64> {
    use crate::catalog::workbench::{finding_time, withdrawal_story};
    let story = withdrawal_story(f)?;
    let start = story.withdrawal_at.as_deref()?;
    let end = story.returned_at_min.as_deref()?;
    let ms = |iso: &str| -> Option<i64> {
        let t = finding_time(iso);
        let mut p = t.split(':');
        let h: i64 = p.next()?.parse().ok()?;
        let m: i64 = p.next()?.parse().ok()?;
        let sec: i64 = p.next()?.trim_end_matches('Z').parse().ok()?;
        Some((h * 3600 + m * 60 + sec) * 1000)
    };
    let frac = |iso: &str| -> i64 {
        // Fractional seconds from the full ISO timestamp.
        match iso.find('.') {
            Some(dot) => {
                let digits: String = iso[dot + 1..]
                    .chars()
                    .take_while(|c| c.is_ascii_digit())
                    .collect();
                let n = digits.len().min(3);
                digits[..n].parse::<i64>().unwrap_or(0) * 10i64.pow((3 - n) as u32)
            }
            None => 0,
        }
    };
    let d = ms(end)? - ms(start)? + frac(end) - frac(start);
    if d > 0 {
        Some(d as u64)
    } else {
        None
    }
}

fn absence_duration_seconds(f: &crate::catalog::workbench::RoutingFinding) -> Option<u64> {
    // The Withdrawal-to-return interval from the ordered evidence
    // , never the episode's first-change span.
    let story = crate::catalog::workbench::withdrawal_story(f)?;
    let start = story.withdrawal_at.as_deref()?;
    let end = story.returned_at_min.as_deref()?;
    let s = crate::catalog::workbench::parse_utc_seconds(start)?;
    let e = crate::catalog::workbench::parse_utc_seconds(end)?;
    if e >= s {
        Some((e - s) as u64)
    } else {
        None
    }
}

/// Restoration/final-state summary line: the
/// exact-baseline reappearance and the FINAL observed state are
/// distinct facts and never conflated.
pub fn finding_final_state_line(f: &crate::catalog::workbench::RoutingFinding) -> String {
    use crate::catalog::workbench::{CooldownOutcome as CO, EndState as ES};
    let time = |t: &str| {
        crate::catalog::workbench::workbench_time(t, f.first_observed.as_deref().unwrap_or(""))
    };
    let exact_times: Vec<&str> = f
        .streams
        .iter()
        .filter_map(|s| s.exact_baseline_restored_at.as_deref())
        .collect();
    let mut s = String::new();
    if let Some(t) = &f.exact_baseline_restored_at {
        let (min, max) = (
            exact_times.iter().min().copied().unwrap_or(t),
            exact_times.iter().max().copied().unwrap_or(t),
        );
        // Group restoration wording: several
        // prefixes restoring at different times restore as a GROUP
        // over the interval — one route never gradually restores.
        // time() already appends " UTC"; strip it and add one suffix.
        let bare = |t: &str| time(t).trim_end_matches(" UTC").to_string();
        if f.distinct_prefixes > 1 {
            let (lo, hi) = (bare(min), bare(max));
            if lo != hi {
                s.push_str(&format!(
                    "Exact baseline paths restored across the {}-prefix group between {} and {} UTC.",
                    f.distinct_prefixes, lo, hi
                ));
            } else {
                s.push_str(&format!(
                    "Exact baseline paths restored across the {}-prefix group at {} UTC.",
                    f.distinct_prefixes, lo
                ));
            }
        } else {
            s.push_str(&format!(
                "The exact baseline path was restored at {} UTC.",
                bare(t)
            ));
        }
    } else {
        match &f.state_at_window_end {
            ES::StillChangedAtWindowEnd => {
                s.push_str("The route remained changed at the event-window end.")
            }
            ES::AbsentAtWindowEnd => {
                s.push_str("The prefixes remained absent at the event-window end.")
            }
            ES::Unresolved => s.push_str("The end state is unresolved."),
            _ => {}
        }
        if let Some(v) = &f.visibility_restored_at {
            let tv = time(v);
            let bare = tv.trim_end_matches(" UTC");
            let mut returned = format!("Visibility returned at {bare} UTC.");
            // "By TIME, the selected path contained AS225xM." — a later
            // post-return prepend settle, from the ordered evidence
            //
            if let Some(o) = f.target_origin_asns.first() {
                if let Some(story) = crate::catalog::workbench::withdrawal_story(f) {
                    let return_n = story.return_path.iter().filter(|a| **a == *o).count();
                    let last = f
                        .streams
                        .iter()
                        .filter_map(|s| s.transitions.iter().last())
                        .max_by_key(|t| t.timestamp.clone());
                    if let Some(last_t) = last {
                        let last_n = last_t.after_path.iter().filter(|a| **a == *o).count();
                        if last_n != return_n && last_n > 0 {
                            let lt = crate::catalog::workbench::finding_time(&last_t.timestamp);
                            returned.push_str(&format!(
                                " By {lt} UTC, the selected path contained AS{o}×{last_n}."
                            ));
                        }
                    }
                }
            }
            s.push_str(&returned);
        }
    }
    // Final observed state, from the actual final route
    // : never assume the restoration event is the final state,
    // and never echo internal enum names.
    if f.final_path_signature != "—" {
        let human = crate::catalog::workbench::human_window_end_state(f);
        let lower: String = human
            .chars()
            .next()
            .map(|c| c.to_lowercase().collect::<String>() + &human[c.len_utf8()..])
            .unwrap_or(human);
        s.push_str(&format!(" Final observed state: {lower}."));
    }
    match &f.state_at_analysis_end {
        CO::StillChangingBeforeAnalysisEnd(t) => {
            s.push_str(&format!(
                " The routes changed again at {} and remained changed at analysis end ({}).",
                time(t),
                crate::catalog::workbench::workbench_time(
                    &f.analysis_end,
                    f.first_observed.as_deref().unwrap_or("")
                )
            ));
        }
        CO::NoRestorationBeforeAnalysisEnd(_)
            if f.exact_baseline_restored_at.is_none()
                && f.visibility_restored_at.is_none()
                && f.reviewed_plane_restored_at.is_none() =>
        {
            s.push_str(&format!(
                " No restoration was observed before the analysis ended at {}.",
                crate::catalog::workbench::workbench_time(
                    &f.analysis_end,
                    f.first_observed.as_deref().unwrap_or("")
                )
            ));
        }
        _ => {}
    }
    s
}

/// Observer comparison by region: narrative
/// routing differences per observer site. Regional ratios are NOT part
/// of this section — they live in Observation coverage.
fn region_comparison_rows(
    vm: &crate::catalog::workbench::IncidentWorkbenchViewModel,
    f: &WorkbenchFilters,
) -> Vec<WorkbenchRegionComparisonRow> {
    use crate::catalog::workbench::select_principal_findings;
    let event_date = crate::catalog::workbench::window_date(&vm.window_start);
    let (principal, _) = select_principal_findings(vm.findings.clone(), &vm.plane_asns, 4);
    let principal: Vec<&crate::catalog::workbench::RoutingFinding> = principal
        .iter()
        .filter(|fnd| matches_finding_filter(fnd, f))
        .collect();
    let mut by_region: std::collections::BTreeMap<String, Vec<String>> =
        std::collections::BTreeMap::new();
    let mut changed_sites: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    // One narrative line per observer site, from the site's principal
    // story (the highest-ranked finding at that session).
    let mut by_site: std::collections::BTreeMap<
        String,
        &crate::catalog::workbench::RoutingFinding,
    > = std::collections::BTreeMap::new();
    for fnd in &principal {
        by_site.entry(fnd.observer_site.clone()).or_insert(fnd);
    }
    for fnd in by_site.values() {
        by_region
            .entry(fnd.observer_region.clone())
            .or_default()
            .push(region_site_narrative(
                fnd,
                &vm.plane_asns,
                &event_date,
                &vm.asn_identities,
            ));
        changed_sites.insert(fnd.observer_site.clone());
    }
    // No-change sessions per region: "no route-state counterpart".
    for e in &vm.episodes {
        if e.effect_kind == crate::catalog::workbench::EffectKind::NoRouteStateChange {
            let site = e.observer_site.clone();
            if !changed_sites.contains(&site) {
                by_region
                    .entry(e.observer_region.clone())
                    .or_default()
                    .push(format!(
                        "{}: no route-state counterpart observed for the selected prefixes.",
                        site
                    ));
            }
        }
    }
    by_region
        .into_iter()
        .map(|(region, mut statements)| {
            statements.sort();
            WorkbenchRegionComparisonRow {
                region,
                statements,
                ratio: String::new(),
            }
        })
        .collect()
}

/// One narrative sentence for an observer site ,
/// built from the site's principal finding and its path semantics.
fn region_site_narrative(
    f: &crate::catalog::workbench::RoutingFinding,
    plane_asns: &[u32],
    event_date: &str,
    identities: &[crate::catalog::workbench::AsnIdentity],
) -> String {
    use crate::catalog::workbench::{diff_paths, RoutingEffect as RE};
    let name_for = |asn: u32| -> String {
        match identities
            .iter()
            .find(|i| i.asn == asn && i.valid_at(event_date) && i.has_display_name())
        {
            Some(i) => format!("{} (AS{})", i.display_name, asn),
            None => format!("AS{asn} — name not reviewed"),
        }
    };
    let plane = if f.named_path_plane.is_empty() {
        "the reviewed path plane".to_string()
    } else {
        f.named_path_plane.clone()
    };
    let (baseline, changed) = finding_path_pair(f);
    let d = diff_paths(&baseline, &changed, plane_asns);
    let n = f.distinct_prefixes;
    let group = if n == 1 {
        "a single prefix".to_string()
    } else {
        format!("{n} prefixes")
    };
    let site = f.observer_site.clone();
    match f.effect {
        RE::PrefixesTemporarilyAbsent | RE::PrefixesWithdrawn => {
            if f.visibility_restored_at.is_some() {
                format!(
                    "The {} view briefly lost {} and then saw them return on a different path.",
                    site, group
                )
            } else {
                format!("The {} view lost {} and they remained absent.", site, group)
            }
        }
        _ if d.plane_departed => {
            format!(
                "The selected prefixes remained visible at {}, but the new path no longer traversed {}.",
                site, plane
            )
        }
        RE::AsPathChanged | RE::PrependingChanged if d.plane_retained => {
            if f.exact_baseline_restored_at.is_none()
                && f.state_at_window_end
                    == crate::catalog::workbench::EndState::StillChangedAtWindowEnd
            {
                format!(
                    "{} at {} remained on {} and were still changed at the event-window end.",
                    group, site, plane
                )
            } else if d.longer {
                format!(
                    "{} at {} changed path while remaining on {} through a longer path.",
                    group, site, plane
                )
            } else {
                format!(
                    "{} at {} changed path while remaining on {}.",
                    group, site, plane
                )
            }
        }
        _ => {
            let _ = &name_for;
            format!("{} at {} showed a route-state change.", group, site)
        }
    }
}

/// Display-time renderer (Part 4): HH:MM:SS UTC for same-day rows.
fn wb_time(ts: &str, window_start: &str) -> String {
    crate::catalog::workbench::workbench_time(ts, window_start)
}

/// "HH:MM" of a timestamp (header ranges, Part 9).
fn hhmm(ts: &str) -> String {
    ts.get(11..16)
        .map(|s| s.to_string())
        .unwrap_or_else(|| ts.to_string())
}

/// "YYYY-MM-DD HH:MM–HH:MM UTC" (operator incident range, Part 9).
fn human_date_range(start: &str, end: &str) -> String {
    let date = start.get(0..10).unwrap_or("");
    format!("{date} {}–{} UTC", hhmm(start), hhmm(end))
}

/// "HH:MM–HH:MM UTC" (pilot range; the date appears in the incident
/// line of the same header, Part 9).
fn human_pilot_range(start: &str, end: &str) -> String {
    format!("{}–{} UTC", hhmm(start), hhmm(end))
}

/// Pre-rendered breadth row for the template (Part 5): a glanceable
/// matrix — combined CHANGED/ELIGIBLE and STREAMS/BASELINE cells.
#[derive(Serialize)]
pub struct WorkbenchBreadthRow {
    pub region: String,
    pub changed_eligible: String,
    pub episodes: String,
    pub streams_baseline: String,
    pub prefixes: String,
    pub route_instances: String,
    pub transitions: String,
    pub first_change: String,
    pub last_restoration: String,
    pub gaps: String,
    pub changed_class: String,
}

fn breadth_rows(
    vm: &crate::catalog::workbench::IncidentWorkbenchViewModel,
) -> Vec<WorkbenchBreadthRow> {
    let window = &vm.window_start;
    vm.breadth
        .iter()
        .map(|b| {
            let changed = b.changed_observer_sessions > 0;
            let eligible = b.eligible_observer_sessions > 0;
            let changed_class = if changed {
                "wb-breadth-changed"
            } else if eligible {
                "wb-breadth-unchanged"
            } else {
                "wb-breadth-none"
            };
            WorkbenchBreadthRow {
                region: b.region.clone(),
                changed_eligible: format!(
                    "{}/{}",
                    b.changed_observer_sessions, b.eligible_observer_sessions
                ),
                episodes: b.episode_count.to_string(),
                streams_baseline: format!("{}/{}", b.changed_streams, b.baseline_streams),
                prefixes: b.changed_prefixes.to_string(),
                route_instances: b.route_instances.to_string(),
                transitions: b.transition_count.to_string(),
                first_change: b
                    .first_change
                    .as_ref()
                    .map(|t| wb_time(t, window))
                    .unwrap_or_else(|| "—".to_string()),
                last_restoration: b
                    .last_restoration
                    .as_ref()
                    .map(|t| wb_time(t, window))
                    .unwrap_or_else(|| "—".to_string()),
                gaps: (b.sessions_without_baseline_visibility
                    + b.sessions_with_incomplete_coverage)
                    .to_string(),
                changed_class: changed_class.to_string(),
            }
        })
        .collect()
}

/// Pre-rendered timeline row for the template.
#[derive(Serialize)]
pub struct WorkbenchTimelineRow {
    pub session: String,
    pub region: String,
    pub window: String,
    pub first_change: String,
    pub absence: String,
    pub path_change: String,
    pub restoration: String,
    pub end_state: String,
}

fn timeline_rows(
    vm: &crate::catalog::workbench::IncidentWorkbenchViewModel,
    f: &WorkbenchFilters,
) -> Vec<WorkbenchTimelineRow> {
    use crate::catalog::workbench::session_key_of;
    let window = &vm.window_start;
    // Lane labels: collector
    // site + peer identity from the episode/finding evidence, e.g.
    // "RRC15 Sao Paulo, Brazil · RNP (AS1916)". Identities are
    // time-scoped to the event date.
    let event_date = crate::catalog::workbench::window_date(&vm.window_start);
    let mut lane_labels: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    for e in &vm.episodes {
        let key = session_key_of(&e.observer_session);
        let collector = key.collector.clone();
        let peer = match e.peer_asn.or_else(|| {
            if e.observed_peer_asns.len() == 1 {
                Some(e.observed_peer_asns[0])
            } else {
                None
            }
        }) {
            Some(asn) => {
                let name = vm
                    .asn_identities
                    .iter()
                    .find(|i| i.asn == asn && i.valid_at(&event_date) && i.has_display_name())
                    .map(|i| i.display_name.as_str())
                    .unwrap_or("");
                if name.is_empty() {
                    format!("AS{asn}")
                } else {
                    format!("{name} (AS{asn})")
                }
            }
            None => "peer ASN unreviewed".to_string(),
        };
        lane_labels.insert(
            e.observer_session.clone(),
            format!("{} {} · {}", collector, e.observer_site, peer),
        );
    }
    // Active filters also constrain the timeline lanes: a lane whose
    // session has no matching episode is dropped.
    let allowed: std::collections::BTreeSet<&str> = vm
        .episodes
        .iter()
        .filter(|e| matches_episode_filter(e, f))
        .map(|e| e.observer_session.as_str())
        .collect();
    vm.timeline
        .iter()
        .filter(|l| !f.active() || allowed.contains(l.observer_session.as_str()))
        .map(|l| WorkbenchTimelineRow {
            session: lane_labels
                .get(&l.observer_session)
                .cloned()
                .unwrap_or_else(|| l.observer_session.clone()),
            region: l.region.clone(),
            window: format!(
                "{} – {}",
                wb_time(&l.window_start, window),
                wb_time(&l.window_end, window)
            ),
            first_change: l
                .first_route_change
                .as_ref()
                .map(|m| wb_time(&m.timestamp_utc, window))
                .unwrap_or_else(|| "—".to_string()),
            absence: l
                .absence_interval
                .as_ref()
                .map(|(a, b)| format!("{} – {}", wb_time(a, window), wb_time(b, window)))
                .unwrap_or_else(|| "—".to_string()),
            path_change: l
                .path_change_interval
                .as_ref()
                .map(|(a, b)| format!("{} – {}", wb_time(a, window), wb_time(b, window)))
                .unwrap_or_else(|| "—".to_string()),
            restoration: l
                .restoration_interval
                .as_ref()
                .map(|(a, b)| {
                    if a == b {
                        wb_time(a, window)
                    } else {
                        format!("{} – {}", wb_time(a, window), wb_time(b, window))
                    }
                })
                .unwrap_or_else(|| "—".to_string()),
            end_state: if l.unresolved_end_state {
                "unresolved".to_string()
            } else {
                "observed".to_string()
            },
        })
        .collect()
}

/// Pre-rendered operator anchor row.
#[derive(Serialize)]
pub struct WorkbenchAnchorRow {
    pub timestamp: String,
    pub label: String,
    pub kind: String,
}

fn anchor_rows(
    vm: &crate::catalog::workbench::IncidentWorkbenchViewModel,
) -> Vec<WorkbenchAnchorRow> {
    vm.operator_anchors
        .iter()
        .map(|m| WorkbenchAnchorRow {
            timestamp: m.timestamp_utc.clone(),
            label: m.label.clone(),
            kind: m.kind.clone(),
        })
        .collect()
}

/// Pre-rendered coverage-only session row.
#[derive(Serialize)]
pub struct WorkbenchCoverageRow {
    pub session: String,
    pub region: String,
    pub status: String,
    pub reason: String,
    pub detail: String,
}

fn coverage_rows(
    vm: &crate::catalog::workbench::IncidentWorkbenchViewModel,
) -> Vec<WorkbenchCoverageRow> {
    let mut out: Vec<WorkbenchCoverageRow> = vm
        .no_baseline_sessions
        .iter()
        .map(|s| WorkbenchCoverageRow {
            session: s.observer_session.clone(),
            region: s.region.clone(),
            status: "NoBaselineVisibility".to_string(),
            reason: s.reason.human_label().to_string(),
            detail: s.detail.clone(),
        })
        .collect();
    out.extend(vm.incomplete_sessions.iter().map(|s| WorkbenchCoverageRow {
        session: s.observer_session.clone(),
        region: s.region.clone(),
        status: "IncompleteCoverage".to_string(),
        reason: s.reason.human_label().to_string(),
        detail: s.detail.clone(),
    }));
    out
}

/// A read-only wrapper that counts SQL statements executed through it.
///
/// Used to bound the workbench's catalog query count (Part 13). Only the
/// query counter and elapsed time are tracked; no statement content is
/// logged.
pub struct CountingConnection<'a> {
    pub conn: &'a rusqlite::Connection,
    pub query_count: usize,
    pub elapsed_ms: f64,
}

impl<'a> CountingConnection<'a> {
    pub fn new(conn: &'a rusqlite::Connection) -> Self {
        CountingConnection {
            conn,
            query_count: 0,
            elapsed_ms: 0.0,
        }
    }

    pub fn prepare(&mut self, sql: &str) -> Result<rusqlite::Statement<'_>, rusqlite::Error> {
        let start = std::time::Instant::now();
        let stmt = self.conn.prepare(sql)?;
        self.elapsed_ms += start.elapsed().as_secs_f64() * 1000.0;
        self.query_count += 1;
        Ok(stmt)
    }

    pub fn query_row<T, F, P>(&mut self, sql: &str, params: P, f: F) -> Result<T, rusqlite::Error>
    where
        F: FnOnce(&rusqlite::Row<'_>) -> rusqlite::Result<T>,
        P: rusqlite::Params,
    {
        let start = std::time::Instant::now();
        let out = self.conn.query_row(sql, params, f);
        self.elapsed_ms += start.elapsed().as_secs_f64() * 1000.0;
        self.query_count += 1;
        out
    }
}

// ── Job-workflow view models (jobs_view.rs) ─────────────────────────

pub use crate::catalog::web::jobs_view::{
    api_job_detail, api_jobs_index, api_plan_detail, edit_plan_revision, load_job_detail,
    load_jobs_index, load_plan_review, JobDetailView, JobsIndexView, PlanReviewView,
};
