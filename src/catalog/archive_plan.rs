//! Historical-archive planning for a case study.
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

fn default_family() -> String {
    SourceFamily::RouteViews.as_str().to_string()
}

/// RIPE RIS collector candidates (route collectors) that existed in 2019.
pub const RIS_COLLECTOR_CANDIDATES_2019: &[&str] = &["rrc00", "rrc01"];

/// BGP archive source family.
///
/// RouteViews and RIPE RIS are distinct observer sources: distinct
/// archive bases, file conventions, RIB cadences, and compression. A
/// collector identifier is only meaningful together with its family
/// (`rrc00` exists only in RIPE RIS; `route-views2` only in RouteViews).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SourceFamily {
    RouteViews,
    RipeRis,
}

impl SourceFamily {
    /// Stable string form.
    pub fn as_str(&self) -> &'static str {
        match self {
            SourceFamily::RouteViews => "RouteViews",
            SourceFamily::RipeRis => "RipeRis",
        }
    }

    /// Human label.
    pub fn label(&self) -> &'static str {
        match self {
            SourceFamily::RouteViews => "RouteViews",
            SourceFamily::RipeRis => "RIPE RIS",
        }
    }

    /// bgpkit-broker project name for discovery.
    pub fn broker_project(&self) -> &'static str {
        match self {
            SourceFamily::RouteViews => "routeviews",
            SourceFamily::RipeRis => "riperis",
        }
    }

    /// Parse a manifest/plan family string (tolerant: accepts the
    /// stable `as_str()` forms and the broker project names).
    pub fn parse_family(s: &str) -> Option<SourceFamily> {
        match s {
            "RouteViews" | "routeviews" => Some(SourceFamily::RouteViews),
            "RipeRis" | "riperis" | "RIPE RIS" => Some(SourceFamily::RipeRis),
            _ => None,
        }
    }
}

/// 2019-era RouteViews RIB interval (seconds) and estimated sizes.
const RIB_INTERVAL_SECS: i64 = 2 * 3600;
/// RIPE RIS bview interval: every 8 hours on the 00/08/16 grid.
const RIS_BVIEW_INTERVAL_SECS: i64 = 8 * 3600;
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
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CollectorPlan {
    pub collector: String,
    /// Source family this collector belongs to (RouteViews | RipeRis).
    /// Defaults to RouteViews so plans stored before RIPE RIS support parse.
    #[serde(default = "default_family")]
    pub source_family: String,
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
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct BlockedTarget {
    pub source_label: String,
    pub reason: String,
}

/// Pilot-analysis state (reviewed data, Part 10).
///
/// A pilot is ONE target, ONE collector, ONE bounded window — never a
/// whole-incident conclusion.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PilotRecord {
    pub status: String,
    pub target: String,
    pub collector: String,
    pub window_start_utc: String,
    pub window_end_utc: String,
    pub run_id: Option<i64>,
    pub baseline_streams: usize,
    pub operator_evidence: String,
    pub bgp_observation: String,
    pub temporal_relationship: String,
    pub interpretation: String,
    pub limitation: String,
    pub finding: String,
}

/// The complete archive plan.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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
    /// Reviewed pilot state (none when not yet planned).
    #[serde(default)]
    pub pilot: Option<PilotRecord>,
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
    file[dot..]
        .trim_end_matches(".bz2")
        .trim_end_matches(".gz")
        .to_string()
}

/// Family-aware archive URL for a timestamp.
pub(crate) fn archive_url_for(
    family: SourceFamily,
    collector: &str,
    t: chrono::DateTime<chrono::Utc>,
    kind: &str,
) -> String {
    match family {
        SourceFamily::RouteViews => routeviews_url_for(collector, t, kind),
        SourceFamily::RipeRis => ris_url_for(collector, t, kind),
    }
}

/// RIPE RIS archive URL: `https://data.ris.ripe.net/{collector}/{YYYY.MM}/`
/// with `bview.{stamp}.gz` (8-hourly RIB snapshots) and
/// `updates.{stamp}.gz` (5-minute UPDATE archives).
fn ris_url_for(collector: &str, t: chrono::DateTime<chrono::Utc>, kind: &str) -> String {
    let stamp = rib_or_update_stamp(t, kind);
    let file = if kind == "RIBS" {
        format!("bview.{stamp}.gz")
    } else {
        format!("updates.{stamp}.gz")
    };
    format!(
        "https://data.ris.ripe.net/{collector}/{year:04}.{month:02}/{file}",
        year = t.year(),
        month = t.month()
    )
}

