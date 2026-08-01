//! Repository import — bring existing canonical manifests and analysis
//! artifacts into the catalog.
//!
//! The importer validates identities and hashes, imports inside one
//! transaction, and never reinterprets or recomputes analysis results.
//! A repeated identical import is idempotent; a conflicting artifact on
//! an existing immutable run rejects the whole import.

use rusqlite::Connection;
use std::path::{Path, PathBuf};

use super::domain::*;
use super::store;
use super::sync::hex_sha256;

/// Import summary.
#[derive(Debug, Default)]
pub struct ImportSummary {
    pub events: usize,
    pub snapshots: usize,
    pub manifests: usize,
    pub plans: usize,
    pub runs: usize,
    pub artifacts: usize,
    pub streams: usize,
    pub waves: usize,
}

/// Import all canonical manifests under `root/manifests/` and artifacts
/// under `root/out/<event_id>/` into the catalog.
pub fn import_repository(
    conn: &Connection,
    root: &Path,
    software_version: &str,
    git_revision: Option<&str>,
) -> Result<ImportSummary, String> {
    let manifests_dir = root.join("manifests");
    let out_dir = root.join("out");
    if !manifests_dir.is_dir() {
        return Err(format!(
            "cannot import: manifests directory {} not found",
            manifests_dir.display()
        ));
    }

    let mut manifest_paths: Vec<PathBuf> = std::fs::read_dir(&manifests_dir)
        .map_err(|e| format!("cannot read {}: {e}", manifests_dir.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().map(|x| x == "json").unwrap_or(false))
        .collect();
    manifest_paths.sort();

    let tx = conn
        .unchecked_transaction()
        .map_err(|e| format!("cannot start import transaction: {e}"))?;

    let mut summary = ImportSummary::default();
    for manifest_path in &manifest_paths {
        import_one(
            &tx,
            manifest_path,
            &out_dir,
            software_version,
            git_revision,
            &mut summary,
        )?;
    }

    tx.commit()
        .map_err(|e| format!("cannot commit import: {e}"))?;
    Ok(summary)
}

fn import_one(
    conn: &Connection,
    manifest_path: &Path,
    out_dir: &Path,
    software_version: &str,
    git_revision: Option<&str>,
    summary: &mut ImportSummary,
) -> Result<(), String> {
    let manifest = crate::manifest::Manifest::load(manifest_path)?;
    let event_id_str = manifest.event_id.clone();
    let manifest_payload =
        std::fs::read_to_string(manifest_path).map_err(|e| format!("cannot read manifest: {e}"))?;
    let manifest_sha = hex_sha256(&manifest_payload);

    // ── Source snapshot: prefer the ticket fixture; else derive. ──
    let fixture_path = ticket_fixture_for(&event_id_str);
    let (snapshot, snapshot_sha) = if let Some(fixture) = fixture_path {
        let raw = std::fs::read_to_string(&fixture)
            .map_err(|e| format!("cannot read fixture {}: {e}", fixture.display()))?;
        let sha = hex_sha256(&raw);
        let normalized = serde_json::json!({
            "id": event_id_str,
            "title": manifest.target.label,
            "source": "ticket-fixture",
            "start": manifest.event_window_utc.start,
            "end": manifest.event_window_utc.end,
        });
        (
            EventSnapshot {
                id: 0,
                event_id: 0,
                fetched_at: "2026-07-31T00:00:00Z".to_string(),
                source_url: format!("file://{}", fixture.display()),
                content_sha256: sha.clone(),
                raw_payload: raw,
                normalized_json: normalized.to_string(),
                parser_version: "fixture-1".to_string(),
            },
            sha,
        )
    } else {
        let raw = serde_json::json!({
            "id": event_id_str,
            "title": manifest.target.label,
            "origin_asns": manifest.target.origin_asns,
            "source": "manifest-derived",
            "start": manifest.event_window_utc.start,
            "end": manifest.event_window_utc.end,
        })
        .to_string();
        let sha = hex_sha256(&raw);
        (
            EventSnapshot {
                id: 0,
                event_id: 0,
                fetched_at: "2026-07-31T00:00:00Z".to_string(),
                source_url: format!("file://{}", manifest_path.display()),
                content_sha256: sha.clone(),
                raw_payload: raw,
                normalized_json: serde_json::json!({"id": event_id_str}).to_string(),
                parser_version: "manifest-derived-1".to_string(),
            },
            sha,
        )
    };

    let existed =
        super::db::get_event_by_external(conn, "local-repository", &event_id_str)?.is_some();
    let event_id = store::upsert_event(
        conn,
        "local-repository",
        &event_id_str,
        "2026-07-31T00:00:00Z",
    )?;
    if !existed {
        summary.events += 1;
    }

    let snapshot_id = store::insert_snapshot(conn, event_id, &snapshot)?;
    summary.snapshots += 1;

    let review_status = if manifest.target.transit_predicate.is_ready() {
        "Reviewed"
    } else {
        "Unresolved"
    };
    let revision = ManifestRevision {
        id: 0,
        event_id,
        snapshot_id,
        manifest_schema: crate::manifest::MANIFEST_SCHEMA_VERSION,
        payload: manifest_payload.clone(),
        sha256: manifest_sha,
        review_status: review_status.to_string(),
        reviewed_at: Some("2026-07-31T00:00:00Z".to_string()),
        reviewer: manifest
            .target
            .transit_predicate
            .provenance
            .as_ref()
            .map(|p| p.reviewed_by.clone()),
    };
    let manifest_revision_id = store::insert_manifest_revision(conn, &revision)?;
    summary.manifests += 1;

    // ── Plan ───────────────────────────────────────────────────────
    let plan = build_plan_record(conn, manifest_revision_id, &manifest, manifest_path)?;
    let plan_id = store::insert_plan(conn, &plan)?;
    summary.plans += 1;

    // ── Run + artifacts (completed analyses only) ─────────────────
    let event_out = out_dir.join(&event_id_str);
    if event_out.is_dir() {
        let report_path = event_out.join("report.json");
        if report_path.is_file() {
            let report_content = std::fs::read_to_string(&report_path)
                .map_err(|e| format!("cannot read report: {e}"))?;
            let report: serde_json::Value = serde_json::from_str(&report_content)
                .map_err(|e| format!("invalid report.json for {event_id_str}: {e}"))?;
            let report_schema = report
                .get("schema_version")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as u32;
            if report_schema != crate::schema::REPORT_SCHEMA_VERSION {
                return Err(format!(
                    "import rejected for {event_id_str}: report schema v{report_schema} is not current v{}",
                    crate::schema::REPORT_SCHEMA_VERSION
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
            let generated_at = report
                .get("outcome")
                .and_then(|o| o.get("assessment"))
                .and_then(|a| a.get("generated_at"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| "2026-07-31T00:00:00Z".to_string());

            let run = AnalysisRun {
                id: 0,
                plan_id,
                software_version: software_version.to_string(),
                git_revision: git_revision.map(|g| g.to_string()),
                parser_identity: crate::derived_cache::PARSER_VERSION.to_string(),
                cache_schema_version: crate::schema::RIB_CACHE_SCHEMA_VERSION,
                report_schema_version: report_schema,
                status: "Complete".to_string(),
                started_at: generated_at.clone(),
                completed_at: Some(generated_at),
                runtime_secs: None,
                verdict,
                assessment,
            };
            // ── Artifacts: validate hash + conflict policy ─────────
            // Runs first (before the idempotent short-circuit) so a
            // conflicting artifact on an existing immutable run rejects
            // the import instead of silently succeeding.
            let mut artifact_paths: Vec<PathBuf> = std::fs::read_dir(&event_out)
                .map_err(|e| format!("cannot read {}: {e}", event_out.display()))?
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.is_file())
                .collect();
            artifact_paths.sort();
            let mut artifacts: Vec<AnalysisArtifact> = Vec::new();
            for path in artifact_paths {
                let bytes =
                    std::fs::read(&path).map_err(|e| format!("cannot read artifact: {e}"))?;
                let sha = sha256_hex_bytes(&bytes);
                let rel = path
                    .strip_prefix(out_dir)
                    .map_err(|_| "artifact outside out/".to_string())?
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
                    sha256: sha,
                    size: bytes.len() as i64,
                    created_at: "2026-07-31T00:00:00Z".to_string(),
                });
            }

            // Idempotent short-circuit: an existing run for the exact
            // (plan, started_at) must still satisfy the conflict policy.
            let existing_run: Option<i64> = conn
                .query_row(
                    "SELECT id FROM analysis_runs WHERE plan_id = ?1 AND started_at = ?2",
                    rusqlite::params![plan_id, run.started_at],
                    |r| r.get(0),
                )
                .ok();
            if let Some(run_id) = existing_run {
                for a in &artifacts {
                    let existing_sha: Option<String> = conn
                        .query_row(
                            "SELECT sha256 FROM analysis_artifacts WHERE run_id = ?1 AND relative_path = ?2",
                            rusqlite::params![run_id, a.relative_path],
                            |r| r.get(0),
                        )
                        .ok();
                    if let Some(old) = existing_sha {
                        if old != a.sha256 {
                            return Err(format!(
                                "import rejected: artifact {} conflicts with an existing immutable run (hash mismatch)",
                                a.relative_path
                            ));
                        }
                    }
                }
                return Ok(());
            }
            let run_id = store::insert_run(conn, &run)?;
            summary.runs += 1;

            for mut a in artifacts {
                a.run_id = run_id;
                store::insert_artifact(conn, &a)?;
                summary.artifacts += 1;
            }

            // ── Stream + wave summaries from the artifact JSONs ────
            import_stream_summaries(conn, run_id, &event_out, summary)?;
            import_wave_summaries(conn, run_id, &event_out, summary)?;
        }
    }

    let _ = snapshot_sha;
    Ok(())
}

fn build_plan_record(
    _conn: &Connection,
    manifest_revision_id: i64,
    manifest: &crate::manifest::Manifest,
    _manifest_path: &Path,
) -> Result<AnalysisPlanRecord, String> {
    // Plan payload is deterministic: reviewed manifest identity + status.
    let status = if manifest.target.transit_predicate.is_ready() {
        "Ready"
    } else {
        "Blocked"
    };
    let block_reason = if status == "Blocked" {
        Some("MissingReviewedTransitPredicate".to_string())
    } else {
        None
    };
    let payload = serde_json::json!({
        "event_id": manifest.event_id,
        "manifest_schema": manifest.schema_version,
        "manifest_revision": manifest.revision,
        "status": status,
        "block_reason": block_reason,
        "origin_asns": manifest.target.origin_asns,
        "transit_predicate_status": format!("{:?}", manifest.target.transit_predicate.status),
    })
    .to_string();
    let sha = hex_sha256(&payload);
    Ok(AnalysisPlanRecord {
        id: 0,
        manifest_revision_id,
        plan_schema: crate::schema::ANALYSIS_PLAN_SCHEMA_VERSION,
        payload,
        sha256: sha,
        status: status.to_string(),
        block_reason,
        created_at: "2026-07-31T00:00:00Z".to_string(),
    })
}

fn import_stream_summaries(
    conn: &Connection,
    run_id: i64,
    event_out: &Path,
    summary: &mut ImportSummary,
) -> Result<(), String> {
    let path = event_out.join("lifecycle.json");
    if !path.is_file() {
        return Ok(());
    }
    let content =
        std::fs::read_to_string(&path).map_err(|e| format!("cannot read lifecycle: {e}"))?;
    let value: serde_json::Value =
        serde_json::from_str(&content).map_err(|e| format!("invalid lifecycle.json: {e}"))?;
    let lifecycles = value
        .get("lifecycles")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let mut rows = Vec::new();
    for lc in lifecycles {
        let transit_state = lc
            .get("category")
            .and_then(|v| v.as_str())
            .unwrap_or("Unchanged")
            .to_string();
        rows.push(StreamLifecycleSummary {
            id: 0,
            run_id,
            collector: lc
                .get("collector")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            peer_ip: lc
                .get("peer_ip")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            prefix: lc
                .get("prefix")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            category: transit_state.clone(),
            baseline_instances: lc
                .get("baseline_instance_count")
                .and_then(|v| v.as_i64())
                .unwrap_or(0),
            max_active_instances: lc
                .get("max_concurrent_instances")
                .and_then(|v| v.as_i64())
                .unwrap_or(0),
            transition_count: lc
                .get("transitions")
                .and_then(|v| v.as_array())
                .map(|a| a.len() as i64)
                .unwrap_or(0),
            withdrawn: lc
                .get("was_withdrawn")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            restored: lc
                .get("flags")
                .and_then(|f| f.get("restored"))
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            transit_state,
            add_path_ambiguous: lc
                .get("flags")
                .and_then(|f| f.get("add_path_ambiguous"))
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            evidence_refs: "[]".to_string(),
        });
    }
    store::insert_streams(conn, run_id, &rows)?;
    summary.streams += rows.len();
    Ok(())
}

fn import_wave_summaries(
    conn: &Connection,
    run_id: i64,
    event_out: &Path,
    summary: &mut ImportSummary,
) -> Result<(), String> {
    let path = event_out.join("semantic_waves.json");
    if !path.is_file() {
        return Ok(());
    }
    let content = std::fs::read_to_string(&path).map_err(|e| format!("cannot read waves: {e}"))?;
    let value: serde_json::Value =
        serde_json::from_str(&content).map_err(|e| format!("invalid semantic_waves.json: {e}"))?;
    let waves = value
        .get("waves")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let mut rows = Vec::new();
    for w in waves {
        rows.push(SemanticWaveSummary {
            id: 0,
            run_id,
            wave_id: w
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            label: w
                .get("label")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            start: w
                .get("start")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            peak_start: w
                .get("peak_start")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            peak_end: w
                .get("peak_end")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            end: w
                .get("end")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            stream_count: w.get("stream_count").and_then(|v| v.as_i64()).unwrap_or(0),
            instance_count: w
                .get("route_instance_count")
                .and_then(|v| v.as_i64())
                .unwrap_or(0),
        });
    }
    store::insert_waves(conn, run_id, &rows)?;
    summary.waves += rows.len();
    Ok(())
}

/// Find a ticket fixture for an event id.
fn ticket_fixture_for(event_id: &str) -> Option<PathBuf> {
    for base in ["tests/fixtures/internet2", "tests/fixtures/grnoc"] {
        let p = Path::new(base).join(format!("{event_id}.json"));
        if p.is_file() {
            return Some(p);
        }
    }
    None
}

fn artifact_kind(rel: &str) -> &'static str {
    if rel.ends_with("report.json") || rel.ends_with("report.txt") {
        "report"
    } else if rel.ends_with("evidence_appendix.jsonl") {
        "evidence-appendix"
    } else if rel.ends_with("archive_manifest.json") {
        "archive-manifest"
    } else if rel.ends_with("lifecycle.json") {
        "lifecycle"
    } else if rel.ends_with("semantic_waves.json") {
        "semantic-waves"
    } else if rel.ends_with("withdrawal_audit.json") {
        "withdrawal-audit"
    } else if rel.ends_with("limitations.json") {
        "limitations"
    } else if rel.ends_with("stdout.json") {
        "stdout"
    } else if rel.ends_with("stderr.log") {
        "stderr"
    } else {
        "artifact"
    }
}

fn media_type_for(kind: &str) -> &'static str {
    match kind {
        "report" | "archive-manifest" | "lifecycle" | "semantic-waves" | "withdrawal-audit"
        | "limitations" | "stdout" => "application/json",
        "evidence-appendix" => "application/x-ndjson",
        "stderr" => "text/plain",
        _ => "application/octet-stream",
    }
}

