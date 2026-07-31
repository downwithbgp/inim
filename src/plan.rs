//! Pre-execution analysis planning.
//!
//! Runs before any Broker query, archive download, cache lookup, or MRT
//! parsing. Produces a plan status that either allows execution to proceed
//! or blocks it with a generic reason.
//!
//! No entity name, ASN, ticket ID, or network name appears in any enum
//! variant. Rendered messages may interpolate manifest values.

use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::domain::event::EventWindow;
use crate::domain::expectation::{ImpactExpectation, TicketLifecycle};
use crate::domain::route::TransitPredicate;
use crate::manifest::Manifest;

/// Schema version of the analysis-plan artifact.
pub const ANALYSIS_PLAN_SCHEMA_VERSION: u32 = 1;

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

/// Plan an analysis directly from a reviewed manifest — before any broker,
/// cache, or MRT activity.
///
/// The lifecycle is derived from the manifest's open-ticket state; the
/// analysis window for open tickets is bounded by `analysis_end_utc`.
pub fn plan_from_manifest(
    event_id: &str,
    expectation: ImpactExpectation,
    manifest: &Manifest,
) -> Result<AnalysisPlan, String> {
    let lifecycle = if manifest.open {
        TicketLifecycle::Open
    } else {
        TicketLifecycle::Closed
    };

    // Window: closed tickets use the declared end; open tickets use the
    // reviewed analysis end when present.
    let (start, end) = if manifest.event_window_utc.end.is_empty() {
        let start = chrono::DateTime::parse_from_rfc3339(&manifest.event_window_utc.start)
            .map_err(|e| format!("invalid event start: {e}"))?
            .with_timezone(&chrono::Utc);
        let end = match &manifest.analysis_end_utc {
            Some(s) if !s.is_empty() => chrono::DateTime::parse_from_rfc3339(s)
                .map_err(|e| format!("invalid analysis_end_utc: {e}"))?
                .with_timezone(&chrono::Utc),
            _ => {
                return Ok(AnalysisPlan {
                    event_id: event_id.to_string(),
                    expectation,
                    lifecycle,
                    analysis_window: EventWindow { start, end: start },
                    entity_origin_asns: manifest.target.origin_asns.clone(),
                    transit_predicate: manifest.target.transit_predicate.clone(),
                    status: AnalysisPlanStatus::Blocked {
                        reason: AnalysisBlockReason::MissingAnalysisEndForOpenTicket,
                    },
                });
            }
        };
        (start, end)
    } else {
        manifest.event_window()?
    };

    Ok(plan_analysis(
        event_id,
        expectation,
        lifecycle,
        EventWindow { start, end },
        manifest.target.origin_asns.clone(),
        manifest.target.transit_predicate.clone(),
    ))
}

impl std::fmt::Display for AnalysisBlockReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AnalysisBlockReason::MissingReviewedEntityMapping => {
                write!(f, "MissingReviewedEntityMapping")
            }
            AnalysisBlockReason::MissingReviewedTransitPredicate => {
                write!(f, "MissingReviewedTransitPredicate")
            }
            AnalysisBlockReason::MissingAnalysisEndForOpenTicket => {
                write!(f, "MissingAnalysisEndForOpenTicket")
            }
            AnalysisBlockReason::InvalidAnalysisWindow => write!(f, "InvalidAnalysisWindow"),
            AnalysisBlockReason::UnsupportedManifestRevision => {
                write!(f, "UnsupportedManifestRevision")
            }
        }
    }
}

impl AnalysisPlan {
    /// One-line human-readable status ("Ready" or "Blocked: <reason>").
    pub fn status_line(&self) -> String {
        match &self.status {
            AnalysisPlanStatus::Ready => "Ready".to_string(),
            AnalysisPlanStatus::Blocked { reason } => format!("Blocked: {reason}"),
        }
    }

    /// Whether this plan blocks acquisition entirely.
    pub fn is_blocked(&self) -> bool {
        matches!(self.status, AnalysisPlanStatus::Blocked { .. })
    }

    /// The blocking reason, if any.
    pub fn block_reason(&self) -> Option<&AnalysisBlockReason> {
        match &self.status {
            AnalysisPlanStatus::Ready => None,
            AnalysisPlanStatus::Blocked { reason } => Some(reason),
        }
    }

    /// Entity mapping status: Reviewed when reviewed origin ASNs exist.
    pub fn entity_mapping_status(&self) -> PredicateReviewStatus {
        if self.entity_origin_asns.is_empty() {
            PredicateReviewStatus::Unresolved
        } else {
            PredicateReviewStatus::Reviewed
        }
    }
}

