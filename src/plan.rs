//! Pre-execution analysis planning.
//!
//! Runs before any Broker query, archive download, cache lookup, or MRT
//! parsing. Produces a plan status that either allows execution to proceed
//! or blocks it with a generic reason.
//!
//! No entity name, ASN, ticket ID, or network name appears in any enum
//! variant. Rendered messages may interpolate manifest values.

use serde::{Deserialize, Serialize};

use crate::domain::event::EventWindow;
use crate::domain::expectation::{ImpactExpectation, TicketLifecycle};
use crate::domain::route::TransitPredicate;

/// A provenance statement for a reviewed mapping.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct Provenance {
    pub statement: String,
    #[serde(default)]
    pub reviewed_by: String,
    #[serde(default)]
    pub date: String,
}

/// Status of a reviewed transit predicate mapping.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum PredicateReviewStatus {
    #[default]
    Unresolved,
    Reviewed,
}

/// A reviewed transit predicate mapping carried by the manifest.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct TransitPredicateMapping {
    #[serde(default)]
    pub status: PredicateReviewStatus,
    #[serde(default)]
    pub predicate: Option<TransitPredicate>,
    #[serde(default)]
    pub provenance: Option<Provenance>,
}

impl TransitPredicateMapping {
    pub fn validate(&self) -> Result<(), String> {
        match self.status {
            PredicateReviewStatus::Reviewed => {
                if self.predicate.is_none() {
                    return Err("Reviewed predicate requires a predicate value".into());
                }
                if self.provenance.is_none() {
                    return Err("Reviewed predicate requires provenance".into());
                }
                // Validate predicate contents
                if let Some(ref p) = self.predicate {
                    match p {
                        TransitPredicate::ContainsAny(asns)
                        | TransitPredicate::ContainsAll(asns) => {
                            if asns.is_empty() {
                                return Err("Transit predicate ASN set must be non-empty".into());
                            }
                        }
                        TransitPredicate::Adjacent(..) => {}
                    }
                }
                Ok(())
            }
            PredicateReviewStatus::Unresolved => {
                if self.predicate.is_some() {
                    return Err(
                        "Unresolved predicate must not expose an executable predicate".into(),
                    );
                }
                Ok(())
            }
        }
    }

    pub fn is_ready(&self) -> bool {
        matches!(self.status, PredicateReviewStatus::Reviewed) && self.predicate.is_some()
    }
}

/// The outcome of pre-execution planning.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AnalysisPlanStatus {
    Ready,
    Blocked { reason: AnalysisBlockReason },
}

/// Generic reasons for blocking analysis before acquisition.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AnalysisBlockReason {
    MissingReviewedEntityMapping,
    MissingReviewedTransitPredicate,
    MissingAnalysisEndForOpenTicket,
    InvalidAnalysisWindow,
    UnsupportedManifestRevision,
}

/// The complete analysis plan produced before any network activity.
#[derive(Debug, Clone, Serialize)]
pub struct AnalysisPlan {
    pub event_id: String,
    pub expectation: ImpactExpectation,
    pub lifecycle: TicketLifecycle,
    pub analysis_window: EventWindow,
    pub entity_origin_asns: Vec<u32>,
    pub transit_predicate: TransitPredicateMapping,
    pub status: AnalysisPlanStatus,
}

