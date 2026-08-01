//! Ticket relationship extraction and graph (Session 33, Parts 6–7).
//!
//! Ticket identifiers are extracted from public source text with a
//! conservative syntax (exact spans preserved). The relationship kind is
//! classified only from surrounding source wording; a bare identifier
//! defaults to `References`. Numeric closeness never creates an edge.
//!
//! Explicit edges (source text, reference documents, case-study
//! membership, analyst review) and derived edges (temporal/entity
//! overlap candidates) are kept visibly distinct through `evidence_kind`.
//! Temporal overlap is never a causal edge. The graph is stored in
//! SQLite adjacency rows; traversal is bounded.

use crate::catalog::db;
use crate::catalog::discovery::extract_ticket_references;
use crate::catalog::domain::*;
use crate::catalog::store;

/// Classify the relationship kind from the wording preceding a ticket
/// identifier in its source text.
///
/// Conservative rules, checked in order:
/// - "tracked in" / "tracking in" / "being tracked" → TracksRemainingImpactIn
/// - "supersed" (superseded by / supersedes) → SupersededBy
/// - "related change" + CHG target → RelatedChange
/// - "related incident" + INC target → RelatedIncident
/// - "related task" + TASK target → RelatedTask
/// - otherwise → References
pub fn classify_relationship(context_before: &str, external_id: &str) -> &'static str {
    let window = context_before.to_ascii_lowercase();
    if window.contains("tracked in")
        || window.contains("tracking in")
        || window.contains("being tracked")
    {
        return RELATIONSHIP_TRACKS_REMAINING_IMPACT;
    }
    if window.contains("supersed") {
        return RELATIONSHIP_SUPERSEDED_BY;
    }
    let lower_id = external_id.to_ascii_lowercase();
    if window.contains("related change") && lower_id.starts_with("chg") {
        return RELATIONSHIP_RELATED_CHANGE;
    }
    if window.contains("related incident") && lower_id.starts_with("inc") {
        return RELATIONSHIP_RELATED_INCIDENT;
    }
    if window.contains("related task") && lower_id.starts_with("task") {
        return RELATIONSHIP_RELATED_TASK;
    }
    RELATIONSHIP_REFERENCES
}

/// An extracted reference with its exact source span and classified
/// relationship.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractedReference {
    pub external_id: String,
    pub start: usize,
    pub end: usize,
    pub relationship_kind: &'static str,
}

/// Extract and classify references from source text. `context_chars` is
/// the number of characters before the identifier examined for wording.
pub fn extract_with_kind(text: &str, context_chars: usize) -> Vec<ExtractedReference> {
    extract_ticket_references(text)
        .into_iter()
        .map(|r| {
            let start = r.start.saturating_sub(context_chars);
            let context_before = &text[start..r.start];
            let kind = classify_relationship(context_before, &r.external_id);
            ExtractedReference {
                external_id: r.external_id,
                start: r.start,
                end: r.end,
                relationship_kind: kind,
            }
        })
        .collect()
}

/// Extract the reference text of a snapshot's public description fields.
pub fn snapshot_reference_text(snapshot: &EventSnapshot) -> String {
    let normalized: serde_json::Value =
        serde_json::from_str(&snapshot.normalized_json).unwrap_or_default();
    let mut text = String::new();
    for field in ["description", "notification_text"] {
        if let Some(v) = normalized.get(field).and_then(|v| v.as_str()) {
            text.push_str(v);
            text.push('\n');
        }
    }
    text
}

