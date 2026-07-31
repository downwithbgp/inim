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

/// A piece of evidence linking a conclusion to source records.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Evidence {
    pub description: String,
    /// References to source records (e.g. "rv2:AS6447 2025-06-15T05:25:18Z").
    pub source_records: Vec<String>,
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
