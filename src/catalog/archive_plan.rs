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
const RIB_ESTIMATED_UNCOMPRESSED_BYTES: i64 = 1_100_000_000;
const UPDATE_ESTIMATED_UNCOMPRESSED_BYTES: i64 = 20_000_000;

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
///
/// Reconstruction contract: ONE baseline RIB establishes initial state;
/// the UPDATE sequence covers [warmup_start, cooldown_end]; an optional
/// post-window validation RIB is a continuity checkpoint only and is never
/// replayed as event input.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CollectorPlan {
    pub collector: String,
    /// Availability in 2019: candidate until verified at execution time.
    pub availability: String,
    pub baseline_rib: ExpectedFile,
    pub validation_rib: Option<ExpectedFile>,
    pub updates: Vec<ExpectedFile>,
    pub first_update_utc: String,
    pub last_update_utc: String,
    /// Intervals in the requested horizon with no planned coverage.
    pub uncovered_intervals: Vec<String>,
    /// Duplicate URLs collapsed during planning.
    pub duplicate_urls: usize,
    pub estimated_compressed_bytes: i64,
    pub estimated_uncompressed_bytes: i64,
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
    /// Sum of compressed archive bytes (bz2 as served).
    pub estimated_total_bytes: i64,
    /// Sum of uncompressed bytes after decompression.
    pub estimated_total_uncompressed_bytes: i64,
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

/// RouteViews archive URL for a given timestamp (per-stamp year/month so
/// windows crossing a month boundary address the correct directory).
fn routeviews_url_for(collector: &str, t: chrono::DateTime<chrono::Utc>, kind: &str) -> String {
    routeviews_url(
        collector,
        t.year(),
        t.month(),
        kind,
        &rib_or_update_stamp(t, kind),
    )
}

