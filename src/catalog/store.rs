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

// ── Case-study layer (Session 30) ──────────────────────────────────

/// Insert a case study; idempotent for (slug, content_sha256), rejecting a
/// conflicting immutable revision for an existing slug.
pub fn insert_case_study(conn: &Connection, cs: &CaseStudy) -> Result<i64, String> {
    let existing: Option<(i64, String)> = conn
        .query_row(
            "SELECT id, content_sha256 FROM case_studies WHERE slug = ?1",
            params![cs.slug],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .ok();
    if let Some((id, sha)) = existing {
        if sha == cs.content_sha256 {
            return Ok(id);
        }
        return Err(format!(
            "conflicting immutable case study revision for slug '{}'",
            cs.slug
        ));
    }
    conn.execute(
        "INSERT INTO case_studies
           (slug, title, summary, start_utc, end_utc, status, content_sha256, created_utc, updated_utc)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            cs.slug,
            cs.title,
            cs.summary,
            cs.start_utc,
            cs.end_utc,
            cs.status,
            cs.content_sha256,
            cs.created_utc,
            cs.updated_utc
        ],
    )
    .map_err(|e| format!("catalog write failed: {e}"))?;
    Ok(conn.last_insert_rowid())
}

/// Insert a reference document; reuses the existing row for
/// (title, source_url). Returns the document id.
pub fn insert_reference_document(
    conn: &Connection,
    doc: &ReferenceDocument,
) -> Result<i64, String> {
    let existing: Option<i64> = conn
        .query_row(
            "SELECT id FROM reference_documents WHERE title = ?1 AND source_url IS ?2",
            params![doc.title, doc.source_url],
            |r| r.get(0),
        )
        .ok();
    if let Some(id) = existing {
        return Ok(id);
    }
    conn.execute(
        "INSERT INTO reference_documents
           (title, source_url, doc_type, redistribution_status, publication_date, provenance, imported_utc)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            doc.title,
            doc.source_url,
            doc.doc_type,
            doc.redistribution_status,
            doc.publication_date,
            doc.provenance,
            doc.imported_utc
        ],
    )
    .map_err(|e| format!("catalog write failed: {e}"))?;
    Ok(conn.last_insert_rowid())
}

/// Insert a document revision; deduplicates identical content by SHA-256.
/// Returns the revision id (existing when the content is already present).
pub fn insert_document_revision(conn: &Connection, rev: &DocumentRevision) -> Result<i64, String> {
    let existing: Option<i64> = conn
        .query_row(
            "SELECT id FROM document_revisions WHERE sha256 = ?1",
            params![rev.sha256],
            |r| r.get(0),
        )
        .ok();
    if let Some(id) = existing {
        return Ok(id);
    }
    conn.execute(
        "INSERT INTO document_revisions
           (document_id, revision, sha256, media_type, page_count, local_path, metadata_json, imported_utc)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            rev.document_id,
            rev.revision,
            rev.sha256,
            rev.media_type,
            rev.page_count,
            rev.local_path,
            rev.metadata_json,
            rev.imported_utc
        ],
    )
    .map_err(|e| format!("catalog write failed: {e}"))?;
    Ok(conn.last_insert_rowid())
}

/// Insert a case-study event link; idempotent per (case_study, external id).
pub fn insert_case_study_event_link(
    conn: &Connection,
    link: &CaseStudyEventLink,
) -> Result<i64, String> {
    let existing: Option<i64> = conn
        .query_row(
            "SELECT id FROM case_study_event_links
             WHERE case_study_id = ?1 AND external_identifier = ?2",
            params![link.case_study_id, link.external_identifier],
            |r| r.get(0),
        )
        .ok();
    if let Some(id) = existing {
        return Ok(id);
    }
    conn.execute(
        "INSERT INTO case_study_event_links
           (case_study_id, catalog_event_id, external_identifier, relationship, reviewed_note, sort_order, source_document_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            link.case_study_id,
            link.catalog_event_id,
            link.external_identifier,
            link.relationship,
            link.reviewed_note,
            link.sort_order,
            link.source_document_id
        ],
    )
    .map_err(|e| format!("catalog write failed: {e}"))?;
    Ok(conn.last_insert_rowid())
}

