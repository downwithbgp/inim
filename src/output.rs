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
    /// Human label for the ticket expectation kind (e.g. "redundant-attachment").
    pub expectation_kind_label: &'a str,
    /// Canonical TransitPredicate identity used for the analysis.
    pub transit_predicate_identity: &'a str,
    /// The reviewed transit predicate itself, when available.
    pub transit_predicate: Option<&'a crate::domain::route::TransitPredicate>,
    /// Collectors requested by the reviewed manifest (before preflight).
    pub requested_collectors: &'a [String],
    /// Source family label for report wording (e.g. RouteViews or RIS).
    pub source_family_label: &'a str,
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

        // Transitions artifact (per-transition records for phase summaries).
        let transitions_path = out_dir.join("transitions.json");
        write_transitions(ctx, &transitions_path)?;
        files.push(transitions_path);
    }

    let limitations_path = out_dir.join("limitations.json");
    write_limitations(ctx, &limitations_path)?;
    files.push(limitations_path);

    Ok(files)
}

// ── report.txt ─────────────────────────────────────────────────────

/// Rendered report line prefix used for the schema line.
fn schema_line() -> String {
    format!(
        "Report schema: v{} (JSON schema v{})",
        crate::schema::REPORT_SCHEMA_VERSION,
        crate::schema::REPORT_SCHEMA_VERSION
    )
}

/// Human-facing assessment sentence for the expectation.
pub fn render_assessment_line(
    verdict: &crate::domain::assessment::Verdict,
    expectation: &str,
) -> String {
    use crate::domain::assessment::AssessmentKind;
    match verdict.assessment_kind() {
        AssessmentKind::NotAssessable => {
            "Not assessable: insufficient visibility from the selected observers.".to_string()
        }
        kind => {
            let mut line = format!("{} {expectation} expectation.", kind.human_label());
            if verdict.is_provisional() {
                line.push_str(" Observation is provisional (open event); later route-state changes may alter this.");
            }
            line
        }
    }
}

/// Archive-coverage statement, always scoped to the selected plan.
pub fn render_archive_coverage(
    requested: &[String],
    retained: &[String],
    limitations: &[String],
) -> String {
    // Incomplete only when a selected archive could not be acquired.
    for lim in limitations {
        let l = lim.to_lowercase();
        if l.contains("failed to cache")
            || l.contains("could not be acquired")
            || l.contains("download failed")
        {
            return format!("Incomplete because a selected archive could not be acquired ({lim}).");
        }
    }
    let missing: Vec<&str> = requested
        .iter()
        .filter(|r| !retained.contains(r))
        .map(|r| r.as_str())
        .collect();
    if !missing.is_empty() {
        return format!(
            "Complete for the selected analysis plan at {}; {} had no qualifying baseline target streams after RIB preflight.",
            retained.join(", "),
            missing.join(", ")
        );
    }
    "Complete for the selected analysis plan (selected collectors, planned interval, verified archives).".to_string()
}

/// Data-derived finding paragraph for the Result section.
pub fn render_finding(
    lcs: &[crate::lifecycle::StreamLifecycle],
    transitions: &[crate::domain::route::RouteTransition],
    collectors: &[String],
) -> String {
    use crate::lifecycle::StreamCategory;
    let total = lcs.len();
    let unchanged = lcs
        .iter()
        .filter(|l| l.category == StreamCategory::Unchanged)
        .count();
    let prepend = lcs
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
    let collector_txt = collectors.join(", ");

    let any_change = prepend + still_via + departed + withdrawn > 0;

    if !any_change && transitions.is_empty() {
        return format!(
            "Across {total} selected observer-prefix streams at {collector_txt}, inim observed no announcements, withdrawals, path changes, or community changes during the event analysis window."
        );
    }
    if !any_change {
        let community_only = transitions
            .iter()
            .filter(|t| t.effects.communities_changed && !t.effects.material_path_changed)
            .count();
        return format!(
            "Across {total} selected observer-prefix streams at {collector_txt}, inim observed no announcements, withdrawals, or path changes; {community_only} community-only attribute change(s) occurred."
        );
    }

    // Partial / heterogeneous case. Streams are the primary unit.
    let mut parts: Vec<String> =
        vec!["The event produced partial and heterogeneous external routing impact.".to_string()];
    if withdrawn > 0 {
        let mut w =
            format!("{withdrawn} of {total} selected observer-prefix streams became absent");
        if restored == withdrawn {
            w.push_str(" and later returned");
        } else if restored > 0 {
            w.push_str(&format!("; {restored} of them later returned"));
        }
        if unresolved > 0 {
            w.push_str(&format!(
                ", while {unresolved} had not returned by the end of the observation window"
            ));
        }
        w.push('.');
        parts.push(w);
    }
    let remaining = total.saturating_sub(withdrawn);
    let mut remainder: Vec<String> = Vec::new();
    if prepend > 0 {
        remainder.push(format!("{prepend} showed prepend-only changes"));
    }
    if still_via > 0 {
        remainder.push(format!(
            "{still_via} had other material path changes while retaining the reviewed transit"
        ));
    }
    if departed > 0 {
        remainder.push(format!(
            "{departed} remained visible after departing that transit"
        ));
    }
    if unchanged > 0 {
        remainder.push(format!("{unchanged} remained unchanged"));
    }
    if !remainder.is_empty() {
        let joined = join_sentences(&remainder);
        parts.push(format!(
            "Among the remaining {remaining} streams, {joined}."
        ));
    }
    if ambiguous > 0 {
        parts.push(format!(
            "{ambiguous} stream(s) have ambiguous ADD-PATH continuity and were excluded from strong stream-level assessment."
        ));
    }
    parts.join(" ")
}

fn join_sentences(items: &[String]) -> String {
    match items.len() {
        0 => String::new(),
        1 => items[0].clone(),
        2 => format!("{} and {}", items[0], items[1]),
        _ => {
            let mut out = String::new();
            for (i, item) in items.iter().enumerate() {
                if i == items.len() - 1 && i > 0 {
                    out.push_str(&format!(", and {item}"));
                } else if i > 0 {
                    out.push_str(&format!(", {item}"));
                } else {
                    out.push_str(item);
                }
            }
            out
        }
    }
}

