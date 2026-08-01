//! Historical-archive planning for a case study (Session 30, Part 8).
//!
//! The planner computes a reproducible initial analysis horizon and the
//! expected 2019 RouteViews archive files for it — **without downloading
//! anything**. Archive volume and execution cost are estimated before any
//! run; targets whose historical mappings are not reviewed are reported as
//! blocked with reasons. The stored plan stays `Draft` until reviewed.

use chrono::Datelike;
use rusqlite::Connection;

use super::domain::{CaseStudy, CaseStudyAnalysisPlan, CaseStudyTarget};
use super::store;

/// Default warmup before the incident window.
pub const DEFAULT_WARMUP_HOURS: i64 = 2;
/// Default cooldown after the incident window.
pub const DEFAULT_COOLDOWN_HOURS: i64 = 2;

/// RouteViews collector candidates that existed in 2019.
pub const COLLECTOR_CANDIDATES_2019: &[&str] = &["route-views2", "route-views6"];

/// 2019-era RIB interval (seconds) and estimated sizes.
const RIB_INTERVAL_SECS: i64 = 2 * 3600;
const UPDATE_INTERVAL_SECS: i64 = 5 * 60;
const RIB_ESTIMATED_BYTES: i64 = 75_000_000;
const UPDATE_ESTIMATED_BYTES: i64 = 3_000_000;

/// Reviewed analysis horizon.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AnalysisHorizon {
    pub warmup_start_utc: String,
    pub incident_start_utc: String,
    pub incident_end_utc: String,
    pub cooldown_end_utc: String,
    pub warmup_hours: i64,
    pub cooldown_hours: i64,
    /// Recorded for the plan; the final window must be reviewed.
    pub review_required: bool,
}

/// An expected archive file (URL + estimated size).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExpectedFile {
    pub url: String,
    /// Estimated size in bytes; `size_estimated` marks it as an estimate.
    pub size_estimated_bytes: Option<i64>,
    pub size_estimated: bool,
}

/// Per-collector archive expectations.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CollectorPlan {
    pub collector: String,
    /// Availability in 2019: candidate until verified at execution time.
    pub availability: String,
    pub ribs: Vec<ExpectedFile>,
    pub updates: Vec<ExpectedFile>,
}

/// A target excluded from execution and why.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BlockedTarget {
    pub source_label: String,
    pub reason: String,
}

/// The complete archive plan.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ArchivePlan {
    pub collectors: Vec<CollectorPlan>,
    pub blocked_targets: Vec<BlockedTarget>,
    pub skipped_targets: Vec<BlockedTarget>,
    pub estimated_total_bytes: i64,
    pub estimated_total_is_estimate: bool,
    pub notes: Vec<String>,
}

fn parse_utc(s: &str) -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&chrono::Utc))
        .unwrap_or_else(|_| panic!("invalid UTC timestamp in plan: {s}"))
}

fn floor_to(t: chrono::DateTime<chrono::Utc>, interval: i64) -> chrono::DateTime<chrono::Utc> {
    let secs = t.timestamp();
    chrono::DateTime::from_timestamp(secs - secs.rem_euclid(interval), 0).unwrap()
}

fn routeviews_url(collector: &str, year: i32, month: u32, kind: &str, stamp: &str) -> String {
    let base = if collector == "route-views2" {
        "http://archive.routeviews.org/bgpdata".to_string()
    } else {
        format!("http://archive.routeviews.org/{collector}/bgpdata")
    };
    let file_prefix = match kind {
        "RIBS" => "rib",
        "UPDATES" => "updates",
        _ => kind,
    };
    format!("{base}/{year:04}.{month:02}/{kind}/{file_prefix}.{stamp}.bz2")
}

fn rib_stamp(t: chrono::DateTime<chrono::Utc>) -> String {
    t.format("%Y%m%d.%H00").to_string()
}

fn update_stamp(t: chrono::DateTime<chrono::Utc>) -> String {
    t.format("%Y%m%d.%H%M").to_string()
}