/// Hex SHA-256 of bytes.
pub fn sha256_hex_bytes(bytes: &[u8]) -> String {
    use sha2::Digest;
    let digest = sha2::Sha256::digest(bytes);
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::db;

    fn open_temp_db() -> (tempfile::TempDir, Connection) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("catalog.sqlite");
        let conn = db::open_catalog(&path).unwrap();
        (dir, conn)
    }

    /// Copy manifests/ and out/ into a temp root so tests never mutate
    /// (or race on) the repository's real artifacts.
    fn repo_artifacts_available() -> bool {
        Path::new("manifests").is_dir() && Path::new("out/INC0302574/report.json").is_file()
    }

    fn temp_repo_root() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        copy_tree(Path::new("manifests"), &dir.path().join("manifests"));
        copy_tree(Path::new("out"), &dir.path().join("out"));
        dir
    }

    fn copy_tree(from: &Path, to: &Path) {
        std::fs::create_dir_all(to).unwrap();
        for entry in std::fs::read_dir(from).unwrap() {
            let entry = entry.unwrap();
            let target = to.join(entry.file_name());
            if entry.file_type().unwrap().is_dir() {
                copy_tree(&entry.path(), &target);
            } else {
                std::fs::copy(entry.path(), target).unwrap();
            }
        }
    }

    #[test]
    fn import_completed_event_creates_analysis_run() {
        if !repo_artifacts_available() {
            return;
        }
        let (_dir, conn) = open_temp_db();
        let summary = import_repository(&conn, Path::new("."), "0.1.0", Some("test-git")).unwrap();
        // INC0302574 + INC0299001 completed; INC0301970 blocked.
        assert_eq!(summary.events, 3);
        assert_eq!(summary.manifests, 3);
        assert_eq!(summary.plans, 3);
        assert_eq!(summary.runs, 2);
        assert!(summary.artifacts > 0);
        // INC0301970: no run, no outcome.
        let e = db::get_event_by_external(&conn, "local-repository", "INC0301970")
            .unwrap()
            .unwrap();
        assert!(db::list_runs_for_event(&conn, e.id).unwrap().is_empty());
        let manifests = db::list_manifest_revisions(&conn, e.id).unwrap();
        assert_eq!(manifests.len(), 1);
        let plans = db::list_plans_for_manifest(&conn, manifests[0].id).unwrap();
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].status, "Blocked");
        assert_eq!(
            plans[0].block_reason.as_deref(),
            Some("MissingReviewedTransitPredicate")
        );
    }

    #[test]
    fn import_blocked_event_creates_plan_without_analysis_outcome() {
        if !repo_artifacts_available() {
            return;
        }
        let (_dir, conn) = open_temp_db();
        import_repository(&conn, Path::new("."), "0.1.0", None).unwrap();
        let e = db::get_event_by_external(&conn, "local-repository", "INC0301970")
            .unwrap()
            .unwrap();
        let runs = db::list_runs_for_event(&conn, e.id).unwrap();
        assert!(runs.is_empty(), "blocked event has no analysis run");
        let manifests = db::list_manifest_revisions(&conn, e.id).unwrap();
        let plans = db::list_plans_for_manifest(&conn, manifests[0].id).unwrap();
        assert_eq!(plans[0].status, "Blocked");
    }

    #[test]
    fn repeated_import_is_idempotent() {
        if !repo_artifacts_available() {
            return;
        }
        let (_dir, conn) = open_temp_db();
        let a = import_repository(&conn, Path::new("."), "0.1.0", None).unwrap();
        let b = import_repository(&conn, Path::new("."), "0.1.0", None).unwrap();
        assert_eq!(b.events, 0, "no new events on re-import");
        assert_eq!(b.runs, 0);
        assert_eq!(b.artifacts, 0);
        assert_eq!(db::list_events(&conn).unwrap().len(), a.events);
    }

    #[test]
    fn conflicting_immutable_run_is_rejected() {
        if !repo_artifacts_available() {
            return;
        }
        let root = temp_repo_root();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("catalog.sqlite");
        let conn = db::open_catalog(&path).unwrap();
        import_repository(&conn, root.path(), "0.1.0", None).unwrap();
        // Corrupt an artifact file so its hash no longer matches the
        // catalog; re-import must reject rather than modify history.
        let stderr = root.path().join("out/INC0302574/stderr.log");
        std::fs::write(&stderr, "corrupted").unwrap();
        let err = import_repository(&conn, root.path(), "0.1.0", None).unwrap_err();
        assert!(
            err.contains("conflicts with an existing immutable run"),
            "{err}"
        );
    }

    #[test]
    fn artifact_hash_mismatch_is_rejected() {
        if !repo_artifacts_available() {
            return;
        }
        let root = temp_repo_root();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("catalog.sqlite");
        let conn = db::open_catalog(&path).unwrap();
        import_repository(&conn, root.path(), "0.1.0", None).unwrap();
        let stderr = root.path().join("out/INC0302574/stderr.log");
        std::fs::write(&stderr, "tampered").unwrap();
        let err = import_repository(&conn, root.path(), "0.1.0", None).unwrap_err();
        assert!(
            err.contains("hash mismatch") || err.contains("schema"),
            "{err}"
        );
    }

    #[test]
    fn imported_stream_counts_match_report() {
        if !repo_artifacts_available() {
            return;
        }
        let (_dir, conn) = open_temp_db();
        import_repository(&conn, Path::new("."), "0.1.0", None).unwrap();
        let e = db::get_event_by_external(&conn, "local-repository", "INC0299001")
            .unwrap()
            .unwrap();
        let runs = db::list_runs_for_event(&conn, e.id).unwrap();
        assert_eq!(runs.len(), 1);
        let streams = db::list_streams(&conn, runs[0].id, None, None).unwrap();
        let report: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string("out/INC0299001/report.json").unwrap())
                .unwrap();
        let report_streams = report["observed_event_signature"]["observer_scope"]
            ["baseline_observer_prefix_streams"]
            .as_u64()
            .unwrap() as usize;
        assert_eq!(streams.len(), report_streams);
        let withdrawn = report["observed_event_signature"]["stream_lifecycle"]["withdrawn_streams"]
            .as_u64()
            .unwrap() as usize;
        assert_eq!(streams.iter().filter(|s| s.withdrawn).count(), withdrawn);
    }

    #[test]
    fn imported_wave_counts_match_report() {
        if !repo_artifacts_available() {
            return;
        }
        let (_dir, conn) = open_temp_db();
        import_repository(&conn, Path::new("."), "0.1.0", None).unwrap();
        let e = db::get_event_by_external(&conn, "local-repository", "INC0299001")
            .unwrap()
            .unwrap();
        let runs = db::list_runs_for_event(&conn, e.id).unwrap();
        let waves = db::list_waves(&conn, runs[0].id).unwrap();
        let report: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string("out/INC0299001/report.json").unwrap())
                .unwrap();
        let report_waves = report["observed_event_signature"]["semantic_waves"]
            .as_array()
            .map(|a| a.len())
            .unwrap_or(0);
        assert_eq!(waves.len(), report_waves);
        assert_eq!(waves[0].label, "PrependReduction");
    }
}