/// The full analyst-facing text report.
pub fn render_report_txt(ctx: &OutputContext) -> String {
    use crate::lifecycle::StreamCategory;

    let mut buf = String::new();
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
    let material_transitions = ctx
        .transitions
        .iter()
        .filter(|t| t.effects.material_path_changed)
        .count();
    let withdrawal_transitions = ctx
        .transitions
        .iter()
        .filter(|t| matches!(t.kind, crate::domain::route::TransitionKind::Withdrawal))
        .count();

    // ── Header ────────────────────────────────────────────────────
    push_ln(&mut buf, "══════════════════════════════════════════");
    push_ln(&mut buf, "  EXTERNAL BGP EVENT ANALYSIS");
    push_ln(&mut buf, "══════════════════════════════════════════");
    push_ln(&mut buf, "");
    push_ln(&mut buf, ctx.event_id);
    push_ln(&mut buf, ctx.ticket_title);
    push_ln(&mut buf, "");
    push_ln(&mut buf, &schema_line());
    push_ln(&mut buf, "");

    // ── Result ────────────────────────────────────────────────────
    push_ln(&mut buf, "Result");
    match ctx.outcome {
        AnalysisOutcome::Completed { assessment } => {
            push_ln(
                &mut buf,
                &format!("    {}", assessment.verdict.human_label()),
            );
            push_ln(&mut buf, "");
            let finding = render_finding(lcs, ctx.transitions, ctx.collectors);
            for line in wrap_text(&finding, 76, 4) {
                push_ln(&mut buf, &line);
            }
        }
        AnalysisOutcome::InsufficientVisibility { reason } => {
            push_ln(&mut buf, "    Insufficient visibility");
            push_ln(&mut buf, "");
            push_ln(&mut buf, &format!("    {reason}"));
        }
        AnalysisOutcome::Incomplete { failure } => {
            push_ln(&mut buf, "    Analysis incomplete");
            push_ln(&mut buf, "");
            push_ln(&mut buf, &format!("    {failure}"));
        }
    }
    push_ln(&mut buf, "");

    // ── Assessment against ticket expectation ─────────────────────
    push_ln(&mut buf, "Assessment against ticket expectation");
    if let AnalysisOutcome::Completed { assessment } = ctx.outcome {
        let line = render_assessment_line(&assessment.verdict, ctx.expectation_kind_label);
        for l in wrap_text(&line, 76, 4) {
            push_ln(&mut buf, &l);
        }
        // Explicitly separate the observed signature from the assessment.
        if matches!(
            assessment.verdict,
            crate::domain::assessment::Verdict::NoObservableBgpImpact
        ) {
            push_ln(&mut buf, "");
            push_ln(
                &mut buf,
                "    The absence of observed route-state changes does not prove that the",
            );
            push_ln(
                &mut buf,
                "    attachment, circuit, or network was physically redundant.",
            );
        }
    } else {
        push_ln(
            &mut buf,
            "    Not assessable: no BGP evidence was observed (planning blocked or analysis incomplete).",
        );
    }
    push_ln(&mut buf, "");

    // ── Observation scope ─────────────────────────────────────────
    push_ln(&mut buf, "Observation scope");
    push_ln(
        &mut buf,
        &format!("    Event window:      {}", ctx.event_window),
    );
    push_ln(
        &mut buf,
        &format!("    Warmup:            {}", ctx.warmup_window),
    );
    push_ln(
        &mut buf,
        &format!("    Cooldown:          {}", ctx.cooldown_window),
    );
    push_ln(
        &mut buf,
        &format!("    Collectors:        {}", ctx.collectors.join(", ")),
    );
    push_ln(
        &mut buf,
        &format!("    Selected streams:  {baseline_streams} observer-prefix streams"),
    );
    push_ln(
        &mut buf,
        &format!("    Baseline routes:   {baseline_instances} route instances"),
    );
    if multi_instance > 0 {
        push_ln(
            &mut buf,
            &format!(
                "    Multi-instance:    {multi_instance} stream(s) with multiple route instances"
            ),
        );
    }
    if let Some(pred) = ctx.transit_predicate {
        push_ln(&mut buf, "    Baseline qualification:");
        push_ln(&mut buf, &format!("        {}", pred.human_description()));
    }
    push_labeled(
        &mut buf,
        "    Archive coverage:  ",
        &render_archive_coverage(ctx.requested_collectors, ctx.collectors, ctx.limitations),
        76,
    );
    push_ln(&mut buf, "");

    // ── Important limitation ──────────────────────────────────────
    push_ln(&mut buf, "Important limitation");
    push_ln(
        &mut buf,
        "    This finding is limited to externally exported BGP route state at the",
    );
    push_ln(
        &mut buf,
        "    selected public collectors. It does not measure traffic, circuit state,",
    );
    push_ln(&mut buf, "    or global reachability.");
    push_ln(&mut buf, "");

    // ── Ticket interpretation ─────────────────────────────────────
    push_ln(&mut buf, "Ticket interpretation");
    push_labeled(
        &mut buf,
        "    Ticket expectation: ",
        ctx.declared_expectation,
        76,
    );
    push_ln(
        &mut buf,
        &format!("    Ticket lifecycle:   {}", ctx.ticket_lifecycle),
    );
    push_ln(
        &mut buf,
        &format!("    Target predicate:   {}", ctx.target_predicate),
    );
    if let Some(pred) = ctx.transit_predicate {
        push_ln(
            &mut buf,
            &format!("    TransitPredicate:   {}", pred.render_canonical()),
        );
    }
    push_ln(&mut buf, "");

    // ── Evidence details ──────────────────────────────────────────
    push_ln(&mut buf, "Evidence details");
    push_ln(
        &mut buf,
        &format!("    Route-instance transitions: {}", ctx.transitions.len()),
    );
    push_ln(
        &mut buf,
        &format!(
            "      event-window: {}  cooldown: {}",
            ctx.transitions
                .iter()
                .filter(|t| t.phase == AnalysisPhase::Event)
                .count(),
            ctx.transitions
                .iter()
                .filter(|t| t.phase == AnalysisPhase::Cooldown)
                .count()
        ),
    );
    push_ln(
        &mut buf,
        &format!("    Route-instance withdrawal transitions: {withdrawal_transitions}"),
    );
    push_ln(
        &mut buf,
        &format!("    Material path changes (transitions): {material_transitions}"),
    );
    push_ln(
        &mut buf,
        &format!("    Baseline route instances: {baseline_instances}"),
    );
    if let AnalysisOutcome::Completed { assessment } = ctx.outcome {
        for ev in &assessment.evidence {
            // The compact lifecycle dump lives in report.json; the text
            // report renders the readable Lifecycle counts section instead.
            if ev.description.starts_with("Stream lifecycle:") {
                continue;
            }
            push_ln(&mut buf, &format!("    - {}", ev.description));
        }
    }
    push_ln(&mut buf, "");

    // ── Lifecycle counts ──────────────────────────────────────────
    push_ln(&mut buf, "Lifecycle counts (observer-prefix streams)");
    push_ln(
        &mut buf,
        &format!("    Total selected streams:      {baseline_streams}"),
    );
    push_ln(
        &mut buf,
        &format!("    Unchanged streams:            {unchanged}"),
    );
    push_ln(
        &mut buf,
        &format!("    Prepend-only streams:         {prepend_only}"),
    );
    push_ln(
        &mut buf,
        &format!("    Material change, still via reviewed transit: {still_via}"),
    );
    push_ln(
        &mut buf,
        &format!("    Streams departing the reviewed transit: {departed}"),
    );
    push_ln(
        &mut buf,
        &format!("    Observer-prefix streams that became absent: {withdrawn}"),
    );
    if withdrawn > 0 || departed > 0 {
        push_ln(
            &mut buf,
            &format!("    Streams restored after absence: {restored}"),
        );
    } else {
        push_labeled(
            &mut buf,
            "    Streams restored after absence: ",
            "Not applicable (no stream became absent or departed)",
            76,
        );
    }
    push_ln(
        &mut buf,
        &format!("    Unresolved streams:            {unresolved}"),
    );
    push_ln(
        &mut buf,
        &format!("    ADD-PATH ambiguous streams:    {ambiguous}"),
    );
    push_ln(&mut buf, "");

    // ── Semantic waves ────────────────────────────────────────────
    if !ctx.semantic_waves.is_empty() {
        push_ln(&mut buf, "Semantic waves");
        for w in ctx.semantic_waves {
            let wave_line = format!(
                "{}: {} — {} ({} – {}, {} observer-prefix streams, {} route instances)",
                w.id,
                w.label.as_str(),
                wave_label_human(&w.label),
                w.start.format("%H:%M:%S"),
                w.end.format("%H:%M:%S"),
                w.stream_count,
                w.route_instance_count,
            );
            push_labeled(&mut buf, "    ", &wave_line, 76);
        }
        push_ln(&mut buf, "");
    }

    // ── Observable mechanism hints ────────────────────────────────
    push_ln(&mut buf, "Observable mechanism hints");
    let gshut_streams = lcs.iter().filter(|l| l.graceful_shutdown_seen).count();
    if gshut_streams == 0 {
        push_ln(
            &mut buf,
            "    No RFC 8326 GRACEFUL_SHUTDOWN community reached the selected observers.",
        );
        push_ln(
            &mut buf,
            "    Its absence does not establish that graceful shutdown was not used.",
        );
    } else {
        push_ln(
            &mut buf,
            &format!(
                "    RFC 8326 GRACEFUL_SHUTDOWN community was observed on {gshut_streams} selected observer-prefix stream(s)."
            ),
        );
        for l in lcs.iter().filter(|l| l.graceful_shutdown_seen) {
            let first = l
                .first_gshut_timestamp
                .map(|t| t.format("%H:%M:%S").to_string())
                .unwrap_or_else(|| "?".into());
            let last = l
                .last_gshut_timestamp
                .map(|t| t.format("%H:%M:%S").to_string())
                .unwrap_or_else(|| "?".into());
            push_ln(
                &mut buf,
                &format!(
                    "      {}:{} {} first={first} last={last} before-withdrawal={} before-replacement={}",
                    l.collector, l.peer_ip, l.prefix, l.gshut_before_withdrawal, l.gshut_before_path_change
                ),
            );
        }
        push_ln(
            &mut buf,
            "    The tag's presence does not imply it caused any subsequent route-state change.",
        );
    }
    let community_only = ctx
        .transitions
        .iter()
        .filter(|t| t.effects.communities_changed && !t.effects.material_path_changed)
        .count();
    push_ln(
        &mut buf,
        &format!("    Community-only changes: {community_only} transition(s)"),
    );
    push_ln(&mut buf, "");
    push_ln(&mut buf, "    Not directly observable here:");
    push_ln(&mut buf, "      RFC 9003 administrative-shutdown message");
    push_ln(&mut buf, "      RFC 8327 operational intent");
    push_ln(&mut buf, "      Graceful Restart negotiation state");
    push_ln(
        &mut buf,
        "    Mechanism hints do not change the impact assessment by themselves.",
    );
    push_ln(&mut buf, "");

    // ── Method and limitations ────────────────────────────────────
    push_ln(&mut buf, "Method and limitations");
    push_ln(
        &mut buf,
        "    Selected collectors do not provide global visibility.",
    );
    push_ln(&mut buf, "    BGP route state is not traffic measurement.");
    push_ln(
        &mut buf,
        "    Local BGP session state is not directly observed.",
    );
    push_ln(&mut buf, "    Physical-link state is not observed.");
    push_ln(
        &mut buf,
        "    Absent optional communities prove nothing about mechanism non-use.",
    );
    push_ln(
        &mut buf,
        "    Temporal association is not automatic causation.",
    );
    for lim in ctx.limitations {
        for l in wrap_text(&format!("• {lim}"), 72, 4) {
            push_ln(&mut buf, &l);
        }
    }
    push_ln(&mut buf, "");

    // ── Evidence artifact references ──────────────────────────────
    push_ln(&mut buf, "Evidence artifact references");
    push_ln(
        &mut buf,
        "    Full evidence with schema versions is written to this directory:",
    );
    push_ln(
        &mut buf,
        "      report.json        assessment, signature, mechanism hints, limitations",
    );
    push_ln(
        &mut buf,
        "      evidence_appendix.jsonl   per-transition evidence",
    );
    push_ln(
        &mut buf,
        "      archive_manifest.json     source archives with SHA-256",
    );
    push_ln(
        &mut buf,
        "      lifecycle.json           per-stream lifecycles",
    );
    push_ln(
        &mut buf,
        "      semantic_waves.json      wave boundaries and facets",
    );
    push_ln(
        &mut buf,
        "      withdrawal_audit.json    withdrawn-stream audit",
    );

    buf
}

