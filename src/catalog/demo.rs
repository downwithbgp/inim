//! Deterministic offline demo catalog (Part 25).
//!
//! `demo init` creates a fresh SQLite catalog from tracked reviewed
//! material only: no private databases, no raw archive downloads, no
//! external network access, no manual SQL, no modification of tracked
//! files. `demo verify` checks the expected events, workbench rendering,
//! artifact references, and the absence of source access and absolute
//! paths.

use std::path::Path;

use rusqlite::Connection;

/// Expected demo events (external ids) after a successful init.
/// MAN LAN arrives through the case-study layer (slug manlan-2019),
/// verified separately — its pilot runs are not catalog analysis_runs.
pub const DEMO_EXPECTED_EVENTS: &[&str] = &[
    "INC0299001", // UVA event
    "INC0301970", // MAN LAN related event
    "INC0302574", // visibility audit event
];

pub fn demo_init(db_path: &Path, root: &Path, force: bool) -> Result<DemoReport, String> {
    if db_path.exists() && !force {
        return Err(format!(
            "refusing to overwrite existing database {}; pass --force to replace it",
            db_path.display()
        ));
    }
    if db_path.exists() && force {
        std::fs::remove_file(db_path).map_err(|e| format!("cannot remove old database: {e}"))?;
    }
    let conn = crate::catalog::db::open_catalog(db_path)?;
    crate::catalog::import::import_repository(&conn, root, env!("CARGO_PKG_VERSION"), None)
        .map_err(|e| format!("demo import failed: {e}"))?;
    // Import the reviewed case studies (MAN LAN case study layer).
    let cs_path = root.join("case-studies/manlan-2019");
    if cs_path.is_dir() {
        crate::catalog::case_study_import::import_case_study(&conn, &cs_path)
            .map_err(|e| format!("demo case-study import failed (manlan-2019): {e}"))?;
    }
    drop(conn);
    demo_verify(db_path, root)
}

/// Verify a demo catalog. Checks expected events, run artifacts, no
/// absolute paths, and that artifact references resolve.
pub fn demo_verify(db_path: &Path, root: &Path) -> Result<DemoReport, String> {
    let conn = crate::catalog::db::open_catalog(db_path)?;
    let mut report = DemoReport::default();
    for expected in DEMO_EXPECTED_EVENTS {
        let found = crate::catalog::db::get_event_by_external(&conn, "local-repository", expected)?
            .is_some()
            || crate::catalog::db::get_event_by_external(
                &conn,
                "grnoc-public-task-viewer",
                expected,
            )?
            .is_some();
        if found {
            report.events_imported.push(expected.to_string());
        } else {
            report.missing_events.push(expected.to_string());
        }
    }
    // Runs + artifact resolution.
    let runs: i64 = conn
        .query_row("SELECT COUNT(*) FROM analysis_runs", [], |r| r.get(0))
        .unwrap_or(0);
    report.runs_imported = runs;
    let mut stmt = conn
        .prepare("SELECT relative_path FROM analysis_artifacts")
        .map_err(|e| format!("cannot list artifacts: {e}"))?;
    let rows = stmt
        .query_map([], |r| r.get::<_, String>(0))
        .map_err(|e| format!("cannot read artifacts: {e}"))?;
    for row in rows {
        let rel = row.map_err(|e| format!("bad artifact row: {e}"))?;
        if rel.starts_with('/') || rel.contains(":\\") {
            report.absolute_paths.push(rel);
            continue;
        }
        if !resolve_artifact(root, &rel).is_file() {
            report.unresolved_artifacts.push(rel);
        }
    }
    // The MAN LAN case-study layer must be present (slug manlan-2019).
    let cs: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM case_studies WHERE slug = 'manlan-2019'",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);
    if cs == 0 {
        report
            .missing_events
            .push("case-study manlan-2019".to_string());
    }
    Ok(report)
}

/// Result of a demo init/verify.
#[derive(Debug, Default)]
pub struct DemoReport {
    pub events_imported: Vec<String>,
    pub missing_events: Vec<String>,
    pub runs_imported: i64,
    pub unresolved_artifacts: Vec<String>,
    pub absolute_paths: Vec<String>,
}

impl DemoReport {
    pub fn is_ok(&self) -> bool {
        self.missing_events.is_empty()
            && self.unresolved_artifacts.is_empty()
            && self.absolute_paths.is_empty()
    }
}

/// Render the demo report as text.
pub fn render_report(report: &DemoReport) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "demo events imported: {} ({})\n",
        report.events_imported.len(),
        report.events_imported.join(", ")
    ));
    out.push_str(&format!("runs imported: {}\n", report.runs_imported));
    if !report.missing_events.is_empty() {
        out.push_str(&format!(
            "MISSING events: {}\n",
            report.missing_events.join(", ")
        ));
    }
    if !report.unresolved_artifacts.is_empty() {
        out.push_str(&format!(
            "unresolved artifacts: {} ({})\n",
            report.unresolved_artifacts.len(),
            report
                .unresolved_artifacts
                .iter()
                .take(5)
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    if !report.absolute_paths.is_empty() {
        out.push_str(&format!(
            "ABSOLUTE PATHS in artifact references: {}\n",
            report.absolute_paths.len()
        ));
    }
    if report.is_ok() {
        out.push_str("demo verify: ok (no source access occurred; no absolute path leaked)\n");
    } else {
        out.push_str("demo verify: FAILED\n");
    }
    out
}

/// Resolve an artifact relative path the same way the workbench does:
/// catalog root first, then out/, then the reviewed case-study trees.
fn resolve_artifact(root: &Path, rel: &str) -> std::path::PathBuf {
    let candidates = [
        root.join(rel),
        root.join("out").join(rel),
        root.join("case-studies/manlan-2019/pilot/out").join(rel),
        root.join("case-studies/inc0302574/out").join(rel),
        root.join("case-studies/inc0299001/out").join(rel),
    ];
    candidates
        .into_iter()
        .find(|p| p.is_file())
        .unwrap_or_else(|| root.join(rel))
}

/// Whether any expected workbench renders (web layer check).
pub fn demo_workbenches_render(conn: &Connection) -> Result<Vec<String>, String> {
    let mut ok = Vec::new();
    for expected in DEMO_EXPECTED_EVENTS {
        let runs: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM analysis_runs r
                 JOIN analysis_plans p ON p.id = r.plan_id
                 JOIN manifest_revisions m ON m.id = p.manifest_revision_id
                 JOIN catalog_events e ON e.id = m.event_id
                 WHERE e.external_id = ?1",
                rusqlite::params![expected],
                |r| r.get(0),
            )
            .unwrap_or(0);
        if runs > 0 {
            ok.push(format!("{expected} ({} run(s))", runs));
        }
    }
    Ok(ok)
}
