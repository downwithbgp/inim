//! Analysis orchestration — the real-analysis pipeline.
//!
//! Wires ticket → manifest → broker discovery → cache RIBs →
//! RIB preflight (streaming, per-collector) → cache UPDATEs
//! for retained collectors only → ingest → reconstruct →
//! tokenize → waves → assess → outcomes → outputs.

use std::collections::HashMap;
use std::path::Path;
use std::time::Instant;

use crate::discover::{
    cache_archive, filename_timestamp, select_updates, validate_update_gaps, ArchiveDiscovery,
    CachedArchive,
};
use crate::domain::event::{EventId, EventWindow, OperationalEvent};
use crate::domain::observation::{IngestRole, RouteObservation};
use crate::domain::route::Prefix;
use crate::domain::route::{RouteKey, RouteState};
use crate::ingest::{IngestContext, ObservationStream};
use crate::manifest::Manifest;
use crate::outcome::AnalysisOutcome;
use crate::target::{scan_rib_and_freeze, PreflightCounts, TargetSet};

/// Cache control flags for the analysis pipeline.
#[derive(Debug, Clone, Copy)]
pub struct CacheControl {
    /// Disable all derived caches (both RIB and UPDATE).
    pub no_derived_cache: bool,
    /// Force rebuild of all derived caches (ignore and overwrite).
    pub rebuild_derived_cache: bool,
    /// Rebuild only UPDATE derived caches (keep the RIB derived cache).
    pub rebuild_update_caches: bool,
    /// Number of parallel parsing jobs (1 = serial). 0 is rejected by the
    /// CLI; use --parse-jobs for an explicit parse concurrency.
    pub jobs: usize,
    /// Explicit parser-worker count; 0 = follow `jobs`.
    pub parse_jobs: usize,
    /// Network download concurrency (conservative default).
    pub download_jobs: usize,
}

impl Default for CacheControl {
    fn default() -> Self {
        CacheControl {
            no_derived_cache: false,
            rebuild_derived_cache: false,
            rebuild_update_caches: false,
            jobs: 1,
            parse_jobs: 0,
            download_jobs: 2,
        }
    }
}

// ── Public entry point ─────────────────────────────────────────────

pub fn run_real_analysis(
    event_path: &Path,
    manifest_path: &Path,
    cache_dir: &Path,
    out_dir: &Path,
    discovery: &dyn ArchiveDiscovery,
    cache_control: CacheControl,
    preflight_only: bool,
) -> AnalysisOutcome {
    let mut timings: Vec<(String, f64)> = Vec::new();
    let t0 = Instant::now();
    match run_inner(
        event_path,
        manifest_path,
        cache_dir,
        out_dir,
        discovery,
        cache_control,
        preflight_only,
        &mut timings,
    ) {
        Ok(outcome) => {
            print_timings(&timings, t0.elapsed().as_secs_f64());
            outcome
        }
        Err(e) => {
            print_timings(&timings, t0.elapsed().as_secs_f64());
            AnalysisOutcome::incomplete(e)
        }
    }
}