/// Ordinary-language description for a semantic wave label.
pub fn wave_label_human(label: &crate::lifecycle::WaveLabel) -> &'static str {
    use crate::lifecycle::WaveLabel;
    match label {
        WaveLabel::PrependReduction => "widespread reduction in origin-AS prepending",
        WaveLabel::PrependIncrease => "widespread increase in origin-AS prepending",
        WaveLabel::StreamWithdrawal => {
            "clustered temporary observer-stream withdrawals and associated route-state changes"
        }
        WaveLabel::TransitDeparture => "observer streams departing the reviewed transit",
        WaveLabel::StreamRestoration => "observer-stream restoration after absence",
        WaveLabel::TransitReturn => "observer streams returning to the reviewed transit",
        WaveLabel::BaselinePolicyRestoration => "return to baseline route policy",
        WaveLabel::MixedRouteChange => "mixed route-state changes",
    }
}

/// Print a label followed by wrapped text, aligning continuations.
fn push_labeled(buf: &mut String, label: &str, text: &str, width: usize) {
    let indent = label.len();
    let mut first = true;
    for l in wrap_text(text, width.saturating_sub(indent), 0) {
        if first {
            buf.push_str(label);
            buf.push_str(&l);
            buf.push('\n');
            first = false;
        } else {
            buf.push_str(&" ".repeat(indent));
            buf.push_str(&l);
            buf.push('\n');
        }
    }
}

/// Wrap a paragraph at `width` columns with `indent` leading spaces.
fn wrap_text(text: &str, width: usize, indent: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut line = String::new();
    let prefix = " ".repeat(indent);
    for word in text.split_whitespace() {
        if line.is_empty() {
            line = format!("{prefix}{word}");
        } else if line.len() + 1 + word.len() <= width {
            line.push(' ');
            line.push_str(word);
        } else {
            out.push(line);
            line = format!("{prefix}{word}");
        }
    }
    if !line.is_empty() {
        out.push(line);
    }
    out
}

fn write_report_txt(ctx: &OutputContext, path: &Path) -> Result<(), String> {
    let content = render_report_txt(ctx);
    std::fs::write(path, content).map_err(|e| format!("cannot write {}: {e}", path.display()))
}

// ── report.json ─────────────────────────────────────────────────────

#[derive(Serialize)]
struct JsonReport {
    schema_version: u32,
    event_id: String,
    result: serde_json::Value,
    assessment: serde_json::Value,
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
            "archive_coverage": render_archive_coverage(
                ctx.requested_collectors,
                ctx.collectors,
                ctx.limitations,
            ),
            "baseline_qualification": ctx.transit_predicate.map(|p| p.human_description()),
            "exact_transit_predicate": ctx.transit_predicate.map(|p| p.render_canonical()),
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

    let (result_json, assessment_json) = if let AnalysisOutcome::Completed { assessment } =
        ctx.outcome
    {
        let verdict_label = assessment.verdict.human_label();
        let finding = render_finding(lcs, ctx.transitions, ctx.collectors);
        let assessment_text =
            render_assessment_line(&assessment.verdict, ctx.expectation_kind_label);
        (
            serde_json::json!({
                "verdict": assessment.verdict,
                "verdict_label": verdict_label,
                "finding": finding,
            }),
            serde_json::json!({
                "statement": assessment_text,
                "verdict": assessment.verdict,
                "provisional": assessment.verdict.is_provisional(),
            }),
        )
    } else {
        (
            serde_json::json!({
                "verdict": null,
                "verdict_label": match ctx.outcome {
                    AnalysisOutcome::InsufficientVisibility { .. } => "Insufficient visibility",
                    AnalysisOutcome::Incomplete { .. } => "Analysis incomplete",
                    AnalysisOutcome::Completed { .. } => unreachable!(),
                },
                "finding": null,
            }),
            serde_json::json!({
                "statement": "Not assessable: no BGP evidence was observed (planning blocked or analysis incomplete).",
                "verdict": null,
                "provisional": false,
            }),
        )
    };

