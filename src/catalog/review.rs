//! Reviewed ticket interpretation (Session 34, Parts 1–2).
//!
//! A reviewed interpretation is analyst-reviewed case-study context for a
//! catalog ticket, stored SEPARATELY from its immutable source snapshot.
//! The review file (`ticket-reviews.json`) carries:
//!
//! - per-ticket reviews: reviewed case-study roles, entity/asset labels,
//!   linked maintenance/change identifiers, analysis applicability,
//!   relationship to the case study, and per-field provenance;
//! - reviewed relationship edges: specific kinds (RollbackFor,
//!   ParticipantImpactDuring, AlarmDuring, OperationalTaskDuring,
//!   RelatedIncident, RelatedChange, TracksRemainingImpactIn, References)
//!   with evidence kinds and exact supporting sources.
//!
//! Rules enforced here:
//!
//! - source wording is never modified — the review table is a separate
//!   layer; `event_snapshots` rows are untouched by import;
//! - a missing source field is never backfilled without a cited document
//!   (AAR) provenance entry;
//! - `ReferenceDocument` provenance requires a resolved document id;
//! - reviewed roles are vocabulary-checked and never replace the source
//!   task type;
//! - reviewed edges are idempotent and never overwrite existing rows;
//! - relationships targeting unavailable tickets (e.g. TASK records with
//!   no snapshot) remain unresolved references — no snapshot is
//!   manufactured.

use rusqlite::Connection;

use super::db;
use super::domain::*;
use super::store;

/// File-format entry for one reviewed ticket (lean JSON; the DB struct
/// keeps its own fields).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ReviewFileEntry {
    pub external_id: String,
    #[serde(default)]
    pub roles: Vec<String>,
    #[serde(default)]
    pub entity_labels: Vec<String>,
    #[serde(default)]
    pub linked_change_ids: Vec<String>,
    #[serde(default)]
    pub analysis_applicability: String,
    #[serde(default)]
    pub applicability_rationale: String,
    #[serde(default)]
    pub relationship_to_case_study: String,
    #[serde(default)]
    pub provenance: Vec<ReviewProvenance>,
}

impl ReviewFileEntry {
    pub fn into_review(self, reviewer: &str, reviewed_at: &str) -> TicketReview {
        TicketReview {
            id: 0,
            catalog_event_id: 0,
            external_id: self.external_id,
            reviewed_roles: self.roles,
            entity_labels: self.entity_labels,
            linked_change_ids: self.linked_change_ids,
            analysis_applicability: self.analysis_applicability,
            applicability_rationale: self.applicability_rationale,
            relationship_to_case_study: self.relationship_to_case_study,
            review_status: "Reviewed".to_string(),
            reviewer: reviewer.to_string(),
            reviewed_at: reviewed_at.to_string(),
            provenance: self.provenance,
            source_document_id: None,
        }
    }
}

/// Reviewed-role vocabulary (fixed).
pub fn valid_role(role: &str) -> bool {
    reviewed_role::ALL.contains(&role)
}

/// Applicability vocabulary (fixed).
pub fn valid_applicability(app: &str) -> bool {
    matches!(
        app,
        applicability::POTENTIALLY_VISIBLE
            | applicability::NOT_APPLICABLE
            | applicability::TARGET_NOT_YET_MAPPED
    )
}

/// Evidence kinds accepted on reviewed relationship edges.
pub fn valid_reviewed_evidence(kind: &str) -> bool {
    matches!(
        kind,
        EVIDENCE_EXPLICIT_TICKET_TEXT | EVIDENCE_REFERENCE_DOCUMENT | EVIDENCE_ANALYST_REVIEWED
    )
}

/// Find the reference document (e.g. the AAR) linked to a case study.
pub fn case_study_document(
    conn: &Connection,
    case_study_id: i64,
    doc_type: &str,
) -> Result<Option<i64>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT d.id FROM reference_documents d
             JOIN case_study_document_links l ON l.document_id = d.id
             WHERE l.case_study_id = ?1 AND d.doc_type = ?2
             ORDER BY d.id LIMIT 1",
        )
        .map_err(|e| format!("catalog read failed: {e}"))?;
    let mut rows = stmt
        .query_map(rusqlite::params![case_study_id, doc_type], |r| {
            r.get::<_, i64>(0)
        })
        .map_err(|e| format!("catalog read failed: {e}"))?;
    match rows.next() {
        None => Ok(None),
        Some(r) => Ok(Some(r.map_err(|e| format!("catalog read failed: {e}"))?)),
    }
}

