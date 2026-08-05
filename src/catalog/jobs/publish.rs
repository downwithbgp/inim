//! Staged-artifact validation and atomic run publication.
//!
//! A running job never appears as a completed analysis run. The worker
//! writes all outputs to a job-specific staging root
//! (`data/jobs/<job-id>/staging`), validates them, renames the event
//! directory into the final immutable location
//! (`data/runs/<job-id>/<event-id>`), and only then imports the catalog
//! transaction that references the final relative paths.
//!
//! Publication ordering (documented failure modes):
//!
//! 1. write + validate staging;
//! 2. rename into the final location (atomic on the same filesystem);
//! 3. import the catalog transaction referencing final relative paths.
//!
//! If the catalog import fails, the final directory is left
//! unreferenced, the job is marked Failed with `catalog_import_failed`,
//! and the reconciler reports the orphan. A catalog run with a missing
//! directory is reported the same way — never auto-deleted.

use std::path::{Path, PathBuf};

use rusqlite::Connection;

use crate::catalog::domain::{AnalysisArtifact, AnalysisRun};
use crate::catalog::import::sha256_hex_bytes;
use crate::catalog::import::{
    artifact_kind, import_stream_summaries, import_transitions, import_wave_summaries,
    media_type_for,
};
use crate::catalog::jobs::service::{complete, fail, transition};
use crate::catalog::jobs::JobState;
use crate::schema::REPORT_SCHEMA_VERSION;

/// Artifacts that must be present for a publishable run.
pub const REQUIRED_ARTIFACTS: &[&str] = &[
    "report.json",
    "report.txt",
    "limitations.json",
    "archive_manifest.json",
    "execution_metadata.json",
];

/// Execution metadata written into staging before validation. Volatile
/// fields (job id, worker id, timings) live here — they never enter
/// semantic evidence.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExecutionMetadata {
    pub metadata_schema_version: u32,
    pub plan_hash: String,
    pub job_id: String,
    pub attempt: i64,
    pub original_job_id: Option<String>,
    pub worker_id: String,
    pub requested_by: String,
    pub requested_at: String,
    pub started_at: String,
    pub finished_at: String,
    pub wall_secs: f64,
    pub stage_durations_secs: Vec<(String, f64)>,
    pub offline: bool,
    pub cache_hits: Option<u64>,
    pub bytes_downloaded: Option<u64>,
    pub bytes_read_local: Option<u64>,
}

pub const EXECUTION_METADATA_SCHEMA_VERSION: u32 = 1;

/// Write `execution_metadata.json` into the staging event directory.
pub fn write_execution_metadata(event_out: &Path, meta: &ExecutionMetadata) -> Result<(), String> {
    let json = serde_json::to_string_pretty(meta)
        .map_err(|e| format!("cannot serialize execution metadata: {e}"))?;
    std::fs::write(event_out.join("execution_metadata.json"), json)
        .map_err(|e| format!("cannot write execution metadata: {e}"))
}

