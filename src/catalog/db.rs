//! Catalog database — open, migrate, and query helpers.
//!
//! Connection policy: `PRAGMA foreign_keys = ON`, WAL journal mode for the
//! local web application, a reasonable busy timeout, transactional
//! migrations, and immutable revision rows enforced by the schema plus
//! insert-only helpers.

use rusqlite::{Connection, OpenFlags};
use std::path::Path;
use std::time::Duration;

use super::migrations::{CATALOG_SCHEMA_VERSION, MIGRATIONS};

/// Open (and migrate) the catalog database.
///
/// Returns a clear error for missing parent directories or unwritable
/// paths. A database at a higher schema version is rejected.
pub fn open_catalog(path: &Path) -> Result<Connection, String> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| {
                format!("cannot create catalog directory {}: {e}", parent.display())
            })?;
        }
    }
    let conn = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_CREATE
            | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|e| format!("cannot open catalog database {}: {e}", path.display()))?;
    configure(&conn)?;
    migrate(&conn)?;
    Ok(conn)
}

/// Open an existing catalog database read-only.
pub fn open_catalog_readonly(path: &Path) -> Result<Connection, String> {
    let conn = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|e| format!("cannot open catalog database {}: {e}", path.display()))?;
    configure(&conn)?;
    let version = current_version(&conn)?;
    if version != CATALOG_SCHEMA_VERSION {
        return Err(format!(
            "catalog database schema v{version} is incompatible with expected v{CATALOG_SCHEMA_VERSION}"
        ));
    }
    Ok(conn)
}

fn configure(conn: &Connection) -> Result<(), String> {
    conn.busy_timeout(Duration::from_secs(5))
        .map_err(|e| format!("cannot set busy timeout: {e}"))?;
    conn.execute_batch("PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL;")
        .map_err(|e| format!("cannot configure catalog database: {e}"))
}

/// Current schema version (`PRAGMA user_version`).
pub fn current_version(conn: &Connection) -> Result<u32, String> {
    conn.query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(|e| format!("cannot read catalog schema version: {e}"))
}

/// Apply all pending migrations transactionally.
pub fn migrate(conn: &Connection) -> Result<(), String> {
    let current = current_version(conn)?;
    if current > CATALOG_SCHEMA_VERSION {
        return Err(format!(
            "catalog database schema v{current} is newer than supported v{CATALOG_SCHEMA_VERSION}"
        ));
    }
    for (i, migration) in MIGRATIONS.iter().enumerate() {
        let target = i as u32 + 1;
        if target <= current {
            continue;
        }
        let tx = conn
            .unchecked_transaction()
            .map_err(|e| format!("cannot start migration transaction: {e}"))?;
        tx.execute_batch(migration)
            .map_err(|e| format!("catalog migration to v{target} failed: {e}"))?;
        tx.pragma_update(None, "user_version", target)
            .map_err(|e| format!("cannot record catalog schema version: {e}"))?;
        tx.commit()
            .map_err(|e| format!("cannot commit catalog migration: {e}"))?;
    }
    Ok(())
}

// ── Row helpers ─────────────────────────────────────────────────────

fn row_to_event(row: &rusqlite::Row<'_>) -> rusqlite::Result<super::domain::CatalogEvent> {
    Ok(super::domain::CatalogEvent {
        id: row.get(0)?,
        source_kind: row.get(1)?,
        external_id: row.get(2)?,
        first_seen: row.get(3)?,
        last_seen: row.get(4)?,
    })
}

/// Fetch one event by internal id.
pub fn get_event(
    conn: &Connection,
    id: i64,
) -> Result<Option<super::domain::CatalogEvent>, String> {
    conn.query_row(
        "SELECT id, source_kind, external_id, first_seen, last_seen FROM catalog_events WHERE id = ?1",
        [id],
        row_to_event,
    )
    .map(Some)
    .or_else(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => Ok(None),
        other => Err(format!("catalog query failed: {other}")),
    })
}