/// Validate one review against the catalog and the vocabulary.
///
/// Returns the review with a resolved `source_document_id` (the cited AAR
/// when document provenance is used). Every non-empty interpretation field
/// must be covered by at least one provenance entry; `ReferenceDocument`
/// provenance requires `source_document_id`.
pub fn validate_review(
    conn: &Connection,
    source_kind: &str,
    mut review: TicketReview,
    aar_document_id: Option<i64>,
) -> Result<TicketReview, String> {
    let event =
        db::get_event_by_external(conn, source_kind, &review.external_id)?.ok_or_else(|| {
            format!(
                "review target {} is not a catalog event",
                review.external_id
            )
        })?;
    review.catalog_event_id = event.id;

    if review.reviewed_roles.is_empty() {
        return Err(format!(
            "{}: at least one reviewed role is required",
            review.external_id
        ));
    }
    for role in &review.reviewed_roles {
        if !valid_role(role) {
            return Err(format!(
                "{}: unknown reviewed role '{role}' (vocabulary: {:?})",
                review.external_id,
                reviewed_role::ALL
            ));
        }
    }
    if !valid_applicability(&review.analysis_applicability) {
        return Err(format!(
            "{}: unknown applicability '{}'",
            review.external_id, review.analysis_applicability
        ));
    }

    // Every non-empty field must be covered by provenance.
    let covered: Vec<&str> = review.provenance.iter().map(|p| p.field.as_str()).collect();
    for (field, present) in [
        ("roles", !review.reviewed_roles.is_empty()),
        ("entity_labels", !review.entity_labels.is_empty()),
        ("linked_change_ids", !review.linked_change_ids.is_empty()),
        ("applicability", !review.analysis_applicability.is_empty()),
        (
            "relationship_to_case_study",
            !review.relationship_to_case_study.is_empty(),
        ),
    ] {
        if present && !covered.contains(&field) {
            return Err(format!(
                "{}: field '{field}' has no provenance entry",
                review.external_id
            ));
        }
    }

    // Document provenance must resolve; snapshot provenance is free.
    // A `ReferenceDocument` entry without its own id falls back to the
    // case study's cited AAR; with neither, the review is rejected.
    for p in review.provenance.iter_mut() {
        if p.source.starts_with("ReferenceDocument") {
            if p.source_document_id.is_none() {
                p.source_document_id = aar_document_id;
            }
            if p.source_document_id.is_none() {
                return Err(format!(
                    "{}: ReferenceDocument provenance for field '{}' requires a cited document",
                    review.external_id, p.field
                ));
            }
        }
    }
    if review
        .provenance
        .iter()
        .any(|p| p.source_document_id.is_some())
    {
        review.source_document_id = Some(
            review
                .provenance
                .iter()
                .find_map(|p| p.source_document_id)
                .or(aar_document_id)
                .ok_or_else(|| {
                    format!(
                        "{}: document provenance cited but no document id resolved",
                        review.external_id
                    )
                })?,
        );
    }

    if review.review_status.is_empty() {
        review.review_status = "Reviewed".to_string();
    }
    Ok(review)
}

/// One reviewed relationship edge as authored in the review file.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ReviewedEdgeInput {
    pub from: String,
    pub to: String,
    pub kind: String,
    pub evidence: String,
    #[serde(default)]
    pub note: Option<String>,
    /// Whether the edge is supported by the cited reference document.
    #[serde(default)]
    pub document_cited: bool,
}

/// Import a reviewed relationship edge. Idempotent (INSERT OR IGNORE on
/// the dedup key); existing rows — including reviewed edges — are never
/// overwritten. Returns true when a new row was inserted.
pub fn import_reviewed_edge(
    conn: &Connection,
    source_kind: &str,
    edge: &ReviewedEdgeInput,
    aar_document_id: Option<i64>,
    created_utc: &str,
) -> Result<bool, String> {
    let from_external = &edge.from;
    let to_external = &edge.to;
    let kind = &edge.kind;
    let evidence = &edge.evidence;
    let document_cited = edge.document_cited;
    if !valid_reviewed_evidence(evidence) {
        return Err(format!(
            "{from_external} -> {to_external}: invalid evidence kind '{evidence}'"
        ));
    }
    let from = db::get_event_by_external(conn, source_kind, from_external)?
        .ok_or_else(|| format!("relationship source {from_external} is not a catalog event"))?;
    // The target may be unresolved (no snapshot) — that is a first-class
    // document reference, not an error.
    let to = db::get_event_by_external(conn, source_kind, to_external)?;
    if evidence == EVIDENCE_REFERENCE_DOCUMENT && !document_cited {
        return Err(format!(
            "{from_external} -> {to_external}: ReferenceDocument evidence requires document_cited"
        ));
    }
    let source_document_id = if document_cited {
        aar_document_id
    } else {
        None
    };
    let source_snapshot_id = db::list_snapshots(conn, from.id)?.first().map(|s| s.id);
    store::insert_relationship(
        conn,
        &TicketRelationship {
            id: 0,
            from_event_id: from.id,
            to_event_id: to.as_ref().map(|e| e.id),
            to_external_id: to_external.to_string(),
            relationship_kind: kind.to_string(),
            evidence_kind: evidence.to_string(),
            source_snapshot_id,
            source_document_id,
            reviewed_status: REVIEW_ACCEPTED.to_string(),
            note: edge.note.clone(),
            created_utc: created_utc.to_string(),
        },
    )
}

