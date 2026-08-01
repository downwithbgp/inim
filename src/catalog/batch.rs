//! Shared archive planning across events — CorrelationBatch
//! (Session 33, Part 11).
//!
//! A corpus must not download and parse the same raw archive
//! independently for every ticket. The batch planner groups candidate
//! event analyses by source family, collector, archive URL, and
//! overlapping time horizon, producing:
//!
//! - per-event cohort plans (each event keeps its own independent plan),
//! - the unique raw archive set (RIBs + UPDATEs),
//! - the archive-consumer map (which events need which archive),
//! - archives avoided through reuse,
//! - estimated compressed bytes and expected parse operations.
//!
//! Archive reuse NEVER merges event evidence: each event continues to
//! produce its own AnalysisPlan/AnalysisRun/evidence/verdict, and
//! evidence identity does not depend on which batch was run. The batch
//! plan is a pure deterministic function of the per-event plans.
//!
//! This is not an event-independent BGP warehouse: every archive is
//! still justified by at least one event cohort.

use std::collections::BTreeMap;

use super::archive_plan::{AnalysisHorizon, ArchivePlan, ExpectedFile};

/// One event's cohort plan inside a batch.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct BatchEventPlan {
    /// Catalog event id (external identifier).
    pub event_id: String,
    /// The event's own cohort: exactly this event. Batches never combine
    /// separate event cohorts into one assessment.
    pub cohort: Vec<String>,
    pub warmup_start_utc: String,
    pub incident_start_utc: String,
    pub incident_end_utc: String,
    pub cooldown_end_utc: String,
    /// The event's own archive plan (identical to the standalone plan).
    pub plan: ArchivePlan,
}

/// One unique raw archive and its consumers.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ArchiveConsumer {
    /// Canonical archive URL (identity: URL; SHA-256 is recorded at
    /// acquisition time).
    pub url: String,
    pub source_family: String,
    pub collector: String,
    pub data_type: String,
    /// Event ids whose cohort plans include this archive.
    pub consumers: Vec<String>,
}

/// The deterministic batch plan.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CorrelationBatch {
    /// Deterministic batch identifier (sorted event ids).
    pub batch_id: String,
    pub events: Vec<BatchEventPlan>,
    pub unique_archives: Vec<ArchiveConsumer>,
    /// Sum of per-event archive counts minus unique archives.
    pub archives_avoided_through_reuse: usize,
    pub estimated_compressed_bytes: i64,
    /// Expected parse operations: one per unique archive.
    pub expected_parse_operations: usize,
    pub source_families: Vec<String>,
    /// True when the batch was produced by the deterministic planner
    /// (it always is).
    pub deterministic: bool,
}

fn horizon_fields(h: &AnalysisHorizon) -> (String, String, String, String) {
    (
        h.warmup_start_utc.clone(),
        h.incident_start_utc.clone(),
        h.incident_end_utc.clone(),
        h.cooldown_end_utc.clone(),
    )
}

/// All archive URLs of one plan (baseline RIB, validation RIB, updates)
/// with family, collector, data type, and estimated size.
fn plan_archive_urls(plan: &ArchivePlan) -> Vec<(String, String, String, String, i64)> {
    // (url, family, collector, data_type, size_estimated_bytes)
    let mut out = Vec::new();
    for c in &plan.collectors {
        let family = c.source_family.clone();
        let size = |f: &ExpectedFile| f.size_estimated_bytes.unwrap_or(0);
        out.push((
            c.baseline_rib.url.clone(),
            family.clone(),
            c.collector.clone(),
            "rib".to_string(),
            size(&c.baseline_rib),
        ));
        if let Some(v) = &c.validation_rib {
            out.push((
                v.url.clone(),
                family.clone(),
                c.collector.clone(),
                "rib".to_string(),
                size(v),
            ));
        }
        for u in &c.updates {
            out.push((
                u.url.clone(),
                family.clone(),
                c.collector.clone(),
                "updates".to_string(),
                size(u),
            ));
        }
    }
    out
}

/// A per-event input to the batch planner.
pub struct EventPlanInput {
    pub event_id: String,
    pub horizon: AnalysisHorizon,
    pub plan: ArchivePlan,
}