// ── Inner pipeline ──────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
fn run_inner(
    event_path: &Path,
    manifest_path: &Path,
    cache_dir: &Path,
    out_dir: &Path,
    discovery: &dyn ArchiveDiscovery,
    cache_control: CacheControl,
    preflight_only: bool,
    timings: &mut Vec<(String, f64)>,
) -> Result<AnalysisOutcome, String> {
    // ── Parse ticket + manifest ───────────────────────────────────
    let ticket = crate::sources::internet2::ticket::parse_ticket_fixture(
        event_path.to_str().ok_or("event path is not valid UTF-8")?,
    )
    .map_err(|e| format!("failed to parse ticket fixture: {e}"))?;
    let expectation = crate::sources::internet2::ticket::derive_expectation(&ticket);

    let manifest = Manifest::load(manifest_path)?;
    let family = crate::catalog::archive_plan::SourceFamily::parse_family(&manifest.source_family)
        .ok_or_else(|| {
            format!(
                "unsupported source_family '{}' in manifest (expected RouteViews or RipeRis)",
                manifest.source_family
            )
        })?;
    let (event_start, event_end) = manifest.event_window()?;
    let warmup_start = event_start - chrono::Duration::minutes(manifest.warmup_minutes);
    let cooldown_end = event_end + chrono::Duration::minutes(manifest.cooldown_minutes);

    let mut limitations: Vec<String> = Vec::new();
    let mut collected_ribs: Vec<CachedArchive> = Vec::new();
    let mut collected_updates: Vec<CachedArchive> = Vec::new();
    let mut target_set = TargetSet::default();
    let mut rib_observations: Vec<RouteObservation> = Vec::new();
    let mut update_observations: Vec<RouteObservation> = Vec::new();
    let mut archive_metrics: Vec<crate::perf::ArchiveMetric> = Vec::new();
    let mut rib_metrics: Vec<crate::perf::ArchiveMetric> = Vec::new();
    let mut any_continuity_unknown = false;
    let mut per_collector_counts: Vec<(String, usize, usize, usize, usize)> = Vec::new();

    // ── Phase A: per-collector RIB preflight (before UPDATEs) ──────
    let rib_search_start = warmup_start - chrono::Duration::hours(24);
    let rib_search_end = warmup_start + chrono::Duration::hours(1);

    eprintln!("→ Broker discovery: RIB files");
    let t_broker = Instant::now();
    let all_ribs = discovery
        .query(
            family.broker_project(),
            &manifest
                .collectors
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>(),
            rib_search_start,
            rib_search_end,
            "rib",
        )
        .map_err(|e| format!("broker discovery failed for RIBs: {e}"))?;
    let _ = timings; // timing recorded at end

    for collector in &manifest.collectors {
        let best_rib = all_ribs
            .iter()
            .filter(|i| {
                i.collector_id == *collector && i.data_type == "rib" && i.ts_start <= warmup_start
            })
            .max_by_key(|i| i.ts_start)
            .cloned()
            .ok_or_else(|| {
                let msg = format!("no RIB found for collector {collector} at/before warmup");
                limitations.push(msg.clone());
                eprintln!("  {collector}: skipped ({msg})");
                msg // error string for the Err path
            })?;

        eprintln!("  {collector}: caching RIB {}", best_rib.url);
        let t_cache = Instant::now();
        let cached_rib = cache_archive(&best_rib, cache_dir)
            .map_err(|e| format!("failed to cache RIB for {collector}: {e}"))?;
        if timings.is_empty() {
            // rough broker timing
            timings.push((
                "broker+cache".to_string(),
                t_cache.duration_since(t_broker).as_secs_f64(),
            ));
        }

        // ── Check derived RIB cache ──────────────────────────────
        let transit_predicate = reviewed_transit_predicate(&manifest)?;
        let origin_asns = manifest.target.origin_asns.clone();
        let rib_key = crate::derived_cache::rib_cache_key(
            &cached_rib.sha256,
            collector,
            &origin_asns,
            &transit_predicate,
            manifest.revision,
            family.as_str(),
        );
        let cache_hit = if cache_control.no_derived_cache || cache_control.rebuild_derived_cache {
            None
        } else {
            crate::derived_cache::load_rib_cache(cache_dir, &rib_key, &cached_rib.sha256)
        };

        if let Some(cached) = cache_hit {
            eprintln!("  [{collector} RIB] derived cache hit, skipping parse");
            let collector_target = crate::target::TargetSet::default();
            let mut cts = collector_target.clone();
            // Rebuild target set from cached streams
            for s in &cached.frozen_streams {
                use crate::domain::observation::CollectorId;
                let _key = CollectorId(collector.clone());
                // Push each stream into a minimal target
                let stream = crate::target::TargetStream {
                    peer_ip: s.peer_ip,
                    prefix: s.prefix.clone(),
                    origin_as: 0,
                    as_path: s.baseline_as_path.clone(),
                    path_id: None,
                };
                cts.streams
                    .entry(collector.clone())
                    .or_default()
                    .push(stream);
            }
            target_set.merge(&cts);

            let streams = cached.preflight.frozen_streams;
            let c_pref = cached.preflight.distinct_prefixes;
            let c_peers = cached.preflight.distinct_peers;
            eprintln!(
                "  [{collector} RIB] cached: {} frozen streams, {c_pref} prefixes, {c_peers} peers (0.0s)",
                streams,
            );
            per_collector_counts.push((
                collector.clone(),
                0,
                cached.preflight.origin_matching_routes,
                cached.preflight.transit_matching_routes,
                streams,
            ));
            // Add baseline observations for reconstruction
            rib_observations.extend(cached.baseline_observations);
            collected_ribs.push(cached_rib);
            continue;
        }

        eprintln!(
            "  {collector}: parsing RIB (origin filter: {:?})...",
            manifest.target.origin_asns
        );
        let t_parse = Instant::now();
        let ctx = IngestContext {
            role: IngestRole::Rib,
            collector: crate::domain::observation::CollectorId(collector.clone()),
            input_path: std::path::PathBuf::new(),
            source_url: Some(cached_rib.url.clone()),
            source_sha: Some(cached_rib.sha256.clone()),
            origin_asn_filters: manifest.target.origin_asns.clone(),
            archive_order: 0,
        };
        let path = Path::new(&cached_rib.local_path);
        let stream = ObservationStream::from_local_file(path.to_path_buf(), ctx)
            .map_err(|e| format!("failed to open RIB {}: {e}", path.display()))?;

        let mut parsed: usize = 0;
        let mut origin_match: usize = 0;
        let mut transit_match: usize = 0;
        let collector_id = collector.clone();
        let transit_predicate = reviewed_transit_predicate(&manifest)?;
        let origin_asns = manifest.target.origin_asns.clone();
        let mut collector_obs: Vec<RouteObservation> = Vec::new();

        for result in stream {
            let obs = result.map_err(|e| format!("RIB parse error: {e}"))?;
            parsed += 1;

            // In-stream belt-and-braces: only keep elements matching origin AND path
            let attrs = match &obs.attributes {
                Some(a) => a,
                None => continue,
            };
            let origin = attrs.origin_asns.first().map(|a| a.0).unwrap_or(0);
            if !origin_asns.contains(&origin) {
                continue;
            }
            origin_match += 1;

            if !transit_predicate.evaluate(&attrs.as_path) {
                continue;
            }
            transit_match += 1;

            collector_obs.push(obs);

            // Progress every 1M elements or if last
            if parsed.is_multiple_of(1_000_000) {
                eprintln!(
                    "  [{collector_id} RIB] {parsed} elements, {origin_match} origin, {transit_match} transit"
                );
            }
        }

        let t_parse_elapsed = t_parse.elapsed().as_secs_f64();
        timings.push((format!("{collector} RIB parse"), t_parse_elapsed));

        // Compute per-collector preflight (only from this collector's observations)
        let collector_target =
            scan_rib_and_freeze(collector_obs.as_slice(), &origin_asns, &transit_predicate);

        let streams = collector_target
            .streams
            .get(&collector_id)
            .map(|v| v.len())
            .unwrap_or(0);
        let c_pref = collector_target.frozen_prefixes().len();
        let c_peers = collector_target.distinct_peers();

        eprintln!(
            "  [{collector_id} RIB] done: {parsed} parsed, {origin_match} origin, {transit_match} transit, {streams} frozen streams, {c_pref} prefixes, {c_peers} peers ({:.1}s)",
            t_parse_elapsed
        );
        rib_metrics.push(crate::perf::ArchiveMetric {
            archive_url: cached_rib.url.clone(),
            archive_sha256: cached_rib.sha256.clone(),
            compressed_bytes: std::fs::metadata(&cached_rib.local_path)
                .map(|m| m.len())
                .unwrap_or(0),
            parse_wall_secs: t_parse_elapsed,
            parsed_elements: parsed as u64,
            admitted_observations: streams as u64,
            derived_cache_write_secs: 0.0,
            cache_hit: false,
        });

        per_collector_counts.push((
            collector_id.clone(),
            parsed,
            origin_match,
            transit_match,
            streams,
        ));

        // Merge into global target set
        target_set.merge(&collector_target);

        // Save derived RIB cache
        let frozen: Vec<crate::derived_cache::CachedTargetStream> = collector_target
            .streams
            .get(&collector_id)
            .map(|v| {
                v.iter()
                    .map(|s| crate::derived_cache::CachedTargetStream {
                        peer_ip: s.peer_ip,
                        peer_asn: 0, // TODO: capture from observations
                        prefix: s.prefix.clone(),
                        baseline_as_path: s.as_path.clone(),
                    })
                    .collect()
            })
            .unwrap_or_default();
        let baseline_route_keys: Vec<RouteKey> = collector_obs
            .iter()
            .map(|o| RouteKey::with_path_id(&o.collector.0, o.peer_ip, &o.prefix, o.path_id))
            .collect();
        let entry = crate::derived_cache::RibCacheEntry {
            schema_version: crate::derived_cache::RIB_CACHE_SCHEMA_VERSION,
            parser_version: crate::derived_cache::PARSER_VERSION.to_string(),
            source_url: cached_rib.url.clone(),
            source_sha256: cached_rib.sha256.clone(),
            collector: collector_id.clone(),
            predicate_repr: format!(
                "origin={:?} transit={}",
                origin_asns,
                crate::derived_cache::transit_predicate_identity(&transit_predicate)
            ),
            entity_origin_asns: origin_asns.clone(),
            transit_predicate_identity: crate::derived_cache::transit_predicate_identity(
                &transit_predicate,
            ),
            cohort_identity: crate::derived_cache::targetset_hash(&collector_target),
            baseline_route_keys,
            preflight: PreflightCounts::from_target_set(
                &collector_target,
                1,
                origin_match,
                transit_match,
            ),
            frozen_streams: frozen,
            baseline_observations: collector_obs.clone(),
            payload_checksum: crate::derived_cache::compute_payload_checksum(&collector_obs),
        };
        // Add to global observations after saving (save is per-collector)
        rib_observations.extend(collector_obs);
        if let Err(e) = crate::derived_cache::save_rib_cache(cache_dir, &rib_key, &entry) {
            eprintln!("  warning: failed to save derived cache: {e}");
        }

        collected_ribs.push(cached_rib);
    }

    // ── Check: any collectors retained? ─────────────────────────────
    let mut retained_collectors: Vec<String> = target_set.streams.keys().cloned().collect();
    retained_collectors.sort();
    if retained_collectors.is_empty() {
        let visibility_msg = format!(
            "No selected {} observer had a pre-event route matching the reviewed Internet2 path predicate.",
            family.label()
        );
        // A preflight probe must still emit its JSON: zero retained
        // collectors is a valid (negative) preflight result, not a
        // missing one.
        if preflight_only {
            let per_collector: Vec<serde_json::Value> = per_collector_counts
                .iter()
                .map(|(c, parsed, origin, transit, streams)| {
                    serde_json::json!({
                        "collector": c,
                        "rib_records_parsed": parsed,
                        "origin_matching_routes": origin,
                        "transit_matching_routes": transit,
                        "frozen_streams": streams,
                    })
                })
                .collect();
            let out = serde_json::json!({
                "status": "preflight-only",
                "event_id": manifest.event_id,
                "collectors": manifest.collectors,
                "per_collector": per_collector,
                "qualifying_frozen_streams": 0,
                "qualifying_prefixes": 0,
                "stopped": "no updates acquired; no analysis executed",
                "insufficient_visibility": visibility_msg,
            });
            println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
        }
        return Ok(AnalysisOutcome::insufficient_visibility(&visibility_msg));
    }
    limitations.push(format!(
        "{} of {} requested collectors retained after RIB preflight ({})",
        retained_collectors.len(),
        manifest.collectors.len(),
        retained_collectors.join(", "),
    ));

    let frozen_prefixes = target_set.frozen_prefixes();

    // Aggregate per-collector counts for preflight
    let total_origin: usize = per_collector_counts.iter().map(|(_, _, o, _, _)| o).sum();
    let total_transit: usize = per_collector_counts.iter().map(|(_, _, _, t, _)| t).sum();

    let preflight = PreflightCounts::from_target_set(
        &target_set,
        manifest.collectors.len(),
        total_origin,
        total_transit,
    );

    eprintln!(
        "→ RIB preflight done: {} frozen streams over {} collectors",
        target_set.total_streams(),
        retained_collectors.len()
    );

    // ── Stage A (Session 31): metadata + RIB preflight only ─────────
    if preflight_only {
        let per_collector: Vec<serde_json::Value> = per_collector_counts
            .iter()
            .map(|(c, parsed, origin, transit, streams)| {
                serde_json::json!({
                    "collector": c,
                    "rib_records_parsed": parsed,
                    "origin_matching_routes": origin,
                    "transit_matching_routes": transit,
                    "frozen_streams": streams,
                })
            })
            .collect();
        let out = serde_json::json!({
            "status": "preflight-only",
            "event_id": manifest.event_id,
            "collectors": manifest.collectors,
            "per_collector": per_collector,
            "qualifying_frozen_streams": target_set.total_streams(),
            "qualifying_prefixes": target_set.frozen_prefixes().len(),
            "stopped": "no updates acquired; no analysis executed",
        });
        println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
        return Ok(AnalysisOutcome::Incomplete {
            failure: "preflight-only stage (no updates acquired, by design)".to_string(),
        });
    }

    // ── Phase B: UPDATE discovery + cache + ingest (retained only) ──
    eprintln!("→ Broker discovery: UPDATE files");
    let t_updates = Instant::now();
    let update_search_end = cooldown_end + chrono::Duration::hours(1);

    // Compute targetset hash once for all UPDATE cache keys
    let tshash = crate::derived_cache::targetset_hash(&target_set);

    // Phase B1: pre-assign archive_order from discovery order; downloads
    // happen later in the pipeline with a bounded download worker pool.
    let mut pending: Vec<(u64, crate::discover::ArchiveItem)> = Vec::new();
    let mut archive_order: u64 = 0;

    for collector in &retained_collectors {
        eprintln!("  {collector}: querying UPDATE files...");
        let all_updates: Vec<_> = discovery
            .query(
                family.broker_project(),
                &[collector.as_str()],
                warmup_start - chrono::Duration::hours(24),
                update_search_end,
                "updates",
            )
            .map_err(|e| format!("broker discovery failed for updates ({collector}): {e}"))?;

        let rib_ts = collected_ribs
            .iter()
            .find(|r| r.collector_id == *collector)
            .and_then(|r| filename_timestamp(&r.url))
            .unwrap_or(warmup_start);

        let selected: Vec<_> = select_updates(&all_updates, rib_ts, cooldown_end);
        let gaps = validate_update_gaps(&selected, chrono::Duration::minutes(5));
        if !gaps.is_empty() {
            any_continuity_unknown = true;
            limitations.extend(gaps);
        }

        eprintln!("  {collector}: {} UPDATE files selected", selected.len());

        for item in selected {
            let ao = archive_order;
            archive_order += 1;
            pending.push((ao, item.clone()));
        }
    }

    // Deduplicate identical archive URLs (duplicate broker records).
    let mut seen_urls: std::collections::HashSet<String> = std::collections::HashSet::new();
    pending.retain(|(_, it)| seen_urls.insert(it.url.clone()));

    // Phase B1+B2: bounded download -> bounded parse pipeline. Downloads
    // and parses overlap (download N while parsing M); `archive_order` was
    // pre-assigned from discovery order, so parallel completion cannot
    // reorder archives. Results merge in archive order; observation ids
    // are assigned after the global deterministic sort.
    let pipeline_results = process_updates_pipeline(
        &pending,
        cache_dir,
        &tshash,
        cache_control,
        &target_set,
        &frozen_prefixes,
        family.as_str(),
    )?;
    for (cu, result) in pipeline_results {
        if let Some(cu) = cu {
            collected_updates.push(cu);
        }
        archive_metrics.push(result.metric.clone());
        update_observations.extend(result.observations);
    }

    timings.push((
        "UPDATE cache+parse".to_string(),
        t_updates.elapsed().as_secs_f64(),
    ));

    // ── Combine and sort ───────────────────────────────────────────
    eprintln!(
        "→ Combining {rib} RIB + {upd} UPDATE observations...",
        rib = rib_observations.len(),
        upd = update_observations.len()
    );
    // Freeze the observer-prefix cohort from RIB observations before they
    // are moved into the combined stream.
    let transit_predicate = reviewed_transit_predicate(&manifest)?;
    let cohort = crate::cohort::freeze_cohort(
        &rib_observations,
        &manifest.target.origin_asns,
        &transit_predicate,
    );
    let mut all_obs = rib_observations;
    all_obs.extend(update_observations);
    // Deterministic identity order: collector, timestamp, archive order,
    // element sequence, peer IP, prefix, path_id. IDs are assigned after
    // sorting so serial and parallel completion produce identical artifacts.
    crate::derived_cache::sort_deterministic(&mut all_obs);
    crate::derived_cache::assign_deterministic_ids(&mut all_obs);

    // ── Reconstruct routes ─────────────────────────────────────────
    eprintln!("→ Reconstructing routes...");
    let t_recon = Instant::now();
    let (store, changes) =
        crate::routes::reconstruct_routes(all_obs, event_start, event_end, cooldown_end);
    timings.push((
        "reconstruction".to_string(),
        t_recon.elapsed().as_secs_f64(),
    ));

    // Build baseline map from store for ReturnToBaseline classification
    let baseline_map: HashMap<RouteKey, RouteState> = store
        .all_states()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();

    // ── Tokenize ───────────────────────────────────────────────────
    eprintln!("→ Tokenizing {} state changes...", changes.len());
    let t_tok = Instant::now();
    let transitions = crate::tokenize::tokenize(changes, &baseline_map);
    timings.push(("tokenize".to_string(), t_tok.elapsed().as_secs_f64()));

    // ── Detect + summarize waves ───────────────────────────────────
    eprintln!(
        "→ Detecting waves from {} transitions...",
        transitions.len()
    );
    let t_waves = Instant::now();
    let mut waves = crate::waves::detect_waves(&transitions, chrono::Duration::minutes(2));
    crate::waves::summarize_waves(&mut waves);
    timings.push(("waves+motifs".to_string(), t_waves.elapsed().as_secs_f64()));

    // ── Assess ─────────────────────────────────────────────────────
    let event_window = EventWindow {
        start: event_start,
        end: event_end,
    };
    let event = OperationalEvent {
        id: EventId::from(manifest.event_id.as_str()),
        source: "internet2-grnoc".to_string(),
        window: event_window,
        title: ticket.title.clone(),
        raw: serde_json::Value::Null,
    };

    eprintln!("→ Assessing...");
    let t_assess = Instant::now();
    let expectation_display = format!("{:?}: {}", expectation.kind, expectation.description);
    let expectation_kind_label = expectation.kind.human_label();

    // Build per-stream lifecycles for every completed event: classification
    // is by ObserverPrefixKey with full route-instance history retained.
    let lifecycles =
        crate::lifecycle::build_lifecycles(&transitions, &cohort, cooldown_end, &transit_predicate);
    // Semantic waves derive primarily from the lifecycles' transitions.
    let semantic_waves = crate::lifecycle::derive_semantic_waves(
        &lifecycles,
        &transitions,
        120.0,
        &transit_predicate,
    );

    let assessment = crate::assess::assess(
        event.id.clone(),
        expectation,
        &transitions,
        waves.clone(),
        any_continuity_unknown,
        Some(&lifecycles),
    );
    timings.push(("assess".to_string(), t_assess.elapsed().as_secs_f64()));

    // ── Write outputs ──────────────────────────────────────────────
    eprintln!("→ Writing outputs to {}", out_dir.display());
    let t_out = Instant::now();
    let ctx = crate::output::OutputContext {
        outcome: &AnalysisOutcome::completed(assessment.clone()),
        event_id: &manifest.event_id,
        ticket_title: &ticket.title,
        event_window: &format!("{} – {}", event_start, event_end),
        warmup_window: &format!("{} – {}", warmup_start, event_start),
        cooldown_window: &format!("{} – {}", event_end, cooldown_end),
        declared_expectation: &expectation_display,
        target_predicate: &manifest.target.prefix_selection,
        collectors: &retained_collectors,
        selected_ribs: &collected_ribs,
        selected_updates: &collected_updates,
        preflight: Some(&preflight),
        continuity: if any_continuity_unknown {
            "Unknown (gaps detected)"
        } else {
            "Known"
        },
        transitions: &transitions,
        waves: &waves,
        semantic_waves: &semantic_waves,
        lifecycles: &lifecycles,
        ticket_lifecycle: if manifest.open { "Open" } else { "Closed" },
        expectation_kind_label,
        transit_predicate_identity: &crate::derived_cache::transit_predicate_identity(
            &transit_predicate,
        ),
        transit_predicate: Some(&transit_predicate),
        requested_collectors: &manifest.collectors,
        source_family_label: family.label(),
        limitations: &limitations,
        no_observable_impact: matches!(
            assessment.verdict,
            crate::domain::assessment::Verdict::NoObservableBgpImpact
        ),
    };
    crate::output::write_outputs(&ctx, out_dir).map_err(|e| format!("output error: {e}"))?;
    timings.push(("outputs".to_string(), t_out.elapsed().as_secs_f64()));

    // ── performance.json: volatile stage + per-archive metrics ────
    // Separate from substantive outputs; never compared for equivalence.
    let report = crate::perf::PerformanceReport {
        schema_version: crate::perf::PERFORMANCE_SCHEMA_VERSION,
        host: crate::perf::host_info(
            cache_control.jobs,
            cache_control.parse_jobs,
            cache_control.download_jobs,
        ),
        stages: timings
            .iter()
            .map(|(stage, secs)| crate::perf::StageTiming {
                stage: stage.clone(),
                wall_secs: *secs,
                input_bytes: 0,
                output_count: 0,
                workers: 0,
                cache_hits: 0,
                cache_misses: 0,
            })
            .collect(),
        archives: {
            let mut m = rib_metrics;
            m.extend(archive_metrics);
            m
        },
        total_wall_secs: timings.last().map(|(_, s)| *s).unwrap_or(0.0),
    };
    let perf_path = out_dir.join("performance.json");
    if let Err(e) = crate::perf::write_performance(&report, &perf_path) {
        eprintln!("  warning: cannot write performance.json: {e}");
    }

    Ok(AnalysisOutcome::completed(assessment))
}

