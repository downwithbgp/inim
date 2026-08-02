//! Candidate incident grouping.
//!
//! A correlation workspace groups tickets that MAY describe parts of one
//! operational incident. Every candidate group states why it was
//! suggested and carries a categorical, explainable confidence:
//!
//! - `ExplicitlyLinked` — source text / reference-document assertions
//! - `StrongCandidate` — reviewed case-study membership (multi-member)
//! - `WeakCandidate` — temporal overlap PLUS at least one supporting
//!   signal (shared reviewed entity/asset label, shared maintenance or
//!   change identifier, explicit reference)
//! - `TemporalCoincidence` — temporal overlap alone; queryable but
//!   hidden from the default analyst queue
//! - `Rejected` — the analyst rejected the candidate
//!
//! There is no pseudo-scientific numerical confidence score. Groups are
//! suggestions only: `CatalogEvent`s are never merged or replaced.
//! A rejected candidate is not regenerated without NEW evidence (the
//! evidence fingerprint changes when the evidence set changes).
//!
//! Generation is per unordered ticket pair: a pair gets ONE candidate
//! whose evidence is the union of every supporting signal and whose
//! category is the best applicable. Superseded Unreviewed rows (strict
//! subset of the same pair's signals) are removed — their evidence is
//! preserved inside the merged row.

use crate::catalog::domain::*;
use crate::catalog::store;

/// Confidence categories (categorical and explainable).
pub mod confidence {
    pub const EXPLICITLY_LINKED: &str = "ExplicitlyLinked";
    pub const STRONG_CANDIDATE: &str = "StrongCandidate";
    pub const WEAK_CANDIDATE: &str = "WeakCandidate";
    pub const TEMPORAL_COINCIDENCE: &str = "TemporalCoincidence";
    pub const REJECTED: &str = "Rejected";
}

/// Supporting-signal names (stable strings; also used by the web view).
pub mod signal {
    pub const EXPLICIT_TICKET_TEXT: &str = "ExplicitTicketText";
    pub const SHARED_CASE_STUDY: &str = "SharedCaseStudy";
    pub const DERIVED_TEMPORAL_OVERLAP: &str = "DerivedTemporalOverlap";
    pub const SHARED_REVIEWED_ENTITY: &str = "SharedReviewedEntity";
    pub const SHARED_MAINTENANCE_CHANGE: &str = "SharedMaintenanceChange";
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
    // Supersede Unreviewed rows for the same pair whose evidence is a
    // strict subset of the merged evidence (provenance is preserved in
    // the merged row; the pair appears exactly once).
    let signals: Vec<String> = evidence.iter().map(|e| e.signal.clone()).collect();
    let _ = store::supersede_pair_groups(conn, members, &signals)?;
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

/// Category for a pair given its supporting signals (best applicable).
fn classify_pair(
    has_explicit: bool,
    has_support: bool,
    has_temporal: bool,
) -> Option<&'static str> {
    if has_explicit {
        Some(confidence::EXPLICITLY_LINKED)
    } else if has_support {
        Some(confidence::WEAK_CANDIDATE)
    } else if has_temporal {
        Some(confidence::TEMPORAL_COINCIDENCE)
    } else {
        None
    }
}