/// Insert a case-study document link; idempotent per
/// (case_study, document, relationship).
pub fn insert_case_study_document_link(
    conn: &Connection,
    link: &CaseStudyDocumentLink,
) -> Result<i64, String> {
    let existing: Option<i64> = conn
        .query_row(
            "SELECT id FROM case_study_document_links
             WHERE case_study_id = ?1 AND document_id = ?2 AND relationship = ?3",
            params![link.case_study_id, link.document_id, link.relationship],
            |r| r.get(0),
        )
        .ok();
    if let Some(id) = existing {
        return Ok(id);
    }
    conn.execute(
        "INSERT INTO case_study_document_links
           (case_study_id, document_id, relationship, reviewed_note)
         VALUES (?1, ?2, ?3, ?4)",
        params![
            link.case_study_id,
            link.document_id,
            link.relationship,
            link.reviewed_note
        ],
    )
    .map_err(|e| format!("catalog write failed: {e}"))?;
    Ok(conn.last_insert_rowid())
}

/// Insert a reviewed phase; idempotent per (case_study, sort_order).
pub fn insert_case_study_phase(conn: &Connection, phase: &CaseStudyPhase) -> Result<i64, String> {
    let existing: Option<i64> = conn
        .query_row(
            "SELECT id FROM case_study_phases WHERE case_study_id = ?1 AND sort_order = ?2",
            params![phase.case_study_id, phase.sort_order],
            |r| r.get(0),
        )
        .ok();
    if let Some(id) = existing {
        return Ok(id);
    }
    conn.execute(
        "INSERT INTO case_study_phases
           (case_study_id, label, start_utc, end_utc, start_precision, end_precision,
            description, source_document_id, source_page_or_section, review_status, sort_order)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![
            phase.case_study_id,
            phase.label,
            phase.start_utc,
            phase.end_utc,
            phase.start_precision,
            phase.end_precision,
            phase.description,
            phase.source_document_id,
            phase.source_page_or_section,
            phase.review_status,
            phase.sort_order
        ],
    )
    .map_err(|e| format!("catalog write failed: {e}"))?;
    Ok(conn.last_insert_rowid())
}

/// Insert a case-study ↔ analysis-run link; idempotent per
/// (case_study, run, role).
pub fn insert_case_study_analysis_link(
    conn: &Connection,
    link: &CaseStudyAnalysisLink,
) -> Result<i64, String> {
    let existing: Option<i64> = conn
        .query_row(
            "SELECT id FROM case_study_analysis_links
             WHERE case_study_id = ?1 AND run_id = ?2 AND role = ?3",
            params![link.case_study_id, link.run_id, link.role],
            |r| r.get(0),
        )
        .ok();
    if let Some(id) = existing {
        return Ok(id);
    }
    conn.execute(
        "INSERT INTO case_study_analysis_links
           (case_study_id, run_id, role, reviewed_note)
         VALUES (?1, ?2, ?3, ?4)",
        params![
            link.case_study_id,
            link.run_id,
            link.role,
            link.reviewed_note
        ],
    )
    .map_err(|e| format!("catalog write failed: {e}"))?;
    Ok(conn.last_insert_rowid())
}

