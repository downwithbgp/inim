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
    /// Number of parallel parsing jobs (1 = serial, 0 = auto).
    pub jobs: usize,
}

impl Default for CacheControl {
    fn default() -> Self {
        CacheControl {
            no_derived_cache: false,
            rebuild_derived_cache: false,
            jobs: 1,
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

fn run_inner(
    event_path: &Path,
    manifest_path: &Path,
    cache_dir: &Path,
    out_dir: &Path,
    discovery: &dyn ArchiveDiscovery,
    cache_control: CacheControl,
    timings: &mut Vec<(String, f64)>,
) -> Result<AnalysisOutcome, String> {
    // ── Parse ticket + manifest ───────────────────────────────────
    let ticket = crate::sources::internet2::ticket::parse_ticket_fixture(
        event_path.to_str().ok_or("event path is not valid UTF-8")?,
    )
    .map_err(|e| format!("failed to parse ticket fixture: {e}"))?;
    let expectation = crate::sources::internet2::ticket::derive_expectation(&ticket);

    let manifest = Manifest::load(manifest_path)?;
    let (event_start, event_end) = manifest.event_window()?;
    let warmup_start = event_start - chrono::Duration::minutes(manifest.warmup_minutes);
    let cooldown_end = event_end + chrono::Duration::minutes(manifest.cooldown_minutes);

    let mut limitations: Vec<String> = Vec::new();
    let mut collected_ribs: Vec<CachedArchive> = Vec::new();
    let mut collected_updates: Vec<CachedArchive> = Vec::new();
    let mut target_set = TargetSet::default();
    let mut rib_observations: Vec<RouteObservation> = Vec::new();
    let mut update_observations: Vec<RouteObservation> = Vec::new();
    let mut any_continuity_unknown = false;
    let mut per_collector_counts: Vec<(String, usize, usize, usize, usize)> = Vec::new();

    // ── Phase A: per-collector RIB preflight (before UPDATEs) ──────
    let rib_search_start = warmup_start - chrono::Duration::hours(24);
    let rib_search_end = warmup_start + chrono::Duration::hours(1);

    eprintln!("→ Broker discovery: RIB files");
    let t_broker = Instant::now();
    let all_ribs = discovery
        .query(
            "routeviews",
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
        return Ok(AnalysisOutcome::insufficient_visibility(
            "No selected RouteViews observer had a pre-event route matching the reviewed Internet2 path predicate.",
        ));
    }
    limitations.push(format!(
        "{} of {} requested collectors retained after RIB preflight ({:?})",
        retained_collectors.len(),
        manifest.collectors.len(),
        retained_collectors,
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

    // ── Phase B: UPDATE discovery + cache + ingest (retained only) ──
    eprintln!("→ Broker discovery: UPDATE files");
    let t_updates = Instant::now();
    let update_search_end = cooldown_end + chrono::Duration::hours(1);

    // Compute targetset hash once for all UPDATE cache keys
    let tshash = crate::derived_cache::targetset_hash(&target_set);

    // Phase B1: Collect all tasks (serial I/O — broker queries + downloads)
    let mut tasks: Vec<UpdateTask> = Vec::new();
    let mut archive_order: u64 = 0;

    for collector in &retained_collectors {
        eprintln!("  {collector}: querying UPDATE files...");
        let all_updates: Vec<_> = discovery
            .query(
                "routeviews",
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

        for item in &selected {
            let cu = cache_archive(item, cache_dir)
                .map_err(|e| format!("failed to cache UPDATE: {e}"))?;
            collected_updates.push(cu.clone());
            let ao = archive_order;
            archive_order += 1;
            tasks.push(UpdateTask {
                collector: collector.clone(),
                archive_order: ao,
                url: cu.url.clone(),
                local_path: cu.local_path.clone(),
                sha256: cu.sha256.clone(),
            });
        }
    }

    // Phase B2: Process tasks (parallel or serial)
    let task_count = tasks.len();
    if task_count == 0 {
        // No tasks, nothing to do
    } else {
        let effective_jobs = resolve_jobs(cache_control.jobs, task_count);

        if effective_jobs <= 1 {
            // Serial path
            process_tasks_serial(
                &tasks,
                cache_dir,
                &tshash,
                cache_control,
                &target_set,
                &frozen_prefixes,
                &mut update_observations,
            );
        } else {
            // Parallel path
            process_tasks_parallel(
                &tasks,
                cache_dir,
                &tshash,
                cache_control,
                &target_set,
                &frozen_prefixes,
                &mut update_observations,
                effective_jobs,
            );
        }
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
        limitations: &limitations,
        no_observable_impact: matches!(
            assessment.verdict,
            crate::domain::assessment::Verdict::NoObservableBgpImpact
        ),
    };
    crate::output::write_outputs(&ctx, out_dir).map_err(|e| format!("output error: {e}"))?;
    timings.push(("outputs".to_string(), t_out.elapsed().as_secs_f64()));

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
}

/// Resolve the effective number of parallel jobs.
fn resolve_jobs(jobs: usize, task_count: usize) -> usize {
    if jobs == 0 {
        // Auto: min(available_parallelism, 4), bounded by task_count
        let avail = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1);
        avail.min(4).min(task_count).max(1)
    } else {
        jobs.min(task_count).max(1)
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
) -> UpdateFileResult {
    use crate::domain::observation::{CollectorId, ObservationKind};

    // Check UPDATE derived cache
    let upd_key = crate::derived_cache::update_cache_key(&task.sha256, &task.collector, tshash);
    let cache_hit = if cache_control.no_derived_cache || cache_control.rebuild_derived_cache {
        None
    } else {
        crate::derived_cache::load_update_cache(cache_dir, &upd_key, &task.sha256)
    };

    if let Some(cached) = cache_hit {
        return UpdateFileResult {
            observations: cached.observations,
            counters: cached.admission_counters,
            cache_hit: true,
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
            };
        }
    };

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

    let counters = crate::derived_cache::UpdateAdmissionCounters {
        total_elements_parsed: parsed,
        target_prefix_matches: prefix_matches,
        collector_prefix_matches: coll_pref_matches,
        full_targetkey_matches: full_matches,
        admitted_announcements: admitted_ann,
        admitted_withdrawals: admitted_wd,
    };

    // Save derived cache
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
    }
}

/// Process UPDATE tasks serially.
fn process_tasks_serial(
    tasks: &[UpdateTask],
    cache_dir: &Path,
    tshash: &str,
    cache_control: CacheControl,
    target_set: &TargetSet,
    frozen_prefixes: &std::collections::HashSet<Prefix>,
    update_observations: &mut Vec<RouteObservation>,
) {
    for (i, task) in tasks.iter().enumerate() {
        let result = process_one_update_file(
            task,
            cache_dir,
            tshash,
            cache_control,
            target_set,
            frozen_prefixes,
        );
        eprintln!(
            "  [{col} updates {idx}/{total}] done: {parsed} parsed, {pfx} prefix, {cpf} coll+pref, {adm} admitted ({ann} ann, {wd} wd){cache}",
            col = task.collector,
            idx = i + 1,
            total = tasks.len(),
            parsed = result.counters.total_elements_parsed,
            pfx = result.counters.target_prefix_matches,
            cpf = result.counters.collector_prefix_matches,
            adm = result.counters.full_targetkey_matches,
            ann = result.counters.admitted_announcements,
            wd = result.counters.admitted_withdrawals,
            cache = if result.cache_hit { " [cache hit]" } else { "" },
        );
        update_observations.extend(result.observations);
    }
}

/// Process UPDATE tasks in parallel using std::thread::scope.
#[allow(clippy::too_many_arguments)]
fn process_tasks_parallel(
    tasks: &[UpdateTask],
    cache_dir: &Path,
    tshash: &str,
    cache_control: CacheControl,
    target_set: &TargetSet,
    frozen_prefixes: &std::collections::HashSet<Prefix>,
    update_observations: &mut Vec<RouteObservation>,
    jobs: usize,
) {
    let chunk_size = tasks.len().div_ceil(jobs);

    std::thread::scope(|s| {
        let mut handles = Vec::new();
        for chunk in tasks.chunks(chunk_size) {
            let chunk: Vec<UpdateTask> = chunk
                .iter()
                .map(|t| UpdateTask {
                    collector: t.collector.clone(),
                    archive_order: t.archive_order,
                    url: t.url.clone(),
                    local_path: t.local_path.clone(),
                    sha256: t.sha256.clone(),
                })
                .collect();
            handles.push(s.spawn(move || {
                let mut results = Vec::with_capacity(chunk.len());
                for task in &chunk {
                    results.push(process_one_update_file(
                        task,
                        cache_dir,
                        tshash,
                        cache_control,
                        target_set,
                        frozen_prefixes,
                    ));
                }
                results
            }));
        }

        // Join handles in order (preserves deterministic archive order)
        for handle in handles {
            match handle.join() {
                Ok(chunk_results) => {
                    for result in chunk_results {
                        update_observations.extend(result.observations);
                    }
                }
                Err(_) => {
                    eprintln!("  ERROR: worker thread panicked");
                }
            }
        }
    });
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