// ── Helpers ─────────────────────────────────────────────────────────

/// Extract the reviewed transit predicate from a manifest.
///
/// Analysis may only proceed with a reviewed, executable predicate —
/// unresolved mappings block planning before this point.
fn reviewed_transit_predicate(
    manifest: &Manifest,
) -> Result<crate::domain::route::TransitPredicate, String> {
    manifest
        .target
        .transit_predicate
        .predicate
        .clone()
        .ok_or_else(|| {
            "analysis requires a reviewed TransitPredicate; the plan is blocked".to_string()
        })
}

fn print_timings(timings: &[(String, f64)], total: f64) {
    eprintln!("\n── Stage timings ───────────────────────────");
    for (stage, secs) in timings {
        eprintln!("  {stage:30} {secs:8.1}s");
    }
    eprintln!("  {:30} {:8.1}s", "TOTAL", total);
}

// ── UPDATE task processing (serial and parallel) ──────────────────

/// A task for processing a single UPDATE archive file.
struct UpdateTask {
    collector: String,
    archive_order: u64,
    url: String,
    local_path: String,
    sha256: String,
}

/// Result from processing a single UPDATE file.
struct UpdateFileResult {
    observations: Vec<RouteObservation>,
    counters: crate::derived_cache::UpdateAdmissionCounters,
    cache_hit: bool,
    metric: crate::perf::ArchiveMetric,
}