/// Insert a reviewed claim; idempotent per (case_study, sort_order).
pub fn insert_case_study_claim(conn: &Connection, claim: &CaseStudyClaim) -> Result<i64, String> {
    let existing: Option<i64> = conn
        .query_row(
            "SELECT id FROM case_study_claims WHERE case_study_id = ?1 AND sort_order = ?2",
            params![claim.case_study_id, claim.sort_order],
            |r| r.get(0),
        )
        .ok();
    if let Some(id) = existing {
        return Ok(id);
    }
    conn.execute(
        "INSERT INTO case_study_claims
           (case_study_id, claim_type, claim_text, qualification, source_document_id,
            source_page_or_section, review_status, time_or_phase, observability,
            observability_rationale, sort_order)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![
            claim.case_study_id,
            claim.claim_type,
            claim.claim_text,
            claim.qualification,
            claim.source_document_id,
            claim.source_page_or_section,
            claim.review_status,
            claim.time_or_phase,
            claim.observability,
            claim.observability_rationale,
            claim.sort_order
        ],
    )
    .map_err(|e| format!("catalog write failed: {e}"))?;
    Ok(conn.last_insert_rowid())
}

/// Insert a reviewed analysis target; idempotent per (case_study, sort_order).
pub fn insert_case_study_target(
    conn: &Connection,
    target: &CaseStudyTarget,
) -> Result<i64, String> {
    let existing: Option<i64> = conn
        .query_row(
            "SELECT id FROM case_study_targets WHERE case_study_id = ?1 AND sort_order = ?2",
            params![target.case_study_id, target.sort_order],
            |r| r.get(0),
        )
        .ok();
    if let Some(id) = existing {
        return Ok(id);
    }
    conn.execute(
        "INSERT INTO case_study_targets
           (case_study_id, source_label, role_in_report, candidate_org_identity,
            candidate_origin_asns_json, candidate_predicate, historical_validity_status,
            provenance, research_status, reviewed_note, sort_order)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![
            target.case_study_id,
            target.source_label,
            target.role_in_report,
            target.candidate_org_identity,
            target.candidate_origin_asns_json,
            target.candidate_predicate,
            target.historical_validity_status,
            target.provenance,
            target.research_status,
            target.reviewed_note,
            target.sort_order
        ],
    )
    .map_err(|e| format!("catalog write failed: {e}"))?;
    Ok(conn.last_insert_rowid())
}

/// Insert (or replace) the analysis plan for a case study (one per case).
pub fn upsert_case_study_analysis_plan(
    conn: &Connection,
    plan: &CaseStudyAnalysisPlan,
) -> Result<i64, String> {
    conn.execute(
        "INSERT INTO case_study_analysis_plans
           (case_study_id, horizon_json, plan_json, status, created_utc)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(case_study_id) DO UPDATE SET
           horizon_json = excluded.horizon_json,
           plan_json = excluded.plan_json,
           status = excluded.status,
           created_utc = excluded.created_utc",
        params![
            plan.case_study_id,
            plan.horizon_json,
            plan.plan_json,
            plan.status,
            plan.created_utc
        ],
    )
    .map_err(|e| format!("catalog write failed: {e}"))?;
    Ok(conn.last_insert_rowid())
}

/// Insert a run transition; idempotent per (run_id, seq).
pub fn insert_run_transition(conn: &Connection, t: &RunTransitionRecord) -> Result<i64, String> {
    let existing: Option<i64> = conn
        .query_row(
            "SELECT id FROM run_transitions WHERE run_id = ?1 AND seq = ?2",
            params![t.run_id, t.seq],
            |r| r.get(0),
        )
        .ok();
    if let Some(id) = existing {
        return Ok(id);
    }
    conn.execute(
        "INSERT INTO run_transitions
           (run_id, seq, kind, occurred_utc, run_phase, collector, peer_ip, prefix,
            path_id, material_path_changed, communities_changed, announced, withdrawn,
            observation_id, archive_sha256)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
        params![
            t.run_id,
            t.seq,
            t.kind,
            t.occurred_utc,
            t.run_phase,
            t.collector,
            t.peer_ip,
            t.prefix,
            t.path_id,
            t.material_path_changed,
            t.communities_changed,
            t.announced,
            t.withdrawn,
            t.observation_id,
            t.archive_sha256
        ],
    )
    .map_err(|e| format!("catalog write failed: {e}"))?;
    Ok(conn.last_insert_rowid())
}

