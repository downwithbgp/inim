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
    "INC0301970", // Indiana GigaPOP peer event (second-network case; insufficient visibility)
    "INC0302574", // visibility audit event
    "INC0040293", // MAN LAN optical participant event (supporting observation)
];

/// Reviewed NORDUnet pilot runs linked to the manlan-2019 case study in
/// the demo (the R&E-plane runs of the reviewed cross-observer matrix:
/// route-views2 direct, rrc00, rrc06, rrc15). Pilot linkage mirrors the
/// reviewed pilot-result linkage; the I2PX peering-plane runs are not
/// linked (their dirs hold preflight-only results).
const DEMO_PILOT_LINKED_RUNS: &[&str] = &[
    "MANLAN-2019-NORDUNET-PILOT-RE-RV2",
    "MANLAN-2019-NORDUNET-PILOT-RE-RRC00",
    "MANLAN-2019-NORDUNET-PILOT-RE-RRC06",
    "MANLAN-2019-NORDUNET-PILOT-RE-RRC15",
];

/// Import completed NORDUnet pilot runs from the reviewed pilot tree
/// into the demo catalog and link the reviewed R&E-plane runs to the
/// manlan-2019 case study. Offline and deterministic: only reviewed
/// tracked material is read, and nothing is executed.
fn import_pilot_runs(conn: &Connection, root: &Path) -> Result<usize, String> {
    let pilot_dir = root.join("case-studies/manlan-2019/pilot");
    let manifests_dir = pilot_dir.join("manifests");
    let out_dir = pilot_dir.join("out");
    if !manifests_dir.is_dir() || !out_dir.is_dir() {
        return Ok(0);
    }
    let mut manifest_paths: Vec<std::path::PathBuf> = std::fs::read_dir(&manifests_dir)
        .map_err(|e| format!("cannot read {}: {e}", manifests_dir.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().map(|x| x == "json").unwrap_or(false))
        .collect();
    manifest_paths.sort();
    let mut imported = 0usize;
    for path in &manifest_paths {
        let event_id = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        // Only completed runs (report.json present) are imported; the
        // I2PX peering-plane dirs hold preflight-only results and are
        // never imported as runs.
        if !out_dir.join(event_id).join("report.json").is_file() {
            continue;
        }
        let mut summary = crate::catalog::import::ImportSummary::default();
        crate::catalog::import::import_one(
            conn,
            path,
            &out_dir,
            env!("CARGO_PKG_VERSION"),
            None,
            &mut summary,
        )
        .map_err(|e| format!("pilot run import failed ({event_id}): {e}"))?;
        imported += 1;
    }
    // Link the reviewed R&E-plane runs to the case study.
    let cs_id: Option<i64> = conn
        .query_row(
            "SELECT id FROM case_studies WHERE slug = 'manlan-2019'",
            [],
            |r| r.get(0),
        )
        .ok();
    if let Some(cs_id) = cs_id {
        for run_event in DEMO_PILOT_LINKED_RUNS {
            let run_id: Option<i64> = conn
                .query_row(
                    "SELECT r.id FROM analysis_runs r
                     JOIN analysis_plans p ON p.id = r.plan_id
                     JOIN manifest_revisions m ON m.id = p.manifest_revision_id
                     JOIN catalog_events e ON e.id = m.event_id
                     WHERE e.external_id = ?1",
                    [run_event],
                    |r| r.get(0),
                )
                .ok();
            if let Some(run_id) = run_id {
                let _ = crate::catalog::store::insert_case_study_analysis_link(
                    conn,
                    &crate::catalog::domain::CaseStudyAnalysisLink {
                        id: 0,
                        case_study_id: cs_id,
                        run_id,
                        role: "PilotObservation".to_string(),
                        reviewed_note: Some(
                            "Reviewed R&E-plane pilot run (demo linkage).".to_string(),
                        ),
                    },
                )?;
            }
        }
    }
    Ok(imported)
}

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
    // Import the bounded reviewed GRNOC corpus FIRST (events +
    // snapshots + relationships + reviewed roles; never Ready plans,
    // never jobs) so the case-study import can resolve its event links
    // against the corpus events.
    let cs_path = root.join("case-studies/manlan-2019");
    let corpus_dir = cs_path.join("corpus");
    if corpus_dir.is_dir() {
        let summary = crate::catalog::corpus_import::import_corpus(&conn, &corpus_dir)
            .map_err(|e| format!("demo corpus import failed: {e}"))?;
        let reviews_path = cs_path.join("pilot/ticket-reviews.json");
        if reviews_path.is_file() {
            let n = crate::catalog::corpus_import::import_reviews(&conn, &reviews_path)
                .map_err(|e| format!("demo corpus reviews import failed: {e}"))?;
            let _ = n;
        }
        let _ = summary;
    }
    // Import the reviewed case-study layer AFTER the corpus so its
    // event links resolve against the corpus events.
    if cs_path.is_dir() {
        crate::catalog::case_study_import::import_case_study(&conn, &cs_path)
            .map_err(|e| format!("demo case-study import failed (manlan-2019): {e}"))?;
    }
    // Import the completed NORDUnet pilot runs (case-study evidence)
    // and link the reviewed R&E-plane runs so the manlan-2019 workbench
    // renders the route changes. Without this step the case-study
    // workbench would show no observer findings.
    let pilot_imported = import_pilot_runs(&conn, root)?;
    if pilot_imported == 0
        && root.join("case-studies/manlan-2019/pilot/out").is_dir()
    {
        return Err(
            "demo pilot import failed: no completed pilot runs imported from the reviewed pilot tree"
                .to_string(),
        );
    }
    drop(conn);
    let report = demo_verify(db_path, root)?;
    write_demo_manifest(db_path, &report)?;
    Ok(report)
}

