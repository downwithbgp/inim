//! Output artifacts — report.txt, report.json, archive_manifest.json,
//! evidence_appendix.jsonl, and limitations.json.
//!
//! All output is deterministic; no timestamps other than the analysis
//! generation time. Template language is cautious and observer-scoped.

use serde::Serialize;
use std::path::{Path, PathBuf};

use crate::discover::CachedArchive;
use crate::domain::route::{AnalysisPhase, RouteTransition};
use crate::domain::wave::ImpactWave;
use crate::outcome::AnalysisOutcome;
use crate::target::PreflightCounts;

/// Context for all output artifacts.
pub struct OutputContext<'a> {
    pub outcome: &'a AnalysisOutcome,
    pub event_id: &'a str,
    pub ticket_title: &'a str,
    pub event_window: &'a str,
    pub warmup_window: &'a str,
    pub cooldown_window: &'a str,
    pub declared_expectation: &'a str,
    pub target_predicate: &'a str,
    pub collectors: &'a [String],
    pub selected_ribs: &'a [CachedArchive],
    pub selected_updates: &'a [CachedArchive],
    pub preflight: Option<&'a PreflightCounts>,
    pub continuity: &'a str,
    pub transitions: &'a [RouteTransition],
    pub waves: &'a [ImpactWave],
    /// Semantic waves derived from ObserverPrefixKey lifecycles.
    pub semantic_waves: &'a [crate::lifecycle::SemanticWave],
    /// Observer-prefix stream lifecycles.
    pub lifecycles: &'a [crate::lifecycle::StreamLifecycle],
    /// Ticket lifecycle (Open/Closed) for the report.
    pub ticket_lifecycle: &'a str,
    /// Canonical TransitPredicate identity used for the analysis.
    pub transit_predicate_identity: &'a str,
    pub limitations: &'a [String],
    /// Whether the report may use the NoObservableBgpImpact wording.
    pub no_observable_impact: bool,
}

/// Write all output artifacts to `out_dir`. Returns the list of created files.
pub fn write_outputs(ctx: &OutputContext, out_dir: &Path) -> Result<Vec<PathBuf>, String> {
    std::fs::create_dir_all(out_dir)
        .map_err(|e| format!("cannot create output dir {}: {e}", out_dir.display()))?;

    let mut files = Vec::new();

    let report_txt = out_dir.join("report.txt");
    write_report_txt(ctx, &report_txt)?;
    files.push(report_txt);

    let report_json = out_dir.join("report.json");
    write_report_json(ctx, &report_json)?;
    files.push(report_json);

    // Archive manifest (only when datasets exist)
    if !ctx.selected_ribs.is_empty() || !ctx.selected_updates.is_empty() {
        let manifest_path = out_dir.join("archive_manifest.json");
        write_archive_manifest(ctx, &manifest_path)?;
        files.push(manifest_path);
    }

    // Evidence appendix — always written for Completed outcomes
    if matches!(ctx.outcome, AnalysisOutcome::Completed { .. }) {
        let appendix_path = out_dir.join("evidence_appendix.jsonl");
        write_evidence_appendix(ctx, &appendix_path)?;
        files.push(appendix_path);
    }

    // Semantic waves artifact — always written for Completed outcomes.
    if matches!(ctx.outcome, AnalysisOutcome::Completed { .. }) {
        let waves_path = out_dir.join("semantic_waves.json");
        write_semantic_waves(ctx, &waves_path)?;
        files.push(waves_path);

        // Withdrawal audit artifact.
        let audit_path = out_dir.join("withdrawal_audit.json");
        write_withdrawal_audit(ctx, &audit_path)?;
        files.push(audit_path);

        // Lifecycle artifact.
        let lifecycle_path = out_dir.join("lifecycle.json");
        write_lifecycle_artifact(ctx, &lifecycle_path)?;
        files.push(lifecycle_path);
    }

    let limitations_path = out_dir.join("limitations.json");
    write_limitations(ctx, &limitations_path)?;
    files.push(limitations_path);

    Ok(files)
}

// ── report.txt ─────────────────────────────────────────────────────