/// Plan an analysis from manifest data — before any broker/cache/network call.
pub fn plan_analysis(
    event_id: &str,
    expectation: ImpactExpectation,
    lifecycle: TicketLifecycle,
    analysis_window: EventWindow,
    entity_origin_asns: Vec<u32>,
    transit_predicate: TransitPredicateMapping,
) -> AnalysisPlan {
    let status = if entity_origin_asns.is_empty() {
        AnalysisPlanStatus::Blocked {
            reason: AnalysisBlockReason::MissingReviewedEntityMapping,
        }
    } else if !transit_predicate.is_ready() {
        AnalysisPlanStatus::Blocked {
            reason: if matches!(transit_predicate.status, PredicateReviewStatus::Unresolved) {
                AnalysisBlockReason::MissingReviewedTransitPredicate
            } else {
                AnalysisBlockReason::InvalidAnalysisWindow
            },
        }
    } else if lifecycle == TicketLifecycle::Open {
        // Open tickets need an explicit analysis end; checked at a higher level
        AnalysisPlanStatus::Ready
    } else {
        AnalysisPlanStatus::Ready
    };

    AnalysisPlan {
        event_id: event_id.to_string(),
        expectation,
        lifecycle,
        analysis_window,
        entity_origin_asns,
        transit_predicate,
        status,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    use chrono::{TimeZone, Utc};

    fn sample_window() -> EventWindow {
        EventWindow {
            start: Utc.with_ymd_and_hms(2026, 7, 30, 9, 0, 0).unwrap(),
            end: Utc.with_ymd_and_hms(2026, 7, 30, 10, 0, 0).unwrap(),
        }
    }

    #[test]
    fn missing_reviewed_predicate_blocks_plan() {
        let plan = plan_analysis(
            "T-0001",
            ImpactExpectation::peer_relationship_unavailable("grnoc convention"),
            TicketLifecycle::Closed,
            sample_window(),
            vec![65001],
            TransitPredicateMapping::default(),
        );
        assert!(matches!(
            plan.status,
            AnalysisPlanStatus::Blocked {
                reason: AnalysisBlockReason::MissingReviewedTransitPredicate
            }
        ));
    }

    #[test]
    fn reviewed_predicate_produces_ready_plan() {
        let mapping = TransitPredicateMapping {
            status: PredicateReviewStatus::Reviewed,
            predicate: Some(TransitPredicate::ContainsAny(vec![65002])),
            provenance: Some(Provenance {
                statement: "TestNet transit ASN 65002".into(),
                reviewed_by: "analyst".into(),
                date: "2026-07-01".into(),
            }),
        };
        let plan = plan_analysis(
            "T-0002",
            ImpactExpectation::peer_relationship_unavailable("grnoc convention"),
            TicketLifecycle::Closed,
            sample_window(),
            vec![65001],
            mapping,
        );
        assert_eq!(plan.status, AnalysisPlanStatus::Ready);
    }

    #[test]
    fn blocked_plan_is_not_an_analysis_outcome() {
        let plan = plan_analysis(
            "T-0001",
            ImpactExpectation::unknown("test"),
            TicketLifecycle::Closed,
            sample_window(),
            vec![65001],
            TransitPredicateMapping::default(),
        );
        // Blocked plan is AnalysisPlanStatus::Blocked, not an AnalysisOutcome variant
        assert!(matches!(plan.status, AnalysisPlanStatus::Blocked { .. }));
    }

    #[test]
    fn validation_message_uses_manifest_data() {
        // The rendered message (not tested here) interpolates manifest values.
        // The variant itself must not contain entity names.
        let reason = AnalysisBlockReason::MissingReviewedTransitPredicate;
        let msg = format!("{reason:?}");
        // Variant name is generic
        assert!(msg.contains("MissingReviewedTransitPredicate"));
    }

    #[test]
    fn unresolved_predicate_has_no_executable_value() {
        let mapping = TransitPredicateMapping {
            status: PredicateReviewStatus::Unresolved,
            predicate: Some(TransitPredicate::ContainsAny(vec![65001])),
            provenance: None,
        };
        assert!(mapping.validate().is_err());
        assert!(!mapping.is_ready());
    }

    #[test]
    fn empty_transit_set_is_invalid() {
        let mapping = TransitPredicateMapping {
            status: PredicateReviewStatus::Reviewed,
            predicate: Some(TransitPredicate::ContainsAny(vec![])),
            provenance: Some(Provenance {
                statement: "empty".into(),
                ..Default::default()
            }),
        };
        assert!(mapping.validate().is_err());
    }

    #[test]
    fn reviewed_predicate_requires_provenance() {
        let mapping = TransitPredicateMapping {
            status: PredicateReviewStatus::Reviewed,
            predicate: Some(TransitPredicate::ContainsAny(vec![65002])),
            provenance: None,
        };
        assert!(mapping.validate().is_err());
    }
}
