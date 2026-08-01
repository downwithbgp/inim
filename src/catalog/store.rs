//! Catalog write path — insert-only operations.
//!
//! Revisions are immutable: rows are created, never updated in place.
//! Callers wrap multi-row imports in a transaction.

use rusqlite::{params, Connection};

use super::domain::*;

/// Upsert a source event by (source_kind, external_id); returns its id.
pub fn upsert_event(
    conn: &Connection,
    source_kind: &str,
    external_id: &str,
    seen_at: &str,
) -> Result<i64, String> {
    let existing = super::db::get_event_by_external(conn, source_kind, external_id)?;
    if let Some(e) = existing {
        conn.execute(
            "UPDATE catalog_events SET last_seen = ?1 WHERE id = ?2",
            params![seen_at, e.id],
        )
        .map_err(|e| format!("catalog write failed: {e}"))?;
        return Ok(e.id);
    }
    conn.execute(
        "INSERT INTO catalog_events (source_kind, external_id, first_seen, last_seen)
         VALUES (?1, ?2, ?3, ?3)",
        params![source_kind, external_id, seen_at],
    )
    .map_err(|e| format!("catalog write failed: {e}"))?;
    Ok(conn.last_insert_rowid())
}

/// Insert a snapshot; deduplicates identical (event, content_sha256).
pub fn insert_snapshot(
    conn: &Connection,
    event_id: i64,
    snapshot: &EventSnapshot,
) -> Result<i64, String> {
    let existing: Option<i64> = conn
        .query_row(
            "SELECT id FROM event_snapshots WHERE event_id = ?1 AND content_sha256 = ?2",
            params![event_id, snapshot.content_sha256],
            |r| r.get(0),
        )
        .ok();
    if let Some(id) = existing {
        return Ok(id);
    }
    conn.execute(
        "INSERT INTO event_snapshots
           (event_id, fetched_at, source_url, content_sha256, raw_payload, normalized_json, parser_version)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            event_id,
            snapshot.fetched_at,
            snapshot.source_url,
            snapshot.content_sha256,
            snapshot.raw_payload,
            snapshot.normalized_json,
            snapshot.parser_version
        ],
    )
    .map_err(|e| format!("catalog write failed: {e}"))?;
    Ok(conn.last_insert_rowid())
}

/// Insert a manifest revision; deduplicates by sha256.
pub fn insert_manifest_revision(
    conn: &Connection,
    revision: &ManifestRevision,
) -> Result<i64, String> {
    let existing: Option<i64> = conn
        .query_row(
            "SELECT id FROM manifest_revisions WHERE sha256 = ?1",
            params![revision.sha256],
            |r| r.get(0),
        )
        .ok();
    if let Some(id) = existing {
        return Ok(id);
    }
    conn.execute(
        "INSERT INTO manifest_revisions
           (event_id, snapshot_id, manifest_schema, payload, sha256, review_status, reviewed_at, reviewer)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            revision.event_id,
            revision.snapshot_id,
            revision.manifest_schema,
            revision.payload,
            revision.sha256,
            revision.review_status,
            revision.reviewed_at,
            revision.reviewer
        ],
    )
    .map_err(|e| format!("catalog write failed: {e}"))?;
    Ok(conn.last_insert_rowid())
}

/// Insert an analysis plan; deduplicates by sha256.
pub fn insert_plan(conn: &Connection, plan: &AnalysisPlanRecord) -> Result<i64, String> {
    let existing: Option<i64> = conn
        .query_row(
            "SELECT id FROM analysis_plans WHERE sha256 = ?1",
            params![plan.sha256],
            |r| r.get(0),
        )
        .ok();
    if let Some(id) = existing {
        return Ok(id);
    }
    conn.execute(
        "INSERT INTO analysis_plans
           (manifest_revision_id, plan_schema, payload, sha256, status, block_reason, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            plan.manifest_revision_id,
            plan.plan_schema,
            plan.payload,
            plan.sha256,
            plan.status,
            plan.block_reason,
            plan.created_at
        ],
    )
    .map_err(|e| format!("catalog write failed: {e}"))?;
    Ok(conn.last_insert_rowid())
}

