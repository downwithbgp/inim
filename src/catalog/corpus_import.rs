//! Deterministic import of the tracked reviewed GRNOC corpus
//! (case-studies/<slug>/corpus/).
//!
//! The corpus directory holds immutable public snapshots, the reviewed
//! relationship graph, and (via the pilot ticket-reviews file) the
//! reviewed per-ticket interpretations. Importing creates catalog
//! events with source snapshots, relationships, and reviews ONLY —
//! never Ready plans and never jobs. All timestamps come from the
//! tracked manifest, so the import is deterministic and offline.

use std::path::Path;

use rusqlite::{Connection, OptionalExtension};

use crate::catalog::document::hex_sha256;
use crate::sources::grnoc::ViewerRecord;

/// Import the tracked corpus directory into a catalog.
///
/// `corpus_dir` must contain `manifest.json`, `snapshots/*.json`, and
/// `relationships.json`. Reviews are loaded from the optional
/// `ticket-reviews.json` path (the pilot file is the reviewed
/// interpretation). Returns an import summary.
pub fn import_corpus(conn: &Connection, corpus_dir: &Path) -> Result<CorpusImportSummary, String> {
    let manifest_path = corpus_dir.join("manifest.json");
    let manifest: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(&manifest_path)
            .map_err(|e| format!("corpus manifest unreadable: {e}"))?,
    )
    .map_err(|e| format!("corpus manifest invalid JSON: {e}"))?;
    let snapshots = manifest
        .get("snapshots")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "corpus manifest missing snapshots list".to_string())?;

    let mut summary = CorpusImportSummary::default();
    let mut seen: Vec<(String, String)> = Vec::new(); // (external_id, sha256)

    for entry in snapshots {
        let ext = entry
            .get("external_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "snapshot entry missing external_id".to_string())?
            .to_string();
        let file = entry
            .get("file")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "snapshot entry missing file".to_string())?
            .to_string();
        let fetched_at = entry
            .get("fetched_at")
            .and_then(|v| v.as_str())
            .unwrap_or("1970-01-01T00:00:00Z")
            .to_string();
        let source_url = entry
            .get("source_url")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        // Containment: snapshot paths must stay inside the corpus
        // directory (no .., no absolute, no backslash components).
        if file.starts_with('/')
            || file.split('/').any(|c| c == ".." || c.is_empty())
            || file.contains('\\')
        {
            return Err(format!(
                "corpus manifest snapshot path escapes the corpus directory: {file}"
            ));
        }
        let raw = std::fs::read_to_string(corpus_dir.join(&file))
            .map_err(|e| format!("cannot read corpus snapshot {file}: {e}"))?;
        let sha = hex_sha256(raw.as_bytes());
        if let Some((existing, _)) = seen.iter().find(|(id, _)| id == &ext) {
            return Err(format!(
                "corpus manifest lists duplicate snapshot for {existing}"
            ));
        }
        seen.push((ext.clone(), sha.clone()));

        // A corpus ticket that already has a reviewed Ready plan under
        // ANY source kind (e.g. a reviewed analysis manifest imported
        // from manifests/) is represented by that reviewed analysis
        // event; importing the raw corpus row again would create a
        // duplicate catalog event for the same public ticket.
        let already_reviewed: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM analysis_plans p
                 JOIN manifest_revisions m ON m.id = p.manifest_revision_id
                 JOIN catalog_events e ON e.id = m.event_id
                 WHERE e.external_id = ?1 AND p.status = 'Ready'",
                [&ext],
                |r| r.get(0),
            )
            .unwrap_or(0);
        if already_reviewed > 0 {
            summary.skip_reviewed_tickets.push(ext.clone());
            continue;
        }
        // Parse the stored viewer record and derive the normalized
        // catalog item the same way the live sync does.
        let record: ViewerRecord = serde_json::from_str(&raw)
            .map_err(|e| format!("corpus snapshot {file} is not a viewer record: {e}"))?;
        let grnoc = record.to_grnoc_record();
        let normalized = serde_json::json!({
            "id": grnoc.number,
            "title": grnoc.short_description,
            "task_type": grnoc.task_type,
            "category": grnoc.category,
            "start": grnoc.start,
            "end": grnoc.end,
            "opened": grnoc.opened,
            "state": grnoc.state,
            "state_code": grnoc.state_code,
            "priority": grnoc.priority,
            "priority_code": grnoc.priority_code,
            "planned_start": grnoc.planned_start,
            "planned_end": grnoc.planned_end,
            "maintenance_type": grnoc.maintenance_type,
            "description": grnoc.description,
            "notification_text": grnoc.notification_text,
            "source_url": grnoc.source_url,
        })
        .to_string();

        let event_id = crate::catalog::store::upsert_event(
            conn,
            "grnoc-public-task-viewer",
            &ext,
            &fetched_at,
        )?;
        let snapshot = crate::catalog::domain::EventSnapshot {
            id: 0,
            event_id,
            fetched_at: fetched_at.clone(),
            source_url,
            content_sha256: sha.clone(),
            raw_payload: raw,
            normalized_json: normalized,
            parser_version: "grnoc-viewer-1".to_string(),
        };
        let sid = crate::catalog::store::insert_snapshot(conn, event_id, &snapshot)?;
        summary.events += 1;
        summary.snapshots += 1;
        summary.snapshot_ids.insert(ext.clone(), sid);
    }

    // Relationships (reviewed + derived edges with provenance).
    let rel_path = corpus_dir.join("relationships.json");
    if rel_path.is_file() {
        let rels: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(&rel_path)
                .map_err(|e| format!("cannot read relationships.json: {e}"))?,
        )
        .map_err(|e| format!("relationships.json invalid: {e}"))?;
        if let Some(list) = rels.get("relationships").and_then(|v| v.as_array()) {
            for edge in list {
                let from = edge
                    .get("from_external_id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| "relationship missing from_external_id".to_string())?;
                let to = edge
                    .get("to_external_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let kind = edge
                    .get("relationship_kind")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Related");
                let evidence = edge
                    .get("evidence_kind")
                    .and_then(|v| v.as_str())
                    .unwrap_or("DerivedTemporalOverlap");
                let reviewed = edge
                    .get("reviewed_status")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Unreviewed");
                let note = edge
                    .get("note")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let created = edge
                    .get("created_utc")
                    .and_then(|v| v.as_str())
                    .unwrap_or("1970-01-01T00:00:00Z")
                    .to_string();
                let source_snapshot = edge
                    .get("source_snapshot_ticket")
                    .and_then(|v| v.as_str())
                    .and_then(|t| summary.snapshot_ids.get(t))
                    .copied();
                let from_id = resolve_event_id(conn, from)?;
                let to_id = if to.is_empty() {
                    None
                } else {
                    resolve_event_id(conn, to)?
                };
                if let Some(from_id) = from_id {
                    let edge = crate::catalog::domain::TicketRelationship {
                        id: 0,
                        from_event_id: from_id,
                        to_event_id: to_id,
                        to_external_id: to.to_string(),
                        relationship_kind: kind.to_string(),
                        evidence_kind: evidence.to_string(),
                        source_snapshot_id: source_snapshot,
                        source_document_id: None,
                        reviewed_status: reviewed.to_string(),
                        note: Some(note.clone()),
                        created_utc: created,
                    };
                    crate::catalog::store::insert_relationship(conn, &edge)?;
                    summary.relationships += 1;
                }
                // A relationship whose from-ticket is absent stays an
                // unresolved reference; it is not counted as an event.
                if to_id.is_none() && !to.is_empty() {
                    summary.unresolved_references += 1;
                }
            }
        }
    }

    Ok(summary)
}