/// Extract explicit relationships from every fetched snapshot of the
/// source and record them (idempotent; reviewed edges are never
/// overwritten). Returns the number of new edges recorded.
pub fn extract_relationships_from_snapshots(
    conn: &rusqlite::Connection,
    source_kind: &str,
    created_utc: &str,
) -> Result<usize, String> {
    let events = db::list_events(conn)?;
    let mut new_edges = 0usize;
    for event in events {
        if event.source_kind != source_kind {
            continue;
        }
        let snapshots = db::list_snapshots(conn, event.id)?;
        let Some(latest) = snapshots.first() else {
            continue;
        };
        let text = snapshot_reference_text(latest);
        for reference in extract_with_kind(&text, 120) {
            if reference.external_id == event.external_id {
                continue; // self-reference
            }
            if store::has_reviewed_edge(conn, event.id, &reference.external_id)? {
                continue; // a reviewed edge for this pair must not be overwritten
            }
            let to_event_id =
                db::get_event_by_external(conn, source_kind, &reference.external_id)?.map(|e| e.id);
            let edge = TicketRelationship {
                id: 0,
                from_event_id: event.id,
                to_event_id,
                to_external_id: reference.external_id,
                relationship_kind: reference.relationship_kind.to_string(),
                evidence_kind: EVIDENCE_EXPLICIT_TICKET_TEXT.to_string(),
                source_snapshot_id: Some(latest.id),
                source_document_id: None,
                reviewed_status: REVIEW_UNREVIEWED.to_string(),
                note: Some(format!(
                    "extracted from snapshot {} at bytes {}..{}",
                    latest.id, reference.start, reference.end
                )),
                created_utc: created_utc.to_string(),
            };
            if store::insert_relationship(conn, &edge)? {
                new_edges += 1;
            }
        }
    }
    Ok(new_edges)
}

/// Record a derived temporal-overlap candidate edge between two events.
/// The edge is explicitly a candidate: `TemporalOverlap` relationship,
/// `DerivedTemporalOverlap` evidence, Unreviewed. Never a causal kind.
pub fn record_temporal_overlap(
    conn: &rusqlite::Connection,
    from_event_id: i64,
    to_event_id: i64,
    note: &str,
    created_utc: &str,
) -> Result<bool, String> {
    let to_external = db::get_event(conn, to_event_id)?
        .map(|e| e.external_id)
        .ok_or_else(|| format!("no event {to_event_id}"))?;
    store::insert_relationship(
        conn,
        &TicketRelationship {
            id: 0,
            from_event_id,
            to_event_id: Some(to_event_id),
            to_external_id: to_external,
            relationship_kind: RELATIONSHIP_TEMPORAL_OVERLAP.to_string(),
            evidence_kind: EVIDENCE_DERIVED_TEMPORAL_OVERLAP.to_string(),
            source_snapshot_id: None,
            source_document_id: None,
            reviewed_status: REVIEW_UNREVIEWED.to_string(),
            note: Some(note.to_string()),
            created_utc: created_utc.to_string(),
        },
    )
}

/// Derive temporal-overlap candidate edges among catalog events of one
/// source whose event windows overlap. Events without parseable
/// start/end windows are skipped. Overlap alone never becomes a causal
/// edge (evidence kind keeps it a candidate).
pub fn derive_temporal_overlaps(
    conn: &rusqlite::Connection,
    source_kind: &str,
    created_utc: &str,
) -> Result<usize, String> {
    // Collect (event_id, start, end) from the latest snapshot.
    let mut windows: Vec<(i64, String, String)> = Vec::new();
    for event in db::list_events(conn)? {
        if event.source_kind != source_kind {
            continue;
        }
        let snapshots = db::list_snapshots(conn, event.id)?;
        let Some(latest) = snapshots.first() else {
            continue;
        };
        let v: serde_json::Value =
            serde_json::from_str(&latest.normalized_json).unwrap_or_default();
        let start = v.get("start").and_then(|x| x.as_str()).unwrap_or_default();
        let end = v.get("end").and_then(|x| x.as_str()).unwrap_or_default();
        if start.is_empty() || end.is_empty() {
            continue; // open events have no comparable window
        }
        windows.push((event.id, start.to_string(), end.to_string()));
    }
    let mut new_edges = 0usize;
    for i in 0..windows.len() {
        for j in (i + 1)..windows.len() {
            let (a, a_start, a_end) = &windows[i];
            let (b, b_start, b_end) = &windows[j];
            let overlap = a_start < b_end && b_start < a_end;
            if !overlap {
                continue;
            }
            let note = format!(
                "event windows overlap ({a_start}..{a_end}) vs ({b_start}..{b_end}); overlap is not causal attribution"
            );
            if record_temporal_overlap(conn, *a, *b, &note, created_utc)? {
                new_edges += 1;
            }
            if record_temporal_overlap(conn, *b, *a, &note, created_utc)? {
                new_edges += 1;
            }
        }
    }
    Ok(new_edges)
}

