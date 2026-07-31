//! Impact wave detection — temporally concentrated groups of related
//! route transitions across multiple observers.
//!
//! Stability and duration are derived wave properties, never emitted
//! as instantaneous transition symbols.

use chrono::{DateTime, Utc};

use crate::domain::route::RouteTransition;
#[allow(unused_imports)]
use crate::domain::route::TransitionKind;
use crate::domain::wave::{ImpactWave, WaveMotif, MotifEvidenceRange, fnv1a_64};

/// Detect impact waves from a sequence of route transitions.
///
/// Groups transitions into temporal clusters using a simple gap-threshold
/// heuristic. Transitions occurring close together in time are grouped
/// into the same wave; a gap exceeding `max_gap` starts a new wave.
///
/// This is a deterministic clustering, not ML-based anomaly detection.
pub fn detect_waves(
    transitions: &[RouteTransition],
    max_gap: chrono::Duration,
) -> Vec<ImpactWave> {
    if transitions.is_empty() {
        return vec![];
    }

    let mut sorted: Vec<&RouteTransition> = transitions.iter().collect();
    sorted.sort_by_key(|t| t.to.timestamp());

    let mut waves: Vec<ImpactWave> = Vec::new();
    let mut current_group: Vec<&RouteTransition> = vec![sorted[0]];
    let mut wave_start = sorted[0].to.timestamp();
    let mut wave_id: usize = 1;

    for window in sorted.windows(2) {
        let prev = window[0];
        let next = window[1];
        let gap = next.to.timestamp() - prev.to.timestamp();

        if gap <= max_gap {
            current_group.push(next);
        } else {
            waves.push(build_wave(wave_id, &current_group, wave_start));
            wave_id += 1;
            current_group = vec![next];
            wave_start = next.to.timestamp();
        }
    }

    // Final group
    if !current_group.is_empty() {
        waves.push(build_wave(wave_id, &current_group, wave_start));
    }

    waves
}

fn build_wave(id: usize, transitions: &[&RouteTransition], wave_start: DateTime<Utc>) -> ImpactWave {
    let peak = transitions[transitions.len() / 2].to.timestamp();
    let end = transitions.last().map(|t| t.to.timestamp()).unwrap_or(wave_start);

    let mut prefixes: Vec<_> = transitions
        .iter()
        .map(|t| t.to.prefix().clone())
        .collect();
    prefixes.sort();
    prefixes.dedup();

    let mut peers: Vec<_> = transitions
        .iter()
        .map(|t| t.to.observer().to_string())
        .collect();
    peers.sort();
    peers.dedup();

    // SEQUITUR-derived motif
    let motif = sequitur_motif(transitions);

    ImpactWave {
        id,
        label: String::new(), // filled by summarize later
        start: wave_start,
        peak,
        end,
        affected_prefixes: prefixes,
        affected_peers: peers,
        motif,
    }
}