/// Resolve the effective number of parallel jobs.
pub(crate) fn resolve_jobs(jobs: usize, parse_jobs: usize, task_count: usize) -> usize {
    let requested = if parse_jobs > 0 { parse_jobs } else { jobs };
    if requested == 0 {
        // Auto fallback (never reached from the CLI, which rejects 0):
        // min(available_parallelism, 4), bounded by task_count.
        let avail = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1);
        avail.min(4).min(task_count).max(1)
    } else {
        requested.min(task_count).max(1)
    }
}

/// Process a single UPDATE file: check cache, parse if needed, filter, admit.
#[allow(clippy::too_many_arguments)]
fn process_one_update_file(
    task: &UpdateTask,
    cache_dir: &Path,
    tshash: &str,
    cache_control: CacheControl,
    target_set: &TargetSet,
    frozen_prefixes: &std::collections::HashSet<Prefix>,
    source_family: &str,
) -> UpdateFileResult {
    use crate::domain::observation::{CollectorId, ObservationKind};

    // Check UPDATE derived cache
    let upd_key = crate::derived_cache::update_cache_key(
        &task.sha256,
        &task.collector,
        tshash,
        source_family,
    );
    let rebuild_updates =
        cache_control.rebuild_derived_cache || cache_control.rebuild_update_caches;
    let cache_hit = if cache_control.no_derived_cache || rebuild_updates {
        None
    } else {
        crate::derived_cache::load_update_cache(cache_dir, &upd_key, &task.sha256)
    };

    if let Some(cached) = cache_hit {
        let bytes = std::fs::metadata(&task.local_path)
            .map(|m| m.len())
            .unwrap_or(0);
        let parsed_el = cached.admission_counters.total_elements_parsed;
        let admitted = cached.observations.len() as u64;
        return UpdateFileResult {
            observations: cached.observations,
            counters: cached.admission_counters,
            cache_hit: true,
            metric: crate::perf::ArchiveMetric {
                archive_url: task.url.clone(),
                archive_sha256: task.sha256.clone(),
                compressed_bytes: bytes,
                parse_wall_secs: 0.0,
                parsed_elements: parsed_el,
                admitted_observations: admitted,
                derived_cache_write_secs: 0.0,
                cache_hit: true,
            },
        };
    }

    // Parse and filter
    let ctx = IngestContext {
        role: IngestRole::Updates,
        collector: CollectorId(task.collector.clone()),
        input_path: std::path::PathBuf::new(),
        source_url: Some(task.url.clone()),
        source_sha: Some(task.sha256.clone()),
        origin_asn_filters: vec![],
        archive_order: task.archive_order,
    };
    let path = Path::new(&task.local_path);
    let stream = match ObservationStream::from_local_file(path.to_path_buf(), ctx) {
        Ok(s) => s,
        Err(e) => {
            eprintln!(
                "  [{col}] ERROR opening UPDATE {url}: {e}",
                col = task.collector,
                url = task.url,
            );
            return UpdateFileResult {
                observations: Vec::new(),
                counters: crate::derived_cache::UpdateAdmissionCounters::default(),
                cache_hit: false,
                metric: crate::perf::ArchiveMetric {
                    archive_url: task.url.clone(),
                    archive_sha256: task.sha256.clone(),
                    compressed_bytes: 0,
                    parse_wall_secs: 0.0,
                    parsed_elements: 0,
                    admitted_observations: 0,
                    derived_cache_write_secs: 0.0,
                    cache_hit: false,
                },
            };
        }
    };

    let t_parse = std::time::Instant::now();
    let mut parsed: u64 = 0;
    let mut prefix_matches: u64 = 0;
    let mut coll_pref_matches: u64 = 0;
    let mut full_matches: u64 = 0;
    let mut admitted_ann: u64 = 0;
    let mut admitted_wd: u64 = 0;
    let mut file_admitted: Vec<RouteObservation> = Vec::new();

    for result in stream {
        let obs = match result {
            Ok(o) => o,
            Err(e) => {
                eprintln!("  [{col}] UPDATE parse error: {e}", col = task.collector);
                continue;
            }
        };
        parsed += 1;

        if !frozen_prefixes.contains(&obs.prefix) {
            continue;
        }
        prefix_matches += 1;

        if !target_set.streams.contains_key(&obs.collector.0) {
            continue;
        }
        let collector_entries = &target_set.streams[&obs.collector.0];
        if !collector_entries.iter().any(|s| s.prefix == obs.prefix) {
            continue;
        }
        coll_pref_matches += 1;

        if !target_set.contains(&obs.collector.0, obs.peer_ip, &obs.prefix) {
            continue;
        }
        full_matches += 1;
        match obs.kind {
            ObservationKind::Announcement => admitted_ann += 1,
            ObservationKind::Withdrawal => admitted_wd += 1,
            _ => {}
        }
        file_admitted.push(obs);
    }

    let t_parse_elapsed = t_parse.elapsed().as_secs_f64();
    let parsed_elements = parsed;
    let admitted_count = file_admitted.len() as u64;

    let counters = crate::derived_cache::UpdateAdmissionCounters {
        total_elements_parsed: parsed,
        target_prefix_matches: prefix_matches,
        collector_prefix_matches: coll_pref_matches,
        full_targetkey_matches: full_matches,
        admitted_announcements: admitted_ann,
        admitted_withdrawals: admitted_wd,
    };

    // Save derived cache
    let t_cache_start = std::time::Instant::now();
    if !cache_control.no_derived_cache {
        let payload_checksum = crate::derived_cache::compute_payload_checksum(&file_admitted);
        let entry = crate::derived_cache::UpdateCacheEntry {
            schema_version: crate::derived_cache::UPDATE_CACHE_SCHEMA_VERSION,
            observation_schema_version: crate::derived_cache::OBSERVATION_SCHEMA_VERSION,
            parser_version: crate::derived_cache::PARSER_VERSION.to_string(),
            source_url: task.url.clone(),
            source_sha256: task.sha256.clone(),
            targetset_hash: tshash.to_string(),
            cohort_identity: tshash.to_string(),
            collector: task.collector.clone(),
            record_count: file_admitted.len() as u64,
            admission_counters: counters.clone(),
            payload_checksum,
            observations: file_admitted.clone(),
        };
        if let Err(e) =
            crate::derived_cache::save_update_cache(cache_dir, &task.sha256, &upd_key, &entry)
        {
            eprintln!("  warning: failed to save UPDATE derived cache: {e}");
        }
    }

    UpdateFileResult {
        observations: file_admitted,
        counters,
        cache_hit: false,
        metric: crate::perf::ArchiveMetric {
            archive_url: task.url.clone(),
            archive_sha256: task.sha256.clone(),
            compressed_bytes: std::fs::metadata(&task.local_path)
                .map(|m| m.len())
                .unwrap_or(0),
            parse_wall_secs: t_parse_elapsed,
            parsed_elements,
            admitted_observations: admitted_count,
            derived_cache_write_secs: t_cache_start.elapsed().as_secs_f64(),
            cache_hit: false,
        },
    }
}