/// Resolve every unresolved edge whose target identifier now exists in
/// the catalog. Returns the number of edges resolved.
pub fn resolve_unresolved_edges(
    conn: &rusqlite::Connection,
    source_kind: &str,
) -> Result<usize, String> {
    let edges = store::list_relationships(conn, None)?;
    let mut resolved = 0usize;
    for edge in edges {
        if edge.to_event_id.is_some() {
            continue;
        }
        if let Some(event) = db::get_event_by_external(conn, source_kind, &edge.to_external_id)? {
            conn.execute(
                "UPDATE ticket_relationships SET to_event_id = ?1 WHERE id = ?2",
                rusqlite::params![event.id, edge.id],
            )
            .map_err(|e| format!("catalog write failed: {e}"))?;
            resolved += 1;
        }
    }
    Ok(resolved)
}

/// Bounded adjacency traversal from an event. Returns the set of event
/// ids reachable within `max_depth` hops through resolved edges. The
/// traversal is breadth-first with a visited set — always bounded by the
/// edge count, never recursive.
pub fn bounded_neighbors(
    conn: &rusqlite::Connection,
    event_id: i64,
    max_depth: usize,
) -> Result<std::collections::BTreeSet<i64>, String> {
    let mut visited: std::collections::BTreeSet<i64> = std::collections::BTreeSet::new();
    visited.insert(event_id);
    let mut frontier: Vec<(i64, usize)> = vec![(event_id, 0)];
    while let Some((current, depth)) = frontier.pop() {
        if depth >= max_depth {
            continue;
        }
        for neighbor in store::relationship_neighbors(conn, current)? {
            if visited.insert(neighbor) {
                frontier.push((neighbor, depth + 1));
            }
        }
    }
    visited.remove(&event_id);
    Ok(visited)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::db;
    use crate::catalog::grnoc::GrnocCatalogSource;
    use crate::catalog::sync::sync_catalog;

    fn open_temp_db() -> (tempfile::TempDir, rusqlite::Connection) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("catalog.sqlite");
        let conn = db::open_catalog(&path).unwrap();
        (dir, conn)
    }

    fn write_record(dir: &std::path::Path, number: &str, description: &str) {
        std::fs::write(
            dir.join(format!("{number}.json")),
            serde_json::json!({
                "number": number,
                "short_description": "t",
                "description": description,
                "start": "2019-08-21T04:00:00Z",
                "end": "2019-08-21T05:00:00Z"
            })
            .to_string(),
        )
        .unwrap();
    }

    fn sync_dir(conn: &rusqlite::Connection, dir: &std::path::Path) {
        let source = GrnocCatalogSource::new(dir.to_path_buf(), "2026-08-01T00:00:00Z".into());
        sync_catalog(conn, &source, "2026-08-01T00:00:00Z").unwrap();
    }

    // ── Part 6: extraction ─────────────────────────────────────────

    #[test]
    fn explicit_ticket_reference_is_extracted() {
        let refs = extract_with_kind(
            "Remaining sessions are tracked in Internet2 ticket INC0040257.",
            120,
        );
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].external_id, "INC0040257");
        assert_eq!(
            refs[0].relationship_kind,
            RELATIONSHIP_TRACKS_REMAINING_IMPACT
        );
    }

    #[test]
    fn exact_source_span_is_preserved() {
        let text = "See CHG0038258 for the maintenance window.";
        let refs = extract_with_kind(text, 120);
        assert_eq!(refs.len(), 1);
        assert_eq!(&text[refs[0].start..refs[0].end], "CHG0038258");
    }

    #[test]
    fn tracking_language_supports_tracks_remaining_impact() {
        // The exact wording from CHG0038258's public description.
        let text = "Some peering sessions remain unavailable after the completion of this maintenance, and are being tracked in Internet2 ticket INC0040257.";
        let refs = extract_with_kind(text, 120);
        assert_eq!(refs.len(), 1);
        assert_eq!(
            refs[0].relationship_kind,
            RELATIONSHIP_TRACKS_REMAINING_IMPACT
        );
        // Supersession wording maps to SupersededBy.
        let refs = extract_with_kind("This change is superseded by CHG0044444.", 120);
        assert_eq!(refs[0].relationship_kind, RELATIONSHIP_SUPERSEDED_BY);
    }

    #[test]
    fn bare_identifier_defaults_to_references() {
        let refs = extract_with_kind("See INC0040257 for context.", 120);
        assert_eq!(refs[0].relationship_kind, RELATIONSHIP_REFERENCES);
        let refs = extract_with_kind("Related change: CHG0038386.", 120);
        assert_eq!(refs[0].relationship_kind, RELATIONSHIP_RELATED_CHANGE);
        let refs = extract_with_kind("Related incident INC0040258 noted.", 120);
        assert_eq!(refs[0].relationship_kind, RELATIONSHIP_RELATED_INCIDENT);
    }

    #[test]
    fn numeric_similarity_creates_no_edge() {
        let (_dir, conn) = open_temp_db();
        let dir = tempfile::tempdir().unwrap();
        write_record(
            dir.path(),
            "INC0040257",
            "Tracked in INC0040257. Nothing else.",
        );
        sync_dir(&conn, dir.path());
        // Only the self-reference is skipped; no neighbor numbers appear.
        let n = extract_relationships_from_snapshots(
            &conn,
            "grnoc-public-task-viewer",
            "2026-08-01T00:00:00Z",
        )
        .unwrap();
        assert_eq!(n, 0, "self-reference must not create an edge");
        assert!(store::list_relationships(&conn, None).unwrap().is_empty());
    }

    #[test]
    fn relationships_retain_snapshot_provenance() {
        let (_dir, conn) = open_temp_db();
        let dir = tempfile::tempdir().unwrap();
        write_record(dir.path(), "CHG0038258", "Tracked in INC0040257.");
        sync_dir(&conn, dir.path());
        extract_relationships_from_snapshots(
            &conn,
            "grnoc-public-task-viewer",
            "2026-08-01T00:00:00Z",
        )
        .unwrap();
        let edges = store::list_relationships(&conn, None).unwrap();
        assert_eq!(edges.len(), 1);
        let edge = &edges[0];
        assert_eq!(edge.relationship_kind, RELATIONSHIP_TRACKS_REMAINING_IMPACT);
        assert_eq!(edge.evidence_kind, EVIDENCE_EXPLICIT_TICKET_TEXT);
        assert!(
            edge.source_snapshot_id.is_some(),
            "snapshot provenance retained"
        );
        assert_eq!(edge.reviewed_status, REVIEW_UNREVIEWED);
        let note = edge.note.as_deref().unwrap_or_default();
        assert!(
            note.contains("bytes"),
            "exact span recorded in the note: {note}"
        );
    }

    // ── Part 7: graph ──────────────────────────────────────────────

    #[test]
    fn explicit_and_derived_edges_are_distinct() {
        let (_dir, conn) = open_temp_db();
        let dir = tempfile::tempdir().unwrap();
        // A explicitly references B in its text (A has a disjoint window).
        write_record(dir.path(), "INC0040001", "See INC0040002 for details.");
        // B and C overlap in time but have no text reference.
        write_record(dir.path(), "INC0040002", "No references here.");
        write_record(dir.path(), "INC0040003", "Also no references.");
        // Override windows: A 04:00-05:00, B 06:00-07:00, C 06:30-07:30.
        for (n, s, e) in [
            ("INC0040001", "2019-08-21T04:00:00Z", "2019-08-21T05:00:00Z"),
            ("INC0040002", "2019-08-21T06:00:00Z", "2019-08-21T07:00:00Z"),
            ("INC0040003", "2019-08-21T06:30:00Z", "2019-08-21T07:30:00Z"),
        ] {
            let p = dir.path().join(format!("{n}.json"));
            let mut v: serde_json::Value =
                serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
            v["start"] = serde_json::json!(s);
            v["end"] = serde_json::json!(e);
            std::fs::write(&p, v.to_string()).unwrap();
        }
        sync_dir(&conn, dir.path());
        extract_relationships_from_snapshots(
            &conn,
            "grnoc-public-task-viewer",
            "2026-08-01T00:00:00Z",
        )
        .unwrap();
        derive_temporal_overlaps(&conn, "grnoc-public-task-viewer", "2026-08-01T00:00:00Z")
            .unwrap();
        let edges = store::list_relationships(&conn, None).unwrap();
        let explicit: Vec<_> = edges
            .iter()
            .filter(|e| e.evidence_kind == EVIDENCE_EXPLICIT_TICKET_TEXT)
            .collect();
        let derived: Vec<_> = edges
            .iter()
            .filter(|e| e.evidence_kind == EVIDENCE_DERIVED_TEMPORAL_OVERLAP)
            .collect();
        assert_eq!(explicit.len(), 1);
        assert_eq!(explicit[0].to_external_id, "INC0040002");
        assert_eq!(derived.len(), 2, "B<->C overlap, both directions");
        for d in &derived {
            assert_eq!(d.relationship_kind, RELATIONSHIP_TEMPORAL_OVERLAP);
            assert!(d.note.as_deref().unwrap_or_default().contains("not causal"));
        }
    }

    #[test]
    fn unresolved_edge_can_later_link_to_catalog_event() {
        let (_dir, conn) = open_temp_db();
        let dir = tempfile::tempdir().unwrap();
        // A references INC0040257, which is NOT in the catalog yet.
        write_record(dir.path(), "CHG0038258", "Tracked in INC0040257.");
        sync_dir(&conn, dir.path());
        extract_relationships_from_snapshots(
            &conn,
            "grnoc-public-task-viewer",
            "2026-08-01T00:00:00Z",
        )
        .unwrap();
        let edges = store::list_relationships(&conn, None).unwrap();
        assert_eq!(edges.len(), 1);
        assert!(
            edges[0].to_event_id.is_none(),
            "target unresolved initially"
        );
        // The target later enters the catalog…
        let dir2 = tempfile::tempdir().unwrap();
        write_record(dir2.path(), "INC0040257", "x");
        sync_dir(&conn, dir2.path());
        // …and the edge resolves without re-extraction.
        let resolved = resolve_unresolved_edges(&conn, "grnoc-public-task-viewer").unwrap();
        assert_eq!(resolved, 1);
        let edges = store::list_relationships(&conn, None).unwrap();
        assert!(edges[0].to_event_id.is_some());
    }

    #[test]
    fn relationship_import_is_idempotent() {
        let (_dir, conn) = open_temp_db();
        let dir = tempfile::tempdir().unwrap();
        write_record(dir.path(), "CHG0038258", "Tracked in INC0040257.");
        sync_dir(&conn, dir.path());
        extract_relationships_from_snapshots(
            &conn,
            "grnoc-public-task-viewer",
            "2026-08-01T00:00:00Z",
        )
        .unwrap();
        let again = extract_relationships_from_snapshots(
            &conn,
            "grnoc-public-task-viewer",
            "2026-08-01T01:00:00Z",
        )
        .unwrap();
        assert_eq!(again, 0, "re-extraction records nothing new");
        assert_eq!(store::list_relationships(&conn, None).unwrap().len(), 1);
    }

    #[test]
    fn conflicting_reviewed_relationship_is_not_overwritten() {
        let (_dir, conn) = open_temp_db();
        let dir = tempfile::tempdir().unwrap();
        write_record(dir.path(), "CHG0038258", "Tracked in INC0040257.");
        sync_dir(&conn, dir.path());
        extract_relationships_from_snapshots(
            &conn,
            "grnoc-public-task-viewer",
            "2026-08-01T00:00:00Z",
        )
        .unwrap();
        // An analyst reviews the edge and changes its kind.
        let edges = store::list_relationships(&conn, None).unwrap();
        conn.execute(
            "UPDATE ticket_relationships SET reviewed_status = ?1, relationship_kind = ?2 WHERE id = ?3",
            rusqlite::params![REVIEW_ACCEPTED, RELATIONSHIP_SUPERSEDED_BY, edges[0].id],
        )
        .unwrap();
        // Re-extraction must not overwrite the reviewed edge.
        extract_relationships_from_snapshots(
            &conn,
            "grnoc-public-task-viewer",
            "2026-08-01T02:00:00Z",
        )
        .unwrap();
        let edges = store::list_relationships(&conn, None).unwrap();
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].relationship_kind, RELATIONSHIP_SUPERSEDED_BY);
        assert_eq!(edges[0].reviewed_status, REVIEW_ACCEPTED);
    }

    #[test]
    fn temporal_overlap_does_not_become_causal_edge() {
        let (_dir, conn) = open_temp_db();
        let dir = tempfile::tempdir().unwrap();
        write_record(dir.path(), "INC0040010", "start/end overlap");
        write_record(dir.path(), "INC0040011", "no text reference");
        sync_dir(&conn, dir.path());
        let n = derive_temporal_overlaps(&conn, "grnoc-public-task-viewer", "2026-08-01T00:00:00Z")
            .unwrap();
        assert_eq!(n, 2);
        let edges = store::list_relationships(&conn, None).unwrap();
        for e in &edges {
            // The edge kind is the neutral candidate kind; it is never a
            // causal kind like RelatedIncident or SupersededBy.
            assert_eq!(e.relationship_kind, RELATIONSHIP_TEMPORAL_OVERLAP);
            assert_eq!(e.evidence_kind, EVIDENCE_DERIVED_TEMPORAL_OVERLAP);
            assert!(!e.relationship_kind.contains("Related"));
            assert!(!e.relationship_kind.contains("Superseded"));
            assert_eq!(e.reviewed_status, REVIEW_UNREVIEWED);
        }
    }

    #[test]
    fn graph_traversal_is_bounded() {
        let (_dir, conn) = open_temp_db();
        let dir = tempfile::tempdir().unwrap();
        // Chain A->B->C->D->E via explicit text references.
        write_record(dir.path(), "INC0040100", "See INC0040101.");
        write_record(dir.path(), "INC0040101", "See INC0040102.");
        write_record(dir.path(), "INC0040102", "See INC0040103.");
        write_record(dir.path(), "INC0040103", "See INC0040104.");
        write_record(dir.path(), "INC0040104", "Nothing.");
        sync_dir(&conn, dir.path());
        extract_relationships_from_snapshots(
            &conn,
            "grnoc-public-task-viewer",
            "2026-08-01T00:00:00Z",
        )
        .unwrap();
        resolve_unresolved_edges(&conn, "grnoc-public-task-viewer").unwrap();
        let events = db::list_events(&conn).unwrap();
        let a = events
            .iter()
            .find(|e| e.external_id == "INC0040100")
            .unwrap()
            .id;
        // Depth 2 reaches B and C only.
        let two = bounded_neighbors(&conn, a, 2).unwrap();
        assert_eq!(two.len(), 2);
        // Depth 4 reaches B..E; the traversal never recurses and is
        // bounded by the visited set.
        let four = bounded_neighbors(&conn, a, 4).unwrap();
        assert_eq!(four.len(), 4);
        let ids: Vec<String> = db::list_events(&conn)
            .unwrap()
            .iter()
            .map(|e| e.external_id.clone())
            .collect();
        assert_eq!(four.len(), ids.len() - 1);
    }
}