/// Insert an analysis run; deduplicates by (plan_id, started_at).
pub fn insert_run(conn: &Connection, run: &AnalysisRun) -> Result<i64, String> {
    let existing: Option<i64> = conn
        .query_row(
            "SELECT id FROM analysis_runs WHERE plan_id = ?1 AND started_at = ?2",
            params![run.plan_id, run.started_at],
            |r| r.get(0),
        )
        .ok();
    if let Some(id) = existing {
        return Ok(id);
    }
    conn.execute(
        "INSERT INTO analysis_runs
           (plan_id, software_version, git_revision, parser_identity, cache_schema_version,
            report_schema_version, status, started_at, completed_at, runtime_secs, verdict, assessment)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        params![
            run.plan_id,
            run.software_version,
            run.git_revision,
            run.parser_identity,
            run.cache_schema_version,
            run.report_schema_version,
            run.status,
            run.started_at,
            run.completed_at,
            run.runtime_secs,
            run.verdict,
            run.assessment
        ],
    )
    .map_err(|e| format!("catalog write failed: {e}"))?;
    Ok(conn.last_insert_rowid())
}

/// Insert an artifact; deduplicates by (run_id, relative_path).
pub fn insert_artifact(conn: &Connection, artifact: &AnalysisArtifact) -> Result<i64, String> {
    let existing: Option<i64> = conn
        .query_row(
            "SELECT id FROM analysis_artifacts WHERE run_id = ?1 AND relative_path = ?2",
            params![artifact.run_id, artifact.relative_path],
            |r| r.get(0),
        )
        .ok();
    if let Some(id) = existing {
        return Ok(id);
    }
    conn.execute(
        "INSERT INTO analysis_artifacts
           (run_id, kind, relative_path, media_type, schema_version, sha256, size, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            artifact.run_id,
            artifact.kind,
            artifact.relative_path,
            artifact.media_type,
            artifact.schema_version,
            artifact.sha256,
            artifact.size,
            artifact.created_at
        ],
    )
    .map_err(|e| format!("catalog write failed: {e}"))?;
    Ok(conn.last_insert_rowid())
}

/// Insert stream lifecycle summaries in bulk.
pub fn insert_streams(
    conn: &Connection,
    run_id: i64,
    streams: &[StreamLifecycleSummary],
) -> Result<(), String> {
    let mut stmt = conn
        .prepare(
            "INSERT INTO stream_lifecycle_summaries
               (run_id, collector, peer_ip, prefix, category, baseline_instances,
                max_active_instances, transition_count, withdrawn, restored,
                transit_state, add_path_ambiguous, evidence_refs)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
        )
        .map_err(|e| format!("catalog write failed: {e}"))?;
    for s in streams {
        stmt.execute(params![
            run_id,
            s.collector,
            s.peer_ip,
            s.prefix,
            s.category,
            s.baseline_instances,
            s.max_active_instances,
            s.transition_count,
            s.withdrawn as i64,
            s.restored as i64,
            s.transit_state,
            s.add_path_ambiguous as i64,
            s.evidence_refs
        ])
        .map_err(|e| format!("catalog write failed: {e}"))?;
    }
    Ok(())
}

/// Insert semantic wave summaries in bulk.
pub fn insert_waves(
    conn: &Connection,
    run_id: i64,
    waves: &[SemanticWaveSummary],
) -> Result<(), String> {
    let mut stmt = conn
        .prepare(
            "INSERT INTO semantic_wave_summaries
               (run_id, wave_id, label, start, peak_start, peak_end, end, stream_count, instance_count)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        )
        .map_err(|e| format!("catalog write failed: {e}"))?;
    for w in waves {
        stmt.execute(params![
            run_id,
            w.wave_id,
            w.label,
            w.start,
            w.peak_start,
            w.peak_end,
            w.end,
            w.stream_count,
            w.instance_count
        ])
        .map_err(|e| format!("catalog write failed: {e}"))?;
    }
    Ok(())
}

/// Insert a sync run record.
pub fn insert_sync_run(conn: &Connection, sync: &CatalogSyncRun) -> Result<i64, String> {
    conn.execute(
        "INSERT INTO catalog_sync_runs
           (source, started_at, completed_at, status, events_examined, new_events,
            changed_events, unchanged_events, failures)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            sync.source,
            sync.started_at,
            sync.completed_at,
            sync.status,
            sync.events_examined,
            sync.new_events,
            sync.changed_events,
            sync.unchanged_events,
            sync.failures
        ],
    )
    .map_err(|e| format!("catalog write failed: {e}"))?;
    Ok(conn.last_insert_rowid())
}