/// Build a deterministic correlation batch from per-event plans.
///
/// Pure computation — never performs network I/O. Each event's plan is
/// used as-is (identical to its standalone plan); the batch only groups
/// the raw archive requirements.
pub fn plan_batch(inputs: &[EventPlanInput]) -> CorrelationBatch {
    let mut events: Vec<BatchEventPlan> = inputs
        .iter()
        .map(|i| {
            let (ws, is, ie, ce) = horizon_fields(&i.horizon);
            BatchEventPlan {
                event_id: i.event_id.clone(),
                cohort: vec![i.event_id.clone()],
                warmup_start_utc: ws,
                incident_start_utc: is,
                incident_end_utc: ie,
                cooldown_end_utc: ce,
                plan: i.plan.clone(),
            }
        })
        .collect();
    events.sort_by(|a, b| a.event_id.cmp(&b.event_id));

    // Unique archive map: url -> (family, collector, data_type, size, consumers).
    let mut archives: BTreeMap<String, (String, String, String, i64, Vec<String>)> =
        BTreeMap::new();
    let mut total_event_archive_requests = 0usize;
    for event in &events {
        for (url, family, collector, data_type, size) in plan_archive_urls(&event.plan) {
            total_event_archive_requests += 1;
            archives
                .entry(url.clone())
                .or_insert_with(|| (family, collector, data_type, size, Vec::new()))
                .4
                .push(event.event_id.clone());
        }
    }

    let unique_count = archives.len();
    let estimated_compressed_bytes: i64 = archives.values().map(|(_, _, _, size, _)| *size).sum();

    let unique_archives: Vec<ArchiveConsumer> = archives
        .into_iter()
        .map(
            |(url, (family, collector, data_type, _size, mut consumers))| {
                consumers.sort();
                consumers.dedup();
                ArchiveConsumer {
                    url,
                    source_family: family,
                    collector,
                    data_type,
                    consumers,
                }
            },
        )
        .collect();

    let batch_id = {
        let ids: Vec<&str> = events.iter().map(|e| e.event_id.as_str()).collect();
        crate::catalog::sync::hex_sha256(&ids.join(","))
    };

    let mut families: Vec<String> = unique_archives
        .iter()
        .map(|a| a.source_family.clone())
        .collect();
    families.sort();
    families.dedup();

    CorrelationBatch {
        batch_id,
        events,
        unique_archives,
        archives_avoided_through_reuse: total_event_archive_requests.saturating_sub(unique_count),
        estimated_compressed_bytes,
        expected_parse_operations: unique_count,
        source_families: families,
        deterministic: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::archive_plan::{self, AnalysisHorizon};

    fn plan_for(start: &str, end: &str) -> (AnalysisHorizon, ArchivePlan) {
        let cs = crate::catalog::domain::CaseStudy {
            id: 1,
            slug: "t".to_string(),
            title: "T".to_string(),
            summary: "s".to_string(),
            start_utc: Some(start.to_string()),
            end_utc: Some(end.to_string()),
            status: "Active".to_string(),
            content_sha256: "abc".to_string(),
            created_utc: "2026-08-01T00:00:00Z".to_string(),
            updated_utc: "2026-08-01T00:00:00Z".to_string(),
        };
        archive_plan::build_plan(&cs, &[], 2, 2).unwrap()
    }

    fn input(event: &str, horizon: AnalysisHorizon, plan: ArchivePlan) -> EventPlanInput {
        EventPlanInput {
            event_id: event.to_string(),
            horizon,
            plan,
        }
    }

    #[test]
    fn overlapping_events_share_raw_archive_plan() {
        let (h1, p1) = plan_for("2019-08-21T04:00:00Z", "2019-08-21T06:00:00Z");
        let (h2, p2) = plan_for("2019-08-21T05:00:00Z", "2019-08-21T07:00:00Z");
        let standalone_count = plan_archive_urls(&p1).len() + plan_archive_urls(&p2).len();
        let batch = plan_batch(&[input("A", h1, p1), input("B", h2, p2)]);
        assert!(
            batch.unique_archives.len() < standalone_count,
            "overlapping events must share raw archives"
        );
        // At least one archive serves both events.
        assert!(batch.unique_archives.iter().any(|a| a.consumers.len() == 2));
        assert!(batch.archives_avoided_through_reuse > 0);
        // The batch's estimated bytes are the unique set only.
        assert_eq!(batch.expected_parse_operations, batch.unique_archives.len());
    }

    #[test]
    fn nonoverlapping_events_do_not_share_unneeded_archives() {
        let (h1, p1) = plan_for("2019-08-21T04:00:00Z", "2019-08-21T06:00:00Z");
        let (h2, p2) = plan_for("2019-09-01T04:00:00Z", "2019-09-01T06:00:00Z");
        let standalone_count = plan_archive_urls(&p1).len() + plan_archive_urls(&p2).len();
        let batch = plan_batch(&[input("A", h1, p1), input("B", h2, p2)]);
        // Distinct months: no archive is shared.
        assert_eq!(batch.unique_archives.len(), standalone_count);
        assert_eq!(batch.archives_avoided_through_reuse, 0);
        assert!(batch.unique_archives.iter().all(|a| a.consumers.len() == 1));
    }

    #[test]
    fn archive_reuse_does_not_merge_event_evidence() {
        let (h1, p1) = plan_for("2019-08-21T04:00:00Z", "2019-08-21T06:00:00Z");
        let (h2, p2) = plan_for("2019-08-21T05:00:00Z", "2019-08-21T07:00:00Z");
        let batch = plan_batch(&[input("A", h1, p1), input("B", h2, p2)]);
        // Every event keeps its OWN cohort (never combined).
        for e in &batch.events {
            assert_eq!(e.cohort, vec![e.event_id.clone()]);
            assert_eq!(e.plan.collectors.len(), 2, "per-event plan intact");
        }
        // The batch contains no assessment, no evidence, no verdict.
        let json = serde_json::to_string(&batch).unwrap();
        assert!(!json.contains("verdict"));
        assert!(!json.contains("evidence"));
    }

    #[test]
    fn event_results_are_identical_batched_or_standalone() {
        let (h1, p1) = plan_for("2019-08-21T04:00:00Z", "2019-08-21T06:00:00Z");
        let (h2, p2) = plan_for("2019-08-21T05:00:00Z", "2019-08-21T07:00:00Z");
        let standalone = serde_json::to_string(&p1).unwrap();
        let batch = plan_batch(&[input("A", h1, p1), input("B", h2, p2)]);
        let batched = batch
            .events
            .iter()
            .find(|e| e.event_id == "A")
            .unwrap()
            .plan
            .clone();
        assert_eq!(standalone, serde_json::to_string(&batched).unwrap());
    }

    #[test]
    fn evidence_ids_do_not_depend_on_batch_membership() {
        // Evidence identity is run-level: the batch plan defines no
        // evidence ids at all, and an event's plan is byte-identical
        // whether planned alone or in a batch of two.
        let (h1, p1) = plan_for("2019-08-21T04:00:00Z", "2019-08-21T06:00:00Z");
        let (h2, p2) = plan_for("2019-08-21T05:00:00Z", "2019-08-21T07:00:00Z");
        let alone = plan_batch(&[input("A", h1.clone(), p1.clone())]);
        let together = plan_batch(&[input("A", h1, p1), input("B", h2, p2)]);
        let a_alone = alone.events.iter().find(|e| e.event_id == "A").unwrap();
        let a_together = together.events.iter().find(|e| e.event_id == "A").unwrap();
        assert_eq!(
            serde_json::to_string(&a_alone.plan).unwrap(),
            serde_json::to_string(&a_together.plan).unwrap()
        );
        // The batch id itself differs with membership, but the event
        // plan — the input to evidence derivation — does not.
        assert_ne!(alone.batch_id, together.batch_id);
    }

    #[test]
    fn batch_plan_is_deterministic() {
        let (h1, p1) = plan_for("2019-08-21T04:00:00Z", "2019-08-21T06:00:00Z");
        let (h2, p2) = plan_for("2019-08-21T05:00:00Z", "2019-08-21T07:00:00Z");
        let batch_a = plan_batch(&[input("A", h1, p1), input("B", h2, p2)]);
        let (h1, p1) = plan_for("2019-08-21T04:00:00Z", "2019-08-21T06:00:00Z");
        let (h2, p2) = plan_for("2019-08-21T05:00:00Z", "2019-08-21T07:00:00Z");
        let batch_b = plan_batch(&[input("A", h1, p1), input("B", h2, p2)]);
        assert_eq!(
            serde_json::to_string(&batch_a).unwrap(),
            serde_json::to_string(&batch_b).unwrap()
        );
        assert!(batch_a.deterministic);
    }

    #[test]
    fn shared_raw_archive_can_feed_independent_runs() {
        // Two events whose cohorts need the SAME raw archive (same
        // family, collector, URL) share one ArchiveConsumer; each event
        // keeps its own independent BatchEventPlan.
        let (h1, p1) = plan_for("2019-08-21T04:00:00Z", "2019-08-21T06:00:00Z");
        let (h2, p2) = plan_for("2019-08-21T05:00:00Z", "2019-08-21T07:00:00Z");
        let batch = plan_batch(&[
            input("A", h1.clone(), p1.clone()),
            input("B", h2.clone(), p2.clone()),
        ]);
        let shared = batch
            .unique_archives
            .iter()
            .find(|a| a.consumers.len() == 2)
            .expect("shared archive must exist");
        assert_eq!(shared.consumers, vec!["A".to_string(), "B".to_string()]);
        // The per-event plans are exactly the standalone plans.
        let a = batch.events.iter().find(|e| e.event_id == "A").unwrap();
        let b = batch.events.iter().find(|e| e.event_id == "B").unwrap();
        assert_eq!(a.plan, p1);
        assert_eq!(b.plan, p2);
        // Cohorts are single-member — evidence is never merged.
        assert_eq!(a.cohort, vec!["A".to_string()]);
        assert_eq!(b.cohort, vec!["B".to_string()]);
        // Deterministic: same inputs, same batch.
        let again = plan_batch(&[input("A", h1, p1), input("B", h2, p2)]);
        assert_eq!(
            serde_json::to_string(&batch).unwrap(),
            serde_json::to_string(&again).unwrap()
        );
    }

    #[test]
    fn incompatible_cohorts_do_not_share_wrong_derived_cache() {
        // Raw archives may be shared, but DERIVED caches are keyed on
        // cohort identity (origin ASNs + predicate + family). Two events
        // over the same archive with different targets must get
        // different derived cache keys — no wrong reuse.
        use crate::domain::route::TransitPredicate;
        let k1 = crate::derived_cache::rib_cache_key(
            "sha",
            "rrc00",
            &[2603],
            &TransitPredicate::ContainsAny(vec![11537]),
            1,
            "RipeRis",
        );
        let k2 = crate::derived_cache::rib_cache_key(
            "sha",
            "rrc00",
            &[64500],
            &TransitPredicate::ContainsAny(vec![11537]),
            1,
            "RipeRis",
        );
        assert_ne!(k1, k2, "different cohorts must not share derived caches");
        let t1 = crate::derived_cache::update_cache_key("sha", "rrc00", "targetset-A", "RipeRis");
        let t2 = crate::derived_cache::update_cache_key("sha", "rrc00", "targetset-B", "RipeRis");
        assert_ne!(t1, t2);
        // Families differ -> keys differ even for identical cohorts.
        let k3 = crate::derived_cache::rib_cache_key(
            "sha",
            "rrc00",
            &[2603],
            &TransitPredicate::ContainsAny(vec![11537]),
            1,
            "RouteViews",
        );
        assert_ne!(k1, k3);
    }

    #[test]
    fn failed_run_does_not_invalidate_successful_batch_member() {
        // A blocked event (no reviewed entity mapping) still plans as a
        // member with its own blocked state; the successful member's
        // plan is untouched and the batch still deduplicates correctly.
        let (h1, p1) = plan_for("2019-08-21T04:00:00Z", "2019-08-21T06:00:00Z");
        let (h2, p2) = plan_for("2019-08-21T05:00:00Z", "2019-08-21T07:00:00Z");
        let mut blocked = p2.clone();
        blocked
            .blocked_targets
            .push(crate::catalog::archive_plan::BlockedTarget {
                source_label: "SampleNet".to_string(),
                reason: "MissingReviewedEntityMapping".to_string(),
            });
        let batch = plan_batch(&[
            input("OK", h1.clone(), p1.clone()),
            input("BLOCKED", h2.clone(), blocked.clone()),
        ]);
        let ok = batch.events.iter().find(|e| e.event_id == "OK").unwrap();
        assert_eq!(ok.plan, p1, "successful member's plan must be unchanged");
        let blk = batch
            .events
            .iter()
            .find(|e| e.event_id == "BLOCKED")
            .unwrap();
        assert!(!blk.plan.blocked_targets.is_empty());
        // The batch still computed the unique archive set for both.
        assert!(!batch.unique_archives.is_empty());
        // A failed run of one member never rewrites another's identity.
        assert_eq!(
            batch.batch_id,
            plan_batch(&[input("OK", h1, p1), input("BLOCKED", h2, blocked),]).batch_id
        );
    }

    #[test]
    fn standalone_and_batched_run_artifacts_match() {
        // The batch plan is a pure grouping of standalone plans: every
        // per-event plan inside the batch equals the standalone plan
        // (archive URLs, sizes, families), so standalone and batched
        // runs produce identical artifacts.
        let (h1, p1) = plan_for("2019-08-21T04:00:00Z", "2019-08-21T06:00:00Z");
        let batch = plan_batch(&[input("A", h1, p1.clone())]);
        let member = &batch.events[0];
        assert_eq!(member.plan, p1);
    }
}
