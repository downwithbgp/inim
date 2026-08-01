//! Reviewed target-research application (Session 31, Parts 4-5).
//!
//! The reviewed research record (`case-studies/<slug>/target-research.json`)
//! is the canonical artifact for historical target mappings. Applying it
//! updates ONLY the research fields of matching `case_study_targets` rows
//! (research status, mapped ASNs, candidate predicate, predicate status,
//! notes, provenance, audit timestamp). This is the documented exception to
//! row immutability: research state is review progress, not incident
//! content — phases, claims, documents, and the case-study revision itself
//! are never touched. Applying the same record twice is idempotent.

use rusqlite::Connection;

use super::domain::*;

/// Research-record schema version.
pub const TARGET_RESEARCH_SCHEMA_VERSION: u32 = 1;

/// Apply summary.
#[derive(Debug, Clone, Default)]
pub struct ResearchApplySummary {
    pub case_study_id: i64,
    pub slug: String,
    pub targets_applied: usize,
    pub targets_missing: usize,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct TargetResearchFile {
    pub schema_version: u32,
    pub case_study_slug: String,
    pub reviewed_at: String,
    pub reviewer: String,
    pub method: String,
    #[serde(default)]
    pub path_predicate_note: Option<String>,
    pub targets: Vec<ResearchTarget>,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct ResearchTarget {
    pub source_label: String,
    pub entity_type: String,
    #[serde(default)]
    pub historical_asns: Vec<u32>,
    pub asn_validity_date: Option<String>,
    pub asn_confidence: String,
    pub sources: Vec<ResearchSource>,
    pub reviewed_statement: String,
    pub path_predicate: ResearchPredicate,
    pub bgp_applicability: String,
    pub research_status: String,
    pub historical_validity_status: String,
    pub applicability_status: String,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct ResearchSource {
    pub url: String,
    pub note: String,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct ResearchPredicate {
    pub status: String,
    pub predicate: Option<String>,
    pub note: String,
}

fn valid_status(s: &str) -> bool {
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

/// Validate a parsed research record.
fn validate(data: &TargetResearchFile) -> Result<(), String> {
    let mut problems: Vec<String> = Vec::new();
    if data.schema_version != TARGET_RESEARCH_SCHEMA_VERSION {
        problems.push(format!(
            "schema_version {} not supported (expected {TARGET_RESEARCH_SCHEMA_VERSION})",
            data.schema_version
        ));
    }
    if data.case_study_slug.is_empty() || data.reviewed_at.is_empty() || data.reviewer.is_empty() {
        problems.push("case_study_slug, reviewed_at, reviewer must be non-empty".to_string());
    }
    for (i, t) in data.targets.iter().enumerate() {
        if t.source_label.is_empty() {
            problems.push(format!("targets[{i}] source_label must be non-empty"));
        }
        if !valid_status(&t.research_status) {
            problems.push(format!(
                "targets[{i}] research_status '{0}' invalid",
                t.research_status
            ));
        }
        if !valid_status(&t.historical_validity_status) {
            problems.push(format!(
                "targets[{i}] historical_validity_status '{0}' invalid",
                t.historical_validity_status
            ));
        }
        // Reviewed mappings require a dated validity marker.
        if t.research_status == TARGET_STATUS_HISTORICALLY_REVIEWED
            && (t.historical_asns.is_empty() || t.asn_validity_date.is_none())
        {
            problems.push(format!(
                "targets[{i}] HistoricallyReviewed requires historical_asns and asn_validity_date"
            ));
        }
        if t.sources.is_empty() {
            problems.push(format!("targets[{i}] sources must be non-empty"));
        }
        for (j, s) in t.sources.iter().enumerate() {
            if s.url.is_empty() {
                problems.push(format!("targets[{i}] sources[{j}] url must be non-empty"));
            }
        }
    }
    if problems.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "target-research file is invalid:\n  - {}",
            problems.join("\n  - ")
        ))
    }
}

/// Apply a reviewed target-research record to the catalog.
///
/// Updates only research fields of matching target rows; missing targets are
/// reported (never created — the record must match reviewed data).
pub fn apply_target_research(
    conn: &Connection,
    path: &std::path::Path,
) -> Result<ResearchApplySummary, String> {
    let raw = std::fs::read_to_string(path)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let data: TargetResearchFile = serde_json::from_str(&raw)
        .map_err(|e| format!("invalid target-research file {}: {e}", path.display()))?;
    validate(&data)?;
    let Some(cs) = super::archive_plan::find_case_study(conn, &data.case_study_slug) else {
        return Err(format!(
            "no case study with slug '{}'",
            data.case_study_slug
        ));
    };
    let now = chrono::Utc::now().to_rfc3339();
    let mut summary = ResearchApplySummary {
        case_study_id: cs.id,
        slug: data.case_study_slug.clone(),
        ..Default::default()
    };
    for t in &data.targets {
        let row: Option<i64> = conn
            .query_row(
                "SELECT id FROM case_study_targets
                 WHERE case_study_id = ?1 AND source_label = ?2",
                rusqlite::params![cs.id, t.source_label],
                |r| r.get(0),
            )
            .ok();
        let Some(target_id) = row else {
            summary.targets_missing += 1;
            continue;
        };
        let asns_json = if t.historical_asns.is_empty() {
            None
        } else {
            Some(serde_json::to_string(&t.historical_asns).unwrap())
        };
        let sources_txt = t
            .sources
            .iter()
            .map(|s| format!("{} — {}", s.url, s.note))
            .collect::<Vec<_>>()
            .join(" | ");
        let note = format!(
            "{}; validity: {}; confidence: {}; sources: {}; reviewed {} by {}",
            t.reviewed_statement,
            t.asn_validity_date.as_deref().unwrap_or("n/a"),
            t.asn_confidence,
            sources_txt,
            data.reviewed_at,
            data.reviewer
        );
        conn.execute(
            "UPDATE case_study_targets SET
               research_status = ?1,
               historical_validity_status = ?2,
               candidate_origin_asns_json = ?3,
               candidate_predicate = ?4,
               path_predicate_status = ?5,
               candidate_org_identity = ?6,
               reviewed_note = ?7,
               provenance = ?8,
               research_updated_utc = ?9
             WHERE id = ?10",
            rusqlite::params![
                t.research_status,
                t.historical_validity_status,
                asns_json,
                t.path_predicate.predicate,
                t.path_predicate.status,
                t.entity_type,
                note,
                format!("{} (applied from {})", data.reviewed_at, path.display()),
                now,
                target_id
            ],
        )
        .map_err(|e| format!("catalog write failed: {e}"))?;
        summary.targets_applied += 1;
    }
    Ok(summary)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::db;
    use crate::catalog::store;

    fn open_temp_db() -> (tempfile::TempDir, Connection) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("catalog.sqlite");
        let conn = db::open_catalog(&path).unwrap();
        (dir, conn)
    }

    /// Seed a minimal case study with one Unresearched target via the
    /// reviewed case-study import path.
    fn seed_case_study(conn: &Connection) -> tempfile::TempDir {
        let data = serde_json::json!({
            "schema_version": 1,
            "slug": "incident-x",
            "title": "Incident X",
            "summary": "s",
            "start_utc": "2019-08-21T04:00:00Z",
            "end_utc": "2019-08-21T22:38:00Z",
            "documents": [{
                "title": "AAR",
                "source_url": "https://example.invalid/aar.pdf",
                "doc_type": "AfterActionReport",
                "media_type": "application/pdf",
                "sha256": "d29df26a269962afeb4c671063ea64dec6103e226c039e5939d5af99eedd7114",
                "provenance": "p"
            }],
            "phases": [{
                "label": "p1",
                "start_utc": "2019-08-21T04:00:00Z",
                "end_utc": "2019-08-21T10:00:00Z",
                "start_precision": "exact",
                "end_precision": "summarized",
                "description": "d",
                "source_document": 0,
                "source_page_or_section": "Timeline",
                "review_status": "Reviewed"
            }],
            "related_events": [],
            "claims": [],
            "targets": [{
                "source_label": "Participant A",
                "role_in_report": "participant",
                "historical_validity_status": "Unresearched",
                "research_status": "Unresearched",
                "provenance": "AAR context"
            }]
        });
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("case-study.json");
        std::fs::write(&path, serde_json::to_string_pretty(&data).unwrap()).unwrap();
        crate::catalog::case_study_import::import_case_study(conn, &path).unwrap();
        dir
    }

    fn research_record(status: &str, with_asn: bool) -> serde_json::Value {
        serde_json::json!({
            "schema_version": 1,
            "case_study_slug": "incident-x",
            "reviewed_at": "2026-08-01",
            "reviewer": "test",
            "method": "dated evidence hierarchy",
            "targets": [{
                "source_label": "Participant A",
                "entity_type": "research network",
                "historical_asns": if with_asn { vec![2603] } else { Vec::<u32>::new() },
                "asn_validity_date": if with_asn { Some("2019-08-21") } else { None },
                "asn_confidence": "high",
                "sources": [{"url": "https://example.invalid/evidence", "note": "dated capture"}],
                "reviewed_statement": "operated AS2603 in 2019",
                "path_predicate": {"status": "Candidate", "predicate": "ContainsAny[11537]", "note": "candidate"},
                "bgp_applicability": "potentially visible",
                "research_status": status,
                "historical_validity_status": status,
                "applicability_status": "PotentiallyVisibleInPublicBgp"
            }]
        })
    }

    fn write_record(dir: &std::path::Path, v: &serde_json::Value) -> std::path::PathBuf {
        let p = dir.join("target-research.json");
        std::fs::write(&p, serde_json::to_string_pretty(v).unwrap()).unwrap();
        p
    }

    #[test]
    fn research_apply_updates_only_research_fields() {
        let (_dir, conn) = open_temp_db();
        let cs_dir = seed_case_study(&conn);
        let (content_sha, phase_count): (String, i64) = conn
            .query_row(
                "SELECT content_sha256, (SELECT COUNT(*) FROM case_study_phases)
                 FROM case_studies WHERE slug = 'incident-x'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        let record = research_record("HistoricallyReviewed", true);
        let path = write_record(cs_dir.path(), &record);
        let summary = apply_target_research(&conn, &path).unwrap();
        assert_eq!(summary.targets_applied, 1);
        assert_eq!(summary.targets_missing, 0);
        let (status, asns, predicate, pstatus, updated): (
            String,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
        ) = conn
            .query_row(
                "SELECT research_status, candidate_origin_asns_json, candidate_predicate,
                            path_predicate_status, research_updated_utc
                     FROM case_study_targets WHERE source_label = 'Participant A'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )
            .unwrap();
        assert_eq!(status, "HistoricallyReviewed");
        assert_eq!(asns.as_deref(), Some("[2603]"));
        assert_eq!(predicate.as_deref(), Some("ContainsAny[11537]"));
        assert_eq!(pstatus.as_deref(), Some("Candidate"));
        assert!(updated.is_some());
        // Case-study content untouched.
        let (sha2, phases2): (String, i64) = conn
            .query_row(
                "SELECT content_sha256, (SELECT COUNT(*) FROM case_study_phases)
                 FROM case_studies WHERE slug = 'incident-x'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(sha2, content_sha);
        assert_eq!(phases2, phase_count);
    }

    #[test]
    fn research_apply_is_idempotent() {
        let (_dir, conn) = open_temp_db();
        let cs_dir = seed_case_study(&conn);
        let record = research_record("HistoricallyReviewed", true);
        let path = write_record(cs_dir.path(), &record);
        apply_target_research(&conn, &path).unwrap();
        apply_target_research(&conn, &path).unwrap();
        let (status, updated): (String, Option<String>) = conn
            .query_row(
                "SELECT research_status, research_updated_utc FROM case_study_targets
                 WHERE source_label = 'Participant A'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(status, "HistoricallyReviewed");
        assert!(updated.is_some());
    }

    #[test]
    fn research_apply_rejects_invalid_status() {
        let (_dir, conn) = open_temp_db();
        let cs_dir = seed_case_study(&conn);
        let record = research_record("Guessed", true);
        let path = write_record(cs_dir.path(), &record);
        let err = apply_target_research(&conn, &path).unwrap_err();
        assert!(err.contains("invalid"), "{err}");
    }

    #[test]
    fn historically_reviewed_requires_asn_and_date() {
        let (_dir, conn) = open_temp_db();
        let cs_dir = seed_case_study(&conn);
        let record = research_record("HistoricallyReviewed", false);
        let path = write_record(cs_dir.path(), &record);
        let err = apply_target_research(&conn, &path).unwrap_err();
        assert!(err.contains("requires historical_asns"), "{err}");
    }

    #[test]
    fn missing_target_is_reported_not_created() {
        let (_dir, conn) = open_temp_db();
        let cs_dir = seed_case_study(&conn);
        let mut record = research_record("NotApplicableToPublicBgp", false);
        record["targets"][0]["source_label"] = serde_json::json!("Participant Z");
        let path = write_record(cs_dir.path(), &record);
        let summary = apply_target_research(&conn, &path).unwrap();
        assert_eq!(summary.targets_applied, 0);
        assert_eq!(summary.targets_missing, 1);
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM case_study_targets WHERE source_label = 'Participant Z'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 0, "missing targets must not be created");
    }

    #[test]
    fn ambiguous_service_identity_is_a_valid_status() {
        let (_dir, conn) = open_temp_db();
        let cs_dir = seed_case_study(&conn);
        let record = research_record("AmbiguousServiceIdentity", false);
        let path = write_record(cs_dir.path(), &record);
        let summary = apply_target_research(&conn, &path).unwrap();
        assert_eq!(summary.targets_applied, 1);
    }
}
