//! Assessment types — verdicts, evidence, and event assessments.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::event::EventId;
use super::expectation::ImpactExpectation;
use super::wave::ImpactWave;

/// The verdict for an event assessment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Verdict {
    /// Impact matches the redundant-failover expectation.
    ExpectedRedundantImpact,
    /// Impact matches the loss-of-reachability expectation.
    ExpectedLossOfReachability,
    /// Participant relationship unavailability was observed.
    ExpectedParticipantUnavailability,
    /// Alternate routing preserved reachability for some streams.
    ExpectedAlternateRouting,
    /// Partial impact — some streams affected, others unchanged.
    PartialImpact,
    /// Internet2 path persisted unexpectedly through the event window.
    UnexpectedContinuedInternet2Path,
    /// Only policy-shape changes (prepending) occurred without route withdrawal or
    /// departure from the required transit path.
    PolicyChangeObserved,
    /// Impact observed for an open event (restoration pending).
    ProvisionalImpactObserved,
    /// No impact observed so far for an open event.
    ProvisionalNoImpactSoFar,
    /// Unexpected withdrawals occurred contrary to declared redundancy.
    UnexpectedWithdrawals,
    /// Redundancy failed — reachability was lost when it should have been preserved.
    RedundancyFailureObserved,
    /// Impact extended beyond the declared participant set.
    UnexpectedBlastRadius,
    /// Less impact occurred than declared (e.g. nothing happened).
    LessImpactThanExpected,
    /// No BGP impact was visible from available observers.
    NoObservableBgpImpact,
    /// Available observers did not provide sufficient coverage.
    InsufficientVisibility,
    /// Unable to determine a verdict from available data.
    Indeterminate,
}

impl std::fmt::Display for Verdict {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Verdict::ExpectedRedundantImpact => write!(f, "EXPECTED REDUNDANT IMPACT"),
            Verdict::ExpectedLossOfReachability => write!(f, "EXPECTED LOSS OF REACHABILITY"),
            Verdict::ExpectedParticipantUnavailability => {
                write!(f, "EXPECTED PARTICIPANT UNAVAILABILITY")
            }
            Verdict::ExpectedAlternateRouting => write!(f, "EXPECTED ALTERNATE ROUTING"),
            Verdict::PartialImpact => write!(f, "PARTIAL IMPACT"),
            Verdict::UnexpectedContinuedInternet2Path => {
                write!(f, "UNEXPECTED CONTINUED INTERNET2 PATH")
            }
            Verdict::PolicyChangeObserved => write!(f, "POLICY CHANGE OBSERVED"),
            Verdict::ProvisionalImpactObserved => write!(f, "PROVISIONAL IMPACT OBSERVED"),
            Verdict::ProvisionalNoImpactSoFar => write!(f, "PROVISIONAL NO IMPACT SO FAR"),
            Verdict::UnexpectedWithdrawals => write!(f, "UNEXPECTED WITHDRAWALS"),
            Verdict::RedundancyFailureObserved => write!(f, "REDUNDANCY FAILURE"),
            Verdict::UnexpectedBlastRadius => write!(f, "UNEXPECTED BLAST RADIUS"),
            Verdict::LessImpactThanExpected => write!(f, "LESS IMPACT THAN EXPECTED"),
            Verdict::NoObservableBgpImpact => write!(f, "NO OBSERVABLE BGP IMPACT"),
            Verdict::InsufficientVisibility => write!(f, "INSUFFICIENT VISIBILITY"),
            Verdict::Indeterminate => write!(f, "INDETERMINATE"),
        }
    }
}