/// Merge pairwise signals into one candidate per unordered pair.
///
/// Supporting signals (from edges and reviewed interpretations):
/// - explicit ticket-text reference → ExplicitlyLinked
/// - shared reviewed entity/asset label OR shared maintenance/change id
///   → WeakCandidate (with or without temporal overlap)
/// - temporal overlap alone → TemporalCoincidence
///
/// The shared reviewed case-study membership keeps its own multi-member
/// StrongCandidate group (see below) — it is not duplicated per pair.
///
/// Returns the number of NEW candidate rows. Deterministic order.
pub fn generate_candidates(
    conn: &rusqlite::Connection,
    created_utc: &str,
) -> Result<usize, String> {
    let mut new_groups = 0usize;

    // ── Multi-member group: shared reviewed case-study membership ────
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
                    signal: signal::SHARED_CASE_STUDY.to_string(),
                    detail: format!("reviewed case study {cs_id} ({title})"),
                }],
                confidence::STRONG_CANDIDATE,
                created_utc,
            )? {
                new_groups += 1;
            }
        }
    }

    // ── Per-pair candidates (explicit / entity / maintenance / temporal)
    // Collect every supporting signal per unordered pair.
    let mut pairs: std::collections::BTreeMap<(i64, i64), Vec<GroupEvidence>> =
        std::collections::BTreeMap::new();

    // Explicit ticket-text edges.
    for edge in store::list_relationships(conn, None)? {
        if edge.evidence_kind != EVIDENCE_EXPLICIT_TICKET_TEXT {
            continue;
        }
        let Some(to_id) = edge.to_event_id else {
            continue;
        };
        let (a, b) = sort_pair(edge.from_event_id, to_id);
        pairs.entry((a, b)).or_default().push(GroupEvidence {
            signal: signal::EXPLICIT_TICKET_TEXT.to_string(),
            detail: format!(
                "{} ({}): {} -> {}",
                edge.relationship_kind, edge.evidence_kind, edge.from_event_id, edge.to_external_id
            ),
        });
    }

    // Derived temporal-overlap edges.
    for edge in store::list_relationships(conn, None)? {
        if edge.evidence_kind != EVIDENCE_DERIVED_TEMPORAL_OVERLAP {
            continue;
        }
        let Some(to_id) = edge.to_event_id else {
            continue;
        };
        let (a, b) = sort_pair(edge.from_event_id, to_id);
        pairs.entry((a, b)).or_default().push(GroupEvidence {
            signal: signal::DERIVED_TEMPORAL_OVERLAP.to_string(),
            detail: edge.note.unwrap_or_default(),
        });
    }

    // Shared reviewed entity/asset labels and shared maintenance/change
    // identifiers (from reviewed interpretations).
    let reviews = store::list_ticket_reviews(conn)?;
    for i in 0..reviews.len() {
        for j in (i + 1)..reviews.len() {
            let a = &reviews[i];
            let b = &reviews[j];
            let (lo, hi) = sort_pair(a.catalog_event_id, b.catalog_event_id);
            let entry = pairs.entry((lo, hi)).or_default();
            let shared_entities: Vec<&String> = a
                .entity_labels
                .iter()
                .filter(|l| b.entity_labels.contains(l))
                .collect();
            if !shared_entities.is_empty() {
                entry.push(GroupEvidence {
                    signal: signal::SHARED_REVIEWED_ENTITY.to_string(),
                    detail: format!(
                        "shared reviewed entity/asset label(s): {}",
                        shared_entities
                            .iter()
                            .map(|s| s.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                });
            }
            let shared_changes: Vec<&String> = a
                .linked_change_ids
                .iter()
                .filter(|c| b.linked_change_ids.contains(c))
                .collect();
            if !shared_changes.is_empty() {
                entry.push(GroupEvidence {
                    signal: signal::SHARED_MAINTENANCE_CHANGE.to_string(),
                    detail: format!(
                        "shared reviewed maintenance/change identifier(s): {}",
                        shared_changes
                            .iter()
                            .map(|s| s.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                });
            }
        }
    }

    // Emit one candidate per pair: evidence = union of all signals,
    // category = best applicable.
    for ((a, b), mut evidence) in pairs {
        evidence.sort_by(|x, y| (&x.signal, &x.detail).cmp(&(&y.signal, &y.detail)));
        evidence.dedup_by(|x, y| x.signal == y.signal && x.detail == y.detail);
        let has_explicit = evidence
            .iter()
            .any(|e| e.signal == signal::EXPLICIT_TICKET_TEXT);
        let has_support = evidence.iter().any(|e| {
            e.signal == signal::SHARED_REVIEWED_ENTITY
                || e.signal == signal::SHARED_MAINTENANCE_CHANGE
        });
        let has_temporal = evidence
            .iter()
            .any(|e| e.signal == signal::DERIVED_TEMPORAL_OVERLAP);
        let Some(category) = classify_pair(has_explicit, has_support, has_temporal) else {
            continue;
        };
        let label = match category {
            c if c == confidence::EXPLICITLY_LINKED => format!("Explicit link {a} <-> {b}"),
            c if c == confidence::WEAK_CANDIDATE => format!("Weak candidate {a} <-> {b}"),
            _ => format!("Temporal coincidence {a} <-> {b}"),
        };
        let members = [a, b];
        if upsert_group(conn, &label, &members, evidence, category, created_utc)? {
            new_groups += 1;
        }
    }

    Ok(new_groups)
}

fn sort_pair(a: i64, b: i64) -> (i64, i64) {
    if a < b {
        (a, b)
    } else {
        (b, a)
    }
}

/// Candidates visible in the default analyst queue: everything except
/// temporal-only coincidence. Coincidence remains queryable.
pub fn default_queue_candidates(groups: &[IncidentGroupCandidate]) -> Vec<&IncidentGroupCandidate> {
    groups
        .iter()
        .filter(|g| g.confidence != confidence::TEMPORAL_COINCIDENCE)
        .collect()
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
    fn explicit_reference_is_prominent() {
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
    fn temporal_overlap_only_is_temporal_coincidence() {
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
        let coincidence: Vec<_> = groups
            .iter()
            .filter(|g| g.confidence == confidence::TEMPORAL_COINCIDENCE)
            .collect();
        assert!(
            !coincidence.is_empty(),
            "overlap alone must be TemporalCoincidence"
        );
        assert!(coincidence[0]
            .evidence
            .iter()
            .any(|e| e.signal == signal::DERIVED_TEMPORAL_OVERLAP));
        // Overlap alone never produces a weak/strong/explicit group.
        assert!(groups
            .iter()
            .all(|g| g.confidence != confidence::WEAK_CANDIDATE
                && g.confidence != confidence::STRONG_CANDIDATE
                && g.confidence != confidence::EXPLICITLY_LINKED));
    }

    #[test]
    fn temporal_coincidence_is_hidden_by_default() {
        let (_dir, conn) = open_temp_db();
        let dir = tempfile::tempdir().unwrap();
        write_record(
            dir.path(),
            "INC0040211",
            "x",
            "2019-08-21T04:00:00Z",
            "2019-08-21T05:00:00Z",
        );
        write_record(
            dir.path(),
            "INC0040212",
            "y",
            "2019-08-21T04:30:00Z",
            "2019-08-21T05:30:00Z",
        );
        sync_dir(&conn, dir.path());
        derive_temporal_overlaps(&conn, "grnoc-public-task-viewer", "2026-08-01T00:00:00Z")
            .unwrap();
        generate_candidates(&conn, "2026-08-01T00:00:00Z").unwrap();
        let groups = list_candidates(&conn).unwrap();
        assert!(!groups.is_empty());
        // The default analyst queue hides temporal-only coincidence.
        let default_view = default_queue_candidates(&groups);
        assert!(default_view
            .iter()
            .all(|g| g.confidence != confidence::TEMPORAL_COINCIDENCE));
        assert!(default_view.is_empty());
        // The coincidence remains queryable with its provenance intact.
        assert!(groups
            .iter()
            .any(|g| g.confidence == confidence::TEMPORAL_COINCIDENCE
                && g.evidence
                    .iter()
                    .any(|e| e.signal == signal::DERIVED_TEMPORAL_OVERLAP)));
    }

    #[test]
    fn shared_asset_plus_overlap_can_be_weak_candidate() {
        let (_dir, conn) = open_temp_db();
        let dir = tempfile::tempdir().unwrap();
        write_record(
            dir.path(),
            "INC0040221",
            "x",
            "2019-08-21T04:00:00Z",
            "2019-08-21T05:00:00Z",
        );
        write_record(
            dir.path(),
            "INC0040222",
            "y",
            "2019-08-21T04:30:00Z",
            "2019-08-21T05:30:00Z",
        );
        sync_dir(&conn, dir.path());
        derive_temporal_overlaps(&conn, "grnoc-public-task-viewer", "2026-08-01T00:00:00Z")
            .unwrap();
        // Reviewed interpretations share an entity/asset label.
        let mut r1 = crate::catalog::review::tests::sample_review("INC0040221");
        r1.catalog_event_id = event_id(&conn, "INC0040221");
        r1.entity_labels = vec!["sw.net.manlan (core node)".to_string()];
        crate::catalog::store::upsert_ticket_review(&conn, &r1).unwrap();
        let mut r2 = crate::catalog::review::tests::sample_review("INC0040222");
        r2.catalog_event_id = event_id(&conn, "INC0040222");
        r2.entity_labels = vec!["sw.net.manlan (core node)".to_string()];
        crate::catalog::store::upsert_ticket_review(&conn, &r2).unwrap();
        let n = generate_candidates(&conn, "2026-08-01T00:00:00Z").unwrap();
        assert!(n >= 1);
        let groups = list_candidates(&conn).unwrap();
        let weak: Vec<_> = groups
            .iter()
            .filter(|g| g.confidence == confidence::WEAK_CANDIDATE)
            .collect();
        assert!(
            !weak.is_empty(),
            "shared asset + overlap must be a WeakCandidate"
        );
        assert!(weak[0]
            .evidence
            .iter()
            .any(|e| e.signal == signal::SHARED_REVIEWED_ENTITY));
        assert!(weak[0]
            .evidence
            .iter()
            .any(|e| e.signal == signal::DERIVED_TEMPORAL_OVERLAP));
        // It appears in the default queue.
        let default_view = default_queue_candidates(&groups);
        assert!(default_view
            .iter()
            .any(|g| g.confidence == confidence::WEAK_CANDIDATE));
    }

    #[test]
    fn candidate_explanation_lists_every_supporting_signal() {
        let (_dir, conn) = open_temp_db();
        let dir = tempfile::tempdir().unwrap();
        write_record(
            dir.path(),
            "CHG0099301",
            "Tracked in INC0040231.",
            "2019-08-21T04:00:00Z",
            "2019-08-21T05:00:00Z",
        );
        write_record(
            dir.path(),
            "INC0040231",
            "x",
            "2019-08-21T04:30:00Z",
            "2019-08-21T05:30:00Z",
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
        derive_temporal_overlaps(&conn, "grnoc-public-task-viewer", "2026-08-01T00:00:00Z")
            .unwrap();
        // Shared reviewed entity label as a third supporting signal.
        let mut r1 = crate::catalog::review::tests::sample_review("CHG0099301");
        r1.catalog_event_id = event_id(&conn, "CHG0099301");
        r1.entity_labels = vec!["SampleParticipant".to_string()];
        crate::catalog::store::upsert_ticket_review(&conn, &r1).unwrap();
        let mut r2 = crate::catalog::review::tests::sample_review("INC0040231");
        r2.catalog_event_id = event_id(&conn, "INC0040231");
        r2.entity_labels = vec!["SampleParticipant".to_string()];
        crate::catalog::store::upsert_ticket_review(&conn, &r2).unwrap();
        let n = generate_candidates(&conn, "2026-08-01T00:00:00Z").unwrap();
        assert!(n >= 1);
        let groups = list_candidates(&conn).unwrap();
        // The pair appears ONCE, as ExplicitlyLinked, listing every
        // supporting signal in its explanation.
        let pair_groups: Vec<_> = groups
            .iter()
            .filter(|g| g.confidence == confidence::EXPLICITLY_LINKED)
            .collect();
        assert_eq!(pair_groups.len(), 1);
        let signals: Vec<&str> = pair_groups[0]
            .evidence
            .iter()
            .map(|e| e.signal.as_str())
            .collect();
        assert!(signals.contains(&signal::EXPLICIT_TICKET_TEXT));
        assert!(signals.contains(&signal::DERIVED_TEMPORAL_OVERLAP));
        assert!(signals.contains(&signal::SHARED_REVIEWED_ENTITY));
        // No duplicate pair rows linger (superseded rows are removed).
        let pair_members = &pair_groups[0].member_event_ids;
        let same_pair = groups
            .iter()
            .filter(|g| g.member_event_ids == *pair_members)
            .count();
        assert_eq!(same_pair, 1);
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
    fn rejected_candidate_remains_suppressed_without_new_evidence() {
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