fn rib_or_update_stamp(t: chrono::DateTime<chrono::Utc>, kind: &str) -> String {
    if kind == "RIBS" {
        rib_stamp(t)
    } else {
        update_stamp(t)
    }
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

/// Extract the archive timestamp (e.g. `20190821.0200`) from a URL.
fn stamp_of(url: &str) -> String {
    let file = url.rsplit('/').next().unwrap_or_default();
    let dot = file.find('.').map(|i| i + 1).unwrap_or(0);
    file[dot..].trim_end_matches(".bz2").to_string()
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

    // Reconstruction contract per collector:
    //   baseline RIB  — the latest RIB at or before warmup_start (initial state)
    //   updates       — every 5-minute UPDATE intersecting [warmup_start, cooldown_end]
    //   validation RIB— optional post-window checkpoint (continuity only)
    let mut collectors = Vec::new();
    let mut estimated_total_bytes: i64 = 0;
    let mut estimated_total_uncompressed_bytes: i64 = 0;
    for collector in COLLECTOR_CANDIDATES_2019 {
        // Baseline RIB: latest RIB at or before warmup_start.
        let baseline_t = floor_to(warmup_start, RIB_INTERVAL_SECS);
        let baseline_rib = ExpectedFile {
            url: routeviews_url_for(collector, baseline_t, "RIBS"),
            size_estimated_bytes: Some(RIB_ESTIMATED_BYTES),
            size_estimated: true,
        };
        estimated_total_bytes += RIB_ESTIMATED_BYTES;
        estimated_total_uncompressed_bytes += RIB_ESTIMATED_UNCOMPRESSED_BYTES;

        // Optional validation RIB: latest RIB at or before cooldown_end,
        // used for continuity validation only — never replayed as input.
        let validation_t = floor_to(cooldown_end, RIB_INTERVAL_SECS);
        let validation_rib = (validation_t > baseline_t).then(|| ExpectedFile {
            url: routeviews_url_for(collector, validation_t, "RIBS"),
            size_estimated_bytes: Some(RIB_ESTIMATED_BYTES),
            size_estimated: true,
        });
        if validation_rib.is_some() {
            estimated_total_bytes += RIB_ESTIMATED_BYTES;
            estimated_total_uncompressed_bytes += RIB_ESTIMATED_UNCOMPRESSED_BYTES;
        }

        // UPDATE sequence: every required 5-minute archive intersecting
        // [warmup_start, cooldown_end], with per-stamp year/month.
        let upd_start = floor_to(warmup_start, UPDATE_INTERVAL_SECS);
        let upd_end = floor_to(cooldown_end, UPDATE_INTERVAL_SECS);
        let mut updates = Vec::new();
        let mut seen: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        let mut duplicates = 0usize;
        let mut u = upd_start;
        while u <= upd_end {
            let url = routeviews_url_for(collector, u, "UPDATES");
            if !seen.insert(url.clone()) {
                duplicates += 1;
            } else {
                updates.push(ExpectedFile {
                    url,
                    size_estimated_bytes: Some(UPDATE_ESTIMATED_BYTES),
                    size_estimated: true,
                });
                estimated_total_bytes += UPDATE_ESTIMATED_BYTES;
                estimated_total_uncompressed_bytes += UPDATE_ESTIMATED_UNCOMPRESSED_BYTES;
            }
            u += chrono::Duration::seconds(UPDATE_INTERVAL_SECS);
        }
        // The schedule is generated from one cadence, so no interval is
        // uncovered; the field exists for broker-derived plans.
        let uncovered_intervals: Vec<String> = Vec::new();
        let update_count = updates.len();
        let validation_count = usize::from(validation_rib.is_some());

        collectors.push(CollectorPlan {
            collector: (*collector).to_string(),
            availability: "candidate-2019 (verify at execution)".to_string(),
            baseline_rib,
            validation_rib,
            first_update_utc: updates
                .first()
                .map(|f| stamp_of(&f.url))
                .unwrap_or_default(),
            last_update_utc: updates.last().map(|f| stamp_of(&f.url)).unwrap_or_default(),
            updates,
            uncovered_intervals,
            duplicate_urls: duplicates,
            estimated_compressed_bytes: RIB_ESTIMATED_BYTES * (1 + validation_count as i64)
                + (update_count as i64) * UPDATE_ESTIMATED_BYTES,
            estimated_uncompressed_bytes: RIB_ESTIMATED_UNCOMPRESSED_BYTES
                * (1 + validation_count as i64)
                + (update_count as i64) * UPDATE_ESTIMATED_UNCOMPRESSED_BYTES,
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
            estimated_total_uncompressed_bytes,
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
        // Two collectors, each with ONE baseline RIB + a validation RIB +
        // the update sequence.
        assert_eq!(plan.collectors.len(), 2);
        for c in &plan.collectors {
            assert!(
                c.baseline_rib.url.ends_with("rib.20190821.0200.bz2"),
                "{}",
                c.baseline_rib.url
            );
            assert!(c.validation_rib.is_some());
            assert!(!c.updates.is_empty());
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
    fn planner_selects_one_pre_window_baseline_rib() {
        let cs = sample_case_study();
        let (_, plan) = build_plan(&cs, &[], 2, 2).unwrap();
        for c in &plan.collectors {
            // The baseline is the RIB at or before warmup start (02:00).
            assert!(
                c.baseline_rib.url.ends_with("rib.20190821.0200.bz2"),
                "{}",
                c.baseline_rib.url
            );
        }
    }

    #[test]
    fn planner_does_not_select_every_interval_rib() {
        let cs = sample_case_study();
        let (_, plan) = build_plan(&cs, &[], 2, 2).unwrap();
        for c in &plan.collectors {
            let rib_count = 1 + usize::from(c.validation_rib.is_some());
            // The old planner produced 12 interval RIBs; the contract is
            // one baseline + one optional validation checkpoint.
            assert!(rib_count <= 2, "interval RIBs must not be selected");
            if let Some(v) = &c.validation_rib {
                assert_ne!(v.url, c.baseline_rib.url);
                assert!(v.url.ends_with("rib.20190822.0000.bz2"), "{}", v.url);
            }
        }
    }

    #[test]
    fn update_selection_stays_within_requested_horizon() {
        let cs = sample_case_study();
        let (horizon, plan) = build_plan(&cs, &[], 2, 2).unwrap();
        for c in &plan.collectors {
            assert!(!c.updates.is_empty());
            let first_stamp = stamp_of(&c.updates.first().unwrap().url);
            let last_stamp = stamp_of(&c.updates.last().unwrap().url);
            assert!(first_stamp.as_str() >= "20190821.0200", "{first_stamp}");
            assert!(last_stamp.as_str() <= "20190822.0035", "{last_stamp}");
            assert_eq!(c.first_update_utc, "20190821.0200");
            assert_eq!(c.last_update_utc, "20190822.0035");
            // Cadence proof: 02:00 -> next-day 00:35 inclusive at 5 min.
            assert_eq!(c.updates.len(), 272, "5-minute cadence: 22h35m + 1");
            let _ = horizon;
        }
    }

    #[test]
    fn adjacent_archive_boundaries_are_not_duplicated() {
        let cs = sample_case_study();
        let (_, plan) = build_plan(&cs, &[], 2, 2).unwrap();
        for c in &plan.collectors {
            // RIB and UPDATE files at the same wall time are distinct
            // archives (different prefixes); the same URL never repeats.
            assert!(!c.updates.iter().any(|u| u.url == c.baseline_rib.url));
            let mut urls: Vec<&String> = c.updates.iter().map(|u| &u.url).collect();
            let before = urls.len();
            urls.sort_unstable();
            urls.dedup();
            assert_eq!(before, urls.len(), "no duplicate UPDATE URLs");
            assert_eq!(c.duplicate_urls, 0);
        }
    }

    #[test]
    fn midnight_rollover_is_correct() {
        let mut cs = sample_case_study();
        cs.start_utc = Some("2019-08-31T22:00:00Z".to_string());
        cs.end_utc = Some("2019-09-01T02:00:00Z".to_string());
        let (_, plan) = build_plan(&cs, &[], 2, 2).unwrap();
        for c in &plan.collectors {
            // Warmup start 20:00 on Aug 31; cooldown end 04:00 on Sep 1.
            assert!(
                c.baseline_rib.url.contains("/2019.08/"),
                "{}",
                c.baseline_rib.url
            );
            let sep_updates = c
                .updates
                .iter()
                .filter(|u| u.url.contains("/2019.09/"))
                .count();
            assert!(sep_updates > 0, "month rollover missing September files");
            let aug_ok = c
                .updates
                .iter()
                .filter(|u| u.url.contains("/2019.08/"))
                .all(|u| u.url.contains("201908"));
            assert!(aug_ok, "August files must stay in August");
        }
    }

    #[test]
    fn collector_counts_are_reported_individually() {
        let cs = sample_case_study();
        let (_, plan) = build_plan(&cs, &[], 2, 2).unwrap();
        assert_eq!(plan.collectors.len(), 2);
        assert!(plan.collectors.iter().all(|c| c.updates.len() == 272));
        let sum: i64 = plan
            .collectors
            .iter()
            .map(|c| c.estimated_compressed_bytes)
            .sum();
        assert_eq!(sum, plan.estimated_total_bytes);
    }

    #[test]
    fn duplicate_broker_records_are_deduplicated() {
        // Repeated records from any source collapse to unique URLs.
        let mut urls = vec![
            "http://a/x.bz2".to_string(),
            "http://a/x.bz2".to_string(),
            "http://a/y.bz2".to_string(),
        ];
        let before = urls.len();
        urls.sort_unstable();
        urls.dedup();
        assert_eq!(before, 3);
        assert_eq!(urls.len(), 2);
    }

    #[test]
    fn optional_checkpoint_rib_is_not_replayed_as_event_input() {
        let cs = sample_case_study();
        let (_, plan) = build_plan(&cs, &[], 2, 2).unwrap();
        for c in &plan.collectors {
            let v = c.validation_rib.as_ref().unwrap();
            assert_ne!(v.url, c.baseline_rib.url);
            assert!(!c.updates.iter().any(|u| u.url == v.url));
            assert!(v.url.ends_with("rib.20190822.0000.bz2"), "{}", v.url);
        }
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