/// Build the analysis horizon and expected archive plan for a case study.
///
/// Pure computation — never performs network I/O or downloads.
pub fn build_plan(
    cs: &CaseStudy,
    targets: &[CaseStudyTarget],
    warmup_hours: i64,
    cooldown_hours: i64,
) -> Result<(AnalysisHorizon, ArchivePlan), String> {
    let incident_start = cs.start_utc.as_deref().map(parse_utc).ok_or_else(|| {
        "case study has no start time; cannot plan an analysis window".to_string()
    })?;
    let incident_end =
        cs.end_utc.as_deref().map(parse_utc).ok_or_else(|| {
            "case study has no end time; cannot plan an analysis window".to_string()
        })?;
    if incident_end <= incident_start {
        return Err("case study end must be after start".to_string());
    }
    let warmup_start = incident_start - chrono::Duration::hours(warmup_hours);
    let cooldown_end = incident_end + chrono::Duration::hours(cooldown_hours);

    let horizon = AnalysisHorizon {
        warmup_start_utc: warmup_start.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        incident_start_utc: incident_start.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        incident_end_utc: incident_end.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        cooldown_end_utc: cooldown_end.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        warmup_hours,
        cooldown_hours,
        review_required: true,
    };

    let year = incident_start.year();
    let month = incident_start.month();
    let mut collectors = Vec::new();
    let mut estimated_total_bytes: i64 = 0;
    for collector in COLLECTOR_CANDIDATES_2019 {
        let rib_start = floor_to(warmup_start, RIB_INTERVAL_SECS);
        let rib_end = floor_to(cooldown_end, RIB_INTERVAL_SECS);
        let mut ribs = Vec::new();
        let mut t = rib_start;
        while t <= rib_end {
            ribs.push(ExpectedFile {
                url: routeviews_url(collector, year, month, "RIBS", &rib_stamp(t)),
                size_estimated_bytes: Some(RIB_ESTIMATED_BYTES),
                size_estimated: true,
            });
            estimated_total_bytes += RIB_ESTIMATED_BYTES;
            t += chrono::Duration::seconds(RIB_INTERVAL_SECS);
        }
        let upd_start = floor_to(warmup_start, UPDATE_INTERVAL_SECS);
        let upd_end = floor_to(cooldown_end, UPDATE_INTERVAL_SECS);
        let mut updates = Vec::new();
        let mut u = upd_start;
        while u <= upd_end {
            updates.push(ExpectedFile {
                url: routeviews_url(collector, year, month, "UPDATES", &update_stamp(u)),
                size_estimated_bytes: Some(UPDATE_ESTIMATED_BYTES),
                size_estimated: true,
            });
            estimated_total_bytes += UPDATE_ESTIMATED_BYTES;
            u += chrono::Duration::seconds(UPDATE_INTERVAL_SECS);
        }
        collectors.push(CollectorPlan {
            collector: (*collector).to_string(),
            availability: "candidate-2019 (verify at execution)".to_string(),
            ribs,
            updates,
        });
    }

    // Target coverage: only historically reviewed mappings may enter the
    // analysis; everything else is blocked with the reason recorded.
    let mut blocked_targets = Vec::new();
    let mut skipped_targets = Vec::new();
    for t in targets {
        match t.research_status.as_str() {
            crate::catalog::domain::TARGET_STATUS_HISTORICALLY_REVIEWED => {}
            crate::catalog::domain::TARGET_STATUS_NOT_APPLICABLE => {
                skipped_targets.push(BlockedTarget {
                    source_label: t.source_label.clone(),
                    reason: "not applicable to public BGP (reviewed)".to_string(),
                });
            }
            other => blocked_targets.push(BlockedTarget {
                source_label: t.source_label.clone(),
                reason: format!("target mapping unresolved (research status: {other})"),
            }),
        }
    }

    let mut notes = vec![
        "initial reproducible horizon; final window requires review".to_string(),
        "sizes are estimates; exact sizes recorded at acquisition".to_string(),
        "2019 collector and peer availability verified at execution time".to_string(),
        "no archives were downloaded to produce this plan".to_string(),
    ];
    if blocked_targets.is_empty() && skipped_targets.is_empty() {
        notes.push("no analysis targets are blocked".to_string());
    }

    Ok((
        horizon,
        ArchivePlan {
            collectors,
            blocked_targets,
            skipped_targets,
            estimated_total_bytes,
            estimated_total_is_estimate: true,
            notes,
        },
    ))
}