/// Derive a SEQUITUR-based motif from route transitions.
///
/// Groups transitions by route key (observer, prefix), orders each group
/// chronologically, maps TransitionKind → TransitionSymbol, runs SEQUITUR,
/// and builds a WaveMotif with identity, expanded sequence, structure,
/// occurrence count, coverage, and evidence ranges.
fn sequitur_motif(transitions: &[&RouteTransition]) -> Option<WaveMotif> {
    use std::collections::HashMap;
    use crate::sequitur;
    use crate::tokenize::TransitionSymbol;

    if transitions.is_empty() {
        return None;
    }

    // Group transitions by (observer, prefix)
    let mut groups: HashMap<(String, String), Vec<&RouteTransition>> = HashMap::new();
    for t in transitions {
        let key = (t.to.observer().to_string(), t.to.prefix().0.clone());
        groups.entry(key).or_default().push(t);
    }

    // For each group: sort chronologically, build symbol sequence, run SEQUITUR
    struct GroupMotif {
        expanded: Vec<String>,
        structure: Vec<String>,
        scope: String,
        time_start: chrono::DateTime<chrono::Utc>,
        time_end: chrono::DateTime<chrono::Utc>,
    }

    let mut group_motifs: Vec<GroupMotif> = Vec::new();

    for ((observer, prefix), group) in groups.iter_mut() {
        group.sort_by_key(|t| t.to.timestamp());
        let symbols: Vec<TransitionSymbol> = group
            .iter()
            .map(|t| TransitionSymbol::from_kind(&t.kind))
            .collect();

        if symbols.len() >= 2 {
            let grammar = sequitur::build(&symbols);
            let expanded: Vec<String> = grammar.expand().iter().map(|s| s.0.clone()).collect();
            let structure = motif_structure(&grammar, &symbols);

            if !expanded.is_empty() {
                group_motifs.push(GroupMotif {
                    expanded,
                    structure,
                    scope: format!("{observer} {prefix}"),
                    time_start: group.first().map(|t| t.to.timestamp()).unwrap(),
                    time_end: group.last().map(|t| t.to.timestamp()).unwrap(),
                });
            }
        } else if let Some(sym) = symbols.first() {
            group_motifs.push(GroupMotif {
                expanded: vec![sym.0.clone()],
                structure: vec![format!("ROOT → {}", sym.0)],
                scope: format!("{observer} {prefix}"),
                time_start: group.first().map(|t| t.to.timestamp()).unwrap(),
                time_end: group.last().map(|t| t.to.timestamp()).unwrap(),
            });
        }
    }

    if group_motifs.is_empty() {
        return None;
    }

    // Find the most frequent expanded sequence
    let mut motif_counts: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, gm) in group_motifs.iter().enumerate() {
        let key = gm.expanded.join(" ");
        motif_counts.entry(key).or_default().push(i);
    }

    let (best_expanded, best_indices) = motif_counts
        .into_iter()
        .max_by_key(|(_, indices)| indices.len())
        .unwrap();

    let expanded = best_expanded;
    let best_groups: Vec<&GroupMotif> = best_indices.iter().map(|&i| &group_motifs[i]).collect();

    // Build identity hash from the expanded sequence
    let motif_id = fnv1a_64(expanded.as_bytes());

    // Collect structure from the first representative group
    let structure = best_groups.first().map(|g| g.structure.clone()).unwrap_or_default();

    // Scopes
    let scopes: Vec<String> = best_groups.iter().map(|g| g.scope.clone()).collect();

    // Evidence ranges
    let total_transitions = transitions.len();
    let evidence_ranges: Vec<MotifEvidenceRange> = best_groups
        .iter()
        .take(3) // representative up to 3
        .map(|g| {
            let obs = &g.scope.split_whitespace().next().unwrap_or("unknown");
            let pfx = g.scope.split_whitespace().nth(1).unwrap_or("unknown");
            MotifEvidenceRange {
                observer: obs.to_string(),
                prefix: pfx.to_string(),
                time_start: g.time_start,
                time_end: g.time_end,
                transition_start: 0,
                transition_end: g.expanded.len().saturating_sub(1),
            }
        })
        .collect();

    // Coverage
    let covered_terminals: usize = best_groups.iter().map(|g| g.expanded.len()).sum();

    Some(WaveMotif {
        id: motif_id,
        expanded: expanded.clone(),
        structure,
        occurrences: best_groups.len(),
        covered_terminals,
        total_terminals: total_transitions,
        scopes,
        evidence_ranges,
    })
}

