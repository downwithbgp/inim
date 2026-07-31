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
    pub limitations: &'a [String],
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

    // Evidence appendix (only for Completed outcomes)
    if matches!(ctx.outcome, AnalysisOutcome::Completed { .. }) && !ctx.transitions.is_empty() {
        let appendix_path = out_dir.join("evidence_appendix.jsonl");
        write_evidence_appendix(ctx, &appendix_path)?;
        files.push(appendix_path);
    }

    let limitations_path = out_dir.join("limitations.json");
    write_limitations(ctx, &limitations_path)?;
    files.push(limitations_path);

    Ok(files)
}

// ── report.txt ─────────────────────────────────────────────────────

fn write_report_txt(ctx: &OutputContext, path: &Path) -> Result<(), String> {
    let mut buf = String::new();

    push_ln(&mut buf, "══════════════════════════════════════════");
    push_ln(&mut buf, "  INTERNET IMPACT ANALYSIS");
    push_ln(&mut buf, "══════════════════════════════════════════");
    push_ln(&mut buf, "");

    push_ln(&mut buf, &format!("Event:       {}", ctx.event_id));
    push_ln(&mut buf, &format!("Title:       {}", ctx.ticket_title));
    push_ln(&mut buf, &format!("UTC Window:  {}", ctx.event_window));
    push_ln(&mut buf, &format!("Warmup:      {}", ctx.warmup_window));
    push_ln(&mut buf, &format!("Cooldown:    {}", ctx.cooldown_window));
    push_ln(&mut buf, "");

    push_ln(&mut buf, "── Declared expectation ──────────────────");
    push_ln(&mut buf, ctx.declared_expectation);
    push_ln(&mut buf, "");

    push_ln(&mut buf, "── Target predicate ──────────────────────");
    push_ln(&mut buf, ctx.target_predicate);
    push_ln(&mut buf, "");

    push_ln(&mut buf, "── Collectors ────────────────────────────");
    for c in ctx.collectors {
        push_ln(&mut buf, &format!("  {c}"));
    }
    push_ln(&mut buf, "");

    if let Some(preflight) = ctx.preflight {
        push_ln(&mut buf, "── RIB preflight ─────────────────────────");
        push_ln(&mut buf, &format!("  Collectors requested:      {}", preflight.collectors_requested));
        push_ln(&mut buf, &format!("  Collectors with usable RIBs: {}", preflight.collectors_with_usable_ribs));
        push_ln(&mut buf, &format!("  Origin-matching routes:    {}", preflight.origin_matching_routes));
        push_ln(&mut buf, &format!("  Transit-matching routes:   {}", preflight.transit_matching_routes));
        push_ln(&mut buf, &format!("  Frozen streams:            {}", preflight.frozen_streams));
        push_ln(&mut buf, &format!("  Distinct prefixes:         {}", preflight.distinct_prefixes));
        push_ln(&mut buf, &format!("  Distinct peers:            {}", preflight.distinct_peers));
        push_ln(&mut buf, "");
    }

    push_ln(&mut buf, "── Continuity ────────────────────────────");
    push_ln(&mut buf, ctx.continuity);
    push_ln(&mut buf, "");

    let event_transitions: Vec<_> = ctx.transitions.iter().filter(|t| matches!(t.phase, AnalysisPhase::Event)).collect();
    let cooldown_transitions: Vec<_> = ctx.transitions.iter().filter(|t| matches!(t.phase, AnalysisPhase::Cooldown)).collect();

    push_ln(&mut buf, "── Transitions ───────────────────────────");
    push_ln(&mut buf, &format!("  Total:     {}", ctx.transitions.len()));
    push_ln(&mut buf, &format!("  Event:     {}", event_transitions.len()));
    push_ln(&mut buf, &format!("  Cooldown:  {}", cooldown_transitions.len()));
    push_ln(&mut buf, "");

    push_ln(&mut buf, "── Impact waves ──────────────────────────");
    push_ln(&mut buf, &format!("  Detected:  {}", ctx.waves.len()));
    for wave in ctx.waves {
        push_ln(&mut buf, &format!("  Wave {}: {}  {}-{} ({})",
            wave.id, wave.label, wave.start, wave.end,
            wave.affected_prefixes.len()));
    }
    push_ln(&mut buf, "");

    // Outcome
    push_ln(&mut buf, "── Outcome ───────────────────────────────");
    match ctx.outcome {
        AnalysisOutcome::Completed { assessment } => {
            push_ln(&mut buf, &format!("  Verdict: {}", assessment.verdict));
            for ev in &assessment.evidence {
                push_ln(&mut buf, &format!("  - {}", ev.description));
            }
        }
        AnalysisOutcome::InsufficientVisibility { reason } => {
            push_ln(&mut buf, "  INSUFFICIENT VISIBILITY");
            push_ln(&mut buf, &format!("  {reason}"));
        }
        AnalysisOutcome::Incomplete { failure } => {
            push_ln(&mut buf, "  INCOMPLETE");
            push_ln(&mut buf, &format!("  {failure}"));
        }
    }
    push_ln(&mut buf, "");

    push_ln(&mut buf, "── Limitations ────────────────────────────");
    for lim in ctx.limitations {
        push_ln(&mut buf, &format!("  • {lim}"));
    }
    push_ln(&mut buf, "");

    push_ln(&mut buf, "This analysis uses control-plane observations from selected RouteViews");
    push_ln(&mut buf, "collectors only. Conclusions are observer-scoped and do not claim");
    push_ln(&mut buf, "global reachability or physical-layer attribution.");

    std::fs::write(path, buf).map_err(|e| format!("cannot write {}: {e}", path.display()))
}