/// Fetch one event by source identity.
pub fn get_event_by_external(
    conn: &Connection,
    source_kind: &str,
    external_id: &str,
) -> Result<Option<super::domain::CatalogEvent>, String> {
    conn.query_row(
        "SELECT id, source_kind, external_id, first_seen, last_seen FROM catalog_events
         WHERE source_kind = ?1 AND external_id = ?2",
        rusqlite::params![source_kind, external_id],
        row_to_event,
    )
    .map(Some)
    .or_else(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => Ok(None),
        other => Err(format!("catalog query failed: {other}")),
    })
}

/// List all events, newest first by `last_seen`, tie-broken by external id.
pub fn list_events(conn: &Connection) -> Result<Vec<super::domain::CatalogEvent>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, source_kind, external_id, first_seen, last_seen
             FROM catalog_events ORDER BY last_seen DESC, external_id ASC",
        )
        .map_err(|e| format!("catalog query failed: {e}"))?;
    let rows = stmt
        .query_map([], row_to_event)
        .map_err(|e| format!("catalog query failed: {e}"))?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r.map_err(|e| format!("catalog query failed: {e}"))?);
    }
    Ok(out)
}

/// Snapshot rows for an event, newest first.
pub fn list_snapshots(
    conn: &Connection,
    event_id: i64,
) -> Result<Vec<super::domain::EventSnapshot>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, event_id, fetched_at, source_url, content_sha256, raw_payload,
                    normalized_json, parser_version
             FROM event_snapshots WHERE event_id = ?1 ORDER BY fetched_at DESC",
        )
        .map_err(|e| format!("catalog query failed: {e}"))?;
    let rows = stmt
        .query_map([event_id], |row| {
            Ok(super::domain::EventSnapshot {
                id: row.get(0)?,
                event_id: row.get(1)?,
                fetched_at: row.get(2)?,
                source_url: row.get(3)?,
                content_sha256: row.get(4)?,
                raw_payload: row.get(5)?,
                normalized_json: row.get(6)?,
                parser_version: row.get(7)?,
            })
        })
        .map_err(|e| format!("catalog query failed: {e}"))?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r.map_err(|e| format!("catalog query failed: {e}"))?);
    }
    Ok(out)
}

/// Manifest revisions for an event, newest first.
pub fn list_manifest_revisions(
    conn: &Connection,
    event_id: i64,
) -> Result<Vec<super::domain::ManifestRevision>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, event_id, snapshot_id, manifest_schema, payload, sha256,
                    review_status, reviewed_at, reviewer
             FROM manifest_revisions WHERE event_id = ?1 ORDER BY id DESC",
        )
        .map_err(|e| format!("catalog query failed: {e}"))?;
    let rows = stmt
        .query_map([event_id], |row| {
            Ok(super::domain::ManifestRevision {
                id: row.get(0)?,
                event_id: row.get(1)?,
                snapshot_id: row.get(2)?,
                manifest_schema: row.get(3)?,
                payload: row.get(4)?,
                sha256: row.get(5)?,
                review_status: row.get(6)?,
                reviewed_at: row.get(7)?,
                reviewer: row.get(8)?,
            })
        })
        .map_err(|e| format!("catalog query failed: {e}"))?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r.map_err(|e| format!("catalog query failed: {e}"))?);
    }
    Ok(out)
}

