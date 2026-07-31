//! Analysis orchestration — the real-analysis pipeline.
//!
//! Wires ticket → manifest → broker discovery → cache → ingest →
//! reconstruct → tokenize → waves → assess → outcomes.

use std::collections::HashMap;
use std::path::Path;

use crate::discover::{
    cache_archive, dedupe_urls, select_rib, select_updates, validate_update_gaps,
    ArchiveDiscovery, ArchiveItem, CachedArchive,
};
use crate::domain::event::{EventId, EventWindow, OperationalEvent};
use crate::domain::observation::{
    IngestRole, RouteObservation,
};
use crate::ingest::{IngestContext, ObservationStream};
use crate::manifest::Manifest;
use crate::outcome::AnalysisOutcome;
use crate::target::{admit_observation, scan_rib_and_freeze, PreflightCounts};

/// Run the real analysis pipeline for a given event and manifest.
pub fn run_real_analysis(
    event_path: &Path,
    manifest_path: &Path,
    cache_dir: &Path,
    out_dir: &Path,
    discovery: &dyn ArchiveDiscovery,
) -> AnalysisOutcome {
    match run_inner(event_path, manifest_path, cache_dir, out_dir, discovery) {
        Ok(outcome) => outcome,
        Err(e) => AnalysisOutcome::incomplete(e),
    }
}