    let report = JsonReport {
        schema_version: crate::schema::REPORT_SCHEMA_VERSION,
        event_id: ctx.event_id.to_string(),
        result: result_json,
        assessment: assessment_json,
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

// ── transitions.json ───────────────────────────────────────────────

#[derive(Serialize)]
struct TransitionsArtifact {
    schema_version: u32,
    event_id: String,
    transitions: Vec<TransitionArtifactRecord>,
}

#[derive(Serialize)]
struct TransitionArtifactRecord {
    seq: usize,
    kind: String,
    occurred_utc: String,
    phase: String,
    collector: String,
    peer_ip: String,
    prefix: String,
    path_id: Option<u32>,
    material_path_changed: bool,
    communities_changed: bool,
    announced: bool,
    withdrawn: bool,
    observation_id: u64,
    archive_sha256: Option<String>,
}

/// Compact label for a transition kind (variant name only).
pub fn transition_kind_label(kind: &crate::domain::route::TransitionKind) -> &'static str {
    use crate::domain::route::TransitionKind;
    match kind {
        TransitionKind::Announcement => "Announcement",
        TransitionKind::Withdrawal => "Withdrawal",
        TransitionKind::Duplicate => "Duplicate",
        TransitionKind::PathReplacement { .. } => "PathReplacement",
        TransitionKind::AttributeChange => "AttributeChange",
        TransitionKind::SessionReset => "SessionReset",
        TransitionKind::Restoration => "Restoration",
        TransitionKind::ReturnToBaseline => "ReturnToBaseline",
    }
}

/// Human label for the run analysis phase.
pub fn analysis_phase_label(phase: &crate::domain::route::AnalysisPhase) -> &'static str {
    use crate::domain::route::AnalysisPhase;
    match phase {
        AnalysisPhase::Warmup => "Warmup",
        AnalysisPhase::Event => "Event",
        AnalysisPhase::Cooldown => "Cooldown",
    }
}

