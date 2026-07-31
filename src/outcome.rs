//! Analysis outcomes — distinguishes completed assessments from
//! visibility failures and infrastructure errors.
//!
//! Infrastructure failures must produce `Incomplete`, never a routing
//! verdict like `INSUFFICIENT_VISIBILITY`.

use serde::{Deserialize, Serialize};

use crate::domain::assessment::EventAssessment;

/// The outcome of an analysis run.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status")]
pub enum AnalysisOutcome {
    /// Analysis completed normally with an evidence-backed assessment.
    #[serde(rename = "completed")]
    Completed { assessment: EventAssessment },
    /// Available observers could not evaluate the event.
    /// (e.g. empty RIB preflight, no relevant baseline visibility).
    #[serde(rename = "insufficient_visibility")]
    InsufficientVisibility { reason: String },
    /// Infrastructure failure prevented analysis.
    /// (e.g. broker unreachable, download failed, checksum mismatch).
    /// Never rendered as a statement about routing visibility.
    #[serde(rename = "incomplete")]
    Incomplete { failure: String },
}

impl AnalysisOutcome {
    /// A completed assessment.
    pub fn completed(assessment: EventAssessment) -> Self {
        AnalysisOutcome::Completed { assessment }
    }

    /// An inability to evaluate due to observer coverage.
    pub fn insufficient_visibility(reason: impl Into<String>) -> Self {
        AnalysisOutcome::InsufficientVisibility {
            reason: reason.into(),
        }
    }

    /// An infrastructure or data failure.
    pub fn incomplete(failure: impl Into<String>) -> Self {
        AnalysisOutcome::Incomplete {
            failure: failure.into(),
        }
    }

    /// Whether this outcome is a successful completion.
    pub fn is_completed(&self) -> bool {
        matches!(self, AnalysisOutcome::Completed { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::assessment::{Evidence, Verdict};
    use crate::domain::event::EventId;
    use crate::domain::expectation::{ExpectationKind, ImpactExpectation};
    use chrono::{TimeZone, Utc};

    fn sample_assessment() -> EventAssessment {
        EventAssessment {
            event_id: EventId::from("TEST"),
            expectation: ImpactExpectation {
                kind: ExpectationKind::Redundant,
                description: "test".into(),
                provenance: "test".into(),
            },
            verdict: Verdict::NoObservableBgpImpact,
            evidence: vec![Evidence {
                description: "none".into(),
                source_records: vec![],
            }],
            waves: vec![],
            generated_at: Utc.with_ymd_and_hms(2026, 7, 30, 9, 0, 0).unwrap(),
        }
    }

    #[test]
    fn completed_serializes() {
        let outcome = AnalysisOutcome::completed(sample_assessment());
        let json = serde_json::to_string(&outcome).unwrap();
        assert!(json.contains("\"status\":\"completed\""));
        assert!(json.contains("TEST"));
    }

    #[test]
    fn insufficient_visibility_serializes() {
        let outcome = AnalysisOutcome::insufficient_visibility("no relevant streams");
        let json = serde_json::to_string(&outcome).unwrap();
        assert!(json.contains("\"status\":\"insufficient_visibility\""));
        assert!(json.contains("no relevant streams"));
    }

    #[test]
    fn incomplete_serializes() {
        let outcome = AnalysisOutcome::incomplete("broker unreachable");
        let json = serde_json::to_string(&outcome).unwrap();
        assert!(json.contains("\"status\":\"incomplete\""));
        assert!(json.contains("broker unreachable"));
    }

    #[test]
    fn incomplete_is_not_routing_verdict() {
        // The string "visible" or "impact" must not appear in infrastructure errors
        let outcome = AnalysisOutcome::incomplete("download failed: connection refused");
        let json = serde_json::to_string(&outcome).unwrap();
        assert!(!json.contains("visible"));
        assert!(!json.contains("impact"));
    }
}