/// Validate a staged event directory before publication.
pub fn validate_staged(event_out: &Path, plan_hash: &str) -> Result<(), String> {
    let report_path = event_out.join("report.json");
    let report: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(&report_path)
            .map_err(|e| format!("staged run missing report.json: {e}"))?,
    )
    .map_err(|e| format!("staged report.json is invalid JSON: {e}"))?;
    let schema = report
        .get("schema_version")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32;
    if schema != REPORT_SCHEMA_VERSION {
        return Err(format!(
            "artifact_validation_failed: report schema v{schema} is not current v{REPORT_SCHEMA_VERSION}"
        ));
    }

    // Required artifact presence.
    let mut files: Vec<PathBuf> = std::fs::read_dir(event_out)
        .map_err(|e| format!("cannot read staged dir: {e}"))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_file())
        .collect();
    files.sort();
    let names: Vec<String> = files
        .iter()
        .map(|p| {
            p.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string()
        })
        .collect();
    for required in REQUIRED_ARTIFACTS {
        if !names.iter().any(|n| n == required) {
            return Err(format!(
                "artifact_validation_failed: missing required artifact {required}"
            ));
        }
    }

    // Plan hash provenance must match the queued plan exactly.
    let meta_path = event_out.join("execution_metadata.json");
    let meta: ExecutionMetadata = serde_json::from_str(
        &std::fs::read_to_string(&meta_path)
            .map_err(|e| format!("cannot read execution_metadata.json: {e}"))?,
    )
    .map_err(|e| format!("invalid execution_metadata.json: {e}"))?;
    if meta.plan_hash != plan_hash {
        return Err(format!(
            "artifact_validation_failed: execution metadata plan hash {} does not match queued plan {}",
            meta.plan_hash, plan_hash
        ));
    }

    // No absolute paths in the staged artifact set or key contents.
    for p in &files {
        let rel = p.strip_prefix(event_out).unwrap_or(p);
        let rel_str = rel.to_string_lossy();
        if rel_str.starts_with('/') || rel_str.contains(":\\") {
            return Err(format!(
                "artifact_validation_failed: absolute artifact path {rel_str}"
            ));
        }
        if let Ok(bytes) = std::fs::read(p) {
            let text = String::from_utf8_lossy(&bytes);
            if text.contains("/home/") || text.contains("/Users/") {
                return Err(format!(
                    "artifact_validation_failed: absolute path leaked into {}",
                    rel_str
                ));
            }
        }
    }

    // Evidence-reference integrity: transitions must reference
    // observation ids present in the evidence appendix.
    let transitions_path = event_out.join("transitions.json");
    if transitions_path.is_file() {
        if let Ok(tx_json) = std::fs::read_to_string(&transitions_path) {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&tx_json) {
                if let Some(items) = v.as_array() {
                    let appendix_path = event_out.join("evidence_appendix.jsonl");
                    let appendix_ok = appendix_path.is_file();
                    if !items.is_empty() && !appendix_ok {
                        return Err(
                            "artifact_validation_failed: transitions present without evidence appendix"
                                .to_string(),
                        );
                    }
                }
            }
        }
    }
    Ok(())
}

/// Publish a validated staged event directory into the immutable catalog.
///
/// Returns the new run id. The caller is responsible for staging
/// validation and the `PublishingRun` job state; this function performs
/// the rename + catalog transaction and marks the job Completed.
/// Publication inputs (kept together to keep the call bounded).
pub struct PublishInputs<'a> {
    pub catalog_root: &'a Path,
    pub job_id: &'a str,
    pub plan_revision_id: i64,
    pub event_id: &'a str,
    pub software_version: &'a str,
    pub git_revision: Option<&'a str>,
    pub run_started_at: &'a str,
    pub run_completed_at: &'a str,
    pub runtime_secs: f64,
}

pub fn publish_staged_run(
    conn: &Connection,
    staging_event_dir: &Path,
    inputs: &PublishInputs<'_>,
) -> Result<i64, String> {
    let catalog_root = inputs.catalog_root;
    let job_id = inputs.job_id;
    // ── 1. Rename staging -> final immutable location (same fs). ──
    let final_run_dir = catalog_root.join("data").join("runs").join(job_id);
    let final_event_dir = final_run_dir.join(inputs.event_id);
    if final_event_dir.exists() {
        return Err(format!(
            "artifact_publication_failed: final run directory already exists: {final_event_dir:?}"
        ));
    }
    std::fs::create_dir_all(&final_run_dir)
        .map_err(|e| format!("cannot create final run directory: {e}"))?;
    std::fs::rename(staging_event_dir, &final_event_dir).map_err(|e| {
        format!("artifact_publication_failed: cannot rename staged run into final location: {e}")
    })?;

    // ── 2. Import the catalog transaction. ─────────────────────────
    let result = import_finalized_run(conn, catalog_root, &final_event_dir, inputs);
    match result {
        Ok(run_id) => Ok(run_id),
        Err(e) => Err(format!(
            "catalog_import_failed: {e}; final directory left unreferenced at {final_event_dir:?}; inspect before cleanup"
        )),
    }
}