/// Write the per-transition artifact used for phase-conditioned summaries.
///
/// `occurred_utc` is the route state timestamp when the state is present;
/// for absent states (withdrawals) the evidence timestamp is used.
pub fn write_transitions(ctx: &OutputContext, path: &Path) -> Result<(), String> {
    let transitions = ctx
        .transitions
        .iter()
        .enumerate()
        .map(|(seq, t)| {
            let occurred =
                t.to.state
                    .as_ref()
                    .map(|st| st.timestamp)
                    .unwrap_or(t.to.evidence.timestamp);
            TransitionArtifactRecord {
                seq,
                kind: transition_kind_label(&t.kind).to_string(),
                occurred_utc: occurred.to_rfc3339(),
                phase: analysis_phase_label(&t.phase).to_string(),
                collector: t.key.collector.clone(),
                peer_ip: t.key.peer_ip.to_string(),
                prefix: t.key.prefix.to_string(),
                path_id: t.key.path_id,
                material_path_changed: t.effects.material_path_changed,
                communities_changed: t.effects.communities_changed,
                announced: matches!(t.kind, crate::domain::route::TransitionKind::Announcement),
                withdrawn: matches!(t.kind, crate::domain::route::TransitionKind::Withdrawal),
                observation_id: t.triggering.observation_id.0,
                archive_sha256: t.triggering.archive_sha256.clone(),
            }
        })
        .collect::<Vec<_>>();
    let artifact = TransitionsArtifact {
        schema_version: crate::schema::TRANSITIONS_ARTIFACT_SCHEMA_VERSION,
        event_id: ctx.event_id.to_string(),
        transitions,
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
            format!(
                "Analysis uses selected {} collectors only. Selected collectors do not provide global visibility.",
                ctx.source_family_label
            ),
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
            source_family_label: "RouteViews",
            selected_ribs: ribs,
            selected_updates: updates,
            preflight: None,
            continuity: "Known (no gaps)",
            transitions,
            waves,
            semantic_waves,
            lifecycles,
            ticket_lifecycle: "Closed",
            expectation_kind_label: "redundant-attachment",
            transit_predicate_identity: "ContainsAny[11537]",
            transit_predicate: None,
            requested_collectors: collectors,
            limitations,
            no_observable_impact: true,
        }
    }

    #[test]
    fn transitions_artifact_records_withdrawal_timestamp_fallback() {
        use crate::domain::observation::{Asn, CollectorId, EvidenceRef};
        use crate::domain::route::{
            AnalysisPhase, EvidencedRouteState, GenericTransitionEffects, RouteKey, RouteState,
            RouteTransition, TransitionKind,
        };
        use chrono::TimeZone;

        let t = |secs: i64| {
            chrono::Utc.with_ymd_and_hms(2019, 8, 21, 4, 0, 0).unwrap()
                + chrono::Duration::seconds(secs)
        };
        let ev = |id: u64| EvidenceRef {
            observation_id: crate::domain::observation::ObservationId(id),
            source_url: None,
            archive_sha256: Some("abc123".to_string()),
            collector: CollectorId("route-views2".to_string()),
            peer_ip: "128.223.51.110".parse().unwrap(),
            peer_asn: Asn(6447),
            prefix: crate::domain::route::Prefix::from("192.0.2.0/24"),
            timestamp: t(100),
            element_seq: 0,
            path_id: None,
        };
        let key = RouteKey::new(
            "route-views2",
            "128.223.51.110".parse().unwrap(),
            &crate::domain::route::Prefix::from("192.0.2.0/24"),
        );
        // Withdrawal: `to` has no state; timestamp falls back to evidence.
        let wd = RouteTransition::new(
            key.clone(),
            None,
            Some(EvidencedRouteState::present(
                RouteState {
                    prefix: crate::domain::route::Prefix::from("192.0.2.0/24"),
                    attributes: crate::domain::route::RouteAttributes::from_as_path(vec![
                        6447, 65002, 65001,
                    ]),
                    timestamp: t(90),
                    observer: "route-views2:128.223.51.110".into(),
                    path_id: None,
                },
                ev(1),
            )),
            EvidencedRouteState::absent(ev(2)),
            ev(2),
            TransitionKind::Withdrawal,
            GenericTransitionEffects::default(),
            AnalysisPhase::Event,
        );
        // Path replacement: material change.
        let effects = GenericTransitionEffects {
            material_path_changed: true,
            ..Default::default()
        };
        let pr = RouteTransition::new(
            key,
            None,
            Some(EvidencedRouteState::present(
                RouteState {
                    prefix: crate::domain::route::Prefix::from("192.0.2.0/24"),
                    attributes: crate::domain::route::RouteAttributes::from_as_path(vec![
                        6447, 65002, 65001,
                    ]),
                    timestamp: t(200),
                    observer: "route-views2:128.223.51.110".into(),
                    path_id: None,
                },
                ev(3),
            )),
            EvidencedRouteState::present(
                RouteState {
                    prefix: crate::domain::route::Prefix::from("192.0.2.0/24"),
                    attributes: crate::domain::route::RouteAttributes::from_as_path(vec![
                        6447, 9999, 65001,
                    ]),
                    timestamp: t(300),
                    observer: "route-views2:128.223.51.110".into(),
                    path_id: None,
                },
                ev(4),
            ),
            ev(4),
            TransitionKind::PathReplacement {
                old: crate::domain::route::AsPath(vec![6447, 65002, 65001]),
                new: crate::domain::route::AsPath(vec![6447, 9999, 65001]),
            },
            effects,
            AnalysisPhase::Event,
        );

        let outcome = sample_outcome();
        let collectors = vec![];
        let ribs = vec![];
        let updates = vec![];
        let transitions = vec![wd, pr];
        let waves = vec![];
        let semantic_waves = vec![];
        let lifecycles = vec![];
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
        let path = dir.path().join("transitions.json");
        write_transitions(&ctx, &path).unwrap();
        let v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(v["schema_version"], 1);
        assert_eq!(v["event_id"], "TEST");
        let rows = v["transitions"].as_array().unwrap();
        assert_eq!(rows.len(), 2);
        // Withdrawal: absent state falls back to evidence timestamp.
        assert_eq!(rows[0]["kind"], "Withdrawal");
        assert_eq!(rows[0]["withdrawn"], true);
        assert_eq!(rows[0]["announced"], false);
        assert_eq!(rows[0]["occurred_utc"], t(100).to_rfc3339());
        assert_eq!(rows[0]["collector"], "route-views2");
        assert_eq!(rows[0]["observation_id"], 2);
        assert_eq!(rows[0]["archive_sha256"], "abc123");
        // Path replacement: state timestamp, material flag, kind label.
        assert_eq!(rows[1]["kind"], "PathReplacement");
        assert_eq!(rows[1]["material_path_changed"], true);
        assert_eq!(rows[1]["occurred_utc"], t(300).to_rfc3339());
        assert_eq!(rows[1]["phase"], "Event");
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
    fn ris_report_names_ripe_ris_not_routeviews() {
        // A run whose manifest family is RIPE RIS must name RIPE RIS in
        // its report and never claim RouteViews collectors.
        use crate::domain::assessment::{Evidence, Verdict};
        use crate::domain::event::EventId;
        use crate::domain::expectation::{ExpectationKind, ImpactExpectation};
        use chrono::{TimeZone, Utc};
        let outcome = AnalysisOutcome::completed(EventAssessment {
            event_id: EventId::from("TEST"),
            expectation: ImpactExpectation {
                kind: ExpectationKind::Redundant,
                description: "test".into(),
                provenance: "test".into(),
            },
            verdict: Verdict::ExpectedLossOfReachability,
            evidence: vec![Evidence {
                description: "none".into(),
                source_records: vec![],
            }],
            waves: vec![],
            generated_at: Utc.with_ymd_and_hms(2019, 8, 21, 17, 0, 0).unwrap(),
        });
        let collectors = vec!["rrc00".to_string()];
        let ribs = vec![];
        let updates = vec![];
        let transitions = vec![];
        let waves = vec![];
        let semantic_waves = vec![];
        let lifecycles = vec![];
        let limitations = vec![];
        let ctx = OutputContext {
            outcome: &outcome,
            event_id: "TEST",
            ticket_title: "Test Ticket",
            event_window: "2019-08-21 16:00:00 UTC - 2019-08-21 17:30:00 UTC",
            warmup_window: "2019-08-21 02:00:00 UTC - 2019-08-21 16:00:00 UTC",
            cooldown_window: "2019-08-21 17:30:00 UTC - 2019-08-21 18:30:00 UTC",
            declared_expectation: "Redundant",
            target_predicate: "origin AS2603 AND AS11537 in path",
            collectors: &collectors,
            source_family_label: "RIPE RIS",
            selected_ribs: &ribs,
            selected_updates: &updates,
            preflight: None,
            continuity: "Known",
            transitions: &transitions,
            waves: &waves,
            semantic_waves: &semantic_waves,
            lifecycles: &lifecycles,
            ticket_lifecycle: "Closed",
            expectation_kind_label: "redundant-attachment",
            transit_predicate_identity: "ContainsAny[11537]",
            transit_predicate: None,
            requested_collectors: &collectors,
            limitations: &limitations,
            no_observable_impact: false,
        };
        let dir = tempfile::tempdir().unwrap();
        write_outputs(&ctx, dir.path()).unwrap();
        let limitations = std::fs::read_to_string(dir.path().join("limitations.json")).unwrap();
        assert!(
            limitations.contains("RIPE RIS"),
            "RIS run must name RIPE RIS, got: {limitations}"
        );
        assert!(
            !limitations.contains("RouteViews"),
            "RIS run must not be mislabeled as RouteViews"
        );
        let report = std::fs::read_to_string(dir.path().join("report.txt")).unwrap();
        assert!(report.contains("rrc00"));
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
        assert!(report.contains("Insufficient visibility"));
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
        assert!(report.contains("Analysis incomplete"));
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
        assert!(report.contains("selected public collectors"));
        assert!(report.contains("does not measure traffic, circuit state,"));
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
        let result = report.find("Result").unwrap();
        let hints = report.find("Observable mechanism hints").unwrap();
        let method = report.find("Method and limitations").unwrap();
        assert!(result < hints, "result precedes mechanism hints");
        assert!(hints < method, "mechanism hints precede method/limitations");
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
        assert!(report.contains("Selected streams:  2 observer-prefix streams"));
        assert!(report.contains("Baseline routes:   2 route instances"));
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
        assert!(report.contains("Observer-prefix streams that became absent: 1"));
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
        assert!(report.contains("Not directly observable here:"));
        assert!(report.contains("RFC 9003 administrative-shutdown message"));
        assert!(report.contains("RFC 8327 operational intent"));
        assert!(report.contains("Graceful Restart negotiation state"));
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

    // ── analyst-facing output tests ───────────────────

    use crate::domain::route::TransitPredicate;

    /// Owns every value a rendered report needs, so OutputContext borrows
    /// live data for the whole test.
    struct TestCtx {
        outcome: AnalysisOutcome,
        collectors: Vec<String>,
        ribs: Vec<CachedArchive>,
        updates: Vec<CachedArchive>,
        transitions: Vec<RouteTransition>,
        waves: Vec<ImpactWave>,
        semantic: Vec<crate::lifecycle::SemanticWave>,
        lifecycles: Vec<crate::lifecycle::StreamLifecycle>,
        limitations: Vec<String>,
        predicate: Option<TransitPredicate>,
    }

    impl TestCtx {
        fn new(
            outcome: AnalysisOutcome,
            collectors: Vec<String>,
            lifecycles: Vec<crate::lifecycle::StreamLifecycle>,
        ) -> Self {
            TestCtx {
                outcome,
                collectors,
                ribs: vec![],
                updates: vec![],
                transitions: vec![],
                waves: vec![],
                semantic: vec![],
                lifecycles,
                limitations: vec![],
                predicate: None,
            }
        }

        fn ctx(&self) -> OutputContext<'_> {
            OutputContext {
                outcome: &self.outcome,
                event_id: "TEST",
                ticket_title: "Test Ticket",
                event_window: "2026-07-30 09:25:00 UTC - 2026-07-30 09:47:00 UTC",
                warmup_window: "2026-07-30 08:25:00 UTC - 2026-07-30 09:25:00 UTC",
                cooldown_window: "2026-07-30 09:47:00 UTC - 2026-07-30 10:47:00 UTC",
                declared_expectation: "Redundant: Parenthesized site code (NEWA)",
                target_predicate: "origin AS3333 AND baseline AS path contains AS11537",
                collectors: &self.collectors,
                source_family_label: "RouteViews",
                selected_ribs: &self.ribs,
                selected_updates: &self.updates,
                preflight: None,
                continuity: "Known (no gaps)",
                transitions: &self.transitions,
                waves: &self.waves,
                semantic_waves: &self.semantic,
                lifecycles: &self.lifecycles,
                ticket_lifecycle: "Closed",
                expectation_kind_label: "redundant-attachment",
                transit_predicate_identity: "ContainsAny[11537]",
                transit_predicate: self.predicate.as_ref(),
                requested_collectors: &self.collectors,
                limitations: &self.limitations,
                no_observable_impact: true,
            }
        }

        fn report(&self) -> String {
            render_report_txt(&self.ctx())
        }
    }

    fn lifecycle_with(
        category: crate::lifecycle::StreamCategory,
        was_withdrawn: bool,
        restored: bool,
        not_restored: bool,
        ambiguous: bool,
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
                restored,
                not_restored,
                multiple_cycles: false,
                add_path_ambiguous: ambiguous,
            },
            first_change: None,
            transitions: vec![],
            min_absence_secs: None,
            max_absence_secs: None,
            was_withdrawn,
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

    fn no_change_ctx() -> TestCtx {
        use crate::lifecycle::StreamCategory;
        TestCtx::new(
            sample_outcome(),
            vec!["route-views2".to_string(), "route-views6".to_string()],
            vec![lifecycle_with(
                StreamCategory::Unchanged,
                false,
                false,
                false,
                false,
            )],
        )
    }

    fn partial_outcome() -> AnalysisOutcome {
        use crate::domain::assessment::{EventAssessment, Evidence, Verdict};
        use crate::domain::event::EventId;
        use crate::domain::expectation::ImpactExpectation;
        AnalysisOutcome::Completed {
            assessment: EventAssessment {
                event_id: EventId::from("T"),
                expectation: ImpactExpectation::participant_unavailable("t"),
                verdict: Verdict::PartialImpact,
                evidence: vec![Evidence {
                    description: "x".into(),
                    source_records: vec![],
                }],
                waves: vec![],
                generated_at: chrono::Utc::now(),
            },
        }
    }

    #[test]
    fn completed_report_uses_external_bgp_heading() {
        let report = no_change_ctx().report();
        assert!(report.contains("EXTERNAL BGP EVENT ANALYSIS"));
        assert!(!report.contains("INTERNET IMPACT ANALYSIS"));
    }

    #[test]
    fn no_change_verdict_displays_route_state_language() {
        let report = no_change_ctx().report();
        assert!(report.contains("No route-state change observed"));
        assert!(!report.contains("NO OBSERVABLE BGP IMPACT"));
    }

    #[test]
    fn partial_verdict_displays_observer_scoped_language() {
        let report = TestCtx::new(
            partial_outcome(),
            vec!["route-views2".to_string()],
            vec![lifecycle_with(
                crate::lifecycle::StreamCategory::Unchanged,
                false,
                false,
                false,
                false,
            )],
        )
        .report();
        assert!(report.contains("Partial routing impact observed"));
        assert!(!report.contains("PARTIAL IMPACT"));
    }

    #[test]
    fn provisional_verdict_says_so_far() {
        use crate::domain::assessment::{EventAssessment, Verdict};
        use crate::domain::event::EventId;
        use crate::domain::expectation::ImpactExpectation;
        for (v, phrase) in [
            (
                Verdict::ProvisionalImpactObserved,
                "Routing impact observed so far",
            ),
            (
                Verdict::ProvisionalNoImpactSoFar,
                "No route-state change observed so far",
            ),
        ] {
            let outcome = AnalysisOutcome::Completed {
                assessment: EventAssessment {
                    event_id: EventId::from("T"),
                    expectation: ImpactExpectation::redundant(Some("SITE"), "t"),
                    verdict: v,
                    evidence: vec![],
                    waves: vec![],
                    generated_at: chrono::Utc::now(),
                },
            };
            let report = TestCtx::new(
                outcome,
                vec!["route-views2".to_string()],
                vec![lifecycle_with(
                    crate::lifecycle::StreamCategory::Unchanged,
                    false,
                    false,
                    false,
                    false,
                )],
            )
            .report();
            assert!(report.contains(phrase), "{phrase}");
            assert!(report.contains("so far"));
            assert!(report.contains("provisional"));
        }
    }

    #[test]
    fn blocked_plan_has_no_impact_verdict() {
        use crate::domain::assessment::Verdict;
        let blocked = crate::plan::AnalysisPlanStatus::Blocked {
            reason: crate::plan::AnalysisBlockReason::MissingReviewedTransitPredicate,
        };
        assert!(matches!(
            blocked,
            crate::plan::AnalysisPlanStatus::Blocked { .. }
        ));
        // No impact verdict exists for blocked plans.
        for v in [
            Verdict::NoObservableBgpImpact,
            Verdict::PartialImpact,
            Verdict::ProvisionalImpactObserved,
        ] {
            assert!(!v.human_label().contains("blocked"));
        }
        let plan = crate::plan::PlanArtifact::from_plan(&crate::plan::AnalysisPlan {
            event_id: "INC0301970".into(),
            expectation:
                crate::domain::expectation::ImpactExpectation::peer_relationship_unavailable("t"),
            lifecycle: crate::domain::expectation::TicketLifecycle::Open,
            analysis_window: crate::domain::event::EventWindow {
                start: chrono::Utc::now(),
                end: chrono::Utc::now(),
            },
            entity_origin_asns: vec![11550],
            transit_predicate: crate::plan::TransitPredicateMapping::default(),
            status: blocked,
        });
        let text = plan.render_text();
        assert!(text.contains("Blocked"));
        assert!(!text.contains("Consistent with"));
        assert!(!text.contains("route-state change observed"));
    }

    #[test]
    fn no_change_summary_names_scope_and_zero_change() {
        let report = TestCtx::new(
            sample_outcome(),
            vec!["route-views2".to_string(), "route-views6".to_string()],
            vec![
                lifecycle_with(
                    crate::lifecycle::StreamCategory::Unchanged,
                    false,
                    false,
                    false,
                    false,
                ),
                lifecycle_with(
                    crate::lifecycle::StreamCategory::Unchanged,
                    false,
                    false,
                    false,
                    false,
                ),
            ],
        )
        .report();
        let flat = report
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(&String::from(' '));
        assert!(flat
            .contains("Across 2 selected observer-prefix streams at route-views2, route-views6"));
        assert!(flat.contains("no announcements, withdrawals, path changes, or community changes"));
    }

    #[test]
    fn partial_summary_reports_stream_categories() {
        use crate::lifecycle::StreamCategory;
        let report = TestCtx::new(
            partial_outcome(),
            vec!["route-views2".to_string()],
            vec![
                lifecycle_with(StreamCategory::Withdrawn, true, true, false, false),
                lifecycle_with(StreamCategory::PrependOnly, false, false, false, false),
                lifecycle_with(
                    StreamCategory::PathChangedStillViaTransit,
                    false,
                    false,
                    false,
                    false,
                ),
                lifecycle_with(
                    StreamCategory::DepartedTransitPath,
                    false,
                    false,
                    false,
                    false,
                ),
                lifecycle_with(StreamCategory::Unchanged, false, false, false, false),
            ],
        )
        .report();
        let flat = report
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(&String::from(' '));
        assert!(flat
            .contains("1 of 5 selected observer-prefix streams became absent and later returned"));
        assert!(flat.contains("1 showed prepend-only changes"));
        assert!(
            flat.contains("1 had other material path changes while retaining the reviewed transit")
        );
        assert!(flat.contains("1 remained visible after departing that transit"));
        assert!(flat.contains("1 remained unchanged"));
    }

    #[test]
    fn summary_values_are_derived_from_report_data() {
        let one = TestCtx::new(
            sample_outcome(),
            vec!["route-views2".to_string()],
            vec![lifecycle_with(
                crate::lifecycle::StreamCategory::Unchanged,
                false,
                false,
                false,
                false,
            )],
        )
        .report();
        let three = TestCtx::new(
            sample_outcome(),
            vec!["route-views2".to_string()],
            vec![
                lifecycle_with(
                    crate::lifecycle::StreamCategory::Unchanged,
                    false,
                    false,
                    false,
                    false,
                ),
                lifecycle_with(
                    crate::lifecycle::StreamCategory::Unchanged,
                    false,
                    false,
                    false,
                    false,
                ),
                lifecycle_with(
                    crate::lifecycle::StreamCategory::Unchanged,
                    false,
                    false,
                    false,
                    false,
                ),
            ],
        )
        .report();
        assert!(one.contains("Across 1 selected observer-prefix stream"));
        assert!(three.contains("Across 3 selected observer-prefix streams"));
        assert_ne!(one, three);
    }

    #[test]
    fn summary_does_not_hardcode_known_event_entities() {
        // The renderer source (before the test module) must not hardcode
        // known event entities in its prose.
        let src = include_str!("output.rs");
        let renderer = src
            .lines()
            .take_while(|l| !l.starts_with("#[cfg(test)]") && !l.starts_with("#[test]"))
            .collect::<Vec<_>>()
            .join("\n");
        for hardcoded in ["RIPE", "UVA", "NEWA", "INC0302574", "INC0299001"] {
            assert!(
                !renderer.contains(hardcoded),
                "renderer must not hardcode {hardcoded}"
            );
        }
    }

    #[test]
    fn summary_uses_streams_as_primary_unit() {
        let report = TestCtx::new(
            sample_outcome(),
            vec!["route-views2".to_string()],
            vec![lifecycle_with(
                crate::lifecycle::StreamCategory::Withdrawn,
                true,
                true,
                false,
                false,
            )],
        )
        .report();
        assert!(report.contains("observer-prefix streams"));
    }

    #[test]
    fn observation_and_expectation_assessment_are_distinct() {
        let report = no_change_ctx().report();
        let result = report.find("Result").unwrap();
        let assessment = report
            .find("Assessment against ticket expectation")
            .unwrap();
        assert!(result < assessment);
        assert!(report.contains("Assessment against ticket expectation"));
        assert!(report.contains("Consistent with the redundant-attachment expectation"));
    }

    #[test]
    fn no_change_does_not_render_redundancy_proven() {
        let report = no_change_ctx().report();
        assert!(report.contains("does not prove that the"));
        assert!(!report.contains("redundancy proven"));
        assert!(!report.contains("proves that the attachment"));
    }

    #[test]
    fn temporal_association_does_not_render_causation() {
        let report = no_change_ctx().report();
        assert!(report.contains("Temporal association is not automatic causation"));
        assert!(!report.contains("caused by the ticket"));
        assert!(!report.contains("because the event"));
    }

    #[test]
    fn blocked_plan_is_not_assessed_against_bgp_evidence() {
        let plan = crate::plan::PlanArtifact::from_plan(&crate::plan::AnalysisPlan {
            event_id: "INC0301970".into(),
            expectation:
                crate::domain::expectation::ImpactExpectation::peer_relationship_unavailable("t"),
            lifecycle: crate::domain::expectation::TicketLifecycle::Open,
            analysis_window: crate::domain::event::EventWindow {
                start: chrono::Utc::now(),
                end: chrono::Utc::now(),
            },
            entity_origin_asns: vec![11550],
            transit_predicate: crate::plan::TransitPredicateMapping::default(),
            status: crate::plan::AnalysisPlanStatus::Blocked {
                reason: crate::plan::AnalysisBlockReason::MissingReviewedTransitPredicate,
            },
        });
        let text = plan.render_text();
        assert!(!text.contains("BGP evidence"));
        assert!(!text.contains("Consistent"));
        assert!(text.contains("Blocked"));
    }

    #[test]
    fn every_summary_count_has_an_explicit_unit() {
        use crate::lifecycle::StreamCategory;
        let report = TestCtx::new(
            sample_outcome(),
            vec!["route-views2".to_string()],
            vec![
                lifecycle_with(StreamCategory::Withdrawn, true, true, false, false),
                lifecycle_with(StreamCategory::PrependOnly, false, false, false, false),
            ],
        )
        .report();
        assert!(report.contains("observer-prefix streams"));
        assert!(report.contains("route instances"));
        assert!(report.contains("Route-instance transitions"));
        assert!(!report.contains("Withdrawals: 1\n"), "{report}");
    }

    #[test]
    fn text_report_does_not_repeat_compact_lifecycle_dump() {
        let report = no_change_ctx().report();
        assert!(!report.contains("Stream lifecycle: total="), "{report}");
    }

    #[test]
    fn report_contains_no_stale_departed_i2_label() {
        let report = no_change_ctx().report();
        assert!(!report.contains("departed-I2"), "{report}");
    }

    #[test]
    fn report_distinguishes_stream_and_transition_withdrawals() {
        use crate::lifecycle::StreamCategory;
        let report = TestCtx::new(
            sample_outcome(),
            vec!["route-views2".to_string()],
            vec![lifecycle_with(
                StreamCategory::Withdrawn,
                true,
                true,
                false,
                false,
            )],
        )
        .report();
        assert!(report.contains("Observer-prefix streams that became absent: 1"));
        assert!(report.contains("Route-instance withdrawal transitions: 0"));
    }

    #[test]
    fn concise_no_change_report_suppresses_irrelevant_zero_categories() {
        let report = no_change_ctx().report();
        assert!(report.contains("Unchanged streams:            1"));
        assert!(
            report.contains("Streams restored after absence: Not applicable"),
            "restoration must be Not applicable when no stream changed"
        );
    }

    #[test]
    fn concise_partial_report_includes_every_nonzero_category() {
        use crate::lifecycle::StreamCategory;
        let report = TestCtx::new(
            sample_outcome(),
            vec!["route-views2".to_string()],
            vec![
                lifecycle_with(StreamCategory::Withdrawn, true, true, false, false),
                lifecycle_with(StreamCategory::PrependOnly, false, false, false, false),
                lifecycle_with(
                    StreamCategory::PathChangedStillViaTransit,
                    false,
                    false,
                    false,
                    false,
                ),
                lifecycle_with(
                    StreamCategory::DepartedTransitPath,
                    false,
                    false,
                    false,
                    false,
                ),
            ],
        )
        .report();
        for needle in [
            "Prepend-only streams:         1",
            "Material change, still via reviewed transit: 1",
            "Streams departing the reviewed transit: 1",
            "Observer-prefix streams that became absent: 1",
            "Streams restored after absence: 1",
        ] {
            assert!(report.contains(needle), "missing {needle}");
        }
    }

    #[test]
    fn evidence_details_include_complete_zero_and_nonzero_counts() {
        let report = no_change_ctx().report();
        assert!(report.contains("Route-instance transitions: 0"));
        assert!(report.contains("Route-instance withdrawal transitions: 0"));
        assert!(report.contains("Material path changes (transitions): 0"));
        assert!(report.contains("Unchanged streams:            1"));
    }

    #[test]
    fn unresolved_and_ambiguous_counts_are_never_hidden_when_nonzero() {
        use crate::lifecycle::StreamCategory;
        let report = TestCtx::new(
            sample_outcome(),
            vec!["route-views2".to_string()],
            vec![
                lifecycle_with(StreamCategory::Withdrawn, true, false, true, false),
                lifecycle_with(StreamCategory::Unchanged, false, false, false, true),
            ],
        )
        .report();
        let flat = report
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(&String::from(' '));
        assert!(flat.contains("Unresolved streams: 1"));
        assert!(flat.contains("ADD-PATH ambiguous streams: 1"));
        assert!(flat.contains("ambiguous ADD-PATH continuity"));
    }

    #[test]
    fn collector_list_is_human_formatted() {
        let report = no_change_ctx().report();
        assert!(report.contains("route-views2, route-views6"));
        assert!(!report.contains('['), "no Rust debug collections in text");
        assert!(!report.contains(']'));
    }

    #[test]
    fn contains_any_has_human_description() {
        assert_eq!(
            TransitPredicate::ContainsAny(vec![11537]).human_description(),
            "at least one route path traversed AS11537"
        );
        assert_eq!(
            TransitPredicate::ContainsAny(vec![11537, 11538]).human_description(),
            "at least one route path traversed one of AS11537, AS11538"
        );
    }

    #[test]
    fn contains_all_has_human_description() {
        assert_eq!(
            TransitPredicate::ContainsAll(vec![11537, 11538]).human_description(),
            "route path contained all of AS11537, AS11538"
        );
    }

    #[test]
    fn adjacency_has_human_description() {
        assert_eq!(
            TransitPredicate::Adjacent(11537, 11538).human_description(),
            "route path contained the adjacency AS11537 \u{2192} AS11538"
        );
    }

    #[test]
    fn exact_predicate_remains_in_evidence_details() {
        use crate::lifecycle::StreamCategory;
        let mut tc = TestCtx::new(
            sample_outcome(),
            vec!["route-views2".to_string()],
            vec![lifecycle_with(
                StreamCategory::Unchanged,
                false,
                false,
                false,
                false,
            )],
        );
        tc.predicate = Some(TransitPredicate::ContainsAny(vec![11537]));
        let report = tc.report();
        assert!(report.contains("at least one route path traversed AS11537"));
        assert!(report.contains("TransitPredicate:   ContainsAny { 11537 }"));
    }

    #[test]
    fn complete_coverage_is_scoped_to_selected_plan() {
        let s = render_archive_coverage(
            &["route-views2".to_string(), "route-views6".to_string()],
            &["route-views2".to_string(), "route-views6".to_string()],
            &[],
        );
        assert!(s.contains("Complete for the selected analysis plan"));
        assert!(
            !s.starts_with("Complete."),
            "coverage must not be global: {s}"
        );
    }

    #[test]
    fn collector_without_targets_is_not_called_archive_failure() {
        let s = render_archive_coverage(
            &["route-views2".to_string(), "route-views6".to_string()],
            &["route-views2".to_string()],
            &[],
        );
        assert!(s.contains(
            "route-views6 had no qualifying baseline target streams after RIB preflight"
        ));
        assert!(!s.to_lowercase().contains("failure"));
        assert!(!s.to_lowercase().contains("incomplete"));
    }

    #[test]
    fn missing_selected_archive_renders_incomplete_coverage() {
        let s = render_archive_coverage(
            &["route-views2".to_string(), "route-views6".to_string()],
            &["route-views2".to_string()],
            &["failed to cache UPDATE: download failed".to_string()],
        );
        assert!(s.contains("Incomplete because a selected archive could not be acquired"));
    }

    #[test]
    fn negative_finding_requires_coverage_statement() {
        let report = no_change_ctx().report();
        assert!(report.contains("Archive coverage:"));
        assert!(report.contains("Complete for the selected analysis plan"));
    }

    #[test]
    fn mechanism_zero_case_is_compact() {
        let report = no_change_ctx().report();
        assert!(report
            .contains("No RFC 8326 GRACEFUL_SHUTDOWN community reached the selected observers."));
        assert!(
            report.contains("Its absence does not establish that graceful shutdown was not used.")
        );
    }

    #[test]
    fn nonobservable_mechanisms_are_grouped() {
        let report = no_change_ctx().report();
        let idx = report.find("Not directly observable here:").unwrap();
        let rfc9003 = report
            .find("RFC 9003 administrative-shutdown message")
            .unwrap();
        let rfc8327 = report.find("RFC 8327 operational intent").unwrap();
        let gr = report.find("Graceful Restart negotiation state").unwrap();
        assert!(idx < rfc9003 && rfc9003 < rfc8327 && rfc8327 < gr);
        assert!(report.matches("not observable").count() <= 3);
    }

    #[test]
    fn gshut_presence_reports_stream_count_and_timing() {
        let mut lc = lifecycle_with(
            crate::lifecycle::StreamCategory::Unchanged,
            false,
            false,
            false,
            false,
        );
        lc.graceful_shutdown_seen = true;
        lc.first_gshut_timestamp = Some(chrono::Utc::now());
        lc.last_gshut_timestamp = Some(chrono::Utc::now());
        let report =
            TestCtx::new(sample_outcome(), vec!["route-views2".to_string()], vec![lc]).report();
        assert!(report.contains(
            "GRACEFUL_SHUTDOWN community was observed on 1 selected observer-prefix stream(s)"
        ));
        assert!(report.contains("first="));
        assert!(report.contains("does not imply it caused any subsequent route-state change"));
    }

    #[test]
    fn mechanism_section_does_not_change_assessment() {
        let a = no_change_ctx().report();
        let b = no_change_ctx().report();
        assert!(a.contains("Consistent with the redundant-attachment expectation"));
        assert_eq!(a, b);
    }

    #[test]
    fn concise_limit_appears_near_result() {
        let report = no_change_ctx().report();
        let result = report.find("Result").unwrap();
        let lim = report.find("Important limitation").unwrap();
        let details = report.find("Evidence details").unwrap();
        assert!(result < lim && lim < details);
        assert!(report.contains("does not measure traffic, circuit state,"));
    }

    #[test]
    fn detailed_limitations_remain_available() {
        let report = no_change_ctx().report();
        assert!(report.contains("Method and limitations"));
        assert!(report.contains("Selected collectors do not provide global visibility."));
        assert!(report.contains("BGP route state is not traffic measurement."));
        assert!(report.contains("Local BGP session state is not directly observed."));
        assert!(report.contains("Physical-link state is not observed."));
        assert!(
            report.contains("Absent optional communities prove nothing about mechanism non-use.")
        );
        assert!(report.contains("Temporal association is not automatic causation."));
    }

    #[test]
    fn report_never_claims_global_reachability() {
        let report = no_change_ctx().report();
        assert!(!report.contains("global reachability was"));
        assert!(!report.contains("globally reachable"));
    }

    #[test]
    fn report_never_claims_traffic_impact_from_route_state() {
        let report = no_change_ctx().report();
        assert!(!report.contains("traffic impact"));
        assert!(!report.contains("traffic was"));
    }

    #[test]
    fn report_uses_no_causal_claim() {
        let report = no_change_ctx().report();
        assert!(!report.contains("caused by"));
        assert!(!report.contains("because of the ticket"));
    }

    #[test]
    fn report_primary_result_precedes_detailed_counters() {
        let report = no_change_ctx().report();
        let result = report.find("Result").unwrap();
        let counters = report.find("Lifecycle counts").unwrap();
        let details = report.find("Evidence details").unwrap();
        assert!(result < counters && result < details);
    }

    #[test]
    fn no_change_report_is_not_dominated_by_zero_values() {
        let report = no_change_ctx().report();
        let first_screen = &report[..report.find("Evidence details").unwrap_or(report.len())];
        assert!(first_screen.contains("No route-state change observed"));
        assert!(first_screen.contains("Across 1 selected observer-prefix stream"));
        assert!(
            first_screen.matches(": 0").count() <= 6,
            "concise layer should not parade zeros"
        );
    }

    #[test]
    fn report_answers_expectation_observation_assessment_in_first_section() {
        let report = no_change_ctx().report();
        let first_section = &report[..report.find("Ticket interpretation").unwrap_or(report.len())];
        assert!(first_section.contains("Result"));
        assert!(first_section.contains("Assessment against ticket expectation"));
        assert!(first_section.contains("Observation scope"));
        assert!(first_section.contains("Important limitation"));
    }

    #[test]
    fn report_contains_no_rust_debug_collections() {
        let report = no_change_ctx().report();
        assert!(!report.contains('['));
        assert!(!report.contains(']'));
    }
}