// ── report.json ─────────────────────────────────────────────────────

#[derive(Serialize)]
struct JsonReport {
    event_id: String,
    outcome: serde_json::Value,
    transitions: JsonTransitionStats,
    waves: Vec<JsonWaveSummary>,
    limitations: Vec<String>,
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
    let outcome_json = serde_json::to_value(ctx.outcome).unwrap_or_default();
    let event_count = ctx.transitions.iter().filter(|t| matches!(t.phase, AnalysisPhase::Event)).count();
    let cooldown_count = ctx.transitions.iter().filter(|t| matches!(t.phase, AnalysisPhase::Cooldown)).count();

    let report = JsonReport {
        event_id: ctx.event_id.to_string(),
        outcome: outcome_json,
        transitions: JsonTransitionStats {
            total: ctx.transitions.len(),
            event_window: event_count,
            cooldown: cooldown_count,
        },
        waves: ctx.waves.iter().map(|w| JsonWaveSummary {
            id: w.id,
            label: w.label.clone(),
            start: format!("{}", w.start),
            end: format!("{}", w.end),
            affected_prefix_count: w.affected_prefixes.len(),
            motif_id: w.motif.as_ref().map(|m| m.id.clone()),
        }).collect(),
        limitations: ctx.limitations.to_vec(),
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
        ribs: ctx.selected_ribs.iter().map(|a| JsonCachedFile {
            url: a.url.clone(),
            local_path: a.local_path.clone(),
            collector_id: a.collector_id.clone(),
            data_type: a.data_type.clone(),
            size_bytes: a.size,
            sha256: a.sha256.clone(),
        }).collect(),
        updates: ctx.selected_updates.iter().map(|a| JsonCachedFile {
            url: a.url.clone(),
            local_path: a.local_path.clone(),
            collector_id: a.collector_id.clone(),
            data_type: a.data_type.clone(),
            size_bytes: a.size,
            sha256: a.sha256.clone(),
        }).collect(),
    };
    let json = serde_json::to_string_pretty(&manifest)
        .map_err(|e| format!("JSON error: {e}"))?;
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
        (t.to.timestamp(), t.key.collector.clone(), t.key.peer_ip.to_string(), t.key.prefix.0.to_string())
    });

    let mut lines = Vec::new();
    for t in &sorted {
        let wave_id = ctx.waves.iter()
            .find(|w| t.to.timestamp() >= w.start && t.to.timestamp() <= w.end)
            .map(|w| w.id);
        let motif_id = ctx.waves.iter()
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
            "Analysis uses selected RouteViews collectors only. Other observation points may yield different results.".into(),
        ],
        continuity_gaps: ctx.limitations.iter()
            .filter(|l| l.contains("gap"))
            .cloned()
            .collect(),
        add_path: vec![
            "ADD-PATH identity is not preserved. Multiple paths from the same peer for the same prefix may be collapsed.".into(),
        ],
        manual_mappings: vec![
            "Participant-to-ASN mapping is manually supplied in the reviewed manifest. It is not derived automatically from ticket text.".into(),
        ],
        archive_parser: vec![
            "Parsed via bgpkit-parser. Parser limitations apply.".into(),
        ],
        unsupported_conclusions: vec![
            "Physical-layer attribution (e.g. 'the NYIIX circuit failed') is not supported by control-plane observations alone.".into(),
            "Global reachability cannot be asserted from a limited set of observer streams.".into(),
        ],
    };
    let json = serde_json::to_string_pretty(&lim)
        .map_err(|e| format!("JSON error: {e}"))?;
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

    fn make_ctx<'a>(
        outcome: &'a AnalysisOutcome,
        collectors: &'a Vec<String>,
        ribs: &'a Vec<CachedArchive>,
        updates: &'a Vec<CachedArchive>,
        transitions: &'a Vec<RouteTransition>,
        waves: &'a Vec<ImpactWave>,
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
            limitations,
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
        let limitations = vec!["Test limitation 1".to_string(), "Test limitation 2".to_string()];

        let ctx = make_ctx(&outcome, &collectors, &ribs, &updates, &transitions, &waves, &limitations);

        let files_a = write_outputs(&ctx, dir_a.path()).unwrap();
        let files_b = write_outputs(&ctx, dir_b.path()).unwrap();

        assert_eq!(files_a.len(), files_b.len());
        for (fa, fb) in files_a.iter().zip(files_b.iter()) {
            let content_a = std::fs::read_to_string(fa).unwrap();
            let content_b = std::fs::read_to_string(fb).unwrap();
            assert_eq!(content_a, content_b, "output must be deterministic: {}", fa.display());
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
        let limitations = vec!["gap detected".to_string()];
        let ctx = make_ctx(&outcome, &collectors, &ribs, &updates, &transitions, &waves, &limitations);
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
        let limitations = vec![];
        let ctx = make_ctx(&outcome, &collectors, &ribs, &updates, &transitions, &waves, &limitations);
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
        let limitations = vec![];
        let ctx = make_ctx(&outcome, &collectors, &ribs, &updates, &transitions, &waves, &limitations);
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
        let limitations = vec![];
        let ctx = make_ctx(&outcome, &collectors, &ribs, &updates, &transitions, &waves, &limitations);
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
        use crate::domain::route::{AnalysisPhase, RouteKey, TransitionKind};
        use crate::domain::observation::EvidenceRef;
        use crate::domain::route::Prefix;
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
        let limitations = vec![];
        let ctx = make_ctx(&outcome, &collectors, &ribs, &updates, &transitions, &waves, &limitations);
        let dir = tempfile::tempdir().unwrap();
        write_outputs(&ctx, dir.path()).unwrap();

        let appendix = std::fs::read_to_string(dir.path().join("evidence_appendix.jsonl")).unwrap();
        assert!(appendix.contains("baseline"));
        assert!(appendix.contains("after"));
        // Verify one line per transition
        assert_eq!(appendix.lines().count(), 1);
    }
}