fn write_report_txt(ctx: &OutputContext, path: &Path) -> Result<(), String> {
    use crate::lifecycle::StreamCategory;

    let mut buf = String::new();

    push_ln(&mut buf, "══════════════════════════════════════════");
    push_ln(&mut buf, "  INTERNET IMPACT ANALYSIS");
    push_ln(&mut buf, "══════════════════════════════════════════");
    push_ln(&mut buf, "");
    push_ln(&mut buf, &format!("Event:        {}", ctx.event_id));
    push_ln(&mut buf, &format!("Title:        {}", ctx.ticket_title));
    push_ln(
        &mut buf,
        &format!("Report schema: v{}", crate::schema::REPORT_SCHEMA_VERSION),
    );
    push_ln(&mut buf, "");

    // ── 8.1 Observed event signature ────────────────────────────
    push_ln(&mut buf, "── Observed event signature ────────────────");
    push_ln(
        &mut buf,
        &format!("  Ticket expectation:      {}", ctx.declared_expectation),
    );
    push_ln(
        &mut buf,
        &format!("  Ticket lifecycle:        {}", ctx.ticket_lifecycle),
    );
    push_ln(
        &mut buf,
        &format!(
            "  Transit predicate:       {}",
            ctx.transit_predicate_identity
        ),
    );
    push_ln(
        &mut buf,
        &format!("  Analysis window (UTC):   {}", ctx.event_window),
    );
    push_ln(
        &mut buf,
        &format!("  Warmup:                  {}", ctx.warmup_window),
    );
    push_ln(
        &mut buf,
        &format!("  Cooldown:                {}", ctx.cooldown_window),
    );
    push_ln(
        &mut buf,
        &format!(
            "  Observer scope:          {} collectors ({})",
            ctx.collectors.len(),
            ctx.collectors.join(", ")
        ),
    );
    push_ln(&mut buf, "");

    let lcs = ctx.lifecycles;
    let baseline_streams = lcs.len();
    let baseline_instances: usize = lcs.iter().map(|l| l.baseline_instance_count).sum();
    let multi_instance = lcs
        .iter()
        .filter(|l| {
            l.baseline_instance_count > 1 || l.total_route_instances > l.baseline_instance_count
        })
        .count();
    let unchanged = lcs
        .iter()
        .filter(|l| l.category == StreamCategory::Unchanged)
        .count();
    let prepend_only = lcs
        .iter()
        .filter(|l| l.category == StreamCategory::PrependOnly)
        .count();
    let still_via = lcs
        .iter()
        .filter(|l| l.category == StreamCategory::PathChangedStillViaTransit)
        .count();
    let departed = lcs
        .iter()
        .filter(|l| l.category == StreamCategory::DepartedTransitPath)
        .count();
    let withdrawn = lcs.iter().filter(|l| l.was_withdrawn).count();
    let restored = lcs.iter().filter(|l| l.flags.restored).count();
    let unresolved = lcs.iter().filter(|l| l.flags.not_restored).count();
    let ambiguous = lcs.iter().filter(|l| l.flags.add_path_ambiguous).count();
    let retaining = unchanged + prepend_only + still_via;
    let material_changes = ctx
        .transitions
        .iter()
        .filter(|t| t.effects.material_path_changed)
        .count();

    push_ln(&mut buf, "  ── Observer scope (streams and instances) ──");
    push_ln(
        &mut buf,
        &format!("    Baseline observer-prefix streams: {}", baseline_streams),
    );
    push_ln(
        &mut buf,
        &format!(
            "    Baseline route instances:          {}",
            baseline_instances
        ),
    );
    push_ln(
        &mut buf,
        &format!("    Multiple-instance streams:          {}", multi_instance),
    );
    push_ln(&mut buf, "");
    push_ln(&mut buf, "  ── Stream lifecycle ─────────────────────");
    push_ln(
        &mut buf,
        &format!("    Unchanged streams:                  {}", unchanged),
    );
    push_ln(
        &mut buf,
        &format!("    Prepend-only streams:               {}", prepend_only),
    );
    push_ln(
        &mut buf,
        &format!(
            "    Material path changes (transitions):{}",
            material_changes
        ),
    );
    push_ln(
        &mut buf,
        &format!("    Streams retaining transit:          {}", retaining),
    );
    push_ln(
        &mut buf,
        &format!("    Streams departing transit:          {}", departed),
    );
    push_ln(
        &mut buf,
        &format!("    Withdrawn streams:                  {}", withdrawn),
    );
    push_ln(
        &mut buf,
        &format!("    Restored streams:                   {}", restored),
    );
    push_ln(
        &mut buf,
        &format!("    Unresolved streams:                 {}", unresolved),
    );
    push_ln(
        &mut buf,
        &format!("    ADD-PATH ambiguous streams:         {}", ambiguous),
    );
    push_ln(&mut buf, "");

    if !ctx.semantic_waves.is_empty() {
        push_ln(&mut buf, "  ── Semantic waves ────────────────────────");
        for w in ctx.semantic_waves {
            push_ln(
                &mut buf,
                &format!(
                    "    {} {}  {} – {} ({} streams, {} route instances, peak {} – {})",
                    w.id,
                    w.label.as_str(),
                    w.start.format("%H:%M:%S"),
                    w.end.format("%H:%M:%S"),
                    w.stream_count,
                    w.route_instance_count,
                    w.peak_start.format("%H:%M:%S"),
                    w.peak_end.format("%H:%M:%S"),
                ),
            );
        }
        push_ln(&mut buf, "");
    }

    push_ln(&mut buf, "  ── Final impact assessment ───────────────");
    match ctx.outcome {
        AnalysisOutcome::Completed { assessment } => {
            push_ln(&mut buf, &format!("    Verdict: {}", assessment.verdict));
            for ev in &assessment.evidence {
                push_ln(&mut buf, &format!("    - {}", ev.description));
            }
            if ctx.no_observable_impact {
                push_ln(&mut buf, "");
                push_ln(
                    &mut buf,
                    "    No route-state changes were observed among the selected RouteViews",
                );
                push_ln(
                    &mut buf,
                    "    observer-prefix streams. This is consistent with the",
                );
                push_ln(&mut buf, "    redundant-attachment expectation.");
            }
        }
        AnalysisOutcome::InsufficientVisibility { reason } => {
            push_ln(&mut buf, "    INSUFFICIENT VISIBILITY");
            push_ln(&mut buf, &format!("    {reason}"));
        }
        AnalysisOutcome::Incomplete { failure } => {
            push_ln(&mut buf, "    INCOMPLETE");
            push_ln(&mut buf, &format!("    {failure}"));
        }
    }
    push_ln(&mut buf, "");

    // ── 8.2 Observable mechanism hints ───────────────────────────
    push_ln(&mut buf, "── Observable mechanism hints ──────────────");
    let gshut_streams = lcs.iter().filter(|l| l.graceful_shutdown_seen).count();
    if gshut_streams > 0 {
        push_ln(
            &mut buf,
            &format!(
                "  RFC 8326 GRACEFUL_SHUTDOWN community (65535:0) was observed on {gshut_streams} selected observer-prefix streams."
            ),
        );
        // GSHUT timing hints.
        let with_timing: Vec<_> = lcs
            .iter()
            .filter(|l| l.graceful_shutdown_seen)
            .map(|l| {
                let first = l
                    .first_gshut_timestamp
                    .map(|t| t.format("%H:%M:%S").to_string())
                    .unwrap_or_else(|| "?".into());
                let last = l
                    .last_gshut_timestamp
                    .map(|t| t.format("%H:%M:%S").to_string())
                    .unwrap_or_else(|| "?".into());
                format!(
                    "    {}:{} {} first={first} last={last} before-withdrawal={} before-replacement={}",
                    l.collector, l.peer_ip, l.prefix, l.gshut_before_withdrawal, l.gshut_before_path_change
                )
            })
            .collect();
        for line in with_timing {
            push_ln(&mut buf, &line);
        }
    } else {
        push_ln(
            &mut buf,
            "  No RFC 8326 GRACEFUL_SHUTDOWN community reached the selected observers.",
        );
        push_ln(
            &mut buf,
            "  Its absence does not establish that graceful shutdown was not used.",
        );
    }
    let community_only = ctx
        .transitions
        .iter()
        .filter(|t| t.effects.communities_changed && !t.effects.material_path_changed)
        .count();
    push_ln(
        &mut buf,
        &format!("  Community-only changes (no path change): {community_only} transition(s)."),
    );
    push_ln(
        &mut buf,
        "  RFC 9003: administrative-shutdown message not observable from these remote collector sessions.",
    );
    push_ln(
        &mut buf,
        "  RFC 8327: operational intent not directly observable.",
    );
    push_ln(
        &mut buf,
        "  Graceful Restart: negotiated session capability/state not directly observable from this dataset.",
    );
    push_ln(
        &mut buf,
        "  Mechanism hints do not change the impact assessment by themselves.",
    );
    push_ln(&mut buf, "");

    // ── 8.3 Limitations ─────────────────────────────────────────
    push_ln(&mut buf, "── Limitations ─────────────────────────────");
    push_ln(
        &mut buf,
        "  • Selected collectors do not provide global visibility.",
    );
    push_ln(&mut buf, "  • BGP route state is not traffic measurement.");
    push_ln(&mut buf, "  • Local session state is not observed.");
    push_ln(&mut buf, "  • Physical-link state is not observed.");
    push_ln(
        &mut buf,
        "  • Absent communities do not prove a mechanism was unused.",
    );
    push_ln(
        &mut buf,
        "  • Event declarations and BGP changes establish temporal association, not automatic causation.",
    );
    for lim in ctx.limitations {
        push_ln(&mut buf, &format!("  • {lim}"));
    }
    push_ln(&mut buf, "");

    push_ln(
        &mut buf,
        "This analysis uses control-plane observations from selected RouteViews",
    );
    push_ln(
        &mut buf,
        "collectors only. Conclusions are observer-scoped and do not claim",
    );
    push_ln(
        &mut buf,
        "global reachability or physical-layer attribution.",
    );

    std::fs::write(path, buf).map_err(|e| format!("cannot write {}: {e}", path.display()))
}

