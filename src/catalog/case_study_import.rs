//! Case-study data-file import.
//!
//! A reviewed case-study data file (`case-studies/<slug>/case-study.json`)
//! is the canonical representation of an `IncidentCaseStudy`: documents,
//! phases, related tickets, claims with observability, and analysis targets.
//! The import is transactional, idempotent (slug + content hash), and
//! schema-validated; a conflicting immutable revision for the same slug is
//! rejected. Related tickets are linked to existing catalog events where
//! they exist; otherwise their external identifiers are preserved as
//! unresolved document references — no source snapshot is fabricated.

use rusqlite::Connection;

use super::domain::*;
use super::store;

/// Canonical data-file schema version.
pub const CASE_STUDY_DATA_SCHEMA_VERSION: u32 = 1;

/// Import summary.
#[derive(Debug, Clone, Default)]
pub struct CaseStudyImportSummary {
    pub case_study_id: i64,
    pub slug: String,
    pub created: bool,
    pub documents: usize,
    pub event_links: usize,
    pub linked_events: usize,
    pub unresolved_references: usize,
    pub phases: usize,
    pub claims: usize,
    pub targets: usize,
}

// ── Data-file model ────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct CaseStudyDataFile {
    pub schema_version: u32,
    pub slug: String,
    pub title: String,
    pub summary: String,
    pub start_utc: Option<String>,
    pub end_utc: Option<String>,
    pub status: Option<String>,
    pub documents: Vec<DataDocument>,
    #[serde(default)]
    pub document_links: Vec<DataDocumentLink>,
    pub phases: Vec<DataPhase>,
    pub related_events: Vec<DataEventLink>,
    pub claims: Vec<DataClaim>,
    pub targets: Vec<DataTarget>,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct DataDocument {
    pub title: String,
    pub source_url: Option<String>,
    pub doc_type: String,
    pub media_type: String,
    pub sha256: String,
    pub page_count: Option<i64>,
    pub publication_date: Option<String>,
    pub provenance: String,
    #[serde(default)]
    pub redistribution_status: Option<String>,
    /// Catalog-relative local path; usually NULL until `document import`.
    pub local_path: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct DataDocumentLink {
    /// Index into `documents`.
    pub document: usize,
    pub relationship: String,
    pub reviewed_note: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct DataPhase {
    pub label: String,
    pub start_utc: String,
    pub end_utc: String,
    pub start_precision: String,
    pub end_precision: String,
    pub description: String,
    /// Index into `documents`.
    pub source_document: usize,
    pub source_page_or_section: String,
    pub review_status: String,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct DataEventLink {
    pub external_identifier: String,
    pub relationship: String,
    pub reviewed_note: Option<String>,
    /// Index into `documents`; the document that references this ticket.
    #[serde(default)]
    pub source_document: Option<usize>,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct DataClaim {
    pub claim_type: String,
    pub claim_text: String,
    pub qualification: Option<String>,
    /// Index into `documents`.
    pub source_document: usize,
    pub source_page_or_section: String,
    pub review_status: String,
    pub time_or_phase: Option<String>,
    pub observability: String,
    pub observability_rationale: String,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct DataTarget {
    pub source_label: String,
    pub role_in_report: String,
    pub candidate_org_identity: Option<String>,
    #[serde(default)]
    pub candidate_origin_asns: Vec<u32>,
    pub candidate_predicate: Option<String>,
    pub historical_validity_status: String,
    pub provenance: Option<String>,
    pub research_status: String,
    pub reviewed_note: Option<String>,
}

// ── Validation ─────────────────────────────────────────────────────

fn is_utc(s: &str) -> bool {
    chrono::DateTime::parse_from_rfc3339(s).is_ok()
}

fn normalize_utc(s: &str) -> Result<String, String> {
    use chrono::SecondsFormat;
    chrono::DateTime::parse_from_rfc3339(s)
        .map(|dt| {
            dt.with_timezone(&chrono::Utc)
                .to_rfc3339_opts(SecondsFormat::Secs, true)
        })
        .map_err(|_| format!("timestamp is not valid UTC (RFC 3339): {s}"))
}

fn valid_relationship(s: &str) -> bool {
    matches!(
        s,
        RELATIONSHIP_PRIMARY_CHANGE
            | RELATIONSHIP_PRIMARY_INCIDENT
            | RELATIONSHIP_ROLLBACK_CHANGE
            | RELATIONSHIP_PARTICIPANT_INCIDENT
            | RELATIONSHIP_ALARM
            | RELATIONSHIP_OPERATIONAL_TASK
            | RELATIONSHIP_COMMUNICATION
            | RELATIONSHIP_RELATED
    )
}

fn valid_claim_type(s: &str) -> bool {
    matches!(
        s,
        CLAIM_TYPE_REPORTED_IMPACT
            | CLAIM_TYPE_REPORTED_MECHANISM
            | CLAIM_TYPE_REPORTED_TIMELINE
            | CLAIM_TYPE_REPORTED_RECOVERY
            | CLAIM_TYPE_REPORTED_LIMITATION
            | CLAIM_TYPE_PROCESS_FINDING
    )
}

fn valid_observability(s: &str) -> bool {
    matches!(
        s,
        OBSERVABILITY_POTENTIALLY_VISIBLE
            | OBSERVABILITY_INDIRECTLY_VISIBLE
            | OBSERVABILITY_NOT_DIRECTLY_VISIBLE
            | OBSERVABILITY_UNKNOWN
    )
}

fn valid_target_status(s: &str) -> bool {
    matches!(
        s,
        TARGET_STATUS_UNRESEARCHED
            | TARGET_STATUS_CANDIDATE
            | TARGET_STATUS_HISTORICALLY_REVIEWED
            | TARGET_STATUS_UNRESOLVED
            | TARGET_STATUS_NOT_APPLICABLE
            | TARGET_STATUS_AMBIGUOUS_SERVICE_IDENTITY
    )
}

fn valid_media_type(s: &str) -> bool {
    super::document::MEDIA_TYPE_ALLOWLIST
        .iter()
        .any(|(_, mt)| *mt == s)
}

fn valid_sha256(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Validate a parsed data file; returns a list of violations.
fn validate(data: &CaseStudyDataFile) -> Result<(), String> {
    let ndocs = data.documents.len();
    let mut problems: Vec<String> = Vec::new();
    if data.schema_version != CASE_STUDY_DATA_SCHEMA_VERSION {
        problems.push(format!(
            "schema_version {} not supported (expected {CASE_STUDY_DATA_SCHEMA_VERSION})",
            data.schema_version
        ));
    }
    if data.slug.is_empty() || data.title.is_empty() || data.summary.is_empty() {
        problems.push("slug, title, and summary must be non-empty".to_string());
    }
    for (i, d) in data.documents.iter().enumerate() {
        if !valid_sha256(&d.sha256) {
            problems.push(format!("documents[{i}] sha256 is not a 64-hex digest"));
        }
        if !valid_media_type(&d.media_type) {
            problems.push(format!(
                "documents[{i}] media_type '{0}' not allowlisted",
                d.media_type
            ));
        }
        if d.title.is_empty() || d.provenance.is_empty() {
            problems.push(format!(
                "documents[{i}] title and provenance must be non-empty"
            ));
        }
        if let Some(p) = &d.local_path {
            if p.starts_with('/') || p.contains("..") {
                problems.push(format!(
                    "documents[{i}] local_path must be catalog-relative"
                ));
            }
        }
    }
    for (i, dl) in data.document_links.iter().enumerate() {
        if dl.document >= ndocs {
            problems.push(format!(
                "document_links[{i}] references missing document {0}",
                dl.document
            ));
        }
    }
    for (i, p) in data.phases.iter().enumerate() {
        if p.source_document >= ndocs {
            problems.push(format!(
                "phases[{i}] references missing document {0}",
                p.source_document
            ));
        }
        if p.source_page_or_section.is_empty() || p.review_status.is_empty() {
            problems.push(format!(
                "phases[{i}] requires source_page_or_section and review_status (source provenance)"
            ));
        }
        if !is_utc(&p.start_utc) || !is_utc(&p.end_utc) {
            problems.push(format!("phases[{i}] boundaries must be UTC (RFC 3339)"));
        }
        if p.start_precision != PHASE_PRECISION_EXACT
            && p.start_precision != PHASE_PRECISION_SUMMARIZED
        {
            problems.push(format!(
                "phases[{i}] start_precision must be exact or summarized"
            ));
        }
        if p.end_precision != PHASE_PRECISION_EXACT && p.end_precision != PHASE_PRECISION_SUMMARIZED
        {
            problems.push(format!(
                "phases[{i}] end_precision must be exact or summarized"
            ));
        }
        if p.label.is_empty() {
            problems.push(format!("phases[{i}] label must be non-empty"));
        }
    }
    // Unintended overlap: consecutive phases must not overlap (touching
    // boundaries are allowed); each phase start must precede its end.
    let mut sorted: Vec<(usize, &DataPhase)> = data.phases.iter().enumerate().collect();
    sorted.sort_by(|a, b| a.1.start_utc.cmp(&b.1.start_utc).then(a.0.cmp(&b.0)));
    for w in sorted.windows(2) {
        let (_, prev) = w[0];
        let (_, next) = w[1];
        if next.start_utc < prev.end_utc {
            problems.push(format!(
                "phases overlap unintentionally: '{}' ends {} after '{}' starts {}",
                prev.label, prev.end_utc, next.label, next.start_utc
            ));
        }
    }
    for (i, p) in data.phases.iter().enumerate() {
        if p.start_utc >= p.end_utc {
            problems.push(format!("phases[{i}] start must precede end"));
        }
    }
    for (i, e) in data.related_events.iter().enumerate() {
        if e.external_identifier.is_empty() {
            problems.push(format!(
                "related_events[{i}] external_identifier must be non-empty"
            ));
        }
        if !valid_relationship(&e.relationship) {
            problems.push(format!(
                "related_events[{i}] relationship '{0}' is not in the vocabulary",
                e.relationship
            ));
        }
        if let Some(d) = e.source_document {
            if d >= ndocs {
                problems.push(format!(
                    "related_events[{i}] references missing document {d}"
                ));
            }
        }
    }
    for (i, c) in data.claims.iter().enumerate() {
        if c.source_document >= ndocs {
            problems.push(format!(
                "claims[{i}] references missing document {0}",
                c.source_document
            ));
        }
        if c.source_page_or_section.is_empty() || c.review_status.is_empty() {
            problems.push(format!("claims[{i}] requires source provenance"));
        }
        if !valid_claim_type(&c.claim_type) {
            problems.push(format!(
                "claims[{i}] claim_type '{0}' is not in the vocabulary",
                c.claim_type
            ));
        }
        if !valid_observability(&c.observability) {
            problems.push(format!(
                "claims[{i}] observability must be explicit (one of the four classifications)"
            ));
        }
        if c.observability_rationale.is_empty() {
            problems.push(format!(
                "claims[{i}] observability_rationale must be non-empty"
            ));
        }
        if c.claim_text.is_empty() {
            problems.push(format!("claims[{i}] claim_text must be non-empty"));
        }
    }
    for (i, t) in data.targets.iter().enumerate() {
        if !valid_target_status(&t.research_status) {
            problems.push(format!(
                "targets[{i}] research_status '{0}' is not in the vocabulary",
                t.research_status
            ));
        }
        if !valid_target_status(&t.historical_validity_status) {
            problems.push(format!(
                "targets[{i}] historical_validity_status '{0}' is not in the vocabulary",
                t.historical_validity_status
            ));
        }
        if t.source_label.is_empty() {
            problems.push(format!("targets[{i}] source_label must be non-empty"));
        }
    }
    if problems.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "case-study data file is invalid:\n  - {}",
            problems.join("\n  - ")
        ))
    }
}

// ── Import ─────────────────────────────────────────────────────────

/// Resolve a document by content SHA-256, creating a metadata-only
/// reference-document record when the content is not yet cataloged.
fn resolve_document(conn: &Connection, d: &DataDocument) -> Result<i64, String> {
    let existing: Option<i64> = conn
        .query_row(
            "SELECT document_id FROM document_revisions WHERE sha256 = ?1",
            [&d.sha256],
            |r| r.get(0),
        )
        .ok();
    if let Some(document_id) = existing {
        return Ok(document_id);
    }
    let now = chrono::Utc::now().to_rfc3339();
    let doc = ReferenceDocument {
        id: 0,
        title: d.title.clone(),
        source_url: d.source_url.clone(),
        doc_type: d.doc_type.clone(),
        redistribution_status: d
            .redistribution_status
            .clone()
            .unwrap_or_else(|| "Unknown".to_string()),
        publication_date: d.publication_date.clone(),
        provenance: d.provenance.clone(),
        imported_utc: now.clone(),
    };
    let document_id = store::insert_reference_document(conn, &doc)?;
    let revision: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(revision), 0) + 1 FROM document_revisions WHERE document_id = ?1",
            [document_id],
            |r| r.get(0),
        )
        .map_err(|e| format!("catalog read failed: {e}"))?;
    let rev = DocumentRevision {
        id: 0,
        document_id,
        revision,
        sha256: d.sha256.clone(),
        media_type: d.media_type.clone(),
        page_count: d.page_count,
        local_path: d.local_path.clone(),
        metadata_json: None,
        imported_utc: now,
    };
    store::insert_document_revision(conn, &rev)?;
    Ok(document_id)
}

/// Import a reviewed case-study data file into the catalog.
///
/// `path` may be the directory containing `case-study.json` or the file
/// itself. The whole import is one transaction.
pub fn import_case_study(
    conn: &Connection,
    path: &std::path::Path,
) -> Result<CaseStudyImportSummary, String> {
    let file = if path.is_dir() {
        path.join("case-study.json")
    } else {
        path.to_path_buf()
    };
    if !file.is_file() {
        return Err(format!(
            "case-study data file not found: {}",
            file.display()
        ));
    }
    let raw = std::fs::read_to_string(&file)
        .map_err(|e| format!("cannot read {}: {e}", file.display()))?;
    let data: CaseStudyDataFile = serde_json::from_str(&raw)
        .map_err(|e| format!("invalid case-study data file {}: {e}", file.display()))?;
    validate(&data)?;
    let content_sha = super::import::sha256_hex_bytes(
        serde_json::to_string(&data)
            .map_err(|e| format!("cannot canonicalize data file: {e}"))?
            .as_bytes(),
    );
    let now = chrono::Utc::now().to_rfc3339();

    let tx = conn
        .unchecked_transaction()
        .map_err(|e| format!("cannot start import transaction: {e}"))?;

    let cs = CaseStudy {
        id: 0,
        slug: data.slug.clone(),
        title: data.title.clone(),
        summary: data.summary.clone(),
        start_utc: data.start_utc.as_deref().map(normalize_utc).transpose()?,
        end_utc: data.end_utc.as_deref().map(normalize_utc).transpose()?,
        status: data.status.clone().unwrap_or_else(|| "Active".to_string()),
        content_sha256: content_sha.clone(),
        created_utc: now.clone(),
        updated_utc: now,
    };
    let case_study_id = store::insert_case_study(&tx, &cs)?;

    // Idempotent short-circuit: children already imported for this revision.
    let already: Option<i64> = tx
        .query_row(
            "SELECT id FROM case_study_phases WHERE case_study_id = ?1 LIMIT 1",
            [case_study_id],
            |r| r.get(0),
        )
        .ok();
    let mut summary = CaseStudyImportSummary {
        case_study_id,
        slug: data.slug.clone(),
        created: already.is_none(),
        ..Default::default()
    };
    if already.is_some() {
        tx.commit()
            .map_err(|e| format!("cannot commit import: {e}"))?;
        return Ok(summary);
    }

    let mut doc_ids: Vec<i64> = Vec::with_capacity(data.documents.len());
    for d in &data.documents {
        let id = resolve_document(&tx, d)?;
        doc_ids.push(id);
        summary.documents += 1;
    }
    let aar_doc = doc_ids.first().copied();

    for dl in &data.document_links {
        store::insert_case_study_document_link(
            &tx,
            &CaseStudyDocumentLink {
                id: 0,
                case_study_id,
                document_id: doc_ids[dl.document],
                relationship: dl.relationship.clone(),
                reviewed_note: dl.reviewed_note.clone(),
            },
        )?;
    }

    for (i, e) in data.related_events.iter().enumerate() {
        // Link the first catalog event with this external id, if any; never
        // fabricate a source snapshot for missing historical tickets.
        let event_id: Option<i64> = tx
            .query_row(
                "SELECT e.id FROM catalog_events e
                 WHERE e.external_id = ?1
                 ORDER BY (SELECT COUNT(*) FROM analysis_plans p
                           JOIN manifest_revisions m ON m.event_id = e.id
                           AND p.manifest_revision_id = m.id) DESC,
                          e.source_kind LIMIT 1",
                [&e.external_identifier],
                |r| r.get(0),
            )
            .ok();
        let link = CaseStudyEventLink {
            id: 0,
            case_study_id,
            catalog_event_id: event_id,
            external_identifier: e.external_identifier.clone(),
            relationship: e.relationship.clone(),
            reviewed_note: e.reviewed_note.clone(),
            sort_order: i as i64,
            source_document_id: e.source_document.map(|d| doc_ids[d]).or(aar_doc),
        };
        store::insert_case_study_event_link(&tx, &link)?;
        summary.event_links += 1;
        if event_id.is_some() {
            summary.linked_events += 1;
        } else {
            summary.unresolved_references += 1;
        }
    }

    for (i, p) in data.phases.iter().enumerate() {
        store::insert_case_study_phase(
            &tx,
            &CaseStudyPhase {
                id: 0,
                case_study_id,
                label: p.label.clone(),
                start_utc: normalize_utc(&p.start_utc)?,
                end_utc: normalize_utc(&p.end_utc)?,
                start_precision: p.start_precision.clone(),
                end_precision: p.end_precision.clone(),
                description: p.description.clone(),
                source_document_id: doc_ids[p.source_document],
                source_page_or_section: p.source_page_or_section.clone(),
                review_status: p.review_status.clone(),
                sort_order: i as i64,
            },
        )?;
        summary.phases += 1;
    }

    for (i, c) in data.claims.iter().enumerate() {
        store::insert_case_study_claim(
            &tx,
            &CaseStudyClaim {
                id: 0,
                case_study_id,
                claim_type: c.claim_type.clone(),
                claim_text: c.claim_text.clone(),
                qualification: c.qualification.clone(),
                source_document_id: doc_ids[c.source_document],
                source_page_or_section: c.source_page_or_section.clone(),
                review_status: c.review_status.clone(),
                time_or_phase: c.time_or_phase.clone(),
                observability: c.observability.clone(),
                observability_rationale: c.observability_rationale.clone(),
                sort_order: i as i64,
            },
        )?;
        summary.claims += 1;
    }

    for (i, t) in data.targets.iter().enumerate() {
        store::insert_case_study_target(
            &tx,
            &CaseStudyTarget {
                id: 0,
                case_study_id,
                source_label: t.source_label.clone(),
                role_in_report: t.role_in_report.clone(),
                candidate_org_identity: t.candidate_org_identity.clone(),
                candidate_origin_asns_json: if t.candidate_origin_asns.is_empty() {
                    None
                } else {
                    Some(serde_json::to_string(&t.candidate_origin_asns).unwrap())
                },
                candidate_predicate: t.candidate_predicate.clone(),
                historical_validity_status: t.historical_validity_status.clone(),
                provenance: t.provenance.clone(),
                research_status: t.research_status.clone(),
                reviewed_note: t.reviewed_note.clone(),
                sort_order: i as i64,
            },
        )?;
        summary.targets += 1;
    }

    tx.commit()
        .map_err(|e| format!("cannot commit import: {e}"))?;
    Ok(summary)
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

    fn doc() -> serde_json::Value {
        serde_json::json!({
            "title": "After Action Report",
            "source_url": "https://example.invalid/reports/aar.pdf",
            "doc_type": "AfterActionReport",
            "media_type": "application/pdf",
            "sha256": "d29df26a269962afeb4c671063ea64dec6103e226c039e5939d5af99eedd7114",
            "page_count": 15,
            "provenance": "operator-authored report",
            "redistribution_status": "Unknown"
        })
    }

    fn base_data() -> serde_json::Value {
        serde_json::json!({
            "schema_version": 1,
            "slug": "incident-x",
            "title": "Incident X",
            "summary": "Reviewed operator-reported incident summary.",
            "start_utc": "2019-08-21T04:00:00Z",
            "end_utc": "2019-08-21T22:38:00Z",
            "documents": [doc()],
            "document_links": [{"document": 0, "relationship": "PrimarySource"}],
            "phases": [{
                "label": "Scheduled migration",
                "start_utc": "2019-08-21T04:00:00Z",
                "end_utc": "2019-08-21T10:00:00Z",
                "start_precision": "exact",
                "end_precision": "summarized",
                "description": "Planned maintenance window.",
                "source_document": 0,
                "source_page_or_section": "Timeline (detailed)",
                "review_status": "Reviewed"
            }],
            "related_events": [{
                "external_identifier": "INC0040257",
                "relationship": "PrimaryIncident",
                "reviewed_note": "referenced by AAR"
            }],
            "claims": [{
                "claim_type": "ReportedImpact",
                "claim_text": "The change caused disruption.",
                "qualification": "operator-reported; extent varied",
                "source_document": 0,
                "source_page_or_section": "Summary",
                "review_status": "Reviewed",
                "time_or_phase": "phase:0",
                "observability": "PotentiallyVisibleInPublicBgp",
                "observability_rationale": "Participant path changes may be visible."
            }],
            "targets": [{
                "source_label": "Participant A",
                "role_in_report": "connector participant",
                "historical_validity_status": "Unresearched",
                "research_status": "Unresearched",
                "provenance": "AAR context"
            }]
        })
    }

    fn write_data(dir: &std::path::Path, data: &serde_json::Value) -> std::path::PathBuf {
        let p = dir.join("case-study.json");
        std::fs::write(&p, serde_json::to_string_pretty(data).unwrap()).unwrap();
        p
    }

    #[test]
    fn case_study_import_is_idempotent() {
        let (_dir, conn) = open_temp_db();
        let tmp = tempfile::tempdir().unwrap();
        let data = base_data();
        let path = write_data(tmp.path(), &data);
        let a = import_case_study(&conn, &path).unwrap();
        assert!(a.created);
        assert_eq!(a.phases, 1);
        let b = import_case_study(&conn, &path).unwrap();
        assert!(!b.created);
        assert_eq!(a.case_study_id, b.case_study_id);
        let counts: (i64, i64, i64) = conn
            .query_row(
                "SELECT (SELECT COUNT(*) FROM case_study_phases),
                        (SELECT COUNT(*) FROM case_study_claims),
                        (SELECT COUNT(*) FROM case_study_event_links)",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(counts, (1, 1, 1));
    }

    #[test]
    fn case_study_import_links_existing_event() {
        let (_dir, conn) = open_temp_db();
        let e = store::upsert_event(&conn, "grnoc", "INC0040257", "2019-08-22T00:00:00Z").unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let path = write_data(tmp.path(), &base_data());
        let summary = import_case_study(&conn, &path).unwrap();
        assert_eq!(summary.linked_events, 1);
        assert_eq!(summary.unresolved_references, 0);
        let (linked, ext): (Option<i64>, String) = conn
            .query_row(
                "SELECT catalog_event_id, external_identifier FROM case_study_event_links
                 WHERE case_study_id = ?1",
                [summary.case_study_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(linked, Some(e));
        assert_eq!(ext, "INC0040257");
    }

    #[test]
    fn case_study_import_preserves_unresolved_ticket_reference() {
        let (_dir, conn) = open_temp_db();
        let tmp = tempfile::tempdir().unwrap();
        let path = write_data(tmp.path(), &base_data());
        // No catalog event exists for INC0040257.
        let summary = import_case_study(&conn, &path).unwrap();
        assert_eq!(summary.linked_events, 0);
        assert_eq!(summary.unresolved_references, 1);
        let (linked, events): (Option<i64>, i64) = conn
            .query_row(
                "SELECT (SELECT catalog_event_id FROM case_study_event_links
                          WHERE case_study_id = ?1),
                        (SELECT COUNT(*) FROM event_snapshots)",
                [summary.case_study_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert!(linked.is_none());
        assert_eq!(events, 0, "no source snapshot may be fabricated");
    }

    #[test]
    fn invalid_phase_provenance_rejects_import() {
        let (_dir, conn) = open_temp_db();
        let tmp = tempfile::tempdir().unwrap();
        let mut data = base_data();
        data["phases"][0]["source_document"] = serde_json::json!(7);
        let path = write_data(tmp.path(), &data);
        let err = import_case_study(&conn, &path).unwrap_err();
        assert!(err.contains("references missing document"), "{err}");

        let mut data = base_data();
        data["phases"][0]["source_page_or_section"] = serde_json::json!("");
        let path = write_data(tmp.path(), &data);
        let err = import_case_study(&conn, &path).unwrap_err();
        assert!(err.contains("source provenance"), "{err}");
    }

    #[test]
    fn conflicting_immutable_case_study_revision_is_rejected() {
        let (_dir, conn) = open_temp_db();
        let tmp = tempfile::tempdir().unwrap();
        let path = write_data(tmp.path(), &base_data());
        import_case_study(&conn, &path).unwrap();
        let mut data = base_data();
        data["summary"] = serde_json::json!("Changed reviewed summary.");
        let path2 = write_data(tmp.path(), &data);
        let err = import_case_study(&conn, &path2).unwrap_err();
        assert!(err.contains("conflicting immutable"), "{err}");
    }

    #[test]
    fn phase_times_are_utc() {
        let (_dir, conn) = open_temp_db();
        let tmp = tempfile::tempdir().unwrap();
        let mut data = base_data();
        data["phases"][0]["start_utc"] = serde_json::json!("2019-08-21T04:00:00+02:00");
        let path = write_data(tmp.path(), &data);
        let summary = import_case_study(&conn, &path).unwrap();
        let stored: String = conn
            .query_row(
                "SELECT start_utc FROM case_study_phases WHERE case_study_id = ?1",
                [summary.case_study_id],
                |r| r.get(0),
            )
            .unwrap();
        assert!(stored.ends_with('Z'), "{stored}");
        assert_eq!(stored, "2019-08-21T02:00:00Z");

        let mut data = base_data();
        data["phases"][0]["start_utc"] = serde_json::json!("2019-08-21 04:00:00");
        let path = write_data(tmp.path(), &data);
        let err = import_case_study(&conn, &path).unwrap_err();
        assert!(err.contains("UTC"), "{err}");
    }

    #[test]
    fn phase_ranges_do_not_overlap_unintentionally() {
        let (_dir, conn) = open_temp_db();
        let tmp = tempfile::tempdir().unwrap();
        let mut data = base_data();
        data["phases"] = serde_json::json!([
            {
                "label": "first",
                "start_utc": "2019-08-21T04:00:00Z",
                "end_utc": "2019-08-21T10:00:00Z",
                "start_precision": "exact",
                "end_precision": "exact",
                "description": "d",
                "source_document": 0,
                "source_page_or_section": "Timeline",
                "review_status": "Reviewed"
            },
            {
                "label": "second",
                "start_utc": "2019-08-21T09:00:00Z",
                "end_utc": "2019-08-21T14:00:00Z",
                "start_precision": "exact",
                "end_precision": "exact",
                "description": "d",
                "source_document": 0,
                "source_page_or_section": "Timeline",
                "review_status": "Reviewed"
            }
        ]);
        let path = write_data(tmp.path(), &data);
        let err = import_case_study(&conn, &path).unwrap_err();
        assert!(err.contains("overlap unintentionally"), "{err}");

        // Touching boundaries are allowed (contiguous timeline).
        let mut data = base_data();
        data["phases"] = serde_json::json!([
            {
                "label": "first",
                "start_utc": "2019-08-21T04:00:00Z",
                "end_utc": "2019-08-21T10:00:00Z",
                "start_precision": "exact",
                "end_precision": "exact",
                "description": "d",
                "source_document": 0,
                "source_page_or_section": "Timeline",
                "review_status": "Reviewed"
            },
            {
                "label": "second",
                "start_utc": "2019-08-21T10:00:00Z",
                "end_utc": "2019-08-21T14:00:00Z",
                "start_precision": "exact",
                "end_precision": "exact",
                "description": "d",
                "source_document": 0,
                "source_page_or_section": "Timeline",
                "review_status": "Reviewed"
            }
        ]);
        let path = write_data(tmp.path(), &data);
        let summary = import_case_study(&conn, &path).unwrap();
        assert_eq!(summary.phases, 2);
    }

    #[test]
    fn phase_provenance_identifies_document_section() {
        let (_dir, conn) = open_temp_db();
        let tmp = tempfile::tempdir().unwrap();
        let path = write_data(tmp.path(), &base_data());
        let summary = import_case_study(&conn, &path).unwrap();
        let (doc_id, section, doc_title): (i64, String, String) = conn
            .query_row(
                "SELECT p.source_document_id, p.source_page_or_section, d.title
                 FROM case_study_phases p
                 JOIN reference_documents d ON d.id = p.source_document_id
                 WHERE p.case_study_id = ?1",
                [summary.case_study_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(section, "Timeline (detailed)");
        assert_eq!(doc_title, "After Action Report");
        assert!(doc_id > 0);
    }

    #[test]
    fn retrospective_belief_is_not_rendered_as_measured_fact() {
        let (_dir, conn) = open_temp_db();
        let tmp = tempfile::tempdir().unwrap();
        let path = write_data(tmp.path(), &base_data());
        let summary = import_case_study(&conn, &path).unwrap();
        let (start_prec, end_prec): (String, String) = conn
            .query_row(
                "SELECT start_precision, end_precision FROM case_study_phases
                 WHERE case_study_id = ?1",
                [summary.case_study_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(start_prec, PHASE_PRECISION_EXACT);
        assert_eq!(end_prec, PHASE_PRECISION_SUMMARIZED);
        assert_ne!(start_prec, end_prec);
    }

    #[test]
    fn case_study_timeline_is_deterministic() {
        let (_dir, conn) = open_temp_db();
        let tmp = tempfile::tempdir().unwrap();
        let mut data = base_data();
        data["phases"] = serde_json::json!([
            {
                "label": "scheduled",
                "start_utc": "2019-08-21T04:00:00Z",
                "end_utc": "2019-08-21T10:00:00Z",
                "start_precision": "exact",
                "end_precision": "exact",
                "description": "d",
                "source_document": 0,
                "source_page_or_section": "Timeline",
                "review_status": "Reviewed"
            },
            {
                "label": "troubleshooting",
                "start_utc": "2019-08-21T10:00:00Z",
                "end_utc": "2019-08-21T14:14:00Z",
                "start_precision": "exact",
                "end_precision": "exact",
                "description": "d",
                "source_document": 0,
                "source_page_or_section": "Timeline",
                "review_status": "Reviewed"
            }
        ]);
        let path = write_data(tmp.path(), &data);
        let summary = import_case_study(&conn, &path).unwrap();
        let labels: Vec<String> = conn
            .prepare(
                "SELECT label FROM case_study_phases WHERE case_study_id = ?1 ORDER BY sort_order",
            )
            .unwrap()
            .query_map([summary.case_study_id], |r| r.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(labels, vec!["scheduled", "troubleshooting"]);
    }

    #[test]
    fn claim_observability_is_explicit() {
        let (_dir, conn) = open_temp_db();
        let tmp = tempfile::tempdir().unwrap();
        let mut data = base_data();
        data["claims"][0]["observability"] = serde_json::json!("AnythingElse");
        let path = write_data(tmp.path(), &data);
        let err = import_case_study(&conn, &path).unwrap_err();
        assert!(err.contains("observability must be explicit"), "{err}");

        let mut data = base_data();
        data["claims"][0]["observability_rationale"] = serde_json::json!("");
        let path = write_data(tmp.path(), &data);
        let err = import_case_study(&conn, &path).unwrap_err();
        assert!(err.contains("observability_rationale"), "{err}");
    }

    #[test]
    fn not_directly_visible_claim_is_not_reported_as_bgp_absence() {
        let (_dir, conn) = open_temp_db();
        let tmp = tempfile::tempdir().unwrap();
        let mut data = base_data();
        data["claims"][0]["claim_type"] = serde_json::json!("ReportedMechanism");
        data["claims"][0]["observability"] = serde_json::json!("NotDirectlyVisible");
        data["claims"][0]["observability_rationale"] =
            serde_json::json!("Layer-2 replication itself is not observable in public BGP.");
        let path = write_data(tmp.path(), &data);
        let summary = import_case_study(&conn, &path).unwrap();
        let (obs, rationale): (String, String) = conn
            .query_row(
                "SELECT observability, observability_rationale FROM case_study_claims
                 WHERE case_study_id = ?1",
                [summary.case_study_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(obs, OBSERVABILITY_NOT_DIRECTLY_VISIBLE);
        assert!(rationale.contains("not observable"));
    }
}