/// Import the reviewed per-ticket interpretations (ticket-reviews.json
/// format, as tracked under case-studies/<slug>/pilot/).
pub fn import_reviews(conn: &Connection, reviews_path: &Path) -> Result<usize, String> {
    let content = std::fs::read_to_string(reviews_path)
        .map_err(|e| format!("cannot read reviews file: {e}"))?;
    let value: serde_json::Value =
        serde_json::from_str(&content).map_err(|e| format!("reviews file invalid JSON: {e}"))?;
    let reviewer = value
        .get("reviewer")
        .and_then(|v| v.as_str())
        .unwrap_or("local-review")
        .to_string();
    let reviewed_at = value
        .get("reviewed_at")
        .and_then(|v| v.as_str())
        .unwrap_or("1970-01-01T00:00:00Z")
        .to_string();
    let mut count = 0;
    if let Some(reviews) = value.get("reviews").and_then(|v| v.as_array()) {
        for r in reviews {
            let ext = r
                .get("external_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "review missing external_id".to_string())?;
            let Some(event) =
                crate::catalog::db::get_event_by_external(conn, "grnoc-public-task-viewer", ext)?
            else {
                continue; // unresolved reference, not an event
            };
            let roles = r
                .get("roles")
                .cloned()
                .unwrap_or(serde_json::json!([]))
                .to_string();
            let labels = r
                .get("entity_labels")
                .cloned()
                .unwrap_or(serde_json::json!([]))
                .to_string();
            let links = r
                .get("linked_change_ids")
                .cloned()
                .unwrap_or(serde_json::json!([]))
                .to_string();
            let applicability = r
                .get("analysis_applicability")
                .and_then(|v| v.as_str())
                .unwrap_or("NotApplicableToPublicBgp")
                .to_string();
            let rationale = r
                .get("applicability_rationale")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let relationship = r
                .get("relationship_to_case_study")
                .and_then(|v| v.as_str())
                .unwrap_or("Related")
                .to_string();
            let provenance = r
                .get("provenance")
                .cloned()
                .unwrap_or(serde_json::json!([]));
            conn.execute(
                "INSERT INTO ticket_reviews (catalog_event_id, external_id, reviewed_roles_json,
                     entity_labels_json, linked_change_ids_json, analysis_applicability,
                     applicability_rationale, relationship_to_case_study, review_status,
                     reviewer, reviewed_at, provenance_json)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'Reviewed', ?9, ?10, ?11)",
                rusqlite::params![
                    event.id,
                    ext,
                    roles,
                    labels,
                    links,
                    applicability,
                    rationale,
                    relationship,
                    reviewer,
                    reviewed_at,
                    provenance.to_string(),
                ],
            )
            .map_err(|e| format!("cannot insert ticket review for {ext}: {e}"))?;
            count += 1;
        }
    }
    Ok(count)
}