/// Persist a plan for a case study (one plan per case study, Draft status).
pub fn save_plan(
    conn: &Connection,
    case_study_id: i64,
    horizon: &AnalysisHorizon,
    plan: &ArchivePlan,
) -> Result<i64, String> {
    let record = CaseStudyAnalysisPlan {
        id: 0,
        case_study_id,
        horizon_json: serde_json::to_string(horizon).unwrap(),
        plan_json: serde_json::to_string(plan).unwrap(),
        status: crate::catalog::domain::PLAN_STATUS_DRAFT.to_string(),
        created_utc: chrono::Utc::now().to_rfc3339(),
    };
    store::upsert_case_study_analysis_plan(conn, &record)
}

/// Load the stored plan for a case study, if any.
pub fn load_plan(conn: &Connection, case_study_id: i64) -> Option<CaseStudyAnalysisPlan> {
    conn.query_row(
        "SELECT id, case_study_id, horizon_json, plan_json, status, created_utc
         FROM case_study_analysis_plans WHERE case_study_id = ?1",
        [case_study_id],
        |r| {
            Ok(CaseStudyAnalysisPlan {
                id: r.get(0)?,
                case_study_id: r.get(1)?,
                horizon_json: r.get(2)?,
                plan_json: r.get(3)?,
                status: r.get(4)?,
                created_utc: r.get(5)?,
            })
        },
    )
    .ok()
}

/// Case-study target rows in deterministic order.
pub fn list_targets(conn: &Connection, case_study_id: i64) -> Result<Vec<CaseStudyTarget>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, case_study_id, source_label, role_in_report, candidate_org_identity,
                    candidate_origin_asns_json, candidate_predicate, historical_validity_status,
                    provenance, research_status, reviewed_note, sort_order
             FROM case_study_targets WHERE case_study_id = ?1 ORDER BY sort_order",
        )
        .map_err(|e| format!("catalog read failed: {e}"))?;
    let rows = stmt
        .query_map([case_study_id], |r| {
            Ok(CaseStudyTarget {
                id: r.get(0)?,
                case_study_id: r.get(1)?,
                source_label: r.get(2)?,
                role_in_report: r.get(3)?,
                candidate_org_identity: r.get(4)?,
                candidate_origin_asns_json: r.get(5)?,
                candidate_predicate: r.get(6)?,
                historical_validity_status: r.get(7)?,
                provenance: r.get(8)?,
                research_status: r.get(9)?,
                reviewed_note: r.get(10)?,
                sort_order: r.get(11)?,
            })
        })
        .map_err(|e| format!("catalog read failed: {e}"))?;
    rows.collect::<Result<_, _>>()
        .map_err(|e| format!("catalog read failed: {e}"))
}