impl Verdict {
    /// Precise human-facing label for the verdict.
    ///
    /// These labels describe externally observed BGP route state — they
    /// never claim general Internet impact, traffic measurement, or
    /// physical-layer conclusions.
    pub fn human_label(&self) -> &'static str {
        match self {
            Verdict::ExpectedRedundantImpact => "Expected redundant-attachment impact observed",
            Verdict::ExpectedLossOfReachability => "Expected loss of reachability observed",
            Verdict::ExpectedParticipantUnavailability => {
                "Expected participant unavailability observed"
            }
            Verdict::ExpectedAlternateRouting => "Expected alternate routing observed",
            Verdict::PartialImpact => "Partial routing impact observed",
            Verdict::UnexpectedContinuedInternet2Path => {
                "Unexpected continued reviewed-transit path"
            }
            Verdict::PolicyChangeObserved => "Policy-shape change observed",
            Verdict::ProvisionalImpactObserved => "Routing impact observed so far",
            Verdict::ProvisionalNoImpactSoFar => "No route-state change observed so far",
            Verdict::UnexpectedWithdrawals => "Unexpected withdrawals observed",
            Verdict::RedundancyFailureObserved => "Redundancy failure observed",
            Verdict::UnexpectedBlastRadius => "Unexpected blast radius observed",
            Verdict::LessImpactThanExpected => "Less impact than expected",
            Verdict::NoObservableBgpImpact => "No route-state change observed",
            Verdict::InsufficientVisibility => "Insufficient visibility",
            Verdict::Indeterminate => "Indeterminate",
        }
    }

    /// The observed route-state result, independent of any expectation.
    ///
    /// This is the ONLY presentation used for the route-state observation.
    pub fn observed_result_kind(&self) -> ObservedResultKind {
        match self {
            Verdict::ExpectedRedundantImpact
            | Verdict::ExpectedLossOfReachability
            | Verdict::ExpectedParticipantUnavailability
            | Verdict::ExpectedAlternateRouting
            | Verdict::PartialImpact
            | Verdict::PolicyChangeObserved
            | Verdict::ProvisionalImpactObserved
            | Verdict::UnexpectedWithdrawals
            | Verdict::RedundancyFailureObserved
            | Verdict::UnexpectedBlastRadius => ObservedResultKind::RouteStateChangesObserved,
            Verdict::UnexpectedContinuedInternet2Path
            | Verdict::ProvisionalNoImpactSoFar
            | Verdict::LessImpactThanExpected
            | Verdict::NoObservableBgpImpact => ObservedResultKind::NoRouteStateChangeObserved,
            Verdict::InsufficientVisibility => ObservedResultKind::InsufficientQualifyingVisibility,
            Verdict::Indeterminate => ObservedResultKind::AnalysisIncomplete,
        }
    }

    /// The assessment of the observation against the reviewed ticket
    /// expectation. Distinct from `observed_result_kind()`: the two must
    /// never be merged into one label.
    pub fn expectation_assessment_kind(&self) -> ExpectationAssessmentKind {
        match self {
            Verdict::ExpectedRedundantImpact
            | Verdict::ExpectedLossOfReachability
            | Verdict::ExpectedParticipantUnavailability
            | Verdict::ExpectedAlternateRouting
            | Verdict::NoObservableBgpImpact
            | Verdict::ProvisionalImpactObserved
            | Verdict::ProvisionalNoImpactSoFar => {
                ExpectationAssessmentKind::ConsistentWithReviewedExpectation
            }
            Verdict::PartialImpact => {
                ExpectationAssessmentKind::PartiallyConsistentWithReviewedExpectation
            }
            Verdict::PolicyChangeObserved
            | Verdict::UnexpectedContinuedInternet2Path
            | Verdict::LessImpactThanExpected => {
                ExpectationAssessmentKind::LessExternallyVisibleChangeThanReviewedExpectation
            }
            Verdict::UnexpectedWithdrawals
            | Verdict::RedundancyFailureObserved
            | Verdict::UnexpectedBlastRadius => {
                ExpectationAssessmentKind::MoreExternallyVisibleChangeThanReviewedExpectation
            }
            Verdict::InsufficientVisibility | Verdict::Indeterminate => {
                ExpectationAssessmentKind::NotAssessableFromSelectedPublicObservers
            }
        }
    }

    /// Resolve a stored verdict string (machine enum name or a frozen
    /// deprecated human label) back to the machine verdict, for
    /// presentation of historical runs. Legacy labels are frozen artifact
    /// vocabulary and never change.
    pub fn from_stored(stored: &str) -> Option<Verdict> {
        if let Ok(v) = serde_json::from_str::<Verdict>(&format!("\"{stored}\"")) {
            return Some(v);
        }
        Self::from_deprecated_label(stored)
    }

    /// Frozen legacy presentation labels (retained for artifact
    /// compatibility only). Current presentation must use
    /// `observed_result_kind()` and `expectation_assessment_kind()`.
    pub fn from_deprecated_label(label: &str) -> Option<Verdict> {
        Some(match label {
            "Expected redundant-attachment impact observed" => Verdict::ExpectedRedundantImpact,
            "Expected loss of reachability observed" => Verdict::ExpectedLossOfReachability,
            "Expected participant unavailability observed" => {
                Verdict::ExpectedParticipantUnavailability
            }
            "Expected alternate routing observed" => Verdict::ExpectedAlternateRouting,
            "Partial routing impact observed" => Verdict::PartialImpact,
            "Unexpected continued reviewed-transit path" => {
                Verdict::UnexpectedContinuedInternet2Path
            }
            "Policy-shape change observed" => Verdict::PolicyChangeObserved,
            "Routing impact observed so far" => Verdict::ProvisionalImpactObserved,
            "No route-state change observed so far" => Verdict::ProvisionalNoImpactSoFar,
            "Unexpected withdrawals observed" => Verdict::UnexpectedWithdrawals,
            "Redundancy failure observed" => Verdict::RedundancyFailureObserved,
            "Unexpected blast radius observed" => Verdict::UnexpectedBlastRadius,
            "Less impact than expected" => Verdict::LessImpactThanExpected,
            "No route-state change observed" => Verdict::NoObservableBgpImpact,
            "Insufficient visibility" => Verdict::InsufficientVisibility,
            "Indeterminate" => Verdict::Indeterminate,
            _ => return None,
        })
    }

    /// Whether this verdict is provisional for an open event.
    pub fn is_provisional(&self) -> bool {
        matches!(
            self,
            Verdict::ProvisionalImpactObserved | Verdict::ProvisionalNoImpactSoFar
        )
    }

    /// Assessment posture relative to the ticket expectation.
    pub fn assessment_kind(&self) -> AssessmentKind {
        match self {
            Verdict::ExpectedRedundantImpact
            | Verdict::ExpectedLossOfReachability
            | Verdict::ExpectedParticipantUnavailability
            | Verdict::ExpectedAlternateRouting
            | Verdict::NoObservableBgpImpact => AssessmentKind::Consistent,
            Verdict::PartialImpact => AssessmentKind::PartiallyConsistent,
            Verdict::UnexpectedContinuedInternet2Path
            | Verdict::PolicyChangeObserved
            | Verdict::UnexpectedWithdrawals
            | Verdict::RedundancyFailureObserved
            | Verdict::UnexpectedBlastRadius
            | Verdict::LessImpactThanExpected => AssessmentKind::Inconsistent,
            Verdict::ProvisionalImpactObserved | Verdict::ProvisionalNoImpactSoFar => {
                AssessmentKind::Consistent
            }
            Verdict::InsufficientVisibility => AssessmentKind::NotAssessable,
            Verdict::Indeterminate => AssessmentKind::Indeterminate,
        }
    }
}

