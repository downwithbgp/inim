//! Assessment module — expectation-versus-observation logic and verdict
//! derivation with evidence collection.
//!
//! Compares the declared operational expectation against observed route
//! transitions, waves, and continuity state.

use chrono::Utc;

use crate::domain::assessment::{EventAssessment, Evidence, Verdict};
use crate::domain::event::EventId;
use crate::domain::expectation::{ExpectationKind, ImpactExpectation};
use crate::domain::route::{RouteTransition, TransitionKind};
use crate::domain::wave::ImpactWave;

/// Assess whether observed behavior matches the declared expectation.
///
/// Returns an assessment with verdict, evidence, and wave summary.
/// If continuity is Unknown for any relevant collector, strong verdicts
/// are suppressed (Indeterminate or InsufficientVisibility).
pub fn assess(
    event_id: EventId,
    expectation: ImpactExpectation,
    transitions: &[RouteTransition],
    waves: Vec<ImpactWave>,
    any_unknown_continuity: bool,
) -> EventAssessment {
    let evidence = collect_evidence(transitions, any_unknown_continuity);
    let verdict = derive_verdict(&expectation, transitions, any_unknown_continuity);

    EventAssessment {
        event_id,
        expectation,
        verdict,
        evidence,
        waves,
        generated_at: Utc::now(),
    }
}

// ── Evidence collection ────────────────────────────────────────────

fn collect_evidence(
    transitions: &[RouteTransition],
    any_unknown_continuity: bool,
) -> Vec<Evidence> {
    let mut evidence = Vec::new();

    let total = transitions.len();
    let withdrawals: Vec<_> = transitions
        .iter()
        .filter(|t| matches!(t.kind, TransitionKind::Withdrawal))
        .collect();
    let path_changes: Vec<_> = transitions
        .iter()
        .filter(|t| matches!(t.kind, TransitionKind::PathChange { .. }))
        .collect();
    let restorations: Vec<_> = transitions
        .iter()
        .filter(|t| {
            matches!(
                t.kind,
                TransitionKind::Restoration | TransitionKind::ReturnToBaseline
            )
        })
        .collect();

    evidence.push(Evidence {
        description: format!("Total route transitions observed: {total}"),
        source_records: vec![],
    });

    evidence.push(Evidence {
        description: format!("Withdrawals: {}", withdrawals.len()),
        source_records: withdrawals
            .iter()
            .map(|t| format!("{} at {}", t.to.prefix(), t.to.timestamp()))
            .collect(),
    });

    evidence.push(Evidence {
        description: format!("Path changes: {}", path_changes.len()),
        source_records: path_changes
            .iter()
            .map(|t| {
                format!(
                    "{} at {} ({} → {})",
                    t.to.prefix(),
                    t.to.timestamp(),
                    t.from
                        .as_ref()
                        .and_then(|e| e.state.as_ref())
                        .map(|f| f.attributes.as_path.to_string())
                        .unwrap_or_else(|| "none".into()),
                    t.to.attributes().as_path,
                )
            })
            .collect(),
    });

    evidence.push(Evidence {
        description: format!("Restorations to baseline: {}", restorations.len()),
        source_records: restorations
            .iter()
            .map(|t| format!("{} at {}", t.to.prefix(), t.to.timestamp()))
            .collect(),
    });

    if any_unknown_continuity {
        evidence.push(Evidence {
            description: "⚠️  Observer continuity could not be confirmed — verdict reliability reduced".into(),
            source_records: vec![],
        });
    }

    evidence
}

// ── Verdict derivation ─────────────────────────────────────────────