/// Build hierarchical structure lines from a SEQUITUR grammar.
fn motif_structure<T: std::fmt::Display + Clone + Eq + std::hash::Hash + std::fmt::Debug>(
    grammar: &crate::sequitur::Grammar<T>,
    _symbols: &[crate::tokenize::TransitionSymbol],
) -> Vec<String> {
    let mut lines = Vec::new();
    // Start rule
    let root: Vec<String> = grammar.start.iter().map(|s| format!("{s}")).collect();
    lines.push(format!("ROOT → {}", root.join(" ")));
    // Rules
    for (&rid, body) in &grammar.rules {
        let inner: Vec<String> = body.iter().map(|s| format!("{s}")).collect();
        lines.push(format!("{rid} → {}", inner.join(" ")));
    }
    lines
}

/// Summarize waves with human-readable labels and stability intervals.
pub fn summarize_waves(waves: &mut [ImpactWave]) {
    for (i, wave) in waves.iter_mut().enumerate() {
        let duration = wave.end - wave.start;
        let motif_desc = describe_motif(wave.motif.as_ref());
        let label = format!(
            "Wave {} — {} ({:.1}s, {} prefixes, {} peers)",
            i + 1,
            motif_desc,
            duration.num_milliseconds() as f64 / 1000.0,
            wave.affected_prefixes.len(),
            wave.affected_peers.len(),
        );
        wave.label = label;
    }
}

/// Classify a WaveMotif as a genuine multi-terminal motif or a single
/// dominant transition (no inferred structure).
pub fn classify_motif(motif: &WaveMotif) -> MotifClass {
    // Single terminal with no hierarchical rules => dominant transition
    let terminal_count = motif.expanded.split_whitespace().count();
    let has_rules = motif.structure.len() > 1;
    if terminal_count <= 1 && !has_rules {
        MotifClass::DominantTransition
    } else {
        MotifClass::GenuineMotif
    }
}

/// Classification of a wave's motif.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MotifClass {
    /// A single repeated terminal with no inferred grammar structure.
    DominantTransition,
    /// A genuine multi-terminal or hierarchical motif.
    GenuineMotif,
}

impl MotifClass {
    pub fn heading(&self) -> &str {
        match self {
            MotifClass::DominantTransition => "Dominant transition",
            MotifClass::GenuineMotif => "Motif",
        }
    }
}