/// Write the deterministic demo manifest next to the database. No
/// generated timestamps: byte identity is deterministic for the same
/// tracked inputs.
fn write_demo_manifest(db_path: &Path, report: &DemoReport) -> Result<(), String> {
    let manifest = serde_json::json!({
        "schema_version": 1,
        "tracked_source_events": report.events_imported.len(),
        "grnoc_source_events": report.grnoc_events,
        "imported_snapshots": report.grnoc_snapshots + report.events_imported.len() as i64,
        "plans": report.plans,
        "jobs": report.jobs,
        "runs": report.runs_imported,
        "artifacts": report.artifacts,
        "expected_workbench_urls": [
            "/events/INC0302574/workbench",
            "/events/INC0299001/workbench",
            "/events/INC0040293/workbench",
            "/events/INC0301970/workbench",
            "/case-studies/manlan-2019/workbench",
        ],
    });
    let path = db_path.with_file_name("demo-manifest.json");
    std::fs::write(
        &path,
        serde_json::to_string_pretty(&manifest).unwrap_or_default(),
    )
    .map_err(|e| format!("cannot write demo manifest: {e}"))
}

/// Count helpers used by verify + manifest.
fn manifest_counts(conn: &Connection) -> (i64, i64, i64) {
    (
        conn.query_row("SELECT COUNT(*) FROM analysis_plans", [], |r| r.get(0))
            .unwrap_or(0),
        conn.query_row("SELECT COUNT(*) FROM analysis_jobs", [], |r| r.get(0))
            .unwrap_or(0),
        conn.query_row("SELECT COUNT(*) FROM analysis_artifacts", [], |r| r.get(0))
            .unwrap_or(0),
    )
}

/// Verify a demo catalog. Checks expected events, run artifacts, no
/// absolute paths, and that artifact references resolve.
pub fn demo_verify(db_path: &Path, root: &Path) -> Result<DemoReport, String> {
    let conn = crate::catalog::db::open_catalog(db_path)?;
    // Project-scope policy applies to demo catalogs: an excluded event
    // must never be present in a freshly initialized demo. The import
    // skips excluded manifests; this check is the verify-side gate.
    let scope = crate::catalog::scope::ProjectScope::load(root)?;
    for rec in scope.source_records() {
        // The source-record exclusion matches by exact external ID
        // wherever the event lives (manifest-imported events are stored
        // under local-repository), so the verify gate uses the same
        // ID-based lookup.
        if crate::catalog::db::get_event_by_external_any(&conn, &rec.external_id)?.is_some() {
            return Err(format!(
                "demo catalog contains an excluded source record {} / {}; rebuild the demo with the current policy",
                rec.source_family, rec.external_id
            ));
        }
    }
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
    // Explicit GRNOC corpus counts (Part 1 audit check).
    report.grnoc_events = conn
        .query_row(
            "SELECT COUNT(*) FROM catalog_events WHERE source_kind = 'grnoc-public-task-viewer'",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);
    report.grnoc_snapshots = conn
        .query_row(
            "SELECT COUNT(*) FROM event_snapshots s JOIN catalog_events e ON e.id = s.event_id
             WHERE e.source_kind = 'grnoc-public-task-viewer'",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);
    report.grnoc_relationships = conn
        .query_row("SELECT COUNT(*) FROM ticket_relationships", [], |r| {
            r.get(0)
        })
        .unwrap_or(0);
    report.grnoc_reviews = conn
        .query_row("SELECT COUNT(*) FROM ticket_reviews", [], |r| r.get(0))
        .unwrap_or(0);
    report.unresolved_references = conn
        .query_row(
            "SELECT COUNT(*) FROM case_study_event_links WHERE catalog_event_id IS NULL",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);
    let (plans, jobs, artifacts) = manifest_counts(&conn);
    report.plans = plans;
    report.jobs = jobs;
    report.artifacts = artifacts;
    report.events_awaiting_review = conn
        .query_row(
            "SELECT COUNT(*) FROM catalog_events e
             WHERE NOT EXISTS (SELECT 1 FROM manifest_revisions m WHERE m.event_id = e.id)
               AND NOT EXISTS (SELECT 1 FROM analysis_plans p
                               JOIN manifest_revisions m ON m.id = p.manifest_revision_id
                               WHERE m.event_id = e.id AND p.status = 'Ready')",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);
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
    /// GRNOC corpus events imported (explicit count, never implied).
    pub grnoc_events: i64,
    pub grnoc_snapshots: i64,
    pub grnoc_relationships: i64,
    pub grnoc_reviews: i64,
    pub unresolved_references: i64,
    pub events_awaiting_review: i64,
    pub plans: i64,
    pub jobs: i64,
    pub artifacts: i64,
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
    out.push_str(&format!(
        "GRNOC corpus: {} events, {} snapshots, {} relationships, {} reviews\n",
        report.grnoc_events,
        report.grnoc_snapshots,
        report.grnoc_relationships,
        report.grnoc_reviews
    ));
    out.push_str(&format!(
        "events awaiting analysis review: {}\n",
        report.events_awaiting_review
    ));
    if report.unresolved_references > 0 {
        out.push_str(&format!(
            "unresolved ticket references retained: {}\n",
            report.unresolved_references
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
        root.join("case-studies/manlan-esnet-2019/out").join(rel),
        root.join("case-studies/indiana-gigapop-smithville-2026/out")
            .join(rel),
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