fn derive_verdict(
    expectation: &ImpactExpectation,
    transitions: &[RouteTransition],
    any_unknown_continuity: bool,
) -> Verdict {
    let has_withdrawals = transitions
        .iter()
        .any(|t| matches!(t.kind, TransitionKind::Withdrawal));
    let has_path_changes = transitions
        .iter()
        .any(|t| matches!(t.kind, TransitionKind::PathChange { .. }));
    let _has_restoration = transitions.iter().any(|t| {
        matches!(
            t.kind,
            TransitionKind::Restoration | TransitionKind::ReturnToBaseline
        )
    });
    let has_session_resets = transitions
        .iter()
        .any(|t| matches!(t.kind, TransitionKind::SessionReset));

    // No observable impact at all
    if transitions.is_empty() {
        return Verdict::NoObservableBgpImpact;
    }

    // Continuity gate: suppress strong verdicts
    if any_unknown_continuity || has_session_resets {
        return Verdict::InsufficientVisibility;
    }

    match expectation.kind {
        ExpectationKind::Redundant => {
            if has_withdrawals {
                Verdict::RedundancyFailureObserved
            } else if has_path_changes && !has_withdrawals {
                Verdict::ExpectedRedundantImpact
            } else {
                Verdict::NoObservableBgpImpact
            }
        }
        ExpectationKind::NonRedundant => {
            if has_withdrawals {
                Verdict::ExpectedLossOfReachability
            } else {
                Verdict::LessImpactThanExpected
            }
        }
        ExpectationKind::Unknown => Verdict::Indeterminate,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use crate::domain::event::EventId;
    use crate::domain::expectation::ImpactExpectation;
    use crate::domain::route::{
        AnalysisPhase, AsPath, EvidencedRouteState, Prefix, RouteAttributes, RouteKey, RouteState,
    };
    use crate::domain::observation::EvidenceRef;

    fn t(secs: i64) -> chrono::DateTime<Utc> {
        Utc.with_ymd_and_hms(2025, 6, 15, 5, 25, 0)
            .unwrap()
            + chrono::Duration::seconds(secs)
    }

    fn se() -> EvidenceRef {
        EvidenceRef::synthetic(0, "test", "0000")
    }

    fn make_transition(
        from: Option<RouteState>,
        to: RouteState,
        kind: TransitionKind,
    ) -> RouteTransition {
        let key = RouteKey::new("test", "0.0.0.0".parse().unwrap(), &to.prefix);
        let ev = se();
        let from_ev = from.map(|s| EvidencedRouteState::present(s, ev.clone()));
        let to_ev = EvidencedRouteState::present(to, ev.clone());
        RouteTransition::new(key, None, from_ev, to_ev, ev, kind, AnalysisPhase::Event)
    }

    fn path_change(old: Vec<u32>, new: Vec<u32>, at: i64) -> RouteTransition {
        let from = RouteState {
            prefix: Prefix::from("192.0.2.0/24"),
            attributes: RouteAttributes::from_as_path(old),
            timestamp: t(at - 1),
            observer: "rv2:185.1.8.65".into(),
        };
        let to = RouteState {
            prefix: Prefix::from("192.0.2.0/24"),
            attributes: RouteAttributes::from_as_path(new),
            timestamp: t(at),
            observer: "rv2:185.1.8.65".into(),
        };
        make_transition(
            Some(from), to,
            TransitionKind::PathChange {
                old: AsPath(vec![]),
                new: AsPath(vec![]),
            },
        )
    }

    fn return_to_baseline(at: i64) -> RouteTransition {
        let from = RouteState {
            prefix: Prefix::from("192.0.2.0/24"),
            attributes: RouteAttributes::from_as_path(vec![6447, 237, 1101]),
            timestamp: t(at - 1),
            observer: "rv2:185.1.8.65".into(),
        };
        let to = RouteState {
            prefix: Prefix::from("192.0.2.0/24"),
            attributes: RouteAttributes::from_as_path(vec![6447, 11537, 1101]),
            timestamp: t(at),
            observer: "rv2:185.1.8.65".into(),
        };
        make_transition(Some(from), to, TransitionKind::ReturnToBaseline)
    }

    fn withdrawal(at: i64) -> RouteTransition {
        let from = RouteState {
            prefix: Prefix::from("192.0.2.0/24"),
            attributes: RouteAttributes::from_as_path(vec![6447, 11537, 1101]),
            timestamp: t(at - 1),
            observer: "rv2:185.1.8.65".into(),
        };
        let to = RouteState {
            prefix: Prefix::from("192.0.2.0/24"),
            attributes: RouteAttributes::from_as_path(vec![]),
            timestamp: t(at),
            observer: "rv2:185.1.8.65".into(),
        };
        make_transition(Some(from), to, TransitionKind::Withdrawal)
    }

    #[test]
    fn redundant_with_path_change_only() {
        let exp = ImpactExpectation::redundant(Some("NEWY32AOA"), "test");
        let transitions = vec![
            path_change(vec![6447, 11537, 1101], vec![6447, 237, 1101], 0),
            return_to_baseline(10),
        ];
        let assessment = assess(
            EventId::from("TEST"),
            exp,
            &transitions,
            vec![],
            false,
        );
        assert_eq!(assessment.verdict, Verdict::ExpectedRedundantImpact);
    }

    #[test]
    fn redundant_with_withdrawals_is_failure() {
        let exp = ImpactExpectation::redundant(Some("NEWY32AOA"), "test");
        let transitions = vec![withdrawal(0)];
        let assessment = assess(
            EventId::from("TEST"),
            exp,
            &transitions,
            vec![],
            false,
        );
        assert_eq!(assessment.verdict, Verdict::RedundancyFailureObserved);
    }

    #[test]
    fn non_redundant_with_withdrawal_is_expected() {
        let exp = ImpactExpectation::non_redundant("test");
        let transitions = vec![
            withdrawal(0),
            return_to_baseline(10),
        ];
        let assessment = assess(
            EventId::from("TEST"),
            exp,
            &transitions,
            vec![],
            false,
        );
        assert_eq!(
            assessment.verdict,
            Verdict::ExpectedLossOfReachability
        );
    }

    #[test]
    fn empty_transitions_is_no_impact() {
        let exp = ImpactExpectation::redundant(Some("NEWY32AOA"), "test");
        let assessment = assess(
            EventId::from("TEST"),
            exp,
            &[],
            vec![],
            false,
        );
        assert_eq!(assessment.verdict, Verdict::NoObservableBgpImpact);
    }

    #[test]
    fn unknown_continuity_suppresses_strong_verdict() {
        let exp = ImpactExpectation::redundant(Some("NEWY32AOA"), "test");
        let transitions = vec![
            path_change(vec![6447, 11537, 1101], vec![6447, 237, 1101], 0),
        ];
        let assessment = assess(
            EventId::from("TEST"),
            exp,
            &transitions,
            vec![],
            true, // unknown continuity
        );
        assert_eq!(assessment.verdict, Verdict::InsufficientVisibility);
    }

    #[test]
    fn assessment_includes_evidence() {
        let exp = ImpactExpectation::redundant(Some("NEWY32AOA"), "test");
        let transitions = vec![
            path_change(vec![6447, 11537, 1101], vec![6447, 237, 1101], 0),
        ];
        let assessment = assess(
            EventId::from("TEST"),
            exp,
            &transitions,
            vec![],
            false,
        );
        assert!(!assessment.evidence.is_empty());
        assert!(assessment
            .evidence
            .iter()
            .any(|e| e.description.contains("Path changes: 1")));
    }

    #[test]
    fn verdict_independent_of_motif() {
        // Verdict must not change based on whether a motif is present
        use crate::domain::wave::{WaveMotif, MotifEvidenceRange};
        use chrono::TimeZone;

        let exp = ImpactExpectation::redundant(Some("NEWY32AOA"), "test");
        let transitions = vec![
            path_change(vec![6447, 11537, 1101], vec![6447, 237, 1101], 0),
            return_to_baseline(10),
        ];

        let t0 = Utc.with_ymd_and_hms(2025, 6, 15, 5, 25, 0).unwrap();

        let wave_no_motif = ImpactWave::new("test", t0, t0, t0);
        let wave_with_motif = {
            let mut w = ImpactWave::new("test", t0, t0, t0);
            w.motif = Some(WaveMotif {
                id: "abc123".into(),
                expanded: "PATH_CHANGE RETURN_TO_BASELINE".into(),
                structure: vec!["ROOT → PATH_CHANGE RETURN_TO_BASELINE".into()],
                occurrences: 1,
                covered_terminals: 2,
                total_terminals: 2,
                scopes: vec![],
                evidence_ranges: vec![MotifEvidenceRange {
                    observer: "rv2:185.1.8.65".into(),
                    prefix: "192.0.2.0/24".into(),
                    time_start: t0,
                    time_end: t0 + chrono::Duration::seconds(10),
                    transition_start: 0,
                    transition_end: 2,
                }],
            });
            w
        };

        let a1 = assess(
            EventId::from("TEST"),
            exp.clone(),
            &transitions,
            vec![wave_no_motif],
            false,
        );
        let a2 = assess(
            EventId::from("TEST"),
            exp,
            &transitions,
            vec![wave_with_motif],
            false,
        );

        assert_eq!(a1.verdict, a2.verdict, "verdict must be independent of motif presence");
    }
}