/// Resolve a ticket to a catalog event: first under the corpus source
/// kind, then (for tickets with a reviewed analysis event) under any
/// other source kind.
fn resolve_event_id(conn: &Connection, external_id: &str) -> Result<Option<i64>, String> {
    if let Some(e) =
        crate::catalog::db::get_event_by_external(conn, "grnoc-public-task-viewer", external_id)?
    {
        return Ok(Some(e.id));
    }
    conn.query_row(
        "SELECT id FROM catalog_events WHERE external_id = ?1 ORDER BY id LIMIT 1",
        [external_id],
        |r| r.get(0),
    )
    .optional()
    .map_err(|e| format!("cannot resolve event {external_id}: {e}"))
}

/// Result of a corpus import.
#[derive(Debug, Default)]
pub struct CorpusImportSummary {
    pub events: usize,
    pub snapshots: usize,
    pub relationships: usize,
    pub unresolved_references: usize,
    /// Tickets skipped because a reviewed Ready plan already exists.
    pub skip_reviewed_tickets: Vec<String>,
    snapshot_ids: std::collections::HashMap<String, i64>,
}

/// Round-trip guard used by tests: the manifest snapshot list must
/// equal the number of snapshot files on disk.
pub fn validate_corpus_directory(corpus_dir: &Path) -> Result<CorpusDirCheck, String> {
    let manifest: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(corpus_dir.join("manifest.json"))
            .map_err(|e| format!("corpus manifest unreadable: {e}"))?,
    )
    .map_err(|e| format!("corpus manifest invalid: {e}"))?;
    let listed = manifest
        .get("snapshots")
        .and_then(|v| v.as_array())
        .map(|a| a.len())
        .unwrap_or(0);
    let on_disk = std::fs::read_dir(corpus_dir.join("snapshots"))
        .map(|it| {
            it.filter_map(|e| e.ok())
                .filter(|e| e.path().is_file())
                .count()
        })
        .unwrap_or(0);
    Ok(CorpusDirCheck {
        listed,
        on_disk,
        consistent: listed == on_disk,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CorpusDirCheck {
    pub listed: usize,
    pub on_disk: usize,
    pub consistent: bool,
}