/// Plans for a manifest revision.
pub fn list_plans_for_manifest(
    conn: &Connection,
    manifest_revision_id: i64,
) -> Result<Vec<super::domain::AnalysisPlanRecord>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, manifest_revision_id, plan_schema, payload, sha256, status,
                    block_reason, created_at
             FROM analysis_plans WHERE manifest_revision_id = ?1 ORDER BY id DESC",
        )
        .map_err(|e| format!("catalog query failed: {e}"))?;
    let rows = stmt
        .query_map([manifest_revision_id], |row| {
            Ok(super::domain::AnalysisPlanRecord {
                id: row.get(0)?,
                manifest_revision_id: row.get(1)?,
                plan_schema: row.get(2)?,
                payload: row.get(3)?,
                sha256: row.get(4)?,
                status: row.get(5)?,
                block_reason: row.get(6)?,
                created_at: row.get(7)?,
            })
        })
        .map_err(|e| format!("catalog query failed: {e}"))?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r.map_err(|e| format!("catalog query failed: {e}"))?);
    }
    Ok(out)
}

/// Runs for an event (through plans and manifest revisions).
pub fn list_runs_for_event(
    conn: &Connection,
    event_id: i64,
) -> Result<Vec<super::domain::AnalysisRun>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT r.id, r.plan_id, r.software_version, r.git_revision, r.parser_identity,
                    r.cache_schema_version, r.report_schema_version, r.status, r.started_at,
                    r.completed_at, r.runtime_secs, r.verdict, r.assessment
             FROM analysis_runs r
             JOIN analysis_plans p ON p.id = r.plan_id
             JOIN manifest_revisions m ON m.id = p.manifest_revision_id
             WHERE m.event_id = ?1 ORDER BY r.started_at DESC",
        )
        .map_err(|e| format!("catalog query failed: {e}"))?;
    let rows = stmt
        .query_map([event_id], |row| {
            Ok(super::domain::AnalysisRun {
                id: row.get(0)?,
                plan_id: row.get(1)?,
                software_version: row.get(2)?,
                git_revision: row.get(3)?,
                parser_identity: row.get(4)?,
                cache_schema_version: row.get(5)?,
                report_schema_version: row.get(6)?,
                status: row.get(7)?,
                started_at: row.get(8)?,
                completed_at: row.get(9)?,
                runtime_secs: row.get(10)?,
                verdict: row.get(11)?,
                assessment: row.get(12)?,
            })
        })
        .map_err(|e| format!("catalog query failed: {e}"))?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r.map_err(|e| format!("catalog query failed: {e}"))?);
    }
    Ok(out)
}

/// One run by id.
pub fn get_run(
    conn: &Connection,
    run_id: i64,
) -> Result<Option<super::domain::AnalysisRun>, String> {
    conn.query_row(
        "SELECT id, plan_id, software_version, git_revision, parser_identity,
                cache_schema_version, report_schema_version, status, started_at,
                completed_at, runtime_secs, verdict, assessment
         FROM analysis_runs WHERE id = ?1",
        [run_id],
        |row| {
            Ok(super::domain::AnalysisRun {
                id: row.get(0)?,
                plan_id: row.get(1)?,
                software_version: row.get(2)?,
                git_revision: row.get(3)?,
                parser_identity: row.get(4)?,
                cache_schema_version: row.get(5)?,
                report_schema_version: row.get(6)?,
                status: row.get(7)?,
                started_at: row.get(8)?,
                completed_at: row.get(9)?,
                runtime_secs: row.get(10)?,
                verdict: row.get(11)?,
                assessment: row.get(12)?,
            })
        },
    )
    .map(Some)
    .or_else(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => Ok(None),
        other => Err(format!("catalog query failed: {other}")),
    })
}

/// One plan by id.
pub fn get_plan(
    conn: &Connection,
    plan_id: i64,
) -> Result<Option<super::domain::AnalysisPlanRecord>, String> {
    conn.query_row(
        "SELECT id, manifest_revision_id, plan_schema, payload, sha256, status,
                block_reason, created_at
         FROM analysis_plans WHERE id = ?1",
        [plan_id],
        |row| {
            Ok(super::domain::AnalysisPlanRecord {
                id: row.get(0)?,
                manifest_revision_id: row.get(1)?,
                plan_schema: row.get(2)?,
                payload: row.get(3)?,
                sha256: row.get(4)?,
                status: row.get(5)?,
                block_reason: row.get(6)?,
                created_at: row.get(7)?,
            })
        },
    )
    .map(Some)
    .or_else(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => Ok(None),
        other => Err(format!("catalog query failed: {other}")),
    })
}

