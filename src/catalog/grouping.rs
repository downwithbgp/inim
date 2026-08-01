//! Candidate incident grouping (Session 33, Part 9).
//!
//! A correlation workspace groups tickets that MAY describe parts of one
//! operational incident. Every candidate group states why it was
//! suggested and carries a categorical, explainable confidence:
//!
//! - `ExplicitlyLinked` — source text / reference-document assertions
//! - `StrongCandidate` — reviewed case-study membership, shared
//!   maintenance/change references
//! - `WeakCandidate` — temporal overlap, entity overlap alone
//! - `Rejected` — the analyst rejected the candidate
//!
//! There is no pseudo-scientific numerical confidence score. Groups are
//! suggestions only: `CatalogEvent`s are never merged or replaced.
//! A rejected candidate is not regenerated without NEW evidence (the
//! evidence fingerprint changes when the evidence set changes).

use crate::catalog::domain::*;
use crate::catalog::store;

/// Confidence categories (categorical and explainable).
pub mod confidence {
    pub const EXPLICITLY_LINKED: &str = "ExplicitlyLinked";
    pub const STRONG_CANDIDATE: &str = "StrongCandidate";
    pub const WEAK_CANDIDATE: &str = "WeakCandidate";
    pub const REJECTED: &str = "Rejected";
}

/// Deterministic evidence fingerprint: sha256 over sorted member ids and
/// sorted signal/detail pairs.
pub fn evidence_fingerprint(members: &[i64], evidence: &[GroupEvidence]) -> String {
    let mut member_ids: Vec<i64> = members.to_vec();
    member_ids.sort_unstable();
    let mut signals: Vec<String> = evidence
        .iter()
        .map(|e| format!("{}:{}", e.signal, e.detail))
        .collect();
    signals.sort();
    let payload = serde_json::json!({
        "members": member_ids,
        "signals": signals,
    })
    .to_string();
    crate::catalog::sync::hex_sha256(&payload)
}

/// Insert a candidate group; idempotent per evidence fingerprint.
/// Returns true when a new row was inserted.
pub fn insert_candidate(
    conn: &rusqlite::Connection,
    candidate: &IncidentGroupCandidate,
) -> Result<bool, String> {
    store::insert_group_candidate(conn, candidate)
}

/// Mark a candidate as rejected by an analyst. The rejected fingerprint
/// is retained so regeneration skips it.
pub fn reject_candidate(conn: &rusqlite::Connection, candidate_id: i64) -> Result<(), String> {
    conn.execute(
        "UPDATE incident_group_candidates
         SET confidence = ?1, review_status = ?2, updated_utc = ?3
         WHERE id = ?4",
        rusqlite::params![
            confidence::REJECTED,
            "Reviewed",
            chrono::Utc::now().to_rfc3339(),
            candidate_id
        ],
    )
    .map_err(|e| format!("catalog write failed: {e}"))?;
    Ok(())
}

/// Whether a candidate with this evidence fingerprint has been rejected
/// (regeneration must not re-create it without new evidence).
pub fn is_rejected_fingerprint(
    conn: &rusqlite::Connection,
    fingerprint: &str,
) -> Result<bool, String> {
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM incident_group_candidates
             WHERE evidence_fingerprint = ?1 AND confidence = ?2",
            rusqlite::params![fingerprint, confidence::REJECTED],
            |r| r.get(0),
        )
        .map_err(|e| format!("catalog read failed: {e}"))?;
    Ok(count > 0)
}

/// Candidate groups in deterministic order (newest first).
pub fn list_candidates(conn: &rusqlite::Connection) -> Result<Vec<IncidentGroupCandidate>, String> {
    store::list_group_candidates(conn)
}

// ── Generation ─────────────────────────────────────────────────────

fn upsert_group(
    conn: &rusqlite::Connection,
    label: &str,
    members: &[i64],
    evidence: Vec<GroupEvidence>,
    confidence: &str,
    created_utc: &str,
) -> Result<bool, String> {
    let fingerprint = evidence_fingerprint(members, &evidence);
    if is_rejected_fingerprint(conn, &fingerprint)? {
        return Ok(false); // rejected until new evidence changes the fingerprint
    }
    store::insert_group_candidate(
        conn,
        &IncidentGroupCandidate {
            id: 0,
            label: label.to_string(),
            member_event_ids: members.to_vec(),
            evidence,
            confidence: confidence.to_string(),
            review_status: "Unreviewed".to_string(),
            evidence_fingerprint: fingerprint,
            created_utc: created_utc.to_string(),
            updated_utc: created_utc.to_string(),
        },
    )
}