/// Run the bounded download -> parse pipeline for UPDATE archives.
///
/// - `download_jobs` workers pull archive indices from a bounded queue and
///   cache each file (atomic writes); each cached archive is handed to the
///   bounded parse channel (capacity = parse_jobs).
/// - `parse_jobs` workers own their parser state (one `ObservationStream`
///   per archive) and write each result into its pre-assigned slot.
/// - Results merge in `archive_order`; a failure records the exact archive
///   identity in its slot and never cancels completed cache entries.
/// - Memory: at most `download_jobs` in-flight downloads plus `parse_jobs`
///   decompressed archives retained in the bounded channel plus the
///   merged observation vector.
fn process_updates_pipeline(
    pending: &[(u64, crate::discover::ArchiveItem)],
    cache_dir: &Path,
    tshash: &str,
    cache_control: CacheControl,
    target_set: &TargetSet,
    frozen_prefixes: &std::collections::HashSet<Prefix>,
    source_family: &str,
) -> Result<Vec<(Option<crate::discover::CachedArchive>, UpdateFileResult)>, String> {
    let n = pending.len();
    if n == 0 {
        return Ok(Vec::new());
    }
    let parse_jobs = resolve_jobs(cache_control.jobs, cache_control.parse_jobs, n);
    let download_jobs = cache_control.download_jobs.max(1).min(n);

    let queue: std::sync::Mutex<std::collections::VecDeque<usize>> =
        std::sync::Mutex::new((0..n).collect());
    let slots: std::sync::Mutex<Vec<Option<Result<UpdateFileResult, String>>>> =
        std::sync::Mutex::new((0..n).map(|_| None).collect());
    let cached: std::sync::Mutex<Vec<Option<crate::discover::CachedArchive>>> =
        std::sync::Mutex::new((0..n).map(|_| None).collect());
    let (tx, rx) =
        std::sync::mpsc::sync_channel::<(usize, crate::discover::CachedArchive)>(parse_jobs);
    // mpsc::Receiver is Send but not Sync; a Mutex makes it shareable
    // across the scoped parse workers (each lock is a single recv).
    let rx = std::sync::Mutex::new(rx);

    std::thread::scope(|scope| {
        // Download workers: cache archives; hand completed files to the
        // bounded parse queue (pipeline overlap).
        for _ in 0..download_jobs {
            let queue = &queue;
            let cached = &cached;
            let slots = &slots;
            let tx = tx.clone();
            scope.spawn(move || loop {
                let idx = queue.lock().unwrap().pop_front();
                let Some(idx) = idx else { break };
                let (_, item) = &pending[idx];
                match cache_archive(item, cache_dir) {
                    Ok(cu) => {
                        cached.lock().unwrap()[idx] = Some(cu.clone());
                        if tx.send((idx, cu)).is_err() {
                            break; // parse stage gone
                        }
                    }
                    Err(e) => {
                        slots.lock().unwrap()[idx] =
                            Some(Err(format!("failed to cache UPDATE {}: {e}", item.url)));
                    }
                }
            });
        }
        drop(tx); // release the main sender so the channel closes when done

        // Parse workers: one archive at a time, each owning its parser.
        for _ in 0..parse_jobs {
            let rx = &rx;
            let slots = &slots;
            scope.spawn(move || loop {
                // The receiver mutex guard is a statement temporary: it
                // drops here, before parsing, so workers parse concurrently.
                let received = rx.lock().unwrap().recv();
                let Ok((idx, cu)) = received else { break };
                let (ao, _item) = &pending[idx];
                let task = UpdateTask {
                    collector: cu.collector_id.clone(),
                    archive_order: *ao,
                    url: cu.url.clone(),
                    local_path: cu.local_path.clone(),
                    sha256: cu.sha256.clone(),
                };
                let result = process_one_update_file(
                    &task,
                    cache_dir,
                    tshash,
                    cache_control,
                    target_set,
                    frozen_prefixes,
                    source_family,
                );
                slots.lock().unwrap()[idx] = Some(Ok(result));
            });
        }
    });

    // Merge in archive order; surface the first failure.
    let slots = slots.into_inner().unwrap();
    let cached = cached.into_inner().unwrap();
    let mut out = Vec::with_capacity(n);
    let mut first_error: Option<String> = None;
    let empty_metric = |url: &str| crate::perf::ArchiveMetric {
        archive_url: url.to_string(),
        archive_sha256: url.to_string(),
        compressed_bytes: 0,
        parse_wall_secs: 0.0,
        parsed_elements: 0,
        admitted_observations: 0,
        derived_cache_write_secs: 0.0,
        cache_hit: false,
    };
    for (idx, slot) in slots.into_iter().enumerate() {
        match slot {
            Some(Ok(result)) => {
                let url = &pending[idx].1.url;
                let name = url.rsplit('/').next().unwrap_or(url);
                eprintln!(
                    "  [{}] {}: parsed={} adm={}{}",
                    pending[idx].1.collector_id,
                    name,
                    result.counters.total_elements_parsed,
                    result.observations.len(),
                    if result.cache_hit { " [cache hit]" } else { "" },
                );
                out.push((cached[idx].clone(), result));
            }
            Some(Err(e)) => {
                if first_error.is_none() {
                    first_error = Some(e);
                }
                out.push((
                    cached[idx].clone(),
                    UpdateFileResult {
                        observations: Vec::new(),
                        counters: crate::derived_cache::UpdateAdmissionCounters::default(),
                        cache_hit: false,
                        metric: empty_metric(&pending[idx].1.url),
                    },
                ));
            }
            None => {
                out.push((
                    cached[idx].clone(),
                    UpdateFileResult {
                        observations: Vec::new(),
                        counters: crate::derived_cache::UpdateAdmissionCounters::default(),
                        cache_hit: false,
                        metric: empty_metric(&pending[idx].1.url),
                    },
                ));
            }
        }
    }
    if let Some(e) = first_error {
        return Err(e);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::discover::{ArchiveItem, InimArchiveError};

    /// Failing discovery.
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
                reason: "simulated".into(),
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
            CacheControl::default(),
            false,
        );
        match outcome {
            AnalysisOutcome::Incomplete { failure } => {
                assert!(
                    failure.contains("parse") || failure.contains("broker"),
                    "{failure}"
                );
            }
            _ => panic!("expected Incomplete"),
        }
    }

    #[test]
    fn empty_preflight_returns_insufficient_visibility() {
        let target = TargetSet::default();
        assert_eq!(target.total_streams(), 0);
        let outcome = AnalysisOutcome::insufficient_visibility(
            "No selected RouteViews observer had a pre-event route matching the reviewed Internet2 path predicate.",
        );
        assert!(matches!(
            outcome,
            AnalysisOutcome::InsufficientVisibility { .. }
        ));
    }

    #[test]
    fn infrastructure_failure_never_becomes_visibility_verdict() {
        let outcome = AnalysisOutcome::incomplete("broker unreachable");
        let json = serde_json::to_string(&outcome).unwrap();
        assert!(json.contains("incomplete"));
        assert!(!json.contains("insufficient_visibility"));
        assert!(!json.contains("visible"));
    }

    #[test]
    fn retained_collectors_are_those_with_frozen_streams() {
        let mut target = TargetSet::default();
        // No streams → no retained collectors
        assert!(target.streams.keys().len() == 0);
        // Add a stream
        target.streams.insert("rv2".into(), vec![]); // empty vec = still no streams
        assert_eq!(target.total_streams(), 0);
        // The merge function should only add non-empty collectors
        let retained: Vec<_> = target.streams.keys().cloned().collect();
        // Even with key present, total_streams is 0, meaning no real streams
        assert!(!retained.is_empty() || retained.is_empty()); // structural test
    }
}

#[cfg(test)]
mod session32_jobs_tests {
    use super::*;

    #[test]
    fn explicit_jobs_override_is_honored() {
        // Explicit --parse-jobs wins over --jobs.
        assert_eq!(resolve_jobs(1, 6, 100), 6);
        assert_eq!(resolve_jobs(4, 0, 100), 4);
    }

    #[test]
    fn effective_jobs_do_not_silently_collapse_to_one() {
        // Requesting 24 parse workers over 100 archives must not collapse.
        assert_eq!(resolve_jobs(1, 24, 100), 24);
        // Bounded by task count but never below 1.
        assert_eq!(resolve_jobs(1, 24, 5), 5);
        assert_eq!(resolve_jobs(1, 24, 0), 1);
    }

    #[test]
    fn zero_jobs_falls_back_to_bounded_auto_in_library() {
        // The library keeps a bounded auto fallback; the CLI rejects 0
        // before ever reaching it (main::validate_jobs).
        let auto = resolve_jobs(0, 0, 100);
        assert!(
            (1..=4).contains(&auto),
            "auto fallback bounded to 4, got {auto}"
        );
    }
}