/// One manifest revision by id.
pub fn get_manifest_revision(
    conn: &Connection,
    id: i64,
) -> Result<Option<super::domain::ManifestRevision>, String> {
    conn.query_row(
        "SELECT id, event_id, snapshot_id, manifest_schema, payload, sha256,
                review_status, reviewed_at, reviewer
         FROM manifest_revisions WHERE id = ?1",
        [id],
        |row| {
            Ok(super::domain::ManifestRevision {
                id: row.get(0)?,
                event_id: row.get(1)?,
                snapshot_id: row.get(2)?,
                manifest_schema: row.get(3)?,
                payload: row.get(4)?,
                sha256: row.get(5)?,
                review_status: row.get(6)?,
                reviewed_at: row.get(7)?,
                reviewer: row.get(8)?,
            })
        },
    )
    .map(Some)
    .or_else(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => Ok(None),
        other => Err(format!("catalog query failed: {other}")),
    })
}

/// Artifacts of a run.
pub fn list_artifacts(
    conn: &Connection,
    run_id: i64,
) -> Result<Vec<super::domain::AnalysisArtifact>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, run_id, kind, relative_path, media_type, schema_version, sha256, size, created_at
             FROM analysis_artifacts WHERE run_id = ?1 ORDER BY relative_path",
        )
        .map_err(|e| format!("catalog query failed: {e}"))?;
    let rows = stmt
        .query_map([run_id], |row| {
            Ok(super::domain::AnalysisArtifact {
                id: row.get(0)?,
                run_id: row.get(1)?,
                kind: row.get(2)?,
                relative_path: row.get(3)?,
                media_type: row.get(4)?,
                schema_version: row.get(5)?,
                sha256: row.get(6)?,
                size: row.get(7)?,
                created_at: row.get(8)?,
            })
        })
        .map_err(|e| format!("catalog query failed: {e}"))?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r.map_err(|e| format!("catalog query failed: {e}"))?);
    }
    Ok(out)
}

/// Stream lifecycle summaries of a run, with optional category filter.
pub fn list_streams(
    conn: &Connection,
    run_id: i64,
    category: Option<&str>,
    collector: Option<&str>,
) -> Result<Vec<super::domain::StreamLifecycleSummary>, String> {
    let mut sql = String::from(
        "SELECT id, run_id, collector, peer_ip, prefix, category, baseline_instances,
                max_active_instances, transition_count, withdrawn, restored, transit_state,
                add_path_ambiguous, evidence_refs
         FROM stream_lifecycle_summaries WHERE run_id = ?1",
    );
    let mut params: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(run_id)];
    if let Some(c) = category {
        sql.push_str(" AND category = ?");
        params.push(Box::new(c.to_string()));
    }
    if let Some(col) = collector {
        sql.push_str(" AND collector = ?");
        params.push(Box::new(col.to_string()));
    }
    sql.push_str(" ORDER BY collector, peer_ip, prefix");
    let mut stmt = conn
        .prepare(&sql)
        .map_err(|e| format!("catalog query failed: {e}"))?;
    let rows = stmt
        .query_map(
            rusqlite::params_from_iter(params.iter().map(|p| p.as_ref())),
            |row| {
                Ok(super::domain::StreamLifecycleSummary {
                    id: row.get(0)?,
                    run_id: row.get(1)?,
                    collector: row.get(2)?,
                    peer_ip: row.get(3)?,
                    prefix: row.get(4)?,
                    category: row.get(5)?,
                    baseline_instances: row.get(6)?,
                    max_active_instances: row.get(7)?,
                    transition_count: row.get(8)?,
                    withdrawn: row.get(9)?,
                    restored: row.get(10)?,
                    transit_state: row.get(11)?,
                    add_path_ambiguous: row.get(12)?,
                    evidence_refs: row.get(13)?,
                })
            },
        )
        .map_err(|e| format!("catalog query failed: {e}"))?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r.map_err(|e| format!("catalog query failed: {e}"))?);
    }
    Ok(out)
}

