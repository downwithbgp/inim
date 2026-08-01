//! Corpus discovery — how ticket identifiers enter the catalog
//! (Session 33, Part 3).
//!
//! Supported discovery modes:
//!
//! - **Seed list** — exact ticket IDs supplied by analyst input,
//!   case-study metadata, imported documents, or existing catalog
//!   references (`AnalystSeed`, `CaseStudyReference`,
//!   `DocumentReference`).
//! - **Reference expansion** — ticket identifiers extracted from already
//!   fetched public descriptions (`TicketDescriptionReference`).
//! - **Public search/list** — an official or clearly public viewer
//!   search/list mechanism (`PublicSearchResult`), used only with an
//!   explicit reviewed domain and query (see the protocol audit).
//!
//! There is **no** blind sequential enumeration (`INC0000001`,
//! `INC0000002`, ...). The extractor never fabricates identifiers from
//! numeric closeness; every discovered ID records its provenance.

use regex::Regex;

use crate::catalog::db;
use crate::catalog::domain::*;
use crate::catalog::store;

/// Conservative ticket-identifier syntax: family + at least 5 digits,
/// word-bounded. Matches `INC0040257`, `CHG0038258`, `TASK0038206`.
pub fn ticket_identifier_regex() -> Regex {
    Regex::new(r"\b(INC|CHG|TASK)[0-9]{5,}\b").expect("static identifier regex")
}

/// One extracted ticket identifier with its exact source span.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TicketReference {
    pub external_id: String,
    /// Byte offset of the first character in the source text.
    pub start: usize,
    /// Byte offset one past the last character.
    pub end: usize,
}

/// Extract ticket identifiers from source text with a conservative
/// syntax. Preserves the exact source span. Never infers neighbors from
/// numeric closeness — a text containing only `INC0040257` yields only
/// `INC0040257`.
pub fn extract_ticket_references(text: &str) -> Vec<TicketReference> {
    ticket_identifier_regex()
        .find_iter(text)
        .map(|m| TicketReference {
            external_id: m.as_str().to_string(),
            start: m.start(),
            end: m.end(),
        })
        .collect()
}

/// Record an analyst-supplied seed ticket.
pub fn record_analyst_seed(
    conn: &rusqlite::Connection,
    source_kind: &str,
    external_id: &str,
    discovered_at: &str,
) -> Result<i64, String> {
    store::record_discovery(
        conn,
        &TicketDiscovery {
            id: 0,
            source_kind: source_kind.to_string(),
            external_id: external_id.to_string(),
            provenance: DISCOVERY_ANALYST_SEED.to_string(),
            source_snapshot_id: None,
            source_document_id: None,
            discovered_at: discovered_at.to_string(),
            status: DISCOVERY_STATUS_PENDING.to_string(),
        },
    )
}

/// Record ticket identifiers referenced by a reviewed case study's
/// event links. This records discovery provenance only — it never
/// creates catalog events or source snapshots.
pub fn record_case_study_references(
    conn: &rusqlite::Connection,
    source_kind: &str,
    case_study_id: i64,
    discovered_at: &str,
) -> Result<usize, String> {
    let mut stmt = conn
        .prepare(
            "SELECT external_identifier FROM case_study_event_links
             WHERE case_study_id = ?1 ORDER BY sort_order, id",
        )
        .map_err(|e| format!("catalog read failed: {e}"))?;
    let rows = stmt
        .query_map([case_study_id], |r| r.get::<_, String>(0))
        .map_err(|e| format!("catalog read failed: {e}"))?;
    let mut count = 0usize;
    for row in rows {
        let external_id = row.map_err(|e| format!("catalog read failed: {e}"))?;
        store::record_discovery(
            conn,
            &TicketDiscovery {
                id: 0,
                source_kind: source_kind.to_string(),
                external_id,
                provenance: DISCOVERY_CASE_STUDY_REFERENCE.to_string(),
                source_snapshot_id: None,
                source_document_id: None,
                discovered_at: discovered_at.to_string(),
                status: DISCOVERY_STATUS_PENDING.to_string(),
            },
        )?;
        count += 1;
    }
    Ok(count)
}