/// Look up a case study by slug.
pub fn find_case_study(conn: &Connection, slug: &str) -> Option<CaseStudy> {
    conn.query_row(
        "SELECT id, slug, title, summary, start_utc, end_utc, status, content_sha256,
                created_utc, updated_utc
         FROM case_studies WHERE slug = ?1",
        [slug],
        |r| {
            Ok(CaseStudy {
                id: r.get(0)?,
                slug: r.get(1)?,
                title: r.get(2)?,
                summary: r.get(3)?,
                start_utc: r.get(4)?,
                end_utc: r.get(5)?,
                status: r.get(6)?,
                content_sha256: r.get(7)?,
                created_utc: r.get(8)?,
                updated_utc: r.get(9)?,
            })
        },
    )
    .ok()
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

    fn sample_case_study() -> CaseStudy {
        CaseStudy {
            id: 1,
            slug: "incident-x".to_string(),
            title: "Incident X".to_string(),
            summary: "s".to_string(),
            start_utc: Some("2019-08-21T04:00:00Z".to_string()),
            end_utc: Some("2019-08-21T22:38:00Z".to_string()),
            status: "Active".to_string(),
            content_sha256: "abc".to_string(),
            created_utc: "2019-09-01T00:00:00Z".to_string(),
            updated_utc: "2019-09-01T00:00:00Z".to_string(),
        }
    }

    fn target(label: &str, status: &str) -> CaseStudyTarget {
        CaseStudyTarget {
            id: 0,
            case_study_id: 1,
            source_label: label.to_string(),
            role_in_report: "participant".to_string(),
            candidate_org_identity: None,
            candidate_origin_asns_json: None,
            candidate_predicate: None,
            historical_validity_status: status.to_string(),
            provenance: Some("AAR".to_string()),
            research_status: status.to_string(),
            reviewed_note: None,
            sort_order: 0,
        }
    }

    #[test]
    fn plan_computes_expected_files_without_network() {
        let cs = sample_case_study();
        let (horizon, plan) =
            build_plan(&cs, &[], DEFAULT_WARMUP_HOURS, DEFAULT_COOLDOWN_HOURS).unwrap();
        assert_eq!(horizon.warmup_start_utc, "2019-08-21T02:00:00Z");
        assert_eq!(horizon.cooldown_end_utc, "2019-08-22T00:38:00Z");
        assert!(horizon.review_required);
        // Two collectors, each with a full RIB + update series.
        assert_eq!(plan.collectors.len(), 2);
        for c in &plan.collectors {
            assert!(!c.ribs.is_empty());
            assert!(!c.updates.is_empty());
            assert!(
                c.ribs[0].url.contains("2019.08/RIBS/rib.20190821.0200.bz2"),
                "{}",
                c.ribs[0].url
            );
            assert!(c.ribs.iter().all(|f| f.size_estimated));
            assert!(
                c.updates[0]
                    .url
                    .contains("UPDATES/updates.20190821.0200.bz2"),
                "{}",
                c.updates[0].url
            );
        }
        assert!(plan.estimated_total_bytes > 0);
        assert!(plan
            .notes
            .iter()
            .any(|n| n.contains("no archives were downloaded")));
    }

    #[test]
    fn unresearched_target_is_blocked_with_reason() {
        let cs = sample_case_study();
        let targets = vec![
            target(
                "Participant A",
                crate::catalog::domain::TARGET_STATUS_UNRESEARCHED,
            ),
            target(
                "Participant B",
                crate::catalog::domain::TARGET_STATUS_HISTORICALLY_REVIEWED,
            ),
            target(
                "Layer-2 only",
                crate::catalog::domain::TARGET_STATUS_NOT_APPLICABLE,
            ),
        ];
        let (_, plan) = build_plan(&cs, &targets, 2, 2).unwrap();
        assert_eq!(plan.blocked_targets.len(), 1);
        assert_eq!(plan.blocked_targets[0].source_label, "Participant A");
        assert!(plan.blocked_targets[0].reason.contains("unresolved"));
        assert_eq!(plan.skipped_targets.len(), 1);
        assert_eq!(plan.skipped_targets[0].source_label, "Layer-2 only");
    }

    #[test]
    fn plan_without_start_or_end_is_rejected() {
        let mut cs = sample_case_study();
        cs.start_utc = None;
        let err = build_plan(&cs, &[], 2, 2).unwrap_err();
        assert!(err.contains("no start time"), "{err}");
    }

    #[test]
    fn plan_is_draft_until_reviewed() {
        let (_dir, conn) = open_temp_db();
        let cs = sample_case_study();
        store::insert_case_study(&conn, &cs).unwrap();
        let (horizon, plan) = build_plan(&cs, &[], 2, 2).unwrap();
        save_plan(&conn, cs.id, &horizon, &plan).unwrap();
        let stored = load_plan(&conn, cs.id).unwrap();
        assert_eq!(stored.status, crate::catalog::domain::PLAN_STATUS_DRAFT);
        let p: ArchivePlan = serde_json::from_str(&stored.plan_json).unwrap();
        assert_eq!(p.collectors.len(), 2);
    }
}