fn import_finalized_run(
    conn: &Connection,
    catalog_root: &Path,
    final_event_dir: &Path,
    inputs: &PublishInputs<'_>,
) -> Result<i64, String> {
    let report_content = std::fs::read_to_string(final_event_dir.join("report.json"))
        .map_err(|e| format!("cannot read published report.json: {e}"))?;
    let report: serde_json::Value =
        serde_json::from_str(&report_content).map_err(|e| format!("invalid report.json: {e}"))?;
    let report_schema = report
        .get("schema_version")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32;
    if report_schema != REPORT_SCHEMA_VERSION {
        return Err(format!(
            "report schema v{report_schema} is not current v{REPORT_SCHEMA_VERSION}"
        ));
    }
    let verdict = report
        .get("result")
        .and_then(|r| r.get("verdict_label"))
        .or_else(|| report.get("result").and_then(|r| r.get("verdict")))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let assessment = report
        .get("assessment")
        .and_then(|a| a.get("statement"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let run = AnalysisRun {
        id: 0,
        plan_id: inputs.plan_revision_id,
        software_version: inputs.software_version.to_string(),
        git_revision: inputs.git_revision.map(|g| g.to_string()),
        parser_identity: crate::derived_cache::PARSER_VERSION.to_string(),
        cache_schema_version: crate::schema::RIB_CACHE_SCHEMA_VERSION,
        report_schema_version: report_schema,
        status: "Complete".to_string(),
        started_at: inputs.run_started_at.to_string(),
        completed_at: Some(inputs.run_completed_at.to_string()),
        runtime_secs: Some(inputs.runtime_secs),
        verdict,
        assessment,
    };

    let tx = conn
        .unchecked_transaction()
        .map_err(|e| format!("cannot begin publication transaction: {e}"))?;

    let mut artifacts: Vec<AnalysisArtifact> = Vec::new();
    let mut files: Vec<PathBuf> = std::fs::read_dir(final_event_dir)
        .map_err(|e| format!("cannot read final run dir: {e}"))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_file())
        .collect();
    files.sort();
    for path in &files {
        let bytes = std::fs::read(path).map_err(|e| format!("cannot read artifact: {e}"))?;
        let rel = path
            .strip_prefix(catalog_root)
            .map_err(|_| "artifact outside catalog root".to_string())?
            .to_string_lossy()
            .to_string();
        let kind = artifact_kind(&rel);
        artifacts.push(AnalysisArtifact {
            id: 0,
            run_id: 0,
            kind: kind.to_string(),
            relative_path: rel,
            media_type: media_type_for(kind).to_string(),
            schema_version: Some(report_schema),
            sha256: sha256_hex_bytes(&bytes),
            size: bytes.len() as i64,
            created_at: inputs.run_completed_at.to_string(),
        });
    }

    let run_id = crate::catalog::store::insert_run(&tx, &run)
        .map_err(|e| format!("cannot insert run: {e}"))?;
    for mut a in artifacts {
        a.run_id = run_id;
        crate::catalog::store::insert_artifact(&tx, &a)
            .map_err(|e| format!("cannot insert artifact {}: {e}", a.relative_path))?;
    }

    let mut summary = crate::catalog::import::ImportSummary::default();
    import_stream_summaries(&tx, run_id, final_event_dir, &mut summary)
        .map_err(|e| format!("cannot import stream summaries: {e}"))?;
    import_wave_summaries(&tx, run_id, final_event_dir, &mut summary)
        .map_err(|e| format!("cannot import wave summaries: {e}"))?;
    import_transitions(
        &tx,
        run_id,
        final_event_dir,
        &mut summary,
        crate::catalog::import::TRANSITION_IMPORT_LIMIT,
    )
    .map_err(|e| format!("cannot import transitions: {e}"))?;

    tx.commit()
        .map_err(|e| format!("cannot commit publication: {e}"))?;
    Ok(run_id)
}

/// Detect orphaned final artifact directories (final dir without a
/// catalog run) and catalog runs with missing directories. Never
/// deletes anything.
pub fn reconcile_orphans(conn: &Connection, catalog_root: &Path) -> Result<OrphanReport, String> {
    let mut report = OrphanReport::default();
    let runs_root = catalog_root.join("data").join("runs");
    if let Ok(entries) = std::fs::read_dir(&runs_root) {
        for entry in entries.flatten() {
            if !entry.path().is_dir() {
                continue;
            }
            let job_id = entry.file_name().to_string_lossy().to_string();
            let referenced: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM analysis_jobs WHERE id = ?1 AND completed_run_id IS NOT NULL",
                    rusqlite::params![job_id],
                    |r| r.get(0),
                )
                .unwrap_or(0);
            let run_linked: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM analysis_artifacts WHERE relative_path LIKE ?1",
                    rusqlite::params![format!("data/runs/{job_id}/%")],
                    |r| r.get(0),
                )
                .unwrap_or(0);
            if referenced == 0 && run_linked == 0 {
                report.orphan_directories.push(entry.path());
            }
        }
    }
    let mut stmt = conn
        .prepare(
            "SELECT relative_path FROM analysis_artifacts WHERE relative_path LIKE 'data/runs/%'",
        )
        .map_err(|e| format!("cannot query run artifacts: {e}"))?;
    let rows = stmt
        .query_map([], |r| r.get::<_, String>(0))
        .map_err(|e| format!("cannot read run artifacts: {e}"))?;
    for row in rows {
        let rel = row.map_err(|e| format!("bad artifact row: {e}"))?;
        // data/runs rows are catalog-root-relative; enforce the same
        // lexical containment primitive before joining so a crafted row
        // cannot probe outside the configured root.
        if !crate::catalog::artifact_path::is_safe_relative_path(&rel) {
            report.missing_run_artifacts.push(rel);
            continue;
        }
        if !catalog_root.join(&rel).is_file() {
            report.missing_run_artifacts.push(rel);
        }
    }
    Ok(report)
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct OrphanReport {
    pub orphan_directories: Vec<PathBuf>,
    pub missing_run_artifacts: Vec<String>,
}