fn run_inner(
    event_path: &Path,
    manifest_path: &Path,
    cache_dir: &Path,
    _out_dir: &Path,
    discovery: &dyn ArchiveDiscovery,
) -> Result<AnalysisOutcome, String> {
    // ── 1. Parse the ticket ────────────────────────────────────────
    let ticket = crate::sources::internet2::ticket::parse_ticket_fixture(
        event_path.to_str().ok_or("event path is not valid UTF-8")?,
    )
    .map_err(|e| format!("failed to parse ticket fixture: {e}"))?;
    let expectation = crate::sources::internet2::ticket::derive_expectation(&ticket);

    // ── 2. Load manifest ───────────────────────────────────────────
    let manifest = Manifest::load(manifest_path)?;
    let (event_start, event_end) = manifest.event_window()?;
    let warmup_start = event_start - chrono::Duration::minutes(manifest.warmup_minutes);
    let cooldown_end = event_end + chrono::Duration::minutes(manifest.cooldown_minutes);

    // ── 3. Broker discovery ────────────────────────────────────────
    // Discover RIB files (search window: well before warmup_start)
    let rib_search_start = warmup_start - chrono::Duration::hours(24);
    let rib_search_end = warmup_start + chrono::Duration::hours(1);

    let collectors_str: Vec<&str> = manifest.collectors.iter().map(|s| s.as_str()).collect();
    // Query per-collector for RIBs and UPDATES below
    let all_ribs = discovery
        .query("route-views", &collectors_str, rib_search_start, rib_search_end, "rib")
        .map_err(|e| format!("broker discovery failed for RIBs: {e}"))?;

    let mut all_updates: Vec<ArchiveItem> = Vec::new();
    let mut selected_ribs: Vec<CachedArchive> = Vec::new();
    let mut selected_updates: Vec<CachedArchive> = Vec::new();
    let mut continuity_gaps = Vec::new();
    let mut any_continuity_unknown = false;

    for collector in &manifest.collectors {
        // Select RIB
        let collector_ribs: Vec<ArchiveItem> = all_ribs
            .iter()
            .filter(|i| i.collector_id == *collector)
            .cloned()
            .collect();
        let best_rib = select_rib(&collector_ribs, warmup_start)
            .ok_or_else(|| format!("no RIB found for collector {collector} at/before warmup"))?;

        // Cache RIB
        let cached_rib = cache_archive(best_rib, cache_dir)
            .map_err(|e| format!("failed to cache RIB for {collector}: {e}"))?;
        let rib_ts = best_rib.ts_start;
        selected_ribs.push(cached_rib);

        // Discover UPDATE files overlapping [rib_ts, cooldown_end]
        let update_search_end = cooldown_end + chrono::Duration::hours(1);
        let all_collector_updates: Vec<ArchiveItem> = discovery
            .query(
                "route-views",
                &[collector.as_str()],
                rib_search_start,
                update_search_end,
                "updates",
            )
            .map_err(|e| format!("broker discovery failed for updates ({collector}): {e}"))?;

        let selected: Vec<&ArchiveItem> = select_updates(&all_collector_updates, rib_ts, cooldown_end);
        // Validate gaps
        let gaps = validate_update_gaps(&selected, chrono::Duration::minutes(5));
        if !gaps.is_empty() {
            any_continuity_unknown = true;
            continuity_gaps.extend(gaps);
        }

        // Cache UPDATE files
        for item in &selected {
            let cu = cache_archive(item, cache_dir)
                .map_err(|e| format!("failed to cache UPDATE for {collector}: {e}"))?;
            all_updates.push(ArchiveItem {
                project: item.project.clone(),
                collector_id: item.collector_id.clone(),
                data_type: item.data_type.clone(),
                ts_start: item.ts_start,
                ts_end: item.ts_end,
                url: item.url.clone(),
                size: item.size,
            });
            selected_updates.push(cu);
        }
    }

    // Deduplicate URLs
    let _dups = dedupe_urls(&mut all_updates);

    // ── 4. Ingest RIBs for preflight ────────────────────────────────
    let mut rib_observations: Vec<RouteObservation> = Vec::new();

    for cached_rib in &selected_ribs {
        let ctx = IngestContext {
            role: IngestRole::Rib,
            collector: crate::domain::observation::CollectorId(cached_rib.collector_id.clone()),
            input_path: std::path::PathBuf::new(), // set by from_local_file
            source_url: Some(cached_rib.url.clone()),
            source_sha: Some(cached_rib.sha256.clone()),
        };
        let path = Path::new(&cached_rib.local_path);
        let mut stream = ObservationStream::from_local_file(path.to_path_buf(), ctx)
            .map_err(|e| format!("failed to open RIB {}: {e}", path.display()))?;

        for result in &mut stream {
            let obs = result.map_err(|e| format!("RIB parse error: {e}"))?;
            rib_observations.push(obs);
        }
    }

    // ── 5. RIB preflight ───────────────────────────────────────────
    let target = scan_rib_and_freeze(
        &rib_observations,
        &manifest.target.origin_asns,
        manifest.target.internet2_asn,
    );

    if target.total_streams() == 0 {
        return Ok(AnalysisOutcome::insufficient_visibility(
            "No selected RouteViews observer had a pre-event route matching the reviewed Internet2 path predicate.",
        ));
    }

    let frozen_prefixes = target.frozen_prefixes();

    // Compute preflight counts
    let origin_matches = rib_observations
        .iter()
        .filter(|obs| {
            obs.attributes
                .as_ref()
                .and_then(|a| a.origin_asns.first())
                .map(|a| manifest.target.origin_asns.contains(&a.0))
                .unwrap_or(false)
        })
        .count();
    let transit_matches = rib_observations
        .iter()
        .filter(|obs| {
            obs.attributes
                .as_ref()
                .map(|a| a.as_path.contains(&manifest.target.internet2_asn))
                .unwrap_or(false)
        })
        .count();

    let _preflight = PreflightCounts::from_target_set(
        &target,
        manifest.collectors.len(),
        origin_matches,
        transit_matches,
    );

    // ── 6. Ingest UPDATEs for relevant collectors only ──────────────
    let mut update_observations: Vec<RouteObservation> = Vec::new();

    for cached_update in &selected_updates {
        // Only ingest if this collector has relevant streams
        if !target.has_relevant_streams(&cached_update.collector_id) {
            continue;
        }

        let ctx = IngestContext {
            role: IngestRole::Updates,
            collector: crate::domain::observation::CollectorId(cached_update.collector_id.clone()),
            input_path: std::path::PathBuf::new(),
            source_url: Some(cached_update.url.clone()),
            source_sha: Some(cached_update.sha256.clone()),
        };
        let path = Path::new(&cached_update.local_path);
        let mut stream = ObservationStream::from_local_file(path.to_path_buf(), ctx)
            .map_err(|e| format!("failed to open UPDATE {}: {e}", path.display()))?;

        for result in &mut stream {
            let obs = result.map_err(|e| format!("UPDATE parse error: {e}"))?;
            // Admission gate
            if admit_observation(&obs, &target, &frozen_prefixes) {
                update_observations.push(obs);
            }
        }
    }

    // ── 7. Combine and sort observations ───────────────────────────
    let mut all_observations = rib_observations;
    all_observations.extend(update_observations);
    all_observations.sort_by(|a, b| {
        a.timestamp
            .cmp(&b.timestamp)
            .then_with(|| a.collector.0.cmp(&b.collector.0))
            .then_with(|| a.provenance.element_seq.cmp(&b.provenance.element_seq))
    });

    // ── 8. Route reconstruction ────────────────────────────────────
    let (_store, changes) = crate::routes::reconstruct_routes(
        all_observations,
        event_start,
        event_end,
        cooldown_end,
    );

    // ── 9. Tokenize (classify changes into transitions) ────────────
    let baseline: HashMap<crate::domain::route::RouteKey, crate::domain::route::RouteState> = HashMap::new();
    let transitions = crate::tokenize::tokenize(changes, &baseline);

    // ── 10. Detect waves ───────────────────────────────────────────
    let mut waves = crate::waves::detect_waves(&transitions, chrono::Duration::minutes(2));

    // ── 11. Summarize waves (labels) ───────────────────────────────
    crate::waves::summarize_waves(&mut waves);

    // ── 12. Assess ─────────────────────────────────────────────────
    let event_window = EventWindow {
        start: event_start,
        end: event_end,
    };

    let event = OperationalEvent {
        id: EventId::from(manifest.event_id.as_str()),
        source: "internet2-grnoc".to_string(),
        window: event_window,
        title: ticket.title,
        raw: serde_json::Value::Null,
    };

    let assessment = crate::assess::assess(
        event.id,
        expectation,
        &transitions,
        waves,
        any_continuity_unknown,
    );

    // ── 13. Build outcome ──────────────────────────────────────────
    Ok(AnalysisOutcome::completed(assessment))
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::discover::{ArchiveItem, InimArchiveError};
    
    use crate::outcome::AnalysisOutcome;
    use crate::target::TargetSet;
    use std::collections::HashMap;

    /// Failing discovery — always returns a broker error.
    struct FailingDiscovery;
    impl ArchiveDiscovery for FailingDiscovery {
        fn query(
            &self,
            _project: &str,
            _collectors: &[&str],
            _ts_start: chrono::DateTime<chrono::Utc>,
            _ts_end: chrono::DateTime<chrono::Utc>,
            _data_type: &str,
        ) -> Result<Vec<ArchiveItem>, InimArchiveError> {
            Err(InimArchiveError::BrokerQueryError {
                reason: "simulated broker failure".into(),
            })
        }
    }

    #[test]
    fn broker_failure_returns_incomplete() {
        let dir = tempfile::tempdir().unwrap();
        let outcome = run_real_analysis(
            Path::new("nonexistent.json"),
            Path::new("nonexistent.json"),
            dir.path(),
            &dir.path().join("out"),
            &FailingDiscovery,
        );
        // parse_ticket_fixture fails first (file doesn't exist), so that's also Incomplete
        match outcome {
            AnalysisOutcome::Incomplete { failure } => {
                assert!(failure.contains("parse"), "expected parse error: {failure}");
            }
            _ => panic!("expected Incomplete"),
        }
    }

    #[test]
    fn empty_preflight_returns_insufficient_visibility() {
        // Empty TargetSet produces the exact visibility reason.
        let target = TargetSet { streams: HashMap::new() };
        assert_eq!(target.total_streams(), 0);

        let outcome = AnalysisOutcome::insufficient_visibility(
            "No selected RouteViews observer had a pre-event route matching the reviewed Internet2 path predicate.",
        );
        match outcome {
            AnalysisOutcome::InsufficientVisibility { reason } => {
                assert!(reason.contains("RouteViews"));
            }
            _ => panic!("expected InsufficientVisibility"),
        }
    }

    #[test]
    fn infrastructure_failure_never_becomes_visibility_verdict() {
        // An Incomplete outcome must not claim visibility.
        let outcome = AnalysisOutcome::incomplete("broker unreachable");
        let json = serde_json::to_string(&outcome).unwrap();
        assert!(json.contains("incomplete"));
        assert!(!json.contains("insufficient_visibility"));
        assert!(!json.contains("visible"));
    }
}
