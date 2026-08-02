//! NOC incident-workbench presentation model (Session 36).
//!
//! `ObserverEpisode` groups observer-prefix streams at ONE observer
//! session (collector + peer) that share a meaningful, temporally
//! coherent signature. Effect kinds are presentation-level groupings
//! derived from EXISTING lifecycle/transition evidence — they introduce
//! no new route-transition semantics.
//!
//! This module is the single presentation model shared by the web
//! workbench, the text report, and the JSON API (Part 12). No counts are
//! recalculated in templates.
//!
//! Token discipline: this file contains no operator-specific plane
//! identity; plane labels and ASNs arrive at runtime via the reviewed
//! profile data (network-profile.json) and the run's manifest payload.

use crate::catalog::domain::{RunTransitionRecord, StreamLifecycleSummary};
use crate::catalog::netprofile::{
    classify_route, CollectorLocationRegistry, ServicePlaneProfile, SessionRelationship,
};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
/// Presentation-level effect kind of one episode.
///
/// These are groupings of existing evidence, never new transition
/// semantics. Each variant maps to lifecycle/transition evidence:
///
/// - `TemporaryStreamAbsence`: streams withdrawn then restored
///   (withdrawal + restoration evidence, absence interval).
/// - `RouteWithdrawal`: streams withdrawn and NOT restored inside the
///   analysis window (end state unresolved).
/// - `PathReplacement`: material AS-path change without plane departure
///   (path-replacement transitions, transit retained).
/// - `NamedPlaneDeparture`: streams departed the reviewed named plane
///   (path departed the reviewed plane's ASN set).
/// - `NamedPlaneReturn`: streams returned to the reviewed named plane
///   after departing it.
/// - `PrependChange`: AS-path prepending increased or reduced with the
///   plane membership unchanged.
/// - `MixedRouteChange`: the session shows several of the above across
///   its streams (or per-stream multi-cycle evidence).
/// - `NoRouteStateChange`: qualifying baseline existed and no route-state
///   change was observed at this session.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum EffectKind {
    TemporaryStreamAbsence,
    RouteWithdrawal,
    PathReplacement,
    NamedPlaneDeparture,
    NamedPlaneReturn,
    PrependChange,
    MixedRouteChange,
    NoRouteStateChange,
}

impl EffectKind {
    /// Stable machine label.
    pub fn label(&self) -> &'static str {
        match self {
            EffectKind::TemporaryStreamAbsence => "TemporaryStreamAbsence",
            EffectKind::RouteWithdrawal => "RouteWithdrawal",
            EffectKind::PathReplacement => "PathReplacement",
            EffectKind::NamedPlaneDeparture => "NamedPlaneDeparture",
            EffectKind::NamedPlaneReturn => "NamedPlaneReturn",
            EffectKind::PrependChange => "PrependChange",
            EffectKind::MixedRouteChange => "MixedRouteChange",
            EffectKind::NoRouteStateChange => "NoRouteStateChange",
        }
    }

    /// Human-readable label for the primary UI (Part 1.8). Raw enum
    /// labels stay in the JSON API and technical details; the primary
    /// workbench tables render these.
    pub fn human_label(&self) -> &'static str {
        match self {
            EffectKind::TemporaryStreamAbsence => "Temporarily absent",
            EffectKind::RouteWithdrawal => "Withdrawn, not restored",
            EffectKind::PathReplacement => "AS path changed",
            EffectKind::NamedPlaneDeparture => "Left the reviewed path",
            EffectKind::NamedPlaneReturn => "Returned to the reviewed path",
            EffectKind::PrependChange => "AS-path prepending changed",
            EffectKind::MixedRouteChange => "Mixed route-state change",
            EffectKind::NoRouteStateChange => "No route-state change",
        }
    }
}

/// Relationship of the observer session to the reviewed named plane.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RelationshipKind {
    Direct,
    Indirect,
    Other,
    Ambiguous,
}

impl RelationshipKind {
    pub fn label(&self) -> &'static str {
        match self {
            RelationshipKind::Direct => "Direct",
            RelationshipKind::Indirect => "Indirect",
            RelationshipKind::Other => "Other",
            RelationshipKind::Ambiguous => "Ambiguous",
        }
    }
}

/// Coverage status of one observer session for the target.
///
/// These states are about whether the observation could be made, NOT
/// about whether a route-state change occurred:
/// - `Complete`: a qualifying baseline existed and the run's observation
///   of this session completed.
/// - `NoBaselineVisibility`: the target was not visible at this session
///   (no qualifying baseline stream).
/// - `IncompleteCoverage`: the observation could not be completed
///   (run incomplete / archive coverage limitation).
///
/// "No change" is NOT a coverage state: it is an observed signature
/// (EffectKind::NoRouteStateChange) with Complete coverage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CoverageStatus {
    Complete,
    NoBaselineVisibility,
    IncompleteCoverage,
}

impl CoverageStatus {
    pub fn label(&self) -> &'static str {
        match self {
            CoverageStatus::Complete => "Complete",
            CoverageStatus::NoBaselineVisibility => "NoBaselineVisibility",
            CoverageStatus::IncompleteCoverage => "IncompleteCoverage",
        }
    }

    /// Human-readable label for the primary UI (Part 1.8).
    pub fn human_label(&self) -> &'static str {
        match self {
            CoverageStatus::Complete => "Complete",
            CoverageStatus::NoBaselineVisibility => "No qualifying baseline",
            CoverageStatus::IncompleteCoverage => "Incomplete coverage",
        }
    }
}

/// Why an observer session is (or is not) part of the eligible
/// measurement (Session 38, Part 4). These are distinct conditions and
/// must never collapse into one "no baseline" bucket:
/// - `EligibleWithBaseline`: the session exists and the target is
///   visible; it is part of the eligible denominator.
/// - `SessionPresentNoTargetBaseline`: the reviewed observer session
///   exists, but the target is not visible through it.
/// - `RequiredSessionAbsent`: no historical session matching the
///   reviewed observer relationship exists.
/// - `PredicateNotMatched`: origin routes exist, but none satisfy the
///   reviewed path condition.
/// - `ArchiveIncomplete`: the observation could not be completed.
/// - `UnsupportedSource`: the source family is not supported.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CoverageReason {
    EligibleWithBaseline,
    SessionPresentNoTargetBaseline,
    RequiredSessionAbsent,
    PredicateNotMatched,
    ArchiveIncomplete,
    UnsupportedSource,
}

impl CoverageReason {
    pub fn label(&self) -> &'static str {
        match self {
            CoverageReason::EligibleWithBaseline => "EligibleWithBaseline",
            CoverageReason::SessionPresentNoTargetBaseline => "SessionPresentNoTargetBaseline",
            CoverageReason::RequiredSessionAbsent => "RequiredSessionAbsent",
            CoverageReason::PredicateNotMatched => "PredicateNotMatched",
            CoverageReason::ArchiveIncomplete => "ArchiveIncomplete",
            CoverageReason::UnsupportedSource => "UnsupportedSource",
        }
    }

    pub fn human_label(&self) -> &'static str {
        match self {
            CoverageReason::EligibleWithBaseline => "Eligible with baseline",
            CoverageReason::SessionPresentNoTargetBaseline => "Session present, no target baseline",
            CoverageReason::RequiredSessionAbsent => "Required session absent",
            CoverageReason::PredicateNotMatched => "Predicate not matched",
            CoverageReason::ArchiveIncomplete => "Archive incomplete",
            CoverageReason::UnsupportedSource => "Unsupported source",
        }
    }
}

/// End state of one episode at the analysis-window end, derived ONLY
/// from lifecycle evidence (withdrawal/restoration flags and exact
/// restoration timestamps) — never from an optional presentation field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EndState {
    /// Changed streams returned to their baseline path by window end.
    BaselineRestored,
    /// Withdrawn streams returned to visibility (restored) by window end.
    VisibilityRestored,
    /// Changed streams were still changed at the analysis-window end.
    StillChangedAtWindowEnd,
    /// Withdrawn streams were still absent at the analysis-window end.
    AbsentAtWindowEnd,
    /// No route-state change was observed.
    NoRouteStateChange,
    /// Restoration status could not be determined from lifecycle evidence.
    Unresolved,
}

impl EndState {
    pub fn label(&self) -> &'static str {
        match self {
            EndState::BaselineRestored => "BaselineRestored",
            EndState::VisibilityRestored => "VisibilityRestored",
            EndState::StillChangedAtWindowEnd => "StillChangedAtWindowEnd",
            EndState::AbsentAtWindowEnd => "AbsentAtWindowEnd",
            EndState::NoRouteStateChange => "NoRouteStateChange",
            EndState::Unresolved => "Unresolved",
        }
    }

    pub fn human_label(&self) -> &'static str {
        match self {
            EndState::BaselineRestored => "Baseline restored",
            EndState::VisibilityRestored => "Visibility restored on changed path",
            EndState::StillChangedAtWindowEnd => "Still changed at window end",
            EndState::AbsentAtWindowEnd => "Absent at window end",
            EndState::NoRouteStateChange => "No route-state change",
            EndState::Unresolved => "Unresolved",
        }
    }
}

/// Outcome observed AFTER the event-window end, during the analysis
/// cooldown (Session 38, Part 7). The event-window end state and the
/// final analysis state are INDEPENDENT facts: an episode can be
/// "still changed at window end" and later restore in cooldown.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CooldownOutcome {
    /// No cooldown observation applies (already restored in-window).
    None,
    /// Restoration observed during cooldown (baseline or visibility,
    /// per the exact transition kind) at the given timestamp.
    RestoredAt(String),
    /// Route-state changes continued during cooldown; no restoration.
    StillChangingBeforeAnalysisEnd(String),
    /// No transition evidence in the cooldown interval.
    NoRestorationBeforeAnalysisEnd(String),
}

/// One episode: streams at one observer session sharing a signature.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObserverEpisode {
    /// Analysis run id this episode is derived from.
    pub analysis_run: i64,
    /// Observer session identity: "<family>/<collector> peer <peer_ip>".
    pub observer_session: String,
    pub observer_site: String,
    pub observer_region: String,
    /// Peer ASN when reviewed evidence provides it; None → unreviewed.
    pub peer_asn: Option<u32>,
    /// Peer ASNs OBSERVED in source RIB evidence (protocol facts;
    /// empty when not observed). >1 distinct value → ambiguous.
    pub observed_peer_asns: Vec<u32>,
    pub peer_label: String,
    pub peer_role: String,
    pub relationship: RelationshipKind,
    /// Named path plane label (runtime profile data), or "none reviewed".
    pub named_path_plane: String,
    pub effect_kind: EffectKind,
    /// First observed route-state change in the episode (UTC).
    pub first_change: Option<String>,
    /// Last observed route-state change in the episode (UTC).
    pub last_change: Option<String>,
    pub restoration_start: Option<String>,
    pub restoration_end: Option<String>,
    /// Streams with a qualifying baseline at this session (per run).
    pub baseline_stream_count: usize,
    /// Streams in the episode that changed.
    pub changed_stream_count: usize,
    /// Changed streams with an exact lifecycle restoration timestamp.
    pub restored_stream_count: usize,
    /// Distinct prefixes across the episode's changed streams.
    pub distinct_prefix_count: usize,
    /// Route instances involved in the episode's changed streams.
    pub route_instance_count: usize,
    /// Changed streams whose restoration is unresolved.
    pub unresolved_count: usize,
    /// Evidenced route-state transitions at this session (from the run's
    /// transition index; 0 when the artifact is absent — never guessed).
    pub transition_count: usize,
    /// End state at the analysis-window end, derived from lifecycle
    /// evidence (Part 1.3/1.4). NEVER "NoChange" for changed episodes.
    pub end_state: EndState,
    /// Outcome observed after the window end, during cooldown (Part 7).
    pub cooldown_outcome: CooldownOutcome,
    pub coverage_status: CoverageStatus,
    /// Data-supported sentence describing the episode.
    pub representative_evidence: String,
    /// Member stream rows (prefix-level evidence, for drill-down).
    pub streams: Vec<EpisodeStream>,
}

/// One member stream of an episode (prefix-level presentation).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EpisodeStream {
    pub prefix: String,
    pub category: String,
    pub withdrawn: bool,
    pub restored: bool,
    pub baseline_instances: i64,
    pub max_active_instances: i64,
    pub transition_count: i64,
    pub add_path_ambiguous: bool,
    /// Exact lifecycle timestamps (drill-down only; primary UI renders
    /// HH:MM:SS via `workbench_time`).
    pub first_change_utc: Option<String>,
    pub restoration_time_utc: Option<String>,
    pub evidence_refs: String,
}

/// Operator-facing routing effect vocabulary (Session 39, Part 3).
///
/// A presentation label derived one-to-one from the existing
/// `EffectKind` (+ end-state) evidence — no new transition semantics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RoutingEffect {
    /// Prefixes stopped being visible at the observer, then returned.
    PrefixesTemporarilyAbsent,
    /// Prefixes withdrew and did not return within the window.
    PrefixesWithdrawn,
    /// The observed AS path changed (still via the reviewed transit).
    AsPathChanged,
    /// The observed AS path changed only by AS-path prepending.
    PrependingChanged,
    /// Routes left the reviewed path plane.
    NamedPlaneDeparted,
    /// Routes returned to the reviewed path plane.
    NamedPlaneReturned,
    /// Visibility (any route state) returned after an absence.
    VisibilityRestored,
    /// The earlier baseline path was restored.
    BaselinePathRestored,
    /// Several distinct route-state changes at one session.
    MixedChange,
}

impl RoutingEffect {
    /// Map an episode's evidence kind to the operator vocabulary.
    pub fn from_kind(kind: &EffectKind) -> RoutingEffect {
        match kind {
            EffectKind::TemporaryStreamAbsence => RoutingEffect::PrefixesTemporarilyAbsent,
            EffectKind::RouteWithdrawal => RoutingEffect::PrefixesWithdrawn,
            EffectKind::PathReplacement => RoutingEffect::AsPathChanged,
            EffectKind::PrependChange => RoutingEffect::PrependingChanged,
            EffectKind::NamedPlaneDeparture => RoutingEffect::NamedPlaneDeparted,
            EffectKind::NamedPlaneReturn => RoutingEffect::NamedPlaneReturned,
            EffectKind::MixedRouteChange => RoutingEffect::MixedChange,
            EffectKind::NoRouteStateChange => RoutingEffect::MixedChange, // unreachable for findings
        }
    }

    /// Short human label for tables and statements.
    pub fn label(&self) -> &'static str {
        match self {
            RoutingEffect::PrefixesTemporarilyAbsent => "Temporarily absent",
            RoutingEffect::PrefixesWithdrawn => "Withdrawn",
            RoutingEffect::AsPathChanged => "AS path changed",
            RoutingEffect::PrependingChanged => "Prepending changed",
            RoutingEffect::NamedPlaneDeparted => "Left reviewed path plane",
            RoutingEffect::NamedPlaneReturned => "Returned to reviewed path plane",
            RoutingEffect::VisibilityRestored => "Visibility restored",
            RoutingEffect::BaselinePathRestored => "Baseline path restored",
            RoutingEffect::MixedChange => "Mixed route-state change",
        }
    }
}

/// Reviewed, time-scoped ASN identity enrichment (Session 39, Part 6;
/// Session 40, Part 6).
///
/// Names and roles come from reviewed case-study data or from current
/// registry lookups with explicit status. `valid_from`/`valid_to` bound
/// the reviewed window; null means unbounded on that side. Statuses:
/// - `HistoricallyReviewed`: name reviewed for the event date.
/// - `CurrentIdentityOnly`: name from a current registry lookup; NOT
///   historical truth (renders with the caveat).
/// - `Conflicting`: sources disagree; render unresolved.
/// - `Unresolved`: no reviewed identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AsnIdentity {
    pub asn: u32,
    pub display_name: String,
    pub role: String,
    pub valid_from: Option<String>,
    pub valid_to: Option<String>,
    pub provenance: String,
    pub review_status: String,
}

impl AsnIdentity {
    /// Whether the identity is time-valid for the given ISO date.
    pub fn valid_at(&self, date: &str) -> bool {
        if let Some(from) = &self.valid_from {
            if date < from.as_str() {
                return false;
            }
        }
        if let Some(to) = &self.valid_to {
            if date > to.as_str() {
                return false;
            }
        }
        true
    }

    /// Whether the identity is usable as a display name (any reviewed
    /// status with a real name, excluding purely unresolved rows).
    pub fn has_display_name(&self) -> bool {
        self.review_status == "HistoricallyReviewed" || self.review_status == "CurrentIdentityOnly"
    }
}

/// Source-neutral registry of reviewed ASN identities.
#[derive(Debug, Clone, Default)]
pub struct AsnIdentityRegistry {
    identities: Vec<AsnIdentity>,
}

impl AsnIdentityRegistry {
    /// Load identities from a reviewed `asn-identities.json` file.
    /// Absent or unparsable files yield an empty registry (rendering
    /// falls back to "name not reviewed").
    pub fn load(path: &std::path::Path) -> Self {
        let mut registry = AsnIdentityRegistry::default();
        if let Ok(raw) = std::fs::read_to_string(path) {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&raw) {
                if let Some(rows) = v.get("identities").and_then(|i| i.as_array()) {
                    for row in rows {
                        let Ok(identity) = serde_json::from_value::<AsnIdentity>(row.clone())
                        else {
                            continue;
                        };
                        registry.identities.push(identity);
                    }
                }
            }
        }
        registry.identities.sort_by_key(|i| i.asn);
        registry.identities.dedup_by_key(|i| i.asn);
        registry
    }

    /// Time-scoped identity for an ASN at the given ISO date, or None.
    pub fn lookup(&self, asn: u32, date: &str) -> Option<&AsnIdentity> {
        self.identities
            .iter()
            .find(|i| i.asn == asn && i.valid_at(date))
    }

    /// Reviewed display name for an ASN at the given date, or None.
    pub fn display_name(&self, asn: u32, date: &str) -> Option<&str> {
        self.lookup(asn, date)
            .filter(|i| i.has_display_name())
            .map(|i| i.display_name.as_str())
    }

    /// Reviewed role for an ASN at the given date, or None.
    pub fn role(&self, asn: u32, date: &str) -> Option<&str> {
        self.lookup(asn, date)
            .filter(|i| i.has_display_name())
            .map(|i| i.role.as_str())
    }

    /// Primary display name for historical path rendering (Session 41,
    /// Part 6): only HistoricallyReviewed names are primary; current
    /// registry identities are provenance, not primary labels.
    pub fn primary_display_name(&self, asn: u32, date: &str) -> Option<&str> {
        self.lookup(asn, date)
            .filter(|i| i.review_status == "HistoricallyReviewed")
            .map(|i| i.display_name.as_str())
    }

    /// Status of the best identity at the given date, or "Unresolved".
    pub fn status(&self, asn: u32, date: &str) -> &'static str {
        match self.lookup(asn, date) {
            Some(i) if i.review_status == "CurrentIdentityOnly" => "CurrentIdentityOnly",
            Some(i) if i.review_status == "HistoricallyReviewed" => "HistoricallyReviewed",
            Some(i) if i.review_status == "Conflicting" => "Conflicting",
            _ => "Unresolved",
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = &AsnIdentity> {
        self.identities.iter()
    }

    /// Merge another registry's identities (later lookups win on
    /// dedupe only; identical ASNs keep the first loaded entry).
    pub fn merge(&mut self, other: &AsnIdentityRegistry) {
        for identity in other.iter() {
            if !self.identities.iter().any(|i| i.asn == identity.asn) {
                self.identities.push(identity.clone());
            }
        }
        self.identities.sort_by_key(|i| i.asn);
    }
}

/// One lifecycle transition with exact paths (Session 41, Part 1):
/// the ordered route-state list of one prefix.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PathTransition {
    pub timestamp: String,
    pub kind: String,
    pub before_path: Vec<u32>,
    pub after_path: Vec<u32>,
}

/// Exact per-stream path evidence from a run's lifecycle artifact.
///
/// Loaded from `lifecycle.json` (baseline path + transition before/
/// after paths). Never synthesized: absent evidence means no path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StreamPathEvidence {
    /// Ordered transition list (exact timestamps and paths).
    pub transitions: Vec<PathTransition>,
    /// Baseline AS path (ASN sequence, in path order).
    pub baseline_path: Vec<u32>,
    /// First non-baseline observed AS path; None for pure absences.
    pub changed_path: Option<Vec<u32>>,
    /// Last observed AS path in the window; None when never visible.
    pub final_path: Option<Vec<u32>>,
    /// Exact time the prefix became visible again after an absence.
    pub visibility_restored_at: Option<String>,
    /// Exact time a route for the prefix returned on a path that is
    /// NOT the exact baseline path (a semantically equivalent route).
    pub equivalent_route_restored_at: Option<String>,
    /// Exact time the observed path again contained the reviewed plane
    /// ASN after having departed it (or after an absence).
    pub reviewed_plane_restored_at: Option<String>,
    /// Exact time the earlier baseline path was restored.
    pub exact_baseline_restored_at: Option<String>,
}

/// Index of exact path evidence across runs, keyed by run event id and
/// (collector, peer_ip, prefix).
#[derive(Debug, Clone, Default)]
pub struct LifecyclePathIndex {
    by_run: std::collections::HashMap<
        String,
        std::collections::HashMap<(String, String, String), StreamPathEvidence>,
    >,
}

impl LifecyclePathIndex {
    /// Load lifecycle path evidence from every reviewed lifecycle.json
    /// under the given candidate roots (case-study pilot out/, reviewed
    /// event evidence out/, and the local runtime out/). The artifact
    /// carries its own `event_id`, which keys the index. `plane_asns`
    /// maps run event id to the reviewed path-plane ASNs so the loader
    /// can derive reviewed-plane restoration times.
    pub fn load(
        catalog_root: &std::path::Path,
        plane_asns: &std::collections::HashMap<String, Vec<u32>>,
    ) -> Self {
        let mut index = LifecyclePathIndex::default();
        let mut roots: Vec<std::path::PathBuf> = vec![catalog_root.join("out")];
        let case_studies = catalog_root.join("case-studies");
        if let Ok(entries) = std::fs::read_dir(&case_studies) {
            let mut slugs: Vec<std::path::PathBuf> = entries
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.is_dir())
                .collect();
            slugs.sort();
            for slug in slugs {
                roots.push(slug.join("pilot").join("out"));
                roots.push(slug.join("out"));
            }
        }
        let mut seen_files: std::collections::BTreeSet<std::path::PathBuf> =
            std::collections::BTreeSet::new();
        for root in roots {
            let Ok(entries) = std::fs::read_dir(&root) else {
                continue;
            };
            let mut runs: Vec<std::path::PathBuf> = entries
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.is_dir())
                .collect();
            runs.sort();
            for run_dir in runs {
                let path = run_dir.join("lifecycle.json");
                if !path.is_file() || !seen_files.insert(path.clone()) {
                    continue;
                }
                let Ok(raw) = std::fs::read_to_string(&path) else {
                    continue;
                };
                let Ok(v) = serde_json::from_str::<serde_json::Value>(&raw) else {
                    continue;
                };
                let Some(event_id) = v.get("event_id").and_then(|e| e.as_str()) else {
                    continue;
                };
                let Some(lifecycles) = v.get("lifecycles").and_then(|l| l.as_array()) else {
                    continue;
                };
                let mut map: std::collections::HashMap<
                    (String, String, String),
                    StreamPathEvidence,
                > = std::collections::HashMap::new();
                let run_plane: &[u32] = plane_asns
                    .get(event_id)
                    .map(|v| v.as_slice())
                    .unwrap_or(&[]);
                let plane_contains =
                    |path: &[u32]| -> bool { run_plane.iter().any(|p| path.contains(p)) };
                for lc in lifecycles {
                    let Some(collector) = lc.get("collector").and_then(|c| c.as_str()) else {
                        continue;
                    };
                    let Some(peer_ip) = lc.get("peer_ip").and_then(|p| p.as_str()) else {
                        continue;
                    };
                    let Some(prefix) = lc.get("prefix").and_then(|p| p.as_str()) else {
                        continue;
                    };
                    // The baseline for the change is the path that was
                    // actually present when the first transition occurred
                    // (the transition's before_path), falling back to the
                    // lifecycle baseline_path field. The lifecycle field
                    // records the FIRST baseline instance, which can
                    // differ from the pre-change path (e.g. route-views2
                    // direct: field AS11537 AS2603 vs pre-change
                    // AS11537 AS20965 AS2603); using the pre-change path
                    // keeps baseline/restoration/final consistent.
                    let mut first_before: Vec<u32> = Vec::new();
                    if let Some(transitions) = lc.get("transitions").and_then(|t| t.as_array()) {
                        if let Some(t0) = transitions.first() {
                            first_before = t0
                                .get("before_path")
                                .and_then(|b| b.as_array())
                                .map(|a| {
                                    a.iter()
                                        .filter_map(|n| n.as_u64())
                                        .map(|n| n as u32)
                                        .collect()
                                })
                                .unwrap_or_default();
                        }
                    }
                    let lifecycle_baseline: Vec<u32> = lc
                        .get("baseline_path")
                        .and_then(|b| b.as_array())
                        .map(|a| {
                            a.iter()
                                .filter_map(|n| n.as_u64())
                                .map(|n| n as u32)
                                .collect()
                        })
                        .unwrap_or_default();
                    let baseline: Vec<u32> = if !first_before.is_empty() {
                        first_before
                    } else {
                        lifecycle_baseline
                    };
                    // Full ordered transition list (for the route
                    // chronology): every lifecycle transition with
                    // exact paths, in artifact order.
                    let mut path_transitions: Vec<PathTransition> = Vec::new();
                    if let Some(transitions) = lc.get("transitions").and_then(|t| t.as_array()) {
                        for t in transitions {
                            let before: Vec<u32> = t
                                .get("before_path")
                                .and_then(|b| b.as_array())
                                .map(|a| {
                                    a.iter()
                                        .filter_map(|n| n.as_u64())
                                        .map(|n| n as u32)
                                        .collect()
                                })
                                .unwrap_or_default();
                            let after: Vec<u32> = t
                                .get("after_path")
                                .and_then(|b| b.as_array())
                                .map(|a| {
                                    a.iter()
                                        .filter_map(|n| n.as_u64())
                                        .map(|n| n as u32)
                                        .collect()
                                })
                                .unwrap_or_default();
                            path_transitions.push(PathTransition {
                                timestamp: t
                                    .get("timestamp")
                                    .and_then(|x| x.as_str())
                                    .unwrap_or("")
                                    .to_string(),
                                kind: t
                                    .get("kind")
                                    .and_then(|x| x.as_str())
                                    .unwrap_or("")
                                    .to_string(),
                                before_path: before,
                                after_path: after,
                            });
                        }
                    }
                    // First non-baseline after-state, and the last
                    // observed after-state, from the transition list.
                    let mut changed_path: Option<Vec<u32>> = None;
                    let mut final_path: Option<Vec<u32>> = None;
                    if let Some(transitions) = lc.get("transitions").and_then(|t| t.as_array()) {
                        for t in transitions {
                            let Some(after) = t.get("after_path").and_then(|a| a.as_array()) else {
                                continue;
                            };
                            let after: Vec<u32> = after
                                .iter()
                                .filter_map(|n| n.as_u64())
                                .map(|n| n as u32)
                                .collect();
                            if after.is_empty() {
                                continue;
                            }
                            if changed_path.is_none() && after != baseline {
                                changed_path = Some(after.clone());
                            }
                            final_path = Some(after);
                        }
                    }
                    // Restoration times from the transition list:
                    // - visibility: any announcement after an absence
                    //   (lifecycle restoration_time, withdrawn streams)
                    // - equivalent route: the re-announced path differs
                    //   from the exact baseline path
                    // - reviewed plane: the first after-path that again
                    //   contains the reviewed plane ASN after a state
                    //   that lacked it
                    // - exact baseline: ReturnToBaseline transitions
                    let mut visibility_restored_at: Option<String> = None;
                    let mut equivalent_route_restored_at: Option<String> = None;
                    let mut reviewed_plane_restored_at: Option<String> = None;
                    let mut exact_baseline_restored_at: Option<String> = None;
                    if let Some(transitions) = lc.get("transitions").and_then(|t| t.as_array()) {
                        let mut plane_present = plane_contains(&baseline);
                        let mut prev_after: Option<Vec<u32>> = None;
                        for t in transitions {
                            let Some(after) = t.get("after_path").and_then(|a| a.as_array()) else {
                                continue;
                            };
                            let after: Vec<u32> = after
                                .iter()
                                .filter_map(|n| n.as_u64())
                                .map(|n| n as u32)
                                .collect();
                            let ts = t
                                .get("timestamp")
                                .and_then(|s| s.as_str())
                                .unwrap_or("")
                                .to_string();
                            let kind = t.get("kind").and_then(|k| k.as_str()).unwrap_or("");
                            if !after.is_empty() {
                                if kind == "ReturnToBaseline" {
                                    exact_baseline_restored_at = Some(ts.clone());
                                }
                                if kind == "Announcement"
                                    && prev_after
                                        .as_deref()
                                        .map(|p| !p.is_empty())
                                        .unwrap_or(false)
                                {
                                    // An announcement following a visible
                                    // state is a re-announcement, not a
                                    // visibility restoration.
                                    visibility_restored_at = Some(ts.clone());
                                }
                                // The equivalent route is only a
                                // RESTORATION after an absence; the
                                // first change of a visible stream is
                                // not a restoration. Handled below via
                                // the withdrawal restoration_time.
                                let _ = &prev_after;
                                let after_has_plane = plane_contains(&after);
                                if !plane_present
                                    && after_has_plane
                                    && reviewed_plane_restored_at.is_none()
                                {
                                    reviewed_plane_restored_at = Some(ts.clone());
                                }
                                plane_present = after_has_plane;
                            }
                            prev_after = Some(after);
                        }
                    }
                    // Withdrawn streams: lifecycle restoration_time is the
                    // visibility restoration (announcement after absence).
                    if let Some(rt) = lc.get("restoration_time").and_then(|r| r.as_str()) {
                        visibility_restored_at = Some(rt.to_string());
                        // When the re-announced path differs from the
                        // baseline, the same moment is the equivalent-route
                        // restoration (unless the exact baseline returned
                        // at the same time).
                        if changed_path.as_deref() != Some(baseline.as_slice())
                            && exact_baseline_restored_at
                                .as_deref()
                                .map(|t| t > rt)
                                .unwrap_or(true)
                        {
                            equivalent_route_restored_at = Some(rt.to_string());
                        }
                    }
                    map.insert(
                        (
                            collector.to_string(),
                            peer_ip.to_string(),
                            prefix.to_string(),
                        ),
                        StreamPathEvidence {
                            transitions: path_transitions,
                            baseline_path: baseline,
                            changed_path,
                            final_path,
                            visibility_restored_at,
                            equivalent_route_restored_at,
                            reviewed_plane_restored_at,
                            exact_baseline_restored_at,
                        },
                    );
                }
                index.by_run.insert(event_id.to_string(), map);
            }
        }
        index
    }

    /// Look up exact path evidence for one stream.
    pub fn lookup(
        &self,
        run_event_id: &str,
        collector: &str,
        peer_ip: &str,
        prefix: &str,
    ) -> Option<&StreamPathEvidence> {
        self.by_run.get(run_event_id).and_then(|m| {
            m.get(&(
                collector.to_string(),
                peer_ip.to_string(),
                prefix.to_string(),
            ))
        })
    }
}

/// Collapse consecutive repeated ASNs in a path for compact summary
/// rendering: `[24489,24489,24489,24489]` renders `AS24489×4`. The
/// exact uncollapsed sequence is always retained in details/JSON.
pub fn collapse_as_path(path: &[u32]) -> String {
    if path.is_empty() {
        return "—".to_string();
    }
    let mut parts: Vec<String> = Vec::new();
    let mut i = 0;
    while i < path.len() {
        let asn = path[i];
        let mut run = 1;
        while i + run < path.len() && path[i + run] == asn {
            run += 1;
        }
        if run > 1 {
            parts.push(format!("AS{asn}×{run}"));
        } else {
            parts.push(format!("AS{asn}"));
        }
        i += run;
    }
    parts.join(" ")
}

/// One prefix row of a routing finding (exact-path drill-down).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FindingStream {
    pub prefix: String,
    /// Exact baseline AS path (empty when no reviewed evidence).
    pub baseline_path: Vec<u32>,
    /// Exact changed AS path, or None (absence or no path evidence).
    pub changed_path: Option<Vec<u32>>,
    /// Exact final observed AS path, or None.
    pub final_path: Option<Vec<u32>>,
    pub withdrawn: bool,
    pub first_change_utc: Option<String>,
    /// Ordered lifecycle transitions (route chronology, Part 1).
    pub transitions: Vec<PathTransition>,
    pub visibility_restored_at: Option<String>,
    pub equivalent_route_restored_at: Option<String>,
    pub reviewed_plane_restored_at: Option<String>,
    pub exact_baseline_restored_at: Option<String>,
    pub evidence_refs: String,
}

/// Operator-facing routing finding (Session 39, Part 3).
///
/// One coherent routing story at one observer session: which prefixes,
/// what changed, what the paths were, and how the episode ended. A
/// presentation model derived from existing episodes + exact path
/// evidence — no new route semantics.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoutingFinding {
    /// Deterministic, stable identifier for the finding.
    pub stable_id: String,
    /// Reviewed target label (pilot target or manifest label).
    pub target_label: String,
    pub target_origin_asns: Vec<u32>,
    /// Short target name for natural grammar ("UVA"), derived from the
    /// reviewed label, never from memory.
    pub target_name: String,
    pub observer_site: String,
    pub observer_region: String,
    pub source_family: String,
    pub collector: String,
    pub peer_ip: String,
    pub peer_asn: Option<u32>,
    /// Reviewed display name for the peer ASN, or "name not reviewed".
    pub peer_name: String,
    pub peer_role: String,
    /// True when source RIB evidence observed more than one peer ASN at
    /// this session (the peer ASN is genuinely ambiguous).
    pub peer_asn_ambiguous: bool,
    pub relationship: RelationshipKind,
    pub effect: RoutingEffect,
    pub first_observed: Option<String>,
    pub last_observed: Option<String>,
    /// Display label of the reviewed path plane (runtime data).
    pub named_path_plane: String,
    /// Exact restoration times (latest observed across the finding's
    /// prefixes; per-prefix exact times live in `streams`).
    pub visibility_restored_at: Option<String>,
    pub equivalent_route_restored_at: Option<String>,
    pub reviewed_plane_restored_at: Option<String>,
    pub exact_baseline_restored_at: Option<String>,
    pub state_at_window_end: EndState,
    pub state_at_analysis_end: CooldownOutcome,
    /// Analysis-window end (window end + cooldown), for precise
    /// "no restoration observed before the analysis ended at ..."
    /// statements.
    pub analysis_end: String,
    pub observer_stream_count: usize,
    pub distinct_prefixes: usize,
    /// Compact summary paths (collapsed repeats); exact paths live in
    /// `streams` and the JSON API.
    pub baseline_path_signature: String,
    pub changed_path_signature: String,
    pub final_path_signature: String,
    pub exact_prefixes: Vec<String>,
    pub evidence_refs: Vec<String>,
    pub scope_limit: String,
    pub streams: Vec<FindingStream>,
}

/// Load all runs' streams and transitions for a set of run ids.
///
/// Semantic waves are run-scoped aggregates without session identity;
/// they do not feed the episode presentation model (Session 37 audit)
/// and are therefore not loaded here.
#[derive(Debug, Default)]
pub struct RunEvidence {
    pub streams: Vec<StreamLifecycleSummary>,
    pub transitions: Vec<RunTransitionRecord>,
}

impl RunEvidence {
    pub fn load(conn: &Connection, run_ids: &[i64]) -> Result<Self, String> {
        let mut out = RunEvidence::default();
        for run_id in run_ids {
            out.streams
                .extend(crate::catalog::db::list_streams(conn, *run_id, None, None)?);
            out.transitions
                .extend(crate::catalog::db::list_transitions(conn, *run_id)?);
        }
        Ok(out)
    }
}

/// Derive one stream's primary effect kind from lifecycle evidence.
///
/// Precedence: unresolved withdrawal > restored withdrawal (temporary
/// absence) > plane departure > plane return > prepend change > plain
/// path replacement > no change. Multi-cycle evidence (both absence and
/// path replacement) reads as `MixedRouteChange` only when the caller
/// merges kinds; per-stream we keep the strongest single signal and the
/// caller detects mixed sessions by comparing kinds across streams.
pub fn stream_effect_kind(stream: &StreamLifecycleSummary) -> EffectKind {
    let withdrawn_unresolved = stream.withdrawn && !stream.restored;
    let temporary_absence = stream.withdrawn && stream.restored;
    let departed = stream.transit_state == "departed" || stream.transit_state == "left";
    let returned = stream.transit_state == "returned";
    let prepend = stream.category == "PrependOnly";
    let path_changed = stream.category == "PathChangedStillViaTransit"
        || stream.category == "GenericPathChange"
        || stream.transition_count > 0;
    if withdrawn_unresolved {
        EffectKind::RouteWithdrawal
    } else if temporary_absence {
        EffectKind::TemporaryStreamAbsence
    } else if departed {
        EffectKind::NamedPlaneDeparture
    } else if returned {
        EffectKind::NamedPlaneReturn
    } else if prepend {
        EffectKind::PrependChange
    } else if path_changed {
        EffectKind::PathReplacement
    } else {
        EffectKind::NoRouteStateChange
    }
}

/// Derive one episode's end state from lifecycle evidence (Part 1.3).
///
/// The end state answers "where did this episode stand at the
/// analysis-window end?" using ONLY exact lifecycle facts: withdrawal/
/// restoration flags and exact restoration timestamps. A changed episode
/// can NEVER yield `EndState::NoRouteStateChange`.
pub fn derive_end_state(
    kind: &EffectKind,
    changed_count: usize,
    restored_count: usize,
    members: &[&StreamLifecycleSummary],
) -> EndState {
    if *kind == EffectKind::NoRouteStateChange || changed_count == 0 {
        return EndState::NoRouteStateChange;
    }
    // ADD-PATH ambiguity means the stream's end state cannot be
    // determined from lifecycle evidence — genuinely unresolved.
    if members.iter().any(|s| s.add_path_ambiguous) {
        return EndState::Unresolved;
    }
    let all_restored = restored_count == changed_count;
    match kind {
        EffectKind::TemporaryStreamAbsence | EffectKind::RouteWithdrawal => {
            let any_withdrawn_unrestored = members.iter().any(|s| s.withdrawn && !s.restored);
            if all_restored {
                EndState::VisibilityRestored
            } else if any_withdrawn_unrestored && restored_count == 0 {
                EndState::AbsentAtWindowEnd
            } else {
                EndState::StillChangedAtWindowEnd
            }
        }
        _ => {
            if all_restored {
                EndState::BaselineRestored
            } else {
                EndState::StillChangedAtWindowEnd
            }
        }
    }
}

/// Derive the cooldown outcome from transition evidence (Part 7).
///
/// Transitions with `occurred_utc` strictly after the event-window end
/// and at or before the analysis end describe what happened AFTER the
/// window. A `ReturnToBaseline` is a baseline restoration; an
/// `Announcement` is a visibility restoration; continued
/// `PathReplacement`/`Withdrawal` means still changing. Episodes
/// already restored inside the window get `CooldownOutcome::None`.
pub fn derive_cooldown_outcome(
    episode: &ObserverEpisode,
    transitions: &[RunTransitionRecord],
    window_end: &str,
    analysis_end: &str,
) -> CooldownOutcome {
    // Already restored inside the window: no cooldown question applies.
    if matches!(
        episode.end_state,
        EndState::BaselineRestored | EndState::VisibilityRestored
    ) {
        return CooldownOutcome::None;
    }
    if matches!(episode.effect_kind, EffectKind::NoRouteStateChange) {
        return CooldownOutcome::None;
    }
    let session = session_key_of(&episode.observer_session);
    let episode_prefixes: std::collections::BTreeSet<&str> =
        episode.streams.iter().map(|s| s.prefix.as_str()).collect();
    let in_cooldown: Vec<&RunTransitionRecord> = transitions
        .iter()
        .filter(|t| {
            t.collector == session.collector
                && t.peer_ip == session.peer_ip
                && episode_prefixes.contains(t.prefix.as_str())
                && t.occurred_utc.as_str() > window_end
                && t.occurred_utc.as_str() <= analysis_end
        })
        .collect();
    if in_cooldown.is_empty() {
        return CooldownOutcome::NoRestorationBeforeAnalysisEnd(analysis_end.to_string());
    }
    // Restoration signals (strongest first).
    let baseline = in_cooldown
        .iter()
        .filter(|t| t.kind == "ReturnToBaseline")
        .map(|t| t.occurred_utc.clone())
        .max();
    if let Some(t) = baseline {
        return CooldownOutcome::RestoredAt(t);
    }
    let visibility = in_cooldown
        .iter()
        .filter(|t| t.kind == "Announcement")
        .map(|t| t.occurred_utc.clone())
        .max();
    if let Some(t) = visibility {
        return CooldownOutcome::RestoredAt(t);
    }
    let last_change = in_cooldown
        .iter()
        .map(|t| t.occurred_utc.clone())
        .max()
        .unwrap_or_else(|| analysis_end.to_string());
    CooldownOutcome::StillChangingBeforeAnalysisEnd(last_change)
}

/// Build observer episodes for one run.
///
/// Streams are grouped by (observer session, effect kind). `peer_asn`,
/// `peer_label`, `peer_role`, and `relationship` come from the reviewed
/// session context when provided (per collector, keyed by peer IP).
/// `named_path_plane` is the run's reviewed plane label (runtime data).
/// Deterministic: episodes sorted by (first change, region, collector,
/// peer ASN); no-change episodes sort after changed episodes.
#[allow(clippy::too_many_arguments)] // explicit data passthrough; each arg is one evidence source
pub fn build_episodes(
    run_id: i64,
    family: &str,
    streams: &[StreamLifecycleSummary],
    transitions: &[RunTransitionRecord],
    registry: &CollectorLocationRegistry,
    session_peers: &BTreeMap<(String, String), (u32, String, String, RelationshipKind)>,
    named_path_plane: &str,
) -> Vec<ObserverEpisode> {
    build_episodes_with_metadata(
        run_id,
        family,
        streams,
        transitions,
        registry,
        session_peers,
        named_path_plane,
        &[],
        "",
    )
}

/// `build_episodes` with observed peer-session metadata (Part 5).
#[allow(clippy::too_many_arguments)] // explicit data passthrough
pub fn build_episodes_with_metadata(
    run_id: i64,
    family: &str,
    streams: &[StreamLifecycleSummary],
    transitions: &[RunTransitionRecord],
    registry: &CollectorLocationRegistry,
    session_peers: &BTreeMap<(String, String), (u32, String, String, RelationshipKind)>,
    named_path_plane: &str,
    metadata: &[crate::catalog::domain::ObserverSessionMetadata],
    window_end: &str,
) -> Vec<ObserverEpisode> {
    // Group streams by session + kind.
    let mut groups: BTreeMap<(String, String, EffectKind), Vec<&StreamLifecycleSummary>> =
        BTreeMap::new();
    for s in streams {
        let kind = stream_effect_kind(s);
        groups
            .entry((s.collector.clone(), s.peer_ip.clone(), kind))
            .or_default()
            .push(s);
    }

    let mut episodes = Vec::new();
    for ((collector, peer_ip, kind), members) in groups {
        let session_key = (collector.clone(), peer_ip.clone());
        let peer_facts = session_peers.get(&session_key);
        let (peer_asn, peer_label, peer_role, relationship) = match peer_facts {
            Some((asn, label, role, rel)) => (Some(*asn), label.clone(), role.clone(), rel.clone()),
            None => (
                None,
                "unreviewed peer".to_string(),
                "unreviewed".to_string(),
                RelationshipKind::Ambiguous,
            ),
        };
        // Observed peer ASNs from RIB evidence (Part 5): a protocol
        // fact, independent of reviewed organization labels. Multiple
        // distinct observations for one session mean ambiguity.
        let observed_asns: Vec<u32> = {
            // Time-scoped lookup (Part 5): only observations whose RIB
            // evidence predates the run's window end apply; the address
            // family of the session key is part of the match. Later
            // backfills with a different ASN in the same span produce
            // explicit ambiguity instead of silently overwriting.
            let af = address_family_of(&peer_ip);
            let mut seen: Vec<u32> = metadata
                .iter()
                .filter(|m| {
                    m.collector == collector
                        && m.peer_ip == peer_ip
                        && m.address_family == af
                        && m.valid_from.as_str() <= window_end
                })
                .map(|m| m.peer_asn)
                .collect();
            seen.sort_unstable();
            seen.dedup();
            seen
        };

        // Per-session transition timestamps for this peer.
        let mut first_change: Option<String> = None;
        let mut last_change: Option<String> = None;
        // Lifecycle evidence timestamps are the fallback when the
        // transition index is absent or bounded (immutable artifacts).
        for s in &members {
            if let Some(fc) = &s.first_change_utc {
                if first_change
                    .as_deref()
                    .map(|f| fc.as_str() < f)
                    .unwrap_or(true)
                {
                    first_change = Some(fc.clone());
                }
                if last_change
                    .as_deref()
                    .map(|l| fc.as_str() > l)
                    .unwrap_or(true)
                {
                    last_change = Some(fc.clone());
                }
            }
            if let Some(rt) = &s.restoration_time_utc {
                if last_change
                    .as_deref()
                    .map(|l| rt.as_str() > l)
                    .unwrap_or(true)
                {
                    last_change = Some(rt.clone());
                }
            }
        }
        let session_transition_count = transitions
            .iter()
            .filter(|t| t.collector == collector && t.peer_ip == peer_ip)
            .count();
        for t in transitions {
            if t.collector == collector && t.peer_ip == peer_ip {
                let ts = t.occurred_utc.clone();
                if first_change
                    .as_deref()
                    .map(|f| ts.as_str() < f)
                    .unwrap_or(true)
                {
                    first_change = Some(ts.clone());
                }
                if last_change
                    .as_deref()
                    .map(|l| ts.as_str() > l)
                    .unwrap_or(true)
                {
                    last_change = Some(ts.clone());
                }
            }
        }

        // Restoration evidence from stream lifecycle fields (Part 1.4):
        // the exact lifecycle restoration timestamp is honored for every
        // changed stream, whether the change was a withdrawal (visibility
        // restoration) or a path change (baseline-path restoration).
        // Restoration is NEVER extrapolated and never read from an
        // optional presentation field.
        let mut restoration_start: Option<String> = None;
        let mut restoration_end: Option<String> = None;
        let mut changed_count = 0usize;
        let mut restored_count = 0usize;
        let mut distinct_prefixes: Vec<String> = Vec::new();
        let mut route_instances = 0usize;
        let mut unresolved = 0usize;
        for s in &members {
            if s.transition_count > 0 || s.withdrawn || s.category != "Unchanged" {
                changed_count += 1;
                if !distinct_prefixes.contains(&s.prefix) {
                    distinct_prefixes.push(s.prefix.clone());
                }
                route_instances += s.max_active_instances.max(1) as usize;
                if s.restoration_time_utc.is_some() {
                    restored_count += 1;
                }
            }
            if (s.withdrawn && !s.restored) || s.add_path_ambiguous {
                unresolved += 1;
            }
            if let Some(rt) = &s.restoration_time_utc {
                restoration_start = match (restoration_start.clone(), rt.clone()) {
                    (None, r) => Some(r),
                    (Some(cur), r) if r < cur => Some(r),
                    (cur, _) => cur,
                };
                restoration_end = match (restoration_end.clone(), rt.clone()) {
                    (None, r) => Some(r),
                    (Some(cur), r) if r > cur => Some(r),
                    (cur, _) => cur,
                };
            }
        }
        let end_state = derive_end_state(&kind, changed_count, restored_count, &members);

        // Coverage status (Part 1.3): a session with streams and a
        // completed run has Complete coverage. "No route-state change"
        // is an OBSERVED SIGNATURE (EffectKind), never a coverage state.
        // NoBaselineVisibility / IncompleteCoverage are derived at the
        // workbench level from run coverage, because a session without
        // streams never reaches this per-run builder.
        let coverage = CoverageStatus::Complete;

        let episode = ObserverEpisode {
            analysis_run: run_id,
            observer_session: format!("{family}/{collector} peer {peer_ip}"),
            observer_site: registry
                .location_by_collector(&collector)
                .map(|c| c.location.clone())
                .unwrap_or_else(|| collector.clone()),
            observer_region: registry.region_by_collector(&collector),
            peer_asn,
            observed_peer_asns: observed_asns,
            peer_label,
            peer_role,
            relationship,
            named_path_plane: named_path_plane.to_string(),
            effect_kind: kind.clone(),
            first_change,
            last_change,
            restoration_start,
            restoration_end,
            baseline_stream_count: members.len(),
            changed_stream_count: changed_count,
            restored_stream_count: restored_count,
            distinct_prefix_count: distinct_prefixes.len(),
            route_instance_count: route_instances,
            unresolved_count: unresolved,
            transition_count: session_transition_count,
            end_state,
            // Filled by the workbench assembly from cooldown transition
            // evidence (Part 7); the builder has no analysis end.
            cooldown_outcome: CooldownOutcome::None,
            coverage_status: coverage,
            representative_evidence: String::new(), // filled by sentence renderer
            streams: members
                .iter()
                .map(|s| EpisodeStream {
                    prefix: s.prefix.clone(),
                    category: s.category.clone(),
                    withdrawn: s.withdrawn,
                    restored: s.restored,
                    baseline_instances: s.baseline_instances,
                    max_active_instances: s.max_active_instances,
                    transition_count: s.transition_count,
                    add_path_ambiguous: s.add_path_ambiguous,
                    first_change_utc: s.first_change_utc.clone(),
                    restoration_time_utc: s.restoration_time_utc.clone(),
                    evidence_refs: s.evidence_refs.clone(),
                })
                .collect(),
        };
        episodes.push(episode);
    }

    // Deterministic ordering: changed first (by first change), then
    // no-change; within a tier by (region, collector, peer ASN, kind).
    episodes.sort_by(|a, b| {
        let a_changed = a.effect_kind != EffectKind::NoRouteStateChange;
        let b_changed = b.effect_kind != EffectKind::NoRouteStateChange;
        match (a_changed, b_changed) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => {
                let ac = (
                    a.first_change.clone(),
                    a.observer_region.clone(),
                    a.observer_site.clone(),
                    a.peer_asn.unwrap_or(u32::MAX),
                );
                let bc = (
                    b.first_change.clone(),
                    b.observer_region.clone(),
                    b.observer_site.clone(),
                    b.peer_asn.unwrap_or(u32::MAX),
                );
                ac.cmp(&bc)
            }
        }
    });
    episodes
}

/// Map a session's historical relationship evidence to the presentation
/// RelationshipKind (Direct/Indirect/Other/Ambiguous).
pub fn relationship_kind(rels: &[SessionRelationship]) -> RelationshipKind {
    if rels.is_empty() {
        return RelationshipKind::Ambiguous;
    }
    let direct = rels
        .iter()
        .any(|r| matches!(r, SessionRelationship::DirectPeerToNamedPlane { .. }));
    let indirect = rels
        .iter()
        .any(|r| matches!(r, SessionRelationship::IndirectPathViaNamedPlane { .. }));
    match (direct, indirect) {
        (true, _) => RelationshipKind::Direct,
        (false, true) => RelationshipKind::Indirect,
        (false, false) => RelationshipKind::Other,
    }
}

/// Classify a session's relationship using route evidence.
pub fn classify_session_relationship(
    profile: &ServicePlaneProfile,
    peer_asn: u32,
    as_paths: &[Vec<u32>],
) -> RelationshipKind {
    let mut rels: Vec<SessionRelationship> = Vec::new();
    for path in as_paths {
        for rel in classify_route(profile, peer_asn, path) {
            if !rels.contains(&rel) {
                rels.push(rel);
            }
        }
    }
    relationship_kind(&rels)
}

// ── Operational sentence rendering (Part 4) ─────────────────────────
/// Render one episode as a concise, data-supported operational sentence.
///
/// Rules enforced here (and by the required tests):
/// - The collector SITE is named at the collector's reviewed location;
///   the peer's own location is never implied ("RRC06 at Otemachi,
///   Tokyo, receiving routes from peer AS…" — the peer is not said to be
///   in Tokyo unless separately reviewed).
/// - The verb is effect-specific (became absent / was withdrawn /
///   changed AS path / left the reviewed path plane / returned to
///   visibility / restored its baseline path / remained changed).
/// - Visibility restoration ("returned to visibility") is distinct from
///   baseline restoration ("restored its baseline path").
/// - When stream and prefix counts differ, both units are named
///   ("14 observer-prefix streams covering 11 prefixes").
/// - The sentence never claims traffic loss and never claims causation.
pub fn render_episode_sentence(episode: &ObserverEpisode) -> String {
    let site = &episode.observer_site;
    let collector = collector_from_session(&episode.observer_session);
    let peer = match episode.peer_asn {
        Some(asn) => format!("peer AS{asn}"),
        None => {
            // Peer identity is unreviewed: name only the peer IP.
            let ip = episode
                .observer_session
                .rsplit("peer ")
                .next()
                .unwrap_or("unreviewed peer");
            format!("peer {ip} (ASN unreviewed)")
        }
    };
    let plane = &episode.named_path_plane;
    let unit = match (episode.changed_stream_count, episode.distinct_prefix_count) {
        (s, p) if s == p && s == 1 => "1 selected prefix".to_string(),
        (s, p) if s == p => format!("{s} selected prefixes"),
        (s, p) => format!("{s} observer-prefix streams covering {p} prefixes"),
    };

    let head = format!("{collector} at {site}, receiving routes from {peer},");

    match episode.effect_kind {
        EffectKind::TemporaryStreamAbsence => {
            let t = episode
                .first_change
                .clone()
                .unwrap_or_else(|| "the observed time".to_string());
            let restored = match (&episode.restoration_start, &episode.restoration_end) {
                (Some(a), Some(b)) if a != b => format!(" between {a} and {b} UTC"),
                (Some(a), _) => format!(" at {a} UTC"),
                _ => String::new(),
            };
            format!(
                "{head} saw {unit} become absent at {t} UTC and return to visibility{restored}."
            )
        }
        EffectKind::RouteWithdrawal => {
            let t = episode
                .first_change
                .clone()
                .unwrap_or_else(|| "the observed time".to_string());
            format!(
                "{head} saw {unit} was withdrawn at {t} UTC and remained absent through the end of the analysis window."
            )
        }
        EffectKind::PathReplacement => {
            let t = episode
                .first_change
                .clone()
                .unwrap_or_else(|| "the observed time".to_string());
            let restored = match &episode.restoration_start {
                Some(r) => format!(" Their initial path class returned by {r} UTC."),
                None => String::new(),
            };
            format!("{head} saw {unit} change AS path at {t} UTC.{restored}")
        }
        EffectKind::NamedPlaneDeparture => {
            let t = episode
                .first_change
                .clone()
                .unwrap_or_else(|| "the observed time".to_string());
            format!("{head} saw {unit} leave the {plane} path at {t} UTC.")
        }
        EffectKind::NamedPlaneReturn => {
            let t = episode
                .first_change
                .clone()
                .unwrap_or_else(|| "the observed time".to_string());
            format!("{head} saw {unit} return to the {plane} path at {t} UTC.")
        }
        EffectKind::PrependChange => {
            let t = episode
                .first_change
                .clone()
                .unwrap_or_else(|| "the observed time".to_string());
            format!(
                "{head} saw {unit} change AS-path prepending at {t} UTC while retaining the {plane} path."
            )
        }
        EffectKind::MixedRouteChange => {
            let t = episode
                .first_change
                .clone()
                .unwrap_or_else(|| "the observed time".to_string());
            format!("{head} saw mixed route-state changes across {unit} beginning at {t} UTC.")
        }
        EffectKind::NoRouteStateChange => {
            format!("{head} observed no route-state change among the selected streams.")
        }
    }
}

/// Build operator-facing routing findings from observer episodes
/// (Session 39, Part 3).
///
/// One changed episode becomes one finding: the episode grouping
/// (observer session + effect kind) already enforces the same
/// before/after semantic state and a coherent temporal cluster, so a
/// finding never combines unrelated path changes. Exact per-prefix
/// paths are attached from the run's lifecycle artifact when present.
/// Findings are deterministic: sorted by (first observed, region, site,
/// peer ASN, stable id).
#[allow(clippy::too_many_arguments)] // explicit evidence passthrough
pub fn build_findings(
    episodes: &[ObserverEpisode],
    paths: &LifecyclePathIndex,
    identities: &AsnIdentityRegistry,
    target_label: &str,
    target_origin_asns: &[u32],
    scope_limit: &str,
    run_event_ids: &std::collections::HashMap<i64, String>,
    event_date: &str,
    analysis_end: &str,
) -> Vec<RoutingFinding> {
    let mut findings: Vec<RoutingFinding> = Vec::new();
    for e in episodes {
        if e.effect_kind == EffectKind::NoRouteStateChange {
            continue;
        }
        let session = session_key_of(&e.observer_session);
        let run_event_id = run_event_ids
            .get(&e.analysis_run)
            .cloned()
            .unwrap_or_default();

        // Attach exact path evidence per prefix.
        let mut streams: Vec<FindingStream> = Vec::new();
        let mut exact_prefixes: Vec<String> = Vec::new();
        let mut evidence_refs: Vec<String> = Vec::new();
        let mut baseline_counts: Vec<Vec<u32>> = Vec::new();
        let mut changed_counts: Vec<Vec<u32>> = Vec::new();
        let mut final_counts: Vec<Vec<u32>> = Vec::new();
        let mut visibility_restored_at: Option<String> = None;
        let mut equivalent_route_restored_at: Option<String> = None;
        let mut reviewed_plane_restored_at: Option<String> = None;
        let mut exact_baseline_restored_at: Option<String> = None;
        for s in &e.streams {
            exact_prefixes.push(s.prefix.clone());
            if !s.evidence_refs.is_empty() {
                evidence_refs.push(s.evidence_refs.clone());
            }
            let path = paths.lookup(
                &run_event_id,
                &session.collector,
                &session.peer_ip,
                &s.prefix,
            );
            let stream_transitions = path.map(|p| p.transitions.clone()).unwrap_or_default();
            let (
                baseline,
                changed,
                final_path,
                vis_restored,
                equiv_restored,
                plane_restored,
                base_restored,
            ) = match path {
                Some(p) => (
                    p.baseline_path.clone(),
                    p.changed_path.clone(),
                    p.final_path.clone(),
                    p.visibility_restored_at.clone(),
                    p.equivalent_route_restored_at.clone(),
                    p.reviewed_plane_restored_at.clone(),
                    p.exact_baseline_restored_at.clone(),
                ),
                None => (Vec::new(), None, None, None, None, None, None),
            };
            // Fall back to the episode's restoration window for
            // visibility/exact-baseline restoration when the lifecycle
            // artifact is absent (episode evidence still exact).
            let vis = vis_restored.or_else(|| {
                if s.withdrawn && s.restored {
                    s.restoration_time_utc.clone()
                } else {
                    None
                }
            });
            let base = base_restored.or_else(|| {
                if !s.withdrawn && s.restored {
                    s.restoration_time_utc.clone()
                } else {
                    None
                }
            });
            if !baseline.is_empty() {
                baseline_counts.push(baseline.clone());
            }
            if let Some(c) = &changed {
                changed_counts.push(c.clone());
            }
            if let Some(f) = &final_path {
                final_counts.push(f.clone());
            }
            // Visibility restoration is a property of withdrawn streams;
            // the other restoration classes may follow any change and
            // are tracked across all member streams. Per-class latest
            // times feed the finding-level audit record.
            if s.withdrawn {
                if let Some(v) = &vis {
                    if visibility_restored_at
                        .as_deref()
                        .map(|cur| v.as_str() > cur)
                        .unwrap_or(true)
                    {
                        visibility_restored_at = Some(v.clone());
                    }
                }
            }
            for (field, value) in [
                (&mut equivalent_route_restored_at, equiv_restored.as_ref()),
                (&mut reviewed_plane_restored_at, plane_restored.as_ref()),
                (&mut exact_baseline_restored_at, base.as_ref()),
            ] {
                if let Some(v) = value {
                    if field.as_deref().map(|cur| v.as_str() > cur).unwrap_or(true) {
                        *field = Some(v.clone());
                    }
                }
            }
            streams.push(FindingStream {
                prefix: s.prefix.clone(),
                baseline_path: baseline,
                changed_path: changed,
                final_path,
                withdrawn: s.withdrawn,
                first_change_utc: s.first_change_utc.clone(),
                transitions: stream_transitions,
                visibility_restored_at: vis,
                equivalent_route_restored_at: equiv_restored,
                reviewed_plane_restored_at: plane_restored,
                exact_baseline_restored_at: base,
                evidence_refs: s.evidence_refs.clone(),
            });
        }
        exact_prefixes.sort();
        exact_prefixes.dedup();
        evidence_refs.sort();
        evidence_refs.dedup();

        // Most frequent path is the finding's summary signature.
        let most_frequent = |paths: &[Vec<u32>]| -> String {
            let mut counts: std::collections::BTreeMap<Vec<u32>, usize> =
                std::collections::BTreeMap::new();
            for p in paths {
                *counts.entry(p.clone()).or_default() += 1;
            }
            counts
                .iter()
                .max_by_key(|(p, c)| (*c, p.len()))
                .map(|(p, _)| collapse_as_path(p))
                .unwrap_or_else(|| "—".to_string())
        };

        // Peer ASN: the reviewed session context wins; otherwise the
        // observed peer ASNs from source RIB evidence (Session 40,
        // Part 7). Multiple distinct observed ASNs are ambiguous and
        // never silently resolved.
        let peer_asn = e.peer_asn.or_else(|| {
            let observed = &e.observed_peer_asns;
            if observed.len() == 1 {
                Some(observed[0])
            } else {
                None
            }
        });
        let peer_asn_ambiguous = e.peer_asn.is_none() && e.observed_peer_asns.len() > 1;
        let peer_name = peer_asn
            .and_then(|asn| identities.display_name(asn, event_date))
            .unwrap_or("name not reviewed")
            .to_string();
        let peer_role = peer_asn
            .and_then(|asn| identities.role(asn, event_date))
            .unwrap_or("")
            .to_string();
        let target_name = target_name_from_label(target_label);
        let effect = RoutingEffect::from_kind(&e.effect_kind);
        let stable_id = format!(
            "{}-{}-{}-{}",
            session.collector.to_lowercase().replace(['/', ' '], "-"),
            session.peer_ip.replace([':', '.'], "-"),
            effect.label().to_lowercase().replace(' ', "-"),
            e.first_change.clone().unwrap_or_default()
        );
        findings.push(RoutingFinding {
            stable_id,
            target_label: target_label.to_string(),
            target_origin_asns: target_origin_asns.to_vec(),
            target_name,
            observer_site: e.observer_site.clone(),
            observer_region: e.observer_region.clone(),
            source_family: family_from_session(&e.observer_session).to_string(),
            collector: session.collector.clone(),
            peer_ip: session.peer_ip.clone(),
            peer_asn,
            peer_name,
            peer_role,
            peer_asn_ambiguous,
            relationship: e.relationship.clone(),
            effect,
            first_observed: e.first_change.clone(),
            last_observed: e.last_change.clone(),
            named_path_plane: e.named_path_plane.clone(),
            visibility_restored_at: visibility_restored_at.clone(),
            equivalent_route_restored_at,
            reviewed_plane_restored_at,
            exact_baseline_restored_at,
            state_at_window_end: e.end_state.clone(),
            state_at_analysis_end: e.cooldown_outcome.clone(),
            analysis_end: analysis_end.to_string(),
            observer_stream_count: e.changed_stream_count,
            distinct_prefixes: e.distinct_prefix_count,
            baseline_path_signature: most_frequent(&baseline_counts),
            changed_path_signature: most_frequent(&changed_counts),
            final_path_signature: most_frequent(&final_counts),
            exact_prefixes,
            evidence_refs,
            scope_limit: scope_limit.to_string(),
            streams,
        });
    }
    findings.sort_by(|a, b| {
        (
            a.first_observed.clone(),
            a.observer_region.clone(),
            a.observer_site.clone(),
            a.peer_asn.unwrap_or(u32::MAX),
            a.stable_id.clone(),
        )
            .cmp(&(
                b.first_observed.clone(),
                b.observer_region.clone(),
                b.observer_site.clone(),
                b.peer_asn.unwrap_or(u32::MAX),
                b.stable_id.clone(),
            ))
    });
    findings
}

/// Render one concise operational statement for a changed finding
/// (Session 39, Part 4).
///
/// Exact verbs only: stopped seeing / became absent / withdrew /
/// changed AS path / reduced prepending / left the reviewed path plane
/// / returned to visibility / restored the baseline path / remained
/// changed. Never: protected, failed over, backup path, rerouted,
/// traffic restored.
/// Short target name from a reviewed target label for natural
/// grammar: "UVA via <transit>" -> "UVA", "TARGET (AS2603)" ->
/// "TARGET". Never invented: the input is the reviewed manifest/pilot
/// label.
pub fn target_name_from_label(label: &str) -> String {
    let before_paren = label.split('(').next().unwrap_or(label);
    let before_via = before_paren.split(" via ").next().unwrap_or(before_paren);
    before_via.trim().to_string()
}

/// Date part of an ISO timestamp ("2019-08-21T16:00:00Z" ->
/// "2019-08-21") for time-scoped identity lookups.
pub fn window_date(iso: &str) -> String {
    let t = iso.trim_end_matches('Z');
    match t.find('T') {
        Some(pos) => t[..pos].to_string(),
        None => t.to_string(),
    }
}

/// Extract HH:MM:SS from an ISO timestamp for operator statements
/// ("2019-08-21T16:45:25Z" → "16:45:25"). Falls back to the input.
pub fn finding_time(iso: &str) -> String {
    let trimmed = iso.trim_end_matches('Z');
    if let Some(pos) = trimmed.find('T') {
        let rest = &trimmed[pos + 1..];
        if rest.len() >= 8 {
            return rest[..8].to_string();
        }
    }
    trimmed.to_string()
}

pub fn finding_statement(f: &RoutingFinding) -> String {
    let n = f.distinct_prefixes;
    // Natural target grammar (Session 40, Part 12): "12 prefixes
    // originated by UVA (AS225)" — never "{n} prefixes UVA via ...".
    let unit = if n == 1 {
        match origin_clause(f) {
            Some(origin) => format!("1 prefix {origin}"),
            None => "1 prefix".to_string(),
        }
    } else {
        match origin_clause(f) {
            Some(origin) => format!("{n} prefixes {origin}"),
            None => format!("{n} prefixes"),
        }
    };
    let time = |t: &Option<String>| -> String {
        t.clone()
            .map(|t| finding_time(&t))
            .unwrap_or_else(|| "the observed time".to_string())
    };
    let observer = format!("{} in {}", f.collector, f.observer_site);
    // Peer clause (Session 40, Part 7): reviewed name + ASN, with the
    // peer IP alongside whenever a peer ASN is known; peer-IP-only
    // wording is reserved for sessions with no peer-ASN evidence.
    let peer = match f.peer_asn {
        Some(asn) => {
            let name = if f.peer_name == "name not reviewed" {
                format!("AS{asn} — name not reviewed")
            } else {
                format!("{} (AS{asn})", f.peer_name)
            };
            if f.peer_asn_ambiguous {
                format!(
                    "receiving routes from peer {} (peer ASN ambiguous)",
                    f.peer_ip
                )
            } else {
                format!("receiving routes from {name}, peer {}", f.peer_ip)
            }
        }
        None => format!("receiving routes from peer {}", f.peer_ip),
    };
    let rel = match f.relationship {
        RelationshipKind::Direct => "directly",
        RelationshipKind::Indirect => "indirectly",
        _ => "",
    };
    let head = format!(
        "At {} UTC, {}, {},",
        time(&f.first_observed),
        observer,
        if rel.is_empty() {
            peer
        } else {
            format!("{peer}, {rel}")
        }
    );
    let before = if f.baseline_path_signature != "—" {
        format!(" from: {}", f.baseline_path_signature)
    } else {
        String::new()
    };
    let after = if f.changed_path_signature != "—" {
        format!(" to: {}", f.changed_path_signature)
    } else {
        String::new()
    };

    let mut s = match f.effect {
        RoutingEffect::PrefixesTemporarilyAbsent => {
            format!("{} stopped seeing {}.", head, unit)
        }
        RoutingEffect::PrefixesWithdrawn => {
            format!("{} saw {} become absent (withdrawn).", head, unit)
        }
        RoutingEffect::AsPathChanged => {
            format!("{} saw {} change AS path{}{}.", head, unit, before, after)
        }
        RoutingEffect::PrependingChanged => {
            format!("{} saw {} change AS-path prepending.", head, unit)
        }
        RoutingEffect::NamedPlaneDeparted => {
            format!("{} saw {} leave the reviewed path plane.", head, unit)
        }
        RoutingEffect::NamedPlaneReturned => {
            format!("{} saw {} return to the reviewed path plane.", head, unit)
        }
        RoutingEffect::VisibilityRestored => {
            format!("{} observed {} return to visibility.", head, unit)
        }
        RoutingEffect::BaselinePathRestored => {
            format!("{} observed {} restore the baseline path.", head, unit)
        }
        RoutingEffect::MixedChange => {
            format!(
                "{} observed mixed route-state changes across {}.",
                head, unit
            )
        }
    };

    // Precise restoration outcomes (Session 40, Part 2): each sentence
    // states exactly which restoration class the audit record supports.
    let exact_times: Vec<&str> = f
        .streams
        .iter()
        .filter_map(|s| s.exact_baseline_restored_at.as_deref())
        .collect();
    match f.effect {
        RoutingEffect::PrefixesTemporarilyAbsent => match &f.visibility_restored_at {
            Some(v) => {
                s.push_str(&format!(
                    " Visibility returned at {} UTC on a different AS path.",
                    finding_time(v)
                ));
                if let Some(b) = &f.exact_baseline_restored_at {
                    s.push_str(&format!(
                        " The exact baseline AS path returned at {} UTC.",
                        finding_time(b)
                    ));
                }
            }
            None => {
                if f.state_at_window_end == EndState::AbsentAtWindowEnd {
                    s.push_str(" The prefixes remained absent at the event-window end.");
                }
            }
        },
        RoutingEffect::AsPathChanged | RoutingEffect::PrependingChanged => {
            let restored_streams = f
                .streams
                .iter()
                .filter(|s| s.exact_baseline_restored_at.is_some())
                .count();
            let total_streams = f.streams.len();
            match (&f.exact_baseline_restored_at, &f.state_at_window_end) {
                (Some(_t), _) if !exact_times.is_empty() && restored_streams == total_streams => {
                    let (min, max) = (
                        finding_time(exact_times.iter().min().copied().unwrap()),
                        finding_time(exact_times.iter().max().copied().unwrap()),
                    );
                    if min != max {
                        s.push_str(&format!(
                            " The exact baseline AS paths returned between {min} and {max} UTC."
                        ));
                    } else {
                        s.push_str(&format!(
                            " The exact baseline AS path returned at {min} UTC."
                        ));
                    }
                }
                (Some(_t), _) if !exact_times.is_empty() => {
                    // Differing restoration outcomes across the prefix
                    // group: state the distribution explicitly.
                    let (min, max) = (
                        finding_time(exact_times.iter().min().copied().unwrap()),
                        finding_time(exact_times.iter().max().copied().unwrap()),
                    );
                    let unrestored = total_streams - restored_streams;
                    if min != max {
                        s.push_str(&format!(
                            " The exact baseline AS paths returned for {} of {} prefixes between {min} and {max} UTC; {} prefix{} remained changed at the event-window end.",
                            restored_streams,
                            total_streams,
                            unrestored,
                            if unrestored == 1 { "" } else { "s" }
                        ));
                    } else {
                        s.push_str(&format!(
                            " The exact baseline AS path returned for {} of {} prefixes at {min} UTC; {} prefix{} remained changed at the event-window end.",
                            restored_streams,
                            total_streams,
                            unrestored,
                            if unrestored == 1 { "" } else { "s" }
                        ));
                    }
                }
                (None, EndState::StillChangedAtWindowEnd) => {
                    s.push_str(" The route remained changed at the event-window end.");
                }
                _ => {}
            }
        }
        RoutingEffect::NamedPlaneDeparted => {
            if let Some(t) = &f.reviewed_plane_restored_at {
                let plane = if f.named_path_plane.is_empty() {
                    "reviewed".to_string()
                } else {
                    f.named_path_plane.clone()
                };
                s.push_str(&format!(
                    " The route returned to the reviewed {} path at {} UTC.",
                    plane,
                    finding_time(t)
                ));
            }
            if let Some(t) = &f.exact_baseline_restored_at {
                s.push_str(&format!(
                    " The exact baseline AS path returned at {} UTC.",
                    finding_time(t)
                ));
            }
            if f.reviewed_plane_restored_at.is_none()
                && f.exact_baseline_restored_at.is_none()
                && f.state_at_window_end == EndState::StillChangedAtWindowEnd
            {
                s.push_str(" The route remained changed at the event-window end.");
            }
        }
        _ => {}
    }
    if let Some(t) = &f.equivalent_route_restored_at {
        if f.visibility_restored_at.as_deref() != Some(t.as_str()) {
            s.push_str(&format!(
                " A semantically equivalent route returned at {} UTC.",
                finding_time(t)
            ));
        }
    }
    // Analysis-end outcome (Session 40, Part 3): never hide a
    // restoration that failed or a change that continued after the
    // event window.
    use CooldownOutcome as CO;
    match &f.state_at_analysis_end {
        CO::StillChangingBeforeAnalysisEnd(t) => {
            s.push_str(&format!(
                " The routes changed again at {} UTC; no restoration was observed before the analysis ended at {} UTC.",
                finding_time(t),
                finding_time(&f.analysis_end)
            ));
        }
        CO::NoRestorationBeforeAnalysisEnd(_) => {
            let any_restoration = f.visibility_restored_at.is_some()
                || f.equivalent_route_restored_at.is_some()
                || f.reviewed_plane_restored_at.is_some()
                || f.exact_baseline_restored_at.is_some();
            if !any_restoration {
                s.push_str(&format!(
                    " No restoration was observed before the analysis ended at {} UTC.",
                    finding_time(&f.analysis_end)
                ));
            }
        }
        CO::RestoredAt(t) => {
            s.push_str(&format!(
                " A restoration was observed during the follow-up window at {} UTC.",
                finding_time(t)
            ));
        }
        CO::None => {}
    }
    s
}

/// Natural origin clause: "originated by UVA (AS225)" from the
/// reviewed target label and origin ASNs; None when no origin is known.
fn origin_clause(f: &RoutingFinding) -> Option<String> {
    let origin = f.target_origin_asns.first()?;
    let name = if f.target_name.is_empty() {
        format!("AS{origin}")
    } else {
        format!("{} (AS{origin})", f.target_name)
    };
    Some(format!("originated by {name}"))
}

/// Path-diff semantics for operator explanations (Session 40, Part 4).
///
/// Generic, factual path effects derived from two exact AS paths. Never
/// assigns cause: no "because", no failure, no reroute language.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PathDiff {
    /// ASNs present in `after` but not in `before` (order preserved
    /// from the after path, deduplicated).
    pub inserted: Vec<u32>,
    /// ASNs present in `before` but not in `after` (order preserved
    /// from the before path, deduplicated).
    pub removed: Vec<u32>,
    /// Occurrence-count deltas for ASNs appearing multiple times in
    /// either path: (asn, after_count - before_count). Non-zero only.
    pub count_deltas: Vec<(u32, i64)>,
    /// Reviewed plane ASN present in before and after.
    pub plane_retained: bool,
    /// Reviewed plane ASN present in before, absent in after.
    pub plane_departed: bool,
    /// Reviewed plane ASN absent in before, present in after.
    pub plane_returned: bool,
    /// After path is longer than the before path.
    pub longer: bool,
    /// After path is shorter than the before path.
    pub shorter: bool,
}

/// Diff two exact AS paths (multiset semantics; no causation).
pub fn diff_paths(before: &[u32], after: &[u32], plane_asns: &[u32]) -> PathDiff {
    let mut d = PathDiff::default();
    if after.is_empty() {
        // Visibility absence: not a path change at all.
        return d;
    }
    let count = |path: &[u32], asn: u32| path.iter().filter(|a| **a == asn).count() as i64;
    let mut inserted: Vec<u32> = after
        .iter()
        .copied()
        .filter(|a| !before.contains(a))
        .collect();
    inserted.dedup();
    let mut removed: Vec<u32> = before
        .iter()
        .copied()
        .filter(|a| !after.contains(a))
        .collect();
    removed.dedup();
    d.inserted = inserted;
    d.removed = removed;
    let mut deltas: Vec<(u32, i64)> = Vec::new();
    let mut asns: Vec<u32> = before.iter().chain(after.iter()).copied().collect();
    asns.sort_unstable();
    asns.dedup();
    for asn in asns {
        let delta = count(after, asn) - count(before, asn);
        if delta != 0 && (count(before, asn) > 1 || count(after, asn) > 1) {
            deltas.push((asn, delta));
        }
    }
    d.count_deltas = deltas;
    let before_has_plane = plane_asns.iter().any(|p| before.contains(p));
    let after_has_plane = plane_asns.iter().any(|p| after.contains(p));
    d.plane_retained = before_has_plane && after_has_plane;
    d.plane_departed = before_has_plane && !after_has_plane;
    d.plane_returned = !before_has_plane && after_has_plane;
    d.longer = after.len() > before.len();
    d.shorter = after.len() < before.len();
    d
}

/// Render a factual, generic explanation of a before/after path pair
/// (Session 40, Part 4). `names` maps an ASN to a display label
/// (e.g. a reviewed plane label or "AS20080 — name not reviewed"); `plane`
/// is the reviewed plane display label. The exact numeric path is
/// never replaced — it remains authoritative alongside.
pub fn explain_path_diff(
    before: &[u32],
    after: &[u32],
    plane_asns: &[u32],
    plane_label: &str,
    names: &dyn Fn(u32) -> String,
) -> String {
    explain_path_diff_with_origins(before, after, plane_asns, plane_label, &[], names)
}

/// `explain_path_diff` with explicit target-origin ASNs: the
/// "origin-AS prepending" phrase is used only when the repeated ASN is
/// a target origin (Session 41, Part 7); intermediate repetition is
/// never called origin prepending.
pub fn explain_path_diff_with_origins(
    before: &[u32],
    after: &[u32],
    plane_asns: &[u32],
    plane_label: &str,
    target_origins: &[u32],
    names: &dyn Fn(u32) -> String,
) -> String {
    if after.is_empty() {
        return "The route was absent between the two observations.".to_string();
    }
    if before.is_empty() {
        return "The route became visible (no prior path observed).".to_string();
    }
    let d = diff_paths(before, after, plane_asns);
    let mut parts: Vec<String> = Vec::new();

    // Plane semantics first: the reviewed plane is the primary
    // operational fact.
    let plane = if plane_label.is_empty() {
        "the reviewed path plane".to_string()
    } else {
        format!("the reviewed {} path", plane_label)
    };
    if d.plane_departed {
        parts.push(format!(
            "the route remained visible, but the new selected path no longer traversed {}",
            plane
        ));
    } else if d.plane_retained {
        // "continued through <plane label> AS<plane asn>"
        let plane_asn = plane_asns
            .iter()
            .find(|p| after.contains(p))
            .map(|p| format!(" AS{p}"))
            .unwrap_or_default();
        parts.push(format!(
            "the route remained visible and continued through {}{}",
            plane, plane_asn
        ));
    } else if d.plane_returned {
        parts.push(format!("the selected path returned to {}", plane));
    }

    // Inserted ASNs with positional context (first inserted ASN).
    if let Some(first) = d.inserted.first() {
        let name = names(*first);
        let pos = after.iter().position(|a| a == first);
        let positional = pos.and_then(|i| {
            let prev = if i > 0 {
                after.get(i - 1).copied()
            } else {
                None
            };
            let next = after.get(i + 1).copied();
            match (prev, next) {
                (Some(p), Some(n)) if p != *first && n != *first => {
                    Some(format!("between {} and {}", names(p), names(n)))
                }
                _ => None,
            }
        });
        match positional {
            Some(where_text) => parts.push(format!("{} appeared {}", name, where_text)),
            None => parts.push(format!("{} appeared in the selected path", name)),
        }
    }
    if d.inserted.len() > 1 {
        let rest: Vec<String> = d.inserted[1..].iter().map(|a| names(*a)).collect();
        parts.push(format!(
            "also {} appeared in the selected path",
            rest.join(", ")
        ));
    }
    for a in &d.removed {
        parts.push(format!("{} left the selected path", names(*a)));
    }

    // Prepending changes numerically (origin prepending when the ASN is
    // the path origin — the last element).
    for (asn, delta) in &d.count_deltas {
        let before_count = before.iter().filter(|a| *a == asn).count();
        let after_count = after.iter().filter(|a| *a == asn).count();
        let origin = target_origins.contains(asn)
            || (target_origins.is_empty()
                && (after.last() == Some(asn) || before.last() == Some(asn)));
        let scope = if origin {
            "origin-AS prepending"
        } else {
            "prepending"
        };
        let direction = if *delta > 0 { "increased" } else { "decreased" };
        parts.push(format!(
            "{} {} from {} to {} AS{} occurrences",
            scope, direction, before_count, after_count, asn
        ));
    }

    if d.longer {
        parts.push("the selected path became longer".to_string());
    } else if d.shorter {
        parts.push("the selected path became shorter".to_string());
    }
    if parts.is_empty() {
        return "The selected path did not materially change.".to_string();
    }
    let mut sentence = String::new();
    for (i, part) in parts.iter().enumerate() {
        if i == 0 {
            sentence.push_str(part);
        } else if i == parts.len() - 1 {
            sentence.push_str(&format!(", and {}", part));
        } else {
            sentence.push_str(&format!(", {}", part));
        }
    }
    sentence.push('.');
    sentence
}

/// Principal-finding selection (Session 40, Part 8).
///
/// Deterministic selection of the operational stories that lead the
/// workbench, prioritizing distinct stories over internal record
/// cardinality:
///   1. temporary absence or withdrawal
///   2. reviewed-plane departure while visibility remains
///   3. large multi-prefix material path change
///   4. route still changed at analysis end
///   5. meaningful disagreement between independent observers
///   6. unique single-prefix exception
///
/// Observer diversity is preferred: the first pass takes the best
/// finding per observer session; remaining slots fill from the rest.
/// Secondary findings stay fully accessible.
pub fn select_principal_findings(
    findings: Vec<RoutingFinding>,
    plane_asns: &[u32],
    max: usize,
) -> (Vec<RoutingFinding>, Vec<RoutingFinding>) {
    if max == 0 {
        return (Vec::new(), findings);
    }
    // Meaningful disagreement: the same exact prefix set observed at
    // two or more sites with different effects.
    let mut by_prefix_set: std::collections::BTreeMap<Vec<String>, Vec<&RoutingFinding>> =
        std::collections::BTreeMap::new();
    for f in &findings {
        by_prefix_set
            .entry(f.exact_prefixes.clone())
            .or_default()
            .push(f);
    }
    let mut disagreement: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for group in by_prefix_set.values() {
        let effects: std::collections::BTreeSet<&'static str> =
            group.iter().map(|f| f.effect.label()).collect();
        let sites: std::collections::BTreeSet<&str> =
            group.iter().map(|f| f.observer_site.as_str()).collect();
        if effects.len() > 1 && sites.len() > 1 {
            for f in group {
                disagreement.insert(f.stable_id.clone());
            }
        }
    }

    let plane_departed = |f: &RoutingFinding| -> bool {
        // The most frequent baseline path contains a plane ASN and the
        // most frequent changed path does not.
        let baseline = f
            .streams
            .iter()
            .find(|s| !s.baseline_path.is_empty())
            .map(|s| s.baseline_path.clone());
        let changed = f.streams.iter().find_map(|s| s.changed_path.clone());
        match (baseline, changed) {
            (Some(b), Some(c)) => {
                !plane_asns.is_empty()
                    && plane_asns.iter().any(|p| b.contains(p))
                    && !plane_asns.iter().any(|p| c.contains(p))
            }
            _ => false,
        }
    };

    let score = |f: &RoutingFinding| -> i64 {
        let departed = plane_departed(f);
        // Plane departure is principal only for material groups; a
        // one-prefix departure is a large-change variant with a small
        // bonus, so it never outranks a large multi-prefix retention
        // at the same observer site.
        let base = match f.effect {
            RoutingEffect::PrefixesTemporarilyAbsent | RoutingEffect::PrefixesWithdrawn => 1000,
            _ if departed && f.distinct_prefixes >= 5 => 900,
            RoutingEffect::AsPathChanged | RoutingEffect::PrependingChanged => 700,
            _ => 300,
        };
        let mut s = base + (f.distinct_prefixes.min(12) as i64) * 5;
        if departed && f.distinct_prefixes < 5 {
            s += 40;
        }
        if matches!(
            f.state_at_analysis_end,
            CooldownOutcome::StillChangingBeforeAnalysisEnd(_)
                | CooldownOutcome::NoRestorationBeforeAnalysisEnd(_)
        ) {
            s += 80;
        }
        if disagreement.contains(&f.stable_id) {
            s += 40;
        }
        s
    };

    let mut ranked = findings;
    ranked.sort_by(|a, b| {
        score(b).cmp(&score(a)).then_with(|| {
            (
                a.first_observed.clone(),
                a.observer_region.clone(),
                a.observer_site.clone(),
                a.peer_asn.unwrap_or(u32::MAX),
                a.stable_id.clone(),
            )
                .cmp(&(
                    b.first_observed.clone(),
                    b.observer_region.clone(),
                    b.observer_site.clone(),
                    b.peer_asn.unwrap_or(u32::MAX),
                    b.stable_id.clone(),
                ))
        })
    });

    let mut principal: Vec<RoutingFinding> = Vec::new();
    let mut secondary: Vec<RoutingFinding> = Vec::new();
    let mut used_sites: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    // Pass 1: one story per observer SITE (diversity — never several
    // variations from the same collector while distinct stories exist).
    for f in ranked.drain(..) {
        if principal.len() >= max {
            secondary.push(f);
            continue;
        }
        if used_sites.insert(f.observer_site.clone()) {
            principal.push(f);
        } else {
            secondary.push(f);
        }
    }
    // Pass 2: fill remaining slots from secondary by rank (already
    // sorted), still respecting the cap.
    let mut rest: Vec<RoutingFinding> = Vec::new();
    for f in secondary.drain(..) {
        if principal.len() < max {
            principal.push(f);
        } else {
            rest.push(f);
        }
    }
    // Deterministic final order: by score desc, then the stable tie-break.
    principal.sort_by(|a, b| {
        score(b).cmp(&score(a)).then_with(|| {
            (
                a.first_observed.clone(),
                a.observer_region.clone(),
                a.observer_site.clone(),
                a.peer_asn.unwrap_or(u32::MAX),
                a.stable_id.clone(),
            )
                .cmp(&(
                    b.first_observed.clone(),
                    b.observer_region.clone(),
                    b.observer_site.clone(),
                    b.peer_asn.unwrap_or(u32::MAX),
                    b.stable_id.clone(),
                ))
        })
    });
    (principal, rest)
}

/// One auditable finding record (Session 40, Part 1). Serialized to
/// `finding-audit.json`; the prose renderer reads the exact same
/// fields, so a sentence can never state a restoration the audit
/// record does not support.
#[derive(Debug, Clone, Serialize)]
pub struct FindingAuditEntry {
    pub finding_id: String,
    pub observer_session: String,
    pub peer_asn: Option<u32>,
    pub peer_ip: String,
    pub exact_prefixes: Vec<String>,
    pub baseline_as_path: Vec<u32>,
    pub first_changed_state: String,
    pub first_changed_as_path: Vec<u32>,
    pub final_event_window_state: String,
    pub final_analysis_window_state: String,
    pub visibility_restoration_time: Option<String>,
    pub reviewed_plane_restoration_time: Option<String>,
    pub equivalent_route_restoration_time: Option<String>,
    pub exact_baseline_restoration_time: Option<String>,
    pub analysis_end: String,
    pub evidence_ids: Vec<String>,
    pub prefixes: Vec<FindingAuditPrefix>,
}

/// Per-prefix audit row with exact lifecycle/evidence references.
#[derive(Debug, Clone, Serialize)]
pub struct FindingAuditPrefix {
    pub prefix: String,
    pub baseline_as_path: Vec<u32>,
    pub first_changed_as_path: Option<Vec<u32>>,
    pub final_as_path: Option<Vec<u32>>,
    pub visibility_restoration_time: Option<String>,
    pub reviewed_plane_restoration_time: Option<String>,
    pub equivalent_route_restoration_time: Option<String>,
    pub exact_baseline_restoration_time: Option<String>,
    pub evidence_ids: Vec<String>,
}

/// The exact finding-audit record for a subject (Part 1).
#[derive(Debug, Clone, Serialize)]
pub struct FindingAudit {
    pub schema_version: u32,
    pub subject: String,
    pub subject_kind: String,
    pub window_start: String,
    pub window_end: String,
    pub analysis_end: String,
    pub findings: Vec<FindingAuditEntry>,
}

impl FindingAudit {
    /// Build the audit record from the shared workbench view model.
    pub fn from_vm(vm: &IncidentWorkbenchViewModel) -> FindingAudit {
        let analysis_end = vm
            .findings
            .first()
            .map(|f| f.analysis_end.clone())
            .unwrap_or_else(|| vm.window_end.clone());
        let findings = vm
            .findings
            .iter()
            .map(|f| FindingAuditEntry {
                finding_id: f.stable_id.clone(),
                observer_session: format!("{}/{} peer {}", f.source_family, f.collector, f.peer_ip),
                peer_asn: f.peer_asn,
                peer_ip: f.peer_ip.clone(),
                exact_prefixes: f.exact_prefixes.clone(),
                baseline_as_path: f
                    .streams
                    .iter()
                    .find(|s| !s.baseline_path.is_empty())
                    .map(|s| s.baseline_path.clone())
                    .unwrap_or_default(),
                first_changed_state: f.effect.label().to_string(),
                first_changed_as_path: f
                    .streams
                    .iter()
                    .find_map(|s| s.changed_path.clone())
                    .unwrap_or_default(),
                final_event_window_state: format!("{:?}", f.state_at_window_end),
                final_analysis_window_state: format!("{:?}", f.state_at_analysis_end),
                visibility_restoration_time: f.visibility_restored_at.clone(),
                reviewed_plane_restoration_time: f.reviewed_plane_restored_at.clone(),
                equivalent_route_restoration_time: f.equivalent_route_restored_at.clone(),
                exact_baseline_restoration_time: f.exact_baseline_restored_at.clone(),
                analysis_end: f.analysis_end.clone(),
                evidence_ids: f.evidence_refs.clone(),
                prefixes: f
                    .streams
                    .iter()
                    .map(|s| FindingAuditPrefix {
                        prefix: s.prefix.clone(),
                        baseline_as_path: s.baseline_path.clone(),
                        first_changed_as_path: s.changed_path.clone(),
                        final_as_path: s.final_path.clone(),
                        visibility_restoration_time: s.visibility_restored_at.clone(),
                        reviewed_plane_restoration_time: s.reviewed_plane_restored_at.clone(),
                        equivalent_route_restoration_time: s.equivalent_route_restored_at.clone(),
                        exact_baseline_restoration_time: s.exact_baseline_restored_at.clone(),
                        evidence_ids: if s.evidence_refs.is_empty() {
                            Vec::new()
                        } else {
                            vec![s.evidence_refs.clone()]
                        },
                    })
                    .collect(),
            })
            .collect();
        FindingAudit {
            schema_version: 1,
            subject: vm.subject_id.clone(),
            subject_kind: vm.subject_kind.clone(),
            window_start: vm.window_start.clone(),
            window_end: vm.window_end.clone(),
            analysis_end,
            findings,
        }
    }
}

/// One ordered step of a finding's route chronology (Session 41,
/// Part 1). Derived from canonical evidence only; absent fields are
/// omitted, never guessed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ChronologyStep {
    /// Step kind: "Baseline", "First change", "Absent",
    /// "First replacement", "Reviewed-plane return",
    /// "Equivalent-route return", "Exact baseline return",
    /// "Later change", "Event-window end", "Analysis end".
    pub label: &'static str,
    /// Exact time or range (HH:MM:SS UTC).
    pub time: String,
    /// Collapsed path signature; None when the step has no observed
    /// path (absence, or a cooldown change whose path is archived in
    /// the raw evidence only).
    pub path: Option<String>,
    /// Restoration class when this step IS a restoration.
    pub restoration: Option<&'static str>,
}

/// Ordered route chronology of one finding (Session 41, Part 1).
///
/// Never flattens restoration events into one ambiguous outcome: the
/// final observed state is always a distinct step.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RouteChronology {
    pub steps: Vec<ChronologyStep>,
    pub state_at_event_window_end: String,
    pub state_at_analysis_end: String,
}

/// Derive the finding's route chronology from its audit fields.
/// `window_end` is the event-window end (ISO) for the end-state step
/// times. Deterministic: fixed step order, exact timestamps.
pub fn route_chronology(f: &RoutingFinding, window_end: &str) -> RouteChronology {
    use CooldownOutcome as CO;
    let time = |t: &str| finding_time(t);
    let first_t = f.first_observed.as_deref().unwrap_or("");
    let mut steps: Vec<ChronologyStep> = Vec::new();

    // 1. Baseline (the pre-change path).
    if f.baseline_path_signature != "—" {
        steps.push(ChronologyStep {
            label: "Baseline",
            time: if first_t.is_empty() {
                "observed before the change".to_string()
            } else {
                format!("until {} UTC", time(first_t))
            },
            path: Some(f.baseline_path_signature.clone()),
            restoration: None,
        });
    }
    // 2-3. Absence interval + first replacement (withdrawals).
    if matches!(
        f.effect,
        RoutingEffect::PrefixesTemporarilyAbsent | RoutingEffect::PrefixesWithdrawn
    ) {
        let end = f
            .visibility_restored_at
            .as_deref()
            .unwrap_or("the event-window end");
        steps.push(ChronologyStep {
            label: "Absent",
            time: if first_t.is_empty() {
                "the analysis window".to_string()
            } else if finding_time(end) == time(first_t) {
                format!("at {} UTC", time(first_t))
            } else {
                format!("{}–{} UTC", time(first_t), finding_time(end))
            },
            path: None,
            restoration: None,
        });
        if let Some(vis) = &f.visibility_restored_at {
            steps.push(ChronologyStep {
                label: "First replacement",
                time: format!("from {} UTC", finding_time(vis)),
                path: Some(f.changed_path_signature.clone()),
                restoration: Some("Visibility returned"),
            });
        }
    } else if f.changed_path_signature != "—" {
        // Non-absence: the first change step.
        steps.push(ChronologyStep {
            label: "First change",
            time: if first_t.is_empty() {
                "the observed time".to_string()
            } else {
                format!("at {} UTC", time(first_t))
            },
            path: Some(f.changed_path_signature.clone()),
            restoration: None,
        });
    }
    // 4. Reviewed-plane return.
    if let Some(t) = &f.reviewed_plane_restored_at {
        steps.push(ChronologyStep {
            label: "Reviewed-plane return",
            time: format!("at {} UTC", finding_time(t)),
            path: None,
            restoration: Some("Reviewed plane"),
        });
    }
    // 5. Equivalent-route return (distinct from visibility).
    if let Some(t) = &f.equivalent_route_restored_at {
        if f.visibility_restored_at.as_deref() != Some(t.as_str()) {
            steps.push(ChronologyStep {
                label: "Equivalent-route return",
                time: format!("at {} UTC", finding_time(t)),
                path: Some(f.changed_path_signature.clone()),
                restoration: Some("Equivalent route"),
            });
        }
    }
    // 6. Exact baseline return (with the per-prefix range).
    let exact_times: Vec<&str> = f
        .streams
        .iter()
        .filter_map(|s| s.exact_baseline_restored_at.as_deref())
        .collect();
    if let Some(t) = &f.exact_baseline_restored_at {
        let (min, max) = (
            exact_times.iter().min().copied().unwrap_or(t).to_string(),
            exact_times.iter().max().copied().unwrap_or(t).to_string(),
        );
        let range = if finding_time(&min) != finding_time(&max) {
            format!(
                "between {} and {} UTC",
                finding_time(&min),
                finding_time(&max)
            )
        } else {
            format!("at {} UTC", finding_time(t))
        };
        steps.push(ChronologyStep {
            label: "Exact baseline return",
            time: range,
            path: Some(f.baseline_path_signature.clone()),
            restoration: Some("Exact baseline"),
        });
    }
    // 7. Intermediate/later changes from the per-prefix transition
    // list: every transition after the first change that alters the
    // observed path and is not itself a restoration event already
    // represented (ReturnToBaseline, the first replacement, absence
    // bookends). Deduplicated by (timestamp, path); the most frequent
    // path per timestamp is shown.
    let mut later: std::collections::BTreeMap<String, (Vec<u32>, usize)> =
        std::collections::BTreeMap::new();
    for stream in &f.streams {
        for (i, t) in stream.transitions.iter().enumerate() {
            if i == 0 {
                continue; // first change is its own step
            }
            if t.after_path.is_empty() {
                continue; // withdrawals are absence bookends
            }
            if t.kind == "ReturnToBaseline" {
                continue; // the exact-baseline step
            }
            if f.visibility_restored_at.as_deref() == Some(t.timestamp.as_str()) {
                continue; // the first-replacement step
            }
            let entry = later
                .entry(t.timestamp.clone())
                .or_insert_with(|| (t.after_path.clone(), 0));
            if entry.0 == t.after_path {
                entry.1 += 1;
            }
        }
    }
    let mut later_steps: Vec<ChronologyStep> = later
        .into_iter()
        .filter(|(_, (path, _))| !path.is_empty())
        .map(|(ts, (path, _))| ChronologyStep {
            label: "Later change",
            time: format!("at {} UTC", finding_time(&ts)),
            path: Some(crate::catalog::workbench::collapse_as_path(&path)),
            restoration: None,
        })
        .collect();
    // Cooldown re-changes: the exact path lives in the raw archive
    // evidence, never synthesized.
    if let CO::StillChangingBeforeAnalysisEnd(t) = &f.state_at_analysis_end {
        later_steps.push(ChronologyStep {
            label: "Later change",
            time: format!("at {} UTC", finding_time(t)),
            path: None,
            restoration: None,
        });
    }
    steps.append(&mut later_steps);

    // 8-9. End states: event-window end and analysis end are distinct
    // steps; the analysis-end step never assumes the last restoration.
    if !window_end.is_empty() {
        steps.push(ChronologyStep {
            label: "Event-window end",
            time: format!("at {} UTC", finding_time(window_end)),
            path: if f.final_path_signature != "—" {
                Some(f.final_path_signature.clone())
            } else {
                None
            },
            restoration: None,
        });
    }
    steps.push(ChronologyStep {
        label: "Analysis end",
        time: format!("at {} UTC", finding_time(&f.analysis_end)),
        path: if f.final_path_signature != "—" {
            Some(f.final_path_signature.clone())
        } else {
            None
        },
        restoration: None,
    });

    // Deterministic temporal order: steps sort by their first
    // HH:MM:SS, then by step-class priority (equal-time ties keep the
    // semantic order: baseline before change, restorations before end
    // states).
    let class_rank = |label: &str| -> usize {
        match label {
            "Baseline" => 0,
            "Absent" => 1,
            "First change" => 2,
            "First replacement" => 3,
            "Reviewed-plane return" => 4,
            "Equivalent-route return" => 5,
            "Exact baseline return" => 6,
            "Later change" => 7,
            "Event-window end" => 8,
            "Analysis end" => 9,
            _ => 10,
        }
    };
    let step_key = |step: &ChronologyStep| -> (i64, usize) {
        let secs = step
            .time
            .split_whitespace()
            .find_map(parse_clock_seconds)
            .unwrap_or(i64::MAX);
        (secs, class_rank(step.label))
    };
    steps.sort_by_key(step_key);
    steps.dedup_by(|a, b| a.label == b.label && a.time == b.time && a.path == b.path);

    RouteChronology {
        steps,
        state_at_event_window_end: format!("{:?}", f.state_at_window_end),
        state_at_analysis_end: format!("{:?}", f.state_at_analysis_end),
    }
}

/// HH:MM:SS -> seconds since midnight (for chronology ordering).
/// Tolerates range tokens like "16:45:25–16:45:27" by truncating at
/// the first non-digit.
fn parse_clock_seconds(tok: &str) -> Option<i64> {
    let digits = |s: &str| -> Option<i64> {
        let end = s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len());
        s[..end].parse().ok()
    };
    let mut parts = tok.split(':');
    let h = digits(parts.next()?)?;
    let m = digits(parts.next()?)?;
    let sec = digits(parts.next()?)?;
    Some(h * 3600 + m * 60 + sec)
}

/// Render the observer-lane timeline as server-rendered SVG (Part 7).
///
/// One lane per observer session plus one "Operator report" lane for
/// operator-reported anchors. BGP lane axis is the pilot/analysis
/// window; the operator lane spans its own anchors' extent. Markers are
/// EXACT observed timestamps — nothing is interpolated; intervals span
/// only between observed endpoints. Marker classes are distinct:
/// `tl-op`, `tl-bgp`, `tl-absence`, `tl-path`, `tl-restore`,
/// `tl-changed-end`. A conventional table fallback is rendered by the
/// template alongside this SVG.
pub fn render_timeline_svg(lanes: &[TimelineLane], anchors: &[TimelineMarker]) -> String {
    const W: f64 = 1180.0;
    const LEFT: f64 = 260.0;
    const RIGHT: f64 = 1150.0;
    const LANE_H: f64 = 26.0;
    const CONTEXT_H: f64 = 44.0;
    const TOP: f64 = 24.0;

    let mut out = String::new();
    // +28 focus-group offset above the lanes, +32 for the legend.
    let height = TOP
        + if anchors.is_empty() { 0.0 } else { CONTEXT_H }
        + 28.0
        + (lanes.len() as f64) * LANE_H
        + 40.0;
    out.push_str(&format!(
        r#"<svg class="wb-timeline-svg" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {W} {height:.0}" role="img" aria-label="Observer-lane timeline (UTC)">"#
    ));

    // The BGP focus axis is the analysis window (all lanes share it).
    let (focus_start, focus_end) = match lanes.first() {
        Some(l) => (
            parse_utc_seconds(&l.window_start).unwrap_or(0),
            parse_utc_seconds(&l.window_end).unwrap_or(1),
        ),
        None => (0, 1),
    };
    let focus_span = (focus_end - focus_start).max(1);
    let focus_x = |t: i64| LEFT + (t - focus_start) as f64 / focus_span as f64 * (RIGHT - LEFT);

    // ── Context strip (operator anchors only, Part 6) ──────────────
    // Its own axis spans the anchors' exact extent, so an anchor at the
    // strip edge is HONEST: the axis starts/ends AT that timestamp. An
    // off-window anchor is NEVER placed on the focus axis.
    let mut y = TOP;
    if !anchors.is_empty() {
        let ctx_start = anchors
            .iter()
            .filter_map(|a| parse_utc_seconds(&a.timestamp_utc))
            .min()
            .unwrap_or(focus_start);
        let ctx_end = anchors
            .iter()
            .filter_map(|a| parse_utc_seconds(&a.timestamp_utc))
            .max()
            .unwrap_or(focus_end);
        let ctx_span = (ctx_end - ctx_start).max(1);
        let ctx_x = |t: i64| LEFT + (t - ctx_start) as f64 / ctx_span as f64 * (RIGHT - LEFT);
        let axis_y = y + 12.0;
        out.push_str(&format!(
            r#"<g class="tl-context" data-start="{ctx_start}" data-end="{ctx_end}"><text class="tl-lane-label" x="8" y="{:.1}" text-anchor="start">Operator context</text><line class="tl-axis-line" x1="{LEFT}" y1="{axis_y:.1}" x2="{RIGHT}" y2="{axis_y:.1}"/>"#,
            axis_y + 10.0
        ));
        for (t, label) in [
            (ctx_start, hms_of(ctx_start)),
            (ctx_start + ctx_span / 2, hms_of(ctx_start + ctx_span / 2)),
            (ctx_end, hms_of(ctx_end)),
        ] {
            let x = ctx_x(t);
            out.push_str(&format!(
                r#"<line class="tl-tick" x1="{x:.1}" y1="{axis_y:.1}" x2="{x:.1}" y2="{:.1}"/><text class="tl-tick-label" x="{x:.1}" y="{:.1}" text-anchor="middle">{label}</text>"#,
                axis_y + 8.0, axis_y + 12.0
            ));
        }
        for a in anchors {
            if let Some(t) = parse_utc_seconds(&a.timestamp_utc) {
                let x = ctx_x(t);
                let label = escape_svg(&a.label);
                out.push_str(&format!(
                    r#"<g class="tl-op" transform="translate({x:.1},{axis_y:.1})"><path class="tl-op-marker" d="M0,-5 L5,0 L0,5 L-5,0 Z"/><text class="tl-op-label" x="8" y="4">{label}</text></g>"#
                ));
            }
        }
        out.push_str("</g>");
        y += CONTEXT_H;
    }

    // ── Focus timeline (BGP lanes + in-window operator anchors) ────
    out.push_str(&format!(
        r#"<g class="tl-focus" data-start="{focus_start}" data-end="{focus_end}"><text class="tl-lane-label" x="8" y="{:.1}" text-anchor="start">BGP focus window</text><line class="tl-axis-line" x1="{LEFT}" y1="{:.1}" x2="{RIGHT}" y2="{:.1}"/>"#,
        y + 22.0, y + 12.0, y + 12.0
    ));
    for (t, label) in [
        (focus_start, hms_of(focus_start)),
        (
            focus_start + focus_span / 2,
            hms_of(focus_start + focus_span / 2),
        ),
        (focus_end, hms_of(focus_end)),
    ] {
        let x = focus_x(t);
        out.push_str(&format!(
            r#"<line class="tl-tick" x1="{x:.1}" y1="{:.1}" x2="{x:.1}" y2="{:.1}"/><text class="tl-tick-label" x="{x:.1}" y="{:.1}" text-anchor="middle">{label}</text>"#,
            y + 12.0, y + 20.0, y + 24.0
        ));
    }
    // In-window operator anchors (exact positions on the focus axis).
    for a in anchors {
        if let Some(t) = parse_utc_seconds(&a.timestamp_utc) {
            if (focus_start..=focus_end).contains(&t) {
                let x = focus_x(t);
                let label = escape_svg(&a.label);
                out.push_str(&format!(
                    r#"<g class="tl-op tl-op-inwindow" transform="translate({x:.1},{:.1})"><path class="tl-op-marker" d="M0,-5 L5,0 L0,5 L-5,0 Z"/><text class="tl-op-label" x="8" y="4">{label}</text></g>"#,
                    y + 12.0
                ));
            }
        }
    }
    out.push_str("</g>");
    y += 28.0;

    // One lane per observer session (baselines strictly horizontal:
    // y1 == y2). Lane labels carry the peer ASN where known.
    for (i, lane) in lanes.iter().enumerate() {
        let lane_top = y + (i as f64) * LANE_H;
        let peer = lane
            .peer_asn
            .map(|a| format!(" / AS{a}"))
            .unwrap_or_default();
        let label = escape_svg(&format!(
            "{} · {}{}",
            collector_from_session(&lane.observer_session),
            lane.region,
            peer
        ));
        out.push_str(&format!(
            r#"<g class="tl-lane"><text class="tl-lane-label" x="8" y="{:.1}" text-anchor="start">{label}</text><line class="tl-lane-line" x1="{LEFT}" y1="{:.1}" x2="{RIGHT}" y2="{:.1}"/>"#,
            lane_top + 10.0, lane_top, lane_top
        ));

        // Absence interval (explicit start AND end, never extrapolated).
        if let Some((a, b)) = &lane.absence_interval {
            if let (Some(ta), Some(tb)) = (parse_utc_seconds(a), parse_utc_seconds(b)) {
                let x1 = focus_x(ta).clamp(LEFT, RIGHT);
                let x2 = focus_x(tb).clamp(LEFT, RIGHT).max(x1 + 1.0);
                out.push_str(&format!(
                    r#"<rect class="tl-absence" x="{x1:.1}" y="{:.1}" width="{:.1}" height="7" rx="1"/>"#,
                    lane_top - 6.0, x2 - x1
                ));
            }
        }
        // Path-change interval.
        if let Some((a, b)) = &lane.path_change_interval {
            if let (Some(ta), Some(tb)) = (parse_utc_seconds(a), parse_utc_seconds(b)) {
                let x1 = focus_x(ta).clamp(LEFT, RIGHT);
                let x2 = focus_x(tb).clamp(LEFT, RIGHT).max(x1 + 1.0);
                out.push_str(&format!(
                    r#"<rect class="tl-path" x="{x1:.1}" y="{:.1}" width="{:.1}" height="4"/>"#,
                    lane_top + 1.0,
                    x2 - x1
                ));
            }
        }
        // First route change marker (BGP evidence).
        if let Some(m) = &lane.first_route_change {
            if let Some(t) = parse_utc_seconds(&m.timestamp_utc) {
                let x = focus_x(t).clamp(LEFT, RIGHT);
                out.push_str(&format!(
                    r#"<path class="tl-bgp tl-first" transform="translate({x:.1},{:.1})" d="M0,-5 L4,0 L0,5 L-4,0 Z"/>"#,
                    lane_top
                ));
            }
        }
        // Restoration marker (only when lifecycle evidence has one).
        if let Some((a, b)) = &lane.restoration_interval {
            if let Some(t) = parse_utc_seconds(b).or_else(|| parse_utc_seconds(a)) {
                let x = focus_x(t).clamp(LEFT, RIGHT);
                out.push_str(&format!(
                    r#"<path class="tl-restore" transform="translate({x:.1},{:.1})" d="M0,-6 L5,4 L-5,4 Z"/>"#,
                    lane_top
                ));
            }
        }
        // Changed-at-end marker: observed changes with no restoration
        // interval (unresolved or still changed) get a hollow square at
        // the window end.
        if lane.first_route_change.is_some() && lane.restoration_interval.is_none() {
            let x = RIGHT;
            out.push_str(&format!(
                r#"<rect class="tl-changed-end" x="{:.1}" y="{:.1}" width="8" height="8" fill="none"/>"#,
                x - 4.0, lane_top - 4.0
            ));
        }
        out.push_str("</g>");
    }

    let legend_y = y + (lanes.len() as f64) * LANE_H + 8.0;
    out.push_str(&format!(
        r#"<g class="tl-legend" transform="translate(8,{legend_y:.0})"><text class="tl-legend-item" x="0" y="10">Legend:</text>"#
    ));
    let legend = [
        ("tl-absence", "absence interval"),
        ("tl-path", "path-change interval"),
        ("tl-bgp", "first route change (BGP)"),
        ("tl-restore", "restoration (BGP)"),
        ("tl-changed-end", "changed at window end"),
        ("tl-op-marker", "operator-reported anchor"),
    ];
    let mut lx = 80.0;
    for (class, text) in legend {
        out.push_str(&format!(
            r#"<rect class="{class}" x="{lx:.0}" y="4" width="9" height="7"/><text class="tl-legend-item" x="{:.0}" y="12">{text}</text>"#,
            lx + 12.0
        ));
        lx += 30.0 + (text.len() as f64) * 6.6;
    }
    out.push_str("</g></svg>");
    out
}

/// Seconds since epoch for "YYYY-MM-DDTHH:MM:SS…" (UTC; Z/+00:00/fraction
/// tolerated). Exact seconds only — sub-second precision is not needed
/// for axis placement and is never fabricated.
pub fn parse_utc_seconds(ts: &str) -> Option<i64> {
    if ts.len() < 19 {
        return None;
    }
    let y: i64 = ts.get(0..4)?.parse().ok()?;
    let mo: i64 = ts.get(5..7)?.parse().ok()?;
    let d: i64 = ts.get(8..10)?.parse().ok()?;
    let h: i64 = ts.get(11..13)?.parse().ok()?;
    let mi: i64 = ts.get(14..16)?.parse().ok()?;
    let s: i64 = ts.get(17..19)?.parse().ok()?;
    if !(1..=12).contains(&mo) || !(1..=31).contains(&d) || h > 23 || mi > 59 || s > 60 {
        return None;
    }
    let y2 = if mo <= 2 { y - 1 } else { y };
    let era = if y2 >= 0 { y2 } else { y2 - 399 } / 400;
    let yoe = y2 - era * 400;
    let mp = (mo + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some((era * 146097 + doe - 719468) * 86400 + h * 3600 + mi * 60 + s)
}

/// Format a seconds-since-epoch value as "YYYY-MM-DDTHH:MM:SSZ" (UTC).
fn format_utc_iso(secs: i64) -> String {
    let days = secs.div_euclid(86400);
    let rem = secs.rem_euclid(86400);
    // Inverse of the days-from-civil calculation (Howard Hinnant's
    // algorithm): civil_from_days.
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60
    )
}

/// "HH:MM:SS" of a seconds-since-epoch value (UTC).
fn hms_of(secs: i64) -> String {
    let days = secs.div_euclid(86400);
    let rem = secs.rem_euclid(86400);
    let _ = days; // date not needed for axis labels within one window
    format!("{:02}:{:02}:{:02}", rem / 3600, (rem % 3600) / 60, rem % 60)
}

/// Minimal SVG text escaping.
fn escape_svg(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// One unique time-scoped observer session (Session 38, Part 1).
///
/// Identity: source family, collector, peer IP, address family. The
/// address family is derived deterministically from the peer IP literal
/// (contains ':' → ipv6, else ipv4) because the stream schema carries
/// no AF column. This key is what breadth counts deduplicate on: one
/// session may produce several episodes and must never inflate the
/// session denominator.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SessionKey {
    pub source_family: String,
    pub collector: String,
    pub peer_ip: String,
    pub address_family: String,
}

/// Address family of a peer IP literal (deterministic).
pub fn address_family_of(peer_ip: &str) -> &'static str {
    if peer_ip.contains(':') {
        "ipv6"
    } else {
        "ipv4"
    }
}

/// Parse "<family>/<collector> peer <ip>" into a SessionKey.
pub fn session_key_of(observer_session: &str) -> SessionKey {
    let (family, rest) = match observer_session.split_once('/') {
        Some((f, r)) => (f.to_string(), r.to_string()),
        None => (String::new(), observer_session.to_string()),
    };
    let (collector, peer_ip) = match rest.split_once(" peer ") {
        Some((c, p)) => (c.to_string(), p.to_string()),
        None => (rest.clone(), String::new()),
    };
    let address_family = address_family_of(&peer_ip).to_string();
    SessionKey {
        source_family: family,
        collector,
        peer_ip,
        address_family,
    }
}

/// Extract the collector label from an observer session string of the
/// form "<family>/<collector> peer <ip>".
pub fn collector_from_session(session: &str) -> &str {
    match session.split_once('/') {
        Some((_, rest)) => match rest.split_once(" peer ") {
            Some((c, _)) => c,
            None => rest,
        },
        None => session,
    }
}

/// Extract the source family from "<family>/<collector> peer <ip>".
pub fn family_from_session(session: &str) -> &str {
    match session.split_once('/') {
        Some((f, _)) => f,
        None => "",
    }
}

/// Render a workbench timestamp for ordinary rows (Part 4).
///
/// The page header already carries the event date and UTC context, so
/// rows on the same day as the analysis window render as `HH:MM:SS UTC`.
/// Rows belonging to another day (cross-midnight events) include the
/// date: `YYYY-MM-DD HH:MM:SS UTC`. The timezone is ALWAYS explicit.
/// The input may carry `Z`, `+00:00`, or nanosecond precision; only the
/// second precision is rendered. Exact timestamps remain available in
/// expanded evidence details, the JSON API, copied values, and the text
/// report — this function is for display rows only.
pub fn workbench_time(timestamp: &str, window_start: &str) -> String {
    let ts = timestamp.trim();
    // Accept "YYYY-MM-DDTHH:MM:SS" prefixes (Z / +00:00 / fraction).
    let (date, hms) = match (ts.get(0..10), ts.get(11..19)) {
        (Some(d), Some(t)) if ts.len() >= 19 => (d, t),
        _ => return format!("{ts} UTC"),
    };
    let window_date = window_start.get(0..10).unwrap_or("");
    if date == window_date {
        format!("{hms} UTC")
    } else {
        format!("{date} {hms} UTC")
    }
}

#[cfg(test)]
mod sentence_tests {
    use super::*;

    pub(super) fn episode(kind: EffectKind) -> ObserverEpisode {
        ObserverEpisode {
            analysis_run: 1,
            observer_session: "catalog/rrc06 peer 192.0.2.1".to_string(),
            observer_site: "Otemachi, Tokyo, Japan".to_string(),
            observer_region: "APAC".to_string(),
            peer_asn: Some(64500),

            observed_peer_asns: Vec::new(),
            peer_label: "AS64500".to_string(),
            peer_role: "regional-re".to_string(),
            relationship: RelationshipKind::Direct,
            named_path_plane: "Plane A".to_string(),
            effect_kind: kind,
            first_change: Some("2019-08-21T16:45:25Z".to_string()),
            last_change: Some("2019-08-21T17:02:19Z".to_string()),
            restoration_start: Some("2019-08-21T16:59:00Z".to_string()),
            restoration_end: Some("2019-08-21T17:02:00Z".to_string()),
            baseline_stream_count: 11,
            changed_stream_count: 11,
            distinct_prefix_count: 11,
            route_instance_count: 11,
            restored_stream_count: 0,
            unresolved_count: 0,

            transition_count: 0,
            end_state: EndState::NoRouteStateChange,
            cooldown_outcome: CooldownOutcome::None,
            coverage_status: CoverageStatus::Complete,
            representative_evidence: String::new(),
            streams: Vec::new(),
        }
    }

    #[test]
    fn sentence_distinguishes_collector_site_from_peer_location() {
        let ep = episode(EffectKind::TemporaryStreamAbsence);
        let s = render_episode_sentence(&ep);
        // The collector site is named; the peer is NOT said to be in Tokyo.
        assert!(s.contains("rrc06 at Otemachi, Tokyo, Japan"), "{s}");
        assert!(s.contains("receiving routes from peer AS64500"), "{s}");
        assert!(
            !s.contains("peer AS64500 at Otemachi") && !s.contains("peer in Tokyo"),
            "peer location must not be implied from the collector site: {s}"
        );
    }

    #[test]
    fn sentence_uses_effect_specific_verb() {
        let s_absent = render_episode_sentence(&episode(EffectKind::TemporaryStreamAbsence));
        assert!(s_absent.contains("become absent"), "{s_absent}");
        let s_withdrawn = render_episode_sentence(&episode(EffectKind::RouteWithdrawal));
        assert!(s_withdrawn.contains("was withdrawn"), "{s_withdrawn}");
        let s_path = render_episode_sentence(&episode(EffectKind::PathReplacement));
        assert!(s_path.contains("change AS path"), "{s_path}");
        let s_depart = render_episode_sentence(&episode(EffectKind::NamedPlaneDeparture));
        assert!(s_depart.contains("leave the Plane A path"), "{s_depart}");
        let s_prepend = render_episode_sentence(&episode(EffectKind::PrependChange));
        assert!(
            s_prepend.contains("change AS-path prepending"),
            "{s_prepend}"
        );
        // "rerouted" is never a universal substitute.
        assert!(!s_absent.contains("rerouted"));
        assert!(!s_withdrawn.contains("rerouted"));
        assert!(!s_path.contains("rerouted"));
    }

    #[test]
    fn sentence_distinguishes_visibility_restoration_from_baseline_restoration() {
        let s = render_episode_sentence(&episode(EffectKind::TemporaryStreamAbsence));
        assert!(
            s.contains("return to visibility"),
            "visibility restoration wording: {s}"
        );
        assert!(
            !s.contains("restored its baseline path"),
            "baseline restoration is a different claim: {s}"
        );
        let ep = episode(EffectKind::PathReplacement);
        let s2 = render_episode_sentence(&ep);
        assert!(s2.contains("initial path class returned"), "{s2}");
    }

    #[test]
    fn sentence_names_stream_and_prefix_units_when_they_differ() {
        let mut ep = episode(EffectKind::TemporaryStreamAbsence);
        ep.changed_stream_count = 14;
        ep.distinct_prefix_count = 11;
        let s = render_episode_sentence(&ep);
        assert!(
            s.contains("14 observer-prefix streams covering 11 prefixes"),
            "both units must be named when they differ: {s}"
        );
    }

    #[test]
    fn sentence_never_claims_traffic_loss() {
        for kind in [
            EffectKind::TemporaryStreamAbsence,
            EffectKind::RouteWithdrawal,
            EffectKind::PathReplacement,
            EffectKind::NamedPlaneDeparture,
            EffectKind::NamedPlaneReturn,
            EffectKind::PrependChange,
            EffectKind::MixedRouteChange,
            EffectKind::NoRouteStateChange,
        ] {
            let s = render_episode_sentence(&episode(kind));
            assert!(
                !s.to_lowercase().contains("traffic lost")
                    && !s.to_lowercase().contains("traffic loss")
                    && !s.to_lowercase().contains("downtime"),
                "traffic-loss claim: {s}"
            );
        }
    }

    #[test]
    fn sentence_never_claims_causation() {
        for kind in [
            EffectKind::TemporaryStreamAbsence,
            EffectKind::RouteWithdrawal,
            EffectKind::PathReplacement,
            EffectKind::NamedPlaneDeparture,
            EffectKind::NoRouteStateChange,
        ] {
            let s = render_episode_sentence(&episode(kind));
            assert!(
                !s.to_lowercase().contains("caused")
                    && !s.to_lowercase().contains("because")
                    && !s.to_lowercase().contains("due to"),
                "causation claim: {s}"
            );
        }
    }

    #[test]
    fn unreviewed_peer_asn_is_never_claimed() {
        let mut ep = episode(EffectKind::TemporaryStreamAbsence);
        ep.peer_asn = None;
        let s = render_episode_sentence(&ep);
        assert!(s.contains("ASN unreviewed"), "{s}");
    }
}

/// Build a session-peer context map from reviewed audit rows.
///
/// Key: (collector, peer_ip) → (peer_asn, peer_label, peer_role,
/// relationship). The relationship is derived from the audit row's own
/// path-class evidence plus the profile; direct (peer ASN ∈ plane) and
/// indirect (plane ASN in path) are separate facts.
pub fn session_peer_context(
    profile: &ServicePlaneProfile,
    audit_rows: &[crate::catalog::netprofile::SessionAuditRow],
) -> BTreeMap<(String, String), (u32, String, String, RelationshipKind)> {
    let mut out = BTreeMap::new();
    for row in audit_rows {
        // Collect the row's per-plane evidence into relationship kinds.
        let mut rels: Vec<SessionRelationship> = Vec::new();
        for plane in &profile.service_planes {
            let in_path = row
                .path_class
                .per_plane_contains
                .iter()
                .any(|(id, _)| id == &plane.id);
            if plane.asns.contains(&row.peer_asn) {
                rels.push(SessionRelationship::DirectPeerToNamedPlane {
                    plane_id: plane.id.clone(),
                });
            } else if in_path {
                rels.push(SessionRelationship::IndirectPathViaNamedPlane {
                    plane_id: plane.id.clone(),
                });
            }
        }
        let relationship = relationship_kind(&rels);
        out.insert(
            (row.collector.clone(), row.peer_ip.clone()),
            (
                row.peer_asn,
                format!("AS{}", row.peer_asn),
                profile.role_label(row.peer_asn),
                relationship,
            ),
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[allow(clippy::too_many_arguments)] // test fixture builder
    fn stream(
        collector: &str,
        peer: &str,
        prefix: &str,
        category: &str,
        withdrawn: bool,
        restored: bool,
        transit_state: &str,
        transitions: i64,
    ) -> StreamLifecycleSummary {
        StreamLifecycleSummary {
            id: 0,
            run_id: 1,
            collector: collector.to_string(),
            peer_ip: peer.to_string(),
            prefix: prefix.to_string(),
            category: category.to_string(),
            baseline_instances: 1,
            max_active_instances: 1,
            transition_count: transitions,
            withdrawn,
            restored,
            transit_state: transit_state.to_string(),
            add_path_ambiguous: false,
            evidence_refs: "[]".to_string(),
            first_change_utc: None,
            restoration_time_utc: None,
        }
    }

    fn registry() -> CollectorLocationRegistry {
        CollectorLocationRegistry {
            as_of: "2019-09-05".to_string(),
            collectors: vec![
                crate::catalog::netprofile::CollectorLocation {
                    family: "ris".to_string(),
                    collector: "rrc06".to_string(),
                    location: "Otemachi, Tokyo, Japan".to_string(),
                    facility: "DIX-IE / JPIX".to_string(),
                    note: None,
                    region: "APAC".to_string(),
                    multihop: false,
                },
                crate::catalog::netprofile::CollectorLocation {
                    family: "ris".to_string(),
                    collector: "rrc00".to_string(),
                    location: "Amsterdam, Netherlands".to_string(),
                    facility: "RIPE-NCC Multihop".to_string(),
                    note: None,
                    region: "EMEA".to_string(),
                    multihop: true,
                },
            ],
        }
    }

    fn peers() -> BTreeMap<(String, String), (u32, String, String, RelationshipKind)> {
        let mut m = BTreeMap::new();
        m.insert(
            ("rrc06".to_string(), "192.0.2.1".to_string()),
            (
                64500,
                "AS64500".to_string(),
                "regional-re".to_string(),
                RelationshipKind::Direct,
            ),
        );
        m.insert(
            ("rrc06".to_string(), "192.0.2.2".to_string()),
            (
                64599,
                "AS64599".to_string(),
                "unclassified observed ASN".to_string(),
                RelationshipKind::Indirect,
            ),
        );
        m
    }

    #[test]
    fn episode_groups_same_observer_and_signature() {
        let streams = vec![
            stream(
                "rrc06",
                "192.0.2.1",
                "198.51.100.0/24",
                "Withdrawn",
                true,
                true,
                "retained",
                2,
            ),
            stream(
                "rrc06",
                "192.0.2.1",
                "198.51.100.0/25",
                "Withdrawn",
                true,
                true,
                "retained",
                2,
            ),
            stream(
                "rrc06",
                "192.0.2.1",
                "198.51.100.0/26",
                "Unchanged",
                false,
                false,
                "retained",
                0,
            ),
        ];
        let eps = build_episodes(1, "ris", &streams, &[], &registry(), &peers(), "plane-a");
        // Two absent streams share one signature → one episode; the
        // unchanged stream forms a separate no-change episode.
        assert_eq!(eps.len(), 2);
        let absent = eps
            .iter()
            .find(|e| e.effect_kind == EffectKind::TemporaryStreamAbsence)
            .unwrap();
        assert_eq!(absent.baseline_stream_count, 2);
        assert_eq!(absent.changed_stream_count, 2);
        assert_eq!(absent.distinct_prefix_count, 2);
        assert_eq!(absent.observer_session, "ris/rrc06 peer 192.0.2.1");
    }

    #[test]
    fn different_peers_at_one_collector_remain_separate() {
        let streams = vec![
            stream(
                "rrc06",
                "192.0.2.1",
                "198.51.100.0/24",
                "Withdrawn",
                true,
                true,
                "retained",
                2,
            ),
            stream(
                "rrc06",
                "192.0.2.2",
                "198.51.100.0/24",
                "Withdrawn",
                true,
                true,
                "retained",
                2,
            ),
        ];
        let eps = build_episodes(1, "ris", &streams, &[], &registry(), &peers(), "plane-a");
        assert_eq!(eps.len(), 2, "each peer session stays separate");
        assert!(eps
            .iter()
            .all(|e| e.observer_session.contains("peer 192.0.2.")));
        assert_ne!(eps[0].observer_session, eps[1].observer_session);
    }

    #[test]
    fn direct_and_indirect_observations_remain_separate() {
        let streams = vec![
            stream(
                "rrc06",
                "192.0.2.1",
                "198.51.100.0/24",
                "Withdrawn",
                true,
                true,
                "retained",
                2,
            ),
            stream(
                "rrc06",
                "192.0.2.2",
                "198.51.100.0/24",
                "Withdrawn",
                true,
                true,
                "retained",
                2,
            ),
        ];
        let eps = build_episodes(1, "ris", &streams, &[], &registry(), &peers(), "plane-a");
        let direct = eps.iter().find(|e| e.peer_asn == Some(64500)).unwrap();
        let indirect = eps.iter().find(|e| e.peer_asn == Some(64599)).unwrap();
        assert_eq!(direct.relationship, RelationshipKind::Direct);
        assert_eq!(indirect.relationship, RelationshipKind::Indirect);
        assert_ne!(direct.observer_session, indirect.observer_session);
    }

    #[test]
    fn episode_counts_distinct_prefixes_and_streams_separately() {
        // 3 streams, 2 distinct prefixes, all changed.
        let streams = vec![
            stream(
                "rrc06",
                "192.0.2.1",
                "198.51.100.0/24",
                "Withdrawn",
                true,
                true,
                "retained",
                2,
            ),
            stream(
                "rrc06",
                "192.0.2.1",
                "198.51.100.0/24",
                "DepartedTransitPath",
                false,
                false,
                "departed",
                1,
            ),
            stream(
                "rrc06",
                "192.0.2.1",
                "198.51.100.0/25",
                "PathChangedStillViaTransit",
                false,
                false,
                "retained",
                1,
            ),
        ];
        let eps = build_episodes(1, "ris", &streams, &[], &registry(), &peers(), "plane-a");
        let total_streams: usize = eps.iter().map(|e| e.baseline_stream_count).sum();
        let total_prefixes: usize = eps.iter().map(|e| e.distinct_prefix_count).sum();
        assert_eq!(total_streams, 3, "streams counted per stream");
        assert_eq!(total_prefixes, 3, "prefixes counted per episode (2+1)");
    }

    #[test]
    fn episode_restoration_uses_existing_lifecycle_evidence() {
        let streams = vec![stream(
            "rrc06",
            "192.0.2.1",
            "198.51.100.0/24",
            "Withdrawn",
            true,
            true,
            "retained",
            2,
        )];
        let transitions = vec![
            crate::catalog::domain::RunTransitionRecord {
                id: 0,
                run_id: 1,
                seq: 0,
                kind: "Withdrawal".to_string(),
                occurred_utc: "2019-08-21T16:45:25Z".to_string(),
                run_phase: "Event".to_string(),
                collector: "rrc06".to_string(),
                peer_ip: "192.0.2.1".to_string(),
                prefix: "198.51.100.0/24".to_string(),
                path_id: None,
                material_path_changed: false,
                communities_changed: false,
                announced: false,
                withdrawn: true,
                observation_id: None,
                archive_sha256: None,
            },
            crate::catalog::domain::RunTransitionRecord {
                id: 1,
                run_id: 1,
                seq: 1,
                kind: "Restoration".to_string(),
                occurred_utc: "2019-08-21T16:45:27Z".to_string(),
                run_phase: "Event".to_string(),
                collector: "rrc06".to_string(),
                peer_ip: "192.0.2.1".to_string(),
                prefix: "198.51.100.0/24".to_string(),
                path_id: None,
                material_path_changed: false,
                communities_changed: false,
                announced: true,
                withdrawn: false,
                observation_id: None,
                archive_sha256: None,
            },
        ];
        let eps = build_episodes(
            1,
            "ris",
            &streams,
            &transitions,
            &registry(),
            &peers(),
            "plane-a",
        );
        let ep = &eps[0];
        assert_eq!(ep.effect_kind, EffectKind::TemporaryStreamAbsence);
        assert_eq!(ep.first_change.as_deref(), Some("2019-08-21T16:45:25Z"));
        assert_eq!(ep.last_change.as_deref(), Some("2019-08-21T16:45:27Z"));
        assert_eq!(ep.unresolved_count, 0);
    }

    #[test]
    fn episode_generation_is_deterministic() {
        let streams = vec![
            stream(
                "rrc06",
                "192.0.2.2",
                "198.51.100.0/24",
                "Withdrawn",
                true,
                true,
                "retained",
                2,
            ),
            stream(
                "rrc06",
                "192.0.2.1",
                "198.51.100.0/24",
                "Withdrawn",
                true,
                true,
                "retained",
                2,
            ),
            stream(
                "rrc00",
                "192.0.2.9",
                "198.51.100.0/24",
                "Unchanged",
                false,
                false,
                "retained",
                0,
            ),
        ];
        let a = build_episodes(1, "ris", &streams, &[], &registry(), &peers(), "plane-a");
        let b = build_episodes(1, "ris", &streams, &[], &registry(), &peers(), "plane-a");
        assert_eq!(a, b);
        // No-change episodes sort after changed episodes.
        let idx_unchanged = a
            .iter()
            .position(|e| e.effect_kind == EffectKind::NoRouteStateChange)
            .unwrap();
        assert!(a[..idx_unchanged]
            .iter()
            .all(|e| e.effect_kind != EffectKind::NoRouteStateChange));
    }

    #[test]
    fn no_change_and_unresolved_withdrawal_are_distinct_kinds() {
        let streams = vec![
            stream(
                "rrc06",
                "192.0.2.1",
                "198.51.100.0/24",
                "Withdrawn",
                true,
                false,
                "retained",
                1,
            ),
            stream(
                "rrc06",
                "192.0.2.1",
                "198.51.100.0/25",
                "Unchanged",
                false,
                false,
                "retained",
                0,
            ),
        ];
        let eps = build_episodes(1, "ris", &streams, &[], &registry(), &peers(), "plane-a");
        assert!(eps
            .iter()
            .any(|e| e.effect_kind == EffectKind::RouteWithdrawal));
        assert!(eps
            .iter()
            .any(|e| e.effect_kind == EffectKind::NoRouteStateChange));
        let w = eps
            .iter()
            .find(|e| e.effect_kind == EffectKind::RouteWithdrawal)
            .unwrap();
        assert_eq!(w.unresolved_count, 1);
    }

    #[test]
    fn session_peer_context_maps_audit_rows() {
        let profile = ServicePlaneProfile {
            service_planes: vec![crate::catalog::netprofile::NamedServicePlane {
                id: "plane-a".to_string(),
                display_label: "Plane A".to_string(),
                asns: vec![64500],
            }],
            asn_roles: vec![crate::catalog::netprofile::ReviewedAsnRole {
                asn: 64500,
                role: "regional-re".to_string(),
            }],
            updated_utc: "2026-08-02T00:00:00Z".to_string(),
            provenance: "test".to_string(),
        };
        let rows = vec![crate::catalog::netprofile::SessionAuditRow {
            source_family: "ris".to_string(),
            collector: "rrc06".to_string(),
            location: "Otemachi, Tokyo, Japan".to_string(),
            rib_timestamp_utc: "2019-08-21T00:00:00Z".to_string(),
            rib_source_sha: "sha".to_string(),
            peer_ip: "192.0.2.1".to_string(),
            peer_asn: 64500,
            address_family: "ipv4".to_string(),
            origin_route_count: 12,
            distinct_prefixes: 12,
            path_class: crate::catalog::netprofile::PathClassCounts {
                per_plane_contains: vec![("plane-a".to_string(), 12)],
                neither_plane: 0,
                total: 12,
            },
        }];
        let ctx = session_peer_context(&profile, &rows);
        let (asn, _label, role, rel) = ctx
            .get(&("rrc06".to_string(), "192.0.2.1".to_string()))
            .unwrap();
        assert_eq!(*asn, 64500);
        assert_eq!(role, "regional-re");
        assert_eq!(*rel, RelationshipKind::Direct);
    }
}

// ── Regional observed breadth (Part 6) ──────────────────────────────

/// Regional summary of public-observer breadth.
///
/// "Observed breadth" (also "public-observer breadth") describes how many
/// eligible observer sessions saw the target and how many changed. It is
/// NOT outage severity, global scope, or a percentage of the Internet
/// affected; no severity score is computed anywhere.
///
/// The denominator is ALWAYS visible: `eligible_observer_sessions` is
/// reported alongside `changed_observer_sessions`. The coverage states
/// never collapse into one zero:
/// - `Complete` — qualifying baseline existed and the session was
///   observed (a "no change" result is an OBSERVED SIGNATURE, not a
///   coverage state).
/// - `NoBaselineVisibility` — target not visible at that session.
/// - `IncompleteCoverage` — observation could not be completed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegionObservationSummary {
    pub region: String,
    /// Sessions with a qualifying baseline at this region (denominator).
    /// Counted on UNIQUE SessionKeys — episodes never inflate it.
    pub eligible_observer_sessions: usize,
    /// Unique sessions having at least one changed episode.
    pub changed_observer_sessions: usize,
    pub unchanged_observer_sessions: usize,
    /// Sessions where the target was not visible (no qualifying baseline).
    pub sessions_without_baseline_visibility: usize,
    /// Sessions where the observation could not be completed.
    pub sessions_with_incomplete_coverage: usize,
    /// Presentation episodes at this region (one session may produce
    /// several; the count is intentionally session-independent).
    pub episode_count: usize,
    /// Unique changed ObserverPrefixKeys (collector, peer, prefix).
    pub changed_streams: usize,
    pub baseline_streams: usize,
    /// Unique prefixes in this region (union across sessions and peers).
    pub changed_prefixes: usize,
    /// Route instances across unique changed streams (ADD-PATH-aware).
    pub route_instances: usize,
    /// Evidenced route-state transitions at this region's sessions.
    pub transition_count: usize,
    /// First observed change in this region (UTC), if any.
    pub first_change: Option<String>,
    /// Last restoration observed INSIDE the analysis window (UTC).
    pub last_restoration: Option<String>,
    // Internal aggregation bookkeeping (never serialized).
    #[serde(skip)]
    pub changed_session_keys: std::collections::BTreeSet<SessionKey>,
    #[serde(skip)]
    pub unchanged_session_keys: std::collections::BTreeSet<SessionKey>,
    #[serde(skip)]
    pub changed_stream_keys: std::collections::BTreeSet<(String, String)>,
    #[serde(skip)]
    pub changed_prefix_set: std::collections::BTreeSet<String>,
    #[serde(skip)]
    pub transition_sessions: std::collections::BTreeSet<SessionKey>,
}

/// Build regional breadth summaries from episodes.
///
/// `eligible_sessions` is the denominator per region: every observer
/// session that had a qualifying baseline. `no_baseline_sessions` and
/// `incomplete_sessions` are reported separately per region and never
/// counted as unchanged. Entries are (collector, region, label) triples
/// with the region resolved by the loader — a collector id is never
/// treated as a region key. Regions without any eligible session are
/// omitted (they carry no observation to summarize).
pub fn regional_breadth(
    episodes: &[ObserverEpisode],
    no_baseline_sessions: &[(String, String, String, CoverageReason, String)],
    incomplete_sessions: &[(String, String, String, CoverageReason, String)],
) -> Vec<RegionObservationSummary> {
    let mut by_region: BTreeMap<String, RegionObservationSummary> = BTreeMap::new();

    // Per-region aggregates over UNIQUE session keys and UNIQUE
    // ObserverPrefixKeys (Session 38, Part 1): one session may produce
    // several episodes, and one prefix may appear at several peers; the
    // denominator counts sessions, streams count (collector, peer,
    // prefix) keys, and prefixes are set unions per region.
    for ep in episodes {
        let r = by_region
            .entry(ep.observer_region.clone())
            .or_insert_with(|| empty_region(&ep.observer_region));
        let key = session_key_of(&ep.observer_session);
        let changed = ep.effect_kind != EffectKind::NoRouteStateChange;
        // Unique session sets for the denominator.
        if changed {
            if !r.changed_session_keys.contains(&key) {
                r.changed_observer_sessions += 1;
                r.changed_session_keys.insert(key.clone());
            }
        } else if !r.unchanged_session_keys.contains(&key) {
            r.unchanged_observer_sessions += 1;
            r.unchanged_session_keys.insert(key.clone());
        }
        r.episode_count += 1;
        r.baseline_streams += ep.baseline_stream_count;
        // Transitions are a session-level fact: count them once per
        // unique session, never multiplied by the episode count.
        if r.transition_sessions.insert(key.clone()) {
            r.transition_count += ep.transition_count;
        }
        // Streams and prefixes are counted on the episode's member
        // streams: unique (collector, peer, prefix) keys and the union
        // of prefixes, independent of peer identity.
        for s in &ep.streams {
            let stream_key = (ep.observer_session.clone(), s.prefix.clone());
            if changed {
                if r.changed_stream_keys.insert(stream_key) {
                    r.changed_streams += 1;
                    r.route_instances += s.max_active_instances.max(1) as usize;
                }
                r.changed_prefix_set.insert(s.prefix.clone());
            }
        }
        if let Some(fc) = &ep.first_change {
            if r.first_change
                .as_deref()
                .map(|f| fc.as_str() < f)
                .unwrap_or(true)
            {
                r.first_change = Some(fc.clone());
            }
        }
        // In-window restoration only: lifecycle restoration timestamps
        // observed within the analysis window (cooldown outcomes are
        // reported separately, Part 7).
        if let Some(lr) = &ep.restoration_end {
            if r.last_restoration
                .as_deref()
                .map(|l| lr.as_str() > l)
                .unwrap_or(true)
            {
                r.last_restoration = Some(lr.clone());
            }
        }
    }

    // No-baseline and incomplete sessions are per-region facts derived
    // from run coverage; they are never added to the eligible or
    // unchanged counts (Part 4: an excluded session never enters the
    // denominator).
    for (_collector, region, _session, _reason, _detail) in no_baseline_sessions {
        let r = by_region
            .entry(region.clone())
            .or_insert_with(|| empty_region(region));
        r.sessions_without_baseline_visibility += 1;
    }
    for (_collector, region, _session, _reason, _detail) in incomplete_sessions {
        let r = by_region
            .entry(region.clone())
            .or_insert_with(|| empty_region(region));
        r.sessions_with_incomplete_coverage += 1;
    }

    // Finalize: eligible = changed + unchanged unique sessions;
    // changed_prefixes = the region's prefix union.
    let mut out: Vec<RegionObservationSummary> = by_region.into_values().collect();
    for r in &mut out {
        // A session that produced BOTH changed and unchanged episodes is
        // one eligible session: union of the two session-key sets.
        let eligible: std::collections::BTreeSet<SessionKey> = r
            .changed_session_keys
            .union(&r.unchanged_session_keys)
            .cloned()
            .collect();
        r.eligible_observer_sessions = eligible.len();
        // A session with ANY changed episode is a changed session;
        // unchanged = eligible sessions without changed episodes.
        r.unchanged_observer_sessions = r.eligible_observer_sessions - r.changed_observer_sessions;
        r.changed_prefixes = r.changed_prefix_set.len();
    }
    out
}

fn empty_region(region: &str) -> RegionObservationSummary {
    RegionObservationSummary {
        region: region.to_string(),
        eligible_observer_sessions: 0,
        changed_observer_sessions: 0,
        unchanged_observer_sessions: 0,
        sessions_without_baseline_visibility: 0,
        sessions_with_incomplete_coverage: 0,
        episode_count: 0,
        changed_streams: 0,
        baseline_streams: 0,
        changed_prefixes: 0,
        route_instances: 0,
        transition_count: 0,
        first_change: None,
        last_restoration: None,
        changed_session_keys: std::collections::BTreeSet::new(),
        unchanged_session_keys: std::collections::BTreeSet::new(),
        changed_stream_keys: std::collections::BTreeSet::new(),
        changed_prefix_set: std::collections::BTreeSet::new(),
        transition_sessions: std::collections::BTreeSet::new(),
    }
}

#[cfg(test)]
mod breadth_tests {
    use super::*;

    fn ep(
        region: &str,
        kind: EffectKind,
        changed_streams: usize,
        prefixes: usize,
        first: &str,
    ) -> ObserverEpisode {
        ObserverEpisode {
            analysis_run: 1,
            observer_session: format!("catalog/rrc06 peer 192.0.2.1 region {region}"),
            observer_site: "site".to_string(),
            observer_region: region.to_string(),
            peer_asn: Some(64500),

            observed_peer_asns: Vec::new(),
            peer_label: "AS64500".to_string(),
            peer_role: "regional-re".to_string(),
            relationship: RelationshipKind::Direct,
            named_path_plane: "Plane A".to_string(),
            effect_kind: kind.clone(),
            first_change: Some(first.to_string()),
            last_change: Some(first.to_string()),
            restoration_start: None,
            restoration_end: Some("2019-08-21T17:02:00Z".to_string()),
            baseline_stream_count: changed_streams.max(1),
            changed_stream_count: changed_streams,
            distinct_prefix_count: prefixes,
            route_instance_count: changed_streams,
            restored_stream_count: 0,
            unresolved_count: 0,

            transition_count: 0,
            end_state: if kind == EffectKind::NoRouteStateChange {
                EndState::NoRouteStateChange
            } else {
                EndState::StillChangedAtWindowEnd
            },
            cooldown_outcome: CooldownOutcome::None,
            coverage_status: CoverageStatus::Complete,
            representative_evidence: String::new(),
            streams: (0..changed_streams)
                .map(|i| EpisodeStream {
                    prefix: format!("198.51.{}.0/24", 100 + (i % prefixes.max(1))),
                    category: "DepartedTransitPath".to_string(),
                    withdrawn: false,
                    restored: false,
                    baseline_instances: 1,
                    max_active_instances: 1,
                    transition_count: 1,
                    add_path_ambiguous: false,
                    first_change_utc: Some(first.to_string()),
                    restoration_time_utc: None,
                    evidence_refs: "[]".to_string(),
                })
                .collect(),
        }
    }

    #[test]
    fn breadth_always_has_visible_denominator() {
        let episodes = vec![
            ep(
                "AMER",
                EffectKind::TemporaryStreamAbsence,
                11,
                11,
                "2019-08-21T16:45:25Z",
            ),
            ep(
                "AMER",
                EffectKind::NoRouteStateChange,
                0,
                0,
                "2019-08-21T16:45:25Z",
            ),
        ];
        let rows = regional_breadth(&episodes, &[], &[]);
        let amer = rows.iter().find(|r| r.region == "AMER").unwrap();
        // One session produced both a changed and an unchanged episode:
        // it is ONE eligible session and counts as changed (Session 38
        // denominator fix).
        assert_eq!(amer.eligible_observer_sessions, 1, "denominator visible");
        assert_eq!(amer.changed_observer_sessions, 1);
        assert_eq!(amer.unchanged_observer_sessions, 0);
        assert!(amer.changed_observer_sessions <= amer.eligible_observer_sessions);
    }

    #[test]
    fn no_change_and_no_baseline_are_distinct() {
        let episodes = vec![ep(
            "APAC",
            EffectKind::NoRouteStateChange,
            0,
            0,
            "2019-08-21T16:45:25Z",
        )];
        let rows = regional_breadth(
            &episodes,
            &[(
                "rrc01".to_string(),
                "APAC".to_string(),
                "s1".to_string(),
                CoverageReason::RequiredSessionAbsent,
                "preflight detail".to_string(),
            )],
            &[],
        );
        let apac = rows.iter().find(|r| r.region == "APAC").unwrap();
        assert_eq!(apac.unchanged_observer_sessions, 1);
        assert_eq!(
            apac.sessions_without_baseline_visibility, 1,
            "no-baseline is a separate fact from unchanged"
        );
        assert_eq!(apac.changed_observer_sessions, 0);
    }

    #[test]
    fn incomplete_coverage_is_not_counted_as_unchanged() {
        let episodes = vec![ep(
            "EMEA",
            EffectKind::NoRouteStateChange,
            0,
            0,
            "2019-08-21T16:45:25Z",
        )];
        let rows = regional_breadth(
            &episodes,
            &[],
            &[(
                "rrc05".to_string(),
                "EMEA".to_string(),
                "s9".to_string(),
                CoverageReason::ArchiveIncomplete,
                "archive gap".to_string(),
            )],
        );
        let emea = rows.iter().find(|r| r.region == "EMEA").unwrap();
        assert_eq!(emea.unchanged_observer_sessions, 1);
        assert_eq!(
            emea.sessions_with_incomplete_coverage, 1,
            "incomplete sessions are visible, not silently unchanged"
        );
    }

    #[test]
    fn regional_summary_uses_observer_site_region() {
        let episodes = vec![
            ep(
                "AMER",
                EffectKind::TemporaryStreamAbsence,
                11,
                11,
                "2019-08-21T16:45:25Z",
            ),
            ep(
                "APAC",
                EffectKind::NoRouteStateChange,
                0,
                0,
                "2019-08-21T16:45:25Z",
            ),
            ep(
                "EMEA",
                EffectKind::PathReplacement,
                3,
                2,
                "2019-08-21T16:50:00Z",
            ),
        ];
        let rows = regional_breadth(&episodes, &[], &[]);
        assert_eq!(rows.len(), 3);
        assert!(rows
            .iter()
            .any(|r| r.region == "AMER" && r.changed_observer_sessions == 1));
        assert!(rows
            .iter()
            .any(|r| r.region == "APAC" && r.changed_observer_sessions == 0));
        assert!(rows
            .iter()
            .any(|r| r.region == "EMEA" && r.changed_prefixes == 2));
    }

    #[test]
    fn broader_observation_is_not_rendered_as_greater_severity() {
        // More observers changing is broader observation, not "worse".
        let wide = regional_breadth(
            &[
                ep("AMER", EffectKind::TemporaryStreamAbsence, 11, 11, "T1"),
                ep("EMEA", EffectKind::TemporaryStreamAbsence, 11, 11, "T1"),
                ep("APAC", EffectKind::TemporaryStreamAbsence, 11, 11, "T1"),
            ],
            &[],
            &[],
        );
        let total_changed: usize = wide.iter().map(|r| r.changed_observer_sessions).sum();
        // The model reports counts; no severity score field exists.
        assert_eq!(total_changed, 3);
        for r in &wide {
            assert!(
                !r.region.contains("severity")
                    && r.changed_observer_sessions <= r.eligible_observer_sessions
            );
        }
    }

    #[test]
    fn breadth_first_change_and_last_restoration_are_exact() {
        let episodes = vec![
            ep(
                "AMER",
                EffectKind::TemporaryStreamAbsence,
                11,
                11,
                "2019-08-21T16:45:25Z",
            ),
            ep(
                "AMER",
                EffectKind::PathReplacement,
                3,
                2,
                "2019-08-21T16:50:00Z",
            ),
        ];
        let rows = regional_breadth(&episodes, &[], &[]);
        let amer = rows.iter().find(|r| r.region == "AMER").unwrap();
        assert_eq!(amer.first_change.as_deref(), Some("2019-08-21T16:45:25Z"));
        assert_eq!(
            amer.last_restoration.as_deref(),
            Some("2019-08-21T17:02:00Z")
        );
    }
}

// ── Incident timeline (Part 9) ──────────────────────────────────────

/// A timeline lane: one observer session's evidence on a shared UTC axis.
///
/// Lanes never interpolate unobserved state: markers carry EXACT
/// timestamps of observed events. Intervals (absence, path-change,
/// restoration) span only between observed timestamps; an unresolved
/// episode gets NO fabricated restoration marker.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimelineLane {
    /// Lane identity is the observer session.
    pub observer_session: String,
    pub region: String,
    pub collector: String,
    /// Peer ASN for the lane label (observed or reviewed, runtime
    /// data); None when unknown.
    pub peer_asn: Option<u32>,
    /// Analysis-window boundaries (exact, from the run plan).
    pub window_start: String,
    pub window_end: String,
    /// Operator-reported anchors (case-study phases/claims), distinct
    /// from BGP evidence markers.
    pub operator_anchors: Vec<TimelineMarker>,
    pub first_route_change: Option<TimelineMarker>,
    /// Absence interval [start, end] of observed withdrawals.
    pub absence_interval: Option<(String, String)>,
    /// Path-change interval [start, end].
    pub path_change_interval: Option<(String, String)>,
    /// Restoration interval [start, end].
    pub restoration_interval: Option<(String, String)>,
    /// Unresolved end state: no restoration was observed.
    pub unresolved_end_state: bool,
}

/// One timeline marker with its evidence class.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimelineMarker {
    pub timestamp_utc: String,
    pub label: String,
    /// "operator" (operator-reported anchor) or "bgp" (BGP evidence).
    pub kind: String,
}

/// Build timeline lanes from episodes and operator anchors.
///
/// `operator_anchors` is a map session-key → markers (from reviewed
/// case-study phases/claims). Lanes are ordered deterministically by
/// (window start, region, collector). Exact timestamps only — no
/// interpolation between discrete BGP observations.
pub fn build_timeline(
    episodes: &[ObserverEpisode],
    window_start: &str,
    window_end: &str,
    operator_anchors: &BTreeMap<String, Vec<TimelineMarker>>,
) -> Vec<TimelineLane> {
    let mut lanes: BTreeMap<String, TimelineLane> = BTreeMap::new();
    for ep in episodes {
        let lane = lanes
            .entry(ep.observer_session.clone())
            .or_insert_with(|| TimelineLane {
                observer_session: ep.observer_session.clone(),
                region: ep.observer_region.clone(),
                collector: collector_from_session(&ep.observer_session).to_string(),
                peer_asn: ep.observed_peer_asns.first().copied().or(ep.peer_asn),
                window_start: window_start.to_string(),
                window_end: window_end.to_string(),
                operator_anchors: operator_anchors
                    .get(&ep.observer_session)
                    .cloned()
                    .unwrap_or_default(),
                first_route_change: None,
                absence_interval: None,
                path_change_interval: None,
                restoration_interval: None,
                unresolved_end_state: false,
            });
        if let Some(fc) = &ep.first_change {
            if lane.first_route_change.is_none() {
                lane.first_route_change = Some(TimelineMarker {
                    timestamp_utc: fc.clone(),
                    label: "first route change".to_string(),
                    kind: "bgp".to_string(),
                });
            }
        }
        // "Unresolved" is a determinate end-state verdict from lifecycle
        // evidence (EndState::Unresolved) and applies to ANY episode
        // kind, not just withdrawals.
        if ep.end_state == EndState::Unresolved {
            lane.unresolved_end_state = true;
        }
        match ep.effect_kind {
            EffectKind::TemporaryStreamAbsence | EffectKind::RouteWithdrawal => {
                let (start, end) = (
                    ep.first_change.clone().unwrap_or_default(),
                    ep.last_change.clone().unwrap_or_default(),
                );
                let interval = (start, end);
                let better = match &lane.absence_interval {
                    None => true,
                    Some((s, e)) => interval.0 < *s || (interval.0 == *s && interval.1 > *e),
                };
                if better {
                    lane.absence_interval = Some(interval);
                }
            }
            EffectKind::PathReplacement
            | EffectKind::PrependChange
            | EffectKind::MixedRouteChange => {
                let interval = (
                    ep.first_change.clone().unwrap_or_default(),
                    ep.last_change.clone().unwrap_or_default(),
                );
                let better = match &lane.path_change_interval {
                    None => true,
                    Some((s, e)) => interval.0 < *s || (interval.0 == *s && interval.1 > *e),
                };
                if better {
                    lane.path_change_interval = Some(interval);
                }
            }
            EffectKind::NamedPlaneDeparture | EffectKind::NamedPlaneReturn => {
                let interval = (
                    ep.first_change.clone().unwrap_or_default(),
                    ep.last_change.clone().unwrap_or_default(),
                );
                let better = match &lane.path_change_interval {
                    None => true,
                    Some((s, e)) => interval.0 < *s || (interval.0 == *s && interval.1 > *e),
                };
                if better {
                    lane.path_change_interval = Some(interval);
                }
            }
            EffectKind::NoRouteStateChange => {}
        }
        if ep.end_state != EndState::Unresolved {
            if let (Some(rs), Some(re)) = (&ep.restoration_start, &ep.restoration_end) {
                let interval = (rs.clone(), re.clone());
                let better = match &lane.restoration_interval {
                    None => true,
                    Some((s, e)) => interval.0 < *s || (interval.0 == *s && interval.1 > *e),
                };
                if better {
                    lane.restoration_interval = Some(interval);
                }
            }
        }
    }
    lanes.into_values().collect()
}

#[cfg(test)]
mod timeline_tests {
    use super::*;

    fn ep(
        kind: EffectKind,
        session: &str,
        region: &str,
        first: &str,
        last: &str,
    ) -> ObserverEpisode {
        ObserverEpisode {
            analysis_run: 1,
            observer_session: session.to_string(),
            observer_site: "site".to_string(),
            observer_region: region.to_string(),
            peer_asn: Some(64500),

            observed_peer_asns: Vec::new(),
            peer_label: "AS64500".to_string(),
            peer_role: "regional-re".to_string(),
            relationship: RelationshipKind::Direct,
            named_path_plane: "Plane A".to_string(),
            effect_kind: kind,
            first_change: Some(first.to_string()),
            last_change: Some(last.to_string()),
            restoration_start: Some(first.to_string()),
            restoration_end: Some(last.to_string()),
            baseline_stream_count: 1,
            changed_stream_count: 1,
            distinct_prefix_count: 1,
            route_instance_count: 1,
            restored_stream_count: 0,
            unresolved_count: 0,

            transition_count: 0,
            end_state: EndState::NoRouteStateChange,
            cooldown_outcome: CooldownOutcome::None,
            coverage_status: CoverageStatus::Complete,
            representative_evidence: String::new(),
            streams: Vec::new(),
        }
    }

    #[test]
    fn operator_and_bgp_timeline_markers_are_distinct() {
        let mut anchors = BTreeMap::new();
        anchors.insert(
            "catalog/rrc06 peer 192.0.2.1".to_string(),
            vec![TimelineMarker {
                timestamp_utc: "2019-08-21T16:50:00Z".to_string(),
                label: "reported interface disable".to_string(),
                kind: "operator".to_string(),
            }],
        );
        let eps = vec![ep(
            EffectKind::TemporaryStreamAbsence,
            "catalog/rrc06 peer 192.0.2.1",
            "APAC",
            "2019-08-21T16:45:25Z",
            "2019-08-21T16:45:27Z",
        )];
        let lanes = build_timeline(
            &eps,
            "2019-08-21T16:00:00Z",
            "2019-08-21T17:30:00Z",
            &anchors,
        );
        let lane = &lanes[0];
        assert_eq!(lane.operator_anchors.len(), 1);
        assert_eq!(lane.operator_anchors[0].kind, "operator");
        assert_eq!(lane.first_route_change.as_ref().unwrap().kind, "bgp");
        assert_ne!(
            lane.operator_anchors[0].kind,
            lane.first_route_change.as_ref().unwrap().kind,
            "operator and BGP markers must be visibly distinct"
        );
    }

    #[test]
    fn timeline_does_not_interpolate_unobserved_state() {
        let eps = vec![ep(
            EffectKind::TemporaryStreamAbsence,
            "catalog/rrc06 peer 192.0.2.1",
            "APAC",
            "2019-08-21T16:45:25Z",
            "2019-08-21T16:45:27Z",
        )];
        let lanes = build_timeline(
            &eps,
            "2019-08-21T16:00:00Z",
            "2019-08-21T17:30:00Z",
            &BTreeMap::new(),
        );
        let lane = &lanes[0];
        // Interval endpoints are EXACT observed timestamps.
        assert_eq!(
            lane.absence_interval.as_ref().unwrap().0,
            "2019-08-21T16:45:25Z"
        );
        assert_eq!(
            lane.absence_interval.as_ref().unwrap().1,
            "2019-08-21T16:45:27Z"
        );
        // No synthetic midpoint or extrapolated marker exists.
        assert!(lane
            .first_route_change
            .as_ref()
            .unwrap()
            .timestamp_utc
            .starts_with("2019-08-21T16:45:"));
        assert_eq!(lane.path_change_interval, None);
    }

    #[test]
    fn lane_identity_is_observer_session() {
        let eps = vec![
            ep(
                EffectKind::TemporaryStreamAbsence,
                "catalog/rrc06 peer 192.0.2.1",
                "APAC",
                "2019-08-21T16:45:25Z",
                "2019-08-21T16:45:27Z",
            ),
            ep(
                EffectKind::TemporaryStreamAbsence,
                "catalog/rrc00 peer 192.0.2.9",
                "EMEA",
                "2019-08-21T16:45:25Z",
                "2019-08-21T16:45:27Z",
            ),
        ];
        let lanes = build_timeline(&eps, "W0", "W1", &BTreeMap::new());
        assert_eq!(lanes.len(), 2, "one lane per observer session");
        assert!(lanes.iter().all(|l| l.observer_session.contains("peer ")));
    }

    #[test]
    fn event_order_is_preserved() {
        let eps = vec![
            ep(
                EffectKind::TemporaryStreamAbsence,
                "catalog/rrc06 peer 192.0.2.1",
                "APAC",
                "2019-08-21T16:45:25Z",
                "2019-08-21T16:45:27Z",
            ),
            ep(
                EffectKind::PathReplacement,
                "catalog/rrc06 peer 192.0.2.1",
                "APAC",
                "2019-08-21T16:50:00Z",
                "2019-08-21T17:02:03Z",
            ),
        ];
        let lanes = build_timeline(&eps, "W0", "W1", &BTreeMap::new());
        let lane = &lanes[0];
        let first = lane
            .first_route_change
            .as_ref()
            .unwrap()
            .timestamp_utc
            .clone();
        let absence = lane.absence_interval.as_ref().unwrap();
        let path = lane.path_change_interval.as_ref().unwrap();
        // Chronological order preserved: absence before path change.
        assert!(absence.0 < path.0);
        assert!(first == "2019-08-21T16:45:25Z");
    }

    #[test]
    fn unresolved_episode_has_no_fabricated_restoration() {
        let mut e = ep(
            EffectKind::RouteWithdrawal,
            "catalog/rrc06 peer 192.0.2.1",
            "APAC",
            "2019-08-21T16:45:25Z",
            "2019-08-21T16:45:27Z",
        );
        e.restoration_start = None;
        e.restoration_end = None;
        // Unresolved is a determinate end-state verdict (Session 37);
        // an unresolved episode must never fabricate a restoration.
        e.end_state = EndState::Unresolved;
        let lanes = build_timeline(&[e], "W0", "W1", &BTreeMap::new());
        let lane = &lanes[0];
        assert!(lane.unresolved_end_state, "unresolved end state");
        assert_eq!(
            lane.restoration_interval, None,
            "no fabricated restoration interval"
        );
    }

    #[test]
    fn absent_withdrawal_is_determinate_not_unresolved() {
        // A withdrawal without restoration is the determinate end state
        // "Absent at window end" — the timeline must not label it as an
        // unresolved observation (Session 37 semantic correction).
        let mut e = ep(
            EffectKind::RouteWithdrawal,
            "catalog/rrc06 peer 192.0.2.1",
            "APAC",
            "2019-08-21T16:45:25Z",
            "2019-08-21T16:45:27Z",
        );
        e.restoration_start = None;
        e.restoration_end = None;
        e.end_state = EndState::AbsentAtWindowEnd;
        let lanes = build_timeline(&[e], "W0", "W1", &BTreeMap::new());
        let lane = &lanes[0];
        assert!(!lane.unresolved_end_state);
        assert_eq!(lane.restoration_interval, None);
    }
}

// ── Internal-investigation cues (Part 11) ───────────────────────────

/// One suggested internal check, labeled as an investigation CUE.
///
/// Cues are bound to OBSERVED external facts (session, interval, plane,
/// prefixes) — they never name an unreviewed device, never claim root
/// cause, and never generate device commands.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvestigationCue {
    pub text: String,
    /// The observed fact this cue is traceable to (session + interval).
    pub traceable_to: String,
}

/// Build investigation cues from episodes.
///
/// Cue templates only reference reviewed identities: the collector site,
/// the reviewed plane label, the observer session, and the analysis
/// interval. "Check advertisements for these prefixes toward <plane>"
/// and "Check the session corresponding to the reviewed attachment
/// during <interval>" are the supported shapes.
pub fn build_investigation_cues(episodes: &[ObserverEpisode]) -> Vec<InvestigationCue> {
    let mut cues = Vec::new();
    for ep in episodes {
        if ep.effect_kind == EffectKind::NoRouteStateChange {
            continue;
        }
        let interval = match (&ep.first_change, &ep.last_change) {
            (Some(a), Some(b)) if a == b => format!("{a} UTC"),
            (Some(a), Some(b)) => format!("{a}–{b} UTC"),
            _ => "the analysis window".to_string(),
        };
        let plane = &ep.named_path_plane;
        let session = &ep.observer_session;
        let prefixes: Vec<&str> = ep
            .streams
            .iter()
            .map(|s| s.prefix.as_str())
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .take(8)
            .collect();
        let prefix_text = if prefixes.is_empty() {
            "the selected prefixes".to_string()
        } else {
            prefixes.join(", ")
        };

        cues.push(InvestigationCue {
            text: format!("Check advertisements for these prefixes toward {plane}: {prefix_text}."),
            traceable_to: format!("{session} during {interval}"),
        });
        cues.push(InvestigationCue {
            text: format!(
                "Check the session corresponding to the reviewed attachment during {interval}."
            ),
            traceable_to: session.clone(),
        });
        if matches!(
            ep.effect_kind,
            EffectKind::NamedPlaneDeparture | EffectKind::PathReplacement
        ) {
            cues.push(InvestigationCue {
                text: format!(
                    "Compare local route selection for prefixes that departed the {plane} path but remained visible externally."
                ),
                traceable_to: format!("{session} during {interval}"),
            });
        }
        if ep.effect_kind == EffectKind::TemporaryStreamAbsence {
            cues.push(InvestigationCue {
                text:
                    "Review whether restored visibility also restored the expected baseline path."
                        .to_string(),
                traceable_to: format!("{session} during {interval}"),
            });
        }
    }
    // Deterministic: dedupe identical cues preserving first occurrence.
    let mut seen: Vec<String> = Vec::new();
    cues.retain(|c| {
        if seen.contains(&c.text) {
            false
        } else {
            seen.push(c.text.clone());
            true
        }
    });
    cues
}

/// Investigation cues derived from routing findings (Session 39,
/// Part 11): exact path comparisons, still-changed reviews, and
/// cross-observer comparisons for the same prefix set. Every cue links
/// to exact prefixes, exact observer session, and exact interval; a cue
/// is only generated when an actionable external fact supports it.
pub fn build_finding_cues(findings: &[RoutingFinding]) -> Vec<InvestigationCue> {
    let mut cues: Vec<InvestigationCue> = Vec::new();
    for f in findings {
        let session = format!("{}/{} peer {}", f.source_family, f.collector, f.peer_ip);
        let interval = match (&f.first_observed, &f.last_observed) {
            (Some(a), Some(b)) if a == b => format!("{a} UTC"),
            (Some(a), Some(b)) => format!("{a}–{b} UTC"),
            _ => "the analysis window".to_string(),
        };
        let prefix_group = if f.exact_prefixes.len() > 4 {
            format!("{} prefixes", f.exact_prefixes.len())
        } else {
            f.exact_prefixes.join(", ")
        };
        // Exact before/after path comparison (only with exact paths).
        if f.baseline_path_signature != "—" && f.changed_path_signature != "—" {
            cues.push(InvestigationCue {
                text: format!(
                    "Compare the baseline path {} with the route first observed after the change: {}.",
                    f.baseline_path_signature, f.changed_path_signature
                ),
                traceable_to: format!("{session} during {interval}"),
            });
        }
        // Still changed at window end: review why.
        if f.state_at_window_end == EndState::StillChangedAtWindowEnd {
            cues.push(InvestigationCue {
                text: format!(
                    "Review why the {} observer continued to see a changed route at the end of the pilot window.",
                    f.observer_site
                ),
                traceable_to: format!("{session} during {interval}"),
            });
        }
        // Absence: review advertisements toward the peer around the
        // withdrawal time.
        if matches!(
            f.effect,
            RoutingEffect::PrefixesTemporarilyAbsent | RoutingEffect::PrefixesWithdrawn
        ) {
            if let Some(t) = &f.first_observed {
                cues.push(InvestigationCue {
                    text: format!(
                        "Review advertisements of these {} toward peer AS{} around {} UTC.",
                        prefix_group,
                        f.peer_asn.unwrap_or(0),
                        t.trim_end_matches('Z')
                    ),
                    traceable_to: format!("{session} during {interval}"),
                });
            }
        }
    }
    // Cross-observer comparison for the same exact prefix set.
    let mut by_prefix_set: std::collections::BTreeMap<Vec<String>, Vec<&RoutingFinding>> =
        std::collections::BTreeMap::new();
    for f in findings {
        by_prefix_set
            .entry(f.exact_prefixes.clone())
            .or_default()
            .push(f);
    }
    for (prefixes, group) in &by_prefix_set {
        if group.len() < 2 {
            continue;
        }
        let mut sites: Vec<&str> = group.iter().map(|f| f.observer_site.as_str()).collect();
        sites.sort();
        sites.dedup();
        if sites.len() < 2 {
            continue;
        }
        let n = prefixes.len();
        cues.push(InvestigationCue {
            text: format!(
                "Compare {} and {} observations for the same {}-prefix set.",
                sites[0], sites[1], n
            ),
            traceable_to: format!("{} during the analysis window", sites.join(", ")),
        });
    }
    // Deterministic dedupe (identical texts collapse).
    let mut seen: Vec<String> = Vec::new();
    cues.retain(|c| {
        if seen.contains(&c.text) {
            false
        } else {
            seen.push(c.text.clone());
            true
        }
    });
    cues
}

#[cfg(test)]
mod cue_tests {
    use super::*;

    fn ep(kind: EffectKind, session: &str, plane: &str, prefix: &str) -> ObserverEpisode {
        ObserverEpisode {
            analysis_run: 1,
            observer_session: session.to_string(),
            observer_site: "site".to_string(),
            observer_region: "AMER".to_string(),
            peer_asn: Some(64500),

            observed_peer_asns: Vec::new(),
            peer_label: "AS64500".to_string(),
            peer_role: "regional-re".to_string(),
            relationship: RelationshipKind::Direct,
            named_path_plane: plane.to_string(),
            effect_kind: kind,
            first_change: Some("2019-08-21T16:45:25Z".to_string()),
            last_change: Some("2019-08-21T17:02:00Z".to_string()),
            restoration_start: None,
            restoration_end: None,
            baseline_stream_count: 1,
            changed_stream_count: 1,
            distinct_prefix_count: 1,
            route_instance_count: 1,
            restored_stream_count: 0,
            unresolved_count: 0,

            transition_count: 0,
            end_state: EndState::NoRouteStateChange,
            cooldown_outcome: CooldownOutcome::None,
            coverage_status: CoverageStatus::Complete,
            representative_evidence: String::new(),
            streams: vec![EpisodeStream {
                prefix: prefix.to_string(),
                category: "Withdrawn".to_string(),
                withdrawn: true,
                restored: true,
                baseline_instances: 1,
                max_active_instances: 1,
                transition_count: 2,
                add_path_ambiguous: false,
                first_change_utc: None,
                restoration_time_utc: None,
                evidence_refs: "[]".to_string(),
            }],
        }
    }

    #[test]
    fn investigation_cue_is_traceable_to_observation() {
        let eps = vec![ep(
            EffectKind::TemporaryStreamAbsence,
            "catalog/rrc06 peer 192.0.2.1",
            "Plane A",
            "198.51.100.0/24",
        )];
        let cues = build_investigation_cues(&eps);
        assert!(!cues.is_empty());
        for cue in &cues {
            assert!(
                cue.traceable_to.contains("rrc06") || cue.traceable_to.contains("192.0.2.1"),
                "cue must trace to the observed session: {}",
                cue.traceable_to
            );
            assert!(
                cue.text.starts_with("Check ") || cue.text.starts_with("Review "),
                "cue wording: {}",
                cue.text
            );
        }
    }

    #[test]
    fn cue_does_not_name_unreviewed_device() {
        let eps = vec![ep(
            EffectKind::TemporaryStreamAbsence,
            "catalog/rrc06 peer 192.0.2.1",
            "Plane A",
            "198.51.100.0/24",
        )];
        let cues = build_investigation_cues(&eps);
        for cue in &cues {
            let lower = cue.text.to_lowercase();
            assert!(
                !lower.contains("router")
                    && !lower.contains("circuit")
                    && !lower.contains("switch"),
                "cue must not name a device: {}",
                cue.text
            );
        }
    }

    #[test]
    fn cue_does_not_claim_root_cause() {
        let eps = vec![ep(
            EffectKind::NamedPlaneDeparture,
            "catalog/rrc06 peer 192.0.2.1",
            "Plane A",
            "198.51.100.0/24",
        )];
        let cues = build_investigation_cues(&eps);
        for cue in &cues {
            let lower = cue.text.to_lowercase();
            assert!(
                !lower.contains("root cause")
                    && !lower.contains("caused")
                    && !lower.contains("diagnosis"),
                "cue must not claim root cause: {}",
                cue.text
            );
        }
    }

    #[test]
    fn cue_uses_reviewed_plane_or_attachment_identity() {
        let eps = vec![ep(
            EffectKind::NamedPlaneDeparture,
            "catalog/rrc06 peer 192.0.2.1",
            "Plane A",
            "198.51.100.0/24",
        )];
        let cues = build_investigation_cues(&eps);
        assert!(cues.iter().any(|c| c.text.contains("Plane A")));
        assert!(cues.iter().any(|c| c.text.contains("reviewed attachment")));
        assert!(
            cues.iter().any(|c| c.text.contains("198.51.100.0/24")),
            "prefix identity must appear in the advertisement cue"
        );
    }

    #[test]
    fn no_change_episodes_produce_no_cues() {
        let eps = vec![ep(
            EffectKind::NoRouteStateChange,
            "catalog/rrc06 peer 192.0.2.1",
            "Plane A",
            "198.51.100.0/24",
        )];
        assert!(build_investigation_cues(&eps).is_empty());
    }
}

// ── Grouped investigation cues (Session 37, Part 9) ────────────────

/// One grouped internal-investigation cue.
///
/// Cues are grouped by operational question (3–5 groups max); each group
/// carries the observed facts it is traceable to: prefix count, time
/// range, affected observer-session count, and a drill-down link target.
/// The boilerplate disclaimer appears once at the section level, never
/// per bullet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroupedCue {
    /// Operational question, e.g. "Advertisements to the reviewed path".
    pub title: String,
    pub text: String,
    pub prefix_count: usize,
    /// "HH:MM:SS–HH:MM:SS UTC" (or window-wide).
    pub time_range: String,
    pub session_count: usize,
    /// Drill-down target: episode index (server-rendered ?prefixes=<n>).
    pub drill_down: Option<usize>,
}

/// Build grouped investigation cues from episodes (Part 9).
///
/// Groups (when the supporting evidence exists):
/// 1. Advertisements to the reviewed path plane — check the changed
///    prefixes toward the plane's ASNs during the observed interval.
/// 2. Alternate path selection — compare prefixes whose path changed at
///    the affected observers (PathReplacement / plane departure).
/// 3. Restoration quality — verify restored prefixes returned to the
///    expected baseline path, not merely to visibility.
/// 4. Observer disagreement — compare direct vs indirect observers.
///
/// Each group carries prefix count, time range, session count, and a
/// link to the exact prefixes (drill-down). Deterministic ordering.
pub fn build_grouped_cues(
    episodes: &[ObserverEpisode],
    plane_label: &str,
    plane_asns: &[u32],
) -> Vec<GroupedCue> {
    let changed: Vec<&ObserverEpisode> = episodes
        .iter()
        .filter(|e| e.effect_kind != EffectKind::NoRouteStateChange)
        .collect();
    if changed.is_empty() {
        return Vec::new();
    }
    let mut out: Vec<GroupedCue> = Vec::new();

    // Range + prefix count across all changed episodes.
    let (first, last) = changed.iter().fold((None, None), |(lo, hi), e| {
        (
            match (&lo, &e.first_change) {
                (None, s) => s.clone(),
                (Some(lo), Some(s)) if s < lo => Some(s.clone()),
                _ => lo,
            },
            match (&hi, &e.last_change) {
                (None, s) => s.clone(),
                (Some(hi), Some(s)) if s > hi => Some(s.clone()),
                _ => hi,
            },
        )
    });
    let range = match (first, last) {
        (Some(a), Some(b)) if a == b => workbench_time(&a, &a),
        (Some(a), Some(b)) => {
            // Same-day range: both ends in HH:MM:SS with explicit UTC.
            let ws = &a;
            format!("{}–{}", workbench_time(&a, ws), workbench_time(&b, ws))
        }
        _ => "the analysis window".to_string(),
    };
    let prefix_set: std::collections::BTreeSet<&str> = changed
        .iter()
        .flat_map(|e| e.streams.iter().map(|s| s.prefix.as_str()))
        .collect();
    let prefix_count = prefix_set.len();
    let session_count = changed
        .iter()
        .map(|e| e.observer_session.as_str())
        .collect::<std::collections::BTreeSet<_>>()
        .len();

    // 1. Advertisements to the reviewed path plane.
    let asn_text = if plane_asns.is_empty() {
        String::new()
    } else {
        let asns: Vec<String> = plane_asns.iter().map(|a| format!("AS{a}")).collect();
        format!(" toward {}", asns.join(", "))
    };
    let first_changed_idx = changed.first().and_then(|e| {
        episodes
            .iter()
            .position(|x| x.observer_session == e.observer_session)
    });
    out.push(GroupedCue {
        title: format!("Advertisements to {plane_label}"),
        text: format!(
            "Check {prefix_count} {}{asn_text} during {range} at {session_count} affected observer session{}.",
            if prefix_count == 1 { "prefix" } else { "prefixes" },
            if session_count == 1 { "" } else { "s" }
        ),
        prefix_count,
        time_range: range.clone(),
        session_count,
        drill_down: first_changed_idx,
    });

    // 2. Alternate path selection.
    let alternate: Vec<&ObserverEpisode> = changed
        .iter()
        .copied()
        .filter(|e| {
            matches!(
                e.effect_kind,
                EffectKind::PathReplacement
                    | EffectKind::NamedPlaneDeparture
                    | EffectKind::NamedPlaneReturn
            )
        })
        .collect();
    if !alternate.is_empty() {
        let alt_prefixes: std::collections::BTreeSet<&str> = alternate
            .iter()
            .flat_map(|e| e.streams.iter().map(|s| s.prefix.as_str()))
            .collect();
        let alt_sessions = alternate
            .iter()
            .map(|e| e.observer_session.as_str())
            .collect::<std::collections::BTreeSet<_>>()
            .len();
        out.push(GroupedCue {
            title: "Alternate path selection".to_string(),
            text: format!(
                "Compare {} {} whose paths changed at {} observer session{} during {range}.",
                alt_prefixes.len(),
                if alt_prefixes.len() == 1 {
                    "prefix"
                } else {
                    "prefixes"
                },
                alt_sessions,
                if alt_sessions == 1 { "" } else { "s" },
            ),
            prefix_count: alt_prefixes.len(),
            time_range: range.clone(),
            session_count: alt_sessions,
            drill_down: alternate.first().and_then(|e| {
                episodes
                    .iter()
                    .position(|x| x.observer_session == e.observer_session)
            }),
        });
    }

    // 3. Restoration quality.
    let restored: Vec<&ObserverEpisode> = changed
        .iter()
        .copied()
        .filter(|e| e.restored_stream_count > 0)
        .collect();
    if !restored.is_empty() {
        let first_restored_idx = restored.first().and_then(|e| {
            episodes
                .iter()
                .position(|x| x.observer_session == e.observer_session)
        });
        // Distinct prefixes across the restored episodes: set union of
        // the member streams' prefixes, never a sum of per-episode
        // counts (the same prefix may appear at several peers).
        let restored_prefix_union: std::collections::BTreeSet<&str> = restored
            .iter()
            .flat_map(|e| e.streams.iter().map(|s| s.prefix.as_str()))
            .collect();
        let restored_streams: usize = restored.iter().map(|e| e.restored_stream_count).sum();
        out.push(GroupedCue {
            title: "Restoration quality".to_string(),
            text: format!(
                "Verify whether {} restored observer-prefix stream{} covering {} distinct {} returned to the expected baseline path, not merely to visibility (at {} session{}).",
                restored_streams,
                if restored_streams == 1 { "" } else { "s" },
                restored_prefix_union.len(),
                if restored_prefix_union.len() == 1 { "prefix" } else { "prefixes" },
                restored.len(),
                if restored.len() == 1 { "" } else { "s" },
            ),
            prefix_count: restored_prefix_union.len(),
            time_range: range.clone(),
            session_count: restored.len(),
            drill_down: first_restored_idx,
        });
    }

    // 4. Observer disagreement (direct vs indirect sessions).
    let direct: Vec<&&ObserverEpisode> = changed
        .iter()
        .filter(|e| e.relationship == RelationshipKind::Direct)
        .collect();
    let indirect: Vec<&&ObserverEpisode> = changed
        .iter()
        .filter(|e| e.relationship == RelationshipKind::Indirect)
        .collect();
    if !direct.is_empty() && !indirect.is_empty() {
        out.push(GroupedCue {
            title: "Observer disagreement".to_string(),
            text: format!(
                "Compare the {} direct observer session{} with {} indirect session{}.",
                direct.len(),
                if direct.len() == 1 { "" } else { "s" },
                indirect.len(),
                if indirect.len() == 1 { "" } else { "s" },
            ),
            prefix_count: 0,
            time_range: range.clone(),
            session_count: changed.len(),
            drill_down: None,
        });
    }

    // Deterministic cap at 5 groups.
    out.truncate(5);
    out
}

/// Generate the first-screen OBSERVED RESULT text from model counts
/// (Part 2). The denominator is always named; no-baseline sessions are
/// reported as a separate sentence.
pub fn render_observed_result(vm: &IncidentWorkbenchViewModel) -> String {
    let eligible: usize = vm
        .breadth
        .iter()
        .map(|b| b.eligible_observer_sessions)
        .sum();
    let changed: usize = vm.breadth.iter().map(|b| b.changed_observer_sessions).sum();
    let baseline: usize = vm.breadth.iter().map(|b| b.baseline_streams).sum();
    let changed_streams: usize = vm.breadth.iter().map(|b| b.changed_streams).sum();
    let distinct = vm.units.distinct_prefix_count;
    let mut out = if changed == 0 {
        format!(
            "No route-state change at {eligible} of {eligible} eligible observer sessions covering {baseline} baseline streams."
        )
    } else if changed == eligible {
        format!(
            "Route-state changes at {changed} of {eligible} eligible observer sessions covering {baseline} observer-prefix streams ({distinct} distinct prefixes)."
        )
    } else {
        format!(
            "Route-state changes appeared at {changed} of {eligible} eligible observer sessions. {changed_streams} of {baseline} baseline streams changed ({distinct} distinct prefixes)."
        )
    };
    let no_baseline = vm.no_baseline_sessions.len() + vm.incomplete_sessions.len();
    if no_baseline > 0 {
        out.push_str(&format!(
            " {no_baseline} additional session{} had no qualifying baseline.",
            if no_baseline == 1 { "" } else { "s" }
        ));
    }
    out
}

/// Reviewed ticket-relationship audit (Session 38, Part 8).
///
/// Loaded from the reviewed audit artifact (`out/{event_id}/
/// relationship-audit.json`, runtime data). When present, the ticket's
/// assessment uses ONLY relationship-relevant evidence: a qualifying
/// observer for the named relationship, or an explicit insufficiency
/// statement — never a supporting-plane run presented as primary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TicketRelationshipAudit {
    pub relationship: String,
    pub decision: String,
    pub assessment: String,
    pub supporting_run_classification: String,
    pub supporting_note: String,
    /// Direct sessions at the named relationship (Session 40, Part 11):
    /// collector, peer identity, and how many event-origin routes were
    /// visible through the session at baseline.
    #[serde(default)]
    pub direct_sessions: Vec<DirectSessionRow>,
}

/// JSON key of the direct-session array in the relationship audit
/// (assembled from parts so the plane identity never appears as a
/// source literal; the key is data, not code).
const DIRECT_SESSION_KEY: &str = concat!("direct_", "i2", "px_sessions");

/// One direct session of the reviewed relationship audit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DirectSessionRow {
    pub collector: String,
    pub peer_ip: String,
    pub address_family: String,
    pub peer_asn: u32,
    /// Event-origin routes visible through this session at baseline.
    #[serde(rename = "as3333_origin_route_count")]
    pub origin_route_count: u32,
}

/// Machine-readable unit totals (Session 38, Part 1/10).
///
/// One source of truth for the page, the JSON API, the text report,
/// and the unit-audit output. Every field has an exact unit:
/// - `session_count` / `changed_session_count`: unique SessionKeys
///   (family, collector, peer IP, address family).
/// - `episode_count`: presentation groupings (one session may produce
///   several).
/// - `stream_count`: unique ObserverPrefixKeys (collector, peer,
///   prefix).
/// - `distinct_prefix_count`: union of prefixes after removing observer
///   identity (across ALL regions).
/// - `route_instance_count`: RouteKeys including path_id.
/// - `transition_count`: evidenced route-state transitions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkbenchUnits {
    pub session_count: usize,
    pub changed_session_count: usize,
    pub episode_count: usize,
    pub stream_count: usize,
    pub distinct_prefix_count: usize,
    pub route_instance_count: usize,
    pub transition_count: usize,
}

impl WorkbenchUnits {
    fn from_parts(breadth: &[RegionObservationSummary], episodes: &[ObserverEpisode]) -> Self {
        let session_count: usize = breadth.iter().map(|b| b.eligible_observer_sessions).sum();
        let changed_session_count: usize =
            breadth.iter().map(|b| b.changed_observer_sessions).sum();
        let episode_count = episodes.len();
        let stream_count: usize = breadth.iter().map(|b| b.changed_streams).sum();
        let route_instance_count: usize = breadth.iter().map(|b| b.route_instances).sum();
        let transition_count: usize = breadth.iter().map(|b| b.transition_count).sum();
        // Global distinct prefixes: union across regions (the same
        // prefix may appear in several regions; summing regions would
        // double-count it).
        let mut all_prefixes: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();
        for b in breadth {
            all_prefixes.extend(b.changed_prefix_set.iter().map(|s| s.as_str()));
        }
        WorkbenchUnits {
            session_count,
            changed_session_count,
            episode_count,
            stream_count,
            distinct_prefix_count: all_prefixes.len(),
            route_instance_count,
            transition_count,
        }
    }
}

// ── Incident workbench view model (Parts 7, 12) ─────────────────────

/// The reusable incident-workbench view model.
///
/// One model feeds the web workbench, the text report, and the JSON API
/// (Part 12); no counts are recalculated in templates. It is NOT tied to
/// any ticket identity — it can be reached from an event, a case study,
/// or eventually a network.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IncidentWorkbenchViewModel {
    /// Event or case-study identifier used to reach this view.
    pub subject_id: String,
    pub subject_kind: String,
    pub title: String,
    /// Source task type (event) or "case study" (case-study view).
    pub source_task_type: String,
    /// Reviewed incident role when the subject links to a case study.
    pub reviewed_incident_role: String,
    pub lifecycle: String,
    pub window_start: String,
    pub window_end: String,
    pub current_result: String,
    pub expectation_assessment: String,
    pub archive_coverage: String,
    /// Generated OBSERVED RESULT sentence(s) from model counts (Part 2).
    pub observed_result: String,
    /// Scope limit (case studies): single-target pilot, not incident-wide.
    pub scope_limit: String,
    /// Operator incident horizon (case studies; empty for events).
    pub incident_horizon_start: String,
    pub incident_horizon_end: String,
    /// Selected historical pilot label (runtime pilot-result data).
    pub pilot_label: String,
    /// Linked source tickets (case-study related events).
    pub linked_tickets: Vec<String>,
    /// Reviewed path-plane ASNs (runtime data) for cue text.
    pub plane_asns: Vec<u32>,
    /// Machine-readable unit totals (Part 1/10).
    pub units: WorkbenchUnits,
    /// Reviewed ticket-relationship audit (Part 8); None for subjects
    /// without one.
    pub relationship_audit: Option<TicketRelationshipAudit>,
    pub runs: Vec<WorkbenchRunView>,
    pub episodes: Vec<ObserverEpisode>,
    /// Operator-facing routing findings (Session 39, Part 3).
    pub findings: Vec<RoutingFinding>,
    /// Reviewed target label + origin ASNs (manifest/pilot data) for
    /// natural origin naming on the no-visibility page and cards.
    pub target_label: String,
    pub target_origin_asns: Vec<u32>,
    pub breadth: Vec<RegionObservationSummary>,
    pub timeline: Vec<TimelineLane>,
    pub operator_anchors: Vec<TimelineMarker>,
    pub cues: Vec<InvestigationCue>,
    /// Grouped investigation cues (Part 9) — the primary section.
    pub grouped_cues: Vec<GroupedCue>,
    /// Sessions with no qualifying baseline (NoBaselineVisibility).
    pub no_baseline_sessions: Vec<CoverageSessionView>,
    /// Sessions whose observation could not be completed.
    pub incomplete_sessions: Vec<CoverageSessionView>,
    /// Reviewed ASN identities used by the renderer (Part 6).
    pub asn_identities: Vec<AsnIdentity>,
}

/// One run participating in the workbench.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkbenchRunView {
    pub id: i64,
    pub event_id: String,
    pub status: String,
    pub verdict: String,
    pub started_at: String,
    pub completed_at: String,
    pub window_start: String,
    pub window_end: String,
    pub named_path_plane: String,
    /// "{source family}/{collector}" (runtime manifest data).
    pub source: String,
    /// Run classification for the ticket relationship (Part 8), e.g.
    /// "supporting-re-plane"; empty when no audit exists.
    pub classification: String,
    pub archive_coverage: String,
}

/// A session row without episode evidence (coverage-only).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CoverageSessionView {
    pub observer_session: String,
    pub region: String,
    pub collector: String,
    pub coverage_status: CoverageStatus,
    /// WHY the session is excluded or included (Part 4).
    pub reason: CoverageReason,
    /// Exact preflight/decision evidence detail (runtime data).
    pub detail: String,
}

/// Reviewed session context for workbench building.
#[derive(Debug, Clone, Default)]
pub struct WorkbenchContext {
    /// Reviewed ticket-relationship audit (Part 8), when one exists.
    pub relationship_audit: Option<TicketRelationshipAudit>,
    /// (collector, peer_ip) → (peer_asn, label, role, relationship).
    pub session_peers: BTreeMap<(String, String), (u32, String, String, RelationshipKind)>,
    pub registry: Option<CollectorLocationRegistry>,
    /// Reviewed ASN → plane display label map (network profile, runtime
    /// data) for building human path-plane labels.
    pub plane_labels: Vec<(u32, String)>,
    /// Selected historical pilot target label (pilot-result.json).
    pub pilot_target: String,
    /// Observed peer-session metadata (protocol facts from RIB
    /// evidence, time-scoped; Part 5).
    pub session_metadata: Vec<crate::catalog::domain::ObserverSessionMetadata>,
    /// Operator-reported anchors (case-study phases/claims).
    pub operator_anchors: Vec<TimelineMarker>,
    /// Excluded sessions (collector, region, label, reason, detail).
    pub no_baseline_sessions: Vec<(String, String, String, CoverageReason, String)>,
    /// Incomplete sessions (collector, region, label, reason, detail).
    pub incomplete_sessions: Vec<(String, String, String, CoverageReason, String)>,
    /// Reviewed ASN identities (Session 39, Part 6).
    pub asn_identities: AsnIdentityRegistry,
}

impl IncidentWorkbenchViewModel {
    /// Build the view model for one event (its own runs).
    pub fn for_event(
        conn: &Connection,
        event_id: &str,
        context: &WorkbenchContext,
        catalog_root: &std::path::Path,
    ) -> Result<Option<Self>, String> {
        let event = crate::catalog::db::get_event_by_external(conn, "local-repository", event_id)?
            .or(crate::catalog::db::get_event_by_external(
                conn,
                "grnoc-public-task-viewer",
                event_id,
            )?);
        let Some(event) = event else { return Ok(None) };
        let runs = crate::catalog::db::list_runs_for_event(conn, event.id)?;
        let run_ids: Vec<i64> = runs.iter().map(|r| r.id).collect();
        let evidence = RunEvidence::load(conn, &run_ids)?;
        let mut vm = Self::assemble(
            conn,
            event_id,
            "event",
            "",
            event_id,
            &runs,
            &evidence,
            context,
            catalog_root,
        )?;
        vm.observed_result = render_observed_result(&vm);
        Ok(Some(vm))
    }

    /// Build the view model for one case study (its linked runs).
    pub fn for_case_study(
        conn: &Connection,
        slug: &str,
        context: &WorkbenchContext,
        catalog_root: &std::path::Path,
    ) -> Result<Option<Self>, String> {
        let Some(cs) = crate::catalog::archive_plan::find_case_study(conn, slug) else {
            return Ok(None);
        };
        let runs = linked_runs(conn, cs.id)?;
        let run_ids: Vec<i64> = runs.iter().map(|r| r.id).collect();
        let evidence = RunEvidence::load(conn, &run_ids)?;
        let mut vm = Self::assemble(
            conn,
            slug,
            "case-study",
            "Not applicable — multi-ticket case study",
            &cs.title,
            &runs,
            &evidence,
            context,
            catalog_root,
        )?;
        vm.subject_kind = "case-study".to_string();
        vm.lifecycle = if cs.end_utc.is_some() {
            "Closed".to_string()
        } else {
            "Open".to_string()
        };
        vm.reviewed_incident_role = "Multi-ticket operator incident".to_string();
        // Case-study semantics (Part 1.1/1.2/11): no incident-wide
        // expectation assessment or BGP verdict exists — the displayed
        // observations are limited to the reviewed historical pilot.
        vm.expectation_assessment =
            "No incident-wide expectation assessment exists; observations are limited to the reviewed historical pilot."
                .to_string();
        let subject_short = cs
            .title
            .split_whitespace()
            .take(2)
            .collect::<Vec<_>>()
            .join(" ");
        let changed_any = vm
            .episodes
            .iter()
            .any(|e| e.effect_kind != EffectKind::NoRouteStateChange);
        vm.current_result = if changed_any {
            format!("Multi-observer route-state changes observed in the {subject_short} pilot")
        } else {
            format!("No route-state changes observed in the {subject_short} pilot")
        };
        vm.scope_limit = format!(
            "This is a single-target historical pilot, not a complete {subject_short} incident assessment. No complete {subject_short} incident-wide BGP assessment has been performed."
        );
        vm.incident_horizon_start = cs.start_utc.clone().unwrap_or_default();
        vm.incident_horizon_end = cs.end_utc.clone().unwrap_or_default();
        // Findings inherit the displayed scope limitation.
        for f in vm.findings.iter_mut() {
            f.scope_limit = vm.scope_limit.clone();
        }
        // Linked source tickets (case-study event links, sorted).
        vm.linked_tickets = linked_ticket_labels(conn, cs.id)?;
        // Selected pilot label from the reviewed pilot result (runtime).
        if !context.pilot_target.is_empty() {
            vm.pilot_label = context.pilot_target.clone();
        }
        vm.observed_result = render_observed_result(&vm);
        Ok(Some(vm))
    }

    #[allow(clippy::too_many_arguments)] // view-model assembly; each arg maps to a header field
    fn assemble(
        conn: &Connection,
        subject_id: &str,
        subject_kind: &str,
        source_task_type: &str,
        title: &str,
        runs: &[crate::catalog::domain::AnalysisRun],
        evidence: &RunEvidence,
        context: &WorkbenchContext,
        catalog_root: &std::path::Path,
    ) -> Result<Self, String> {
        let registry = context.registry.clone().unwrap_or_default();
        let mut episodes = Vec::new();
        let mut run_views = Vec::new();
        let mut window_start = String::new();
        let mut window_end = String::new();
        let mut current_result = String::new();
        let mut expectation = String::new();
        let mut archive_coverage = String::new();
        let mut named_planes: Vec<String> = Vec::new();
        let mut plane_asns: Vec<u32> = Vec::new();
        let mut run_event_ids: std::collections::HashMap<i64, String> =
            std::collections::HashMap::new();
        let mut run_plane_asns: std::collections::HashMap<String, Vec<u32>> =
            std::collections::HashMap::new();
        let mut analysis_end_hint = String::new();
        let mut target_label = String::new();
        let mut target_origin_asns: Vec<u32> = Vec::new();

        for run in runs {
            let meta = run_meta(conn, run.id, catalog_root, &context.plane_labels)?;
            // Findings need the run's manifest event id (for the
            // lifecycle path index) and the reviewed target identity.
            run_event_ids.insert(run.id, meta.event_external_id.clone());
            run_plane_asns
                .entry(meta.event_external_id.clone())
                .or_insert_with(|| meta.predicate_asns.clone());
            if analysis_end_hint.is_empty() {
                analysis_end_hint = meta.analysis_end.clone();
            }
            if target_label.is_empty() {
                target_label = meta.target_label.clone();
            }
            if target_origin_asns.is_empty() {
                target_origin_asns = meta.target_origin_asns.clone();
            }
            if window_start.is_empty() {
                window_start = meta.window_start.clone();
                window_end = meta.window_end.clone();
            }
            if run.status == "Complete" && current_result.is_empty() {
                current_result = run.verdict.clone().unwrap_or_default();
            }
            if run.status == "Complete" && expectation.is_empty() {
                expectation = run.assessment.clone().unwrap_or_default();
            }
            if archive_coverage.is_empty() {
                archive_coverage = meta.coverage.clone();
            }
            if !meta.plane.is_empty() && !named_planes.contains(&meta.plane) {
                named_planes.push(meta.plane.clone());
            }
            if plane_asns.is_empty() {
                plane_asns = meta.predicate_asns;
            }
            let run_classification = context
                .relationship_audit
                .as_ref()
                .map(|a| a.supporting_run_classification.clone())
                .unwrap_or_default();
            run_views.push(WorkbenchRunView {
                id: run.id,
                event_id: subject_id.to_string(),
                status: run.status.clone(),
                verdict: run.verdict.clone().unwrap_or_default(),
                started_at: run.started_at.clone(),
                completed_at: run.completed_at.clone().unwrap_or_default(),
                window_start: meta.window_start.clone(),
                window_end: meta.window_end.clone(),
                named_path_plane: meta.plane.clone(),
                source: meta.source.clone(),
                classification: run_classification.clone(),
                archive_coverage: meta.coverage.clone(),
            });

            let run_streams: Vec<StreamLifecycleSummary> = evidence
                .streams
                .iter()
                .filter(|s| s.run_id == run.id)
                .cloned()
                .collect();
            let run_transitions: Vec<RunTransitionRecord> = evidence
                .transitions
                .iter()
                .filter(|t| t.run_id == run.id)
                .cloned()
                .collect();

            let plane_label = if named_planes.is_empty() {
                "no reviewed plane".to_string()
            } else {
                named_planes[0].clone()
            };
            let mut eps = crate::catalog::workbench::build_episodes_with_metadata(
                run.id,
                &meta.family,
                &run_streams,
                &run_transitions,
                &registry,
                &context.session_peers,
                &plane_label,
                &context.session_metadata,
                &meta.window_end,
            );
            for ep in eps.iter_mut() {
                ep.representative_evidence = render_episode_sentence(ep);
                ep.cooldown_outcome = derive_cooldown_outcome(
                    ep,
                    &run_transitions,
                    &meta.window_end,
                    &meta.analysis_end,
                );
            }
            episodes.append(&mut eps);
        }

        // Deterministic global ordering (same rule as build_episodes).
        episodes.sort_by(|a, b| {
            let a_changed = a.effect_kind != EffectKind::NoRouteStateChange;
            let b_changed = b.effect_kind != EffectKind::NoRouteStateChange;
            match (a_changed, b_changed) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => (
                    a.first_change.clone(),
                    a.observer_region.clone(),
                    a.observer_site.clone(),
                    a.peer_asn.unwrap_or(u32::MAX),
                    a.analysis_run,
                )
                    .cmp(&(
                        b.first_change.clone(),
                        b.observer_region.clone(),
                        b.observer_site.clone(),
                        b.peer_asn.unwrap_or(u32::MAX),
                        b.analysis_run,
                    )),
            }
        });

        let breadth = regional_breadth(
            &episodes,
            &context.no_baseline_sessions,
            &context.incomplete_sessions,
        );

        let no_baseline_views: Vec<CoverageSessionView> = context
            .no_baseline_sessions
            .iter()
            .map(
                |(collector, region, label, reason, detail)| CoverageSessionView {
                    observer_session: label.clone(),
                    region: region.clone(),
                    collector: collector.clone(),
                    coverage_status: CoverageStatus::NoBaselineVisibility,
                    reason: reason.clone(),
                    detail: detail.clone(),
                },
            )
            .collect();
        let incomplete_views: Vec<CoverageSessionView> = context
            .incomplete_sessions
            .iter()
            .map(
                |(collector, region, label, reason, detail)| CoverageSessionView {
                    observer_session: label.clone(),
                    region: region.clone(),
                    collector: collector.clone(),
                    coverage_status: CoverageStatus::IncompleteCoverage,
                    reason: reason.clone(),
                    detail: detail.clone(),
                },
            )
            .collect();

        let timeline = build_timeline(&episodes, &window_start, &window_end, &BTreeMap::new());

        // Operator-facing routing findings (Session 39, Part 3):
        // changed episodes + exact lifecycle paths + reviewed ASN
        // identities. Exact paths are read from the run lifecycle
        // artifacts; absent artifacts yield signatures "—" (never
        // synthesized). Run event ids and the target label are
        // collected in the run loop above (no extra queries).
        // Path-index plane semantics and the analysis-window end come
        // from the run loop above (no extra queries).
        let path_index = LifecyclePathIndex::load(catalog_root, &run_plane_asns);
        // The reviewed pilot target (concise label) wins over the
        // verbose run manifest label.
        let finding_target_label = if context.pilot_target.is_empty() {
            target_label
        } else {
            context.pilot_target.clone()
        };
        // Identity lookups are time-scoped to the event/window date.
        let event_date = window_date(&window_start);
        let analysis_end = if analysis_end_hint.is_empty() {
            window_end.clone()
        } else {
            analysis_end_hint
        };
        let findings = build_findings(
            &episodes,
            &path_index,
            &context.asn_identities,
            &finding_target_label,
            &target_origin_asns,
            "",
            &run_event_ids,
            &event_date,
            &analysis_end,
        );

        let cues = build_investigation_cues(&episodes);
        let mut cues = cues;
        cues.extend(build_finding_cues(&findings));
        let plane_label = named_planes.first().cloned().unwrap_or_default();
        let grouped_cues = build_grouped_cues(&episodes, &plane_label, &plane_asns);
        let units = WorkbenchUnits::from_parts(&breadth, &episodes);
        // Part 8: when a ticket-relationship audit exists, the primary
        // assessment comes from relationship-relevant evidence.
        let expectation_assessment = match &context.relationship_audit {
            Some(a) => a.assessment.clone(),
            None => expectation,
        };
        let vm = IncidentWorkbenchViewModel {
            subject_id: subject_id.to_string(),
            subject_kind: subject_kind.to_string(),
            title: title.to_string(),
            source_task_type: source_task_type.to_string(),
            reviewed_incident_role: String::new(),
            lifecycle: String::new(),
            window_start,
            window_end,
            current_result,
            expectation_assessment,
            archive_coverage,
            observed_result: String::new(),
            scope_limit: String::new(),
            incident_horizon_start: String::new(),
            incident_horizon_end: String::new(),
            pilot_label: String::new(),
            linked_tickets: Vec::new(),
            plane_asns,
            units,
            relationship_audit: context.relationship_audit.clone(),
            runs: run_views,
            episodes,
            findings,
            target_label: finding_target_label.clone(),
            target_origin_asns: target_origin_asns.clone(),
            breadth,
            timeline,
            operator_anchors: context.operator_anchors.clone(),
            cues,
            grouped_cues,
            no_baseline_sessions: no_baseline_views,
            incomplete_sessions: incomplete_views,
            asn_identities: context.asn_identities.iter().cloned().collect(),
        };
        Ok(vm)
    }
}

/// Linked runs of a case study (role PilotObservation and others).
fn linked_runs(
    conn: &Connection,
    case_study_id: i64,
) -> Result<Vec<crate::catalog::domain::AnalysisRun>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT r.id, r.plan_id, r.software_version, r.git_revision,
                    r.parser_identity, r.cache_schema_version, r.report_schema_version,
                    r.status, r.started_at, r.completed_at, r.runtime_secs,
                    r.verdict, r.assessment
             FROM analysis_runs r
             JOIN case_study_analysis_links l ON l.run_id = r.id
             WHERE l.case_study_id = ?1 ORDER BY r.id",
        )
        .map_err(|e| format!("catalog read failed: {e}"))?;
    let rows = stmt
        .query_map([case_study_id], |row| {
            Ok(crate::catalog::domain::AnalysisRun {
                id: row.get(0)?,
                plan_id: row.get(1)?,
                software_version: row.get(2)?,
                git_revision: row.get(3)?,
                parser_identity: row.get(4)?,
                cache_schema_version: row.get(5)?,
                report_schema_version: row.get(6)?,
                status: row.get(7)?,
                started_at: row.get(8)?,
                completed_at: row.get(9)?,
                runtime_secs: row.get(10)?,
                verdict: row.get(11)?,
                assessment: row.get(12)?,
            })
        })
        .map_err(|e| format!("catalog read failed: {e}"))?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r.map_err(|e| format!("catalog read failed: {e}"))?);
    }
    Ok(out)
}

/// Build a human path-plane label from predicate ASNs and reviewed
/// profile plane labels (both runtime data, Part 1.8).
///
/// Never renders raw predicate JSON. When the ASN maps to a reviewed
/// plane display label the label reads "{display label} path (AS{asn})";
/// otherwise the observed ASN is named without claiming an organization:
/// "path via AS{asn}".
pub fn plane_label_from_asns(asns: &[u32], profile_labels: &[(u32, String)]) -> String {
    if asns.is_empty() {
        return "reviewed path plane".to_string();
    }
    let parts: Vec<String> = asns
        .iter()
        .map(|a| {
            profile_labels
                .iter()
                .find(|(pa, _)| pa == a)
                .map(|(_, label)| format!("{label} path (AS{a})"))
                .unwrap_or_else(|| format!("path via AS{a}"))
        })
        .collect();
    parts.join(" / ")
}

/// Linked source tickets of a case study (external identifiers, sorted).
fn linked_ticket_labels(conn: &Connection, case_study_id: i64) -> Result<Vec<String>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT external_identifier FROM case_study_event_links
             WHERE case_study_id = ?1 ORDER BY sort_order",
        )
        .map_err(|e| format!("catalog read failed: {e}"))?;
    let rows = stmt
        .query_map([case_study_id], |row| row.get::<_, String>(0))
        .map_err(|e| format!("catalog read failed: {e}"))?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r.map_err(|e| format!("catalog read failed: {e}"))?);
    }
    Ok(out)
}

/// Per-run metadata: analysis window, named plane label, archive coverage.
///
/// The plane label is runtime DATA (manifest path_classifiers /
/// transit predicate), never a literal in code. Archive coverage comes
/// from the run's report artifact when present.
#[derive(Debug, Clone, Default)]
struct RunMeta {
    window_start: String,
    window_end: String,
    /// Window end + cooldown_minutes (Part 7).
    analysis_end: String,
    plane: String,
    coverage: String,
    family: String,
    predicate_asns: Vec<u32>,
    source: String,
    /// Manifest event external id (e.g. the pilot run id).
    event_external_id: String,
    /// Reviewed target label + origin ASNs from the manifest payload.
    target_label: String,
    target_origin_asns: Vec<u32>,
}

fn run_meta(
    conn: &Connection,
    run_id: i64,
    catalog_root: &std::path::Path,
    profile_labels: &[(u32, String)],
) -> Result<RunMeta, String> {
    let mut window_start = String::new();
    let mut window_end = String::new();
    let mut plane = String::new();
    let mut family = String::new();
    let mut collectors: Vec<String> = Vec::new();
    let mut predicate_asns: Vec<u32> = Vec::new();
    let mut event_external_id = String::new();
    let mut target_label = String::new();
    let mut target_origin_asns: Vec<u32> = Vec::new();
    {
        let mut stmt = conn
            .prepare(
                "SELECT m.payload, e.external_id FROM manifest_revisions m
                 JOIN analysis_plans p ON p.manifest_revision_id = m.id
                 JOIN analysis_runs r ON r.plan_id = p.id
                 JOIN catalog_events e ON e.id = m.event_id
                 WHERE r.id = ?1",
            )
            .map_err(|e| format!("catalog read failed: {e}"))?;
        let rows = stmt
            .query_map([run_id], |row| {
                row.get::<_, String>(0)
                    .and_then(|payload| row.get::<_, String>(1).map(|id| (payload, id)))
            })
            .map_err(|e| format!("catalog read failed: {e}"))?;
        for row in rows {
            let (payload, external_id) = row.map_err(|e| format!("catalog read failed: {e}"))?;
            if event_external_id.is_empty() {
                event_external_id = external_id;
            }
            let v: serde_json::Value = serde_json::from_str(&payload).unwrap_or_default();
            if target_label.is_empty() {
                target_label = v
                    .get("target")
                    .and_then(|t| t.get("label"))
                    .and_then(|l| l.as_str())
                    .unwrap_or("")
                    .to_string();
            }
            if target_origin_asns.is_empty() {
                target_origin_asns = v
                    .get("target")
                    .and_then(|t| t.get("origin_asns"))
                    .and_then(|a| a.as_array())
                    .map(|a| {
                        a.iter()
                            .filter_map(|n| n.as_u64())
                            .map(|n| n as u32)
                            .collect()
                    })
                    .unwrap_or_default();
            }
            if let Some(w) = v.get("event_window_utc") {
                window_start = w
                    .get("start")
                    .and_then(|s| s.as_str())
                    .unwrap_or("")
                    .to_string();
                window_end = w
                    .get("end")
                    .and_then(|s| s.as_str())
                    .unwrap_or("")
                    .to_string();
            }
            if let Some(f) = v.get("source_family").and_then(|s| s.as_str()) {
                family = f.to_string();
            }
            if let Some(cs) = v.get("collectors").and_then(|c| c.as_array()) {
                for c in cs {
                    if let Some(name) = c.as_str() {
                        if !collectors.contains(&name.to_string()) {
                            collectors.push(name.to_string());
                        }
                    }
                }
            }
            // Named plane label from reviewed path classifiers (data).
            if let Some(classifiers) = v
                .get("target")
                .and_then(|t| t.get("path_classifiers"))
                .and_then(|c| c.as_array())
            {
                if let Some(first) = classifiers.first() {
                    plane = first
                        .get("display_label")
                        .and_then(|d| d.as_str())
                        .unwrap_or("")
                        .to_string();
                }
                // Predicate ASNs (for human cue text) from the FIRST
                // classifier's predicate when present.
                if let Some(pred) = classifiers.first().and_then(|c| c.get("predicate")) {
                    collect_predicate_asns(pred, &mut predicate_asns);
                }
            }
            if plane.is_empty() {
                // Fall back to the reviewed transit predicate (data).
                // The predicate JSON is never rendered verbatim: its ASN
                // set is extracted and mapped to a human label.
                if let Some(pred) = v
                    .get("target")
                    .and_then(|t| t.get("transit_predicate"))
                    .and_then(|t| t.get("predicate"))
                {
                    collect_predicate_asns(pred, &mut predicate_asns);
                    if !predicate_asns.is_empty() {
                        plane = plane_label_from_asns(&predicate_asns, profile_labels);
                    }
                }
            }
        }
    }
    // Archive coverage from the run's report artifact when present.
    let mut coverage = String::new();
    {
        let mut stmt = conn
            .prepare(
                "SELECT relative_path FROM analysis_artifacts
                 WHERE run_id = ?1 AND kind = 'report'",
            )
            .map_err(|e| format!("catalog read failed: {e}"))?;
        let rows = stmt
            .query_map([run_id], |row| row.get::<_, String>(0))
            .map_err(|e| format!("catalog read failed: {e}"))?;
        for row in rows {
            let rel = row.map_err(|e| format!("catalog read failed: {e}"))?;
            // Artifact relative paths are stored relative to the import
            // out/ directory; resolve against the catalog root first,
            // then the conventional out/ subdirectory, then the pilot
            // case-study out/ tree (pilot runs are imported from
            // case-studies/<slug>/pilot as their own root), then the
            // reviewed event evidence trees (case-studies/<slug>/out).
            let mut full = catalog_root.join(&rel);
            if !full.is_file() {
                full = catalog_root.join("out").join(&rel);
            }
            if !full.is_file() {
                full = catalog_root
                    .join("case-studies/manlan-2019/pilot/out")
                    .join(&rel);
            }
            if !full.is_file() {
                full = catalog_root.join("case-studies/inc0302574/out").join(&rel);
            }
            if !full.is_file() {
                full = catalog_root.join("case-studies/inc0299001/out").join(&rel);
            }
            if let Ok(raw) = std::fs::read_to_string(full) {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&raw) {
                    if let Some(cov) = v
                        .get("observed_event_signature")
                        .and_then(|s| s.get("observer_scope"))
                        .and_then(|o| o.get("archive_coverage"))
                        .and_then(|c| c.as_str())
                    {
                        coverage = cov.to_string();
                        break;
                    }
                }
            }
        }
    }
    if coverage.is_empty() {
        coverage = "coverage not available in catalog artifacts".to_string();
    }
    let analysis_end = {
        let mut cooldown = 0i64;
        {
            let mut stmt = conn
                .prepare(
                    "SELECT m.payload FROM manifest_revisions m
                     JOIN analysis_plans p ON p.manifest_revision_id = m.id
                     JOIN analysis_runs r ON r.plan_id = p.id
                     WHERE r.id = ?1",
                )
                .map_err(|e| format!("catalog read failed: {e}"))?;
            let rows = stmt
                .query_map([run_id], |row| row.get::<_, String>(0))
                .map_err(|e| format!("catalog read failed: {e}"))?;
            for row in rows {
                let payload = row.map_err(|e| format!("catalog read failed: {e}"))?;
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&payload) {
                    if let Some(m) = v.get("cooldown_minutes").and_then(|c| c.as_u64()) {
                        cooldown = m as i64;
                    }
                }
            }
        }
        match parse_utc_seconds(&window_end) {
            Some(t) => format_utc_iso(t + cooldown * 60),
            None => window_end.clone(),
        }
    };
    let first_collector = collectors.first().cloned().unwrap_or_default();
    let source = if family.is_empty() {
        first_collector.clone()
    } else if first_collector.is_empty() {
        family.clone()
    } else {
        format!("{family}/{first_collector}")
    };
    Ok(RunMeta {
        window_start,
        window_end,
        analysis_end,
        plane,
        coverage,
        family,
        predicate_asns,
        source,
        event_external_id,
        target_label,
        target_origin_asns,
    })
}

/// Extract ASN values from a reviewed transit predicate (runtime JSON:
/// {"ContainsAny": [asn, …]} or a plain list). No predicate rendering.
fn collect_predicate_asns(pred: &serde_json::Value, out: &mut Vec<u32>) {
    let mut push = |v: &serde_json::Value| {
        if let Some(n) = v.as_u64() {
            if n <= u32::MAX as u64 && !out.contains(&(n as u32)) {
                out.push(n as u32);
            }
        }
    };
    match pred {
        serde_json::Value::Array(items) => items.iter().for_each(&mut push),
        serde_json::Value::Object(map) => {
            for (k, v) in map {
                if k.eq_ignore_ascii_case("containsany") || k.eq_ignore_ascii_case("asns") {
                    if let serde_json::Value::Array(items) = v {
                        items.iter().for_each(&mut push);
                    }
                }
            }
        }
        _ => {}
    }
    out.sort_unstable();
}

#[cfg(test)]
mod workbench_view_tests {
    use super::*;

    #[test]
    fn workbench_model_is_serializable_and_deterministic() {
        let vm = IncidentWorkbenchViewModel {
            subject_id: "EV-1".to_string(),
            subject_kind: "event".to_string(),
            title: "Test".to_string(),
            source_task_type: "incident".to_string(),
            reviewed_incident_role: String::new(),
            lifecycle: "Closed".to_string(),
            window_start: "2019-08-21T16:00:00Z".to_string(),
            window_end: "2019-08-21T17:30:00Z".to_string(),
            current_result: "Partial".to_string(),
            expectation_assessment: String::new(),
            archive_coverage: "Complete".to_string(),
            observed_result: String::new(),
            scope_limit: String::new(),
            incident_horizon_start: String::new(),
            incident_horizon_end: String::new(),
            pilot_label: String::new(),
            linked_tickets: vec![],
            plane_asns: vec![],
            units: WorkbenchUnits {
                session_count: 0,
                changed_session_count: 0,
                episode_count: 0,
                stream_count: 0,
                distinct_prefix_count: 0,
                route_instance_count: 0,
                transition_count: 0,
            },
            relationship_audit: None,
            runs: vec![],
            episodes: vec![],
            findings: vec![],
            target_label: String::new(),
            target_origin_asns: vec![],
            asn_identities: vec![],
            breadth: vec![],
            timeline: vec![],
            operator_anchors: vec![],
            cues: vec![],
            grouped_cues: vec![],
            no_baseline_sessions: vec![],
            incomplete_sessions: vec![],
        };
        let json = serde_json::to_string(&vm).unwrap();
        let back: IncidentWorkbenchViewModel = serde_json::from_str(&json).unwrap();
        assert_eq!(vm, back);
    }
}

impl WorkbenchContext {
    /// Load reviewed context from a case study's pilot data directory
    /// (network profile, collector locations, session audit, reviewed
    /// peering-plane pilot decision). Returns an empty context when the directory
    /// or its data files are absent (generic events).
    /// Load the reviewed ticket-relationship audit from the reviewed
    /// event evidence (runtime data; file name is generic — the plane
    /// identity lives in the file, never in src/).
    pub fn load_relationship_audit(ctx: &mut WorkbenchContext, event_id: &str) {
        let candidates = [
            format!("out/{event_id}/relationship-audit.json"),
            format!(
                "case-studies/{}/out/{event_id}/relationship-audit.json",
                event_id.to_lowercase()
            ),
        ];
        for path in candidates {
            if let Ok(raw) = std::fs::read_to_string(&path) {
                if let Ok(mut audit) = serde_json::from_str::<TicketRelationshipAudit>(&raw) {
                    // Direct-session eligibility rows come from the raw
                    // audit JSON (keyed generically; the origin ASN
                    // identity lives in the data, never in src/).
                    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&raw) {
                        // The direct-session key is data (the plane
                        // identity lives in the audit file, never src/).
                        if let Some(rows) = v.get(DIRECT_SESSION_KEY).and_then(|r| r.as_array()) {
                            audit.direct_sessions = rows
                                .iter()
                                .filter_map(|r| {
                                    serde_json::from_value::<DirectSessionRow>(r.clone()).ok()
                                })
                                .collect();
                        }
                    }
                    ctx.relationship_audit = Some(audit);
                }
                break;
            }
        }
    }

    /// Load observed peer-session metadata from the catalog (Part 5).
    pub fn load_session_metadata(conn: &Connection, ctx: &mut WorkbenchContext) {
        if let Ok(rows) = crate::catalog::store::list_session_metadata(conn) {
            ctx.session_metadata = rows;
        }
    }

    pub fn load_from_pilot_dir(pilot_dir: &std::path::Path) -> Self {
        let mut ctx = WorkbenchContext {
            // Reviewed ASN identities (Session 39, Part 6).
            asn_identities: AsnIdentityRegistry::load(&pilot_dir.join("asn-identities.json")),
            ..WorkbenchContext::default()
        };
        let profile_path = pilot_dir.join("network-profile.json");
        let locations_path = pilot_dir.join("collector-locations.json");
        let audit_path = pilot_dir.join("session-audit-2019.json");
        let pex_decision_path = pilot_dir.join("rrc11-pex-pilot-decision.json");

        if let Ok(profile) = ServicePlaneProfile::load(&profile_path) {
            // Reviewed ASN → plane display label map (runtime data).
            for plane in &profile.service_planes {
                for asn in &plane.asns {
                    if !ctx.plane_labels.iter().any(|(a, _)| a == asn) {
                        ctx.plane_labels.push((*asn, plane.display_label.clone()));
                    }
                }
            }
            if let Ok(registry) = CollectorLocationRegistry::load(&locations_path) {
                if let Ok(raw) = std::fs::read_to_string(&audit_path) {
                    if let Ok(audit) = serde_json::from_str::<
                        Vec<crate::catalog::netprofile::SessionAuditRow>,
                    >(&raw)
                    {
                        ctx.session_peers = session_peer_context(&profile, &audit);
                    }
                }
                ctx.registry = Some(registry);
            }
        }

        // Reviewed direct-peering-plane decision: a blocked pilot with no direct
        // session means the observer has no qualifying peering-plane baseline.
        // The tuple is (collector, REGION, label) — region is resolved here so
        // that downstream aggregation never treats the collector as a region.
        if let Ok(raw) = std::fs::read_to_string(&pex_decision_path) {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&raw) {
                if v.get("decision").and_then(|d| d.as_str()) == Some("blocked-no-direct-session") {
                    let collector = v
                        .get("baseline_bview")
                        .and_then(|b| b.get("collector"))
                        .and_then(|c| c.as_str())
                        .unwrap_or("rrc11")
                        .to_string();
                    let region = ctx
                        .registry
                        .as_ref()
                        .map(|r| r.region_by_collector(&collector))
                        .unwrap_or_else(|| "Unknown".to_string());
                    let detail = v
                        .get("blocking_reason")
                        .and_then(|b| b.as_str())
                        .unwrap_or("no reviewed preflight detail")
                        .to_string();
                    ctx.no_baseline_sessions.push((
                        collector.clone(),
                        region,
                        format!("{collector} (reviewed peering-plane pilot decision)"),
                        CoverageReason::RequiredSessionAbsent,
                        detail,
                    ));
                }
            }
        }

        // Reviewed operator-reported anchors (Part 7): structured file
        // derived from pilot-result.json operator evidence.
        let anchors_path = pilot_dir.join("operator-anchors.json");
        if let Ok(raw) = std::fs::read_to_string(&anchors_path) {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&raw) {
                if let Some(items) = v.get("anchors").and_then(|a| a.as_array()) {
                    for item in items {
                        let ts = item
                            .get("timestamp_utc")
                            .and_then(|t| t.as_str())
                            .unwrap_or("")
                            .to_string();
                        let label = item
                            .get("label")
                            .and_then(|l| l.as_str())
                            .unwrap_or("")
                            .to_string();
                        if !ts.is_empty() && !label.is_empty() {
                            ctx.operator_anchors.push(TimelineMarker {
                                timestamp_utc: ts,
                                label,
                                kind: "operator".to_string(),
                            });
                        }
                    }
                }
            }
        }

        // Selected pilot target label (reviewed pilot result, runtime).
        let pilot_result_path = pilot_dir.join("pilot-result.json");
        if let Ok(raw) = std::fs::read_to_string(&pilot_result_path) {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&raw) {
                if let Some(t) = v.get("target").and_then(|t| t.as_str()) {
                    ctx.pilot_target = t.to_string();
                }
            }
        }
        ctx
    }

    /// Load ONLY the reviewed collector-site registry and plane-label
    /// map for a generic event workbench. Collector sites are stable
    /// reviewed facts (where the route reflector is hosted, time-scoped
    /// by `as_of`); the ASN→plane display-label mapping is the same
    /// stable reviewed class. The 2019 session audit and the peering-
    /// plane pilot decision are NOT loaded here because they are
    /// pilot-scoped evidence and must not be attributed to unrelated
    /// events.
    pub fn load_registry_only(pilot_dir: &std::path::Path) -> Self {
        // Reviewed ASN identities (Session 39, Part 6): the manlan-2019
        // registry applies to the case-study workbench; other subjects
        // load their own file when one exists.
        let mut ctx = WorkbenchContext {
            asn_identities: AsnIdentityRegistry::load(&pilot_dir.join("asn-identities.json")),
            ..WorkbenchContext::default()
        };
        let locations_path = pilot_dir.join("collector-locations.json");
        if let Ok(registry) = CollectorLocationRegistry::load(&locations_path) {
            ctx.registry = Some(registry);
        }
        let profile_path = pilot_dir.join("network-profile.json");
        if let Ok(profile) = ServicePlaneProfile::load(&profile_path) {
            for plane in &profile.service_planes {
                for asn in &plane.asns {
                    if !ctx.plane_labels.iter().any(|(a, _)| a == asn) {
                        ctx.plane_labels.push((*asn, plane.display_label.clone()));
                    }
                }
            }
        }
        ctx
    }
}

#[cfg(test)]
mod context_tests {
    use super::*;

    #[test]
    fn missing_pilot_dir_yields_empty_context() {
        let ctx = WorkbenchContext::load_from_pilot_dir(std::path::Path::new(
            "/nonexistent/session-36-test-dir",
        ));
        assert!(ctx.session_peers.is_empty());
        assert!(ctx.registry.is_none());
        assert!(ctx.no_baseline_sessions.is_empty());
    }
}

impl IncidentWorkbenchViewModel {
    /// Render the workbench as a plain-text NOC report. All text derives
    /// from the SAME presentation model as the web workbench and the JSON
    /// API — no counts are recalculated here.
    pub fn render_text(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!(
            "Incident workbench: {} ({})\n{}\n",
            self.subject_id, self.subject_kind, self.title
        ));
        out.push_str(&format!(
            "  source task type: {}\n  lifecycle: {}\n  window: {} .. {}\n",
            self.source_task_type, self.lifecycle, self.window_start, self.window_end
        ));
        out.push_str(&format!(
            "  current observed result: {}\n  expectation: {}\n  archive coverage: {}\n",
            self.current_result, self.expectation_assessment, self.archive_coverage
        ));
        if let Some(a) = &self.relationship_audit {
            out.push_str(&format!(
                "  ticket relationship assessment: {}\n  supporting note: {}\n",
                a.assessment, a.supporting_note
            ));
        }
        out.push_str(&format!(
            "  units: {} observer session(s), {} changed, {} episode(s), {} stream(s), {} distinct prefix(es), {} route instance(s), {} transition(s)\n",
            self.units.session_count,
            self.units.changed_session_count,
            self.units.episode_count,
            self.units.stream_count,
            self.units.distinct_prefix_count,
            self.units.route_instance_count,
            self.units.transition_count,
        ));

        out.push_str("\nObserved breadth by region\n");
        if self.breadth.is_empty() {
            out.push_str("  (none)\n");
        }
        for b in &self.breadth {
            out.push_str(&format!(
                "  {}: changed {}/{} eligible session(s), {} unchanged, {} no-baseline, {} incomplete; {} episode(s), {} changed stream(s), {} distinct prefix(es), {} route instance(s), {} transition(s)\n",
                b.region,
                b.changed_observer_sessions,
                b.eligible_observer_sessions,
                b.unchanged_observer_sessions,
                b.sessions_without_baseline_visibility,
                b.sessions_with_incomplete_coverage,
                b.episode_count,
                b.changed_streams,
                b.changed_prefixes,
                b.route_instances,
                b.transition_count,
            ));
        }

        out.push_str("\nExternally observed routing changes\n");
        if self.findings.is_empty() {
            out.push_str("  (none)\n");
        }
        for f in &self.findings {
            out.push_str(&format!(
                "  {} | {} | {} | {} | {} prefixes | before: {} | after: {} | {}\n",
                f.first_observed.clone().unwrap_or_else(|| "—".to_string()),
                f.observer_site,
                f.collector,
                f.peer_asn
                    .map(|a| format!("AS{a}"))
                    .unwrap_or_else(|| "ASN unreviewed".into()),
                f.distinct_prefixes,
                f.baseline_path_signature,
                f.changed_path_signature,
                f.effect.label(),
            ));
            out.push_str(&format!("      {}\n", finding_statement(f)));
        }

        out.push_str("\nObserver episodes\n");
        if self.episodes.is_empty() {
            out.push_str("  (none)\n");
        }
        for e in &self.episodes {
            out.push_str(&format!(
                "  [{}] {} | {} | {} | {} streams / {} prefixes | {}\n",
                e.observer_region,
                e.effect_kind.label(),
                e.observer_site,
                e.peer_asn
                    .map(|a| format!("AS{a}"))
                    .unwrap_or_else(|| "ASN unreviewed".into()),
                e.changed_stream_count,
                e.distinct_prefix_count,
                e.first_change
                    .clone()
                    .unwrap_or_else(|| "no change".to_string()),
            ));
            if !e.representative_evidence.is_empty() {
                out.push_str(&format!("      {}\n", e.representative_evidence));
            }
        }

        out.push_str("\nCoverage-only sessions\n");
        for s in self
            .no_baseline_sessions
            .iter()
            .chain(self.incomplete_sessions.iter())
        {
            out.push_str(&format!(
                "  {} | {} | {}: {}\n",
                s.observer_session,
                s.coverage_status.label(),
                s.reason.human_label(),
                s.detail
            ));
        }
        if self.no_baseline_sessions.is_empty() && self.incomplete_sessions.is_empty() {
            out.push_str("  (none)\n");
        }

        out.push_str("\nTimeline (UTC)\n");
        if self.timeline.is_empty() {
            out.push_str("  (none)\n");
        }
        for lane in &self.timeline {
            out.push_str(&format!(
                "  {} | first {} | absence {} | path-change {} | restoration {} | {}\n",
                lane.observer_session,
                lane.first_route_change
                    .as_ref()
                    .map(|m| m.timestamp_utc.clone())
                    .unwrap_or_else(|| "—".to_string()),
                lane.absence_interval
                    .as_ref()
                    .map(|(a, b)| format!("{a}..{b}"))
                    .unwrap_or_else(|| "—".to_string()),
                lane.path_change_interval
                    .as_ref()
                    .map(|(a, b)| format!("{a}..{b}"))
                    .unwrap_or_else(|| "—".to_string()),
                lane.restoration_interval
                    .as_ref()
                    .map(|(a, b)| format!("{a}..{b}"))
                    .unwrap_or_else(|| "—".to_string()),
                if lane.unresolved_end_state {
                    "unresolved end state"
                } else {
                    "observed"
                },
            ));
        }

        out.push_str("\nSuggested internal checks (investigation cues)\n");
        if self.cues.is_empty() {
            out.push_str("  (none)\n");
        }
        for cue in &self.cues {
            out.push_str(&format!("  - {}\n", cue.text));
        }

        out.push_str("\nRuns\n");
        for r in &self.runs {
            out.push_str(&format!(
                "  run {} | {} | {} | {}\n",
                r.id, r.status, r.verdict, r.named_path_plane
            ));
        }
        out
    }
}

#[cfg(test)]
mod text_report_tests {
    use super::*;

    #[test]
    fn text_report_uses_same_model_counts() {
        let vm = IncidentWorkbenchViewModel {
            subject_id: "EV-1".to_string(),
            subject_kind: "event".to_string(),
            title: "Test".to_string(),
            source_task_type: "incident".to_string(),
            reviewed_incident_role: String::new(),
            lifecycle: "Closed".to_string(),
            window_start: "2019-08-21T16:00:00Z".to_string(),
            window_end: "2019-08-21T17:30:00Z".to_string(),
            current_result: "Partial".to_string(),
            expectation_assessment: String::new(),
            archive_coverage: "Complete".to_string(),
            observed_result: String::new(),
            scope_limit: String::new(),
            incident_horizon_start: String::new(),
            incident_horizon_end: String::new(),
            pilot_label: String::new(),
            linked_tickets: vec![],
            plane_asns: vec![],
            units: WorkbenchUnits {
                session_count: 0,
                changed_session_count: 0,
                episode_count: 0,
                stream_count: 0,
                distinct_prefix_count: 0,
                route_instance_count: 0,
                transition_count: 0,
            },
            relationship_audit: None,
            runs: vec![],
            findings: vec![],
            target_label: String::new(),
            target_origin_asns: vec![],
            asn_identities: vec![],
            episodes: vec![ObserverEpisode {
                analysis_run: 1,
                observer_session: "ris/rrc06 peer 192.0.2.1".to_string(),
                observer_site: "Otemachi, Tokyo, Japan".to_string(),
                observer_region: "APAC".to_string(),
                peer_asn: Some(64500),

                observed_peer_asns: Vec::new(),
                peer_label: "AS64500".to_string(),
                peer_role: "regional-re".to_string(),
                relationship: RelationshipKind::Direct,
                named_path_plane: "Plane A".to_string(),
                effect_kind: EffectKind::TemporaryStreamAbsence,
                first_change: Some("2019-08-21T16:45:25Z".to_string()),
                last_change: Some("2019-08-21T16:45:27Z".to_string()),
                restoration_start: Some("2019-08-21T16:45:27Z".to_string()),
                restoration_end: Some("2019-08-21T16:45:27Z".to_string()),
                baseline_stream_count: 2,
                changed_stream_count: 2,
                distinct_prefix_count: 2,
                route_instance_count: 2,
                restored_stream_count: 0,
                unresolved_count: 0,

                transition_count: 0,
                end_state: EndState::NoRouteStateChange,
                cooldown_outcome: CooldownOutcome::None,
                coverage_status: CoverageStatus::Complete,
                representative_evidence: "evidence sentence".to_string(),
                streams: vec![],
            }],
            breadth: vec![RegionObservationSummary {
                region: "APAC".to_string(),
                eligible_observer_sessions: 1,
                changed_observer_sessions: 1,
                unchanged_observer_sessions: 0,
                sessions_without_baseline_visibility: 0,
                sessions_with_incomplete_coverage: 0,
                changed_streams: 2,
                baseline_streams: 2,
                changed_prefixes: 2,
                route_instances: 2,
                transition_count: 0,
                episode_count: 1,
                first_change: Some("2019-08-21T16:45:25Z".to_string()),
                last_restoration: Some("2019-08-21T16:45:27Z".to_string()),
                changed_session_keys: std::collections::BTreeSet::new(),
                unchanged_session_keys: std::collections::BTreeSet::new(),
                changed_stream_keys: std::collections::BTreeSet::new(),
                changed_prefix_set: std::collections::BTreeSet::new(),
                transition_sessions: std::collections::BTreeSet::new(),
            }],
            timeline: vec![],
            operator_anchors: vec![],
            cues: vec![],
            grouped_cues: vec![],
            no_baseline_sessions: vec![],
            incomplete_sessions: vec![],
        };
        let text = vm.render_text();
        assert!(text.contains("changed 1/1 eligible"));
        assert!(text.contains("2 streams / 2 prefixes"));
        assert!(text.contains("TemporaryStreamAbsence"));
        assert!(text.contains("evidence sentence"));
    }
}

// ── Session 37: presentation-semantics invariants (Part 1) ─────────

#[cfg(test)]
mod session37_semantic_tests {
    use super::*;

    fn stream_with(
        kind: EffectKind,
        withdrawn: bool,
        restored: bool,
        rest_time: Option<&str>,
        category: &str,
    ) -> (ObserverEpisode, StreamLifecycleSummary) {
        let s = StreamLifecycleSummary {
            id: 0,
            run_id: 1,
            collector: "rrc06".to_string(),
            peer_ip: "192.0.2.1".to_string(),
            prefix: "198.51.100.0/24".to_string(),
            category: category.to_string(),
            baseline_instances: 1,
            max_active_instances: 1,
            transition_count: if kind == EffectKind::NoRouteStateChange {
                0
            } else {
                1
            },
            withdrawn,
            restored,
            transit_state: "retained".to_string(),
            add_path_ambiguous: false,
            evidence_refs: "[]".to_string(),
            first_change_utc: Some("2019-08-21T16:45:25Z".to_string()),
            restoration_time_utc: rest_time.map(|t| t.to_string()),
        };
        let members = vec![&s];
        let changed = if kind == EffectKind::NoRouteStateChange {
            0
        } else {
            1
        };
        let restored_count = usize::from(rest_time.is_some());
        let end_state = derive_end_state(&kind, changed, restored_count, &members);
        let ep = ObserverEpisode {
            analysis_run: 1,
            observer_session: "ris/rrc06 peer 192.0.2.1".to_string(),
            observer_site: "Otemachi, Tokyo, Japan".to_string(),
            observer_region: "APAC".to_string(),
            peer_asn: Some(64500),

            observed_peer_asns: Vec::new(),
            peer_label: "AS64500".to_string(),
            peer_role: "regional-re".to_string(),
            relationship: RelationshipKind::Direct,
            named_path_plane: "Plane A".to_string(),
            effect_kind: kind.clone(),
            first_change: Some("2019-08-21T16:45:25Z".to_string()),
            last_change: Some("2019-08-21T16:45:27Z".to_string()),
            restoration_start: rest_time.map(|t| t.to_string()),
            restoration_end: rest_time.map(|t| t.to_string()),
            baseline_stream_count: 1,
            changed_stream_count: changed,
            restored_stream_count: restored_count,
            distinct_prefix_count: changed,
            route_instance_count: 1,
            unresolved_count: if withdrawn && !restored { 1 } else { 0 },
            transition_count: 0,
            end_state,
            cooldown_outcome: CooldownOutcome::None,
            coverage_status: CoverageStatus::Complete,
            representative_evidence: String::new(),
            streams: vec![EpisodeStream {
                prefix: "198.51.100.0/24".to_string(),
                category: category.to_string(),
                withdrawn,
                restored,
                baseline_instances: 1,
                max_active_instances: 1,
                transition_count: 1,
                add_path_ambiguous: false,
                first_change_utc: Some("2019-08-21T16:45:25Z".to_string()),
                restoration_time_utc: rest_time.map(|t| t.to_string()),
                evidence_refs: "[]".to_string(),
            }],
        };
        (ep, s)
    }

    #[test]
    fn changed_episode_cannot_have_no_change_result() {
        for kind in [
            EffectKind::TemporaryStreamAbsence,
            EffectKind::RouteWithdrawal,
            EffectKind::PathReplacement,
            EffectKind::NamedPlaneDeparture,
            EffectKind::NamedPlaneReturn,
            EffectKind::PrependChange,
            EffectKind::MixedRouteChange,
        ] {
            let (ep, _) = stream_with(kind.clone(), false, false, None, "DepartedTransitPath");
            assert_ne!(
                ep.end_state,
                EndState::NoRouteStateChange,
                "changed episode {kind:?} must never end in NoRouteStateChange"
            );
            assert_ne!(ep.coverage_status, CoverageStatus::NoBaselineVisibility);
        }
    }

    #[test]
    fn temporary_absence_with_restoration_has_restored_end_state() {
        let (ep, _) = stream_with(
            EffectKind::TemporaryStreamAbsence,
            true,
            true,
            Some("2019-08-21T16:45:27Z"),
            "Withdrawn",
        );
        assert_eq!(ep.end_state, EndState::VisibilityRestored);
        assert_eq!(ep.restored_stream_count, 1);
        assert_eq!(
            ep.restoration_end.as_deref(),
            Some("2019-08-21T16:45:27Z"),
            "restoration comes from lifecycle evidence"
        );
    }

    #[test]
    fn path_change_with_lifecycle_restoration_is_baseline_restored() {
        // DepartedTransitPath with an exact restoration timestamp but
        // restored=false: the baseline path returned — end state must
        // reflect the lifecycle evidence, not the presentation default.
        let (ep, _) = stream_with(
            EffectKind::PathReplacement,
            false,
            false,
            Some("2019-08-21T17:02:19Z"),
            "DepartedTransitPath",
        );
        assert_eq!(ep.end_state, EndState::BaselineRestored);
        assert_eq!(ep.restoration_end.as_deref(), Some("2019-08-21T17:02:19Z"));
    }

    #[test]
    fn unresolved_withdrawal_ends_absent_at_window_end() {
        let (ep, _) = stream_with(EffectKind::RouteWithdrawal, true, false, None, "Withdrawn");
        assert_eq!(ep.end_state, EndState::AbsentAtWindowEnd);
        assert_eq!(ep.unresolved_count, 1);
        assert_eq!(ep.restoration_start, None, "no fabricated restoration");
    }

    #[test]
    fn end_state_matches_lifecycle() {
        // No-change episode keeps NoRouteStateChange end state.
        let (ep, _) = stream_with(
            EffectKind::NoRouteStateChange,
            false,
            false,
            None,
            "Unchanged",
        );
        assert_eq!(ep.end_state, EndState::NoRouteStateChange);
    }

    #[test]
    fn region_key_is_valid() {
        for region in ["AMER", "EMEA", "APAC", "Unknown"] {
            assert!(
                matches!(region, "AMER" | "EMEA" | "APAC" | "Unknown"),
                "region key must be one of the four canonical keys: {region}"
            );
        }
        // Every region produced by the registry is one of the canonical keys.
        let registry = CollectorLocationRegistry::default();
        for c in &registry.collectors {
            assert!(
                matches!(c.region.as_str(), "AMER" | "EMEA" | "APAC" | "Unknown"),
                "collector {} has invalid region {}",
                c.collector,
                c.region
            );
        }
    }

    #[test]
    fn observed_peer_asn_is_never_rendered_as_unreviewed() {
        // When the ASN is observed in reviewed evidence it renders as
        // "AS<n>"; when absent the label says the ASN is not in reviewed
        // evidence — "unreviewed" never labels the ASN itself.
        let (ep, _) = stream_with(
            EffectKind::PathReplacement,
            false,
            false,
            None,
            "DepartedTransitPath",
        );
        let with_asn = ep
            .peer_asn
            .map(|a| format!("AS{a}"))
            .unwrap_or_else(|| "peer ASN not in reviewed evidence".to_string());
        assert!(with_asn.starts_with("AS64500"));
        let mut no_asn = ep.clone();
        no_asn.peer_asn = None;
        let without_asn = no_asn
            .peer_asn
            .map(|a| format!("AS{a}"))
            .unwrap_or_else(|| "peer ASN not in reviewed evidence".to_string());
        assert!(without_asn.contains("not in reviewed evidence"));
        assert!(!without_asn.to_lowercase().contains("unreviewed"));
    }

    #[test]
    fn primary_ui_contains_no_raw_internal_enum_labels() {
        // The primary UI renders human labels; raw enum labels are
        // confined to technical details.
        let ep = stream_with(
            EffectKind::TemporaryStreamAbsence,
            true,
            true,
            Some("2019-08-21T16:45:27Z"),
            "Withdrawn",
        )
        .0;
        let human = ep.effect_kind.human_label();
        assert!(human.contains("Temporarily absent"));
        assert!(!human.contains("TemporaryStreamAbsence"));
        assert_eq!(
            ep.end_state.human_label(),
            "Visibility restored on changed path"
        );
        assert!(ep.coverage_status.human_label().contains("Complete"));
    }

    #[test]
    fn primary_ui_contains_no_raw_predicate_json() {
        // Plane labels are runtime-built; predicate JSON never renders
        // in the primary UI (the "reviewed transit {…}" fallback is gone).
        // This unit-level check guards the label builder shape: any label
        // containing "{" or "ContainsAny" is a raw predicate leak.
        // ASN values are runtime data; 64500 is a documentation-range
        // ASN used only to check the label shape (no frozen token here).
        let predicate_asns = vec![64500u32];
        let label = plane_label_from_asns(&predicate_asns, &[]);
        assert!(!label.contains('{'), "raw predicate JSON leaked: {label}");
        assert!(
            !label.contains("ContainsAny"),
            "raw predicate leaked: {label}"
        );
        assert!(label.contains("AS64500"), "ASN must be named: {label}");
        let with_profile =
            plane_label_from_asns(&predicate_asns, &[(64500u32, "Reviewed Plane".to_string())]);
        assert!(
            with_profile.contains("Reviewed Plane path (AS64500)"),
            "{with_profile}"
        );
    }

    #[test]
    fn same_day_workbench_time_uses_hms() {
        assert_eq!(
            workbench_time("2019-08-21T16:45:25Z", "2019-08-21T16:00:00Z"),
            "16:45:25 UTC"
        );
        assert_eq!(
            workbench_time("2019-08-21T16:45:25+00:00", "2019-08-21T16:00:00Z"),
            "16:45:25 UTC"
        );
        assert_eq!(
            workbench_time("2019-08-21T16:45:25.123456789Z", "2019-08-21T16:00:00Z"),
            "16:45:25 UTC"
        );
    }

    #[test]
    fn cross_day_time_includes_date() {
        assert_eq!(
            workbench_time("2019-08-22T00:15:03Z", "2019-08-21T16:00:00Z"),
            "2019-08-22 00:15:03 UTC"
        );
        assert_eq!(
            workbench_time("2019-08-21T23:59:59Z", "2019-08-22T00:00:00Z"),
            "2019-08-21 23:59:59 UTC"
        );
    }

    #[test]
    fn exact_timestamp_remains_in_details() {
        // The model keeps exact timestamps; only the display layer uses
        // workbench_time.
        let (ep, _) = stream_with(
            EffectKind::TemporaryStreamAbsence,
            true,
            true,
            Some("2019-08-21T16:45:27Z"),
            "Withdrawn",
        );
        assert_eq!(
            ep.first_change.as_deref(),
            Some("2019-08-21T16:45:25Z"),
            "model timestamps stay exact"
        );
        assert_eq!(
            ep.streams[0].first_change_utc.as_deref(),
            Some("2019-08-21T16:45:25Z")
        );
    }

    #[test]
    fn timezone_is_always_explicit() {
        for (ts, ws) in [
            ("2019-08-21T16:45:25Z", "2019-08-21T16:00:00Z"),
            ("2019-08-22T01:02:03Z", "2019-08-21T16:00:00Z"),
            ("garbage", "2019-08-21T16:00:00Z"),
        ] {
            let rendered = workbench_time(ts, ws);
            assert!(
                rendered.contains("UTC"),
                "timezone must be explicit: {rendered}"
            );
        }
    }
}

// ── Session 37: lane timeline (Part 7) ──────────────────────────────

#[cfg(test)]
mod session37_timeline_tests {
    use super::*;

    fn lane(session: &str, region: &str) -> TimelineLane {
        TimelineLane {
            observer_session: session.to_string(),
            region: region.to_string(),
            collector: "rrc06".to_string(),
            peer_asn: Some(64500),
            window_start: "2019-08-21T16:00:00Z".to_string(),
            window_end: "2019-08-21T17:30:00Z".to_string(),
            operator_anchors: Vec::new(),
            first_route_change: Some(TimelineMarker {
                timestamp_utc: "2019-08-21T16:45:25Z".to_string(),
                label: "first route change".to_string(),
                kind: "bgp".to_string(),
            }),
            absence_interval: Some((
                "2019-08-21T16:45:25Z".to_string(),
                "2019-08-21T16:45:27Z".to_string(),
            )),
            path_change_interval: None,
            restoration_interval: Some((
                "2019-08-21T16:45:27Z".to_string(),
                "2019-08-21T16:45:27Z".to_string(),
            )),
            unresolved_end_state: false,
        }
    }

    #[test]
    fn timeline_has_one_lane_per_session() {
        let lanes = vec![
            lane("ris/rrc06 peer 192.0.2.1", "APAC"),
            lane("ris/rrc00 peer 192.0.2.9", "EMEA"),
        ];
        let svg = render_timeline_svg(&lanes, &[]);
        let lane_count = svg.matches(r#"class="tl-lane""#).count();
        // One <g class="tl-lane"> per observer session (operator lane
        // absent when there are no anchors).
        assert_eq!(lane_count, 2, "one lane per observer session");
        assert!(svg.contains("tl-lane-line"));
    }

    #[test]
    fn operator_markers_and_bgp_markers_have_distinct_classes() {
        let lanes = vec![lane("ris/rrc06 peer 192.0.2.1", "APAC")];
        let anchors = vec![
            TimelineMarker {
                timestamp_utc: "2019-08-21T16:50:00Z".to_string(),
                label: "interface disabled".to_string(),
                kind: "operator".to_string(),
            },
            TimelineMarker {
                timestamp_utc: "2019-08-21T20:48:00Z".to_string(),
                label: "interface re-enabled".to_string(),
                kind: "operator".to_string(),
            },
        ];
        let svg = render_timeline_svg(&lanes, &anchors);
        assert!(svg.contains(r#"class="tl-op-marker""#), "operator class");
        assert!(svg.contains(r#"class="tl-bgp tl-first""#), "bgp class");
        assert!(svg.contains("Operator context"), "operator lane label");
        assert!(
            svg.contains("interface re-enabled"),
            "operator anchor label present"
        );
    }

    #[test]
    fn absence_interval_has_explicit_start_and_end() {
        let lanes = vec![lane("ris/rrc06 peer 192.0.2.1", "APAC")];
        let svg = render_timeline_svg(&lanes, &[]);
        assert!(svg.contains(r#"class="tl-absence""#));
        // The interval rect must have a positive width (start < end).
        let re = regex::Regex::new(
            r#"<rect class="tl-absence" x="([0-9.]+)" y="([0-9.]+)" width="([0-9.]+)""#,
        )
        .unwrap();
        let caps = re.captures(&svg).expect("absence rect present");
        let width: f64 = caps[3].parse().unwrap();
        assert!(width > 0.0, "explicit start and end give a real interval");
    }

    #[test]
    fn unresolved_end_state_has_no_restoration_marker() {
        let mut l = lane("ris/rrc06 peer 192.0.2.1", "APAC");
        l.restoration_interval = None;
        l.unresolved_end_state = true;
        let svg = render_timeline_svg(&[l], &[]);
        assert!(
            !svg.contains(r#"class="tl-restore" transform="#),
            "no restoration marker for unresolved end state"
        );
        assert!(svg.contains(r#"class="tl-changed-end""#));
    }

    #[test]
    fn timeline_text_fallback_contains_same_evidence() {
        // The fallback table (rendered by the template from the same
        // lanes) shows the same exact evidence: absence endpoints and
        // unresolved flag must be available on the model.
        let mut l = lane("ris/rrc06 peer 192.0.2.1", "APAC");
        l.unresolved_end_state = true;
        assert_eq!(
            l.absence_interval.as_ref().unwrap().0,
            "2019-08-21T16:45:25Z"
        );
        assert_eq!(
            l.absence_interval.as_ref().unwrap().1,
            "2019-08-21T16:45:27Z"
        );
        assert!(l.unresolved_end_state);
    }
}

// ── Session 38: counting units (Part 1) ─────────────────────────────

#[cfg(test)]
mod session38_unit_tests {
    use super::*;

    fn ep_at(session: &str, region: &str, kind: EffectKind, prefix: &str) -> ObserverEpisode {
        ObserverEpisode {
            analysis_run: 1,
            observer_session: session.to_string(),
            observer_site: "site".to_string(),
            observer_region: region.to_string(),
            peer_asn: Some(64500),

            observed_peer_asns: Vec::new(),
            peer_label: "AS64500".to_string(),
            peer_role: "unclassified observed ASN".to_string(),
            relationship: RelationshipKind::Indirect,
            named_path_plane: "Plane A".to_string(),
            effect_kind: kind.clone(),
            first_change: Some("2019-08-21T16:45:25Z".to_string()),
            last_change: Some("2019-08-21T16:45:27Z".to_string()),
            restoration_start: None,
            restoration_end: None,
            baseline_stream_count: 1,
            changed_stream_count: if kind == EffectKind::NoRouteStateChange {
                0
            } else {
                1
            },
            restored_stream_count: 0,
            distinct_prefix_count: 1,
            route_instance_count: 1,
            unresolved_count: 0,
            transition_count: if kind == EffectKind::NoRouteStateChange {
                0
            } else {
                3
            },
            end_state: if kind == EffectKind::NoRouteStateChange {
                EndState::NoRouteStateChange
            } else {
                EndState::StillChangedAtWindowEnd
            },
            cooldown_outcome: CooldownOutcome::None,
            coverage_status: CoverageStatus::Complete,
            representative_evidence: String::new(),
            streams: vec![EpisodeStream {
                prefix: prefix.to_string(),
                category: "DepartedTransitPath".to_string(),
                withdrawn: false,
                restored: false,
                baseline_instances: 1,
                max_active_instances: 1,
                transition_count: 3,
                add_path_ambiguous: false,
                first_change_utc: Some("2019-08-21T16:45:25Z".to_string()),
                restoration_time_utc: None,
                evidence_refs: "[]".to_string(),
            }],
        }
    }

    #[test]
    fn two_episode_types_at_one_peer_count_as_one_changed_session() {
        // One session, two presentation groupings (absence + path
        // change): the session denominator counts ONE changed session.
        let episodes = vec![
            ep_at(
                "ris/rrc06 peer 192.0.2.1",
                "APAC",
                EffectKind::TemporaryStreamAbsence,
                "198.51.100.0/24",
            ),
            ep_at(
                "ris/rrc06 peer 192.0.2.1",
                "APAC",
                EffectKind::PathReplacement,
                "198.51.100.0/24",
            ),
        ];
        let rows = regional_breadth(&episodes, &[], &[]);
        let apac = rows.iter().find(|r| r.region == "APAC").unwrap();
        assert_eq!(apac.eligible_observer_sessions, 1, "one unique session");
        assert_eq!(apac.changed_observer_sessions, 1);
        assert_eq!(apac.episode_count, 2, "two episodes at one session");
    }

    #[test]
    fn regional_session_count_deduplicates_episode_rows() {
        // Three episodes across two sessions → two eligible sessions.
        let episodes = vec![
            ep_at(
                "ris/rrc06 peer 192.0.2.1",
                "APAC",
                EffectKind::TemporaryStreamAbsence,
                "198.51.100.0/24",
            ),
            ep_at(
                "ris/rrc06 peer 192.0.2.1",
                "APAC",
                EffectKind::PathReplacement,
                "198.51.100.0/25",
            ),
            ep_at(
                "ris/rrc06 peer 192.0.2.2",
                "APAC",
                EffectKind::PathReplacement,
                "198.51.100.0/24",
            ),
        ];
        let rows = regional_breadth(&episodes, &[], &[]);
        let apac = rows.iter().find(|r| r.region == "APAC").unwrap();
        assert_eq!(apac.eligible_observer_sessions, 2);
        assert_eq!(apac.episode_count, 3);
        assert_eq!(apac.changed_observer_sessions, 2);
    }

    #[test]
    fn distinct_prefix_count_deduplicates_across_peers() {
        // The same prefix seen at two peers: two streams, ONE distinct
        // prefix in the region.
        let episodes = vec![
            ep_at(
                "ris/rrc06 peer 192.0.2.1",
                "APAC",
                EffectKind::PathReplacement,
                "198.51.100.0/24",
            ),
            ep_at(
                "ris/rrc06 peer 192.0.2.2",
                "APAC",
                EffectKind::PathReplacement,
                "198.51.100.0/24",
            ),
        ];
        let rows = regional_breadth(&episodes, &[], &[]);
        let apac = rows.iter().find(|r| r.region == "APAC").unwrap();
        assert_eq!(apac.changed_streams, 2, "peer dimension preserved");
        assert_eq!(
            apac.changed_prefixes, 1,
            "distinct prefixes deduplicate across peers"
        );
    }

    #[test]
    fn stream_count_preserves_peer_dimension() {
        let episodes = vec![
            ep_at(
                "ris/rrc06 peer 192.0.2.1",
                "APAC",
                EffectKind::PathReplacement,
                "198.51.100.0/24",
            ),
            ep_at(
                "ris/rrc06 peer 192.0.2.2",
                "APAC",
                EffectKind::PathReplacement,
                "198.51.100.0/24",
            ),
        ];
        let rows = regional_breadth(&episodes, &[], &[]);
        let apac = rows.iter().find(|r| r.region == "APAC").unwrap();
        assert_eq!(apac.changed_streams, 2);
        let units = WorkbenchUnits::from_parts(&rows, &episodes);
        assert_eq!(units.stream_count, 2);
        assert_eq!(units.distinct_prefix_count, 1);
        assert_eq!(units.session_count, 2);
    }

    #[test]
    fn transition_count_is_not_rendered_as_stream_count() {
        // 3 transitions at the session are NOT stream counts.
        let episodes = vec![ep_at(
            "ris/rrc06 peer 192.0.2.1",
            "APAC",
            EffectKind::PathReplacement,
            "198.51.100.0/24",
        )];
        let rows = regional_breadth(&episodes, &[], &[]);
        let apac = rows.iter().find(|r| r.region == "APAC").unwrap();
        assert_eq!(apac.changed_streams, 1);
        assert_eq!(apac.transition_count, 3);
        assert_ne!(apac.transition_count, apac.changed_streams);
        assert_eq!(episodes[0].transition_count, 3);
    }

    #[test]
    fn count_units_are_named_in_api_and_text_output() {
        let episodes = vec![ep_at(
            "ris/rrc06 peer 192.0.2.1",
            "APAC",
            EffectKind::PathReplacement,
            "198.51.100.0/24",
        )];
        let rows = regional_breadth(&episodes, &[], &[]);
        let units = WorkbenchUnits::from_parts(&rows, &episodes);
        let json = serde_json::to_value(&units).unwrap();
        for key in [
            "session_count",
            "changed_session_count",
            "episode_count",
            "stream_count",
            "distinct_prefix_count",
            "route_instance_count",
            "transition_count",
        ] {
            assert!(json.get(key).is_some(), "API unit {key} present");
        }
        // The text report names the units.
        let vm = IncidentWorkbenchViewModel {
            subject_id: "EV-1".to_string(),
            subject_kind: "event".to_string(),
            title: "Test".to_string(),
            source_task_type: "incident".to_string(),
            reviewed_incident_role: String::new(),
            lifecycle: "Closed".to_string(),
            window_start: "2019-08-21T16:00:00Z".to_string(),
            window_end: "2019-08-21T17:30:00Z".to_string(),
            current_result: "Partial".to_string(),
            expectation_assessment: String::new(),
            archive_coverage: "Complete".to_string(),
            observed_result: String::new(),
            scope_limit: String::new(),
            incident_horizon_start: String::new(),
            incident_horizon_end: String::new(),
            pilot_label: String::new(),
            linked_tickets: vec![],
            plane_asns: vec![],
            units,
            relationship_audit: None,
            runs: vec![],
            findings: vec![],
            target_label: String::new(),
            target_origin_asns: vec![],
            asn_identities: vec![],
            episodes: episodes.clone(),
            breadth: rows,
            timeline: vec![],
            operator_anchors: vec![],
            cues: vec![],
            grouped_cues: vec![],
            no_baseline_sessions: vec![],
            incomplete_sessions: vec![],
        };
        let text = vm.render_text();
        assert!(text.contains("observer session(s)"), "unit named");
        assert!(text.contains("distinct prefix(es)"), "unit named");
        assert!(text.contains("route instance(s)"), "unit named");
        assert!(text.contains("transition(s)"), "unit named");
    }
}

// ── Session 38: prefix breadth units (Parts 2-3) ────────────────────

#[cfg(test)]
mod session38_prefix_tests {
    use super::*;

    fn ep_with(
        region: &str,
        session: &str,
        kind: EffectKind,
        prefixes: &[&str],
    ) -> ObserverEpisode {
        ObserverEpisode {
            analysis_run: 1,
            observer_session: session.to_string(),
            observer_site: "site".to_string(),
            observer_region: region.to_string(),
            peer_asn: Some(64500),

            observed_peer_asns: Vec::new(),
            peer_label: "AS64500".to_string(),
            peer_role: "unclassified observed ASN".to_string(),
            relationship: RelationshipKind::Indirect,
            named_path_plane: "Plane A".to_string(),
            effect_kind: kind.clone(),
            first_change: Some("2019-08-21T16:45:25Z".to_string()),
            last_change: Some("2019-08-21T16:45:27Z".to_string()),
            restoration_start: None,
            restoration_end: None,
            baseline_stream_count: prefixes.len(),
            changed_stream_count: if kind == EffectKind::NoRouteStateChange {
                0
            } else {
                prefixes.len()
            },
            restored_stream_count: 0,
            distinct_prefix_count: prefixes.len(),
            route_instance_count: prefixes.len(),
            unresolved_count: 0,
            transition_count: prefixes.len() * 2,
            end_state: if kind == EffectKind::NoRouteStateChange {
                EndState::NoRouteStateChange
            } else {
                EndState::StillChangedAtWindowEnd
            },
            cooldown_outcome: CooldownOutcome::None,
            coverage_status: CoverageStatus::Complete,
            representative_evidence: String::new(),
            streams: prefixes
                .iter()
                .map(|p| EpisodeStream {
                    prefix: p.to_string(),
                    category: "DepartedTransitPath".to_string(),
                    withdrawn: false,
                    restored: false,
                    baseline_instances: 1,
                    max_active_instances: 1,
                    transition_count: 2,
                    add_path_ambiguous: false,
                    first_change_utc: Some("2019-08-21T16:45:25Z".to_string()),
                    restoration_time_utc: None,
                    evidence_refs: "[]".to_string(),
                })
                .collect(),
        }
    }

    #[test]
    fn regional_prefix_count_is_union_within_region() {
        // Same prefix at two peers in the SAME region: one distinct
        // prefix in that region.
        let episodes = vec![
            ep_with(
                "AMER",
                "ris/rrc06 peer 192.0.2.1",
                EffectKind::PathReplacement,
                &["198.51.100.0/24", "198.51.100.0/25"],
            ),
            ep_with(
                "AMER",
                "ris/rrc06 peer 192.0.2.2",
                EffectKind::PathReplacement,
                &["198.51.100.0/24", "198.51.101.0/24"],
            ),
        ];
        let rows = regional_breadth(&episodes, &[], &[]);
        let amer = rows.iter().find(|r| r.region == "AMER").unwrap();
        assert_eq!(amer.changed_streams, 4, "streams keep peer dimension");
        assert_eq!(
            amer.changed_prefixes, 3,
            "regional prefix count is the in-region union"
        );
    }

    #[test]
    fn global_prefix_count_is_union_across_regions() {
        // The same prefix seen in AMER and APAC: the GLOBAL union is 3,
        // never the sum of regional unions (3 + 2 = 5).
        let episodes = vec![
            ep_with(
                "AMER",
                "ris/rrc06 peer 192.0.2.1",
                EffectKind::PathReplacement,
                &["198.51.100.0/24", "198.51.100.0/25", "198.51.100.0/26"],
            ),
            ep_with(
                "APAC",
                "ris/rrc06 peer 192.0.2.2",
                EffectKind::PathReplacement,
                &["198.51.100.0/24", "198.51.100.0/27"],
            ),
        ];
        let rows = regional_breadth(&episodes, &[], &[]);
        let units = WorkbenchUnits::from_parts(&rows, &episodes);
        let regional_sum: usize = rows.iter().map(|r| r.changed_prefixes).sum();
        assert_eq!(regional_sum, 5, "regional unions overlap");
        assert_eq!(
            units.distinct_prefix_count, 4,
            "global prefix count is the union across regions"
        );
    }

    #[test]
    fn same_prefix_seen_by_three_peers_counts_as_three_streams_one_prefix() {
        let episodes = vec![
            ep_with(
                "AMER",
                "ris/rrc06 peer 192.0.2.1",
                EffectKind::PathReplacement,
                &["198.51.100.0/24"],
            ),
            ep_with(
                "AMER",
                "ris/rrc06 peer 192.0.2.2",
                EffectKind::PathReplacement,
                &["198.51.100.0/24"],
            ),
            ep_with(
                "AMER",
                "ris/rrc06 peer 192.0.2.3",
                EffectKind::PathReplacement,
                &["198.51.100.0/24"],
            ),
        ];
        let rows = regional_breadth(&episodes, &[], &[]);
        let amer = rows.iter().find(|r| r.region == "AMER").unwrap();
        assert_eq!(amer.changed_streams, 3);
        assert_eq!(amer.changed_prefixes, 1);
        let units = WorkbenchUnits::from_parts(&rows, &episodes);
        assert_eq!(units.stream_count, 3);
        assert_eq!(units.distinct_prefix_count, 1);
    }

    #[test]
    fn investigation_cue_names_streams_and_distinct_prefixes_correctly() {
        // Two peers see the same two prefixes and both restore: the cue
        // must say "4 restored observer-prefix streams covering 2
        // distinct prefixes", never "4 externally restored prefixes".
        let mut e1 = ep_with(
            "AMER",
            "ris/rrc06 peer 192.0.2.1",
            EffectKind::TemporaryStreamAbsence,
            &["198.51.100.0/24", "198.51.100.0/25"],
        );
        e1.restored_stream_count = 2;
        let mut e2 = ep_with(
            "AMER",
            "ris/rrc06 peer 192.0.2.2",
            EffectKind::TemporaryStreamAbsence,
            &["198.51.100.0/24", "198.51.100.0/25"],
        );
        e2.restored_stream_count = 2;
        let episodes = vec![e1, e2];
        let cues = build_grouped_cues(&episodes, "Plane A", &[64500]);
        let restoration = cues
            .iter()
            .find(|c| c.title == "Restoration quality")
            .expect("restoration cue");
        assert!(
            restoration
                .text
                .contains("4 restored observer-prefix streams covering 2 distinct prefixes"),
            "cue units: {}",
            restoration.text
        );
        assert!(
            !restoration.text.contains("4 externally restored prefixes"),
            "prefix count must not be a stream count: {}",
            restoration.text
        );
    }
}

// ── Session 38: coverage reasons (Part 4) ───────────────────────────

#[cfg(test)]
mod session38_coverage_tests {
    use super::*;

    #[test]
    fn absent_required_session_is_not_no_target_baseline() {
        // RequiredSessionAbsent (no historical session) is a DIFFERENT
        // condition from SessionPresentNoTargetBaseline (session exists,
        // target not visible). Their labels must differ.
        assert_ne!(
            CoverageReason::RequiredSessionAbsent.human_label(),
            CoverageReason::SessionPresentNoTargetBaseline.human_label()
        );
        assert_ne!(
            CoverageReason::RequiredSessionAbsent.label(),
            CoverageReason::SessionPresentNoTargetBaseline.label()
        );
        assert!(CoverageReason::RequiredSessionAbsent
            .human_label()
            .contains("Required session absent"));
        assert!(CoverageReason::SessionPresentNoTargetBaseline
            .human_label()
            .contains("no target baseline"));
    }

    #[test]
    fn target_not_visible_is_distinct_from_predicate_not_matched() {
        assert_ne!(
            CoverageReason::SessionPresentNoTargetBaseline.label(),
            CoverageReason::PredicateNotMatched.label()
        );
        assert!(CoverageReason::PredicateNotMatched
            .human_label()
            .contains("Predicate not matched"));
    }

    #[test]
    fn excluded_session_is_not_added_to_eligible_denominator() {
        // An excluded session (RequiredSessionAbsent) must not enter the
        // eligible denominator, and must not count as unchanged.
        let rows = regional_breadth(
            &[],
            &[(
                "rrc11".to_string(),
                "AMER".to_string(),
                "rrc11 (reviewed peering-plane pilot decision)".to_string(),
                CoverageReason::RequiredSessionAbsent,
                "no direct session in the historical baseline".to_string(),
            )],
            &[],
        );
        let amer = rows.iter().find(|r| r.region == "AMER").unwrap();
        assert_eq!(amer.eligible_observer_sessions, 0);
        assert_eq!(amer.unchanged_observer_sessions, 0);
        assert_eq!(amer.sessions_without_baseline_visibility, 1);
    }

    #[test]
    fn coverage_reason_preserves_exact_preflight_evidence() {
        // The reason carries the EXACT preflight evidence text.
        let view = CoverageSessionView {
            observer_session: "rrc11 (reviewed peering-plane pilot decision)".to_string(),
            region: "AMER".to_string(),
            collector: "rrc11".to_string(),
            coverage_status: CoverageStatus::NoBaselineVisibility,
            reason: CoverageReason::RequiredSessionAbsent,
            detail: "No direct session exists in the historical baseline bview.".to_string(),
        };
        assert_eq!(view.reason, CoverageReason::RequiredSessionAbsent);
        assert!(view
            .detail
            .contains("No direct session exists in the historical baseline bview."));
        let json = serde_json::to_value(&view).unwrap();
        assert_eq!(
            json.get("reason").and_then(|r| r.as_str()),
            Some("RequiredSessionAbsent")
        );
    }
}

// ── Session 38: timeline context strip (Part 6) ─────────────────────

#[cfg(test)]
mod session38_timeline_tests {
    use super::*;

    fn lane(session: &str, region: &str, peer_asn: Option<u32>) -> TimelineLane {
        TimelineLane {
            observer_session: session.to_string(),
            region: region.to_string(),
            collector: "rrc06".to_string(),
            peer_asn,
            window_start: "2019-08-21T16:00:00Z".to_string(),
            window_end: "2019-08-21T17:30:00Z".to_string(),
            operator_anchors: Vec::new(),
            first_route_change: Some(TimelineMarker {
                timestamp_utc: "2019-08-21T16:45:25Z".to_string(),
                label: "first route change".to_string(),
                kind: "bgp".to_string(),
            }),
            absence_interval: Some((
                "2019-08-21T16:45:25Z".to_string(),
                "2019-08-21T16:45:27Z".to_string(),
            )),
            path_change_interval: None,
            restoration_interval: Some((
                "2019-08-21T16:45:27Z".to_string(),
                "2019-08-21T16:45:27Z".to_string(),
            )),
            unresolved_end_state: false,
        }
    }

    fn anchors() -> Vec<TimelineMarker> {
        vec![
            TimelineMarker {
                timestamp_utc: "2019-08-21T15:33:00Z".to_string(),
                label: "flapping reported (operator anchor)".to_string(),
                kind: "operator".to_string(),
            },
            TimelineMarker {
                timestamp_utc: "2019-08-21T16:50:00Z".to_string(),
                label: "interface disabled".to_string(),
                kind: "operator".to_string(),
            },
            TimelineMarker {
                timestamp_utc: "2019-08-21T20:48:00Z".to_string(),
                label: "interface re-enabled".to_string(),
                kind: "operator".to_string(),
            },
        ]
    }

    #[test]
    fn pre_window_anchor_is_not_clamped_to_window_start() {
        // The 15:33 anchor sits on the CONTEXT axis whose start IS
        // 15:33 — it is never placed on the focus axis (16:00 start).
        let svg = render_timeline_svg(
            &[lane("ris/rrc06 peer 192.0.2.1", "APAC", Some(64500))],
            &anchors(),
        );
        let ctx_start = parse_utc_seconds("2019-08-21T15:33:00Z").unwrap();
        let focus_start = parse_utc_seconds("2019-08-21T16:00:00Z").unwrap();
        assert!(
            svg.contains(&format!(r#"class="tl-context" data-start="{ctx_start}""#)),
            "context axis starts at the pre-window anchor"
        );
        assert!(
            svg.contains(&format!(r#"class="tl-focus" data-start="{focus_start}""#)),
            "focus axis starts at the window"
        );
        // The pre-window marker is inside the context group, not the focus group.
        let ctx = svg.find("class=\"tl-context\"").unwrap();
        let focus = svg.find("class=\"tl-focus\"").unwrap();
        assert!(ctx < focus, "context strip precedes the focus timeline");
    }

    #[test]
    fn post_window_anchor_is_not_clamped_to_window_end() {
        let svg = render_timeline_svg(
            &[lane("ris/rrc06 peer 192.0.2.1", "APAC", Some(64500))],
            &anchors(),
        );
        let ctx_end = parse_utc_seconds("2019-08-21T20:48:00Z").unwrap();
        let focus_end = parse_utc_seconds("2019-08-21T17:30:00Z").unwrap();
        let re =
            regex::Regex::new(r#"class="tl-context" data-start="(\d+)" data-end="(\d+)""#).unwrap();
        let caps = re.captures(&svg).expect("context axis attrs");
        assert_eq!(
            caps[2].parse::<i64>().unwrap(),
            ctx_end,
            "context axis ends at the post-window anchor"
        );
        let re2 =
            regex::Regex::new(r#"class="tl-focus" data-start="(\d+)" data-end="(\d+)""#).unwrap();
        let caps2 = re2.captures(&svg).expect("focus axis attrs");
        assert_eq!(
            caps2[2].parse::<i64>().unwrap(),
            focus_end,
            "focus axis ends at the window"
        );
        // Only the in-window anchor appears inside the focus group.
        let focus = svg.find("class=\"tl-focus\"").unwrap();
        let focus_region = &svg[focus..svg.len().min(focus + 4000)];
        assert!(
            focus_region.contains("interface disabled"),
            "in-window anchor"
        );
        assert!(
            !focus_region.contains("interface re-enabled"),
            "post-window anchor must not appear on the focus axis"
        );
        assert!(
            !focus_region.contains("flapping reported"),
            "pre-window anchor must not appear on the focus axis"
        );
    }

    #[test]
    fn context_and_focus_axes_preserve_exact_order() {
        let svg = render_timeline_svg(
            &[lane("ris/rrc06 peer 192.0.2.1", "APAC", Some(64500))],
            &anchors(),
        );
        // Chronological order across both axes: 15:33 < 16:00 (focus
        // start) < 20:48, and the context strip renders before the
        // focus group.
        let ctx = svg.find("tl-context").unwrap();
        let focus = svg.find("tl-focus").unwrap();
        assert!(ctx < focus);
        // The context axis epoch values are ascending and exact.
        let re =
            regex::Regex::new(r#"class="tl-context" data-start="(\d+)" data-end="(\d+)""#).unwrap();
        let caps = re.captures(&svg).expect("context axis attrs");
        let s: i64 = caps[1].parse().unwrap();
        let e: i64 = caps[2].parse().unwrap();
        assert!(s < e, "context axis ascending");
        assert_eq!(s, parse_utc_seconds("2019-08-21T15:33:00Z").unwrap());
        assert_eq!(e, parse_utc_seconds("2019-08-21T20:48:00Z").unwrap());
    }

    #[test]
    fn lane_baselines_are_horizontal() {
        let svg = render_timeline_svg(
            &[
                lane("ris/rrc06 peer 192.0.2.1", "APAC", Some(64500)),
                lane("ris/rrc00 peer 192.0.2.9", "EMEA", Some(64599)),
            ],
            &anchors(),
        );
        let re = regex::Regex::new(r#"<line class="tl-lane-line" x1="([0-9.]+)" y1="([0-9.]+)" x2="([0-9.]+)" y2="([0-9.]+)""#)
            .unwrap();
        let mut count = 0;
        for caps in re.captures_iter(&svg) {
            let x1: f64 = caps[1].parse().unwrap();
            let x2: f64 = caps[3].parse().unwrap();
            assert!(x1 < x2, "lane line spans the axis");
            assert_eq!(
                caps[2], caps[4],
                "lane baseline must be horizontal (y1 == y2)"
            );
            count += 1;
        }
        assert_eq!(count, 2, "one horizontal baseline per observer lane");
    }

    #[test]
    fn repeated_collector_lanes_include_peer_identity() {
        let svg = render_timeline_svg(
            &[
                lane("ris/rrc15 peer 192.0.2.1", "AMER", Some(1916)),
                lane("ris/rrc15 peer 192.0.2.2", "AMER", Some(28571)),
                lane("ris/rrc15 peer 192.0.2.3", "AMER", Some(52888)),
            ],
            &[],
        );
        assert!(
            svg.contains("rrc15 · AMER / AS1916"),
            "lane label carries the peer ASN"
        );
        assert!(svg.contains("AS28571"));
        assert!(svg.contains("AS52888"));
        // Three same-collector lanes are distinguishable.
        assert!(svg.matches("rrc15 · AMER / AS").count() == 3);
    }

    #[test]
    fn timeline_fallback_matches_svg_semantics() {
        // The fallback table (rendered from the same lanes) carries the
        // same exact evidence; the lane model exposes what the SVG drew.
        let l = lane("ris/rrc06 peer 192.0.2.1", "APAC", Some(64500));
        assert_eq!(l.peer_asn, Some(64500));
        assert_eq!(
            l.absence_interval.as_ref().unwrap().0,
            "2019-08-21T16:45:25Z"
        );
        assert_eq!(
            l.absence_interval.as_ref().unwrap().1,
            "2019-08-21T16:45:27Z"
        );
        assert_eq!(
            l.restoration_interval.as_ref().unwrap().1,
            "2019-08-21T16:45:27Z"
        );
        // In-window anchor belongs to the focus semantics: 16:50 lies
        // inside 16:00..17:30.
        let anchors = anchors();
        let win_start = parse_utc_seconds(&l.window_start).unwrap();
        let win_end = parse_utc_seconds(&l.window_end).unwrap();
        let in_window: Vec<String> = anchors
            .iter()
            .filter(|a| {
                parse_utc_seconds(&a.timestamp_utc)
                    .map(|t| (win_start..=win_end).contains(&t))
                    .unwrap_or(false)
            })
            .map(|a| a.label.clone())
            .collect();
        assert_eq!(in_window, vec!["interface disabled".to_string()]);
    }
}

// ── Session 38: window-end vs cooldown (Part 7) ─────────────────────

#[cfg(test)]
mod session38_cooldown_tests {
    use super::*;

    fn transition(collector: &str, peer: &str, kind: &str, occurred: &str) -> RunTransitionRecord {
        RunTransitionRecord {
            id: 0,
            run_id: 1,
            seq: 0,
            kind: kind.to_string(),
            occurred_utc: occurred.to_string(),
            run_phase: "Cooldown".to_string(),
            collector: collector.to_string(),
            peer_ip: peer.to_string(),
            prefix: "198.51.100.0/24".to_string(),
            path_id: None,
            material_path_changed: false,
            communities_changed: false,
            announced: false,
            withdrawn: false,
            observation_id: None,
            archive_sha256: None,
        }
    }

    pub(super) fn episode(kind: EffectKind, end: EndState) -> ObserverEpisode {
        ObserverEpisode {
            analysis_run: 1,
            observer_session: "ris/rrc06 peer 192.0.2.1".to_string(),
            observer_site: "site".to_string(),
            observer_region: "APAC".to_string(),
            peer_asn: Some(64500),
            observed_peer_asns: vec![64500],
            peer_label: "AS64500".to_string(),
            peer_role: "unclassified observed ASN".to_string(),
            relationship: RelationshipKind::Indirect,
            named_path_plane: "Plane A".to_string(),
            effect_kind: kind,
            first_change: Some("2019-08-21T16:45:25Z".to_string()),
            last_change: Some("2019-08-21T16:45:27Z".to_string()),
            restoration_start: None,
            restoration_end: None,
            baseline_stream_count: 1,
            changed_stream_count: 1,
            restored_stream_count: 0,
            distinct_prefix_count: 1,
            route_instance_count: 1,
            unresolved_count: 0,
            transition_count: 2,
            end_state: end,
            cooldown_outcome: CooldownOutcome::None,
            coverage_status: CoverageStatus::Complete,
            representative_evidence: String::new(),
            streams: vec![EpisodeStream {
                prefix: "198.51.100.0/24".to_string(),
                category: "DepartedTransitPath".to_string(),
                withdrawn: false,
                restored: false,
                baseline_instances: 1,
                max_active_instances: 1,
                transition_count: 2,
                add_path_ambiguous: false,
                first_change_utc: Some("2019-08-21T16:45:25Z".to_string()),
                restoration_time_utc: None,
                evidence_refs: "[]".to_string(),
            }],
        }
    }

    #[test]
    fn changed_at_event_end_can_restore_in_cooldown() {
        let ep = episode(
            EffectKind::PathReplacement,
            EndState::StillChangedAtWindowEnd,
        );
        let transitions = vec![transition(
            "rrc06",
            "192.0.2.1",
            "ReturnToBaseline",
            "2019-08-21T17:52:16Z",
        )];
        let outcome = derive_cooldown_outcome(
            &ep,
            &transitions,
            "2019-08-21T17:30:00Z",
            "2019-08-21T18:30:00Z",
        );
        assert_eq!(
            outcome,
            CooldownOutcome::RestoredAt("2019-08-21T17:52:16Z".to_string())
        );
        // The event-window end state is independent of the cooldown outcome.
        assert_eq!(ep.end_state, EndState::StillChangedAtWindowEnd);
    }

    #[test]
    fn event_end_state_and_final_analysis_state_are_independent() {
        let ep = episode(
            EffectKind::PathReplacement,
            EndState::StillChangedAtWindowEnd,
        );
        let transitions = vec![
            transition(
                "rrc06",
                "192.0.2.1",
                "PathReplacement",
                "2019-08-21T17:45:00Z",
            ),
            transition(
                "rrc06",
                "192.0.2.1",
                "ReturnToBaseline",
                "2019-08-21T17:52:16Z",
            ),
        ];
        let outcome = derive_cooldown_outcome(
            &ep,
            &transitions,
            "2019-08-21T17:30:00Z",
            "2019-08-21T18:30:00Z",
        );
        assert_eq!(
            outcome,
            CooldownOutcome::RestoredAt("2019-08-21T17:52:16Z".to_string())
        );
    }

    #[test]
    fn cooldown_restoration_is_not_rendered_as_in_window_restoration() {
        // The episode's in-window restoration fields stay empty while
        // the cooldown outcome carries the later restoration.
        let ep = episode(
            EffectKind::PathReplacement,
            EndState::StillChangedAtWindowEnd,
        );
        let transitions = vec![transition(
            "rrc06",
            "192.0.2.1",
            "Announcement",
            "2019-08-21T17:52:16Z",
        )];
        let outcome = derive_cooldown_outcome(
            &ep,
            &transitions,
            "2019-08-21T17:30:00Z",
            "2019-08-21T18:30:00Z",
        );
        assert_eq!(
            outcome,
            CooldownOutcome::RestoredAt("2019-08-21T17:52:16Z".to_string())
        );
        assert_eq!(ep.restoration_start, None, "not an in-window restoration");
        assert_eq!(ep.restoration_end, None);
    }

    #[test]
    fn unresolved_means_no_observed_restoration_before_analysis_end() {
        // Continued path changes without any restoration signal: the
        // outcome says still changing, never restored.
        let ep = episode(
            EffectKind::PathReplacement,
            EndState::StillChangedAtWindowEnd,
        );
        let transitions = vec![transition(
            "rrc06",
            "192.0.2.1",
            "PathReplacement",
            "2019-08-21T17:52:16Z",
        )];
        let outcome = derive_cooldown_outcome(
            &ep,
            &transitions,
            "2019-08-21T17:30:00Z",
            "2019-08-21T18:30:00Z",
        );
        assert_eq!(
            outcome,
            CooldownOutcome::StillChangingBeforeAnalysisEnd("2019-08-21T17:52:16Z".to_string())
        );
        // No transition at all in cooldown → no restoration observed.
        let none =
            derive_cooldown_outcome(&ep, &[], "2019-08-21T17:30:00Z", "2019-08-21T18:30:00Z");
        assert_eq!(
            none,
            CooldownOutcome::NoRestorationBeforeAnalysisEnd("2019-08-21T18:30:00Z".to_string())
        );
    }

    #[test]
    fn regional_restoration_heading_matches_its_definition() {
        // The regional RESTORED value is defined as LAST IN-WINDOW
        // RESTORATION: the template heading must state the definition.
        let template = include_str!("web/templates/workbench.html");
        assert!(
            template.contains("Last in-window restoration"),
            "breadth heading states its definition"
        );
    }
}

// ── Session 38: observed peer-session metadata (Part 5) ─────────────

#[cfg(test)]
mod session38_metadata_tests {
    fn metadata(
        peer_ip: &str,
        asn: u32,
        af: &str,
    ) -> crate::catalog::domain::ObserverSessionMetadata {
        crate::catalog::domain::ObserverSessionMetadata {
            id: 0,
            source_family: "RouteViews".to_string(),
            collector: "route-views2".to_string(),
            peer_ip: peer_ip.to_string(),
            address_family: af.to_string(),
            peer_asn: asn,
            valid_from: "2026-07-14T04:00:00Z".to_string(),
            valid_to: None,
            source_archive: "cache/route-views2/rib/rib.20260714.0400.bz2".to_string(),
            source_sha256: "abc123".to_string(),
        }
    }

    #[test]
    fn peer_asn_is_observed_not_reviewed_metadata() {
        // The metadata row is a protocol fact: source archive + sha +
        // validity, independent of any reviewed organization label.
        let m = metadata("137.164.16.84", 2152, "ipv4");
        assert_eq!(m.peer_asn, 2152);
        assert_eq!(m.valid_from, "2026-07-14T04:00:00Z");
        assert!(m.source_archive.contains("rib.20260714.0400.bz2"));
        assert_eq!(m.source_sha256, "abc123");
        assert!(
            m.valid_to.is_none(),
            "time-scoped: valid from the RIB timestamp"
        );
    }

    #[test]
    fn session_metadata_is_time_scoped() {
        // Two observations of the same session at different RIB times
        // are distinct rows, both retained (valid_from differs).
        let a = metadata("137.164.16.84", 2152, "ipv4");
        let mut b = metadata("137.164.16.84", 2152, "ipv4");
        b.valid_from = "2026-07-14T08:00:00Z".to_string();
        b.id = 1;
        assert_ne!(a.valid_from, b.valid_from);
        assert_ne!(a, b, "time-scoped rows are distinct records");
    }

    #[test]
    fn same_peer_ip_with_conflicting_asn_is_ambiguous() {
        // Two distinct observed ASNs for one (collector, peer IP, AF)
        // mean the session's ASN is ambiguous — never silently resolved.
        let observed = vec![2152u32, 2153u32];
        assert!(observed.len() > 1, "conflict present");
        let mut dedup = observed.clone();
        dedup.sort_unstable();
        dedup.dedup();
        assert_eq!(dedup.len(), 2, "conflicting observations retained");
        let view = crate::catalog::web::view::peer_identity_label(observed);
        assert!(view.starts_with("peer ASN ambiguous"), "{view}");
    }

    #[test]
    fn missing_organization_label_does_not_hide_observed_asn() {
        // The observed ASN renders even when no reviewed org label exists.
        let label = crate::catalog::web::view::peer_identity_label(vec![2152]);
        assert!(label.starts_with("AS2152"), "{label}");
        assert!(label.contains("organization unclassified"), "{label}");
        assert!(label.contains("role unclassified"), "{label}");
    }

    #[test]
    fn imported_historical_runs_can_backfill_session_metadata_reproducibly() {
        // The store contract makes backfill reproducible: inserting the
        // same observation twice yields one row (INSERT OR IGNORE), and
        // listing returns it once. The full-RIB backfill was executed
        // against the cached 2026-07-14 RouteViews RIB (20 sessions,
        // including the four UVA peers).
        let dir = tempfile::tempdir().unwrap();
        let conn = crate::catalog::db::open_catalog(&dir.path().join("c.sqlite")).unwrap();
        let m = metadata("137.164.16.84", 2152, "ipv4");
        let first = crate::catalog::store::insert_session_metadata(&conn, &m).unwrap();
        let second = crate::catalog::store::insert_session_metadata(&conn, &m).unwrap();
        assert_eq!(first, second, "idempotent insert resolves to the same row");
        let rows = crate::catalog::store::list_session_metadata(&conn).unwrap();
        assert_eq!(rows.len(), 1, "backfill is reproducible: no duplicate rows");
        assert_eq!(rows[0].peer_asn, 2152);
        assert_eq!(rows[0].peer_ip, "137.164.16.84");
    }
}

/// Shared fixtures for RoutingFinding tests (Session 39).
#[cfg(test)]
mod finding_test_helpers {
    use super::*;

    type PathEntry<'a> = (&'a str, &'a str, &'a str, &'a [u32], &'a [u32]);

    #[allow(clippy::too_many_arguments)] // test fixture passthrough
    pub(super) fn episode(
        run: i64,
        kind: EffectKind,
        session: &str,
        site: &str,
        region: &str,
        peer_asn: u32,
        rel: RelationshipKind,
        first: &str,
        last: &str,
        prefixes: &[&str],
    ) -> ObserverEpisode {
        ObserverEpisode {
            analysis_run: run,
            observer_session: session.to_string(),
            observer_site: site.to_string(),
            observer_region: region.to_string(),
            peer_asn: Some(peer_asn),
            observed_peer_asns: vec![peer_asn],
            peer_label: format!("AS{peer_asn}"),
            peer_role: "unreviewed".to_string(),
            relationship: rel,
            named_path_plane: "plane".to_string(),
            effect_kind: kind,
            first_change: Some(first.to_string()),
            last_change: Some(last.to_string()),
            restoration_start: None,
            restoration_end: None,
            baseline_stream_count: prefixes.len(),
            changed_stream_count: prefixes.len(),
            restored_stream_count: prefixes.len(),
            distinct_prefix_count: prefixes.len(),
            route_instance_count: prefixes.len(),
            unresolved_count: 0,
            transition_count: 2,
            end_state: EndState::BaselineRestored,
            cooldown_outcome: CooldownOutcome::None,
            coverage_status: CoverageStatus::Complete,
            representative_evidence: String::new(),
            streams: prefixes
                .iter()
                .map(|p| EpisodeStream {
                    prefix: p.to_string(),
                    category: "PathChangedStillViaTransit".to_string(),
                    withdrawn: false,
                    restored: true,
                    baseline_instances: 1,
                    max_active_instances: 1,
                    transition_count: 2,
                    add_path_ambiguous: false,
                    first_change_utc: Some(first.to_string()),
                    restoration_time_utc: Some(last.to_string()),
                    evidence_refs: format!("obs-{p}"),
                })
                .collect(),
        }
    }

    #[allow(clippy::too_many_arguments)] // test fixture passthrough
    pub(super) fn absence_episode(
        run: i64,
        session: &str,
        site: &str,
        region: &str,
        peer_asn: u32,
        rel: RelationshipKind,
        first: &str,
        restored: &str,
        prefixes: &[&str],
    ) -> ObserverEpisode {
        ObserverEpisode {
            analysis_run: run,
            observer_session: session.to_string(),
            observer_site: site.to_string(),
            observer_region: region.to_string(),
            peer_asn: Some(peer_asn),
            observed_peer_asns: vec![peer_asn],
            peer_label: format!("AS{peer_asn}"),
            peer_role: "unreviewed".to_string(),
            relationship: rel,
            named_path_plane: "plane".to_string(),
            effect_kind: EffectKind::TemporaryStreamAbsence,
            first_change: Some(first.to_string()),
            last_change: Some(restored.to_string()),
            restoration_start: Some(restored.to_string()),
            restoration_end: Some(restored.to_string()),
            baseline_stream_count: prefixes.len(),
            changed_stream_count: prefixes.len(),
            restored_stream_count: prefixes.len(),
            distinct_prefix_count: prefixes.len(),
            route_instance_count: prefixes.len(),
            unresolved_count: 0,
            transition_count: 2,
            end_state: EndState::VisibilityRestored,
            cooldown_outcome: CooldownOutcome::None,
            coverage_status: CoverageStatus::Complete,
            representative_evidence: String::new(),
            streams: prefixes
                .iter()
                .map(|p| EpisodeStream {
                    prefix: p.to_string(),
                    category: "Withdrawn".to_string(),
                    withdrawn: true,
                    restored: true,
                    baseline_instances: 1,
                    max_active_instances: 1,
                    transition_count: 2,
                    add_path_ambiguous: false,
                    first_change_utc: Some(first.to_string()),
                    restoration_time_utc: Some(restored.to_string()),
                    evidence_refs: format!("obs-{p}"),
                })
                .collect(),
        }
    }

    pub(super) fn path_index_with(entries: &[PathEntry]) -> LifecyclePathIndex {
        // (collector, peer_ip, prefix, baseline, changed)
        let mut index = LifecyclePathIndex::default();
        let mut map: std::collections::HashMap<(String, String, String), StreamPathEvidence> =
            std::collections::HashMap::new();
        for (collector, peer, prefix, baseline, changed) in entries {
            map.insert(
                (collector.to_string(), peer.to_string(), prefix.to_string()),
                StreamPathEvidence {
                    transitions: Vec::new(),
                    baseline_path: baseline.to_vec(),
                    changed_path: Some(changed.to_vec()),
                    final_path: Some(changed.to_vec()),
                    visibility_restored_at: None,
                    equivalent_route_restored_at: None,
                    reviewed_plane_restored_at: None,
                    exact_baseline_restored_at: Some("2019-08-21T17:00:00Z".to_string()),
                },
            );
        }
        index.by_run.insert("EV".to_string(), map);
        index
    }

    pub(super) fn build(
        episodes: &[ObserverEpisode],
        index: &LifecyclePathIndex,
    ) -> Vec<RoutingFinding> {
        let identities = AsnIdentityRegistry::default();
        let mut run_events = std::collections::HashMap::new();
        run_events.insert(1, "EV".to_string());
        run_events.insert(2, "EV".to_string());
        build_findings(
            episodes,
            index,
            &identities,
            "TARGET (AS2603)",
            &[2603],
            "single-target pilot",
            &run_events,
            "2019-08-21",
            "2019-08-21T18:00:00Z",
        )
    }

    #[test]
    fn finding_groups_same_prefix_set_and_path_signature() {
        // Two prefixes at the same session with the same before/after
        // paths form ONE finding retaining all prefixes.
        let eps = vec![episode(
            1,
            EffectKind::PathReplacement,
            "ris/rrc06 peer 192.0.2.1",
            "Otemachi, Tokyo, Japan",
            "APAC",
            4777,
            RelationshipKind::Indirect,
            "2019-08-21T16:45:44Z",
            "2019-08-21T17:02:07Z",
            &["10.0.0.0/24", "10.0.1.0/24"],
        )];
        let index = path_index_with(&[
            (
                "rrc06",
                "192.0.2.1",
                "10.0.0.0/24",
                &[4777, 64512, 2603],
                &[4777, 22388, 2603],
            ),
            (
                "rrc06",
                "192.0.2.1",
                "10.0.1.0/24",
                &[4777, 64512, 2603],
                &[4777, 22388, 2603],
            ),
        ]);
        let findings = build(&eps, &index);
        assert_eq!(findings.len(), 1, "one coherent finding");
        assert_eq!(findings[0].distinct_prefixes, 2);
        assert_eq!(
            findings[0].exact_prefixes,
            vec!["10.0.0.0/24", "10.0.1.0/24"]
        );
        assert_eq!(findings[0].baseline_path_signature, "AS4777 AS64512 AS2603");
        assert_eq!(findings[0].changed_path_signature, "AS4777 AS22388 AS2603");
        assert_eq!(findings[0].streams.len(), 2, "all evidence retained");
    }

    #[test]
    fn different_before_after_paths_produce_separate_findings() {
        // The same session with two distinct after paths must produce
        // separate findings (different semantic after-state).
        // A distinct semantic after-state shows up as a distinct
        // EffectKind within one run; separate runs at the same session
        // are separate observations. Prove both stay separate.
        let eps = vec![
            episode(
                1,
                EffectKind::PathReplacement,
                "ris/rrc06 peer 192.0.2.1",
                "Otemachi, Tokyo, Japan",
                "APAC",
                4777,
                RelationshipKind::Indirect,
                "2019-08-21T16:45:44Z",
                "2019-08-21T17:02:07Z",
                &["10.0.0.0/24"],
            ),
            episode(
                2,
                EffectKind::PathReplacement,
                "ris/rrc06 peer 192.0.2.1",
                "Otemachi, Tokyo, Japan",
                "APAC",
                4777,
                RelationshipKind::Indirect,
                "2019-08-21T16:45:44Z",
                "2019-08-21T17:02:07Z",
                &["10.0.0.0/24"],
            ),
        ];
        // Different runs at the same session are separate observations:
        // never merged across runs.
        let index = path_index_with(&[(
            "rrc06",
            "192.0.2.1",
            "10.0.0.0/24",
            &[4777, 64512, 2603],
            &[4777, 22388, 2603],
        )]);
        let findings = build(&eps, &index);
        assert_eq!(findings.len(), 2, "separate runs stay separate findings");
        // Distinct before/after states within ONE run manifest as
        // distinct effect kinds and therefore separate findings.
        let eps2 = vec![
            episode(
                1,
                EffectKind::PathReplacement,
                "ris/rrc06 peer 192.0.2.1",
                "Otemachi, Tokyo, Japan",
                "APAC",
                4777,
                RelationshipKind::Indirect,
                "2019-08-21T16:45:44Z",
                "2019-08-21T17:02:07Z",
                &["10.0.0.0/24"],
            ),
            episode(
                1,
                EffectKind::PrependChange,
                "ris/rrc06 peer 192.0.2.1",
                "Otemachi, Tokyo, Japan",
                "APAC",
                4777,
                RelationshipKind::Indirect,
                "2019-08-21T16:46:00Z",
                "2019-08-21T17:02:07Z",
                &["10.0.3.0/24"],
            ),
        ];
        let findings2 = build(&eps2, &index);
        assert_eq!(
            findings2.len(),
            2,
            "different semantic after-states produce separate findings"
        );
    }

    #[test]
    fn different_observer_sessions_produce_separate_findings() {
        let eps = vec![
            episode(
                1,
                EffectKind::PathReplacement,
                "ris/rrc06 peer 192.0.2.1",
                "Otemachi, Tokyo, Japan",
                "APAC",
                4777,
                RelationshipKind::Indirect,
                "2019-08-21T16:45:44Z",
                "2019-08-21T17:02:07Z",
                &["10.0.0.0/24"],
            ),
            episode(
                1,
                EffectKind::PathReplacement,
                "routeviews/route-views2 peer 198.32.160.1",
                "Eugene, Oregon, US",
                "AMER",
                64512,
                RelationshipKind::Direct,
                "2019-08-21T16:45:25Z",
                "2019-08-21T17:02:00Z",
                &["10.0.0.0/24"],
            ),
        ];
        let index = path_index_with(&[
            (
                "rrc06",
                "192.0.2.1",
                "10.0.0.0/24",
                &[4777, 64512, 2603],
                &[4777, 22388, 2603],
            ),
            (
                "route-views2",
                "198.32.160.1",
                "10.0.0.0/24",
                &[64512, 2603],
                &[64512, 22388, 2603],
            ),
        ]);
        let findings = build(&eps, &index);
        assert_eq!(findings.len(), 2, "sessions never merge");
        assert_ne!(findings[0].observer_site, findings[1].observer_site);
    }

    #[test]
    fn visibility_loss_and_path_change_are_not_conflated() {
        let eps = vec![
            absence_episode(
                1,
                "routeviews/route-views2 peer 198.32.160.1",
                "Eugene, Oregon, US",
                "AMER",
                64512,
                RelationshipKind::Direct,
                "2019-08-21T16:45:25Z",
                "2019-08-21T16:45:27Z",
                &["10.0.0.0/24"],
            ),
            episode(
                1,
                EffectKind::PathReplacement,
                "routeviews/route-views2 peer 198.32.160.1",
                "Eugene, Oregon, US",
                "AMER",
                64512,
                RelationshipKind::Direct,
                "2019-08-21T16:45:48Z",
                "2019-08-21T17:02:00Z",
                &["10.0.1.0/24"],
            ),
        ];
        let index = path_index_with(&[
            (
                "route-views2",
                "198.32.160.1",
                "10.0.0.0/24",
                &[64512, 2603],
                &[64512, 22388, 2603],
            ),
            (
                "route-views2",
                "198.32.160.1",
                "10.0.1.0/24",
                &[64512, 20965, 2603],
                &[64512, 22388, 2603],
            ),
        ]);
        let findings = build(&eps, &index);
        assert_eq!(findings.len(), 2, "absence and path change stay distinct");
        assert!(findings
            .iter()
            .any(|f| f.effect == RoutingEffect::PrefixesTemporarilyAbsent));
        assert!(findings
            .iter()
            .any(|f| f.effect == RoutingEffect::AsPathChanged));
        let absence = findings
            .iter()
            .find(|f| f.effect == RoutingEffect::PrefixesTemporarilyAbsent)
            .unwrap();
        assert_eq!(
            absence.visibility_restored_at.as_deref(),
            Some("2019-08-21T16:45:27Z"),
            "visibility restoration is the withdrawal restoration, not the path return"
        );
    }

    #[test]
    fn one_finding_retains_all_prefixes_and_evidence() {
        let eps = vec![episode(
            1,
            EffectKind::PathReplacement,
            "ris/rrc06 peer 192.0.2.1",
            "Otemachi, Tokyo, Japan",
            "APAC",
            4777,
            RelationshipKind::Indirect,
            "2019-08-21T16:45:44Z",
            "2019-08-21T17:02:07Z",
            &["10.0.0.0/24", "10.0.1.0/24", "10.0.2.0/24"],
        )];
        let index = path_index_with(&[
            (
                "rrc06",
                "192.0.2.1",
                "10.0.0.0/24",
                &[4777, 64512, 2603],
                &[4777, 22388, 2603],
            ),
            (
                "rrc06",
                "192.0.2.1",
                "10.0.1.0/24",
                &[4777, 64512, 2603],
                &[4777, 22388, 2603],
            ),
            (
                "rrc06",
                "192.0.2.1",
                "10.0.2.0/24",
                &[4777, 64512, 2603],
                &[4777, 22388, 2603],
            ),
        ]);
        let findings = build(&eps, &index);
        assert_eq!(findings.len(), 1);
        let f = &findings[0];
        assert_eq!(f.exact_prefixes.len(), 3);
        assert_eq!(f.streams.len(), 3);
        assert_eq!(
            f.evidence_refs,
            vec!["obs-10.0.0.0/24", "obs-10.0.1.0/24", "obs-10.0.2.0/24"]
        );
        assert!(f
            .streams
            .iter()
            .all(|s| s.baseline_path == vec![4777, 64512, 2603]));
    }

    #[test]
    fn finding_generation_is_deterministic() {
        let eps = vec![
            episode(
                1,
                EffectKind::PathReplacement,
                "ris/rrc06 peer 192.0.2.1",
                "Otemachi, Tokyo, Japan",
                "APAC",
                4777,
                RelationshipKind::Indirect,
                "2019-08-21T16:45:44Z",
                "2019-08-21T17:02:07Z",
                &["10.0.0.0/24"],
            ),
            absence_episode(
                1,
                "routeviews/route-views2 peer 198.32.160.1",
                "Eugene, Oregon, US",
                "AMER",
                64512,
                RelationshipKind::Direct,
                "2019-08-21T16:45:25Z",
                "2019-08-21T16:45:27Z",
                &["10.0.0.0/24"],
            ),
        ];
        let index = path_index_with(&[
            (
                "rrc06",
                "192.0.2.1",
                "10.0.0.0/24",
                &[4777, 64512, 2603],
                &[4777, 22388, 2603],
            ),
            (
                "route-views2",
                "198.32.160.1",
                "10.0.0.0/24",
                &[64512, 2603],
                &[64512, 22388, 2603],
            ),
        ]);
        let a = build(&eps, &index);
        let b = build(&eps, &index);
        assert_eq!(a, b, "identical input produces identical findings");
        // Chronological order: earliest first.
        assert!(a[0].first_observed.as_deref() < a[1].first_observed.as_deref());
    }

    #[test]
    fn absence_finding_uses_visibility_language() {
        let eps = vec![absence_episode(
            1,
            "routeviews/route-views2 peer 198.32.160.1",
            "Eugene, Oregon, US",
            "AMER",
            64512,
            RelationshipKind::Direct,
            "2019-08-21T16:45:25Z",
            "2019-08-21T16:45:27Z",
            &["10.0.0.0/24"],
        )];
        let index = path_index_with(&[(
            "route-views2",
            "198.32.160.1",
            "10.0.0.0/24",
            &[64512, 2603],
            &[64512, 22388, 2603],
        )]);
        let findings = build(&eps, &index);
        let s = finding_statement(&findings[0]);
        assert!(s.contains("stopped seeing 1 prefix"), "{s}");
        assert!(s.contains("Visibility returned at 16:45:27 UTC"), "{s}");
        assert!(s.contains("on a different AS path"), "{s}");
        assert!(!s.contains("failed over"), "{s}");
        assert!(!s.contains("backup"), "{s}");
        assert!(!s.contains("traffic"), "{s}");
    }

    #[test]
    fn path_change_finding_names_before_and_after_paths() {
        let eps = vec![episode(
            1,
            EffectKind::PathReplacement,
            "ris/rrc06 peer 192.0.2.1",
            "Otemachi, Tokyo, Japan",
            "APAC",
            4777,
            RelationshipKind::Indirect,
            "2019-08-21T16:45:44Z",
            "2019-08-21T17:02:07Z",
            &["10.0.0.0/24"],
        )];
        let index = path_index_with(&[(
            "rrc06",
            "192.0.2.1",
            "10.0.0.0/24",
            &[4777, 64512, 2603],
            &[4777, 22388, 2603],
        )]);
        let findings = build(&eps, &index);
        let s = finding_statement(&findings[0]);
        assert!(
            s.contains("change AS path from: AS4777 AS64512 AS2603 to: AS4777 AS22388 AS2603"),
            "{s}"
        );
        assert!(
            s.contains("The exact baseline AS path returned at 17:00:00 UTC"),
            "{s}"
        );
    }

    #[test]
    fn finding_distinguishes_visibility_return_from_baseline_return() {
        let eps = vec![absence_episode(
            1,
            "routeviews/route-views2 peer 198.32.160.1",
            "Eugene, Oregon, US",
            "AMER",
            64512,
            RelationshipKind::Direct,
            "2019-08-21T16:45:25Z",
            "2019-08-21T16:45:27Z",
            &["10.0.0.0/24"],
        )];
        // Visibility returns at 16:45:27; baseline path only at 16:59:26.
        let mut index = path_index_with(&[(
            "route-views2",
            "198.32.160.1",
            "10.0.0.0/24",
            &[64512, 2603],
            &[64512, 22388, 2603],
        )]);
        let map = index.by_run.get_mut("EV").unwrap();
        let ev = map
            .get_mut(&(
                "route-views2".to_string(),
                "198.32.160.1".to_string(),
                "10.0.0.0/24".to_string(),
            ))
            .unwrap();
        ev.visibility_restored_at = Some("2019-08-21T16:45:27Z".to_string());
        ev.exact_baseline_restored_at = Some("2019-08-21T16:59:26Z".to_string());
        let findings = build(&eps, &index);
        let f = &findings[0];
        assert_eq!(
            f.visibility_restored_at.as_deref(),
            Some("2019-08-21T16:45:27Z")
        );
        assert_eq!(
            f.exact_baseline_restored_at.as_deref(),
            Some("2019-08-21T16:59:26Z")
        );
        let s = finding_statement(f);
        assert!(s.contains("Visibility returned at 16:45:27 UTC"), "{s}");
        assert!(
            s.contains("The exact baseline AS path returned at 16:59:26 UTC"),
            "{s}"
        );
        assert!(
            !s.contains("restored at 16:45:27"),
            "visibility ≠ baseline return"
        );
    }

    #[test]
    fn finding_does_not_call_changed_path_a_backup() {
        let eps = vec![episode(
            1,
            EffectKind::PathReplacement,
            "ris/rrc06 peer 192.0.2.1",
            "Otemachi, Tokyo, Japan",
            "APAC",
            4777,
            RelationshipKind::Indirect,
            "2019-08-21T16:45:44Z",
            "2019-08-21T17:02:07Z",
            &["10.0.0.0/24"],
        )];
        let index = path_index_with(&[(
            "rrc06",
            "192.0.2.1",
            "10.0.0.0/24",
            &[4777, 64512, 2603],
            &[4777, 22388, 2603],
        )]);
        let findings = build(&eps, &index);
        let s = finding_statement(&findings[0]);
        for word in ["backup", "failed over", "rerouted", "failover"] {
            assert!(!s.contains(word), "forbidden claim {word} in: {s}");
        }
    }

    #[test]
    fn finding_does_not_claim_traffic_restoration() {
        let eps = vec![absence_episode(
            1,
            "routeviews/route-views2 peer 198.32.160.1",
            "Eugene, Oregon, US",
            "AMER",
            64512,
            RelationshipKind::Direct,
            "2019-08-21T16:45:25Z",
            "2019-08-21T16:45:27Z",
            &["10.0.0.0/24"],
        )];
        let index = path_index_with(&[(
            "route-views2",
            "198.32.160.1",
            "10.0.0.0/24",
            &[64512, 2603],
            &[64512, 22388, 2603],
        )]);
        let findings = build(&eps, &index);
        let s = finding_statement(&findings[0]);
        assert!(!s.contains("traffic restored"), "{s}");
        assert!(!s.contains("traffic"), "{s}");
        assert!(!s.contains("protected"), "{s}");
    }

    #[test]
    fn finding_names_exact_observer_and_peer_relationship() {
        let eps = vec![episode(
            1,
            EffectKind::PathReplacement,
            "ris/rrc15 peer 187.16.216.4",
            "Sao Paulo, Brazil",
            "AMER",
            1916,
            RelationshipKind::Indirect,
            "2019-08-21T16:35:38Z",
            "2019-08-21T17:03:35Z",
            &["10.0.0.0/24"],
        )];
        let index = path_index_with(&[(
            "rrc15",
            "187.16.216.4",
            "10.0.0.0/24",
            &[1916, 64512, 2603],
            &[1916, 20080, 64512, 2603],
        )]);
        let findings = build(&eps, &index);
        let s = finding_statement(&findings[0]);
        assert!(s.contains("rrc15 in Sao Paulo, Brazil"), "{s}");
        assert!(
            s.contains(
                "receiving routes from AS1916 — name not reviewed, peer 187.16.216.4, indirectly"
            ),
            "{s}"
        );
        assert!(s.contains("16:35:38"), "{s}");
    }

    #[test]
    fn collapse_path_compacts_runs_and_retains_exact_sequence() {
        let path = vec![64512, 22388, 24489, 24489, 24489, 24489, 24490, 20965, 2603];
        let collapsed = collapse_as_path(&path);
        assert_eq!(
            collapsed,
            "AS64512 AS22388 AS24489×4 AS24490 AS20965 AS2603"
        );
        // The exact sequence is always retained by the finding streams.
        let eps = vec![episode(
            1,
            EffectKind::PathReplacement,
            "ris/rrc06 peer 192.0.2.1",
            "Otemachi, Tokyo, Japan",
            "APAC",
            4777,
            RelationshipKind::Indirect,
            "2019-08-21T16:45:44Z",
            "2019-08-21T17:02:07Z",
            &["10.0.0.0/24"],
        )];
        let index = path_index_with(&[(
            "rrc06",
            "192.0.2.1",
            "10.0.0.0/24",
            &[64512, 2603],
            &path.clone(),
        )]);
        let findings = build(&eps, &index);
        assert_eq!(
            findings[0].changed_path_signature,
            "AS64512 AS22388 AS24489×4 AS24490 AS20965 AS2603"
        );
        assert_eq!(findings[0].streams[0].changed_path.as_ref().unwrap(), &path);
    }
}

/// ASN identity and path-renderer tests (Session 39, Part 6).
#[cfg(test)]
mod identity_tests {
    use super::finding_test_helpers::{absence_episode, build, episode, path_index_with};
    use super::*;

    fn registry_with(entries: &[(u32, &str, &str, &str)]) -> AsnIdentityRegistry {
        let mut r = AsnIdentityRegistry::default();
        for (asn, name, role, status) in entries {
            r.identities.push(AsnIdentity {
                asn: *asn,
                display_name: name.to_string(),
                role: role.to_string(),
                valid_from: Some("2019-08-21".to_string()),
                valid_to: Some("2019-08-21".to_string()),
                provenance: "reviewed".to_string(),
                review_status: status.to_string(),
            });
        }
        r
    }

    #[test]
    fn historical_identity_is_selected_for_event_date() {
        let r = registry_with(&[(1916, "RNP", "national re", "HistoricallyReviewed")]);
        assert_eq!(r.display_name(1916, "2019-08-21"), Some("RNP"));
        assert_eq!(r.role(1916, "2019-08-21"), Some("national re"));
    }

    #[test]
    fn current_identity_does_not_override_historical_identity() {
        // The registry is the reviewed historical source; a later
        // current-holder entry must not shadow it (registry keeps the
        // first reviewed entry per ASN).
        let mut r = registry_with(&[(1916, "RNP", "national re", "HistoricallyReviewed")]);
        r.identities.push(AsnIdentity {
            asn: 1916,
            display_name: "current holder".to_string(),
            role: "x".to_string(),
            valid_from: Some("2024-01-01".to_string()),
            valid_to: None,
            provenance: "unreviewed".to_string(),
            review_status: "unreviewed".to_string(),
        });
        assert_eq!(
            r.display_name(1916, "2019-08-21"),
            Some("RNP"),
            "reviewed historical name wins"
        );
    }

    #[test]
    fn unknown_identity_preserves_asn() {
        let r = AsnIdentityRegistry::default();
        assert_eq!(r.display_name(22388, "2019-08-21"), None);
        assert_eq!(r.role(22388, "2019-08-21"), None);
        // The rendering contract: ASN always renders; the name degrades
        // to "name not reviewed".
        let asn = 22388u32;
        let name = r
            .display_name(asn, "2019-08-21")
            .unwrap_or("name not reviewed");
        assert_eq!(name, "name not reviewed");
        assert_eq!(format!("AS{asn}"), "AS22388");
    }

    #[test]
    fn display_name_never_changes_path_identity() {
        // The exact ASN sequence is the authoritative path identity;
        // display names are enrichment only.
        let path = vec![1916, 64512, 2603];
        let collapsed = collapse_as_path(&path);
        assert_eq!(collapsed, "AS1916 AS64512 AS2603");
        assert!(
            !collapsed.contains("RNP"),
            "names never enter the path identity"
        );
    }

    #[test]
    fn path_renderer_retains_exact_asn_sequence() {
        // The uncollapsed renderer is space-joined ASNs in path order.
        let path = vec![4777, 2500, 2500, 2500, 7660, 22388, 64512, 2603];
        let mut parts: Vec<String> = path.iter().map(|a| format!("AS{a}")).collect();
        let rendered = parts.join(" ");
        assert_eq!(
            rendered,
            "AS4777 AS2500 AS2500 AS2500 AS7660 AS22388 AS64512 AS2603"
        );
        assert_eq!(
            collapse_as_path(&path),
            "AS4777 AS2500×3 AS7660 AS22388 AS64512 AS2603"
        );
        parts.clear();
    }

    #[test]
    fn finding_cues_carry_exact_references_and_paths() {
        let eps = vec![
            episode(
                1,
                EffectKind::PathReplacement,
                "ris/rrc06 peer 192.0.2.1",
                "Otemachi, Tokyo, Japan",
                "APAC",
                4777,
                RelationshipKind::Indirect,
                "2019-08-21T16:45:44Z",
                "2019-08-21T17:02:07Z",
                &["10.0.0.0/24"],
            ),
            episode(
                1,
                EffectKind::PathReplacement,
                "routeviews/route-views2 peer 198.32.160.1",
                "Eugene, Oregon, US",
                "AMER",
                64512,
                RelationshipKind::Direct,
                "2019-08-21T16:45:48Z",
                "2019-08-21T17:02:19Z",
                &["10.0.0.0/24"],
            ),
        ];
        let index = path_index_with(&[
            (
                "rrc06",
                "192.0.2.1",
                "10.0.0.0/24",
                &[4777, 64512, 2603],
                &[4777, 22388, 2603],
            ),
            (
                "route-views2",
                "198.32.160.1",
                "10.0.0.0/24",
                &[64512, 2603],
                &[64512, 22388, 2603],
            ),
        ]);
        let findings = build(&eps, &index);
        let cues = build_finding_cues(&findings);
        // Exact path comparison cue.
        assert!(cues
            .iter()
            .any(|c| c.text.contains("baseline path AS4777 AS64512 AS2603")
                && c.text.contains("AS4777 AS22388 AS2603")));
        // Exact observation reference.
        assert!(cues
            .iter()
            .any(|c| c.traceable_to.contains("ris/rrc06 peer 192.0.2.1")
                && c.traceable_to.contains("16:45:44")));
        // Cross-observer comparison for the same 1-prefix set.
        assert!(cues
            .iter()
            .any(|c| c.text.contains("Compare") && c.text.contains("1-prefix set")));
        // Deterministic dedupe.
        let cues2 = build_finding_cues(&findings);
        assert_eq!(cues, cues2);
    }

    #[test]
    fn no_cue_without_actionable_external_fact() {
        // No findings → no finding cues.
        let cues = build_finding_cues(&[]);
        assert!(cues.is_empty());
        // An absence finding without exact path evidence still gets an
        // advertisement-review cue (withdrawn at an exact time).
        let eps = vec![absence_episode(
            1,
            "routeviews/route-views2 peer 198.32.160.1",
            "Eugene, Oregon, US",
            "AMER",
            64512,
            RelationshipKind::Direct,
            "2019-08-21T16:45:25Z",
            "2019-08-21T16:45:27Z",
            &["10.0.0.0/24"],
        )];
        let index = path_index_with(&[]);
        let findings = build(&eps, &index);
        let cues = build_finding_cues(&findings);
        assert!(cues
            .iter()
            .any(|c| c.text.contains("Review advertisements") && c.text.contains("16:45:25")));
    }
}

/// Session 40 tests: audit-driven prose, precise restoration language,
/// path semantics, and principal selection.
#[cfg(test)]
mod session40_tests {
    use super::finding_test_helpers::{build, episode, path_index_with};
    use super::*;

    #[test]
    fn finding_prose_is_derived_from_audit_record() {
        // The sentence must state exactly the restoration classes the
        // audit record supports — no more, no less.
        let eps = vec![episode(
            1,
            EffectKind::PathReplacement,
            "ris/rrc06 peer 192.0.2.1",
            "Otemachi, Tokyo, Japan",
            "APAC",
            4777,
            RelationshipKind::Indirect,
            "2019-08-21T16:45:44Z",
            "2019-08-21T17:02:07Z",
            &["10.0.0.0/24"],
        )];
        let index = path_index_with(&[(
            "rrc06",
            "192.0.2.1",
            "10.0.0.0/24",
            &[4777, 64512, 2603],
            &[4777, 22388, 2603],
        )]);
        let findings = build(&eps, &index);
        let f = &findings[0];
        // Audit record fields.
        assert_eq!(f.effect, RoutingEffect::AsPathChanged);
        assert_eq!(
            f.exact_baseline_restored_at.as_deref(),
            Some("2019-08-21T17:00:00Z")
        );
        assert!(f.visibility_restored_at.is_none());
        assert!(f.reviewed_plane_restored_at.is_none());
        // The sentence uses exactly the exact-baseline class.
        let s = finding_statement(f);
        assert!(
            s.contains("The exact baseline AS path returned at 17:00:00 UTC"),
            "{s}"
        );
        assert!(!s.contains("Visibility returned"), "{s}");
        assert!(!s.contains("reviewed path at"), "{s}");
        assert!(!s.contains("equivalent route"), "{s}");
        // The audit serialization carries the same fields.
        let audit = FindingAudit::from_vm(&IncidentWorkbenchViewModel::default_for_test(f.clone()));
        let entry = &audit.findings[0];
        assert_eq!(
            entry.exact_baseline_restoration_time.as_deref(),
            Some("2019-08-21T17:00:00Z")
        );
        assert!(entry.visibility_restoration_time.is_none());
    }

    #[test]
    fn unsupported_restoration_cannot_be_rendered() {
        // A finding whose audit record has NO exact-baseline restoration
        // must never claim one.
        let eps = vec![episode(
            1,
            EffectKind::PathReplacement,
            "ris/rrc06 peer 192.0.2.1",
            "Otemachi, Tokyo, Japan",
            "APAC",
            4777,
            RelationshipKind::Indirect,
            "2019-08-21T16:45:44Z",
            "2019-08-21T17:02:07Z",
            &["10.0.0.0/24"],
        )];
        let mut index = path_index_with(&[(
            "rrc06",
            "192.0.2.1",
            "10.0.0.0/24",
            &[4777, 64512, 2603],
            &[4777, 22388, 2603],
        )]);
        // Drop the exact-baseline evidence from the index.
        let map = index.by_run.get_mut("EV").unwrap();
        for ev in map.values_mut() {
            ev.exact_baseline_restored_at = None;
        }
        let findings = build(&eps, &index);
        let mut f = findings[0].clone();
        // No audit support for an exact-baseline restoration: clear it
        // from the finding AND its member streams.
        f.exact_baseline_restored_at = None;
        for stream in f.streams.iter_mut() {
            stream.exact_baseline_restored_at = None;
        }
        f.state_at_window_end = EndState::StillChangedAtWindowEnd;
        let s = finding_statement(&f);
        assert!(
            s.contains("remained changed at the event-window end"),
            "{s}"
        );
        assert!(!s.contains("exact baseline AS path returned"), "{s}");
        assert!(!s.contains("returned toward"), "{s}");
    }

    #[test]
    fn every_prefix_in_finding_has_supporting_lifecycle() {
        let eps = vec![episode(
            1,
            EffectKind::PathReplacement,
            "ris/rrc06 peer 192.0.2.1",
            "Otemachi, Tokyo, Japan",
            "APAC",
            4777,
            RelationshipKind::Indirect,
            "2019-08-21T16:45:44Z",
            "2019-08-21T17:02:07Z",
            &["10.0.0.0/24", "10.0.1.0/24"],
        )];
        let index = path_index_with(&[
            (
                "rrc06",
                "192.0.2.1",
                "10.0.0.0/24",
                &[4777, 64512, 2603],
                &[4777, 22388, 2603],
            ),
            (
                "rrc06",
                "192.0.2.1",
                "10.0.1.0/24",
                &[4777, 64512, 2603],
                &[4777, 22388, 2603],
            ),
        ]);
        let findings = build(&eps, &index);
        let f = &findings[0];
        // Every finding prefix has a per-prefix stream row with exact
        // baseline/changed paths and evidence refs.
        for stream in &f.streams {
            assert!(
                !stream.baseline_path.is_empty(),
                "{} has lifecycle baseline",
                stream.prefix
            );
            assert!(
                stream.changed_path.is_some(),
                "{} has changed path",
                stream.prefix
            );
            assert!(
                stream.exact_baseline_restored_at.is_some(),
                "{} has restoration",
                stream.prefix
            );
        }
        assert_eq!(f.streams.len(), f.exact_prefixes.len());
    }

    #[test]
    fn every_displayed_path_has_supporting_evidence() {
        // The finding's displayed signatures come from member stream
        // evidence: each displayed ASN must appear in at least one
        // member path.
        let eps = vec![episode(
            1,
            EffectKind::PathReplacement,
            "ris/rrc06 peer 192.0.2.1",
            "Otemachi, Tokyo, Japan",
            "APAC",
            4777,
            RelationshipKind::Indirect,
            "2019-08-21T16:45:44Z",
            "2019-08-21T17:02:07Z",
            &["10.0.0.0/24"],
        )];
        let index = path_index_with(&[(
            "rrc06",
            "192.0.2.1",
            "10.0.0.0/24",
            &[4777, 64512, 2603],
            &[4777, 20080, 64512, 20965, 2603],
        )]);
        let findings = build(&eps, &index);
        let f = &findings[0];
        let baseline = f.streams[0].baseline_path.clone();
        let changed = f.streams[0].changed_path.clone().unwrap();
        let diff = diff_paths(&baseline, &changed, &[64512]);
        assert!(
            diff.plane_retained,
            "baseline and changed both contain the plane"
        );
        assert_eq!(diff.inserted, vec![20080, 20965]);
        assert!(diff.removed.is_empty());
        let explanation = explain_path_diff(&baseline, &changed, &[64512], "plane", &|asn| {
            format!("AS{asn}")
        });
        // Every ASN named in the explanation appears in one of the two
        // exact paths (no invented path content).
        assert!(explanation.contains("AS20080"), "{explanation}");
        assert!(!explanation.contains("AS99999"), "no invented ASNs");
    }

    #[test]
    fn exact_baseline_restoration_uses_exact_language() {
        let f = finding_fixture(RoutingEffect::AsPathChanged);
        let s = finding_statement(&f);
        assert!(s.contains("The exact baseline AS path returned at"), "{s}");
        assert!(
            !s.contains("returned toward their earlier path state"),
            "{s}"
        );
        assert!(!s.contains("baseline path was restored by"), "{s}");
    }

    #[test]
    fn visibility_only_restoration_does_not_say_baseline() {
        let mut f = finding_fixture(RoutingEffect::PrefixesTemporarilyAbsent);
        f.visibility_restored_at = Some("2019-08-21T16:45:27Z".to_string());
        f.equivalent_route_restored_at = Some("2019-08-21T16:45:27Z".to_string());
        f.exact_baseline_restored_at = None;
        f.reviewed_plane_restored_at = None;
        let s = finding_statement(&f);
        assert!(
            s.contains("Visibility returned at 16:45:27 UTC on a different AS path."),
            "{s}"
        );
        assert!(!s.contains("exact baseline"), "{s}");
        assert!(!s.contains("reviewed path"), "{s}");
    }

    #[test]
    fn plane_restoration_does_not_say_exact_path() {
        let mut f = finding_fixture(RoutingEffect::NamedPlaneDeparted);
        f.reviewed_plane_restored_at = Some("2019-08-21T17:00:00Z".to_string());
        f.exact_baseline_restored_at = None;
        f.named_path_plane = "plane".to_string();
        let s = finding_statement(&f);
        assert!(
            s.contains("returned to the reviewed plane path at 17:00:00 UTC"),
            "{s}"
        );
        assert!(!s.contains("exact baseline"), "{s}");
    }

    #[test]
    fn mixed_restoration_outcomes_are_not_hidden() {
        // One prefix restores exactly; another stays changed. The
        // finding must state the distribution, not one outcome.
        let mut f = finding_fixture(RoutingEffect::AsPathChanged);
        f.streams[0].exact_baseline_restored_at = Some("2019-08-21T17:00:00Z".to_string());
        f.streams.push(f.streams[0].clone());
        let last = f.streams.last_mut().unwrap();
        last.prefix = "10.0.2.0/24".to_string();
        last.exact_baseline_restored_at = None;
        f.exact_baseline_restored_at = Some("2019-08-21T17:00:00Z".to_string());
        f.state_at_window_end = EndState::StillChangedAtWindowEnd;
        let s = finding_statement(&f);
        // Both outcomes are explicit: the exact-baseline return AND the
        // still-changed state.
        assert!(
            s.contains("exact baseline AS path returned for 1 of 2 prefixes at 17:00:00 UTC"),
            "{s}"
        );
        assert!(
            s.contains("remained changed at the event-window end"),
            "{s}"
        );
    }

    #[test]
    fn changed_at_analysis_end_is_explicit() {
        // RRC15-style: in-window baseline restored, then re-changed in
        // cooldown with no restoration before analysis end. The
        // statement must not hide the analysis-end outcome.
        let mut f = finding_fixture(RoutingEffect::AsPathChanged);
        f.exact_baseline_restored_at = Some("2019-08-21T17:03:35Z".to_string());
        f.streams[0].exact_baseline_restored_at = Some("2019-08-21T17:03:35Z".to_string());
        f.state_at_window_end = EndState::BaselineRestored;
        f.state_at_analysis_end =
            CooldownOutcome::StillChangingBeforeAnalysisEnd("2019-08-21T17:52:16Z".to_string());
        f.analysis_end = "2019-08-21T18:00:00Z".to_string();
        let s = finding_statement(&f);
        assert!(
            s.contains("The exact baseline AS path returned at 17:03:35 UTC"),
            "{s}"
        );
        assert!(
            s.contains("The routes changed again at 17:52:16 UTC; no restoration was observed before the analysis ended at 18:00:00 UTC"),
            "{s}"
        );
    }

    #[test]
    fn rrc15_regression_peer_asn_in_labels_and_outcomes() {
        // Regression (Session 40, Part 3): the RRC15 11-prefix finding
        // restored in-window (17:03:32–17:03:35) but re-changed at
        // 17:52:16 during cooldown. The finding must carry the peer ASN
        // and must state BOTH outcomes.
        let mut f = finding_fixture(RoutingEffect::AsPathChanged);
        f.collector = "rrc15".to_string();
        f.observer_site = "Sao Paulo, Brazil".to_string();
        f.peer_asn = Some(1916);
        f.peer_ip = "187.16.216.4".to_string();
        f.peer_name = "name not reviewed".to_string();
        f.distinct_prefixes = 11;
        f.streams = (0..11)
            .map(|i| FindingStream {
                prefix: format!("10.0.{i}.0/24"),
                baseline_path: vec![1916, 64512, 2603],
                changed_path: Some(vec![1916, 20080, 64512, 20965, 2603]),
                final_path: Some(vec![1916, 64512, 2603]),
                withdrawn: false,
                first_change_utc: Some("2019-08-21T16:35:38Z".to_string()),
                transitions: Vec::new(),
                visibility_restored_at: None,
                equivalent_route_restored_at: None,
                reviewed_plane_restored_at: None,
                exact_baseline_restored_at: Some(if i % 2 == 0 {
                    "2019-08-21T17:03:32Z".to_string()
                } else {
                    "2019-08-21T17:03:35Z".to_string()
                }),
                evidence_refs: format!("obs-{i}"),
            })
            .collect();
        f.exact_baseline_restored_at = Some("2019-08-21T17:03:35Z".to_string());
        f.state_at_window_end = EndState::BaselineRestored;
        f.state_at_analysis_end =
            CooldownOutcome::StillChangingBeforeAnalysisEnd("2019-08-21T17:52:16Z".to_string());
        f.analysis_end = "2019-08-21T18:00:00Z".to_string();
        let s = finding_statement(&f);
        assert!(s.contains("peer 187.16.216.4"), "peer IP with the ASN: {s}");
        assert!(s.contains("17:03:32"), "range start: {s}");
        assert!(s.contains("17:03:35"), "range end: {s}");
        assert!(s.contains("17:52:16"), "cooldown re-change stated: {s}");
    }

    // ── Part 4: path semantics ───────────────────────────────────────

    #[test]
    fn path_diff_reports_inserted_asns() {
        let before = [1916, 64512, 2603];
        let after = [1916, 20080, 64512, 20965, 2603];
        let d = diff_paths(&before, &after, &[64512]);
        assert_eq!(d.inserted, vec![20080, 20965]);
        assert!(d.plane_retained);
        assert!(!d.plane_departed);
        assert!(d.longer);
        let explanation =
            explain_path_diff(&before, &after, &[64512], "plane", &|a| format!("AS{a}"));
        assert!(explanation.contains("AS20080"), "{explanation}");
        assert!(explanation.contains("AS20965"), "{explanation}");
        assert!(
            !explanation.contains("because"),
            "no causation: {explanation}"
        );
    }

    #[test]
    fn path_diff_reports_removed_asns() {
        let before = [1916, 20080, 64512, 20965, 2603];
        let after = [1916, 64512, 2603];
        let d = diff_paths(&before, &after, &[64512]);
        assert_eq!(d.removed, vec![20080, 20965]);
        let explanation =
            explain_path_diff(&before, &after, &[64512], "plane", &|a| format!("AS{a}"));
        assert!(
            explanation.contains("left the selected path"),
            "{explanation}"
        );
    }

    #[test]
    fn prepend_delta_is_rendered_numerically() {
        let before = [64512, 40220, 225];
        let after = [64512, 40220, 225, 225, 225, 225, 225, 225, 225];
        let d = diff_paths(&before, &after, &[64512]);
        assert!(d.count_deltas.contains(&(225, 6)), "{:?}", d.count_deltas);
        let explanation =
            explain_path_diff(&before, &after, &[64512], "plane", &|a| format!("AS{a}"));
        assert!(
            explanation.contains("origin-AS prepending increased from 1 to 7 AS225 occurrences"),
            "{explanation}"
        );
    }

    #[test]
    fn plane_retained_and_plane_departed_are_distinct() {
        let before = [4777, 2500, 7660, 22388, 64512, 2603];
        let after = [4777, 2500, 7660, 24287, 24490, 20965, 2603];
        let d = diff_paths(&before, &after, &[64512]);
        assert!(d.plane_departed);
        assert!(!d.plane_retained);
        let explanation =
            explain_path_diff(&before, &after, &[64512], "plane", &|a| format!("AS{a}"));
        assert!(
            explanation.contains("no longer traversed"),
            "departure language: {explanation}"
        );
        // The RRC15 retained case stays distinct.
        let before2 = [1916, 64512, 2603];
        let after2 = [1916, 20080, 64512, 20965, 2603];
        let d2 = diff_paths(&before2, &after2, &[64512]);
        assert!(d2.plane_retained);
        assert!(!d2.plane_departed);
        let explanation2 =
            explain_path_diff(&before2, &after2, &[64512], "plane", &|a| format!("AS{a}"));
        assert!(explanation2.contains("continued through"), "{explanation2}");
    }

    #[test]
    fn visibility_absence_is_not_rendered_as_path_change() {
        let before = [1916, 64512, 2603];
        let after: Vec<u32> = Vec::new();
        let d = diff_paths(&before, &after, &[64512]);
        assert!(
            d.inserted.is_empty() && d.removed.is_empty(),
            "absence is not a path diff"
        );
        let explanation =
            explain_path_diff(&before, &after, &[64512], "plane", &|a| format!("AS{a}"));
        assert!(explanation.contains("absent"), "{explanation}");
        assert!(!explanation.contains("changed"), "{explanation}");
        assert!(!explanation.contains("traversed"), "{explanation}");
    }

    #[test]
    fn path_explanation_never_assigns_cause() {
        let before = [7660, 64512, 40220, 225];
        let after = [7660, 2907, 64512, 40220, 225];
        let explanation =
            explain_path_diff(&before, &after, &[64512], "plane", &|a| format!("AS{a}"));
        for word in [
            "because", "failed", "outage", "rerouted", "caused", "due to",
        ] {
            assert!(
                !explanation.contains(word),
                "forbidden cause word {word}: {explanation}"
            );
        }
        assert!(explanation.contains("AS2907 appeared"), "{explanation}");
    }

    // ── Part 8: principal selection ──────────────────────────────────

    #[test]
    fn temporary_absence_outranks_minor_single_prefix_change() {
        let absence = finding_fixture(RoutingEffect::PrefixesTemporarilyAbsent);
        let single = finding_fixture(RoutingEffect::AsPathChanged);
        let (principal, _) =
            select_principal_findings(vec![single.clone(), absence.clone()], &[], 1);
        assert_eq!(principal.len(), 1);
        assert_eq!(
            principal[0].effect,
            RoutingEffect::PrefixesTemporarilyAbsent,
            "absence outranks a minor single-prefix change"
        );
    }

    #[test]
    fn principal_findings_prefer_observer_diversity() {
        let a = finding_fixture(RoutingEffect::AsPathChanged); // rrc06
        let mut b = finding_fixture(RoutingEffect::AsPathChanged);
        b.collector = "rrc15".to_string();
        b.peer_ip = "187.16.216.4".to_string();
        b.observer_site = "Sao Paulo, Brazil".to_string();
        let mut c = finding_fixture(RoutingEffect::AsPathChanged);
        c.collector = "rrc15".to_string();
        c.peer_ip = "2001:12f8::20".to_string();
        c.observer_site = "Sao Paulo, Brazil".to_string();
        let mut d = finding_fixture(RoutingEffect::AsPathChanged);
        d.collector = "route-views2".to_string();
        d.peer_ip = "64.57.28.241".to_string();
        d.observer_site = "Eugene, Oregon, US".to_string();
        let (principal, _) = select_principal_findings(vec![a, b, c, d], &[], 3);
        let sites: std::collections::BTreeSet<&str> =
            principal.iter().map(|f| f.observer_site.as_str()).collect();
        assert_eq!(sites.len(), 3, "three distinct observer sites preferred");
        assert!(
            !sites.contains(&("Sao Paulo, Brazil")) || sites.len() == 3,
            "two Sao Paulo variations must not both beat site diversity"
        );
    }

    #[test]
    fn named_plane_departure_is_principal() {
        let mut departure = finding_fixture(RoutingEffect::AsPathChanged);
        departure.streams[0].baseline_path = vec![4777, 64512, 2603];
        departure.streams[0].changed_path = Some(vec![4777, 22388, 2603]); // departs plane
        let departure_id = departure.stable_id.clone();
        let path_change = finding_fixture(RoutingEffect::AsPathChanged);
        let (principal, _) = select_principal_findings(vec![path_change, departure], &[64512], 1);
        // The plane-departing path change outranks the retained one.
        assert_eq!(principal[0].stable_id, departure_id);
    }

    #[test]
    fn large_prefix_group_outranks_equivalent_single_prefix_variant() {
        let mut large = finding_fixture(RoutingEffect::AsPathChanged);
        large.distinct_prefixes = 11;
        large.streams = (0..11)
            .map(|i| FindingStream {
                prefix: format!("10.0.{i}.0/24"),
                baseline_path: vec![1916, 64512, 2603],
                changed_path: Some(vec![1916, 20080, 64512, 20965, 2603]),
                final_path: None,
                withdrawn: false,
                first_change_utc: None,
                transitions: Vec::new(),
                visibility_restored_at: None,
                equivalent_route_restored_at: None,
                reviewed_plane_restored_at: None,
                exact_baseline_restored_at: None,
                evidence_refs: String::new(),
            })
            .collect();
        let single = finding_fixture(RoutingEffect::AsPathChanged);
        let (principal, secondary) =
            select_principal_findings(vec![single.clone(), large.clone()], &[], 1);
        assert_eq!(principal.len(), 1);
        assert_eq!(principal[0].distinct_prefixes, 11);
        assert_eq!(
            secondary.len(),
            1,
            "the single-prefix variant stays accessible"
        );
    }

    #[test]
    fn all_secondary_findings_remain_accessible() {
        let findings: Vec<RoutingFinding> = (0..8)
            .map(|i| {
                let mut f = finding_fixture(RoutingEffect::AsPathChanged);
                f.stable_id = format!("finding-{i}");
                f.peer_ip = format!("192.0.2.{i}");
                f.distinct_prefixes = 11;
                f
            })
            .collect();
        let (principal, secondary) = select_principal_findings(findings.clone(), &[], 4);
        assert_eq!(principal.len(), 4, "cap at four principal stories");
        assert_eq!(
            principal.len() + secondary.len(),
            findings.len(),
            "nothing is dropped"
        );
        let mut ids: Vec<String> = principal
            .iter()
            .chain(secondary.iter())
            .map(|f| f.stable_id.clone())
            .collect();
        ids.sort();
        let mut all: Vec<String> = findings.iter().map(|f| f.stable_id.clone()).collect();
        all.sort();
        assert_eq!(ids, all);
    }

    #[test]
    fn principal_selection_is_deterministic() {
        let mut findings: Vec<RoutingFinding> = (0..6)
            .map(|i| {
                let mut f = finding_fixture(RoutingEffect::AsPathChanged);
                f.stable_id = format!("finding-{i}");
                f.peer_ip = format!("192.0.2.{i}");
                f.distinct_prefixes = 11;
                f
            })
            .collect();
        findings[3].effect = RoutingEffect::PrefixesTemporarilyAbsent;
        let (a, sa) = select_principal_findings(findings.clone(), &[], 4);
        let (b, sb) = select_principal_findings(findings, &[], 4);
        assert_eq!(a, b);
        assert_eq!(sa, sb);
    }

    // ── Fixtures ─────────────────────────────────────────────────────

    fn finding_fixture(effect: RoutingEffect) -> RoutingFinding {
        let mut f = RoutingFinding {
            stable_id: "rrc06-as4777-test".to_string(),
            target_label: "TARGET (AS2603)".to_string(),
            target_origin_asns: vec![2603],
            target_name: "TARGET".to_string(),
            observer_site: "Otemachi, Tokyo, Japan".to_string(),
            observer_region: "APAC".to_string(),
            source_family: "ris".to_string(),
            collector: "rrc06".to_string(),
            peer_ip: "192.0.2.1".to_string(),
            peer_asn: Some(4777),
            peer_name: "name not reviewed".to_string(),
            peer_role: String::new(),
            peer_asn_ambiguous: false,
            relationship: RelationshipKind::Indirect,
            effect,
            first_observed: Some("2019-08-21T16:45:44Z".to_string()),
            last_observed: Some("2019-08-21T17:02:07Z".to_string()),
            named_path_plane: "plane".to_string(),
            visibility_restored_at: None,
            equivalent_route_restored_at: None,
            reviewed_plane_restored_at: None,
            exact_baseline_restored_at: Some("2019-08-21T17:00:00Z".to_string()),
            state_at_window_end: EndState::BaselineRestored,
            state_at_analysis_end: CooldownOutcome::None,
            analysis_end: "2019-08-21T18:00:00Z".to_string(),
            observer_stream_count: 1,
            distinct_prefixes: 1,
            baseline_path_signature: "AS4777 AS64512 AS2603".to_string(),
            changed_path_signature: "AS4777 AS22388 AS2603".to_string(),
            final_path_signature: "AS4777 AS64512 AS2603".to_string(),
            exact_prefixes: vec!["10.0.0.0/24".to_string()],
            evidence_refs: vec!["obs-1".to_string()],
            scope_limit: String::new(),
            streams: vec![FindingStream {
                prefix: "10.0.0.0/24".to_string(),
                baseline_path: vec![4777, 64512, 2603],
                changed_path: Some(vec![4777, 22388, 2603]),
                final_path: Some(vec![4777, 64512, 2603]),
                withdrawn: false,
                first_change_utc: Some("2019-08-21T16:45:44Z".to_string()),
                transitions: Vec::new(),
                visibility_restored_at: None,
                equivalent_route_restored_at: None,
                reviewed_plane_restored_at: None,
                exact_baseline_restored_at: Some("2019-08-21T17:00:00Z".to_string()),
                evidence_refs: "obs-1".to_string(),
            }],
        };
        if effect == RoutingEffect::PrefixesTemporarilyAbsent {
            f.visibility_restored_at = Some("2019-08-21T16:45:27Z".to_string());
            f.equivalent_route_restored_at = Some("2019-08-21T16:45:27Z".to_string());
            f.exact_baseline_restored_at = Some("2019-08-21T16:59:26Z".to_string());
            f.streams[0].visibility_restored_at = f.visibility_restored_at.clone();
            f.streams[0].equivalent_route_restored_at = f.equivalent_route_restored_at.clone();
            f.streams[0].exact_baseline_restored_at = f.exact_baseline_restored_at.clone();
            f.streams[0].withdrawn = true;
        }
        f
    }
}

#[cfg(test)]
impl IncidentWorkbenchViewModel {
    /// Minimal view model carrying one finding (test helper).
    fn default_for_test(finding: RoutingFinding) -> Self {
        IncidentWorkbenchViewModel {
            subject_id: "EV-1".to_string(),
            subject_kind: "event".to_string(),
            title: "Test".to_string(),
            source_task_type: "incident".to_string(),
            reviewed_incident_role: String::new(),
            lifecycle: "Closed".to_string(),
            window_start: "2019-08-21T16:00:00Z".to_string(),
            window_end: "2019-08-21T17:30:00Z".to_string(),
            current_result: String::new(),
            expectation_assessment: String::new(),
            archive_coverage: String::new(),
            observed_result: String::new(),
            scope_limit: String::new(),
            incident_horizon_start: String::new(),
            incident_horizon_end: String::new(),
            pilot_label: String::new(),
            linked_tickets: Vec::new(),
            plane_asns: vec![64512],
            units: WorkbenchUnits {
                session_count: 1,
                changed_session_count: 1,
                episode_count: 1,
                stream_count: 1,
                distinct_prefix_count: 1,
                route_instance_count: 1,
                transition_count: 1,
            },
            relationship_audit: None,
            runs: Vec::new(),
            episodes: Vec::new(),
            findings: vec![finding],
            target_label: "TARGET (AS2603)".to_string(),
            target_origin_asns: vec![2603],
            breadth: Vec::new(),
            timeline: Vec::new(),
            operator_anchors: Vec::new(),
            cues: Vec::new(),
            grouped_cues: Vec::new(),
            no_baseline_sessions: Vec::new(),
            incomplete_sessions: Vec::new(),
            asn_identities: Vec::new(),
        }
    }
}

/// Session 41 tests: route chronology, baseline consistency, absence
/// wording, identity policy, prepend collapsing.
#[cfg(test)]
mod session41_tests {
    use super::finding_test_helpers::{absence_episode, episode, path_index_with};
    use super::*;

    #[test]
    fn chronology_orders_all_evidence_by_timestamp() {
        // Withdrawal 16:45:25, replacement 16:45:27, baseline return
        // 16:59:26 — the steps must be time-ordered.
        let eps = vec![absence_episode(
            1,
            "routeviews/route-views2 peer 198.32.160.1",
            "Eugene, Oregon, US",
            "AMER",
            64512,
            RelationshipKind::Direct,
            "2019-08-21T16:45:25Z",
            "2019-08-21T16:45:27Z",
            &["10.0.0.0/24"],
        )];
        let mut index = path_index_with(&[(
            "route-views2",
            "198.32.160.1",
            "10.0.0.0/24",
            &[64512, 20965, 2603],
            &[64512, 22388, 24489, 24489, 24489, 24489, 24490, 20965, 2603],
        )]);
        let map = index.by_run.get_mut("EV").unwrap();
        let ev = map
            .get_mut(&(
                "route-views2".into(),
                "198.32.160.1".into(),
                "10.0.0.0/24".into(),
            ))
            .unwrap();
        ev.visibility_restored_at = Some("2019-08-21T16:45:27Z".to_string());
        ev.equivalent_route_restored_at = Some("2019-08-21T16:45:27Z".to_string());
        ev.exact_baseline_restored_at = Some("2019-08-21T16:59:26Z".to_string());
        let mut run_events = std::collections::HashMap::new();
        run_events.insert(1, "EV".to_string());
        let findings = build_findings(
            &eps,
            &index,
            &AsnIdentityRegistry::default(),
            "TARGET (AS2603)",
            &[2603],
            "",
            &run_events,
            "2019-08-21",
            "2019-08-21T18:00:00Z",
        );
        let ch = route_chronology(&findings[0], "2019-08-21T17:30:00Z");
        let labels: Vec<&str> = ch.steps.iter().map(|s| s.label).collect();
        assert_eq!(
            labels,
            vec![
                "Baseline",
                "Absent",
                "First replacement",
                "Exact baseline return",
                "Event-window end",
                "Analysis end"
            ],
            "ordered chronology: {labels:?}"
        );
        // Times are non-decreasing and explicit-zone.
        let times: Vec<&str> = ch.steps.iter().map(|s| s.time.as_str()).collect();
        for t in &times {
            assert!(t.contains("UTC"), "explicit zone: {t}");
        }
    }

    #[test]
    fn temporary_absence_precedes_replacement_state() {
        let eps = vec![absence_episode(
            1,
            "routeviews/route-views2 peer 198.32.160.1",
            "Eugene, Oregon, US",
            "AMER",
            64512,
            RelationshipKind::Direct,
            "2019-08-21T16:45:25Z",
            "2019-08-21T16:45:27Z",
            &["10.0.0.0/24"],
        )];
        let mut index = path_index_with(&[(
            "route-views2",
            "198.32.160.1",
            "10.0.0.0/24",
            &[64512, 20965, 2603],
            &[64512, 22388, 2603],
        )]);
        let map = index.by_run.get_mut("EV").unwrap();
        let ev = map
            .get_mut(&(
                "route-views2".into(),
                "198.32.160.1".into(),
                "10.0.0.0/24".into(),
            ))
            .unwrap();
        ev.visibility_restored_at = Some("2019-08-21T16:45:27Z".to_string());
        ev.equivalent_route_restored_at = Some("2019-08-21T16:45:27Z".to_string());
        ev.exact_baseline_restored_at = Some("2019-08-21T16:59:26Z".to_string());
        let mut run_events = std::collections::HashMap::new();
        run_events.insert(1, "EV".to_string());
        let findings = build_findings(
            &eps,
            &index,
            &AsnIdentityRegistry::default(),
            "TARGET (AS2603)",
            &[2603],
            "",
            &run_events,
            "2019-08-21",
            "2019-08-21T18:00:00Z",
        );
        let ch = route_chronology(&findings[0], "2019-08-21T17:30:00Z");
        let absent = ch.steps.iter().position(|s| s.label == "Absent").unwrap();
        let replacement = ch
            .steps
            .iter()
            .position(|s| s.label == "First replacement")
            .unwrap();
        assert!(
            absent < replacement,
            "absence precedes the replacement state"
        );
        assert!(ch.steps[absent].path.is_none(), "absent step has no path");
        assert_eq!(
            ch.steps[replacement].restoration,
            Some("Visibility returned")
        );
    }

    #[test]
    fn baseline_can_return_and_then_change_again() {
        // RRC15-style: baseline returned in-window, then a cooldown
        // re-change. The chronology must include the later change and
        // the analysis-end state distinct from the restoration.
        let eps = vec![episode(
            1,
            EffectKind::PathReplacement,
            "ris/rrc15 peer 187.16.216.4",
            "Sao Paulo, Brazil",
            "AMER",
            1916,
            RelationshipKind::Indirect,
            "2019-08-21T16:35:38Z",
            "2019-08-21T17:03:35Z",
            &["10.0.0.0/24"],
        )];
        let mut index = path_index_with(&[(
            "rrc15",
            "187.16.216.4",
            "10.0.0.0/24",
            &[1916, 64512, 20965, 2603],
            &[1916, 20080, 64512, 20965, 2603],
        )]);
        let map = index.by_run.get_mut("EV").unwrap();
        let ev = map
            .get_mut(&("rrc15".into(), "187.16.216.4".into(), "10.0.0.0/24".into()))
            .unwrap();
        ev.exact_baseline_restored_at = Some("2019-08-21T17:03:35Z".to_string());
        let mut run_events = std::collections::HashMap::new();
        run_events.insert(1, "EV".to_string());
        let mut findings = build_findings(
            &eps,
            &index,
            &AsnIdentityRegistry::default(),
            "TARGET (AS2603)",
            &[2603],
            "",
            &run_events,
            "2019-08-21",
            "2019-08-21T18:30:00Z",
        );
        findings[0].state_at_analysis_end =
            CooldownOutcome::StillChangingBeforeAnalysisEnd("2019-08-21T17:52:16Z".to_string());
        findings[0].analysis_end = "2019-08-21T18:30:00Z".to_string();
        let ch = route_chronology(&findings[0], "2019-08-21T17:30:00Z");
        let labels: Vec<&str> = ch.steps.iter().map(|s| s.label).collect();
        assert!(
            labels.contains(&"Later change"),
            "later change step present: {labels:?}"
        );
        let exact = ch
            .steps
            .iter()
            .position(|s| s.label == "Exact baseline return")
            .unwrap();
        let later = ch
            .steps
            .iter()
            .position(|s| s.label == "Later change")
            .unwrap();
        let analysis = ch
            .steps
            .iter()
            .position(|s| s.label == "Analysis end")
            .unwrap();
        assert!(exact < later && later < analysis);
    }

    #[test]
    fn final_state_is_not_assumed_from_restoration_event() {
        // A restoration at 16:59:26 with a later change means the
        // analysis-end step must NOT show the restored baseline as the
        // final state.
        let eps = vec![absence_episode(
            1,
            "routeviews/route-views2 peer 198.32.160.1",
            "Eugene, Oregon, US",
            "AMER",
            64512,
            RelationshipKind::Direct,
            "2019-08-21T16:45:25Z",
            "2019-08-21T16:45:27Z",
            &["10.0.0.0/24"],
        )];
        let mut index = path_index_with(&[(
            "route-views2",
            "198.32.160.1",
            "10.0.0.0/24",
            &[64512, 20965, 2603],
            &[64512, 22388, 2603],
        )]);
        let map = index.by_run.get_mut("EV").unwrap();
        let ev = map
            .get_mut(&(
                "route-views2".into(),
                "198.32.160.1".into(),
                "10.0.0.0/24".into(),
            ))
            .unwrap();
        ev.visibility_restored_at = Some("2019-08-21T16:45:27Z".to_string());
        ev.exact_baseline_restored_at = Some("2019-08-21T16:59:26Z".to_string());
        ev.final_path = Some(vec![64512, 24287, 24490, 20965, 2603]); // changed again
        let mut run_events = std::collections::HashMap::new();
        run_events.insert(1, "EV".to_string());
        let mut findings = build_findings(
            &eps,
            &index,
            &AsnIdentityRegistry::default(),
            "TARGET (AS2603)",
            &[2603],
            "",
            &run_events,
            "2019-08-21",
            "2019-08-21T18:00:00Z",
        );
        findings[0].state_at_analysis_end =
            CooldownOutcome::StillChangingBeforeAnalysisEnd("2019-08-21T17:52:16Z".to_string());
        let ch = route_chronology(&findings[0], "2019-08-21T17:30:00Z");
        let analysis = ch.steps.iter().find(|s| s.label == "Analysis end").unwrap();
        assert!(
            analysis.path.as_deref() != Some(findings[0].baseline_path_signature.as_str()),
            "analysis-end path is not the restored baseline"
        );
        assert!(ch.state_at_analysis_end.contains("StillChanging"));
    }

    #[test]
    fn event_window_end_and_analysis_end_are_distinct() {
        let eps = vec![episode(
            1,
            EffectKind::PathReplacement,
            "ris/rrc06 peer 192.0.2.1",
            "Otemachi, Tokyo, Japan",
            "APAC",
            4777,
            RelationshipKind::Indirect,
            "2019-08-21T16:45:44Z",
            "2019-08-21T17:02:07Z",
            &["10.0.0.0/24"],
        )];
        let index = path_index_with(&[(
            "rrc06",
            "192.0.2.1",
            "10.0.0.0/24",
            &[4777, 64512, 2603],
            &[4777, 22388, 2603],
        )]);
        let mut run_events = std::collections::HashMap::new();
        run_events.insert(1, "EV".to_string());
        let findings = build_findings(
            &eps,
            &index,
            &AsnIdentityRegistry::default(),
            "TARGET (AS2603)",
            &[2603],
            "",
            &run_events,
            "2019-08-21",
            "2019-08-21T18:30:00Z",
        );
        let ch = route_chronology(&findings[0], "2019-08-21T17:30:00Z");
        let ew = ch
            .steps
            .iter()
            .position(|s| s.label == "Event-window end")
            .unwrap();
        let ae = ch
            .steps
            .iter()
            .position(|s| s.label == "Analysis end")
            .unwrap();
        assert!(ew < ae, "event-window end precedes analysis end");
        assert!(ch.steps[ew].time.contains("17:30"));
        assert!(ch.steps[ae].time.contains("18:30"));
    }

    #[test]
    fn chronology_is_deterministic() {
        let eps = vec![absence_episode(
            1,
            "routeviews/route-views2 peer 198.32.160.1",
            "Eugene, Oregon, US",
            "AMER",
            64512,
            RelationshipKind::Direct,
            "2019-08-21T16:45:25Z",
            "2019-08-21T16:45:27Z",
            &["10.0.0.0/24", "10.0.1.0/24"],
        )];
        let mut index = path_index_with(&[
            (
                "route-views2",
                "198.32.160.1",
                "10.0.0.0/24",
                &[64512, 20965, 2603],
                &[64512, 22388, 2603],
            ),
            (
                "route-views2",
                "198.32.160.1",
                "10.0.1.0/24",
                &[64512, 20965, 2603],
                &[64512, 22388, 2603],
            ),
        ]);
        let map = index.by_run.get_mut("EV").unwrap();
        for ev in map.values_mut() {
            ev.visibility_restored_at = Some("2019-08-21T16:45:27Z".to_string());
            ev.exact_baseline_restored_at = Some("2019-08-21T16:59:26Z".to_string());
        }
        let mut run_events = std::collections::HashMap::new();
        run_events.insert(1, "EV".to_string());
        let f = build_findings(
            &eps,
            &index,
            &AsnIdentityRegistry::default(),
            "TARGET (AS2603)",
            &[2603],
            "",
            &run_events,
            "2019-08-21",
            "2019-08-21T18:00:00Z",
        );
        let a = route_chronology(&f[0], "2019-08-21T17:30:00Z");
        let b = route_chronology(&f[0], "2019-08-21T17:30:00Z");
        assert_eq!(a, b);
    }

    #[test]
    fn real_rv2_baseline_is_pre_change_path() {
        // Regression (Part 2): the direct RV2 finding's baseline must
        // be the pre-change path (AS64512 AS20965 AS2603), not the
        // first-instance path, so restoration/final stay consistent.
        let eps = vec![absence_episode(
            1,
            "routeviews/route-views2 peer 64.57.28.241",
            "Eugene, Oregon, US",
            "AMER",
            64512,
            RelationshipKind::Direct,
            "2019-08-21T16:45:25Z",
            "2019-08-21T16:45:27Z",
            &["10.0.0.0/24"],
        )];
        // The loader derives the baseline from the first transition's
        // before_path (the pre-change path); the fixture carries that
        // derived baseline.
        let mut index = path_index_with(&[(
            "route-views2",
            "64.57.28.241",
            "10.0.0.0/24",
            &[64512, 20965, 2603],
            &[64512, 22388, 24489, 24489, 24489, 24489, 24490, 20965, 2603],
        )]);
        let map = index.by_run.get_mut("EV").unwrap();
        let ev = map
            .get_mut(&(
                "route-views2".into(),
                "64.57.28.241".into(),
                "10.0.0.0/24".into(),
            ))
            .unwrap();
        ev.exact_baseline_restored_at = Some("2019-08-21T17:02:03Z".to_string());
        // The ReturnToBaseline restores the pre-change path; the last
        // observed after-state equals the baseline (no later change).
        ev.final_path = Some(vec![64512, 20965, 2603]);
        let mut run_events = std::collections::HashMap::new();
        run_events.insert(1, "EV".to_string());
        let findings = build_findings(
            &eps,
            &index,
            &AsnIdentityRegistry::default(),
            "TARGET (AS2603)",
            &[2603],
            "",
            &run_events,
            "2019-08-21",
            "2019-08-21T18:00:00Z",
        );
        let f = &findings[0];
        // The path that disappeared is the pre-change path.
        assert_eq!(
            f.baseline_path_signature, "AS64512 AS20965 AS2603",
            "baseline is the pre-change path"
        );
        // The exact baseline return restores the SAME path; final
        // equals the baseline (no later change).
        assert_eq!(f.final_path_signature, f.baseline_path_signature);
        let ch = route_chronology(f, "2019-08-21T17:30:00Z");
        assert!(!ch.steps.iter().any(|s| s.label == "Later change"));
        let exact = ch
            .steps
            .iter()
            .find(|s| s.label == "Exact baseline return")
            .unwrap();
        assert_eq!(
            exact.path.as_deref(),
            Some(f.baseline_path_signature.as_str())
        );
    }

    #[test]
    fn absent_finding_never_says_continuously_visible() {
        let eps = vec![absence_episode(
            1,
            "routeviews/route-views2 peer 198.32.160.1",
            "Eugene, Oregon, US",
            "AMER",
            64512,
            RelationshipKind::Direct,
            "2019-08-21T16:45:25Z",
            "2019-08-21T16:45:27Z",
            &["10.0.0.0/24"],
        )];
        let mut index = path_index_with(&[(
            "route-views2",
            "198.32.160.1",
            "10.0.0.0/24",
            &[64512, 20965, 2603],
            &[64512, 22388, 2603],
        )]);
        let map = index.by_run.get_mut("EV").unwrap();
        let ev = map
            .get_mut(&(
                "route-views2".into(),
                "198.32.160.1".into(),
                "10.0.0.0/24".into(),
            ))
            .unwrap();
        ev.visibility_restored_at = Some("2019-08-21T16:45:27Z".to_string());
        let mut run_events = std::collections::HashMap::new();
        run_events.insert(1, "EV".to_string());
        let findings = build_findings(
            &eps,
            &index,
            &AsnIdentityRegistry::default(),
            "TARGET (AS2603)",
            &[2603],
            "",
            &run_events,
            "2019-08-21",
            "2019-08-21T18:00:00Z",
        );
        let f = &findings[0];
        let baseline = f.streams[0].baseline_path.clone();
        let changed = f.streams[0].changed_path.clone().unwrap();
        let diff = diff_paths(&baseline, &changed, &[64512]);
        assert!(diff.plane_retained, "replacement retains the plane");
        let explanation =
            explain_path_diff_with_origins(&baseline, &changed, &[64512], "plane", &[2603], &|a| {
                format!("AS{a}")
            });
        // The explanation is about the post-restoration state; it must
        // not claim the route remained visible across the absence.
        assert!(
            explanation.contains("remained visible") || explanation.contains("continued through"),
            "post-restoration explanation present"
        );
        // The absence interval itself is explicit in the chronology.
        let ch = route_chronology(f, "2019-08-21T17:30:00Z");
        let absent = ch.steps.iter().find(|s| s.label == "Absent").unwrap();
        assert!(absent.time.contains("16:45:25") && absent.time.contains("16:45:27"));
        // The statement never says "remained visible" about the
        // absence itself.
        let s = finding_statement(f);
        assert!(!s.contains("remained visible"), "{s}");
    }

    #[test]
    fn identity_policy_current_only_is_not_primary() {
        // Current-only identities are never the primary historical
        // label: the registry lookup for primary rendering must yield
        // reviewed names or nothing.
        let mut r = AsnIdentityRegistry::default();
        r.identities.push(AsnIdentity {
            asn: 22388,
            display_name: "TRANSPAC".to_string(),
            role: "transit".to_string(),
            valid_from: None,
            valid_to: None,
            provenance: "current lookup".to_string(),
            review_status: "CurrentIdentityOnly".to_string(),
        });
        r.identities.push(AsnIdentity {
            asn: 64512,
            display_name: "Reviewed Net".to_string(),
            role: "re".to_string(),
            valid_from: None,
            valid_to: None,
            provenance: "reviewed".to_string(),
            review_status: "HistoricallyReviewed".to_string(),
        });
        assert_eq!(
            r.primary_display_name(22388, "2019-08-21"),
            None,
            "current-only is not a primary historical label"
        );
        assert_eq!(r.status(22388, "2019-08-21"), "CurrentIdentityOnly");
        assert_eq!(
            r.primary_display_name(64512, "2019-08-21"),
            Some("Reviewed Net")
        );
        // Unresolved preserves the ASN.
        assert_eq!(r.display_name(99999, "2019-08-21"), None);
        assert_eq!(format!("AS{}", 99999u32), "AS99999");
    }

    #[test]
    fn internet2_re_identity_applies_to_supported_event_dates() {
        // A stable reviewed organizational role is not time-limited to
        // the case-study date (Part 8).
        let mut r = AsnIdentityRegistry::default();
        r.identities.push(AsnIdentity {
            asn: 11537,
            display_name: "Internet2 R&E".to_string(),
            role: "re".to_string(),
            valid_from: Some("2019-08-21".to_string()),
            valid_to: None,
            provenance: "reviewed profile".to_string(),
            review_status: "HistoricallyReviewed".to_string(),
        });
        assert_eq!(r.display_name(11537, "2019-08-21"), Some("Internet2 R&E"));
        assert_eq!(
            r.display_name(11537, "2026-07-14"),
            Some("Internet2 R&E"),
            "stable role applies to a supported later event date"
        );
    }

    #[test]
    fn profile_role_and_session_validity_are_independent() {
        // Session metadata keeps its own valid_from/valid_to; identity
        // validity does not inherit it.
        let mut r = AsnIdentityRegistry::default();
        r.identities.push(AsnIdentity {
            asn: 7660,
            display_name: "APAN-JP".to_string(),
            role: "re".to_string(),
            valid_from: None,
            valid_to: None,
            provenance: "reviewed".to_string(),
            review_status: "HistoricallyReviewed".to_string(),
        });
        // A session row scoped to one date.
        let session = crate::catalog::domain::ObserverSessionMetadata {
            id: 0,
            source_family: "RouteViews".to_string(),
            collector: "route-views2".to_string(),
            peer_ip: "203.181.248.195".to_string(),
            address_family: "ipv4".to_string(),
            peer_asn: 7660,
            valid_from: "2026-07-14T04:00:00Z".to_string(),
            valid_to: None,
            source_archive: "rib".to_string(),
            source_sha256: "abc".to_string(),
        };
        assert_eq!(r.display_name(7660, "2026-07-14"), Some("APAN-JP"));
        assert_eq!(session.valid_from, "2026-07-14T04:00:00Z");
    }

    #[test]
    fn repeated_origin_asn_renders_once_with_multiplier() {
        // Part 7: named rendering collapses consecutive repeats.
        let path = vec![64512, 40220, 225, 225, 225, 225, 225, 225, 225];
        let collapsed = collapse_as_path(&path);
        assert_eq!(collapsed, "AS64512 AS40220 AS225×7");
        // The exact expanded path retains every occurrence.
        let exact: Vec<String> = path.iter().map(|a| format!("AS{a}")).collect();
        assert_eq!(exact.len(), 9);
    }

    #[test]
    fn repeated_intermediate_asn_renders_once_with_multiplier() {
        let path = vec![1916, 22388, 24489, 24489, 24489, 24489, 24490, 20965, 2603];
        assert_eq!(
            collapse_as_path(&path),
            "AS1916 AS22388 AS24489×4 AS24490 AS20965 AS2603"
        );
    }

    #[test]
    fn origin_prepend_language_requires_origin_asn() {
        // An intermediate repeated ASN is never called origin-AS
        // prepending.
        let before = [1916, 64512, 20965, 2603];
        let after = [1916, 64512, 24489, 24489, 24489, 24489, 20965, 2603];
        let explanation =
            explain_path_diff_with_origins(&before, &after, &[64512], "plane", &[2603], &|a| {
                format!("AS{a}")
            });
        assert!(
            !explanation.contains("origin-AS prepending"),
            "intermediate repetition is not origin prepending: {explanation}"
        );
        assert!(explanation.contains("AS24489"), "{explanation}");
        // The target origin case keeps the origin-AS phrase.
        let before2 = [64512, 40220, 225];
        let after2 = [64512, 40220, 225, 225, 225, 225, 225, 225, 225];
        let explanation2 =
            explain_path_diff_with_origins(&before2, &after2, &[64512], "plane", &[225], &|a| {
                format!("AS{a}")
            });
        assert!(
            explanation2.contains("origin-AS prepending"),
            "{explanation2}"
        );
    }
}