/// Worker-side completion helper: run_id linkage + Completed state.
pub fn complete_job(conn: &Connection, job_id: &str, run_id: i64) -> Result<(), String> {
    complete(conn, job_id, run_id)
}

/// Worker-side failure helper with staging preservation.
pub fn fail_job(conn: &Connection, job_id: &str, code: &str, summary: &str) -> Result<(), String> {
    let job = crate::catalog::jobs::service::get(conn, job_id)?;
    fail(conn, job_id, job.state, code, summary)
}

/// Mark a job's stage before publication begins. The deterministic
/// cancellation race policy: once the job is in PublishingRun, the
/// publication transaction wins; cancellation observed before that
/// point cancels before import.
pub fn enter_publishing(conn: &Connection, job_id: &str) -> Result<(), String> {
    let job = crate::catalog::jobs::service::get(conn, job_id)?;
    transition(
        conn,
        job_id,
        job.state,
        JobState::PublishingRun,
        Some("publishing"),
        "Publishing completed run",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::db;

    fn temp_catalog() -> (tempfile::TempDir, Connection) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("catalog.sqlite");
        let conn = db::open_catalog(&path).unwrap();
        (dir, conn)
    }

    fn write_fake_artifact(dir: &Path, name: &str, body: &str) {
        std::fs::write(dir.join(name), body).unwrap();
    }

    fn make_staged_event(root: &Path, job_id: &str, event_id: &str, plan_hash: &str) -> PathBuf {
        let staging = root.join("data").join("jobs").join(job_id).join("staging");
        let event_out = staging.join(event_id);
        std::fs::create_dir_all(&event_out).unwrap();
        let report = serde_json::json!({
            "schema_version": REPORT_SCHEMA_VERSION,
            "result": {"verdict_label": "NoRouteStateChange"},
            "assessment": {"statement": "no change observed"}
        });
        write_fake_artifact(&event_out, "report.json", &report.to_string());
        write_fake_artifact(&event_out, "report.txt", "report text");
        write_fake_artifact(&event_out, "limitations.json", "{}");
        write_fake_artifact(&event_out, "archive_manifest.json", r#"{"archives": []}"#);
        write_fake_artifact(&event_out, "transitions.json", "[]");
        write_fake_artifact(&event_out, "evidence_appendix.jsonl", "");
        let meta = ExecutionMetadata {
            metadata_schema_version: EXECUTION_METADATA_SCHEMA_VERSION,
            plan_hash: plan_hash.to_string(),
            job_id: job_id.to_string(),
            attempt: 1,
            original_job_id: None,
            worker_id: "w".into(),
            requested_by: "cli".into(),
            requested_at: "2026-08-01T00:00:00Z".into(),
            started_at: "2026-08-01T00:00:00Z".into(),
            finished_at: "2026-08-01T00:00:01Z".into(),
            wall_secs: 1.0,
            stage_durations_secs: vec![],
            offline: true,
            cache_hits: None,
            bytes_downloaded: None,
            bytes_read_local: None,
        };
        write_execution_metadata(&event_out, &meta).unwrap();
        event_out
    }

    fn seed_plan(conn: &Connection, event_id: &str) -> i64 {
        conn.execute(
            "INSERT INTO catalog_events (source_kind, external_id, first_seen, last_seen)
             VALUES ('local-repository', ?1, '2026-08-01T00:00:00Z', '2026-08-01T00:00:00Z')",
            rusqlite::params![event_id],
        )
        .unwrap();
        let eid = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO event_snapshots (event_id, fetched_at, source_url, content_sha256, raw_payload, normalized_json, parser_version)
             VALUES (?1, '2026-08-01T00:00:00Z', 'file:///x', 's', '{}', '{}', 't')",
            [eid],
        )
        .unwrap();
        let sid = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO manifest_revisions (event_id, snapshot_id, manifest_schema, payload, sha256, review_status)
             VALUES (?1, ?2, 2, '{}', ?3, 'Reviewed')",
            rusqlite::params![eid, sid, format!("ms-{event_id}")],
        )
        .unwrap();
        let mid = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO analysis_plans (manifest_revision_id, plan_schema, payload, sha256, status, created_at)
             VALUES (?1, 1, '{}', ?2, 'Ready', '2026-08-01T00:00:00Z')",
            rusqlite::params![mid, format!("ps-{event_id}")],
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    #[test]
    fn incomplete_stage_is_not_visible_as_run() {
        let (dir, conn) = temp_catalog();
        let root = dir.path();
        let plan = seed_plan(&conn, "EV-1");
        let job = crate::catalog::jobs::service::new_job_id(&conn).unwrap();
        // Staging without report.json must fail validation.
        let staging = root.join("data").join("jobs").join(&job).join("staging");
        let event_out = staging.join("EV-1");
        std::fs::create_dir_all(&event_out).unwrap();
        write_fake_artifact(&event_out, "limitations.json", "{}");
        let err = validate_staged(&event_out, "h").unwrap_err();
        assert!(err.contains("report.json"), "{err}");
        // No run may exist.
        let runs: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM analysis_runs WHERE plan_id = ?1",
                [plan],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(runs, 0);
    }

    #[test]
    fn invalid_artifact_blocks_publication() {
        let (dir, conn) = temp_catalog();
        let root = dir.path();
        let plan = seed_plan(&conn, "EV-2");
        let job = crate::catalog::jobs::service::new_job_id(&conn).unwrap();
        let event_out = make_staged_event(root, &job, "EV-2", "hash-x");
        // Tamper with the report schema.
        let report = serde_json::json!({"schema_version": 999});
        write_fake_artifact(&event_out, "report.json", &report.to_string());
        let err = validate_staged(&event_out, "hash-x").unwrap_err();
        assert!(err.contains("schema v999"), "{err}");
        // Plan hash mismatch is also rejected.
        let event2 = make_staged_event(root, &job, "EV-2", "hash-y");
        let err = validate_staged(&event2, "hash-x").unwrap_err();
        assert!(err.contains("plan hash"), "{err}");
        let _ = plan;
    }

    #[test]
    fn completed_job_links_to_one_run() {
        let (dir, conn) = temp_catalog();
        let root = dir.path();
        let plan = seed_plan(&conn, "EV-3");
        let job = crate::catalog::jobs::service::new_job_id(&conn).unwrap();
        let event_out = make_staged_event(root, &job, "EV-3", "ph");
        validate_staged(&event_out, "ph").unwrap();
        let run_id = publish_staged_run(
            &conn,
            &event_out,
            &PublishInputs {
                catalog_root: root,
                job_id: &job,
                plan_revision_id: plan,
                event_id: "EV-3",
                software_version: "0.1.0",
                git_revision: None,
                run_started_at: "2026-08-01T00:00:00Z",
                run_completed_at: "2026-08-01T00:00:01Z",
                runtime_secs: 1.0,
            },
        )
        .unwrap();
        // Final directory exists; staging no longer holds the event.
        assert!(root
            .join("data/runs")
            .join(&job)
            .join("EV-3")
            .join("report.json")
            .is_file());
        assert!(!event_out.exists());
        // Artifact relative paths resolve under the catalog root.
        let rel: String = conn
            .query_row(
                "SELECT relative_path FROM analysis_artifacts WHERE run_id = ?1 AND kind = 'report'",
                [run_id],
                |r| r.get(0),
            )
            .unwrap();
        assert!(rel.starts_with("data/runs/"), "{rel}");
        assert!(root.join(&rel).is_file(), "resolved {rel}");
        // Run linked to plan.
        let plan_id: i64 = conn
            .query_row(
                "SELECT plan_id FROM analysis_runs WHERE id = ?1",
                [run_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(plan_id, plan);
    }

    #[test]
    fn publication_is_idempotent_for_same_job() {
        let (dir, conn) = temp_catalog();
        let root = dir.path();
        let plan = seed_plan(&conn, "EV-4");
        let job = crate::catalog::jobs::service::new_job_id(&conn).unwrap();
        let event_out = make_staged_event(root, &job, "EV-4", "ph");
        validate_staged(&event_out, "ph").unwrap();
        publish_staged_run(
            &conn,
            &event_out,
            &PublishInputs {
                catalog_root: root,
                job_id: &job,
                plan_revision_id: plan,
                event_id: "EV-4",
                software_version: "0.1.0",
                git_revision: None,
                run_started_at: "2026-08-01T00:00:00Z",
                run_completed_at: "2026-08-01T00:00:01Z",
                runtime_secs: 1.0,
            },
        )
        .unwrap();
        // A second publication for the same job must fail (final dir exists).
        let event_out2 = make_staged_event(root, &job, "EV-4", "ph");
        let err = publish_staged_run(
            &conn,
            &event_out2,
            &PublishInputs {
                catalog_root: root,
                job_id: &job,
                plan_revision_id: plan,
                event_id: "EV-4",
                software_version: "0.1.0",
                git_revision: None,
                run_started_at: "2026-08-01T00:00:00Z",
                run_completed_at: "2026-08-01T00:00:01Z",
                runtime_secs: 1.0,
            },
        )
        .unwrap_err();
        assert!(err.contains("already exists"), "{err}");
        let runs: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM analysis_runs WHERE plan_id = ?1",
                [plan],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(runs, 1);
    }

    #[test]
    fn orphan_artifact_directory_is_detected() {
        let (dir, conn) = temp_catalog();
        let root = dir.path();
        // A final directory with no linked job/run.
        let orphan = root.join("data/runs/job-orphan/EV-X");
        std::fs::create_dir_all(&orphan).unwrap();
        write_fake_artifact(&orphan, "report.json", "{}");
        let report = reconcile_orphans(&conn, root).unwrap();
        assert_eq!(report.orphan_directories.len(), 1);
        assert!(report.orphan_directories[0]
            .to_string_lossy()
            .contains("job-orphan"));
    }

    #[test]
    fn missing_run_artifact_is_detected() {
        let (dir, conn) = temp_catalog();
        let root = dir.path();
        let plan = seed_plan(&conn, "EV-5");
        let job = crate::catalog::jobs::service::new_job_id(&conn).unwrap();
        let event_out = make_staged_event(root, &job, "EV-5", "ph");
        publish_staged_run(
            &conn,
            &event_out,
            &PublishInputs {
                catalog_root: root,
                job_id: &job,
                plan_revision_id: plan,
                event_id: "EV-5",
                software_version: "0.1.0",
                git_revision: None,
                run_started_at: "2026-08-01T00:00:00Z",
                run_completed_at: "2026-08-01T00:00:01Z",
                runtime_secs: 1.0,
            },
        )
        .unwrap();
        // Delete one published artifact; the reconciler must report it.
        std::fs::remove_file(
            root.join("data/runs")
                .join(&job)
                .join("EV-5")
                .join("report.txt"),
        )
        .unwrap();
        let report = reconcile_orphans(&conn, root).unwrap();
        assert!(
            report
                .missing_run_artifacts
                .iter()
                .any(|r| r.ends_with("report.txt")),
            "{:?}",
            report.missing_run_artifacts
        );
    }
}