/// How the observed signature compares to the ticket expectation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssessmentKind {
    Consistent,
    PartiallyConsistent,
    Inconsistent,
    Indeterminate,
    NotAssessable,
}

impl AssessmentKind {
    pub fn human_label(&self) -> &'static str {
        match self {
            AssessmentKind::Consistent => "Consistent with the",
            AssessmentKind::PartiallyConsistent => "Partially consistent with the",
            AssessmentKind::Inconsistent => "Inconsistent with the",
            AssessmentKind::Indeterminate => "Indeterminate relative to the",
            AssessmentKind::NotAssessable => "Not assessable from the",
        }
    }
}

/// A piece of evidence linking a conclusion to source records.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Evidence {
    pub description: String,
    /// References to source records (e.g. "rv2:AS6447 2025-06-15T05:25:18Z").
    pub source_records: Vec<String>,
}

/// Observed route-state result — independent of any ticket expectation.
///
/// These are the ONLY labels allowed to describe what public BGP showed.
/// They never contain expectation language ("expected", "unexpected") and
/// never claim traffic or service impact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ObservedResultKind {
    /// At least one route-state transition was observed at a selected observer.
    RouteStateChangesObserved,
    /// No route-state transition was observed at the selected observers.
    NoRouteStateChangeObserved,
    /// The selected observers did not provide qualifying baseline visibility.
    InsufficientQualifyingVisibility,
    /// The analysis did not complete (source or parser failure).
    AnalysisIncomplete,
}