// ── Corpus discovery + fetch records (Session 33) ─────────────────

/// Record a discovery path. Duplicate (source, external_id, provenance)
/// paths merge: the existing row is returned and nothing is inserted.
pub fn record_discovery(conn: &Connection, d: &TicketDiscovery) -> Result<i64, String> {
    let existing: Option<i64> = conn
        .query_row(
            "SELECT id FROM ticket_discoveries
             WHERE source_kind = ?1 AND external_id = ?2 AND provenance = ?3",
            params![d.source_kind, d.external_id, d.provenance],
            |r| r.get(0),
        )
        .ok();
    if let Some(id) = existing {
        return Ok(id);
    }
    conn.execute(
        "INSERT INTO ticket_discoveries
           (source_kind, external_id, provenance, source_snapshot_id,
            source_document_id, discovered_at, status)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            d.source_kind,
            d.external_id,
            d.provenance,
            d.source_snapshot_id,
            d.source_document_id,
            d.discovered_at,
            d.status
        ],
    )
    .map_err(|e| format!("catalog write failed: {e}"))?;
    Ok(conn.last_insert_rowid())
}

/// Update a discovery row's status.
pub fn update_discovery_status(
    conn: &Connection,
    discovery_id: i64,
    status: &str,
) -> Result<(), String> {
    conn.execute(
        "UPDATE ticket_discoveries SET status = ?1 WHERE id = ?2",
        params![status, discovery_id],
    )
    .map_err(|e| format!("catalog write failed: {e}"))?;
    Ok(())
}

/// All discovery rows for a source, optionally filtered by status.
pub fn list_discoveries(
    conn: &Connection,
    source_kind: &str,
    status: Option<&str>,
) -> Result<Vec<TicketDiscovery>, String> {
    let mut sql = String::from(
        "SELECT id, source_kind, external_id, provenance, source_snapshot_id,
                source_document_id, discovered_at, status
         FROM ticket_discoveries WHERE source_kind = ?1",
    );
    let mut p: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(source_kind.to_string())];
    if let Some(s) = status {
        sql.push_str(" AND status = ?2");
        p.push(Box::new(s.to_string()));
    }
    sql.push_str(" ORDER BY discovered_at, id");
    let mut stmt = conn
        .prepare(&sql)
        .map_err(|e| format!("catalog read failed: {e}"))?;
    let rows = stmt
        .query_map(
            rusqlite::params_from_iter(p.iter().map(|x| x.as_ref())),
            |r| {
                Ok(TicketDiscovery {
                    id: r.get(0)?,
                    source_kind: r.get(1)?,
                    external_id: r.get(2)?,
                    provenance: r.get(3)?,
                    source_snapshot_id: r.get(4)?,
                    source_document_id: r.get(5)?,
                    discovered_at: r.get(6)?,
                    status: r.get(7)?,
                })
            },
        )
        .map_err(|e| format!("catalog read failed: {e}"))?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(|e| format!("catalog read failed: {e}"))?);
    }
    Ok(out)
}

/// The fetch frontier: distinct external ids with at least one Pending
/// discovery, in deterministic (discovered_at, id) order.
pub fn pending_frontier(conn: &Connection, source_kind: &str) -> Result<Vec<String>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT external_id FROM ticket_discoveries
             WHERE source_kind = ?1 AND status = ?2
             GROUP BY external_id
             ORDER BY MIN(discovered_at), MIN(id)",
        )
        .map_err(|e| format!("catalog read failed: {e}"))?;
    let rows = stmt
        .query_map(params![source_kind, DISCOVERY_STATUS_PENDING], |r| {
            r.get::<_, String>(0)
        })
        .map_err(|e| format!("catalog read failed: {e}"))?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(|e| format!("catalog read failed: {e}"))?);
    }
    Ok(out)
}