/// RIB snapshot cadence per family (RouteViews: 2h; RIPE RIS bview: 8h).
fn rib_interval_secs(family: SourceFamily) -> i64 {
    match family {
        SourceFamily::RouteViews => RIB_INTERVAL_SECS,
        SourceFamily::RipeRis => RIS_BVIEW_INTERVAL_SECS,
    }
}

/// Collectors to plan for a family (2019-era candidates).
fn family_collectors(family: SourceFamily) -> &'static [&'static str] {
    match family {
        SourceFamily::RouteViews => COLLECTOR_CANDIDATES_2019,
        SourceFamily::RipeRis => RIS_COLLECTOR_CANDIDATES_2019,
    }
}

/// Build the analysis horizon and expected archive plan for a case study
/// over RouteViews collectors.
///
/// Pure computation — never performs network I/O or downloads.
pub fn build_plan(
    cs: &CaseStudy,
    targets: &[CaseStudyTarget],
    warmup_hours: i64,
    cooldown_hours: i64,
) -> Result<(AnalysisHorizon, ArchivePlan), String> {
    build_plan_for_families(
        cs,
        targets,
        warmup_hours,
        cooldown_hours,
        &[SourceFamily::RouteViews],
    )
}

/// Family-aware plan builder. The horizon is shared; each family's
/// collectors get family-correct URLs, RIB cadence, and compression.
/// Collector identity is (family, collector) — never a bare id.
pub fn build_plan_for_families(
    cs: &CaseStudy,
    targets: &[CaseStudyTarget],
    warmup_hours: i64,
    cooldown_hours: i64,
    families: &[SourceFamily],
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
    for family in families {
        for collector in family_collectors(*family) {
            let rib_interval = rib_interval_secs(*family);
            // Baseline RIB: latest RIB at or before warmup_start.
            let baseline_t = floor_to(warmup_start, rib_interval);
            let baseline_rib = ExpectedFile {
                url: archive_url_for(*family, collector, baseline_t, "RIBS"),
                size_estimated_bytes: Some(RIB_ESTIMATED_BYTES),
                size_estimated: true,
            };
            estimated_total_bytes += RIB_ESTIMATED_BYTES;
            estimated_total_uncompressed_bytes += RIB_ESTIMATED_UNCOMPRESSED_BYTES;

            // Optional validation RIB: latest RIB at or before
            // cooldown_end, used for continuity validation only — never
            // replayed as input.
            let validation_t = floor_to(cooldown_end, rib_interval);
            let validation_rib = (validation_t > baseline_t).then(|| ExpectedFile {
                url: archive_url_for(*family, collector, validation_t, "RIBS"),
                size_estimated_bytes: Some(RIB_ESTIMATED_BYTES),
                size_estimated: true,
            });
            if validation_rib.is_some() {
                estimated_total_bytes += RIB_ESTIMATED_BYTES;
                estimated_total_uncompressed_bytes += RIB_ESTIMATED_UNCOMPRESSED_BYTES;
            }

            // UPDATE sequence: every required 5-minute archive
            // intersecting [warmup_start, cooldown_end], per-stamp
            // year/month.
            let upd_start = floor_to(warmup_start, UPDATE_INTERVAL_SECS);
            let upd_end = floor_to(cooldown_end, UPDATE_INTERVAL_SECS);
            let mut updates = Vec::new();
            let mut seen: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
            let mut duplicates = 0usize;
            let mut u = upd_start;
            while u <= upd_end {
                let url = archive_url_for(*family, collector, u, "UPDATES");
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
            // The schedule is generated from one cadence, so no interval
            // is uncovered; the field exists for broker-derived plans.
            let uncovered_intervals: Vec<String> = Vec::new();
            let update_count = updates.len();
            let validation_count = usize::from(validation_rib.is_some());

            collectors.push(CollectorPlan {
                collector: (*collector).to_string(),
                source_family: family.as_str().to_string(),
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
            pilot: None,
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

/// Reviewed pilot-result record file (pilot-result.json).
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct PilotResultFile {
    pub schema_version: u32,
    pub case_study_slug: String,
    pub reviewed_at: String,
    pub status: String,
    pub target: String,
    pub collector: String,
    pub window_start_utc: String,
    pub window_end_utc: String,
    pub run_id: Option<i64>,
    pub baseline_streams: usize,
    pub operator_evidence: String,
    pub bgp_observation: String,
    pub temporal_relationship: String,
    pub interpretation: String,
    pub limitation: String,
    pub finding: String,
}

impl PilotResultFile {
    pub fn to_record(&self) -> PilotRecord {
        PilotRecord {
            status: self.status.clone(),
            target: self.target.clone(),
            collector: self.collector.clone(),
            window_start_utc: self.window_start_utc.clone(),
            window_end_utc: self.window_end_utc.clone(),
            run_id: self.run_id,
            baseline_streams: self.baseline_streams,
            operator_evidence: self.operator_evidence.clone(),
            bgp_observation: self.bgp_observation.clone(),
            temporal_relationship: self.temporal_relationship.clone(),
            interpretation: self.interpretation.clone(),
            limitation: self.limitation.clone(),
            finding: self.finding.clone(),
        }
    }
}

/// Apply a reviewed pilot-result record to the case study's plan.
pub fn apply_pilot_result(conn: &Connection, path: &std::path::Path) -> Result<String, String> {
    let raw = std::fs::read_to_string(path)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let data: PilotResultFile = serde_json::from_str(&raw)
        .map_err(|e| format!("invalid pilot-result file {}: {e}", path.display()))?;
    if data.schema_version != 1 {
        return Err(format!(
            "unsupported pilot-result schema {}",
            data.schema_version
        ));
    }
    let Some(cs) = super::archive_plan::find_case_study(conn, &data.case_study_slug) else {
        return Err(format!(
            "no case study with slug '{}'",
            data.case_study_slug
        ));
    };
    save_pilot(conn, cs.id, &data.to_record())?;
    Ok(data.case_study_slug)
}

/// Record (or update) the reviewed pilot state on the case study's plan.
pub fn save_pilot(
    conn: &Connection,
    case_study_id: i64,
    pilot: &PilotRecord,
) -> Result<(), String> {
    let Some(mut plan) = load_plan(conn, case_study_id) else {
        return Err(
            "no analysis plan exists for this case study; run the planner first".to_string(),
        );
    };
    let mut ap: ArchivePlan =
        serde_json::from_str(&plan.plan_json).map_err(|e| format!("invalid stored plan: {e}"))?;
    ap.pilot = Some(pilot.clone());
    plan.plan_json = serde_json::to_string(&ap).map_err(|e| format!("serialize: {e}"))?;
    store::upsert_case_study_analysis_plan(
        conn,
        &CaseStudyAnalysisPlan {
            id: plan.id,
            case_study_id,
            horizon_json: plan.horizon_json,
            plan_json: plan.plan_json,
            status: plan.status,
            created_utc: plan.created_utc,
        },
    )?;
    Ok(())
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
                created_utc, updated_utc, interconnection_context
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
                interconnection_context: r.get(10)?,
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
            interconnection_context: None,
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

    // ── RouteViews/RIS source-family inventory ──

    #[test]
    fn routeviews_and_ris_collectors_have_distinct_identity() {
        let cs = sample_case_study();
        let (_, plan) = build_plan_for_families(
            &cs,
            &[],
            DEFAULT_WARMUP_HOURS,
            DEFAULT_COOLDOWN_HOURS,
            &[SourceFamily::RouteViews, SourceFamily::RipeRis],
        )
        .unwrap();
        assert_eq!(plan.collectors.len(), 4);
        let rv: Vec<_> = plan
            .collectors
            .iter()
            .filter(|c| c.source_family == SourceFamily::RouteViews.as_str())
            .collect();
        let ris: Vec<_> = plan
            .collectors
            .iter()
            .filter(|c| c.source_family == SourceFamily::RipeRis.as_str())
            .collect();
        assert_eq!(rv.len(), 2);
        assert_eq!(ris.len(), 2);
        // The same collector id never appears in the other family, and
        // identity is (family, collector) — rrc00 is RIPE RIS only.
        assert!(ris.iter().all(|c| c.collector.starts_with("rrc")));
        assert!(rv.iter().all(|c| c.collector.starts_with("route-views")));
        assert_eq!(SourceFamily::RouteViews.broker_project(), "routeviews");
        assert_eq!(SourceFamily::RipeRis.broker_project(), "riperis");
    }

    #[test]
    fn ris_archive_plan_uses_correct_cadence() {
        let cs = sample_case_study();
        let (_, plan) = build_plan_for_families(
            &cs,
            &[],
            DEFAULT_WARMUP_HOURS,
            DEFAULT_COOLDOWN_HOURS,
            &[SourceFamily::RipeRis],
        )
        .unwrap();
        assert_eq!(plan.collectors.len(), 2);
        for c in &plan.collectors {
            // RIS URLs live under data.ris.ripe.net with .gz archives.
            assert!(
                c.baseline_rib.url.starts_with("https://data.ris.ripe.net/"),
                "{}",
                c.baseline_rib.url
            );
            assert!(
                c.baseline_rib.url.contains("/bview."),
                "{}",
                c.baseline_rib.url
            );
            assert!(
                c.baseline_rib.url.ends_with(".gz"),
                "{}",
                c.baseline_rib.url
            );
            // 8-hour bview grid: warmup 02:00 floors to 00:00.
            assert!(
                c.baseline_rib.url.ends_with("bview.20190821.0000.gz"),
                "{}",
                c.baseline_rib.url
            );
            // Validation bview at cooldown end 00:38 -> 00:00 next day.
            assert!(c
                .validation_rib
                .as_ref()
                .unwrap()
                .url
                .ends_with("bview.20190822.0000.gz"));
            // Updates at the 5-minute cadence.
            assert!(
                c.updates[0].url.ends_with("updates.20190821.0200.gz"),
                "{}",
                c.updates[0].url
            );
            assert!(c.updates.iter().all(|u| u.url.ends_with(".gz")));
            assert_eq!(
                c.updates.len(),
                272,
                "5-minute cadence identical to RouteViews"
            );
            assert_eq!(c.first_update_utc, "20190821.0200");
        }
    }

    #[test]
    fn source_family_appears_in_observer_scope() {
        let cs = sample_case_study();
        let (_, plan) = build_plan_for_families(
            &cs,
            &[],
            DEFAULT_WARMUP_HOURS,
            DEFAULT_COOLDOWN_HOURS,
            &[SourceFamily::RouteViews, SourceFamily::RipeRis],
        )
        .unwrap();
        // The plan's observer scope (its collector list) carries the
        // source family explicitly; serialization preserves it.
        let json = serde_json::to_string(&plan).unwrap();
        assert!(json.contains("\"source_family\":\"RouteViews\""), "{json}");
        assert!(json.contains("\"source_family\":\"RipeRis\""), "{json}");
        for c in &plan.collectors {
            assert!(c.source_family == "RouteViews" || c.source_family == "RipeRis");
        }
    }

    #[test]
    fn report_does_not_call_ris_observer_routeviews() {
        // The analyst-facing report renders observers as collector:peer
        // strings (source-neutral); a RIPE RIS observer is never labeled
        // RouteViews.
        use crate::domain::assessment::{Evidence, Verdict};
        use crate::domain::event::EventId;
        use crate::domain::expectation::{ExpectationKind, ImpactExpectation};
        use chrono::TimeZone;
        let assessment = crate::domain::assessment::EventAssessment {
            event_id: EventId("INC-TEST".to_string()),
            expectation: ImpactExpectation {
                kind: ExpectationKind::NonRedundant,
                description: "test".to_string(),
                provenance: "reviewed".to_string(),
            },
            verdict: Verdict::NoObservableBgpImpact,
            evidence: vec![Evidence {
                description: "1 transition".to_string(),
                source_records: vec!["rrc00:195.66.224.1".to_string()],
            }],
            waves: Vec::new(),
            generated_at: chrono::Utc.with_ymd_and_hms(2026, 8, 1, 0, 0, 0).unwrap(),
        };
        let text = crate::report::render_terminal(&assessment, "RIPE RIS rrc00");
        assert!(text.contains("rrc00"), "{text}");
        assert!(
            !text.contains("RouteViews"),
            "a RIS observer must not be called RouteViews: {text}"
        );
    }

    #[test]
    fn mixed_source_plan_is_deterministic() {
        let cs = sample_case_study();
        let families = [SourceFamily::RouteViews, SourceFamily::RipeRis];
        let (h1, p1) = build_plan_for_families(&cs, &[], 2, 2, &families).unwrap();
        let (h2, p2) = build_plan_for_families(&cs, &[], 2, 2, &families).unwrap();
        assert_eq!(
            serde_json::to_string(&p1).unwrap(),
            serde_json::to_string(&p2).unwrap()
        );
        assert_eq!(
            serde_json::to_string(&h1).unwrap(),
            serde_json::to_string(&h2).unwrap()
        );
    }
}