impl ObservedResultKind {
    pub fn human_label(&self) -> &'static str {
        match self {
            ObservedResultKind::RouteStateChangesObserved => "Route-state changes observed",
            ObservedResultKind::NoRouteStateChangeObserved => "No route-state change observed",
            ObservedResultKind::InsufficientQualifyingVisibility => {
                "Insufficient qualifying visibility"
            }
            ObservedResultKind::AnalysisIncomplete => "Analysis incomplete",
        }
    }

    /// The scope statement that must accompany every observed result.
    pub fn scope_statement(&self) -> &'static str {
        "Observation is limited to externally exported BGP route state at the selected public-BGP observer sessions; it does not measure traffic, circuit, or service state."
    }
}

/// The assessment of the observed result against the reviewed ticket
/// expectation.
///
/// These labels reference the reviewed expectation and never contain
/// route-transition counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExpectationAssessmentKind {
    ConsistentWithReviewedExpectation,
    PartiallyConsistentWithReviewedExpectation,
    LessExternallyVisibleChangeThanReviewedExpectation,
    MoreExternallyVisibleChangeThanReviewedExpectation,
    NotAssessableFromSelectedPublicObservers,
    NoReviewedExpectationExists,
    ProvisionalAssessment,
}

impl ExpectationAssessmentKind {
    /// Parse a machine kind name (report.json "kind" field).
    pub fn from_label(kind: &str) -> Option<ExpectationAssessmentKind> {
        Some(match kind {
            "ConsistentWithReviewedExpectation" => {
                ExpectationAssessmentKind::ConsistentWithReviewedExpectation
            }
            "PartiallyConsistentWithReviewedExpectation" => {
                ExpectationAssessmentKind::PartiallyConsistentWithReviewedExpectation
            }
            "LessExternallyVisibleChangeThanReviewedExpectation" => {
                ExpectationAssessmentKind::LessExternallyVisibleChangeThanReviewedExpectation
            }
            "MoreExternallyVisibleChangeThanReviewedExpectation" => {
                ExpectationAssessmentKind::MoreExternallyVisibleChangeThanReviewedExpectation
            }
            "NotAssessableFromSelectedPublicObservers" => {
                ExpectationAssessmentKind::NotAssessableFromSelectedPublicObservers
            }
            "NoReviewedExpectationExists" => ExpectationAssessmentKind::NoReviewedExpectationExists,
            "ProvisionalAssessment" => ExpectationAssessmentKind::ProvisionalAssessment,
            _ => return None,
        })
    }

    pub fn human_label(&self) -> &'static str {
        match self {
            ExpectationAssessmentKind::ConsistentWithReviewedExpectation => {
                "Consistent with the reviewed expectation"
            }
            ExpectationAssessmentKind::PartiallyConsistentWithReviewedExpectation => {
                "Partially consistent with the reviewed expectation"
            }
            ExpectationAssessmentKind::LessExternallyVisibleChangeThanReviewedExpectation => {
                "Less externally visible change than the reviewed expectation"
            }
            ExpectationAssessmentKind::MoreExternallyVisibleChangeThanReviewedExpectation => {
                "More externally visible change than the reviewed expectation"
            }
            ExpectationAssessmentKind::NotAssessableFromSelectedPublicObservers => {
                "Not assessable from the selected public observers"
            }
            ExpectationAssessmentKind::NoReviewedExpectationExists => {
                "No reviewed expectation exists"
            }
            ExpectationAssessmentKind::ProvisionalAssessment => {
                "Provisional assessment (open event)"
            }
        }
    }
}