/// Mark every discovery row of one ticket as Fetched.
pub fn mark_frontier_fetched(
    conn: &Connection,
    source_kind: &str,
    external_id: &str,
) -> Result<(), String> {
    conn.execute(
        "UPDATE ticket_discoveries SET status = ?1
         WHERE source_kind = ?2 AND external_id = ?3 AND status = ?4",
        params![
            DISCOVERY_STATUS_FETCHED,
            source_kind,
            external_id,
            DISCOVERY_STATUS_PENDING
        ],
    )
    .map_err(|e| format!("catalog write failed: {e}"))?;
    Ok(())
}

/// Insert one fetch record (per-fetch provenance; never mutates the
/// snapshot row).
pub fn insert_snapshot_fetch(conn: &Connection, fetch: &SnapshotFetch) -> Result<i64, String> {
    conn.execute(
        "INSERT INTO snapshot_fetches
           (event_id, sync_run_id, fetched_at, source_url, http_status,
            content_type, etag, last_modified, acquisition_method,
            retry_count, snapshot_id, conditional_requested)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        params![
            fetch.event_id,
            fetch.sync_run_id,
            fetch.fetched_at,
            fetch.source_url,
            fetch.http_status,
            fetch.content_type,
            fetch.etag,
            fetch.last_modified,
            fetch.acquisition_method,
            fetch.retry_count,
            fetch.snapshot_id,
            i64::from(fetch.conditional_requested)
        ],
    )
    .map_err(|e| format!("catalog write failed: {e}"))?;
    Ok(conn.last_insert_rowid())
}

/// Fetch records for an event, newest first.
pub fn list_snapshot_fetches(
    conn: &Connection,
    event_id: i64,
) -> Result<Vec<SnapshotFetch>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, event_id, sync_run_id, fetched_at, source_url, http_status,
                    content_type, etag, last_modified, acquisition_method,
                    retry_count, snapshot_id, conditional_requested
             FROM snapshot_fetches WHERE event_id = ?1 ORDER BY fetched_at DESC, id DESC",
        )
        .map_err(|e| format!("catalog read failed: {e}"))?;
    let rows = stmt
        .query_map([event_id], |r| {
            Ok(SnapshotFetch {
                id: r.get(0)?,
                event_id: r.get(1)?,
                sync_run_id: r.get(2)?,
                fetched_at: r.get(3)?,
                source_url: r.get(4)?,
                http_status: r.get(5)?,
                content_type: r.get(6)?,
                etag: r.get(7)?,
                last_modified: r.get(8)?,
                acquisition_method: r.get(9)?,
                retry_count: r.get(10)?,
                snapshot_id: r.get(11)?,
                conditional_requested: r.get(12)?,
            })
        })
        .map_err(|e| format!("catalog read failed: {e}"))?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(|e| format!("catalog read failed: {e}"))?);
    }
    Ok(out)
}

/// Advance every discovery row of one ticket (that is still Pending) to
/// the given status — used when a ticket is fetched, not found, or
/// unsupported.
pub fn update_discovery_status_rows(
    conn: &Connection,
    source_kind: &str,
    external_id: &str,
    status: &str,
) -> Result<(), String> {
    conn.execute(
        "UPDATE ticket_discoveries SET status = ?1
         WHERE source_kind = ?2 AND external_id = ?3 AND status = ?4",
        params![status, source_kind, external_id, DISCOVERY_STATUS_PENDING],
    )
    .map_err(|e| format!("catalog write failed: {e}"))?;
    Ok(())
}

// ── Ticket relationship graph (Session 33, Parts 6–7) ──────────────