/// Wave summaries of a run.
pub fn list_waves(
    conn: &Connection,
    run_id: i64,
) -> Result<Vec<super::domain::SemanticWaveSummary>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, run_id, wave_id, label, start, peak_start, peak_end, end,
                    stream_count, instance_count
             FROM semantic_wave_summaries WHERE run_id = ?1 ORDER BY start",
        )
        .map_err(|e| format!("catalog query failed: {e}"))?;
    let rows = stmt
        .query_map([run_id], |row| {
            Ok(super::domain::SemanticWaveSummary {
                id: row.get(0)?,
                run_id: row.get(1)?,
                wave_id: row.get(2)?,
                label: row.get(3)?,
                start: row.get(4)?,
                peak_start: row.get(5)?,
                peak_end: row.get(6)?,
                end: row.get(7)?,
                stream_count: row.get(8)?,
                instance_count: row.get(9)?,
            })
        })
        .map_err(|e| format!("catalog query failed: {e}"))?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r.map_err(|e| format!("catalog query failed: {e}"))?);
    }
    Ok(out)
}

/// A run's transitions in deterministic order (full records).
pub fn list_transitions(
    conn: &Connection,
    run_id: i64,
) -> Result<Vec<super::domain::RunTransitionRecord>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, run_id, seq, kind, occurred_utc, run_phase, collector, peer_ip,
                    prefix, path_id, material_path_changed, communities_changed, announced,
                    withdrawn, observation_id, archive_sha256
             FROM run_transitions WHERE run_id = ?1 ORDER BY seq",
        )
        .map_err(|e| format!("catalog query failed: {e}"))?;
    let rows = stmt
        .query_map([run_id], |row| {
            Ok(super::domain::RunTransitionRecord {
                id: row.get(0)?,
                run_id: row.get(1)?,
                seq: row.get(2)?,
                kind: row.get(3)?,
                occurred_utc: row.get(4)?,
                run_phase: row.get(5)?,
                collector: row.get(6)?,
                peer_ip: row.get(7)?,
                prefix: row.get(8)?,
                path_id: row.get(9)?,
                material_path_changed: row.get(10)?,
                communities_changed: row.get(11)?,
                announced: row.get(12)?,
                withdrawn: row.get(13)?,
                observation_id: row.get(14)?,
                archive_sha256: row.get(15)?,
            })
        })
        .map_err(|e| format!("catalog query failed: {e}"))?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r.map_err(|e| format!("catalog query failed: {e}"))?);
    }
    Ok(out)
}

/// Most recent sync run for a source.
pub fn latest_sync(
    conn: &Connection,
    source: &str,
) -> Result<Option<super::domain::CatalogSyncRun>, String> {
    conn.query_row(
        "SELECT id, source, started_at, completed_at, status, events_examined,
                new_events, changed_events, unchanged_events, failures
         FROM catalog_sync_runs WHERE source = ?1 ORDER BY id DESC LIMIT 1",
        [source],
        |row| {
            Ok(super::domain::CatalogSyncRun {
                id: row.get(0)?,
                source: row.get(1)?,
                started_at: row.get(2)?,
                completed_at: row.get(3)?,
                status: row.get(4)?,
                events_examined: row.get(5)?,
                new_events: row.get(6)?,
                changed_events: row.get(7)?,
                unchanged_events: row.get(8)?,
                failures: row.get(9)?,
            })
        },
    )
    .map(Some)
    .or_else(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => Ok(None),
        other => Err(format!("catalog query failed: {other}")),
    })
}