/// Derive entity-overlap candidate edges from reviewed entity labels:
/// pairs of tickets whose reviewed interpretations share an entity label
/// get a `DerivedEntityOverlap` edge. Derived — never causal; reviewed
/// edges are never overwritten.
pub fn derive_entity_overlaps(conn: &Connection, created_utc: &str) -> Result<usize, String> {
    let reviews = store::list_ticket_reviews(conn)?;
    let mut new_edges = 0usize;
    for i in 0..reviews.len() {
        for j in (i + 1)..reviews.len() {
            let a = &reviews[i];
            let b = &reviews[j];
            let shared: Vec<&String> = a
                .entity_labels
                .iter()
                .filter(|l| b.entity_labels.contains(l))
                .collect();
            if shared.is_empty() {
                continue;
            }
            let note = format!(
                "shared reviewed entity label(s): {}",
                shared
                    .iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            let inserted = store::insert_relationship(
                conn,
                &TicketRelationship {
                    id: 0,
                    from_event_id: a.catalog_event_id,
                    to_event_id: Some(b.catalog_event_id),
                    to_external_id: b.external_id.clone(),
                    relationship_kind: RELATIONSHIP_ENTITY_OVERLAP.to_string(),
                    evidence_kind: EVIDENCE_DERIVED_ENTITY_OVERLAP.to_string(),
                    source_snapshot_id: None,
                    source_document_id: None,
                    reviewed_status: REVIEW_UNREVIEWED.to_string(),
                    note: Some(note),
                    created_utc: created_utc.to_string(),
                },
            )?;
            if inserted {
                new_edges += 1;
            }
        }
    }
    Ok(new_edges)
}

// ── Audit listing ──────────────────────────────────────────────────

/// One graph-audit row: source node, destination or unresolved reference,
/// relationship, evidence kind, exact source, review status.
#[derive(Debug, Clone, serde::Serialize)]
pub struct GraphAuditRow {
    pub from_external: String,
    pub to_external: String,
    pub to_resolved: bool,
    pub relationship_kind: String,
    pub evidence_kind: String,
    pub exact_source: String,
    pub review_status: String,
}

/// Build the full relationship-graph audit over the corpus source kind.
/// Unresolved targets are listed as unresolved references (never
/// manufactured).
pub fn graph_audit(conn: &Connection, source_kind: &str) -> Result<Vec<GraphAuditRow>, String> {
    let edges = store::list_relationships(conn, None)?;
    let mut out = Vec::new();
    for edge in edges {
        let from = db::get_event(conn, edge.from_event_id)?;
        let from_kind = from.as_ref().map(|e| e.source_kind.clone());
        let from_ext = from
            .map(|e| e.external_id)
            .unwrap_or_else(|| format!("#{}", edge.from_event_id));
        if from_kind.as_deref() != Some(source_kind) {
            continue;
        }
        let exact_source = match (edge.source_snapshot_id, edge.source_document_id) {
            (Some(sid), Some(did)) => {
                format!("snapshot #{sid} (ticket text) + reference document #{did}")
            }
            (Some(sid), None) => {
                format!("snapshot #{sid} (ticket text)")
            }
            (None, Some(did)) => {
                format!("reference document #{did}")
            }
            (None, None) => match edge.evidence_kind.as_str() {
                EVIDENCE_DERIVED_TEMPORAL_OVERLAP => "derived: temporal overlap".to_string(),
                EVIDENCE_DERIVED_ENTITY_OVERLAP => "derived: shared reviewed entity".to_string(),
                EVIDENCE_SHARED_CASE_STUDY => "derived: shared case study".to_string(),
                _ => edge.note.clone().unwrap_or_default(),
            },
        };
        out.push(GraphAuditRow {
            from_external: from_ext,
            to_external: edge.to_external_id.clone(),
            to_resolved: edge.to_event_id.is_some(),
            relationship_kind: edge.relationship_kind.clone(),
            evidence_kind: edge.evidence_kind.clone(),
            exact_source,
            review_status: edge.reviewed_status.clone(),
        });
    }
    out.sort_by(|a, b| {
        (
            &a.from_external,
            &a.to_external,
            &a.relationship_kind,
            &a.evidence_kind,
        )
            .cmp(&(
                &b.from_external,
                &b.to_external,
                &b.relationship_kind,
                &b.evidence_kind,
            ))
    });
    Ok(out)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::catalog::db;
    use crate::catalog::grnoc::GrnocCatalogSource;
    use crate::catalog::relationships::extract_relationships_from_snapshots;
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
        task_type: &str,
        description: &str,
        start: &str,
        end: &str,
    ) {
        std::fs::write(
            dir.join(format!("{number}.json")),
            serde_json::json!({
                "number": number,
                "task_type": task_type,
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

    /// A ready-to-store reviewed interpretation for tests (roles +
    /// entity labels + applicability, snapshot-cited provenance).
    pub(crate) fn sample_review(external: &str) -> TicketReview {
        TicketReview {
            id: 0,
            catalog_event_id: 0,
            external_id: external.to_string(),
            reviewed_roles: vec![reviewed_role::PARTICIPANT_IMPACT.to_string()],
            entity_labels: Vec::new(),
            linked_change_ids: Vec::new(),
            analysis_applicability: applicability::POTENTIALLY_VISIBLE.to_string(),
            applicability_rationale: "test".to_string(),
            relationship_to_case_study: "Related".to_string(),
            review_status: "Reviewed".to_string(),
            reviewer: "test-analyst".to_string(),
            reviewed_at: "2026-08-01T00:00:00Z".to_string(),
            provenance: vec![
                ReviewProvenance {
                    field: "roles".to_string(),
                    source: "SnapshotField:title".to_string(),
                    detail: "title".to_string(),
                    source_document_id: None,
                },
                ReviewProvenance {
                    field: "applicability".to_string(),
                    source: "Analyst".to_string(),
                    detail: "test".to_string(),
                    source_document_id: None,
                },
                ReviewProvenance {
                    field: "relationship_to_case_study".to_string(),
                    source: "Analyst".to_string(),
                    detail: "test".to_string(),
                    source_document_id: None,
                },
            ],
            source_document_id: None,
        }
    }

    /// Insert a reference document (e.g. the AAR) and return its id.
    fn insert_test_document(conn: &rusqlite::Connection) -> i64 {
        store::insert_reference_document(
            conn,
            &ReferenceDocument {
                id: 0,
                title: "Test AAR".to_string(),
                source_url: Some("https://example.invalid/aar".to_string()),
                doc_type: "AfterActionReport".to_string(),
                redistribution_status: "ReviewOnly".to_string(),
                publication_date: Some("2019-08-22".to_string()),
                provenance: "test fixture".to_string(),
                imported_utc: "2026-08-01T00:00:00Z".to_string(),
            },
        )
        .unwrap()
    }

    fn base_review(external: &str) -> TicketReview {
        TicketReview {
            id: 0,
            catalog_event_id: 0,
            external_id: external.to_string(),
            reviewed_roles: vec![reviewed_role::PARTICIPANT_IMPACT.to_string()],
            entity_labels: vec!["SampleParticipant".to_string()],
            linked_change_ids: Vec::new(),
            analysis_applicability: applicability::POTENTIALLY_VISIBLE.to_string(),
            applicability_rationale: "r".to_string(),
            relationship_to_case_study: "ParticipantIncident".to_string(),
            review_status: "Reviewed".to_string(),
            reviewer: "test-analyst".to_string(),
            reviewed_at: "2026-08-01T00:00:00Z".to_string(),
            provenance: vec![
                ReviewProvenance {
                    field: "roles".to_string(),
                    source: "SnapshotField:title".to_string(),
                    detail: "title text".to_string(),
                    source_document_id: None,
                },
                ReviewProvenance {
                    field: "entity_labels".to_string(),
                    source: "SnapshotField:title".to_string(),
                    detail: "participant name".to_string(),
                    source_document_id: None,
                },
                ReviewProvenance {
                    field: "applicability".to_string(),
                    source: "Analyst".to_string(),
                    detail: "public participant".to_string(),
                    source_document_id: None,
                },
                ReviewProvenance {
                    field: "relationship_to_case_study".to_string(),
                    source: "Analyst".to_string(),
                    detail: "case-study review".to_string(),
                    source_document_id: None,
                },
            ],
            source_document_id: None,
        }
    }

    #[test]
    fn source_task_type_and_reviewed_role_are_distinct() {
        let (_dir, conn) = open_temp_db();
        let dir = tempfile::tempdir().unwrap();
        write_record(
            dir.path(),
            "INC0040101",
            "Incident",
            "x",
            "2019-08-21T04:00:00Z",
            "2019-08-21T05:00:00Z",
        );
        sync_dir(&conn, dir.path());
        let mut review = base_review("INC0040101");
        review.reviewed_roles = vec![reviewed_role::PRIMARY_INCIDENT.to_string()];
        let review = validate_review(&conn, "grnoc-public-task-viewer", review, None).unwrap();
        store::upsert_ticket_review(&conn, &review).unwrap();
        // Source task type comes from the snapshot and stays "Incident";
        // the reviewed role is the separate interpretation.
        let snapshots = db::list_snapshots(&conn, event_id(&conn, "INC0040101")).unwrap();
        let normalized: serde_json::Value =
            serde_json::from_str(&snapshots[0].normalized_json).unwrap();
        assert_eq!(normalized["task_type"], "Incident");
        let stored = store::get_ticket_review(&conn, event_id(&conn, "INC0040101"))
            .unwrap()
            .unwrap();
        assert_eq!(stored.reviewed_roles, vec!["PrimaryIncident"]);
        assert_ne!(stored.reviewed_roles[0], normalized["task_type"]);
    }

    #[test]
    fn aar_enrichment_requires_aar_provenance() {
        let (_dir, conn) = open_temp_db();
        let dir = tempfile::tempdir().unwrap();
        // Source ticket has NO window fields.
        write_record(dir.path(), "INC0040102", "Incident", "Customer SOI", "", "");
        sync_dir(&conn, dir.path());
        // A review that cites a document but does not resolve it is rejected.
        let mut review = base_review("INC0040102");
        review.provenance.push(ReviewProvenance {
            field: "window".to_string(),
            source: "ReferenceDocument:AAR Appendix A".to_string(),
            detail: "15:33 GMT Incident Resolved".to_string(),
            source_document_id: None,
        });
        let err = validate_review(&conn, "grnoc-public-task-viewer", review, None).unwrap_err();
        assert!(
            err.contains("requires a cited document"),
            "unexpected error: {err}"
        );
        // With a resolved document id the AAR enrichment is accepted.
        let doc_id = insert_test_document(&conn);
        let mut review = base_review("INC0040102");
        review.provenance.push(ReviewProvenance {
            field: "window".to_string(),
            source: "ReferenceDocument:AAR Appendix A".to_string(),
            detail: "15:33 GMT Incident Resolved".to_string(),
            source_document_id: Some(doc_id),
        });
        let review =
            validate_review(&conn, "grnoc-public-task-viewer", review, Some(doc_id)).unwrap();
        assert_eq!(review.source_document_id, Some(doc_id));
    }

    #[test]
    fn reviewed_interpretation_does_not_modify_source_snapshot() {
        let (_dir, conn) = open_temp_db();
        let dir = tempfile::tempdir().unwrap();
        write_record(
            dir.path(),
            "INC0040103",
            "Incident",
            "x",
            "2019-08-21T04:00:00Z",
            "2019-08-21T05:00:00Z",
        );
        sync_dir(&conn, dir.path());
        let before = db::list_snapshots(&conn, event_id(&conn, "INC0040103")).unwrap();
        let before_raw: Vec<String> = before.iter().map(|s| s.raw_payload.clone()).collect();
        let before_norm: Vec<String> = before.iter().map(|s| s.normalized_json.clone()).collect();
        let review = validate_review(
            &conn,
            "grnoc-public-task-viewer",
            base_review("INC0040103"),
            None,
        )
        .unwrap();
        store::upsert_ticket_review(&conn, &review).unwrap();
        // Re-import with a different interpretation — still no snapshot change.
        let mut review2 = base_review("INC0040103");
        review2.reviewed_roles = vec![reviewed_role::CHANGE_WINDOW.to_string()];
        let review2 = validate_review(&conn, "grnoc-public-task-viewer", review2, None).unwrap();
        store::upsert_ticket_review(&conn, &review2).unwrap();
        let after = db::list_snapshots(&conn, event_id(&conn, "INC0040103")).unwrap();
        let after_raw: Vec<String> = after.iter().map(|s| s.raw_payload.clone()).collect();
        let after_norm: Vec<String> = after.iter().map(|s| s.normalized_json.clone()).collect();
        assert_eq!(before_raw, after_raw);
        assert_eq!(before_norm, after_norm);
        let stored = store::get_ticket_review(&conn, event_id(&conn, "INC0040103"))
            .unwrap()
            .unwrap();
        assert_eq!(stored.reviewed_roles, vec!["ChangeWindow"]);
    }

    #[test]
    fn one_ticket_can_have_multiple_supported_case_study_roles() {
        let (_dir, conn) = open_temp_db();
        let dir = tempfile::tempdir().unwrap();
        write_record(
            dir.path(),
            "CHG0099901",
            "Change Request",
            "x",
            "2019-08-21T04:00:00Z",
            "2019-08-21T05:00:00Z",
        );
        sync_dir(&conn, dir.path());
        let mut review = base_review("CHG0099901");
        review.reviewed_roles = vec![
            reviewed_role::ROLLBACK_OR_RECOVERY.to_string(),
            reviewed_role::CHANGE_WINDOW.to_string(),
        ];
        let review = validate_review(&conn, "grnoc-public-task-viewer", review, None).unwrap();
        store::upsert_ticket_review(&conn, &review).unwrap();
        let stored = store::get_ticket_review(&conn, event_id(&conn, "CHG0099901"))
            .unwrap()
            .unwrap();
        assert_eq!(stored.reviewed_roles.len(), 2);
        assert!(stored
            .reviewed_roles
            .contains(&reviewed_role::ROLLBACK_OR_RECOVERY.to_string()));
        assert!(stored
            .reviewed_roles
            .contains(&reviewed_role::CHANGE_WINDOW.to_string()));
    }

    #[test]
    fn missing_source_field_is_not_filled_without_provenance() {
        let (_dir, conn) = open_temp_db();
        let dir = tempfile::tempdir().unwrap();
        // No start/end in the source record.
        write_record(dir.path(), "INC0040104", "Incident", "Customer SOI", "", "");
        sync_dir(&conn, dir.path());
        let mut review = base_review("INC0040104");
        // An interpretation field with no provenance entry is rejected.
        review.linked_change_ids = vec!["CHG0099901".to_string()];
        let err = validate_review(&conn, "grnoc-public-task-viewer", review, None).unwrap_err();
        assert!(err.contains("linked_change_ids"), "unexpected error: {err}");
        // With provenance the field is accepted.
        let doc_id = insert_test_document(&conn);
        let mut review = base_review("INC0040104");
        review.linked_change_ids = vec!["CHG0099901".to_string()];
        review.provenance.push(ReviewProvenance {
            field: "linked_change_ids".to_string(),
            source: "ReferenceDocument:AAR Appendix A".to_string(),
            detail: "listed under maintenance window".to_string(),
            source_document_id: Some(doc_id),
        });
        let review =
            validate_review(&conn, "grnoc-public-task-viewer", review, Some(doc_id)).unwrap();
        assert_eq!(review.linked_change_ids, vec!["CHG0099901"]);
        // The source snapshot still has no window — nothing was backfilled.
        let snapshots = db::list_snapshots(&conn, event_id(&conn, "INC0040104")).unwrap();
        let normalized: serde_json::Value =
            serde_json::from_str(&snapshots[0].normalized_json).unwrap();
        assert_eq!(normalized["start"], "");
        assert_eq!(normalized["end"], "");
    }

    #[test]
    fn reviewed_relationship_retains_all_supporting_sources() {
        let (_dir, conn) = open_temp_db();
        let dir = tempfile::tempdir().unwrap();
        write_record(
            dir.path(),
            "CHG0099902",
            "Change Request",
            "Tracked in INC0040201.",
            "2019-08-21T04:00:00Z",
            "2019-08-21T05:00:00Z",
        );
        write_record(
            dir.path(),
            "INC0040201",
            "Incident",
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
        let doc_id = insert_test_document(&conn);
        // A reviewed edge with BOTH ticket text and document support.
        let inserted = import_reviewed_edge(
            &conn,
            "grnoc-public-task-viewer",
            &ReviewedEdgeInput {
                from: "INC0040201".to_string(),
                to: "CHG0099902".to_string(),
                kind: RELATIONSHIP_RELATED_CHANGE.to_string(),
                evidence: EVIDENCE_ANALYST_REVIEWED.to_string(),
                note: Some("reviewing services after a partner maintenance".to_string()),
                document_cited: true,
            },
            Some(doc_id),
            "2026-08-01T00:00:00Z",
        )
        .unwrap();
        assert!(inserted);
        let edges = store::list_relationships(&conn, Some(event_id(&conn, "INC0040201"))).unwrap();
        let reviewed: Vec<_> = edges
            .iter()
            .filter(|e| e.relationship_kind == RELATIONSHIP_RELATED_CHANGE)
            .collect();
        assert_eq!(reviewed.len(), 1);
        // One edge carries both the ticket snapshot and the document id.
        assert!(reviewed[0].source_snapshot_id.is_some());
        assert_eq!(reviewed[0].source_document_id, Some(doc_id));
        // Re-import is idempotent — nothing is overwritten.
        let again = import_reviewed_edge(
            &conn,
            "grnoc-public-task-viewer",
            &ReviewedEdgeInput {
                from: "INC0040201".to_string(),
                to: "CHG0099902".to_string(),
                kind: RELATIONSHIP_RELATED_CHANGE.to_string(),
                evidence: EVIDENCE_ANALYST_REVIEWED.to_string(),
                note: None,
                document_cited: true,
            },
            Some(doc_id),
            "2026-08-01T01:00:00Z",
        )
        .unwrap();
        assert!(!again);
        let edges = store::list_relationships(&conn, Some(event_id(&conn, "INC0040201"))).unwrap();
        let reviewed: Vec<_> = edges
            .iter()
            .filter(|e| e.relationship_kind == RELATIONSHIP_RELATED_CHANGE)
            .collect();
        assert_eq!(reviewed.len(), 1);
        assert_eq!(
            reviewed[0].note.as_deref(),
            Some("reviewing services after a partner maintenance")
        );
    }

    #[test]
    fn explicit_reference_has_precedence_over_temporal_candidate() {
        let (_dir, conn) = open_temp_db();
        let dir = tempfile::tempdir().unwrap();
        write_record(
            dir.path(),
            "CHG0099903",
            "Change Request",
            "Tracked in INC0040202.",
            "2019-08-21T04:00:00Z",
            "2019-08-21T05:00:00Z",
        );
        write_record(
            dir.path(),
            "INC0040202",
            "Incident",
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
        // The explicit text edge is extracted; temporal overlap also exists
        // but must not displace the explicit relationship.
        crate::catalog::relationships::derive_temporal_overlaps(
            &conn,
            "grnoc-public-task-viewer",
            "2026-08-01T00:00:00Z",
        )
        .unwrap();
        let edges = store::list_relationships(&conn, None).unwrap();
        let explicit: Vec<_> = edges
            .iter()
            .filter(|e| e.evidence_kind == EVIDENCE_EXPLICIT_TICKET_TEXT)
            .collect();
        assert_eq!(explicit.len(), 1);
        assert_eq!(
            explicit[0].relationship_kind,
            RELATIONSHIP_TRACKS_REMAINING_IMPACT
        );
        // The derived overlap edges exist separately (both directions)
        // with their own evidence kind.
        let derived: Vec<_> = edges
            .iter()
            .filter(|e| e.evidence_kind == EVIDENCE_DERIVED_TEMPORAL_OVERLAP)
            .collect();
        assert_eq!(derived.len(), 2);
        // Reviewing the explicit edge again never turns it into a temporal one.
        import_reviewed_edge(
            &conn,
            "grnoc-public-task-viewer",
            &ReviewedEdgeInput {
                from: "CHG0099903".to_string(),
                to: "INC0040202".to_string(),
                kind: RELATIONSHIP_TRACKS_REMAINING_IMPACT.to_string(),
                evidence: EVIDENCE_ANALYST_REVIEWED.to_string(),
                note: Some("maintenance impact tracked".to_string()),
                document_cited: false,
            },
            None,
            "2026-08-01T00:00:00Z",
        )
        .unwrap();
        let edges = store::list_relationships(&conn, None).unwrap();
        assert!(edges
            .iter()
            .any(|e| e.evidence_kind == EVIDENCE_EXPLICIT_TICKET_TEXT
                && e.relationship_kind == RELATIONSHIP_TRACKS_REMAINING_IMPACT));
    }

    #[test]
    fn unavailable_ticket_remains_unresolved_reference() {
        let (_dir, conn) = open_temp_db();
        let dir = tempfile::tempdir().unwrap();
        write_record(
            dir.path(),
            "INC0040203",
            "Incident",
            "x",
            "2019-08-21T04:00:00Z",
            "2019-08-21T05:00:00Z",
        );
        sync_dir(&conn, dir.path());
        let doc_id = insert_test_document(&conn);
        // TASK0099901 has NO snapshot — a document-cited reference only.
        let inserted = import_reviewed_edge(
            &conn,
            "grnoc-public-task-viewer",
            &ReviewedEdgeInput {
                from: "INC0040203".to_string(),
                to: "TASK0099901".to_string(),
                kind: RELATIONSHIP_OPERATIONAL_TASK_DURING.to_string(),
                evidence: EVIDENCE_REFERENCE_DOCUMENT.to_string(),
                note: Some("AAR timeline names the task".to_string()),
                document_cited: true,
            },
            Some(doc_id),
            "2026-08-01T00:00:00Z",
        )
        .unwrap();
        assert!(inserted);
        // The edge stays unresolved; resolution finds no event to attach.
        let resolved = crate::catalog::relationships::resolve_unresolved_edges(
            &conn,
            "grnoc-public-task-viewer",
        )
        .unwrap();
        assert_eq!(resolved, 0);
        let edges = store::list_relationships(&conn, None).unwrap();
        let task_edge: Vec<_> = edges
            .iter()
            .filter(|e| e.to_external_id == "TASK0099901")
            .collect();
        assert_eq!(task_edge.len(), 1);
        assert!(task_edge[0].to_event_id.is_none());
        assert_eq!(task_edge[0].source_document_id, Some(doc_id));
        // No snapshot was manufactured for the task.
        assert!(
            db::get_event_by_external(&conn, "grnoc-public-task-viewer", "TASK0099901")
                .unwrap()
                .is_none()
        );
        // The audit lists it as an unresolved reference.
        let audit = graph_audit(&conn, "grnoc-public-task-viewer").unwrap();
        let row = audit
            .iter()
            .find(|r| r.to_external == "TASK0099901")
            .unwrap();
        assert!(!row.to_resolved);
        assert_eq!(row.evidence_kind, EVIDENCE_REFERENCE_DOCUMENT);
    }

    #[test]
    fn one_relationship_can_have_document_and_ticket_support() {
        let (_dir, conn) = open_temp_db();
        let dir = tempfile::tempdir().unwrap();
        write_record(
            dir.path(),
            "INC0040204",
            "Incident",
            "reviewing Layer3 services after a partner maintenance",
            "2019-08-21T04:00:00Z",
            "2019-08-21T05:00:00Z",
        );
        write_record(
            dir.path(),
            "CHG0099904",
            "Change Request",
            "x",
            "2019-08-21T04:00:00Z",
            "2019-08-21T05:00:00Z",
        );
        sync_dir(&conn, dir.path());
        let doc_id = insert_test_document(&conn);
        let inserted = import_reviewed_edge(
            &conn,
            "grnoc-public-task-viewer",
            &ReviewedEdgeInput {
                from: "INC0040204".to_string(),
                to: "CHG0099904".to_string(),
                kind: RELATIONSHIP_RELATED_CHANGE.to_string(),
                evidence: EVIDENCE_ANALYST_REVIEWED.to_string(),
                note: Some("partner maintenance wording + AAR sequence".to_string()),
                document_cited: true,
            },
            Some(doc_id),
            "2026-08-01T00:00:00Z",
        )
        .unwrap();
        assert!(inserted);
        let edges = store::list_relationships(&conn, None).unwrap();
        let edge = edges
            .iter()
            .find(|e| e.relationship_kind == RELATIONSHIP_RELATED_CHANGE)
            .unwrap();
        assert!(edge.source_snapshot_id.is_some(), "ticket support");
        assert_eq!(edge.source_document_id, Some(doc_id), "document support");
    }

    #[test]
    fn graph_does_not_replace_individual_ticket_history() {
        let (_dir, conn) = open_temp_db();
        let dir = tempfile::tempdir().unwrap();
        write_record(
            dir.path(),
            "INC0040205",
            "Incident",
            "x",
            "2019-08-21T04:00:00Z",
            "2019-08-21T05:00:00Z",
        );
        write_record(
            dir.path(),
            "INC0040206",
            "Incident",
            "y",
            "2019-08-21T04:00:00Z",
            "2019-08-21T05:00:00Z",
        );
        sync_dir(&conn, dir.path());
        // Reviews with a shared entity label derive an entity-overlap edge.
        let mut r1 = base_review("INC0040205");
        r1.entity_labels = vec!["SampleParticipant".to_string()];
        let r1 = validate_review(&conn, "grnoc-public-task-viewer", r1, None).unwrap();
        store::upsert_ticket_review(&conn, &r1).unwrap();
        let mut r2 = base_review("INC0040206");
        r2.entity_labels = vec!["SampleParticipant".to_string()];
        let r2 = validate_review(&conn, "grnoc-public-task-viewer", r2, None).unwrap();
        store::upsert_ticket_review(&conn, &r2).unwrap();
        let n = derive_entity_overlaps(&conn, "2026-08-01T00:00:00Z").unwrap();
        assert_eq!(n, 1);
        // The graph adds rows; the individual ticket snapshots remain.
        let events = db::list_events(&conn).unwrap();
        assert_eq!(events.len(), 2);
        for e in &events {
            assert_eq!(db::list_snapshots(&conn, e.id).unwrap().len(), 1);
        }
        let reviews = store::list_ticket_reviews(&conn).unwrap();
        assert_eq!(reviews.len(), 2);
    }
}