// ── report.json ─────────────────────────────────────────────────────

#[derive(Serialize)]
struct JsonReport {
    schema_version: u32,
    event_id: String,
    observed_event_signature: serde_json::Value,
    observable_mechanism_hints: serde_json::Value,
    limitations: Vec<String>,
    outcome: serde_json::Value,
    transitions: JsonTransitionStats,
    waves: Vec<JsonWaveSummary>,
}

#[derive(Serialize)]
struct JsonTransitionStats {
    total: usize,
    event_window: usize,
    cooldown: usize,
}

#[derive(Serialize)]
struct JsonWaveSummary {
    id: usize,
    label: String,
    start: String,
    end: String,
    affected_prefix_count: usize,
    motif_id: Option<String>,
}
fn write_report_json(ctx: &OutputContext, path: &Path) -> Result<(), String> {
    use crate::lifecycle::StreamCategory;

    let outcome_json = serde_json::to_value(ctx.outcome).unwrap_or_default();
    let event_count = ctx
        .transitions
        .iter()
        .filter(|t| matches!(t.phase, AnalysisPhase::Event))
        .count();
    let cooldown_count = ctx
        .transitions
        .iter()
        .filter(|t| matches!(t.phase, AnalysisPhase::Cooldown))
        .count();

    let lcs = ctx.lifecycles;
    let signature = serde_json::json!({
        "ticket_expectation": ctx.declared_expectation,
        "ticket_lifecycle": ctx.ticket_lifecycle,
        "transit_predicate": ctx.transit_predicate_identity,
        "analysis_window_utc": ctx.event_window,
        "observer_scope": {
            "collectors": ctx.collectors,
            "baseline_observer_prefix_streams": lcs.len(),
            "baseline_route_instances": lcs.iter().map(|l| l.baseline_instance_count).sum::<usize>(),
            "multiple_instance_streams": lcs.iter().filter(|l| l.baseline_instance_count > 1 || l.total_route_instances > l.baseline_instance_count).count(),
        },
        "stream_lifecycle": {
            "unchanged": lcs.iter().filter(|l| l.category == StreamCategory::Unchanged).count(),
            "prepend_only": lcs.iter().filter(|l| l.category == StreamCategory::PrependOnly).count(),
            "material_path_changes": ctx.transitions.iter().filter(|t| t.effects.material_path_changed).count(),
            "streams_retaining_transit": lcs.iter().filter(|l| matches!(l.category, StreamCategory::Unchanged | StreamCategory::PrependOnly | StreamCategory::PathChangedStillViaTransit)).count(),
            "streams_departing_transit": lcs.iter().filter(|l| l.category == StreamCategory::DepartedTransitPath).count(),
            "withdrawn_streams": lcs.iter().filter(|l| l.was_withdrawn).count(),
            "restored_streams": lcs.iter().filter(|l| l.flags.restored).count(),
            "unresolved_streams": lcs.iter().filter(|l| l.flags.not_restored).count(),
            "add_path_ambiguous_streams": lcs.iter().filter(|l| l.flags.add_path_ambiguous).count(),
        },
        "semantic_waves": ctx.semantic_waves.iter().map(|w| serde_json::json!({
            "id": w.id,
            "label": w.label.as_str(),
            "start": w.start,
            "peak_start": w.peak_start,
            "peak_end": w.peak_end,
            "end": w.end,
            "stream_count": w.stream_count,
            "route_instance_count": w.route_instance_count,
            "prefixes": w.prefixes,
            "peers": w.peers,
            "facets": serde_json::to_value(&w.facets).unwrap_or_default(),
            "event_relative": serde_json::to_value(&w.event_relative).unwrap_or_default(),
            "representative_before": w.representative_before,
            "representative_after": w.representative_after,
            "evidence_refs": w.evidence_refs.iter().map(|e| e.observation_id.0).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
        "final_impact_assessment": serde_json::to_value(ctx.outcome).unwrap_or_default(),
    });

    let gshut_streams = lcs.iter().filter(|l| l.graceful_shutdown_seen).count();
    let mechanism_hints = serde_json::json!({
        "rfc8326": {
            "gshut_streams": gshut_streams,
            "statement": if gshut_streams > 0 {
                format!("RFC 8326 GRACEFUL_SHUTDOWN community was observed on {gshut_streams} selected observer-prefix streams.")
            } else {
                "No RFC 8326 GRACEFUL_SHUTDOWN community reached the selected observers. Its absence does not establish that graceful shutdown was not used.".to_string()
            },
            "gshut_timing": lcs.iter().filter(|l| l.graceful_shutdown_seen).map(|l| serde_json::json!({
                "stream": format!("{}:{} {}", l.collector, l.peer_ip, l.prefix),
                "first": l.first_gshut_timestamp,
                "last": l.last_gshut_timestamp,
                "before_withdrawal": l.gshut_before_withdrawal,
                "before_path_replacement": l.gshut_before_path_change,
                "tag_to_consequence_secs": l.gshut_to_consequence_secs,
            })).collect::<Vec<_>>(),
        },
        "community_only_changes": ctx.transitions.iter().filter(|t| t.effects.communities_changed && !t.effects.material_path_changed).count(),
        "rfc9003": "administrative-shutdown message not observable from these remote collector sessions",
        "rfc8327": "operational intent not directly observable",
        "graceful_restart": "negotiated session capability/state not directly observable from this dataset",
        "mechanism_hints_do_not_change_impact_assessment": true,
    });

    let report = JsonReport {
        schema_version: crate::schema::REPORT_SCHEMA_VERSION,
        event_id: ctx.event_id.to_string(),
        observed_event_signature: signature,
        observable_mechanism_hints: mechanism_hints,
        limitations: ctx.limitations.to_vec(),
        outcome: outcome_json,
        transitions: JsonTransitionStats {
            total: ctx.transitions.len(),
            event_window: event_count,
            cooldown: cooldown_count,
        },
        waves: ctx
            .waves
            .iter()
            .map(|w| JsonWaveSummary {
                id: w.id,
                label: w.label.clone(),
                start: format!("{}", w.start),
                end: format!("{}", w.end),
                affected_prefix_count: w.affected_prefixes.len(),
                motif_id: w.motif.as_ref().map(|m| m.id.clone()),
            })
            .collect(),
    };

    let json = serde_json::to_string_pretty(&report)
        .map_err(|e| format!("JSON serialization failed: {e}"))?;
    std::fs::write(path, json).map_err(|e| format!("cannot write {}: {e}", path.display()))
}

// ── archive_manifest.json ──────────────────────────────────────────

#[derive(Serialize)]
struct ArchiveManifest {
    event_id: String,
    ribs: Vec<JsonCachedFile>,
    updates: Vec<JsonCachedFile>,
}

#[derive(Serialize)]
struct JsonCachedFile {
    url: String,
    local_path: String,
    collector_id: String,
    data_type: String,
    size_bytes: u64,
    sha256: String,
}

fn write_archive_manifest(ctx: &OutputContext, path: &Path) -> Result<(), String> {
    let manifest = ArchiveManifest {
        event_id: ctx.event_id.to_string(),
        ribs: ctx
            .selected_ribs
            .iter()
            .map(|a| JsonCachedFile {
                url: a.url.clone(),
                local_path: a.local_path.clone(),
                collector_id: a.collector_id.clone(),
                data_type: a.data_type.clone(),
                size_bytes: a.size,
                sha256: a.sha256.clone(),
            })
            .collect(),
        updates: ctx
            .selected_updates
            .iter()
            .map(|a| JsonCachedFile {
                url: a.url.clone(),
                local_path: a.local_path.clone(),
                collector_id: a.collector_id.clone(),
                data_type: a.data_type.clone(),
                size_bytes: a.size,
                sha256: a.sha256.clone(),
            })
            .collect(),
    };
    let json = serde_json::to_string_pretty(&manifest).map_err(|e| format!("JSON error: {e}"))?;
    std::fs::write(path, json).map_err(|e| format!("cannot write {}: {e}", path.display()))
}

// ── evidence_appendix.jsonl ─────────────────────────────────────────

#[derive(Serialize)]
struct EvidenceLine {
    route_key: String,
    phase: String,
    transition_kind: String,
    timestamp: String,
    collector: String,
    peer: String,
    prefix: String,
    baseline: serde_json::Value,
    before: serde_json::Value,
    after: serde_json::Value,
    triggering: serde_json::Value,
    archive_url: Option<String>,
    archive_sha256: Option<String>,
    wave_id: Option<usize>,
    motif_id: Option<String>,
}

fn write_evidence_appendix(ctx: &OutputContext, path: &Path) -> Result<(), String> {
    let mut sorted: Vec<&RouteTransition> = ctx.transitions.iter().collect();
    sorted.sort_by_key(|t| {
        (
            t.to.timestamp(),
            t.key.collector.clone(),
            t.key.peer_ip.to_string(),
            t.key.prefix.0.to_string(),
        )
    });

    let mut lines = Vec::new();
    for t in &sorted {
        let wave_id = ctx
            .waves
            .iter()
            .find(|w| t.to.timestamp() >= w.start && t.to.timestamp() <= w.end)
            .map(|w| w.id);
        let motif_id = ctx
            .waves
            .iter()
            .find(|w| w.id == wave_id.unwrap_or(0))
            .and_then(|w| w.motif.as_ref().map(|m| m.id.clone()));

        let line = EvidenceLine {
            route_key: format!("{}/{}", t.key.collector, t.key.prefix),
            phase: format!("{:?}", t.phase),
            transition_kind: format!("{:?}", t.kind),
            timestamp: format!("{}", t.to.timestamp()),
            collector: t.key.collector.clone(),
            peer: t.key.peer_ip.to_string(),
            prefix: format!("{}", t.key.prefix),
            baseline: serde_json::to_value(&t.event_baseline).unwrap_or_default(),
            before: serde_json::to_value(&t.from).unwrap_or_default(),
            after: serde_json::to_value(&t.to).unwrap_or_default(),
            triggering: serde_json::to_value(&t.triggering).unwrap_or_default(),
            archive_url: t.triggering.source_url.clone(),
            archive_sha256: t.triggering.archive_sha256.clone(),
            wave_id,
            motif_id,
        };
        lines.push(serde_json::to_string(&line).unwrap_or_default());
    }

    std::fs::write(path, lines.join("\n") + "\n")
        .map_err(|e| format!("cannot write {}: {e}", path.display()))
}

// ── semantic_waves.json ────────────────────────────────────────────

#[derive(Serialize)]
struct SemanticWavesArtifact {
    schema_version: u32,
    event_id: String,
    waves: Vec<crate::lifecycle::SemanticWave>,
}

fn write_semantic_waves(ctx: &OutputContext, path: &Path) -> Result<(), String> {
    let artifact = SemanticWavesArtifact {
        schema_version: crate::schema::SEMANTIC_WAVE_SCHEMA_VERSION,
        event_id: ctx.event_id.to_string(),
        waves: ctx.semantic_waves.to_vec(),
    };
    let json = serde_json::to_string_pretty(&artifact).map_err(|e| format!("JSON error: {e}"))?;
    std::fs::write(path, json).map_err(|e| format!("cannot write {}: {e}", path.display()))
}

// ── withdrawal_audit.json ──────────────────────────────────────────

#[derive(Serialize)]
struct WithdrawalAuditArtifact {
    schema_version: u32,
    event_id: String,
    summary: crate::lifecycle::WithdrawalAuditSummary,
    records: Vec<crate::lifecycle::WithdrawalRecord>,
}

fn write_withdrawal_audit(ctx: &OutputContext, path: &Path) -> Result<(), String> {
    let records = crate::lifecycle::withdrawal_audit(ctx.lifecycles);
    let artifact = WithdrawalAuditArtifact {
        schema_version: crate::schema::WITHDRAWAL_AUDIT_SCHEMA_VERSION,
        event_id: ctx.event_id.to_string(),
        summary: crate::lifecycle::WithdrawalAuditSummary::from_records(&records),
        records,
    };
    let json = serde_json::to_string_pretty(&artifact).map_err(|e| format!("JSON error: {e}"))?;
    std::fs::write(path, json).map_err(|e| format!("cannot write {}: {e}", path.display()))
}

// ── lifecycle.json ─────────────────────────────────────────────────

#[derive(Serialize)]
struct LifecycleArtifact {
    schema_version: u32,
    event_id: String,
    lifecycles: Vec<crate::lifecycle::StreamLifecycle>,
}

fn write_lifecycle_artifact(ctx: &OutputContext, path: &Path) -> Result<(), String> {
    let artifact = LifecycleArtifact {
        schema_version: crate::schema::LIFECYCLE_ARTIFACT_SCHEMA_VERSION,
        event_id: ctx.event_id.to_string(),
        lifecycles: ctx.lifecycles.to_vec(),
    };
    let json = serde_json::to_string_pretty(&artifact).map_err(|e| format!("JSON error: {e}"))?;
    std::fs::write(path, json).map_err(|e| format!("cannot write {}: {e}", path.display()))
}

// ── limitations.json ───────────────────────────────────────────────

#[derive(Serialize)]
struct Limitations {
    observer: Vec<String>,
    continuity_gaps: Vec<String>,
    add_path: Vec<String>,
    manual_mappings: Vec<String>,
    archive_parser: Vec<String>,
    unsupported_conclusions: Vec<String>,
}

fn write_limitations(ctx: &OutputContext, path: &Path) -> Result<(), String> {
    let lim = Limitations {
        observer: vec![
            "Analysis uses selected RouteViews collectors only. Selected collectors do not provide global visibility.".into(),
            "BGP route state is not traffic measurement.".into(),
        ],
        continuity_gaps: ctx.limitations.iter()
            .filter(|l| l.contains("gap"))
            .cloned()
            .collect(),
        add_path: vec![
            "ADD-PATH identity is preserved per route instance. Streams with mixed keyed/unkeyed encoding are flagged add-path ambiguous and excluded from strong stream-level assessment.".into(),
        ],
        manual_mappings: vec![
            "Participant-to-ASN mapping is manually supplied in the reviewed manifest. It is not derived automatically from ticket text.".into(),
        ],
        archive_parser: vec![
            "Parsed via bgpkit-parser. Parser limitations apply.".into(),
        ],
        unsupported_conclusions: vec![
            "Local session state is not observed.".into(),
            "Physical-link state is not observed.".into(),
            "Absent communities do not prove a mechanism was unused.".into(),
            "Event declarations and BGP changes establish temporal association, not automatic causation.".into(),
            "Global reachability cannot be asserted from a limited set of observer streams.".into(),
        ],
    };
    let json = serde_json::to_string_pretty(&lim).map_err(|e| format!("JSON error: {e}"))?;
    std::fs::write(path, json).map_err(|e| format!("cannot write {}: {e}", path.display()))
}

// ── helpers ─────────────────────────────────────────────────────────

fn push_ln(buf: &mut String, line: &str) {
    buf.push_str(line);
    buf.push('\n');
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::assessment::{EventAssessment, Evidence, Verdict};
    use crate::domain::event::EventId;
    use crate::outcome::AnalysisOutcome;

    fn sample_outcome() -> AnalysisOutcome {
        AnalysisOutcome::Completed {
            assessment: EventAssessment {
                event_id: EventId::from("TEST"),
                expectation: crate::domain::expectation::ImpactExpectation {
                    kind: crate::domain::expectation::ExpectationKind::Redundant,
                    description: "test".into(),
                    provenance: "test".into(),
                },
                verdict: Verdict::NoObservableBgpImpact,
                evidence: vec![Evidence {
                    description: "test evidence".into(),
                    source_records: vec![],
                }],
                waves: vec![],
                generated_at: chrono::Utc::now(),
            },
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn make_ctx<'a>(
        outcome: &'a AnalysisOutcome,
        collectors: &'a Vec<String>,
        ribs: &'a Vec<CachedArchive>,
        updates: &'a Vec<CachedArchive>,
        transitions: &'a Vec<RouteTransition>,
        waves: &'a Vec<ImpactWave>,
        semantic_waves: &'a Vec<crate::lifecycle::SemanticWave>,
        lifecycles: &'a Vec<crate::lifecycle::StreamLifecycle>,
        limitations: &'a Vec<String>,
    ) -> OutputContext<'a> {
        OutputContext {
            outcome,
            event_id: "TEST",
            ticket_title: "Test Ticket",
            event_window: "2026-07-30T09:25Z – 2026-07-30T09:47Z",
            warmup_window: "2026-07-30T08:25Z – 2026-07-30T09:25Z",
            cooldown_window: "2026-07-30T09:47Z – 2026-07-30T10:47Z",
            declared_expectation: "Redundant (site code NEWA)",
            target_predicate: "origin AS3333 AND AS11537 in path",
            collectors,
            selected_ribs: ribs,
            selected_updates: updates,
            preflight: None,
            continuity: "Known (no gaps)",
            transitions,
            waves,
            semantic_waves,
            lifecycles,
            ticket_lifecycle: "Closed",
            transit_predicate_identity: "ContainsAny[11537]",
            limitations,
            no_observable_impact: true,
        }
    }

    #[test]
    fn output_artifacts_are_deterministic() {
        let dir_a = tempfile::tempdir().unwrap();
        let dir_b = tempfile::tempdir().unwrap();
        let outcome = sample_outcome();
        let collectors = vec!["route-views2".to_string()];
        let ribs = vec![];
        let updates = vec![];
        let transitions = vec![];
        let waves = vec![];
        let semantic_waves = vec![];
        let lifecycles: Vec<crate::lifecycle::StreamLifecycle> = vec![];
        let limitations = vec![
            "Test limitation 1".to_string(),
            "Test limitation 2".to_string(),
        ];

        let ctx = make_ctx(
            &outcome,
            &collectors,
            &ribs,
            &updates,
            &transitions,
            &waves,
            &semantic_waves,
            &lifecycles,
            &limitations,
        );

        let files_a = write_outputs(&ctx, dir_a.path()).unwrap();
        let files_b = write_outputs(&ctx, dir_b.path()).unwrap();

        assert_eq!(files_a.len(), files_b.len());
        for (fa, fb) in files_a.iter().zip(files_b.iter()) {
            let content_a = std::fs::read_to_string(fa).unwrap();
            let content_b = std::fs::read_to_string(fb).unwrap();
            assert_eq!(
                content_a,
                content_b,
                "output must be deterministic: {}",
                fa.display()
            );
        }
    }

    #[test]
    fn limitations_list_explicit_categories() {
        let outcome = sample_outcome();
        let collectors = vec!["route-views2".to_string()];
        let ribs = vec![];
        let updates = vec![];
        let transitions = vec![];
        let waves = vec![];
        let semantic_waves = vec![];
        let lifecycles: Vec<crate::lifecycle::StreamLifecycle> = vec![];
        let limitations = vec!["gap detected".to_string()];
        let ctx = make_ctx(
            &outcome,
            &collectors,
            &ribs,
            &updates,
            &transitions,
            &waves,
            &semantic_waves,
            &lifecycles,
            &limitations,
        );
        let dir = tempfile::tempdir().unwrap();
        write_outputs(&ctx, dir.path()).unwrap();

        let content = std::fs::read_to_string(dir.path().join("limitations.json")).unwrap();
        assert!(content.contains("observer"));
        assert!(content.contains("add_path"));
        assert!(content.contains("manual_mappings"));
        assert!(content.contains("unsupported_conclusions"));
    }

    #[test]
    fn insufficient_visibility_produces_report() {
        let outcome = AnalysisOutcome::insufficient_visibility("no streams");
        let collectors = vec![];
        let ribs = vec![];
        let updates = vec![];
        let transitions = vec![];
        let waves = vec![];
        let semantic_waves = vec![];
        let lifecycles: Vec<crate::lifecycle::StreamLifecycle> = vec![];
        let limitations = vec![];
        let ctx = make_ctx(
            &outcome,
            &collectors,
            &ribs,
            &updates,
            &transitions,
            &waves,
            &semantic_waves,
            &lifecycles,
            &limitations,
        );
        let dir = tempfile::tempdir().unwrap();
        write_outputs(&ctx, dir.path()).unwrap();

        let report = std::fs::read_to_string(dir.path().join("report.txt")).unwrap();
        assert!(report.contains("INSUFFICIENT VISIBILITY"));
        assert!(report.contains("no streams"));
    }

    #[test]
    fn incomplete_produces_artifacts() {
        let outcome = AnalysisOutcome::incomplete("download failed");
        let collectors = vec![];
        let ribs = vec![];
        let updates = vec![];
        let transitions = vec![];
        let waves = vec![];
        let semantic_waves = vec![];
        let lifecycles: Vec<crate::lifecycle::StreamLifecycle> = vec![];
        let limitations = vec![];
        let ctx = make_ctx(
            &outcome,
            &collectors,
            &ribs,
            &updates,
            &transitions,
            &waves,
            &semantic_waves,
            &lifecycles,
            &limitations,
        );
        let dir = tempfile::tempdir().unwrap();
        write_outputs(&ctx, dir.path()).unwrap();

        let report = std::fs::read_to_string(dir.path().join("report.txt")).unwrap();
        assert!(report.contains("INCOMPLETE"));
        assert!(report.contains("download failed"));
    }

    #[test]
    fn report_uses_cautious_language() {
        let outcome = sample_outcome();
        let collectors = vec![];
        let ribs = vec![];
        let updates = vec![];
        let transitions = vec![];
        let waves = vec![];
        let semantic_waves = vec![];
        let lifecycles: Vec<crate::lifecycle::StreamLifecycle> = vec![];
        let limitations = vec![];
        let ctx = make_ctx(
            &outcome,
            &collectors,
            &ribs,
            &updates,
            &transitions,
            &waves,
            &semantic_waves,
            &lifecycles,
            &limitations,
        );
        let dir = tempfile::tempdir().unwrap();
        write_outputs(&ctx, dir.path()).unwrap();

        let report = std::fs::read_to_string(dir.path().join("report.txt")).unwrap();
        assert!(!report.contains("remained globally reachable"));
        assert!(!report.contains("traffic successfully failed over"));
        assert!(!report.contains("physical circuit caused"));
        assert!(report.contains("observer-scoped"));
    }

    #[test]
    fn evidence_appendix_contains_baseline_before_after() {
        use crate::domain::observation::EvidenceRef;
        use crate::domain::route::Prefix;
        use crate::domain::route::{
            AnalysisPhase, GenericTransitionEffects, RouteKey, TransitionKind,
        };
        use std::net::IpAddr;

        // Build a minimal transition for the appendix test
        let prefix = Prefix::from("192.0.2.0/24");
        let key = RouteKey::new("rv2", "185.1.8.65".parse::<IpAddr>().unwrap(), &prefix);
        let ev = EvidenceRef::synthetic(0, "test", "0000000000000000");
        let after_state = crate::domain::route::EvidencedRouteState::present(
            crate::domain::route::RouteState {
                prefix: prefix.clone(),
                attributes: crate::domain::route::RouteAttributes::from_as_path(vec![]),
                timestamp: chrono::Utc::now(),
                observer: "185.1.8.65".to_string(),
                path_id: None,
            },
            ev.clone(),
        );
        let transition = RouteTransition::new(
            key,
            None,
            None,
            after_state,
            ev,
            TransitionKind::Announcement,
            GenericTransitionEffects::default(),
            AnalysisPhase::Event,
        );
        let wave = ImpactWave {
            id: 1,
            label: "Test wave".into(),
            start: chrono::Utc::now(),
            peak: chrono::Utc::now(),
            end: chrono::Utc::now(),
            affected_prefixes: vec![prefix],
            affected_peers: vec!["185.1.8.65".into()],
            motif: None,
        };

        let outcome = sample_outcome();
        let collectors = vec![];
        let ribs = vec![];
        let updates = vec![];
        let transitions = vec![transition];
        let waves = vec![wave];
        let semantic_waves = vec![];
        let lifecycles: Vec<crate::lifecycle::StreamLifecycle> = vec![];
        let limitations = vec![];
        let ctx = make_ctx(
            &outcome,
            &collectors,
            &ribs,
            &updates,
            &transitions,
            &waves,
            &semantic_waves,
            &lifecycles,
            &limitations,
        );
        let dir = tempfile::tempdir().unwrap();
        write_outputs(&ctx, dir.path()).unwrap();

        let appendix = std::fs::read_to_string(dir.path().join("evidence_appendix.jsonl")).unwrap();
        assert!(appendix.contains("baseline"));
        assert!(appendix.contains("after"));
        // Verify one line per transition
        assert_eq!(appendix.lines().count(), 1);
    }

    // ── Part 8: report structure tests ────────────────────────────

    fn make_lifecycle(
        category: crate::lifecycle::StreamCategory,
    ) -> crate::lifecycle::StreamLifecycle {
        use crate::lifecycle::{StreamFlags, StreamLifecycle};
        StreamLifecycle {
            collector: "route-views2".into(),
            peer_ip: "185.1.8.65".into(),
            prefix: "192.0.2.0/24".into(),
            baseline_path: vec![6447, 11537, 3333],
            baseline_instance_count: 1,
            max_concurrent_instances: 1,
            total_route_instances: 1,
            category,
            flags: StreamFlags {
                restored: false,
                not_restored: false,
                multiple_cycles: false,
                add_path_ambiguous: false,
            },
            first_change: None,
            transitions: vec![],
            min_absence_secs: None,
            max_absence_secs: None,
            was_withdrawn: false,
            stream_withdrawal_time: None,
            active_before_absence: 0,
            transit_at_withdrawal: None,
            withdrawn_instances: vec![],
            stream_withdrawal_count: 0,
            restorations: vec![],
            add_path_ambiguity: None,
            replacement_appeared: false,
            replacement_retained_transit: None,
            prepending_changed: false,
            cooldown_transitions: vec![],
            final_state: None,
            baseline_restored: false,
            restoration_time: None,
            affected_duration_secs: None,
            graceful_shutdown_seen: false,
            gshut_present_at_baseline: false,
            gshut_newly_added: false,
            gshut_removed: false,
            first_gshut_timestamp: None,
            last_gshut_timestamp: None,
            gshut_before_withdrawal: false,
            gshut_before_path_change: false,
            gshut_to_consequence_secs: None,
            gshut_removed_during_restoration: false,
            communities_before: vec![],
            communities_after: vec![],
        }
    }

    #[test]
    fn report_separates_impact_and_mechanism_sections() {
        let outcome = sample_outcome();
        let collectors = vec!["route-views2".to_string()];
        let ribs = vec![];
        let updates = vec![];
        let transitions = vec![];
        let waves = vec![];
        let semantic_waves = vec![];
        let lifecycles = vec![make_lifecycle(crate::lifecycle::StreamCategory::Unchanged)];
        let limitations = vec![];
        let ctx = make_ctx(
            &outcome,
            &collectors,
            &ribs,
            &updates,
            &transitions,
            &waves,
            &semantic_waves,
            &lifecycles,
            &limitations,
        );
        let dir = tempfile::tempdir().unwrap();
        write_outputs(&ctx, dir.path()).unwrap();
        let report = std::fs::read_to_string(dir.path().join("report.txt")).unwrap();
        let sig = report.find("Observed event signature").unwrap();
        let hints = report.find("Observable mechanism hints").unwrap();
        let lim = report.find("Limitations").unwrap();
        assert!(sig < hints, "signature precedes mechanism hints");
        assert!(hints < lim, "mechanism hints precede limitations");
    }

    #[test]
    fn report_counts_streams_and_instances_separately() {
        let outcome = sample_outcome();
        let collectors = vec!["route-views2".to_string()];
        let ribs = vec![];
        let updates = vec![];
        let transitions = vec![];
        let waves = vec![];
        let semantic_waves = vec![];
        let lifecycles = vec![
            make_lifecycle(crate::lifecycle::StreamCategory::Unchanged),
            make_lifecycle(crate::lifecycle::StreamCategory::Unchanged),
        ];
        let empty_limitations: Vec<String> = vec![];
        let ctx = make_ctx(
            &outcome,
            &collectors,
            &ribs,
            &updates,
            &transitions,
            &waves,
            &semantic_waves,
            &lifecycles,
            &empty_limitations,
        );
        let dir = tempfile::tempdir().unwrap();
        write_outputs(&ctx, dir.path()).unwrap();
        let report = std::fs::read_to_string(dir.path().join("report.txt")).unwrap();
        assert!(report.contains("Baseline observer-prefix streams: 2"));
        assert!(report.contains("Baseline route instances:          2"));
    }

    #[test]
    fn report_uses_observer_scoped_withdrawal_wording() {
        let outcome = sample_outcome();
        let collectors = vec!["route-views2".to_string()];
        let ribs = vec![];
        let updates = vec![];
        let transitions = vec![];
        let waves = vec![];
        let semantic_waves = vec![];
        let mut lc = make_lifecycle(crate::lifecycle::StreamCategory::Withdrawn);
        lc.was_withdrawn = true;
        lc.flags.not_restored = true;
        let lifecycles = vec![lc];
        let empty_limitations: Vec<String> = vec![];
        let ctx = make_ctx(
            &outcome,
            &collectors,
            &ribs,
            &updates,
            &transitions,
            &waves,
            &semantic_waves,
            &lifecycles,
            &empty_limitations,
        );
        let dir = tempfile::tempdir().unwrap();
        write_outputs(&ctx, dir.path()).unwrap();
        let report = std::fs::read_to_string(dir.path().join("report.txt")).unwrap();
        assert!(report.contains("Withdrawn streams:                  1"));
        // Observer-scoped: never "global withdrawal".
        assert!(!report.contains("global withdrawal"), "{report}");
    }

    #[test]
    fn report_does_not_claim_unobservable_mechanisms() {
        let outcome = sample_outcome();
        let collectors = vec![];
        let ribs = vec![];
        let updates = vec![];
        let transitions = vec![];
        let waves = vec![];
        let semantic_waves = vec![];
        let lifecycles = vec![];
        let empty_limitations: Vec<String> = vec![];
        let ctx = make_ctx(
            &outcome,
            &collectors,
            &ribs,
            &updates,
            &transitions,
            &waves,
            &semantic_waves,
            &lifecycles,
            &empty_limitations,
        );
        let dir = tempfile::tempdir().unwrap();
        write_outputs(&ctx, dir.path()).unwrap();
        let report = std::fs::read_to_string(dir.path().join("report.txt")).unwrap();
        // RFC 9003 / 8327 / GR are stated as not observable, never claimed.
        assert!(report.contains("RFC 9003: administrative-shutdown message not observable"));
        assert!(report.contains("RFC 8327: operational intent not directly observable"));
        assert!(report.contains(
            "Graceful Restart: negotiated session capability/state not directly observable"
        ));
        assert!(!report.contains("was administratively shut down"));
        assert!(!report.contains("intended to withdraw"));
    }

    #[test]
    fn report_contains_schema_version() {
        let outcome = sample_outcome();
        let collectors = vec![];
        let ribs = vec![];
        let updates = vec![];
        let transitions = vec![];
        let waves = vec![];
        let semantic_waves = vec![];
        let lifecycles = vec![];
        let empty_limitations: Vec<String> = vec![];
        let ctx = make_ctx(
            &outcome,
            &collectors,
            &ribs,
            &updates,
            &transitions,
            &waves,
            &semantic_waves,
            &lifecycles,
            &empty_limitations,
        );
        let dir = tempfile::tempdir().unwrap();
        write_outputs(&ctx, dir.path()).unwrap();
        let report = std::fs::read_to_string(dir.path().join("report.txt")).unwrap();
        assert!(
            report.contains(&format!(
                "Report schema: v{}",
                crate::schema::REPORT_SCHEMA_VERSION
            )),
            "{report}"
        );
        let json = std::fs::read_to_string(dir.path().join("report.json")).unwrap();
        let val: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(val["schema_version"], crate::schema::REPORT_SCHEMA_VERSION);
    }
}