/// A complete assessment of an operational event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventAssessment {
    pub event_id: EventId,
    pub expectation: ImpactExpectation,
    pub verdict: Verdict,
    pub evidence: Vec<Evidence>,
    pub waves: Vec<ImpactWave>,
    pub generated_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::expectation::ExpectationKind;
    use chrono::TimeZone;

    fn sample_time() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2025, 6, 15, 5, 25, 0).unwrap()
    }

    #[test]
    fn verdict_display_formats() {
        assert_eq!(
            format!("{}", Verdict::ExpectedRedundantImpact),
            "EXPECTED REDUNDANT IMPACT"
        );
        assert_eq!(
            format!("{}", Verdict::RedundancyFailureObserved),
            "REDUNDANCY FAILURE"
        );
        assert_eq!(format!("{}", Verdict::Indeterminate), "INDETERMINATE");
    }

    #[test]
    fn evidence_construction() {
        let ev = Evidence {
            description: "Route moved from NYIIX to alternate path".into(),
            source_records: vec!["rv2:AS6447 2025-06-15T05:25:18Z".into()],
        };
        assert_eq!(ev.source_records.len(), 1);
    }

    #[test]
    fn event_assessment_construction() {
        let assessment = EventAssessment {
            event_id: EventId::from("CHG0107955"),
            expectation: ImpactExpectation::redundant(
                Some("NEWY32AOA"),
                "Internet2 title convention",
            ),
            verdict: Verdict::ExpectedRedundantImpact,
            evidence: vec![],
            waves: vec![],
            generated_at: sample_time(),
        };
        assert_eq!(assessment.event_id.0, "CHG0107955");
        assert_eq!(assessment.expectation.kind, ExpectationKind::Redundant);
        assert_eq!(assessment.verdict, Verdict::ExpectedRedundantImpact);
    }

    // ── Semantic-layer separation (observed vs expectation) ────────

    #[test]
    fn observed_result_contains_no_expectation_language() {
        let forbidden = ["expected", "unexpected", "impact"];
        for kind in [
            ObservedResultKind::RouteStateChangesObserved,
            ObservedResultKind::NoRouteStateChangeObserved,
            ObservedResultKind::InsufficientQualifyingVisibility,
            ObservedResultKind::AnalysisIncomplete,
        ] {
            let label = kind.human_label().to_lowercase();
            for word in forbidden {
                assert!(
                    !label.contains(word),
                    "observed-result label {:?} contains expectation word {word:?}",
                    kind.human_label()
                );
            }
        }
    }

    #[test]
    fn expectation_assessment_contains_no_route_transition_count() {
        for kind in [
            ExpectationAssessmentKind::ConsistentWithReviewedExpectation,
            ExpectationAssessmentKind::PartiallyConsistentWithReviewedExpectation,
            ExpectationAssessmentKind::LessExternallyVisibleChangeThanReviewedExpectation,
            ExpectationAssessmentKind::MoreExternallyVisibleChangeThanReviewedExpectation,
            ExpectationAssessmentKind::NotAssessableFromSelectedPublicObservers,
            ExpectationAssessmentKind::NoReviewedExpectationExists,
            ExpectationAssessmentKind::ProvisionalAssessment,
        ] {
            let label = kind.human_label().to_lowercase();
            assert!(
                !label.chars().any(|c| c.is_ascii_digit()),
                "expectation-assessment label contains a route-transition count: {:?}",
                kind.human_label()
            );
            assert!(
                !label.contains("stream") && !label.contains("transition"),
                "expectation-assessment label contains route-state counts: {:?}",
                kind.human_label()
            );
        }
    }

    #[test]
    fn no_change_does_not_claim_no_service_impact() {
        // The no-change label and its scope statement must not claim
        // traffic or service state.
        let label = ObservedResultKind::NoRouteStateChangeObserved
            .human_label()
            .to_lowercase();
        for word in ["service", "traffic", "impact"] {
            assert!(!label.contains(word), "no-change label claims {word}");
        }
        let scope = ObservedResultKind::NoRouteStateChangeObserved
            .scope_statement()
            .to_lowercase();
        assert!(
            scope.contains("does not measure traffic") || scope.contains("not traffic"),
            "no-change scope must state the traffic limit"
        );
    }

    #[test]
    fn continued_transit_does_not_claim_operator_action_failed() {
        let label = ExpectationAssessmentKind::LessExternallyVisibleChangeThanReviewedExpectation
            .human_label()
            .to_lowercase();
        for word in ["failed", "wrong", "success", "operator"] {
            assert!(
                !label.contains(word),
                "expectation label claims operator outcome: {word}"
            );
        }
    }

    #[test]
    fn public_bgp_scope_is_stated() {
        let scope = ObservedResultKind::NoRouteStateChangeObserved.scope_statement();
        assert!(scope.contains("public-BGP"), "{scope}");
        assert!(scope.contains("selected"), "{scope}");
        assert!(scope.contains("does not measure"), "{scope}");
    }

    #[test]
    fn result_and_assessment_serialize_separately() {
        // The two presentation layers serialize as distinct machine kinds.
        let v = Verdict::UnexpectedContinuedInternet2Path;
        let observed = serde_json::to_string(&v.observed_result_kind()).unwrap();
        let expected = serde_json::to_string(&v.expectation_assessment_kind()).unwrap();
        assert_ne!(observed, expected);
        assert_eq!(observed, "\"NoRouteStateChangeObserved\"");
        assert_eq!(
            expected,
            "\"LessExternallyVisibleChangeThanReviewedExpectation\""
        );
    }

    #[test]
    fn human_labels_are_observer_scoped() {
        for kind in [
            ObservedResultKind::RouteStateChangesObserved,
            ObservedResultKind::NoRouteStateChangeObserved,
            ObservedResultKind::InsufficientQualifyingVisibility,
            ObservedResultKind::AnalysisIncomplete,
        ] {
            assert!(
                kind.scope_statement().contains("observer"),
                "scope statement not observer-scoped for {kind:?}"
            );
        }
    }

    #[test]
    fn human_labels_do_not_claim_traffic() {
        for kind in [
            ObservedResultKind::RouteStateChangesObserved,
            ObservedResultKind::NoRouteStateChangeObserved,
            ObservedResultKind::InsufficientQualifyingVisibility,
            ObservedResultKind::AnalysisIncomplete,
        ] {
            assert!(
                !kind.scope_statement().to_lowercase().contains("traffic is"),
                "{kind:?} scope claims traffic"
            );
        }
    }

    #[test]
    fn expectation_labels_reference_reviewed_expectation() {
        for kind in [
            ExpectationAssessmentKind::ConsistentWithReviewedExpectation,
            ExpectationAssessmentKind::PartiallyConsistentWithReviewedExpectation,
            ExpectationAssessmentKind::LessExternallyVisibleChangeThanReviewedExpectation,
            ExpectationAssessmentKind::MoreExternallyVisibleChangeThanReviewedExpectation,
        ] {
            assert!(
                kind.human_label().contains("reviewed expectation"),
                "{kind:?} does not reference the reviewed expectation"
            );
        }
    }

    #[test]
    fn insufficient_visibility_is_not_no_change() {
        assert_ne!(
            ObservedResultKind::InsufficientQualifyingVisibility.human_label(),
            ObservedResultKind::NoRouteStateChangeObserved.human_label()
        );
    }

    #[test]
    fn analysis_incomplete_is_not_insufficient_visibility() {
        assert_ne!(
            ObservedResultKind::AnalysisIncomplete.human_label(),
            ObservedResultKind::InsufficientQualifyingVisibility.human_label()
        );
    }

    #[test]
    fn deprecated_labels_round_trip() {
        // Frozen legacy labels must resolve back to the machine verdict.
        assert_eq!(
            Verdict::from_stored("Unexpected continued reviewed-transit path"),
            Some(Verdict::UnexpectedContinuedInternet2Path)
        );
        assert_eq!(
            Verdict::from_stored("No route-state change observed"),
            Some(Verdict::NoObservableBgpImpact)
        );
        assert_eq!(
            Verdict::from_stored("UnexpectedContinuedInternet2Path"),
            Some(Verdict::UnexpectedContinuedInternet2Path)
        );
        assert_eq!(Verdict::from_stored("NoSuchLabel"), None);
    }

    #[test]
    fn mapping_is_total_and_separated() {
        // Every verdict maps to exactly one observed result and one
        // expectation assessment; the two never share a label.
        use ExpectationAssessmentKind as E;
        use ObservedResultKind as O;
        let cases = [
            (
                Verdict::ExpectedRedundantImpact,
                O::RouteStateChangesObserved,
                E::ConsistentWithReviewedExpectation,
            ),
            (
                Verdict::ExpectedLossOfReachability,
                O::RouteStateChangesObserved,
                E::ConsistentWithReviewedExpectation,
            ),
            (
                Verdict::ExpectedParticipantUnavailability,
                O::RouteStateChangesObserved,
                E::ConsistentWithReviewedExpectation,
            ),
            (
                Verdict::ExpectedAlternateRouting,
                O::RouteStateChangesObserved,
                E::ConsistentWithReviewedExpectation,
            ),
            (
                Verdict::PartialImpact,
                O::RouteStateChangesObserved,
                E::PartiallyConsistentWithReviewedExpectation,
            ),
            (
                Verdict::UnexpectedContinuedInternet2Path,
                O::NoRouteStateChangeObserved,
                E::LessExternallyVisibleChangeThanReviewedExpectation,
            ),
            (
                Verdict::PolicyChangeObserved,
                O::RouteStateChangesObserved,
                E::LessExternallyVisibleChangeThanReviewedExpectation,
            ),
            (
                Verdict::ProvisionalImpactObserved,
                O::RouteStateChangesObserved,
                E::ConsistentWithReviewedExpectation,
            ),
            (
                Verdict::ProvisionalNoImpactSoFar,
                O::NoRouteStateChangeObserved,
                E::ConsistentWithReviewedExpectation,
            ),
            (
                Verdict::UnexpectedWithdrawals,
                O::RouteStateChangesObserved,
                E::MoreExternallyVisibleChangeThanReviewedExpectation,
            ),
            (
                Verdict::RedundancyFailureObserved,
                O::RouteStateChangesObserved,
                E::MoreExternallyVisibleChangeThanReviewedExpectation,
            ),
            (
                Verdict::UnexpectedBlastRadius,
                O::RouteStateChangesObserved,
                E::MoreExternallyVisibleChangeThanReviewedExpectation,
            ),
            (
                Verdict::LessImpactThanExpected,
                O::NoRouteStateChangeObserved,
                E::LessExternallyVisibleChangeThanReviewedExpectation,
            ),
            (
                Verdict::NoObservableBgpImpact,
                O::NoRouteStateChangeObserved,
                E::ConsistentWithReviewedExpectation,
            ),
            (
                Verdict::InsufficientVisibility,
                O::InsufficientQualifyingVisibility,
                E::NotAssessableFromSelectedPublicObservers,
            ),
            (
                Verdict::Indeterminate,
                O::AnalysisIncomplete,
                E::NotAssessableFromSelectedPublicObservers,
            ),
        ];
        for (v, o, e) in cases {
            assert_eq!(v.observed_result_kind(), o, "{v:?}");
            assert_eq!(v.expectation_assessment_kind(), e, "{v:?}");
            assert_ne!(o.human_label(), e.human_label(), "{v:?}");
        }
    }

    #[test]
    fn assessment_serialization_roundtrip() {
        let assessment = EventAssessment {
            event_id: EventId::from("INC0302574"),
            expectation: ImpactExpectation::non_redundant("manual"),
            verdict: Verdict::ExpectedLossOfReachability,
            evidence: vec![Evidence {
                description: "All prefixes withdrawn during window".into(),
                source_records: vec!["rv2:AS6447 2025-06-15T05:25:00Z".into()],
            }],
            waves: vec![],
            generated_at: sample_time(),
        };
        let json = serde_json::to_string(&assessment).unwrap();
        let parsed: EventAssessment = serde_json::from_str(&json).unwrap();
        assert_eq!(assessment, parsed);
    }
}