/// Insert a relationship edge. Idempotent: a duplicate edge (same from,
/// target, kind, evidence, and provenance) is ignored. Returns true when
/// a new row was inserted.
pub fn insert_relationship(conn: &Connection, edge: &TicketRelationship) -> Result<bool, String> {
    let result = conn
        .execute(
            "INSERT OR IGNORE INTO ticket_relationships
           (from_event_id, to_event_id, to_external_id, relationship_kind,
            evidence_kind, source_snapshot_id, source_document_id,
            reviewed_status, note, created_utc)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                edge.from_event_id,
                edge.to_event_id,
                edge.to_external_id,
                edge.relationship_kind,
                edge.evidence_kind,
                edge.source_snapshot_id,
                edge.source_document_id,
                edge.reviewed_status,
                edge.note,
                edge.created_utc
            ],
        )
        .map_err(|e| format!("catalog write failed: {e}"))?;
    Ok(result > 0)
}

fn row_to_relationship(row: &rusqlite::Row<'_>) -> rusqlite::Result<TicketRelationship> {
    Ok(TicketRelationship {
        id: row.get(0)?,
        from_event_id: row.get(1)?,
        to_event_id: row.get(2)?,
        to_external_id: row.get(3)?,
        relationship_kind: row.get(4)?,
        evidence_kind: row.get(5)?,
        source_snapshot_id: row.get(6)?,
        source_document_id: row.get(7)?,
        reviewed_status: row.get(8)?,
        note: row.get(9)?,
        created_utc: row.get(10)?,
    })
}

/// All relationship edges, optionally filtered to one event's outgoing
/// edges. Deterministic order.
pub fn list_relationships(
    conn: &Connection,
    from_event_id: Option<i64>,
) -> Result<Vec<TicketRelationship>, String> {
    let mut sql = String::from(
        "SELECT id, from_event_id, to_event_id, to_external_id, relationship_kind,
                evidence_kind, source_snapshot_id, source_document_id,
                reviewed_status, note, created_utc
         FROM ticket_relationships",
    );
    let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
    if let Some(id) = from_event_id {
        sql.push_str(" WHERE from_event_id = ?1");
        params.push(Box::new(id));
    }
    sql.push_str(" ORDER BY from_event_id, to_external_id, id");
    let mut stmt = conn
        .prepare(&sql)
        .map_err(|e| format!("catalog read failed: {e}"))?;
    let rows = stmt
        .query_map(
            rusqlite::params_from_iter(params.iter().map(|p| p.as_ref())),
            row_to_relationship,
        )
        .map_err(|e| format!("catalog read failed: {e}"))?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r.map_err(|e| format!("catalog read failed: {e}"))?);
    }
    Ok(out)
}

/// Resolved neighbor event ids of one event (both directions).
pub fn relationship_neighbors(conn: &Connection, event_id: i64) -> Result<Vec<i64>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT to_event_id FROM ticket_relationships
             WHERE from_event_id = ?1 AND to_event_id IS NOT NULL
             UNION
             SELECT from_event_id FROM ticket_relationships
             WHERE to_event_id = ?1 AND from_event_id IS NOT NULL
             ORDER BY 1",
        )
        .map_err(|e| format!("catalog read failed: {e}"))?;
    let rows = stmt
        .query_map([event_id], |r| r.get::<_, i64>(0))
        .map_err(|e| format!("catalog read failed: {e}"))?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r.map_err(|e| format!("catalog read failed: {e}"))?);
    }
    Ok(out)
}

/// Whether a reviewed edge already exists between an event and a target
/// identifier — re-extraction must never overwrite analyst review.
pub fn has_reviewed_edge(
    conn: &Connection,
    from_event_id: i64,
    to_external_id: &str,
) -> Result<bool, String> {
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM ticket_relationships
             WHERE from_event_id = ?1 AND to_external_id = ?2
               AND reviewed_status != ?3",
            params![from_event_id, to_external_id, REVIEW_UNREVIEWED],
            |r| r.get(0),
        )
        .map_err(|e| format!("catalog read failed: {e}"))?;
    Ok(count > 0)
}
