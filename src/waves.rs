//! Impact wave detection — temporally concentrated groups of related
//! route transitions across multiple observers.
//!
//! Stability and duration are derived wave properties, never emitted
//! as instantaneous transition symbols.

use chrono::{DateTime, Utc};

use crate::domain::route::RouteTransition;
#[allow(unused_imports)]
use crate::domain::route::TransitionKind;
use crate::domain::wave::ImpactWave;

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
    sorted.sort_by_key(|t| t.to.timestamp);

    let mut waves: Vec<ImpactWave> = Vec::new();
    let mut current_group: Vec<&RouteTransition> = vec![sorted[0]];
    let mut wave_start = sorted[0].to.timestamp;

    for window in sorted.windows(2) {
        let prev = window[0];
        let next = window[1];
        let gap = next.to.timestamp - prev.to.timestamp;

        if gap <= max_gap {
            current_group.push(next);
        } else {
            waves.push(build_wave(&current_group, wave_start));
            current_group = vec![next];
            wave_start = next.to.timestamp;
        }
    }

    // Final group
    if !current_group.is_empty() {
        waves.push(build_wave(&current_group, wave_start));
    }

    waves
}

fn build_wave(transitions: &[&RouteTransition], wave_start: DateTime<Utc>) -> ImpactWave {
    let peak = transitions[transitions.len() / 2].to.timestamp;
    let end = transitions.last().map(|t| t.to.timestamp).unwrap_or(wave_start);

    let mut prefixes: Vec<_> = transitions
        .iter()
        .map(|t| t.to.prefix.clone())
        .collect();
    prefixes.sort();
    prefixes.dedup();

    let mut peers: Vec<_> = transitions
        .iter()
        .map(|t| t.to.observer.clone())
        .collect();
    peers.sort();
    peers.dedup();

    // SEQUITUR-derived motif
    let motif = sequitur_motif(transitions);

    ImpactWave {
        label: String::new(), // filled by summarize later
        start: wave_start,
        peak,
        end,
        affected_prefixes: prefixes,
        affected_peers: peers,
        motif: Some(motif),
    }
}

/// Derive a SEQUITUR-based motif from route transitions.
///
/// Groups transitions by route key (observer, prefix), orders each group
/// chronologically, maps TransitionKind → TransitionSymbol, runs SEQUITUR,
/// and returns the most frequent root-expansion string across all groups.
/// Falls back to dominant-kind label if no structured motif is found.
fn sequitur_motif(transitions: &[&RouteTransition]) -> String {
    use std::collections::HashMap;
    use crate::sequitur;
    use crate::tokenize::TransitionSymbol;

    // Group transitions by (observer, prefix)
    let mut groups: HashMap<(String, String), Vec<&RouteTransition>> = HashMap::new();
    for t in transitions {
        let key = (t.to.observer.clone(), t.to.prefix.0.clone());
        groups.entry(key).or_default().push(t);
    }

    // For each group, sort chronologically, build symbol sequence, run SEQUITUR
    let mut motifs: Vec<String> = Vec::new();
    for group in groups.values_mut() {
        group.sort_by_key(|t| t.to.timestamp);
        let symbols: Vec<TransitionSymbol> = group
            .iter()
            .map(|t| TransitionSymbol::from_kind(&t.kind))
            .collect();

        if symbols.len() >= 2 {
            let grammar = sequitur::build(&symbols);
            let motif_str = grammar.render_root();
            if !motif_str.is_empty() {
                motifs.push(motif_str);
            }
        } else if let Some(sym) = symbols.first() {
            motifs.push(sym.0.clone());
        }
    }

    // Most frequent motif across groups
    let mut counts: HashMap<String, usize> = HashMap::new();
    for m in &motifs {
        *counts.entry(m.clone()).or_insert(0) += 1;
    }

    counts
        .into_iter()
        .max_by_key(|(_, count)| *count)
        .map(|(label, _)| label)
        .unwrap_or_else(|| "UNKNOWN".to_string())
}

/// Summarize waves with human-readable labels and stability intervals.
pub fn summarize_waves(waves: &mut [ImpactWave]) {
    for (i, wave) in waves.iter_mut().enumerate() {
        let duration = wave.end - wave.start;
        let motif_desc = describe_motif(wave.motif.as_deref());
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

/// Produce a short human-readable description of a SEQUITUR motif.
fn describe_motif(motif: Option<&str>) -> &str {
    match motif {
        Some(m) if m.contains("PATH_CHANGE") && m.contains("RETURN_TO_BASELINE") => {
            "failover and restoration"
        }
        Some(m) if m.contains("RETURN_TO_BASELINE") => "restoration",
        Some(m) if m.contains("PATH_CHANGE") && m.contains('[') => "structured path change",
        Some(m) if m.contains("PATH_CHANGE") => "path change",
        Some(m) if m.contains("WITHDRAWAL") => "withdrawal",
        Some(m) => m,
        None => "activity",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::route::{
        AsPath, Prefix, RouteAttributes, RouteState,
    };
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
        RouteTransition::new(None, state, kind)
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
        assert!(waves[0].motif.as_deref() == Some("PATH_CHANGE"));
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
}
