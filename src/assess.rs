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
use crate::lifecycle::StreamLifecycle;

/// Assess whether observed behavior matches the declared expectation.
///
/// Returns an assessment with verdict, evidence, and wave summary.
/// If continuity is Unknown for any relevant collector, strong verdicts
/// are suppressed (Indeterminate or InsufficientVisibility).
///
/// The continuity gate runs BEFORE result derivation from finding
/// cardinality: an empty finding set cannot bypass failed continuity
/// (a gap-free UPDATE sequence is required before "no route-state
/// change" may be concluded).
///
/// When `lifecycles` is provided, the verdict uses per-stream lifecycle
/// evidence rather than raw transition counts.
pub fn assess(
    event_id: EventId,
    expectation: ImpactExpectation,
    transitions: &[RouteTransition],
    waves: Vec<ImpactWave>,
    any_unknown_continuity: bool,
    lifecycles: Option<&[StreamLifecycle]>,
) -> EventAssessment {
    let evidence = collect_evidence(transitions, any_unknown_continuity, lifecycles);
    let verdict = derive_verdict(
        &expectation,
        transitions,
        any_unknown_continuity,
        lifecycles,
    );

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
    lifecycles: Option<&[StreamLifecycle]>,
) -> Vec<Evidence> {
    let mut evidence = Vec::new();

    let total = transitions.len();
    let withdrawals: Vec<_> = transitions
        .iter()
        .filter(|t| matches!(t.kind, TransitionKind::Withdrawal))
        .collect();
    let path_changes: Vec<_> = transitions
        .iter()
        .filter(|t| matches!(t.kind, TransitionKind::PathReplacement { .. }))
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
        description: format!("Withdrawal transitions: {}", withdrawals.len()),
        source_records: withdrawals
            .iter()
            .map(|t| {
                format!(
                    "{}:{} {} at {}",
                    t.key.collector,
                    t.key.peer_ip,
                    t.key.prefix.0,
                    t.to.timestamp()
                )
            })
            .collect(),
    });

    evidence.push(Evidence {
        description: format!("Path-replacement transitions: {}", path_changes.len()),
        source_records: path_changes
            .iter()
            .map(|t| {
                format!(
                    "{}:{} {} at {} ({} → {})",
                    t.key.collector,
                    t.key.peer_ip,
                    t.key.prefix.0,
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
        description: format!(
            "Restorations to baseline (transitions): {}",
            restorations.len()
        ),
        source_records: restorations
            .iter()
            .map(|t| {
                format!(
                    "{}:{} {} at {}",
                    t.key.collector,
                    t.key.peer_ip,
                    t.key.prefix.0,
                    t.to.timestamp()
                )
            })
            .collect(),
    });

    // Lifecycle-derived evidence
    if let Some(lcs) = lifecycles {
        let unchanged = lcs
            .iter()
            .filter(|l| l.category == crate::lifecycle::StreamCategory::Unchanged)
            .count();
        let prepend = lcs
            .iter()
            .filter(|l| l.category == crate::lifecycle::StreamCategory::PrependOnly)
            .count();
        let withdrawn = lcs.iter().filter(|l| l.was_withdrawn).count();
        let departed = lcs
            .iter()
            .filter(|l| l.category == crate::lifecycle::StreamCategory::DepartedTransitPath)
            .count();
        let restored = lcs.iter().filter(|l| l.flags.restored).count();
        let not_restored = lcs.iter().filter(|l| l.flags.not_restored).count();
        let ambiguous = lcs.iter().filter(|l| l.flags.add_path_ambiguous).count();

        evidence.push(Evidence {
            description: format!(
                "Stream lifecycle: total={} unchanged={} prepend-only={} withdrawn={} departed-transit={} restored={} not-restored={} add-path-ambiguous={}",
                lcs.len(), unchanged, prepend, withdrawn, departed, restored, not_restored, ambiguous,
            ),
            source_records: vec![],
        });

        if ambiguous > 0 {
            evidence.push(Evidence {
                description: format!(
                    "⚠️  {ambiguous} stream(s) have ambiguous ADD-PATH continuity (mixed keyed/unkeyed records). Strong stream-level assessment is suppressed for those streams."
                ),
                source_records: vec![],
            });
        }
    }

    if any_unknown_continuity {
        evidence.push(Evidence {
            description:
                "⚠️  Observer continuity could not be confirmed — verdict reliability reduced"
                    .into(),
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
    lifecycles: Option<&[StreamLifecycle]>,
) -> Verdict {
    let has_withdrawals = transitions
        .iter()
        .any(|t| matches!(t.kind, TransitionKind::Withdrawal));
    let has_path_changes = transitions
        .iter()
        .any(|t| matches!(t.kind, TransitionKind::PathReplacement { .. }));
    let has_session_resets = transitions
        .iter()
        .any(|t| matches!(t.kind, TransitionKind::SessionReset));

    // Continuity gate: suppress strong verdicts BEFORE any result
    // derivation from finding cardinality. An empty finding set must not
    // bypass failed continuity: without a gap-free UPDATE sequence (or
    // with a session reset) the absence of findings is not proven, so a
    // "no route-state change" verdict would overstate the observation.
    if any_unknown_continuity || has_session_resets {
        return Verdict::InsufficientVisibility;
    }

    // No observable impact at all (continuity is established here).
    if transitions.is_empty() {
        return match expectation.kind {
            ExpectationKind::ParticipantRelationshipUnavailable => {
                Verdict::UnexpectedContinuedInternet2Path
            }
            ExpectationKind::NonRedundant => Verdict::LessImpactThanExpected,
            ExpectationKind::PeerRelationshipUnavailable => {
                Verdict::UnexpectedContinuedInternet2Path
            }
            _ => Verdict::NoObservableBgpImpact,
        };
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
        ExpectationKind::ParticipantRelationshipUnavailable => {
            // When lifecycle data is available, use set-based rules.
            // Ambiguous streams (mixed keyed/unkeyed ADD-PATH encoding) are
            // excluded from strong conclusions.
            if let Some(lcs) = lifecycles {
                let total = lcs.len();
                if total == 0 {
                    return Verdict::UnexpectedContinuedInternet2Path;
                }
                let strong: Vec<_> = lcs.iter().filter(|l| !l.flags.add_path_ambiguous).collect();
                if strong.is_empty() {
                    // Every stream is ambiguous — no strong assessment possible.
                    return Verdict::InsufficientVisibility;
                }
                let withdrawn = strong.iter().filter(|l| l.was_withdrawn).count();
                let departed = strong
                    .iter()
                    .filter(|l| l.category == crate::lifecycle::StreamCategory::DepartedTransitPath)
                    .count();
                let only_prepend = strong.iter().all(|l| {
                    matches!(
                        l.category,
                        crate::lifecycle::StreamCategory::Unchanged
                            | crate::lifecycle::StreamCategory::PrependOnly
                    )
                });
                let all_affected =
                    withdrawn + departed == strong.len() && (withdrawn > 0 || departed > 0);
                let some_affected = withdrawn > 0 || departed > 0;

                let verdict = if only_prepend && !some_affected {
                    // Only prepend/policy changes, no withdrawals or departures
                    Verdict::PolicyChangeObserved
                } else if departed > 0 && withdrawn == 0 {
                    // Only departures from AS11537, no withdrawals
                    Verdict::ExpectedAlternateRouting
                } else if all_affected {
                    // All streams affected (withdrawn or departed)
                    Verdict::ExpectedParticipantUnavailability
                } else if some_affected {
                    // Proper subset affected
                    Verdict::PartialImpact
                } else {
                    Verdict::UnexpectedContinuedInternet2Path
                };

                // Ambiguous streams cannot support "no impact" claims: if the
                // only clean evidence shows no impact while ambiguous streams
                // exist, the assessment is not strong enough for a verdict.
                let any_ambiguous = lcs.iter().any(|l| l.flags.add_path_ambiguous);
                if any_ambiguous
                    && matches!(
                        verdict,
                        Verdict::UnexpectedContinuedInternet2Path | Verdict::PolicyChangeObserved
                    )
                {
                    Verdict::InsufficientVisibility
                } else {
                    verdict
                }
            } else {
                // Legacy fallback without lifecycle data
                let departures_from_i2 = transitions.iter().any(|t| {
                    matches!(t.kind, TransitionKind::PathReplacement { .. })
                        && t.from
                            .as_ref()
                            .and_then(|e| e.state.as_ref())
                            .map(|s| s.attributes.as_path.0.contains(&11537))
                            .unwrap_or(false)
                        && !t.to.attributes().as_path.0.contains(&11537)
                });
                if has_withdrawals && !departures_from_i2 {
                    Verdict::ExpectedParticipantUnavailability
                } else if departures_from_i2 && !has_withdrawals {
                    Verdict::ExpectedAlternateRouting
                } else if has_withdrawals && departures_from_i2 {
                    Verdict::PartialImpact
                } else {
                    Verdict::UnexpectedContinuedInternet2Path
                }
            }
        }
        ExpectationKind::PeerRelationshipUnavailable => {
            // Same as ParticipantRelationshipUnavailable but for peer entities
            if has_withdrawals {
                Verdict::ExpectedParticipantUnavailability
            } else if has_path_changes {
                Verdict::ExpectedAlternateRouting
            } else {
                Verdict::UnexpectedContinuedInternet2Path
            }
        }
        ExpectationKind::Unknown => Verdict::Indeterminate,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::event::EventId;
    use crate::domain::expectation::ImpactExpectation;
    use crate::domain::observation::EvidenceRef;
    use crate::domain::route::{
        AnalysisPhase, AsPath, EvidencedRouteState, GenericTransitionEffects, Prefix,
        RouteAttributes, RouteKey, RouteState,
    };
    use chrono::{TimeZone, Utc};

    fn t(secs: i64) -> chrono::DateTime<Utc> {
        Utc.with_ymd_and_hms(2025, 6, 15, 5, 25, 0).unwrap() + chrono::Duration::seconds(secs)
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
        RouteTransition::new(
            key,
            None,
            from_ev,
            to_ev,
            ev,
            kind,
            GenericTransitionEffects::default(),
            AnalysisPhase::Event,
        )
    }

    fn path_change(old: Vec<u32>, new: Vec<u32>, at: i64) -> RouteTransition {
        let from = RouteState {
            prefix: Prefix::from("192.0.2.0/24"),
            attributes: RouteAttributes::from_as_path(old),
            timestamp: t(at - 1),
            observer: "rv2:185.1.8.65".into(),
            path_id: None,
        };
        let to = RouteState {
            prefix: Prefix::from("192.0.2.0/24"),
            attributes: RouteAttributes::from_as_path(new),
            timestamp: t(at),
            observer: "rv2:185.1.8.65".into(),
            path_id: None,
        };
        make_transition(
            Some(from),
            to,
            TransitionKind::PathReplacement {
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
            path_id: None,
        };
        let to = RouteState {
            prefix: Prefix::from("192.0.2.0/24"),
            attributes: RouteAttributes::from_as_path(vec![6447, 11537, 1101]),
            timestamp: t(at),
            observer: "rv2:185.1.8.65".into(),
            path_id: None,
        };
        make_transition(Some(from), to, TransitionKind::ReturnToBaseline)
    }

    fn withdrawal(at: i64) -> RouteTransition {
        let from = RouteState {
            prefix: Prefix::from("192.0.2.0/24"),
            attributes: RouteAttributes::from_as_path(vec![6447, 11537, 1101]),
            timestamp: t(at - 1),
            observer: "rv2:185.1.8.65".into(),
            path_id: None,
        };
        let to = RouteState {
            prefix: Prefix::from("192.0.2.0/24"),
            attributes: RouteAttributes::from_as_path(vec![]),
            timestamp: t(at),
            observer: "rv2:185.1.8.65".into(),
            path_id: None,
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
            None,
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
            None,
        );
        assert_eq!(assessment.verdict, Verdict::RedundancyFailureObserved);
    }

    #[test]
    fn non_redundant_with_withdrawal_is_expected() {
        let exp = ImpactExpectation::non_redundant("test");
        let transitions = vec![withdrawal(0), return_to_baseline(10)];
        let assessment = assess(
            EventId::from("TEST"),
            exp,
            &transitions,
            vec![],
            false,
            None,
        );
        assert_eq!(assessment.verdict, Verdict::ExpectedLossOfReachability);
    }

    #[test]
    fn empty_transitions_is_no_impact() {
        let exp = ImpactExpectation::redundant(Some("NEWY32AOA"), "test");
        let assessment = assess(EventId::from("TEST"), exp, &[], vec![], false, None);
        assert_eq!(assessment.verdict, Verdict::NoObservableBgpImpact);
    }

    #[test]
    fn unknown_continuity_suppresses_strong_verdict() {
        let exp = ImpactExpectation::redundant(Some("NEWY32AOA"), "test");
        let transitions = vec![path_change(
            vec![6447, 11537, 1101],
            vec![6447, 237, 1101],
            0,
        )];
        let assessment = assess(
            EventId::from("TEST"),
            exp,
            &transitions,
            vec![],
            true, // unknown continuity
            None,
        );
        assert_eq!(assessment.verdict, Verdict::InsufficientVisibility);
    }

    // ── Continuity-gate ordering (F-1) ──────────────────────────────
    //
    // The continuity/eligibility gate must execute BEFORE result
    // derivation from finding cardinality. An empty finding set must not
    // bypass failed continuity and produce a "no route-state change"
    // verdict: that would overstate the observation when UPDATE archive
    // gaps or session resets mean the absence of findings is not proven.
    //
    // Named helpers document the boolean meaning:
    //   `any_unknown_continuity = true`  → continuity_unknown()
    //   `any_unknown_continuity = false` → continuity_established()

    fn continuity_established() -> bool {
        false
    }

    fn continuity_unknown() -> bool {
        true
    }

    #[test]
    fn continuity_failure_precedes_empty_finding_fallback() {
        let exp = ImpactExpectation::redundant(Some("NEWY32AOA"), "test");
        let assessment = assess(
            EventId::from("TEST"),
            exp,
            &[],
            vec![],
            continuity_unknown(),
            None,
        );
        assert_eq!(
            assessment.verdict,
            Verdict::InsufficientVisibility,
            "continuity failure must gate before the empty-finding fallback"
        );
    }

    #[test]
    fn empty_findings_do_not_imply_no_change_without_continuity() {
        let exp = ImpactExpectation::redundant(Some("NEWY32AOA"), "test");
        let assessment = assess(
            EventId::from("TEST"),
            exp,
            &[],
            vec![],
            continuity_unknown(),
            None,
        );
        assert_ne!(
            assessment.verdict,
            Verdict::NoObservableBgpImpact,
            "absence of findings is not 'no route-state change' when continuity is unknown"
        );
    }

    #[test]
    fn findings_do_not_override_failed_continuity() {
        let exp = ImpactExpectation::redundant(Some("NEWY32AOA"), "test");
        let transitions = vec![path_change(
            vec![6447, 11537, 1101],
            vec![6447, 237, 1101],
            0,
        )];
        let assessment = assess(
            EventId::from("TEST"),
            exp,
            &transitions,
            vec![],
            continuity_unknown(),
            None,
        );
        assert_eq!(assessment.verdict, Verdict::InsufficientVisibility);
    }

    #[test]
    fn successful_continuity_with_empty_findings_uses_correct_existing_result() {
        let exp = ImpactExpectation::redundant(Some("NEWY32AOA"), "test");
        let assessment = assess(
            EventId::from("TEST"),
            exp,
            &[],
            vec![],
            continuity_established(),
            None,
        );
        assert_eq!(
            assessment.verdict,
            Verdict::NoObservableBgpImpact,
            "established continuity + no findings is the existing no-change result"
        );
    }

    #[test]
    fn successful_continuity_with_findings_preserves_existing_result() {
        let exp = ImpactExpectation::redundant(Some("NEWY32AOA"), "test");
        let transitions = vec![path_change(
            vec![6447, 11537, 1101],
            vec![6447, 237, 1101],
            0,
        )];
        let assessment = assess(
            EventId::from("TEST"),
            exp,
            &transitions,
            vec![],
            continuity_established(),
            None,
        );
        assert_eq!(assessment.verdict, Verdict::ExpectedRedundantImpact);
    }

    #[test]
    fn continuity_gate_decision_table() {
        // (continuity, findings present, expected verdict)
        let exp = ImpactExpectation::redundant(Some("NEWY32AOA"), "test");
        let cases = [
            (
                continuity_established(),
                false,
                Verdict::NoObservableBgpImpact,
            ),
            (continuity_unknown(), false, Verdict::InsufficientVisibility),
            (
                continuity_established(),
                true,
                Verdict::ExpectedRedundantImpact,
            ),
            (continuity_unknown(), true, Verdict::InsufficientVisibility),
        ];
        for (continuity, has_findings, expected) in cases {
            let transitions = if has_findings {
                vec![path_change(
                    vec![6447, 11537, 1101],
                    vec![6447, 237, 1101],
                    0,
                )]
            } else {
                vec![]
            };
            let assessment = assess(
                EventId::from("TEST"),
                exp.clone(),
                &transitions,
                vec![],
                continuity,
                None,
            );
            assert_eq!(assessment.verdict, expected);
        }
    }

    #[test]
    fn assessment_is_deterministic() {
        let exp = ImpactExpectation::redundant(Some("NEWY32AOA"), "test");
        let empty_a = assess(
            EventId::from("TEST"),
            exp.clone(),
            &[],
            vec![],
            continuity_unknown(),
            None,
        );
        let empty_b = assess(
            EventId::from("TEST"),
            exp.clone(),
            &[],
            vec![],
            continuity_unknown(),
            None,
        );
        assert_eq!(empty_a.verdict, empty_b.verdict);
        let findings = vec![path_change(
            vec![6447, 11537, 1101],
            vec![6447, 237, 1101],
            0,
        )];
        let f_a = assess(
            EventId::from("TEST"),
            exp.clone(),
            &findings,
            vec![],
            continuity_established(),
            None,
        );
        let f_b = assess(
            EventId::from("TEST"),
            exp,
            &findings,
            vec![],
            continuity_established(),
            None,
        );
        assert_eq!(f_a.verdict, f_b.verdict);
    }

    #[test]
    fn assessment_includes_evidence() {
        let exp = ImpactExpectation::redundant(Some("NEWY32AOA"), "test");
        let transitions = vec![path_change(
            vec![6447, 11537, 1101],
            vec![6447, 237, 1101],
            0,
        )];
        let assessment = assess(
            EventId::from("TEST"),
            exp,
            &transitions,
            vec![],
            false,
            None,
        );
        assert!(!assessment.evidence.is_empty());
        assert!(assessment
            .evidence
            .iter()
            .any(|e| e.description.contains("Path-replacement transitions: 1")));
    }

    #[test]
    fn verdict_independent_of_motif() {
        // Verdict must not change based on whether a motif is present
        use crate::domain::wave::{MotifEvidenceRange, WaveMotif};
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
            None,
        );
        let a2 = assess(
            EventId::from("TEST"),
            exp,
            &transitions,
            vec![wave_with_motif],
            false,
            None,
        );

        assert_eq!(
            a1.verdict, a2.verdict,
            "verdict must be independent of motif presence"
        );
    }

    #[test]
    fn participant_unavailable_withdrawal_is_detected() {
        let exp = ImpactExpectation::participant_unavailable("test");
        let transitions = vec![withdrawal(0)];
        let assessment = assess(
            EventId::from("TEST"),
            exp,
            &transitions,
            vec![],
            false,
            None,
        );
        assert_eq!(
            assessment.verdict,
            Verdict::ExpectedParticipantUnavailability
        );
    }

    #[test]
    fn path_departure_from_as11537_is_alternate_routing() {
        let exp = ImpactExpectation::participant_unavailable("test");
        let transitions = vec![path_change(
            vec![6447, 11537, 3333],
            vec![6447, 237, 3333],
            0,
        )];
        let assessment = assess(
            EventId::from("TEST"),
            exp,
            &transitions,
            vec![],
            false,
            None,
        );
        assert_eq!(assessment.verdict, Verdict::ExpectedAlternateRouting);
    }

    #[test]
    fn alternate_path_without_as11537_is_not_global_withdrawal() {
        // A path departure from AS11537 is alternate routing, not a withdrawal
        let exp = ImpactExpectation::participant_unavailable("test");
        let transitions = vec![path_change(
            vec![6447, 11537, 225],
            vec![6447, 3356, 225],
            0,
        )];
        let assessment = assess(
            EventId::from("TEST"),
            exp,
            &transitions,
            vec![],
            false,
            None,
        );
        assert_ne!(
            assessment.verdict,
            Verdict::ExpectedParticipantUnavailability
        );
        assert_eq!(assessment.verdict, Verdict::ExpectedAlternateRouting);
    }

    #[test]
    fn partial_stream_impact_is_distinguished() {
        let exp = ImpactExpectation::participant_unavailable("test");
        let transitions = vec![
            withdrawal(0),
            path_change(vec![6447, 11537, 225], vec![6447, 3356, 225], 1),
        ];
        let assessment = assess(
            EventId::from("TEST"),
            exp,
            &transitions,
            vec![],
            false,
            None,
        );
        assert_eq!(assessment.verdict, Verdict::PartialImpact);
    }

    #[test]
    fn unchanged_selected_streams_can_yield_unexpected_continued() {
        let exp = ImpactExpectation::participant_unavailable("test");
        // No transitions → no impact → but expectation was unavailability
        let assessment = assess(EventId::from("TEST"), exp, &[], vec![], false, None);
        assert_eq!(
            assessment.verdict,
            Verdict::UnexpectedContinuedInternet2Path
        );
    }

    #[test]
    fn redundant_and_participant_unavailable_use_distinct_assessment_rules() {
        let redundant_exp = ImpactExpectation::redundant(Some("NEWA"), "test");
        let participant_exp = ImpactExpectation::participant_unavailable("test");
        let transitions = vec![withdrawal(0)];
        let a1 = assess(
            EventId::from("T1"),
            redundant_exp,
            &transitions,
            vec![],
            false,
            None,
        );
        let a2 = assess(
            EventId::from("T2"),
            participant_exp,
            &transitions,
            vec![],
            false,
            None,
        );
        assert_ne!(a1.verdict, a2.verdict);
        assert_eq!(a1.verdict, Verdict::RedundancyFailureObserved);
        assert_eq!(a2.verdict, Verdict::ExpectedParticipantUnavailability);
    }
}