/// Record ticket identifiers named by a reviewed reference document.
/// Discovery only — no source snapshot is fabricated for the tickets.
pub fn record_document_references(
    conn: &rusqlite::Connection,
    source_kind: &str,
    document_id: i64,
    external_ids: &[String],
    discovered_at: &str,
) -> Result<usize, String> {
    let mut count = 0usize;
    for external_id in external_ids {
        store::record_discovery(
            conn,
            &TicketDiscovery {
                id: 0,
                source_kind: source_kind.to_string(),
                external_id: external_id.clone(),
                provenance: DISCOVERY_DOCUMENT_REFERENCE.to_string(),
                source_snapshot_id: None,
                source_document_id: Some(document_id),
                discovered_at: discovered_at.to_string(),
                status: DISCOVERY_STATUS_PENDING.to_string(),
            },
        )?;
        count += 1;
    }
    Ok(count)
}

/// Reference expansion: extract ticket identifiers from the public
/// description text of every fetched snapshot (latest snapshot per
/// event) and record `TicketDescriptionReference` discoveries.
///
/// Self-references (a ticket naming itself) are skipped. The returned
/// count is the number of NEW discovery rows recorded.
pub fn expand_from_snapshots(
    conn: &rusqlite::Connection,
    source_kind: &str,
    discovered_at: &str,
) -> Result<usize, String> {
    let events = db::list_events(conn)?;
    let mut new_rows = 0usize;
    for event in events {
        if event.source_kind != source_kind {
            continue;
        }
        let snapshots = db::list_snapshots(conn, event.id)?;
        let Some(latest) = snapshots.first() else {
            continue;
        };
        let normalized: serde_json::Value =
            serde_json::from_str(&latest.normalized_json).unwrap_or_default();
        let mut text = String::new();
        for field in ["description", "notification_text", "title"] {
            if let Some(v) = normalized.get(field).and_then(|v| v.as_str()) {
                text.push_str(v);
                text.push('\n');
            }
        }
        for reference in extract_ticket_references(&text) {
            if reference.external_id == event.external_id {
                continue; // self-reference
            }
            let before = store::list_discoveries(conn, source_kind, None)?.len();
            store::record_discovery(
                conn,
                &TicketDiscovery {
                    id: 0,
                    source_kind: source_kind.to_string(),
                    external_id: reference.external_id,
                    provenance: DISCOVERY_DESCRIPTION_REFERENCE.to_string(),
                    source_snapshot_id: Some(latest.id),
                    source_document_id: None,
                    discovered_at: discovered_at.to_string(),
                    status: DISCOVERY_STATUS_PENDING.to_string(),
                },
            )?;
            let after = store::list_discoveries(conn, source_kind, None)?.len();
            if after > before {
                new_rows += 1;
            }
        }
    }
    Ok(new_rows)
}