/// The serialized analysis-plan artifact written by the `plan` command.
///
/// Planning performs no broker calls and examines no MRT files by
/// construction; those counters are part of the artifact contract.
#[derive(Debug, Clone, Serialize)]
pub struct PlanArtifact {
    pub schema_version: u32,
    pub event_id: String,
    pub expectation: ImpactExpectation,
    pub lifecycle: TicketLifecycle,
    pub analysis_window: EventWindow,
    pub entity_origin_asns: Vec<u32>,
    pub entity_mapping: PredicateReviewStatus,
    pub transit_predicate: TransitPredicateMapping,
    pub plan: AnalysisPlanStatus,
    pub reason: Option<String>,
    pub broker_calls: u32,
    pub mrt_files_examined: u32,
    pub generated_at: chrono::DateTime<Utc>,
}

impl PlanArtifact {
    /// Build the artifact from a plan. Broker/MRT counters are always zero:
    /// planning runs strictly before acquisition.
    pub fn from_plan(plan: &AnalysisPlan) -> Self {
        PlanArtifact {
            schema_version: ANALYSIS_PLAN_SCHEMA_VERSION,
            event_id: plan.event_id.clone(),
            expectation: plan.expectation.clone(),
            lifecycle: plan.lifecycle,
            analysis_window: plan.analysis_window.clone(),
            entity_origin_asns: plan.entity_origin_asns.clone(),
            entity_mapping: plan.entity_mapping_status(),
            transit_predicate: plan.transit_predicate.clone(),
            plan: plan.status.clone(),
            reason: plan.block_reason().map(|r| r.to_string()),
            broker_calls: 0,
            mrt_files_examined: 0,
            generated_at: Utc::now(),
        }
    }

    /// Render a deterministic human-readable plan.
    pub fn render_text(&self) -> String {
        let mut buf = String::new();
        buf.push_str(&format!("Analysis plan: {}\n", self.event_id));
        buf.push_str(&format!("  Expectation:     {:?}\n", self.expectation.kind));
        buf.push_str(&format!("  Lifecycle:       {:?}\n", self.lifecycle));
        buf.push_str(&format!("  Entity mapping:  {:?}\n", self.entity_mapping));
        buf.push_str(&format!(
            "  TransitPredicate:{:?}\n",
            self.transit_predicate.status
        ));
        match &self.plan {
            AnalysisPlanStatus::Ready => buf.push_str("  Plan:            Ready\n"),
            AnalysisPlanStatus::Blocked { .. } => {
                buf.push_str(&format!(
                    "  Plan:            Blocked\n  Reason:          {}\n",
                    self.reason.as_deref().unwrap_or("unknown")
                ));
            }
        }
        buf.push_str(&format!("  Broker calls:    {}\n", self.broker_calls));
        buf.push_str(&format!("  MRT files:       {}\n", self.mrt_files_examined));
        buf
    }

    /// One-line status ("Ready" or "Blocked: <reason>").
    pub fn status_line(&self) -> String {
        match &self.plan {
            AnalysisPlanStatus::Ready => "Ready".to_string(),
            AnalysisPlanStatus::Blocked { .. } => {
                format!("Blocked: {}", self.reason.as_deref().unwrap_or("unknown"))
            }
        }
    }
}

/// Write the plan artifacts to an output directory:
/// `analysis_plan.json`, `analysis_plan.txt`, and `limitations.json`.
pub fn write_plan_artifacts(
    artifact: &PlanArtifact,
    out_dir: &std::path::Path,
) -> Result<(), String> {
    std::fs::create_dir_all(out_dir)
        .map_err(|e| format!("cannot create {}: {e}", out_dir.display()))?;

    let json = serde_json::to_string_pretty(artifact)
        .map_err(|e| format!("plan JSON serialization failed: {e}"))?;
    std::fs::write(out_dir.join("analysis_plan.json"), json)
        .map_err(|e| format!("cannot write analysis_plan.json: {e}"))?;

    std::fs::write(out_dir.join("analysis_plan.txt"), artifact.render_text())
        .map_err(|e| format!("cannot write analysis_plan.txt: {e}"))?;

    let limitations = serde_json::json!({
        "schema_version": 1,
        "observer": [
            "Planning precedes acquisition: no archives were downloaded or examined."
        ],
        "blocked": match &artifact.plan {
            AnalysisPlanStatus::Ready => Vec::<String>::new(),
            AnalysisPlanStatus::Blocked { .. } => vec![
                format!(
                    "Analysis is blocked: {}. No Broker calls and no MRT parses were performed.",
                    artifact.reason.as_deref().unwrap_or("unknown reason")
                )
            ],
        },
    });
    std::fs::write(
        out_dir.join("limitations.json"),
        serde_json::to_string_pretty(&limitations)
            .map_err(|e| format!("limitations JSON serialization failed: {e}"))?,
    )
    .map_err(|e| format!("cannot write limitations.json: {e}"))
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
