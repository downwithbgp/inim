//! Expectation types — what an operator declared should happen.

use serde::{Deserialize, Serialize};

/// Indicates whether a parenthesized site/attachment appears in the title,
/// suggesting redundant connectivity should preserve reachability.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RedundancyIndicator {
    pub has_parenthesized_site: bool,
    pub site_code: Option<String>,
}

/// The kind of impact expected for an event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExpectationKind {
    /// Redundancy should preserve reachability (parenthesized site).
    Redundant,
    /// Loss of reachability is expected (no parenthesized site).
    NonRedundant,
    /// Participant relationship may be unavailable (non-parenthesized participant title).
    ParticipantRelationshipUnavailable,
    /// Unable to determine expectation from available information.
    Unknown,
}

/// A parsed operational expectation with provenance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImpactExpectation {
    pub kind: ExpectationKind,
    pub description: String,
    /// Where the expectation came from (e.g. "Internet2 title convention").
    pub provenance: String,
}

impl ImpactExpectation {
    /// Create an expectation for a redundant event.
    pub fn redundant(site_code: Option<&str>, provenance: &str) -> Self {
        let site_desc = match site_code {
            Some(code) => format!("Parenthesized site code ({code}) indicates expected redundancy"),
            None => "Redundancy expected".to_string(),
        };
        ImpactExpectation {
            kind: ExpectationKind::Redundant,
            description: site_desc,
            provenance: provenance.to_string(),
        }
    }

    /// Create an expectation for a non-redundant event.
    pub fn non_redundant(provenance: &str) -> Self {
        ImpactExpectation {
            kind: ExpectationKind::NonRedundant,
            description: "No parenthesized site code — loss of reachability may be expected"
                .to_string(),
            provenance: provenance.to_string(),
        }
    }

    /// Create an expectation for a participant relationship unavailability event.
    pub fn participant_unavailable(provenance: &str) -> Self {
        ImpactExpectation {
            kind: ExpectationKind::ParticipantRelationshipUnavailable,
            description: "Non-parenthesized participant title — Internet2 participant relationship may be unavailable. Impact may include path departure from AS11537, alternate routing, or partial restorations."
                .to_string(),
            provenance: provenance.to_string(),
        }
    }

    /// Create an expectation for an event with unknown impact.
    pub fn unknown(provenance: &str) -> Self {
        ImpactExpectation {
            kind: ExpectationKind::Unknown,
            description: "Unable to determine expectation".to_string(),
            provenance: provenance.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redundant_expectation_with_site_code() {
        let exp = ImpactExpectation::redundant(Some("NEWY32AOA"), "Internet2 title convention");
        assert_eq!(exp.kind, ExpectationKind::Redundant);
        assert!(exp.description.contains("NEWY32AOA"));
        assert!(exp.description.contains("redundancy"));
    }

    #[test]
    fn redundant_expectation_without_site_code() {
        let exp = ImpactExpectation::redundant(None, "general");
        assert_eq!(exp.kind, ExpectationKind::Redundant);
    }

    #[test]
    fn non_redundant_expectation() {
        let exp = ImpactExpectation::non_redundant("Internet2 title convention");
        assert_eq!(exp.kind, ExpectationKind::NonRedundant);
        assert!(exp.description.contains("loss of reachability"));
    }

    #[test]
    fn unknown_expectation() {
        let exp = ImpactExpectation::unknown("no data");
        assert_eq!(exp.kind, ExpectationKind::Unknown);
    }

    #[test]
    fn expectation_serialization_roundtrip() {
        let exp = ImpactExpectation::redundant(Some("NEWA"), "Internet2 title convention");
        let json = serde_json::to_string(&exp).unwrap();
        let parsed: ImpactExpectation = serde_json::from_str(&json).unwrap();
        assert_eq!(exp, parsed);
    }

    #[test]
    fn redundancy_indicator_with_site() {
        let ind = RedundancyIndicator {
            has_parenthesized_site: true,
            site_code: Some("NEWY32AOA".into()),
        };
        assert!(ind.has_parenthesized_site);
        assert_eq!(ind.site_code, Some("NEWY32AOA".into()));
    }

    #[test]
    fn redundancy_indicator_without_parenthesized_site() {
        let ind = RedundancyIndicator {
            has_parenthesized_site: false,
            site_code: None,
        };
        assert!(!ind.has_parenthesized_site);
        assert_eq!(ind.site_code, None);
    }

    #[test]
    fn redundancy_indicator_serialization_roundtrip() {
        let ind = RedundancyIndicator {
            has_parenthesized_site: true,
            site_code: Some("NEWY32AOA".into()),
        };
        let json = serde_json::to_string(&ind).unwrap();
        let parsed: RedundancyIndicator = serde_json::from_str(&json).unwrap();
        assert_eq!(ind, parsed);
    }
}