/// Generate candidate groups from the available signals:
///
/// 1. explicit ticket-text references → ExplicitlyLinked
/// 2. shared reviewed case-study membership → StrongCandidate
/// 3. derived temporal overlap → WeakCandidate
///
/// Returns the number of NEW candidate rows. Deterministic order.
pub fn generate_candidates(
    conn: &rusqlite::Connection,
    created_utc: &str,
) -> Result<usize, String> {
    let mut new_groups = 0usize;

    // 1. Explicit text edges (resolved pairs only).
    for edge in store::list_relationships(conn, None)? {
        if edge.evidence_kind != EVIDENCE_EXPLICIT_TICKET_TEXT {
            continue;
        }
        let Some(to_id) = edge.to_event_id else {
            continue;
        };
        let members = [edge.from_event_id, to_id];
        let detail = format!(
            "{} ({}): {} -> {}",
            edge.relationship_kind, edge.evidence_kind, edge.from_event_id, edge.to_external_id
        );
        if upsert_group(
            conn,
            &format!("Explicit link {} <-> {}", edge.from_event_id, to_id),
            &members,
            vec![GroupEvidence {
                signal: "ExplicitTicketText".to_string(),
                detail,
            }],
            confidence::EXPLICITLY_LINKED,
            created_utc,
        )? {
            new_groups += 1;
        }
    }

    // 2. Shared reviewed case-study membership.
    {
        let mut stmt = conn
            .prepare(
                "SELECT l.case_study_id, l.catalog_event_id, c.title
                 FROM case_study_event_links l
                 JOIN case_studies c ON c.id = l.case_study_id
                 WHERE l.catalog_event_id IS NOT NULL
                 ORDER BY l.case_study_id, l.sort_order",
            )
            .map_err(|e| format!("catalog read failed: {e}"))?;
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })
            .map_err(|e| format!("catalog read failed: {e}"))?;
        let mut groups: std::collections::BTreeMap<i64, (String, Vec<i64>)> =
            std::collections::BTreeMap::new();
        for row in rows {
            let (cs_id, event_id, title) = row.map_err(|e| format!("catalog read failed: {e}"))?;
            groups
                .entry(cs_id)
                .or_insert_with(|| (title.clone(), Vec::new()))
                .1
                .push(event_id);
        }
        for (cs_id, (title, members)) in groups {
            if members.len() < 2 {
                continue;
            }
            if upsert_group(
                conn,
                &format!("Case study {cs_id}: {title}"),
                &members,
                vec![GroupEvidence {
                    signal: "SharedCaseStudy".to_string(),
                    detail: format!("reviewed case study {cs_id} ({title})"),
                }],
                confidence::STRONG_CANDIDATE,
                created_utc,
            )? {
                new_groups += 1;
            }
        }
    }

    // 3. Derived temporal overlap (candidate only — never causal).
    for edge in store::list_relationships(conn, None)? {
        if edge.evidence_kind != EVIDENCE_DERIVED_TEMPORAL_OVERLAP {
            continue;
        }
        let Some(to_id) = edge.to_event_id else {
            continue;
        };
        let members = [edge.from_event_id, to_id];
        if upsert_group(
            conn,
            &format!("Temporal overlap {} <-> {}", edge.from_event_id, to_id),
            &members,
            vec![GroupEvidence {
                signal: "DerivedTemporalOverlap".to_string(),
                detail: edge.note.unwrap_or_default(),
            }],
            confidence::WEAK_CANDIDATE,
            created_utc,
        )? {
            new_groups += 1;
        }
    }

    Ok(new_groups)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::db;
    use crate::catalog::grnoc::GrnocCatalogSource;
    use crate::catalog::relationships::{
        derive_temporal_overlaps, extract_relationships_from_snapshots,
    };
    use crate::catalog::sync::sync_catalog;

    fn open_temp_db() -> (tempfile::TempDir, rusqlite::Connection) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("catalog.sqlite");
        let conn = db::open_catalog(&path).unwrap();
        (dir, conn)
    }

    fn write_record(
        dir: &std::path::Path,
        number: &str,
        description: &str,
        start: &str,
        end: &str,
    ) {
        std::fs::write(
            dir.join(format!("{number}.json")),
            serde_json::json!({
                "number": number,
                "short_description": "t",
                "description": description,
                "start": start,
                "end": end
            })
            .to_string(),
        )
        .unwrap();
    }

    fn sync_dir(conn: &rusqlite::Connection, dir: &std::path::Path) {
        let source = GrnocCatalogSource::new(dir.to_path_buf(), "2026-08-01T00:00:00Z".into());
        sync_catalog(conn, &source, "2026-08-01T00:00:00Z").unwrap();
    }

    fn event_id(conn: &rusqlite::Connection, external: &str) -> i64 {
        db::get_event_by_external(conn, "grnoc-public-task-viewer", external)
            .unwrap()
            .unwrap()
            .id
    }

    #[test]
    fn explicit_reference_creates_strong_group_candidate() {
        let (_dir, conn) = open_temp_db();
        let dir = tempfile::tempdir().unwrap();
        write_record(
            dir.path(),
            "CHG0099999",
            "Tracked in INC0040257.",
            "2019-08-21T04:00:00Z",
            "2019-08-21T05:00:00Z",
        );
        write_record(
            dir.path(),
            "INC0040257",
            "x",
            "2019-08-21T04:00:00Z",
            "2019-08-21T05:00:00Z",
        );
        sync_dir(&conn, dir.path());
        extract_relationships_from_snapshots(
            &conn,
            "grnoc-public-task-viewer",
            "2026-08-01T00:00:00Z",
        )
        .unwrap();
        crate::catalog::relationships::resolve_unresolved_edges(&conn, "grnoc-public-task-viewer")
            .unwrap();
        let n = generate_candidates(&conn, "2026-08-01T00:00:00Z").unwrap();
        assert!(n >= 1);
        let groups = list_candidates(&conn).unwrap();
        let explicit: Vec<_> = groups
            .iter()
            .filter(|g| g.confidence == confidence::EXPLICITLY_LINKED)
            .collect();
        assert!(!explicit.is_empty());
        let g = &explicit[0];
        assert_eq!(g.member_event_ids.len(), 2);
        assert!(g.evidence.iter().any(|e| e.signal == "ExplicitTicketText"));
    }

    #[test]
    fn shared_document_supports_group_candidate() {
        let (_dir, conn) = open_temp_db();
        let dir = tempfile::tempdir().unwrap();
        write_record(
            dir.path(),
            "INC0040101",
            "x",
            "2019-08-21T04:00:00Z",
            "2019-08-21T05:00:00Z",
        );
        write_record(
            dir.path(),
            "INC0040102",
            "y",
            "2019-08-21T06:00:00Z",
            "2019-08-21T07:00:00Z",
        );
        sync_dir(&conn, dir.path());
        // A reviewed case study links both tickets.
        let cs = CaseStudy {
            id: 0,
            slug: "shared-doc".to_string(),
            title: "Shared AAR".to_string(),
            summary: "s".to_string(),
            start_utc: Some("2019-08-21T04:00:00Z".to_string()),
            end_utc: Some("2019-08-21T22:38:00Z".to_string()),
            status: "Active".to_string(),
            content_sha256: "abc".to_string(),
            created_utc: "2026-08-01T00:00:00Z".to_string(),
            updated_utc: "2026-08-01T00:00:00Z".to_string(),
        };
        let cs_id = store::insert_case_study(&conn, &cs).unwrap();
        for (i, ext) in ["INC0040101", "INC0040102"].iter().enumerate() {
            store::insert_case_study_event_link(
                &conn,
                &CaseStudyEventLink {
                    id: 0,
                    case_study_id: cs_id,
                    catalog_event_id: Some(event_id(&conn, ext)),
                    external_identifier: ext.to_string(),
                    relationship: "Related".to_string(),
                    reviewed_note: None,
                    sort_order: i as i64,
                    source_document_id: None,
                },
            )
            .unwrap();
        }
        let n = generate_candidates(&conn, "2026-08-01T00:00:00Z").unwrap();
        assert!(n >= 1);
        let groups = list_candidates(&conn).unwrap();
        let strong: Vec<_> = groups
            .iter()
            .filter(|g| g.confidence == confidence::STRONG_CANDIDATE)
            .collect();
        assert!(!strong.is_empty());
        assert!(strong[0]
            .evidence
            .iter()
            .any(|e| e.signal == "SharedCaseStudy"));
    }

    #[test]
    fn temporal_overlap_alone_remains_weak() {
        let (_dir, conn) = open_temp_db();
        let dir = tempfile::tempdir().unwrap();
        write_record(
            dir.path(),
            "INC0040201",
            "x",
            "2019-08-21T04:00:00Z",
            "2019-08-21T05:00:00Z",
        );
        write_record(
            dir.path(),
            "INC0040202",
            "y",
            "2019-08-21T04:30:00Z",
            "2019-08-21T05:30:00Z",
        );
        sync_dir(&conn, dir.path());
        derive_temporal_overlaps(&conn, "grnoc-public-task-viewer", "2026-08-01T00:00:00Z")
            .unwrap();
        let n = generate_candidates(&conn, "2026-08-01T00:00:00Z").unwrap();
        assert!(n >= 1);
        let groups = list_candidates(&conn).unwrap();
        let weak: Vec<_> = groups
            .iter()
            .filter(|g| g.confidence == confidence::WEAK_CANDIDATE)
            .collect();
        assert!(!weak.is_empty());
        assert!(weak[0]
            .evidence
            .iter()
            .any(|e| e.signal == "DerivedTemporalOverlap"));
        // Overlap alone never produces a strong/explicit group.
        assert!(groups
            .iter()
            .all(|g| g.confidence != confidence::EXPLICITLY_LINKED));
    }

    #[test]
    fn analyst_can_reject_candidate() {
        let (_dir, conn) = open_temp_db();
        let dir = tempfile::tempdir().unwrap();
        write_record(
            dir.path(),
            "INC0040301",
            "x",
            "2019-08-21T04:00:00Z",
            "2019-08-21T05:00:00Z",
        );
        write_record(
            dir.path(),
            "INC0040302",
            "y",
            "2019-08-21T04:30:00Z",
            "2019-08-21T05:30:00Z",
        );
        sync_dir(&conn, dir.path());
        derive_temporal_overlaps(&conn, "grnoc-public-task-viewer", "2026-08-01T00:00:00Z")
            .unwrap();
        generate_candidates(&conn, "2026-08-01T00:00:00Z").unwrap();
        let groups = list_candidates(&conn).unwrap();
        let id = groups[0].id;
        reject_candidate(&conn, id).unwrap();
        let groups = list_candidates(&conn).unwrap();
        assert_eq!(groups[0].confidence, confidence::REJECTED);
        assert_eq!(groups[0].review_status, "Reviewed");
    }

    #[test]
    fn rejected_candidate_is_not_regenerated_without_new_evidence() {
        let (_dir, conn) = open_temp_db();
        let dir = tempfile::tempdir().unwrap();
        write_record(
            dir.path(),
            "INC0040401",
            "x",
            "2019-08-21T04:00:00Z",
            "2019-08-21T05:00:00Z",
        );
        write_record(
            dir.path(),
            "INC0040402",
            "y",
            "2019-08-21T04:30:00Z",
            "2019-08-21T05:30:00Z",
        );
        sync_dir(&conn, dir.path());
        derive_temporal_overlaps(&conn, "grnoc-public-task-viewer", "2026-08-01T00:00:00Z")
            .unwrap();
        generate_candidates(&conn, "2026-08-01T00:00:00Z").unwrap();
        let groups = list_candidates(&conn).unwrap();
        let fp = groups[0].evidence_fingerprint.clone();
        reject_candidate(&conn, groups[0].id).unwrap();
        // Regeneration with the same evidence: the rejected fingerprint
        // is skipped — nothing new is created.
        let n = generate_candidates(&conn, "2026-08-01T01:00:00Z").unwrap();
        assert_eq!(n, 0);
        let groups = list_candidates(&conn).unwrap();
        assert!(groups
            .iter()
            .all(|g| g.confidence == confidence::REJECTED || g.evidence_fingerprint != fp));
        // New evidence changes the fingerprint → a new candidate appears.
        let dir2 = tempfile::tempdir().unwrap();
        write_record(
            dir2.path(),
            "INC0040402",
            "See INC0040401 for details.",
            "2019-08-21T04:30:00Z",
            "2019-08-21T05:30:00Z",
        );
        sync_dir(&conn, dir2.path());
        extract_relationships_from_snapshots(
            &conn,
            "grnoc-public-task-viewer",
            "2026-08-01T02:00:00Z",
        )
        .unwrap();
        crate::catalog::relationships::resolve_unresolved_edges(&conn, "grnoc-public-task-viewer")
            .unwrap();
        let n = generate_candidates(&conn, "2026-08-01T02:00:00Z").unwrap();
        assert!(n >= 1, "new explicit evidence regenerates a candidate");
        let groups = list_candidates(&conn).unwrap();
        assert!(groups
            .iter()
            .any(|g| g.confidence == confidence::EXPLICITLY_LINKED));
    }

    #[test]
    fn group_does_not_replace_individual_events() {
        let (_dir, conn) = open_temp_db();
        let dir = tempfile::tempdir().unwrap();
        write_record(
            dir.path(),
            "INC0040501",
            "x",
            "2019-08-21T04:00:00Z",
            "2019-08-21T05:00:00Z",
        );
        write_record(
            dir.path(),
            "INC0040502",
            "y",
            "2019-08-21T04:30:00Z",
            "2019-08-21T05:30:00Z",
        );
        sync_dir(&conn, dir.path());
        derive_temporal_overlaps(&conn, "grnoc-public-task-viewer", "2026-08-01T00:00:00Z")
            .unwrap();
        generate_candidates(&conn, "2026-08-01T00:00:00Z").unwrap();
        // The catalog events remain individual; grouping only adds rows.
        let events = db::list_events(&conn).unwrap();
        assert_eq!(events.len(), 2);
        let groups = list_candidates(&conn).unwrap();
        assert!(!groups.is_empty());
        // Each event still has its own snapshots.
        for e in &events {
            assert_eq!(db::list_snapshots(&conn, e.id).unwrap().len(), 1);
        }
    }
}