/// Produce a short human-readable description of a SEQUITUR motif.
fn describe_motif(motif: Option<&WaveMotif>) -> &str {
    match motif {
        Some(m) => {
            let expanded = &m.expanded;
            if expanded.contains("PATH_CHANGE") && expanded.contains("RETURN_TO_BASELINE") {
                "failover and restoration"
            } else if expanded.contains("RETURN_TO_BASELINE") {
                "restoration"
            } else if expanded.contains("PATH_CHANGE") && m.structure.len() > 1 {
                "structured path change"
            } else if expanded.contains("PATH_CHANGE") {
                "path change"
            } else if expanded.contains("WITHDRAWAL") {
                "withdrawal"
            } else {
                "activity"
            }
        }
        None => "activity",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::route::{
        AsPath, AnalysisPhase, EvidencedRouteState, Prefix, RouteAttributes, RouteKey, RouteState,
    };
    use crate::domain::observation::EvidenceRef;
    use chrono::TimeZone;

    fn t(secs: i64) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2025, 6, 15, 5, 25, 0)
            .unwrap()
            + chrono::Duration::seconds(secs)
    }

    fn transition(kind: TransitionKind, at: i64) -> RouteTransition {
        let state = RouteState {
            prefix: Prefix::from("192.0.2.0/24"),
            attributes: RouteAttributes::from_as_path(vec![6447, 11537, 1101]),
            timestamp: t(at),
            observer: "rv2:185.1.8.65".into(),
        };
        let key = RouteKey::new("test", "0.0.0.0".parse().unwrap(), &state.prefix);
        let ev = EvidenceRef::synthetic(0, "test", "0000");
        let to_ev = EvidencedRouteState::present(state, ev.clone());
        RouteTransition::new(key, None, None, to_ev, ev, kind, AnalysisPhase::Event)
    }

    #[test]
    fn detect_empty() {
        let waves = detect_waves(&[], chrono::Duration::seconds(30));
        assert!(waves.is_empty());
    }

    #[test]
    fn single_transition_is_one_wave() {
        let t = transition(
            TransitionKind::PathChange {
                old: AsPath(vec![11537]),
                new: AsPath(vec![237, 11537]),
            },
            0,
        );
        let waves = detect_waves(&[t], chrono::Duration::seconds(30));
        assert_eq!(waves.len(), 1);
        assert!(waves[0].motif.as_ref().map(|m| m.expanded.as_str()) == Some("PATH_CHANGE"));
    }

    #[test]
    fn close_transitions_merge_into_one_wave() {
        let t1 = transition(
            TransitionKind::PathChange {
                old: AsPath(vec![11537]),
                new: AsPath(vec![237, 11537]),
            },
            0,
        );
        let t2 = transition(
            TransitionKind::PathChange {
                old: AsPath(vec![237, 11537]),
                new: AsPath(vec![3356, 11537]),
            },
            5, // 5 seconds later, within 30s gap
        );
        let waves = detect_waves(&[t1, t2], chrono::Duration::seconds(30));
        assert_eq!(waves.len(), 1);
    }

    #[test]
    fn far_transitions_split_into_separate_waves() {
        let t1 = transition(
            TransitionKind::PathChange {
                old: AsPath(vec![11537]),
                new: AsPath(vec![237, 11537]),
            },
            0,
        );
        let t2 = transition(
            TransitionKind::ReturnToBaseline,
            60, // 60 seconds later, exceeds 30s gap
        );
        let waves = detect_waves(&[t1, t2], chrono::Duration::seconds(30));
        assert_eq!(waves.len(), 2);
    }

    #[test]
    fn summarize_adds_labels() {
        let t = transition(
            TransitionKind::PathChange {
                old: AsPath(vec![11537]),
                new: AsPath(vec![237, 11537]),
            },
            0,
        );
        let mut waves = detect_waves(&[t], chrono::Duration::seconds(30));
        summarize_waves(&mut waves);
        assert!(waves[0].label.contains("Wave 1"));
        assert!(waves[0].label.contains("path change"));
    }

    #[test]
    fn single_terminal_result_is_not_labeled_as_motif() {
        // A wave with a single-terminal motif should classify as DominantTransition
        use crate::domain::wave::{WaveMotif, MotifEvidenceRange};
        let t0 = t(0);
        let motif = WaveMotif {
            id: "abc".into(),
            expanded: "PATH_CHANGE".into(),
            structure: vec!["ROOT → PATH_CHANGE".into()],
            occurrences: 1,
            covered_terminals: 1,
            total_terminals: 1,
            scopes: vec![],
            evidence_ranges: vec![MotifEvidenceRange {
                observer: "rv2:185.1.8.65".into(),
                prefix: "192.0.2.0/24".into(),
                time_start: t0,
                time_end: t0 + chrono::Duration::seconds(1),
                transition_start: 0,
                transition_end: 0,
            }],
        };
        let class = classify_motif(&motif);
        assert_eq!(class, MotifClass::DominantTransition);
        assert_eq!(class.heading(), "Dominant transition");
    }

    #[test]
    fn multi_terminal_rule_is_labeled_as_motif() {
        use crate::domain::wave::WaveMotif;
        let _t0 = t(0);
        let motif = WaveMotif {
            id: "def".into(),
            expanded: "PATH_CHANGE RETURN_TO_BASELINE".into(),
            structure: vec![
                "ROOT → PATH_CHANGE RETURN_TO_BASELINE".into(),
            ],
            occurrences: 2,
            covered_terminals: 4,
            total_terminals: 4,
            scopes: vec![],
            evidence_ranges: vec![],
        };
        let class = classify_motif(&motif);
        assert_eq!(class, MotifClass::GenuineMotif);
        assert_eq!(class.heading(), "Motif");
    }
}