/// Apply the per-sync request budget to the frontier: the caller may
/// fetch at most `budget` IDs this run; the rest stay Pending and are
/// resumed by the next sync run. Deterministic order.
pub fn budgeted_frontier(frontier: &[String], budget: usize) -> &[String] {
    let n = frontier.len().min(budget);
    &frontier[..n]
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

    #[test]
    fn seed_discovery_records_provenance() {
        let (_dir, conn) = open_temp_db();
        let id = record_analyst_seed(
            &conn,
            "grnoc-public-task-viewer",
            "INC0040257",
            "2026-08-01T00:00:00Z",
        )
        .unwrap();
        let rows = store::list_discoveries(&conn, "grnoc-public-task-viewer", None).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, id);
        assert_eq!(rows[0].external_id, "INC0040257");
        assert_eq!(rows[0].provenance, DISCOVERY_ANALYST_SEED);
        assert_eq!(rows[0].status, DISCOVERY_STATUS_PENDING);
        let frontier = store::pending_frontier(&conn, "grnoc-public-task-viewer").unwrap();
        assert_eq!(frontier, vec!["INC0040257"]);
    }

    #[test]
    fn document_reference_is_not_a_source_snapshot() {
        let (_dir, conn) = open_temp_db();
        // Seed a reference document row so the FK is satisfiable.
        let doc = ReferenceDocument {
            id: 0,
            title: "AAR".to_string(),
            source_url: Some("https://example.invalid/aar.pdf".to_string()),
            doc_type: "AfterActionReport".to_string(),
            redistribution_status: "Unknown".to_string(),
            publication_date: None,
            provenance: "test".to_string(),
            imported_utc: "2026-08-01T00:00:00Z".to_string(),
        };
        let doc_id = store::insert_reference_document(&conn, &doc).unwrap();
        let ids = vec!["CHG0038258".to_string(), "INC0040257".to_string()];
        let n = record_document_references(
            &conn,
            "grnoc-public-task-viewer",
            doc_id,
            &ids,
            "2026-08-01T00:00:00Z",
        )
        .unwrap();
        assert_eq!(n, 2);
        let rows = store::list_discoveries(&conn, "grnoc-public-task-viewer", None).unwrap();
        assert_eq!(rows.len(), 2);
        assert!(rows
            .iter()
            .all(|r| r.provenance == DISCOVERY_DOCUMENT_REFERENCE));
        assert!(rows.iter().all(|r| r.source_document_id == Some(doc_id)));
        // Discovery never fabricates catalog events or source snapshots.
        assert!(db::list_events(&conn).unwrap().is_empty());
        assert!(
            store::list_discoveries(&conn, "grnoc-public-task-viewer", None)
                .unwrap()
                .iter()
                .all(|r| r.source_snapshot_id.is_none())
        );
    }

    #[test]
    fn description_reference_enters_fetch_frontier() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        // A fetched ticket whose public description names INC0040257.
        std::fs::write(
            src.join("CHG0038258.json"),
            r#"{
                "number": "CHG0038258",
                "short_description": "Maintenance - MAN LAN",
                "description": "Some peering sessions remain unavailable and are being tracked in Internet2 ticket INC0040257.",
                "start": "2019-08-21T04:00:00Z"
            }"#,
        )
        .unwrap();
        let path = dir.path().join("catalog.sqlite");
        let conn = db::open_catalog(&path).unwrap();
        let source = GrnocCatalogSource::new(src.clone(), "2026-08-01T00:00:00Z".into());
        sync_catalog(&conn, &source, "2026-08-01T00:00:00Z").unwrap();

        let new_rows =
            expand_from_snapshots(&conn, "grnoc-public-task-viewer", "2026-08-01T01:00:00Z")
                .unwrap();
        assert_eq!(new_rows, 1);
        let frontier = store::pending_frontier(&conn, "grnoc-public-task-viewer").unwrap();
        assert_eq!(frontier, vec!["INC0040257"]);
        let rows = store::list_discoveries(&conn, "grnoc-public-task-viewer", None).unwrap();
        let desc = rows.iter().find(|r| r.external_id == "INC0040257").unwrap();
        assert_eq!(desc.provenance, DISCOVERY_DESCRIPTION_REFERENCE);
        assert!(desc.source_snapshot_id.is_some());
    }

    #[test]
    fn duplicate_discoveries_merge_provenance() {
        let (_dir, conn) = open_temp_db();
        let a = record_analyst_seed(
            &conn,
            "grnoc-public-task-viewer",
            "INC0040257",
            "2026-08-01T00:00:00Z",
        )
        .unwrap();
        // Same path again: merged into the existing row.
        let b = record_analyst_seed(
            &conn,
            "grnoc-public-task-viewer",
            "INC0040257",
            "2026-08-01T01:00:00Z",
        )
        .unwrap();
        assert_eq!(a, b);
        let rows = store::list_discoveries(&conn, "grnoc-public-task-viewer", None).unwrap();
        assert_eq!(rows.len(), 1);
        // A different provenance path for the same ticket is retained
        // alongside the first; the frontier lists the ticket once.
        let doc = ReferenceDocument {
            id: 0,
            title: "AAR".to_string(),
            source_url: None,
            doc_type: "AfterActionReport".to_string(),
            redistribution_status: "Unknown".to_string(),
            publication_date: None,
            provenance: "test".to_string(),
            imported_utc: "2026-08-01T00:00:00Z".to_string(),
        };
        let doc_id = store::insert_reference_document(&conn, &doc).unwrap();
        record_document_references(
            &conn,
            "grnoc-public-task-viewer",
            doc_id,
            &["INC0040257".to_string()],
            "2026-08-01T02:00:00Z",
        )
        .unwrap();
        let rows = store::list_discoveries(&conn, "grnoc-public-task-viewer", None).unwrap();
        assert_eq!(rows.len(), 2);
        let frontier = store::pending_frontier(&conn, "grnoc-public-task-viewer").unwrap();
        assert_eq!(frontier, vec!["INC0040257"]);
    }

    #[test]
    fn no_default_numeric_enumeration_exists() {
        // The extractor yields only identifiers present in the text —
        // never numeric neighbors.
        let refs = extract_ticket_references(
            "The remaining sessions are tracked in INC0040257. See CHG0038258.",
        );
        assert_eq!(
            refs.iter()
                .map(|r| r.external_id.as_str())
                .collect::<Vec<_>>(),
            vec!["INC0040257", "CHG0038258"]
        );
        // Spans are exact byte offsets into the source.
        let text = "tracked in INC0040257.";
        let r = &extract_ticket_references(text)[0];
        assert_eq!(&text[r.start..r.end], "INC0040257");
        // No neighbors are fabricated for a lone identifier.
        let refs = extract_ticket_references("INC0040257");
        assert_eq!(refs.len(), 1);
        // Text with no identifiers yields nothing (no sequential guess).
        assert!(extract_ticket_references("no ticket numbers here").is_empty());
        // Even sequential-looking text yields exactly the identifiers
        // present — the extractor never fabricates the next number.
        let refs = extract_ticket_references("INC0000001 INC0000002");
        assert_eq!(refs.len(), 2);
        assert_eq!(
            refs.iter()
                .map(|r| r.external_id.as_str())
                .collect::<Vec<_>>(),
            vec!["INC0000001", "INC0000002"]
        );
    }

    #[test]
    fn request_budget_applies_to_reference_expansion() {
        let (_dir, _conn) = open_temp_db();
        // A description naming five tickets.
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(
            src.join("T1.json"),
            r#"{
                "number": "T1",
                "short_description": "x",
                "description": "See INC0040001, INC0040002, INC0040003, INC0040004, INC0040005",
                "start": "2019-08-21T04:00:00Z"
            }"#,
        )
        .unwrap();
        let path = dir.path().join("catalog.sqlite");
        let conn2 = db::open_catalog(&path).unwrap();
        let source = GrnocCatalogSource::new(src.clone(), "2026-08-01T00:00:00Z".into());
        sync_catalog(&conn2, &source, "2026-08-01T00:00:00Z").unwrap();
        expand_from_snapshots(&conn2, "grnoc-public-task-viewer", "2026-08-01T01:00:00Z").unwrap();
        let frontier = store::pending_frontier(&conn2, "grnoc-public-task-viewer").unwrap();
        assert_eq!(frontier.len(), 5);
        // A budget of 2 fetches only the first two this run…
        let slice = budgeted_frontier(&frontier, 2);
        assert_eq!(slice, &frontier[..2]);
        // …and the rest remain Pending for the next run.
        store::mark_frontier_fetched(&conn2, "grnoc-public-task-viewer", &slice[0]).unwrap();
        store::mark_frontier_fetched(&conn2, "grnoc-public-task-viewer", &slice[1]).unwrap();
        let remaining = store::pending_frontier(&conn2, "grnoc-public-task-viewer").unwrap();
        assert_eq!(remaining.len(), 3);
    }
}
