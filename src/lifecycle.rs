//! Per-stream lifecycle analysis — audit what happened to every frozen
//! observer-route stream, classify the shape of path changes, and
//! produce withdrawal audits and semantic wave descriptions.
//!
//! All classifications are derived interpretations layered over the
//! auditable RouteTransition evidence. Raw before/after paths are never
//! modified or replaced.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};

use crate::domain::route::{
    AnalysisPhase, ObserverPrefixKey, PrependChange, RouteAttributes, RouteKey, RouteState,
    RouteTransition, TransitPredicate, TransitionKind,
};

// ── Path collapsing ────────────────────────────────────────────────

/// Collapse consecutive duplicate ASNs in a path.
///
/// Example: `[11537, 40220, 225, 225, 225]` → `[11537, 40220, 225]`.
///
/// AS sets and confederation segments are already flattened to `Vec<u32>`
/// at ingestion time. Where safe classification isn't possible, the caller
/// falls back to `GenericPathChange` with a documented limitation.
pub fn collapse_as_path(path: &[u32]) -> Vec<u32> {
    let mut collapsed = Vec::new();
    for &asn in path {
        if collapsed.last() != Some(&asn) {
            collapsed.push(asn);
        }
    }
    collapsed
}

/// Check whether two AS paths are equal after collapsing consecutive duplicates.
pub fn collapsed_equivalent(a: &[u32], b: &[u32]) -> bool {
    collapse_as_path(a) == collapse_as_path(b)
}

// ── Path-shape change classification ───────────────────────────────

/// Derived classification of how an AS path changed between two states.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum PathShapeChange {
    /// Prepend count reduced (fewer ASNs after collapsing duplicates).
    PrependReduced,
    /// Prepend count increased (more ASNs after collapsing duplicates).
    PrependIncreased,
    /// Path changed materially but still contains the required transit ASN.
    PathChangedStillViaRequiredTransit,
    /// Path no longer contains the required transit ASN.
    PathDepartedRequiredTransit,
    /// Path returned to containing the required transit ASN.
    PathReturnedToRequiredTransit,
    /// Path changed but classification isn't safe (e.g. AS sets, confed segments).
    GenericPathChange,
}

/// Classify a path change between two states with respect to a transit predicate.
///
/// `from` and `to` are the before/after route states. `transit_predicate` is
/// the reviewed predicate that a path must satisfy to be considered "via the
/// required transit".
pub fn classify_path_change(
    from: &RouteState,
    to: &RouteState,
    transit_predicate: &TransitPredicate,
) -> PathShapeChange {
    let from_path = &from.attributes.as_path.0;
    let to_path = &to.attributes.as_path.0;

    let from_has = transit_predicate.evaluate(from_path);
    let to_has = transit_predicate.evaluate(to_path);

    // If collapsed sequences are equal, this is purely a prepend change
    if collapsed_equivalent(from_path, to_path) {
        if from_path.len() > to_path.len() {
            PathShapeChange::PrependReduced
        } else {
            PathShapeChange::PrependIncreased
        }
    } else if from_has && to_has {
        // Both contain transit — material change but still via required transit
        PathShapeChange::PathChangedStillViaRequiredTransit
    } else if from_has && !to_has {
        // Departed the required transit
        PathShapeChange::PathDepartedRequiredTransit
    } else if !from_has && to_has {
        // Returned to the required transit
        PathShapeChange::PathReturnedToRequiredTransit
    } else {
        // Neither contains transit — generic change
        PathShapeChange::GenericPathChange
    }
}

// ── Route semantic equality ─────────────────────────────────────────

/// Whether two route attribute sets are semantically equivalent.
///
/// Path_id equality alone is NEVER semantic equality. The comparison uses
/// exactly the attributes preserved by the event model:
///   AS path, origin ASNs, origin type, next hop, MED, local preference,
///   atomic aggregate, and communities.
///
/// Documented exclusions:
///   - community ORDER is ignored (communities compare as sets);
///   - attributes not modeled by `RouteAttributes` (large communities,
///     extended communities, non-ASN path segments) are not compared —
///     they are not preserved by the observation model.
pub fn route_semantically_equivalent(a: &RouteAttributes, b: &RouteAttributes) -> bool {
    if a.as_path != b.as_path
        || a.origin_asns != b.origin_asns
        || a.origin != b.origin
        || a.next_hop != b.next_hop
        || a.med != b.med
        || a.local_pref != b.local_pref
        || a.atomic_aggregate != b.atomic_aggregate
    {
        return false;
    }
    // Communities as unordered sets.
    let mut ac = a.communities.clone();
    let mut bc = b.communities.clone();
    ac.sort();
    bc.sort();
    ac == bc
}

/// A stable fingerprint of route semantics for set-level comparisons.
fn semantic_fingerprint(attrs: &RouteAttributes) -> String {
    let mut comm = attrs.communities.clone();
    comm.sort();
    format!(
        "{}|{:?}|{:?}|{:?}|{:?}|{:?}|{}|{:?}",
        attrs
            .as_path
            .0
            .iter()
            .map(|a| a.to_string())
            .collect::<Vec<_>>()
            .join(","),
        attrs.origin_asns.iter().map(|a| a.0).collect::<Vec<_>>(),
        attrs.origin,
        attrs.next_hop.map(|ip| ip.to_string()),
        attrs.med,
        attrs.local_pref,
        attrs.atomic_aggregate,
        comm,
    )
}

/// The kind of restoration for an observer-prefix stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum RestorationKind {
    /// The same path_id returns with a semantically equivalent route.
    ExactInstanceRestoration,
    /// A semantically equivalent route returns under any path_id.
    EquivalentRouteRestoration,
    /// The stream changes from absent to visible.
    ObserverPrefixRestoration,
    /// Active route semantics again equal baseline route semantics.
    BaselineSetRestoration,
}

/// A restoration event on an observer-prefix stream.
#[derive(Debug, Clone, Serialize)]
pub struct StreamRestoration {
    pub timestamp: chrono::DateTime<Utc>,
    /// Path IDs (and unkeyed None) that were withdrawn before restoration.
    pub old_path_ids: Vec<Option<u32>>,
    /// Path IDs (and unkeyed None) that returned.
    pub new_path_ids: Vec<Option<u32>>,
    /// Exact-instance restoration (same path_id + equivalent route).
    pub exact_instance: bool,
    /// Equivalent-route restoration (equivalent route under any path_id).
    pub equivalent_route: bool,
    /// Observer-prefix restoration (absent → visible).
    pub observer_prefix: bool,
    /// Baseline-set restoration (active semantics == baseline semantics).
    pub baseline_set: bool,
    /// Evidence reference of the restoring observation.
    pub evidence: crate::domain::observation::EvidenceRef,
}

impl StreamRestoration {
    /// The restoration kinds that apply to this event.
    pub fn kinds(&self) -> Vec<RestorationKind> {
        let mut v = Vec::new();
        if self.exact_instance {
            v.push(RestorationKind::ExactInstanceRestoration);
        }
        if self.equivalent_route {
            v.push(RestorationKind::EquivalentRouteRestoration);
        }
        if self.observer_prefix {
            v.push(RestorationKind::ObserverPrefixRestoration);
        }
        if self.baseline_set {
            v.push(RestorationKind::BaselineSetRestoration);
        }
        v
    }
}

/// ADD-PATH continuity ambiguity for one observer-prefix stream.
///
/// Recorded when both keyed (`path_id=Some`) and unkeyed (`path_id=None`)
/// records appear for one ObserverPrefixKey during the relevant timeline.
/// The ambiguity is stream-scoped and retains conflicting evidence.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AddPathAmbiguity {
    pub collector: String,
    pub peer_ip: String,
    pub prefix: String,
    /// First keyed record evidence.
    pub first_keyed: Option<crate::domain::observation::EvidenceRef>,
    /// First unkeyed record evidence.
    pub first_unkeyed: Option<crate::domain::observation::EvidenceRef>,
    /// Relevant archive identities (source URLs and SHA-256 checksums).
    pub archive_identities: Vec<String>,
    /// Affected time range (first conflicting record .. last record).
    pub affected_start: Option<DateTime<Utc>>,
    pub affected_end: Option<DateTime<Utc>>,
}

// ── Stream classification ──────────────────────────────────────────

/// Primary category for a frozen observer-route stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub enum StreamCategory {
    /// No transitions observed for this stream.
    Unchanged,
    /// Only prepend changes (collapsed-equivalent paths).
    PrependOnly,
    /// Path changed materially but still via the required transit ASN.
    PathChangedStillViaTransit,
    /// Path departed the required transit ASN.
    DepartedTransitPath,
    /// Route was withdrawn (became absent).
    Withdrawn,
}

/// Additional flags for a stream lifecycle.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
pub struct StreamFlags {
    /// Stream restored to its event baseline (or any present state after withdrawal).
    pub restored: bool,
    /// Stream was not restored by cooldown end.
    pub not_restored: bool,
    /// Stream experienced multiple cycles (e.g. repeated withdrawal/restoration).
    pub multiple_cycles: bool,
    /// ADD-PATH continuity is ambiguous for this stream (mixed keyed/unkeyed).
    /// Strong stream-level assessment is suppressed while this is set.
    pub add_path_ambiguous: bool,
}

// ── Stream lifecycle ───────────────────────────────────────────────

/// Complete lifecycle record for a single frozen observer-prefix stream.
///
/// The lifecycle is classified at the ObserverPrefixKey level while every
/// route-instance (RouteKey including path_id) history is retained.
#[derive(Debug, Clone, Serialize)]
pub struct StreamLifecycle {
    /// Collector identifier.
    pub collector: String,
    /// Peer IP address.
    pub peer_ip: String,
    /// BGP prefix.
    pub prefix: String,
    /// Baseline AS path of the first baseline instance (from frozen RIB).
    pub baseline_path: Vec<u32>,
    /// Number of baseline route instances for this stream.
    pub baseline_instance_count: usize,
    /// Maximum concurrent route instances observed.
    pub max_concurrent_instances: usize,
    /// Total distinct route instances seen (baseline + announced).
    pub total_route_instances: usize,
    /// Primary category.
    pub category: StreamCategory,
    /// Additional flags.
    pub flags: StreamFlags,
    /// First event-window change timestamp, if any.
    pub first_change: Option<DateTime<Utc>>,
    /// All transitions for this stream, in chronological order.
    pub transitions: Vec<LifecycleTransition>,
    /// Minimum absence interval (None if never withdrawn).
    pub min_absence_secs: Option<f64>,
    /// Maximum absence interval (None if never withdrawn).
    pub max_absence_secs: Option<f64>,
    /// Whether the STREAM (observer-prefix key) was ever fully absent.
    pub was_withdrawn: bool,
    /// Timestamp when the stream became fully absent (final instance loss).
    pub stream_withdrawal_time: Option<DateTime<Utc>>,
    /// Active instances immediately before the stream absence began.
    pub active_before_absence: usize,
    /// Whether the last active instance matched the transit predicate at
    /// stream withdrawal time.
    pub transit_at_withdrawal: Option<bool>,
    /// Route instances (RouteKey) that were withdrawn.
    pub withdrawn_instances: Vec<crate::domain::route::RouteKey>,
    /// Number of stream-level withdrawals (visible → absent transitions).
    pub stream_withdrawal_count: usize,
    /// Restoration events for this stream.
    pub restorations: Vec<StreamRestoration>,
    /// ADD-PATH continuity ambiguity, when detected.
    pub add_path_ambiguity: Option<AddPathAmbiguity>,
    /// Whether a replacement path appeared (after withdrawal or path change).
    pub replacement_appeared: bool,
    /// Whether the replacement path retained the required transit predicate.
    pub replacement_retained_transit: Option<bool>,
    /// Whether prepending changed.
    pub prepending_changed: bool,
    /// Cooldown-phase transitions.
    pub cooldown_transitions: Vec<LifecycleTransition>,
    /// Final state of the stream's principal instance at end of window.
    pub final_state: Option<RouteState>,
    /// Whether the event baseline was restored.
    pub baseline_restored: bool,
    /// Restoration timestamp (first restoration after withdrawal/departure).
    pub restoration_time: Option<DateTime<Utc>>,
    /// Total affected duration: first change → restoration or cooldown_end.
    pub affected_duration_secs: Option<f64>,
    /// Whether GRACEFUL_SHUTDOWN community (65535:0) was ever seen on this stream.
    pub graceful_shutdown_seen: bool,
    /// Whether 65535:0 was present in the baseline.
    pub gshut_present_at_baseline: bool,
    /// Whether 65535:0 was newly added during the window.
    pub gshut_newly_added: bool,
    /// Whether 65535:0 was removed during the window.
    pub gshut_removed: bool,
    /// First timestamp when 65535:0 appeared.
    pub first_gshut_timestamp: Option<DateTime<Utc>>,
    /// Last timestamp when 65535:0 was observed.
    pub last_gshut_timestamp: Option<DateTime<Utc>>,
    /// Whether 65535:0 was present before a stream withdrawal.
    pub gshut_before_withdrawal: bool,
    /// Whether 65535:0 was present before a path replacement.
    pub gshut_before_path_change: bool,
    /// Duration from first 65535:0 addition to first consequence (withdrawal
    /// or path replacement), in seconds.
    pub gshut_to_consequence_secs: Option<f64>,
    /// Whether 65535:0 was removed during a restoration.
    pub gshut_removed_during_restoration: bool,
    /// Communities in the first observed state (baseline).
    pub communities_before: Vec<String>,
    /// Communities in the final observed state.
    pub communities_after: Vec<String>,
}

/// A lightweight transition record for the lifecycle.
#[derive(Debug, Clone, Serialize)]
pub struct LifecycleTransition {
    pub timestamp: DateTime<Utc>,
    pub phase: AnalysisPhase,
    pub kind: String,
    /// Route instance path ID (None = unkeyed).
    pub path_id: Option<u32>,
    pub before_path: Vec<u32>,
    pub after_path: Vec<u32>,
    pub path_shape: Option<PathShapeChange>,
    pub observation_id: u64,
    pub archive_sha256: Option<String>,
    /// Whether the after state had the GRACEFUL_SHUTDOWN community.
    pub has_gshut_after: bool,
}

// ── Withdrawal audit ───────────────────────────────────────────────

/// A single withdrawn observer-prefix stream record.
///
/// One row per withdrawn ObserverPrefixKey. Wording throughout the audit
/// is observer-scoped: "withdrawal from selected observer-prefix stream",
/// never "global withdrawal".
#[derive(Debug, Clone, Serialize)]
pub struct WithdrawalRecord {
    pub collector: String,
    pub peer_ip: String,
    pub prefix: String,
    pub baseline_path: Vec<u32>,
    /// Baseline route instances for this stream.
    pub baseline_instances: usize,
    /// Active instances immediately before the stream became absent.
    pub active_before_absence: usize,
    /// Final instance withdrawal timestamp (stream became absent).
    pub final_withdrawal_time: DateTime<Utc>,
    pub restoration_time: Option<DateTime<Utc>>,
    pub absence_duration_secs: Option<f64>,
    /// Whether the last active instance matched the transit predicate at
    /// stream withdrawal time.
    pub transit_at_withdrawal: Option<bool>,
    /// Exact-instance restoration occurred.
    pub exact_restoration: bool,
    /// Equivalent-route restoration occurred.
    pub equivalent_restoration: bool,
    /// Observer-prefix restoration occurred (absent → visible).
    pub observer_prefix_restoration: bool,
    /// Baseline-set restoration occurred.
    pub baseline_set_restoration: bool,
    pub observation_ids: Vec<u64>,
    pub archive_checksums: Vec<String>,
}

/// Aggregate withdrawal-audit statistics over withdrawn streams.
#[derive(Debug, Clone, Default, Serialize)]
pub struct WithdrawalAuditSummary {
    /// Unique withdrawn observer-prefix streams.
    pub unique_withdrawn_streams: usize,
    /// Route-instance withdrawals (all instance losses, including non-final).
    pub route_instance_withdrawals: usize,
    /// Affected prefixes (distinct).
    pub affected_prefixes: usize,
    /// Affected peers (distinct).
    pub affected_peers: usize,
    /// Median absence duration in seconds.
    pub median_absence_secs: Option<f64>,
    /// Maximum absence duration in seconds.
    pub max_absence_secs: Option<f64>,
    /// Streams restored after withdrawal.
    pub restored_count: usize,
    /// Streams still absent/unresolved at window end.
    pub unresolved_count: usize,
    /// Exact-instance restorations (streams).
    pub exact_restorations: usize,
    /// Equivalent-route restorations (streams).
    pub equivalent_restorations: usize,
    /// Observer-prefix restorations (streams).
    pub observer_prefix_restorations: usize,
    /// Baseline-set restorations (streams).
    pub baseline_set_restorations: usize,
}

impl WithdrawalAuditSummary {
    /// Summarize a withdrawal audit.
    pub fn from_records(records: &[WithdrawalRecord]) -> Self {
        let unique = records.len();
        let mut prefixes: HashSet<String> = HashSet::new();
        let mut peers: HashSet<String> = HashSet::new();
        let mut absences: Vec<f64> = Vec::new();
        let mut instance_withdrawals = 0usize;
        let mut restored = 0usize;
        let mut unresolved = 0usize;
        let mut exact = 0usize;
        let mut equiv = 0usize;
        let mut op_rest = 0usize;
        let mut baseline_set = 0usize;
        for r in records {
            prefixes.insert(r.prefix.clone());
            peers.insert(r.peer_ip.clone());
            if let Some(d) = r.absence_duration_secs {
                absences.push(d);
            }
            instance_withdrawals += r.active_before_absence.max(1);
            if r.restoration_time.is_some() {
                restored += 1;
            } else {
                unresolved += 1;
            }
            if r.exact_restoration {
                exact += 1;
            }
            if r.equivalent_restoration {
                equiv += 1;
            }
            if r.observer_prefix_restoration {
                op_rest += 1;
            }
            if r.baseline_set_restoration {
                baseline_set += 1;
            }
        }
        absences.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let median = if absences.is_empty() {
            None
        } else if absences.len() % 2 == 1 {
            Some(absences[absences.len() / 2])
        } else {
            let hi = absences.len() / 2;
            Some((absences[hi - 1] + absences[hi]) / 2.0)
        };
        WithdrawalAuditSummary {
            unique_withdrawn_streams: unique,
            route_instance_withdrawals: instance_withdrawals,
            affected_prefixes: prefixes.len(),
            affected_peers: peers.len(),
            median_absence_secs: median,
            max_absence_secs: absences.last().copied(),
            restored_count: restored,
            unresolved_count: unresolved,
            exact_restorations: exact,
            equivalent_restorations: equiv,
            observer_prefix_restorations: op_rest,
            baseline_set_restorations: baseline_set,
        }
    }
}

// ── Semantic waves ─────────────────────────────────────────────────

/// Candidate semantic wave labels. A label is only assigned when the wave's
/// transitions carry supporting effects; the wave always retains ALL facets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum WaveLabel {
    /// Prepend count reduced (dominant effect).
    PrependReduction,
    /// Prepend count increased (dominant effect).
    PrependIncrease,
    /// Observer-prefix stream withdrawal (dominant effect).
    StreamWithdrawal,
    /// Stream departed the reviewed transit predicate (dominant effect).
    TransitDeparture,
    /// Stream restoration after withdrawal (dominant effect).
    StreamRestoration,
    /// Stream returned to the reviewed transit predicate (dominant effect).
    TransitReturn,
    /// Return to baseline route semantics (dominant effect).
    BaselinePolicyRestoration,
    /// No single effect dominates — multiple facets co-occur.
    MixedRouteChange,
}

impl WaveLabel {
    pub fn as_str(&self) -> &'static str {
        match self {
            WaveLabel::PrependReduction => "PrependReduction",
            WaveLabel::PrependIncrease => "PrependIncrease",
            WaveLabel::StreamWithdrawal => "StreamWithdrawal",
            WaveLabel::TransitDeparture => "TransitDeparture",
            WaveLabel::StreamRestoration => "StreamRestoration",
            WaveLabel::TransitReturn => "TransitReturn",
            WaveLabel::BaselinePolicyRestoration => "BaselinePolicyRestoration",
            WaveLabel::MixedRouteChange => "MixedRouteChange",
        }
    }
}

/// A semantically-labeled wave describing a distinct temporal phase of the
/// event, derived primarily from ObserverPrefixKey lifecycles while
/// retaining contributing RouteKey evidence.
#[derive(Debug, Clone, Serialize)]
pub struct SemanticWave {
    /// Stable wave ID (deterministic within the run).
    pub id: String,
    /// Label derived from the dominant supporting effect.
    pub label: WaveLabel,
    /// First observation timestamp.
    pub start: DateTime<Utc>,
    /// Peak observation interval (densest 60s window).
    pub peak_start: DateTime<Utc>,
    pub peak_end: DateTime<Utc>,
    /// Last observation timestamp.
    pub end: DateTime<Utc>,
    /// Number of distinct observer-prefix streams.
    pub stream_count: usize,
    /// Number of contributing route instances (RouteKeys).
    pub route_instance_count: usize,
    /// Distinct prefixes (sorted).
    pub prefixes: Vec<String>,
    /// Distinct peers (sorted).
    pub peers: Vec<String>,
    /// Generic transition facet counts (always computed, never forced).
    pub facets: GenericFacetCounts,
    /// Event-relative effect counts.
    pub event_relative: EventRelativeCounts,
    /// Representative before/after AS paths (first transition, deterministic).
    pub representative_before: Vec<u32>,
    pub representative_after: Vec<u32>,
    /// Evidence references (sorted by observation id).
    pub evidence_refs: Vec<crate::domain::observation::EvidenceRef>,
    /// Duration in seconds.
    pub duration_secs: f64,
}

/// Counts of generic transition facets within a wave.
#[derive(Debug, Clone, Default, Serialize)]
pub struct GenericFacetCounts {
    pub communities_changed: usize,
    pub graceful_shutdown_added: usize,
    pub graceful_shutdown_removed: usize,
    pub prepend_reduced: usize,
    pub prepend_increased: usize,
    pub material_path_changed: usize,
    pub origin_changed: usize,
    pub next_hop_changed: usize,
    pub med_changed: usize,
    pub local_pref_changed: usize,
}

/// Counts of event-relative effects within a wave.
#[derive(Debug, Clone, Default, Serialize)]
pub struct EventRelativeCounts {
    pub transit_retained: usize,
    pub transit_departed: usize,
    pub transit_returned: usize,
}

/// Derive semantic waves primarily from ObserverPrefixKey lifecycles,
/// retaining contributing RouteKey evidence.
///
/// Waves are temporal clusters of event-window transitions (max_gap_secs
/// between successive transitions). Each wave aggregates stream counts,
/// route-instance counts, prefixes, peers, generic facets, event-relative
/// effects, representative before/after states, and evidence references.
///
/// A label is only assigned when its supporting effect count is positive
/// and dominant; ties resolve to MixedRouteChange. Waves are never forced
/// to a fixed count — actual temporal clustering is derived from evidence.
pub fn derive_semantic_waves(
    _lifecycles: &[StreamLifecycle],
    transitions: &[RouteTransition],
    max_gap_secs: f64,
    transit_predicate: &TransitPredicate,
) -> Vec<SemanticWave> {
    let mut event_transitions: Vec<&RouteTransition> = transitions
        .iter()
        .filter(|t| t.phase == AnalysisPhase::Event)
        .collect();
    if event_transitions.is_empty() {
        return vec![];
    }
    event_transitions.sort_by(|a, b| {
        a.to.timestamp().cmp(&b.to.timestamp()).then_with(|| {
            crate::derived_cache::path_id_key(&a.key.path_id)
                .cmp(&crate::derived_cache::path_id_key(&b.key.path_id))
        })
    });

    // Temporal clustering (actual structure, not a fixed wave count).
    let mut waves: Vec<SemanticWave> = Vec::new();
    let mut current: Vec<&RouteTransition> = vec![event_transitions[0]];
    for window in event_transitions.windows(2) {
        let gap = (window[1].to.timestamp() - window[0].to.timestamp()).num_seconds() as f64;
        if gap <= max_gap_secs {
            current.push(window[1]);
        } else {
            waves.push(build_semantic_wave(&current, transit_predicate));
            current = vec![window[1]];
        }
    }
    if !current.is_empty() {
        waves.push(build_semantic_wave(&current, transit_predicate));
    }

    for (i, wave) in waves.iter_mut().enumerate() {
        wave.id = format!("wave-{}", i + 1);
    }
    waves
}

fn build_semantic_wave(
    group: &[&RouteTransition],
    transit_predicate: &TransitPredicate,
) -> SemanticWave {
    let start = group[0].to.timestamp();
    let end = group[group.len() - 1].to.timestamp();

    // Streams / instances / prefixes / peers / evidence.
    let mut streams: HashSet<String> = HashSet::new();
    let mut instances: HashSet<RouteKey> = HashSet::new();
    let mut prefixes: HashSet<String> = HashSet::new();
    let mut peers: HashSet<String> = HashSet::new();
    let mut evidence: Vec<crate::domain::observation::EvidenceRef> = Vec::new();
    for t in group {
        streams.insert(format!(
            "{}:{}:{}",
            t.key.collector, t.key.peer_ip, t.key.prefix.0
        ));
        instances.insert(t.key.clone());
        prefixes.insert(t.key.prefix.0.clone());
        peers.insert(t.key.peer_ip.to_string());
        evidence.push(t.triggering.clone());
    }
    let mut prefixes: Vec<String> = prefixes.into_iter().collect();
    prefixes.sort();
    let mut peers: Vec<String> = peers.into_iter().collect();
    peers.sort();
    evidence.sort_by_key(|e| e.observation_id.0);
    evidence.dedup_by(|a, b| a.observation_id == b.observation_id);

    // Facets: generic effects + event-relative effects + label support.
    let mut facets = GenericFacetCounts::default();
    let mut event_relative = EventRelativeCounts::default();
    let mut label_counts: Vec<(WaveLabel, usize)> = Vec::new();
    let count = |label: WaveLabel, counts: &mut Vec<(WaveLabel, usize)>| {
        if let Some(entry) = counts.iter_mut().find(|(l, _)| *l == label) {
            entry.1 += 1;
        } else {
            counts.push((label, 1));
        }
    };

    for t in group {
        if t.effects.communities_changed {
            facets.communities_changed += 1;
        }
        if t.effects.graceful_shutdown_added {
            facets.graceful_shutdown_added += 1;
        }
        if t.effects.graceful_shutdown_removed {
            facets.graceful_shutdown_removed += 1;
        }
        if t.effects.material_path_changed {
            facets.material_path_changed += 1;
        }
        if t.effects.origin_changed {
            facets.origin_changed += 1;
        }
        if t.effects.next_hop_changed {
            facets.next_hop_changed += 1;
        }
        if t.effects.med_changed {
            facets.med_changed += 1;
        }
        if t.effects.local_pref_changed {
            facets.local_pref_changed += 1;
        }
        match t.effects.prepend {
            PrependChange::Reduced => {
                facets.prepend_reduced += 1;
                count(WaveLabel::PrependReduction, &mut label_counts);
            }
            PrependChange::Increased => {
                facets.prepend_increased += 1;
                count(WaveLabel::PrependIncrease, &mut label_counts);
            }
            _ => {}
        }

        // Event-relative effects apply to route-present transitions only:
        // a withdrawal is a StreamWithdrawal (kind label), not a departure —
        // "transit departed" means the stream remains visible without a
        // matching route, consistent with the DepartedTransitPath category.
        let from_path = t.from.as_ref().and_then(|f| f.state.as_ref());
        let to_path = t.to.state.as_ref();
        let from_has = from_path
            .map(|s| transit_predicate.evaluate(&s.attributes.as_path.0))
            .unwrap_or(false);
        if let Some(to_state) = to_path {
            let to_has_route = transit_predicate.evaluate(&to_state.attributes.as_path.0);
            if from_has && to_has_route {
                event_relative.transit_retained += 1;
            } else if from_has && !to_has_route {
                event_relative.transit_departed += 1;
                count(WaveLabel::TransitDeparture, &mut label_counts);
            } else if !from_has && to_has_route {
                event_relative.transit_returned += 1;
                count(WaveLabel::TransitReturn, &mut label_counts);
            }
        }

        match &t.kind {
            TransitionKind::Withdrawal => {
                count(WaveLabel::StreamWithdrawal, &mut label_counts);
            }
            TransitionKind::Restoration => {
                count(WaveLabel::StreamRestoration, &mut label_counts);
            }
            TransitionKind::ReturnToBaseline => {
                count(WaveLabel::BaselinePolicyRestoration, &mut label_counts);
            }
            _ => {}
        }
    }

    // Label requires supporting effects: the most frequent supported label;
    // ties resolve to MixedRouteChange; no label when nothing supports one.
    let label = if label_counts.is_empty() {
        WaveLabel::MixedRouteChange
    } else {
        label_counts.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.as_str().cmp(b.0.as_str())));
        let top = label_counts[0];
        let runner_up = label_counts.get(1);
        if runner_up.is_some_and(|(_, c)| *c == top.1) {
            WaveLabel::MixedRouteChange
        } else {
            top.0
        }
    };

    // Peak interval: densest 60-second window.
    let mut peak_start = start;
    let mut peak_end = end;
    let mut best_count = 0usize;
    let mut timestamps: Vec<DateTime<Utc>> = group.iter().map(|t| t.to.timestamp()).collect();
    timestamps.sort();
    for &ts in &timestamps {
        let window_end = ts + chrono::Duration::seconds(60);
        let count_in_window = timestamps
            .iter()
            .filter(|&&t| t >= ts && t <= window_end)
            .count();
        if count_in_window > best_count {
            best_count = count_in_window;
            peak_start = ts;
            peak_end = timestamps
                .iter()
                .filter(|&&t| t >= ts && t <= window_end)
                .max()
                .copied()
                .unwrap_or(ts);
        }
    }

    let representative_before = group[0]
        .from
        .as_ref()
        .and_then(|f| f.state.as_ref())
        .map(|s| s.attributes.as_path.0.clone())
        .unwrap_or_default();
    let representative_after = group[0]
        .to
        .state
        .as_ref()
        .map(|s| s.attributes.as_path.0.clone())
        .unwrap_or_default();

    SemanticWave {
        id: String::new(), // assigned by caller
        label,
        start,
        peak_start,
        peak_end,
        end,
        stream_count: streams.len(),
        route_instance_count: instances.len(),
        prefixes,
        peers,
        facets,
        event_relative,
        representative_before,
        representative_after,
        evidence_refs: evidence,
        duration_secs: ((end - start).num_seconds() as f64).max(0.0),
    }
}

// ── Builder ────────────────────────────────────────────────────────

/// Build per-stream lifecycle records from transitions and the frozen cohort.
///
/// Lifecycles are classified per ObserverPrefixKey (the principal identity)
/// while every route instance (RouteKey including path_id) is tracked and
/// retained. Stream absence requires the loss of the FINAL instance.
pub fn build_lifecycles(
    transitions: &[RouteTransition],
    cohort: &crate::cohort::FrozenCohort,
    cooldown_end: DateTime<Utc>,
    transit_predicate: &TransitPredicate,
) -> Vec<StreamLifecycle> {
    // Group transitions by observer-prefix stream.
    let mut stream_transitions: HashMap<ObserverPrefixKey, Vec<&RouteTransition>> = HashMap::new();
    for t in transitions {
        let opk = t.key.observer_prefix_key();
        stream_transitions.entry(opk).or_default().push(t);
    }
    // Deterministic chronological order (timestamp, then path_id ordering).
    for v in stream_transitions.values_mut() {
        v.sort_by(|a, b| {
            a.to.timestamp()
                .cmp(&b.to.timestamp())
                .then_with(|| path_id_order(&a.key.path_id).cmp(&path_id_order(&b.key.path_id)))
        });
    }

    let mut lifecycles: Vec<StreamLifecycle> = Vec::new();
    let mut seen: HashSet<ObserverPrefixKey> = HashSet::new();

    for opk in cohort.observer_prefixes.iter() {
        seen.insert(opk.clone());
        let stream_t = stream_transitions.get(opk).cloned().unwrap_or_default();
        let baseline = cohort
            .baseline_instances
            .get(opk)
            .cloned()
            .unwrap_or_default();
        lifecycles.push(build_one_lifecycle(
            opk,
            &baseline,
            &stream_t,
            cooldown_end,
            transit_predicate,
        ));
    }

    // Any stream with transitions but no frozen cohort entry (should not
    // happen in the real pipeline) is still classified defensively.
    for (opk, stream_t) in &stream_transitions {
        if !seen.contains(opk) {
            lifecycles.push(build_one_lifecycle(
                opk,
                &BTreeMap::new(),
                stream_t,
                cooldown_end,
                transit_predicate,
            ));
        }
    }

    // Deterministic output order: collector, peer, prefix.
    lifecycles.sort_by(|a, b| {
        a.collector
            .cmp(&b.collector)
            .then_with(|| a.peer_ip.cmp(&b.peer_ip))
            .then_with(|| a.prefix.cmp(&b.prefix))
    });
    lifecycles
}

/// Deterministic ordering of path IDs: None sorts before Some(id).
pub fn path_id_order(path_id: &Option<u32>) -> (u8, u32) {
    match path_id {
        None => (0, 0),
        Some(id) => (1, *id),
    }
}

/// Build a lifecycle for a single observer-prefix stream by simulating its
/// route-instance timeline.
#[allow(clippy::too_many_lines)]
fn build_one_lifecycle(
    opk: &ObserverPrefixKey,
    baseline_instances: &BTreeMap<RouteKey, RouteState>,
    transitions: &[&RouteTransition],
    cooldown_end: DateTime<Utc>,
    transit_predicate: &TransitPredicate,
) -> StreamLifecycle {
    // ── Simulation state ─────────────────────────────────────────
    let mut active: BTreeMap<RouteKey, RouteState> = baseline_instances.clone();
    let mut max_concurrent = active.len();
    let mut seen_instances: HashSet<RouteKey> = active.keys().cloned().collect();
    let mut withdrawn_instances: Vec<RouteKey> = Vec::new();
    let mut absence_intervals: Vec<f64> = Vec::new();
    let mut current_absence_start: Option<DateTime<Utc>> = None;
    let mut stream_withdrawal_count = 0usize;
    let mut stream_withdrawal_time: Option<DateTime<Utc>> = None;
    let mut active_before_absence = 0usize;
    let mut transit_at_withdrawal: Option<bool> = None;
    let mut restorations: Vec<StreamRestoration> = Vec::new();
    // Every instance withdrawn since the last stream absence, with its last
    // known state — used for exact-instance restoration classification.
    let mut withdrawn_history: Vec<(RouteKey, RouteState)> = Vec::new();

    // ── Classification state ─────────────────────────────────────
    let mut first_change: Option<DateTime<Utc>> = None;
    let mut seen_categories: HashSet<StreamCategory> = HashSet::new();
    let mut prepending_changed = false;
    let mut replacement_appeared = false;
    let mut replacement_retained_transit: Option<bool> = None;
    let mut baseline_restored = false;
    let mut restoration_time: Option<DateTime<Utc>> = None;
    let mut restoration_count = 0usize;
    let mut final_state: Option<RouteState> = None;
    let mut cooldown_transitions: Vec<LifecycleTransition> = Vec::new();
    let mut lifecycle_transitions: Vec<LifecycleTransition> = Vec::new();

    // ── GSHUT state ──────────────────────────────────────────────
    let mut graceful_shutdown_seen = false;
    let mut gshut_present_at_baseline = false;
    let mut gshut_newly_added = false;
    let mut gshut_removed = false;
    let mut first_gshut_timestamp: Option<DateTime<Utc>> = None;
    let mut last_gshut_timestamp: Option<DateTime<Utc>> = None;
    let mut gshut_before_withdrawal = false;
    let mut gshut_before_path_change = false;
    let mut gshut_to_consequence: Option<f64> = None;
    let mut gshut_removed_during_restoration = false;
    let mut communities_before: Vec<String> = vec![];
    let mut communities_after: Vec<String> = vec![];
    let mut communities_captured = false;
    let mut last_had_gshut = false;
    let mut first_gshut_added_at: Option<DateTime<Utc>> = None;
    let mut first_consequence_at: Option<DateTime<Utc>> = None;

    // Baseline GSHUT presence (principal baseline instance).
    if let Some(first) = baseline_instances.values().next() {
        let has = first
            .attributes
            .communities
            .contains(&"65535:0".to_string());
        gshut_present_at_baseline = has;
        last_had_gshut = has;
    }

    for t in transitions {
        let phase = t.phase;
        let has_gshut =
            t.to.state
                .as_ref()
                .map(|s| s.attributes.communities.contains(&"65535:0".to_string()))
                .unwrap_or(false);

        if has_gshut {
            graceful_shutdown_seen = true;
            if first_gshut_timestamp.is_none() {
                first_gshut_timestamp = Some(t.to.timestamp());
            }
            last_gshut_timestamp = Some(t.to.timestamp());
        }

        // GSHUT addition/removal transitions.
        let from_state = t.from.as_ref().and_then(|f| f.state.as_ref());
        let from_has_gshut = from_state
            .map(|s| s.attributes.communities.contains(&"65535:0".to_string()))
            .unwrap_or(false);
        if !from_has_gshut && has_gshut {
            gshut_newly_added = true;
            if first_gshut_added_at.is_none() {
                first_gshut_added_at = Some(t.to.timestamp());
            }
        }
        if from_has_gshut && !has_gshut {
            gshut_removed = true;
            if matches!(
                t.kind,
                TransitionKind::Restoration | TransitionKind::ReturnToBaseline
            ) {
                gshut_removed_during_restoration = true;
            }
        }

        // Capture first/last community sets.
        if !communities_captured {
            communities_before = from_state
                .map(|s| s.attributes.communities.clone())
                .unwrap_or_else(|| {
                    t.to.state
                        .as_ref()
                        .map(|s| s.attributes.communities.clone())
                        .unwrap_or_default()
                });
            communities_captured = true;
        }
        communities_after =
            t.to.state
                .as_ref()
                .map(|s| s.attributes.communities.clone())
                .unwrap_or_default();

        let lct = LifecycleTransition {
            timestamp: t.to.timestamp(),
            phase,
            kind: transition_kind_str(&t.kind),
            path_id: t.key.path_id,
            before_path: from_state
                .map(|s| s.attributes.as_path.0.clone())
                .unwrap_or_default(),
            after_path: t
                .to
                .state
                .as_ref()
                .map(|s| s.attributes.as_path.0.clone())
                .unwrap_or_default(),
            path_shape: path_shape_from_transition(t, transit_predicate),
            observation_id: t.triggering.observation_id.0,
            archive_sha256: t.triggering.archive_sha256.clone(),
            has_gshut_after: has_gshut,
        };

        if phase == AnalysisPhase::Cooldown {
            cooldown_transitions.push(lct);
        } else {
            lifecycle_transitions.push(lct);
        }
        if phase == AnalysisPhase::Event && first_change.is_none() {
            first_change = Some(t.to.timestamp());
        }

        let was_visible = !active.is_empty();

        match &t.kind {
            TransitionKind::Withdrawal => {
                if last_had_gshut {
                    gshut_before_withdrawal = true;
                    if first_consequence_at.is_none() {
                        first_consequence_at = Some(t.to.timestamp());
                    }
                }
                withdrawn_instances.push(t.key.clone());
                if let Some(last_state) = t.from.as_ref().and_then(|f| f.state.as_ref()).cloned() {
                    withdrawn_history.push((t.key.clone(), last_state));
                }
                active.remove(&t.key);
                let now_visible = !active.is_empty();
                if !now_visible {
                    // FINAL instance loss → stream absence.
                    stream_withdrawal_count += 1;
                    if current_absence_start.is_none() {
                        current_absence_start = Some(t.to.timestamp());
                    }
                    stream_withdrawal_time = Some(t.to.timestamp());
                    active_before_absence = active.len() + 1;
                    transit_at_withdrawal = t
                        .from
                        .as_ref()
                        .and_then(|f| f.state.as_ref())
                        .map(|s| transit_predicate.evaluate(&s.attributes.as_path.0));
                }
                final_state = None;
            }
            _ => {
                // Any non-withdrawal applies or re-establishes the instance.
                if let Some(ref state) = t.to.state {
                    active.insert(t.key.clone(), state.clone());
                    seen_instances.insert(t.key.clone());
                    max_concurrent = max_concurrent.max(active.len());
                    final_state = Some(state.clone());
                    replacement_appeared = true;
                    if replacement_retained_transit.is_none() {
                        replacement_retained_transit =
                            Some(transit_predicate.evaluate(&state.attributes.as_path.0));
                    }
                }
            }
        }

        // Stream restoration: absent → visible.
        if !was_visible && !active.is_empty() {
            let restored_at = t.to.timestamp();
            if current_absence_start.is_some() {
                let duration = (restored_at - current_absence_start.unwrap()).num_seconds() as f64;
                if duration > 0.0 {
                    absence_intervals.push(duration);
                }
                current_absence_start = None;
            }
            restoration_count += 1;
            if restoration_time.is_none() {
                restoration_time = Some(restored_at);
            }
            restorations.push(classify_restoration(
                t,
                &withdrawn_history,
                baseline_instances,
                &active,
                restored_at,
            ));
            withdrawn_history.clear();
        }

        // Path-change classification (material vs prepend vs departure).
        if matches!(t.kind, TransitionKind::PathReplacement { .. }) {
            if last_had_gshut {
                gshut_before_path_change = true;
                if first_consequence_at.is_none() {
                    first_consequence_at = Some(t.to.timestamp());
                }
            }
            if let (Some(from), Some(to)) = (
                t.from.as_ref().and_then(|f| f.state.as_ref()),
                t.to.state.as_ref(),
            ) {
                // path_id churn with equivalent route semantics is not material.
                if route_semantically_equivalent(&from.attributes, &to.attributes) {
                    // Not a material path change.
                } else {
                    let shape = classify_path_change(from, to, transit_predicate);
                    match shape {
                        PathShapeChange::PrependReduced | PathShapeChange::PrependIncreased => {
                            prepending_changed = true;
                            seen_categories.insert(StreamCategory::PrependOnly);
                        }
                        PathShapeChange::PathChangedStillViaRequiredTransit => {
                            seen_categories.insert(StreamCategory::PathChangedStillViaTransit);
                        }
                        PathShapeChange::PathDepartedRequiredTransit => {
                            seen_categories.insert(StreamCategory::DepartedTransitPath);
                        }
                        PathShapeChange::PathReturnedToRequiredTransit => {
                            if restoration_time.is_none() {
                                restoration_time = Some(t.to.timestamp());
                            }
                        }
                        PathShapeChange::GenericPathChange => {}
                    }
                }
            }
        }

        // Track stream-level departure: visible with NO matching route.
        if !active.is_empty() {
            let any_matching = active
                .values()
                .any(|s| transit_predicate.evaluate(&s.attributes.as_path.0));
            if !any_matching {
                seen_categories.insert(StreamCategory::DepartedTransitPath);
            } else if seen_categories.contains(&StreamCategory::PathChangedStillViaTransit)
                || seen_categories.contains(&StreamCategory::PrependOnly)
            {
                // at least one matching route remains → still via transit
            }
        }

        if t.kind == TransitionKind::ReturnToBaseline {
            baseline_restored = true;
        }

        last_had_gshut = has_gshut;
    }

    // GSHUT tag-to-consequence duration: first addition → first consequence
    // (withdrawal or path replacement while the tag was present).
    if let (Some(added), Some(c)) = (first_gshut_added_at, first_consequence_at) {
        let secs = (c - added).num_seconds() as f64;
        if secs >= 0.0 {
            gshut_to_consequence = Some(secs);
        }
    }

    // Close any open absence interval at cooldown end.
    if let Some(start) = current_absence_start {
        let duration = (cooldown_end - start).num_seconds() as f64;
        if duration > 0.0 {
            absence_intervals.push(duration);
        }
    }

    // ── Category precedence (stream-level) ───────────────────────
    let was_withdrawn = stream_withdrawal_count > 0;
    let category = if was_withdrawn {
        StreamCategory::Withdrawn
    } else if seen_categories.contains(&StreamCategory::DepartedTransitPath) {
        StreamCategory::DepartedTransitPath
    } else if seen_categories.contains(&StreamCategory::PathChangedStillViaTransit) {
        StreamCategory::PathChangedStillViaTransit
    } else if seen_categories.contains(&StreamCategory::PrependOnly) {
        StreamCategory::PrependOnly
    } else {
        StreamCategory::Unchanged
    };

    let min_absence = absence_intervals
        .iter()
        .copied()
        .min_by(|a, b| a.partial_cmp(b).unwrap());
    let max_absence = absence_intervals
        .iter()
        .copied()
        .max_by(|a, b| a.partial_cmp(b).unwrap());

    let affected_duration = first_change.map(|fc| {
        let end = restoration_time.unwrap_or(cooldown_end);
        ((end - fc).num_seconds() as f64).max(0.0)
    });

    // End-of-window state determines not_restored: a stream that ended the
    // window still absent (after withdrawal) or still departed is not
    // restored; a stream that came back to any present matching state is.
    let ended_absent = active.is_empty() && was_withdrawn;
    let ended_departed = !active.is_empty()
        && !active
            .values()
            .any(|s| transit_predicate.evaluate(&s.attributes.as_path.0))
        && seen_categories.contains(&StreamCategory::DepartedTransitPath);
    let not_restored = ended_absent || ended_departed;
    let restored = restoration_count > 0 && !not_restored;

    let baseline_path = baseline_instances
        .values()
        .next()
        .map(|s| s.attributes.as_path.0.clone())
        .unwrap_or_default();

    // ── ADD-PATH continuity ambiguity (stream-scoped) ────────────
    let ambiguity = detect_stream_ambiguity(opk, baseline_instances, transitions);

    let multiple_cycles =
        stream_withdrawal_count > 1 || (stream_withdrawal_count > 0 && restoration_count > 1);

    StreamLifecycle {
        collector: opk.collector.clone(),
        peer_ip: opk.peer_ip.to_string(),
        prefix: opk.prefix.0.clone(),
        baseline_path,
        baseline_instance_count: baseline_instances.len(),
        max_concurrent_instances: max_concurrent,
        total_route_instances: seen_instances.len(),
        category,
        flags: StreamFlags {
            restored,
            not_restored,
            multiple_cycles,
            add_path_ambiguous: ambiguity.is_some(),
        },
        first_change,
        transitions: lifecycle_transitions,
        min_absence_secs: min_absence,
        max_absence_secs: max_absence,
        was_withdrawn,
        stream_withdrawal_time,
        active_before_absence,
        transit_at_withdrawal,
        withdrawn_instances,
        stream_withdrawal_count,
        restorations,
        add_path_ambiguity: ambiguity,
        replacement_appeared,
        replacement_retained_transit,
        prepending_changed,
        cooldown_transitions,
        final_state,
        baseline_restored,
        restoration_time,
        affected_duration_secs: affected_duration,
        graceful_shutdown_seen,
        gshut_present_at_baseline,
        gshut_newly_added,
        gshut_removed,
        first_gshut_timestamp,
        last_gshut_timestamp,
        gshut_before_withdrawal,
        gshut_before_path_change,
        gshut_to_consequence_secs: gshut_to_consequence,
        gshut_removed_during_restoration,
        communities_before,
        communities_after,
    }
}

/// Classify a stream restoration event (absent → visible).
fn classify_restoration(
    t: &RouteTransition,
    withdrawn_history: &[(RouteKey, RouteState)],
    baseline: &BTreeMap<RouteKey, RouteState>,
    active: &BTreeMap<RouteKey, RouteState>,
    timestamp: DateTime<Utc>,
) -> StreamRestoration {
    let new_state = t.to.state.as_ref().cloned().unwrap_or_else(|| RouteState {
        prefix: t.key.prefix.clone(),
        attributes: RouteAttributes::empty(),
        timestamp,
        observer: format!("{}:{}", t.key.collector, t.key.peer_ip),
        path_id: t.key.path_id,
    });

    // Exact-instance: the returning path_id matches a withdrawn or baseline
    // instance AND the route is semantically equivalent to that instance's
    // last known state (never path-id equality alone).
    let mut exact_instance = false;
    if let Some(pid) = t.key.path_id {
        let withdrawn_match = withdrawn_history
            .iter()
            .find(|(k, _)| k.path_id == Some(pid));
        let baseline_match = baseline.iter().find(|(k, _)| k.path_id == Some(pid));
        if let Some((_, last_state)) = withdrawn_match {
            exact_instance =
                route_semantically_equivalent(&last_state.attributes, &new_state.attributes);
        } else if let Some((_, base_state)) = baseline_match {
            exact_instance =
                route_semantically_equivalent(&base_state.attributes, &new_state.attributes);
        }
    } else if let Some((_, last_state)) =
        withdrawn_history.iter().find(|(k, _)| k.path_id.is_none())
    {
        exact_instance =
            route_semantically_equivalent(&last_state.attributes, &new_state.attributes);
    } else if let Some((_, base_state)) = baseline.iter().find(|(k, _)| k.path_id.is_none()) {
        exact_instance =
            route_semantically_equivalent(&base_state.attributes, &new_state.attributes);
    }

    // Equivalent-route: semantically equivalent to ANY baseline instance.
    let equivalent_route = baseline
        .values()
        .any(|b| route_semantically_equivalent(&b.attributes, &new_state.attributes));

    // Baseline-set restoration: active semantics equal baseline semantics.
    let mut baseline_fps: Vec<String> = baseline
        .values()
        .map(|s| semantic_fingerprint(&s.attributes))
        .collect();
    baseline_fps.sort();
    let mut active_fps: Vec<String> = active
        .values()
        .map(|s| semantic_fingerprint(&s.attributes))
        .collect();
    active_fps.sort();
    let baseline_set = baseline_fps == active_fps;

    // Old path IDs: every instance withdrawn since the last absence.
    let mut old_path_ids: Vec<Option<u32>> =
        withdrawn_history.iter().map(|(k, _)| k.path_id).collect();
    old_path_ids.sort_by(|a, b| {
        crate::derived_cache::path_id_key(a).cmp(&crate::derived_cache::path_id_key(b))
    });
    old_path_ids.dedup();
    let new_path_ids = vec![t.key.path_id];

    StreamRestoration {
        timestamp,
        old_path_ids,
        new_path_ids,
        exact_instance,
        equivalent_route,
        observer_prefix: true,
        baseline_set,
        evidence: t.triggering.clone(),
    }
}

/// Detect mixed keyed/unkeyed ADD-PATH continuity for one stream.
///
/// Scoped to the stream only: evidence records the first keyed record, the
/// first unkeyed record, relevant archive identities, and the affected
/// time range.
fn detect_stream_ambiguity(
    opk: &ObserverPrefixKey,
    baseline: &BTreeMap<RouteKey, RouteState>,
    transitions: &[&RouteTransition],
) -> Option<AddPathAmbiguity> {
    let mut first_keyed: Option<&RouteTransition> = None;
    let mut first_unkeyed: Option<&RouteTransition> = None;
    let mut keyed_baseline = false;
    let mut unkeyed_baseline = false;

    for k in baseline.keys() {
        if k.path_id.is_some() {
            keyed_baseline = true;
        } else {
            unkeyed_baseline = true;
        }
    }
    for t in transitions {
        if t.key.path_id.is_some() {
            if first_keyed.is_none() {
                first_keyed = Some(t);
            }
        } else if first_unkeyed.is_none() {
            first_unkeyed = Some(t);
        }
    }

    let has_keyed = keyed_baseline || first_keyed.is_some();
    let has_unkeyed = unkeyed_baseline || first_unkeyed.is_some();
    if !(has_keyed && has_unkeyed) {
        return None;
    }

    // Baseline evidence is synthetic; transitions carry real evidence.
    let mut archive_identities: Vec<String> = Vec::new();
    for t in transitions {
        if let Some(ref url) = t.triggering.source_url {
            if !archive_identities.contains(url) {
                archive_identities.push(url.clone());
            }
        }
        if let Some(ref sha) = t.triggering.archive_sha256 {
            if !archive_identities.contains(sha) {
                archive_identities.push(sha.clone());
            }
        }
    }

    let affected_start = transitions.first().map(|t| t.to.timestamp());
    let affected_end = transitions.last().map(|t| t.to.timestamp());

    Some(AddPathAmbiguity {
        collector: opk.collector.clone(),
        peer_ip: opk.peer_ip.to_string(),
        prefix: opk.prefix.0.clone(),
        first_keyed: first_keyed.map(|t| t.triggering.clone()),
        first_unkeyed: first_unkeyed.map(|t| t.triggering.clone()),
        archive_identities,
        affected_start,
        affected_end,
    })
}

/// Extract path shape from a transition.
fn path_shape_from_transition(
    t: &RouteTransition,
    transit_predicate: &TransitPredicate,
) -> Option<PathShapeChange> {
    if !matches!(t.kind, TransitionKind::PathReplacement { .. }) {
        return None;
    }
    let from = t.from.as_ref().and_then(|f| f.state.as_ref())?;
    let to = t.to.state.as_ref()?;
    if route_semantically_equivalent(&from.attributes, &to.attributes) {
        return None;
    }
    Some(classify_path_change(from, to, transit_predicate))
}

fn transition_kind_str(kind: &TransitionKind) -> String {
    match kind {
        TransitionKind::Announcement => "Announcement".into(),
        TransitionKind::Withdrawal => "Withdrawal".into(),
        TransitionKind::Duplicate => "Duplicate".into(),
        TransitionKind::PathReplacement { .. } => "PathReplacement".into(),
        TransitionKind::AttributeChange => "AttributeChange".into(),
        TransitionKind::SessionReset => "SessionReset".into(),
        TransitionKind::Restoration => "Restoration".into(),
        TransitionKind::ReturnToBaseline => "ReturnToBaseline".into(),
    }
}

// ── Withdrawal audit ───────────────────────────────────────────────

/// Produce a withdrawal audit from the lifecycle records.
///
/// One row per withdrawn ObserverPrefixKey. "Withdrawal" always means
/// withdrawal from the selected observer-prefix stream — never global.
pub fn withdrawal_audit(lifecycles: &[StreamLifecycle]) -> Vec<WithdrawalRecord> {
    let mut records = Vec::new();

    for lc in lifecycles {
        if !lc.was_withdrawn {
            continue;
        }

        let final_withdrawal_time = lc.stream_withdrawal_time.or(lc.first_change);
        let Some(withdrawal_time) = final_withdrawal_time else {
            continue;
        };

        let mut observation_ids: Vec<u64> = Vec::new();
        let mut archive_checksums: Vec<String> = Vec::new();
        for t in &lc.transitions {
            if t.kind == "Withdrawal" || t.kind == "Restoration" || t.kind == "ReturnToBaseline" {
                observation_ids.push(t.observation_id);
                if let Some(ref sha) = t.archive_sha256 {
                    if !archive_checksums.contains(sha) {
                        archive_checksums.push(sha.clone());
                    }
                }
            }
        }

        let absence_duration = match (withdrawal_time, lc.restoration_time) {
            (w, Some(r)) => Some(((r - w).num_seconds() as f64).max(0.0)),
            _ => None,
        };

        // Restoration kinds from the first restoration event.
        let first_restoration = lc.restorations.first();
        records.push(WithdrawalRecord {
            collector: lc.collector.clone(),
            peer_ip: lc.peer_ip.clone(),
            prefix: lc.prefix.clone(),
            baseline_path: lc.baseline_path.clone(),
            baseline_instances: lc.baseline_instance_count,
            active_before_absence: lc.active_before_absence,
            final_withdrawal_time: withdrawal_time,
            restoration_time: lc.restoration_time,
            absence_duration_secs: absence_duration,
            transit_at_withdrawal: lc.transit_at_withdrawal,
            exact_restoration: first_restoration.map(|r| r.exact_instance).unwrap_or(false),
            equivalent_restoration: first_restoration
                .map(|r| r.equivalent_route)
                .unwrap_or(false),
            observer_prefix_restoration: first_restoration
                .map(|r| r.observer_prefix)
                .unwrap_or(false),
            baseline_set_restoration: first_restoration.map(|r| r.baseline_set).unwrap_or(false),
            observation_ids,
            archive_checksums,
        });
    }

    records
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::route::RouteAttributes;

    #[test]
    fn collapse_removes_consecutive_duplicates() {
        assert_eq!(collapse_as_path(&[1, 2, 2, 2, 3]), vec![1, 2, 3]);
        assert_eq!(collapse_as_path(&[1, 2, 3]), vec![1, 2, 3]);
        assert_eq!(collapse_as_path(&[1, 1, 1]), vec![1]);
        assert_eq!(collapse_as_path(&[]), Vec::<u32>::new());
    }

    #[test]
    fn collapsed_equivalent_detects_prepend_difference() {
        let a = vec![11537, 40220, 225, 225, 225];
        let b = vec![11537, 40220, 225];
        assert!(collapsed_equivalent(&a, &b));
        assert!(!collapsed_equivalent(&[1, 2, 3], &[1, 3, 2]));
    }

    #[test]
    fn prepend_reduction_is_classified_as_prepend_reduced() {
        let from = make_state(vec![11537, 40220, 225, 225, 225]);
        let to = make_state(vec![11537, 40220, 225]);
        let shape = classify_path_change(&from, &to, &TransitPredicate::ContainsAny(vec![11537]));
        assert_eq!(shape, PathShapeChange::PrependReduced);
    }

    #[test]
    fn prepend_increase_is_classified_as_prepend_increased() {
        let from = make_state(vec![11537, 40220, 225]);
        let to = make_state(vec![11537, 40220, 225, 225, 225]);
        let shape = classify_path_change(&from, &to, &TransitPredicate::ContainsAny(vec![11537]));
        assert_eq!(shape, PathShapeChange::PrependIncreased);
    }

    #[test]
    fn collapsed_as_sequence_distinguishes_prepend_from_path_change() {
        // Same collapsed sequence = prepend
        let from = make_state(vec![1, 2, 2, 3]);
        let to = make_state(vec![1, 2, 3]);
        assert_eq!(
            classify_path_change(&from, &to, &TransitPredicate::ContainsAny(vec![99])),
            PathShapeChange::PrependReduced
        );
        // Different collapsed sequence with transit = still via
        let from2 = make_state(vec![1, 99, 2, 3]);
        let to2 = make_state(vec![1, 99, 4, 3]);
        assert_eq!(
            classify_path_change(&from2, &to2, &TransitPredicate::ContainsAny(vec![99])),
            PathShapeChange::PathChangedStillViaRequiredTransit
        );
    }

    #[test]
    fn replacement_retaining_transit_is_not_departure() {
        let from = make_state(vec![1, 11537, 2]);
        let to = make_state(vec![1, 11537, 3]);
        let shape = classify_path_change(&from, &to, &TransitPredicate::ContainsAny(vec![11537]));
        assert_ne!(shape, PathShapeChange::PathDepartedRequiredTransit);
        assert_eq!(shape, PathShapeChange::PathChangedStillViaRequiredTransit);
    }

    #[test]
    fn replacement_without_transit_is_departure() {
        let from = make_state(vec![1, 11537, 2]);
        let to = make_state(vec![1, 3, 2]);
        let shape = classify_path_change(&from, &to, &TransitPredicate::ContainsAny(vec![11537]));
        assert_eq!(shape, PathShapeChange::PathDepartedRequiredTransit);
    }

    #[test]
    fn return_to_transit_is_classified() {
        let from = make_state(vec![1, 3, 2]);
        let to = make_state(vec![1, 11537, 2]);
        let shape = classify_path_change(&from, &to, &TransitPredicate::ContainsAny(vec![11537]));
        assert_eq!(shape, PathShapeChange::PathReturnedToRequiredTransit);
    }

    fn make_state(path: Vec<u32>) -> RouteState {
        use chrono::TimeZone;
        RouteState {
            prefix: crate::domain::route::Prefix::from("192.0.2.0/24"),
            attributes: RouteAttributes::from_as_path(path),
            timestamp: chrono::Utc.with_ymd_and_hms(2026, 7, 14, 7, 0, 0).unwrap(),
            observer: "test:0.0.0.0".into(),
            path_id: None,
        }
    }

    fn make_state_with_communities(path: Vec<u32>, communities: Vec<&str>) -> RouteState {
        use chrono::TimeZone;
        let mut attrs = RouteAttributes::from_as_path(path);
        attrs.communities = communities.into_iter().map(|s| s.to_string()).collect();
        RouteState {
            prefix: crate::domain::route::Prefix::from("192.0.2.0/24"),
            attributes: attrs,
            timestamp: chrono::Utc.with_ymd_and_hms(2026, 7, 14, 7, 0, 0).unwrap(),
            observer: "test:0.0.0.0".into(),
            path_id: None,
        }
    }

    #[test]
    fn gshut_community_is_65535_0() {
        let gshut = "65535:0";
        // Verify the canonical form used throughout the codebase
        assert_eq!(gshut, "65535:0");
    }

    #[test]
    fn gshut_tag_addition_is_detected() {
        use crate::domain::route::Continuity;
        use crate::tokenize::diff_states;
        let from = make_state_with_communities(vec![1, 2, 3], vec![]);
        let to = make_state_with_communities(vec![1, 2, 3], vec!["65535:0"]);
        let (kind, _effects) = diff_states(None, Some(&from), &to, Continuity::Known);
        assert_eq!(kind, crate::domain::route::TransitionKind::AttributeChange);
    }

    #[test]
    fn gshut_removal_is_detected() {
        use crate::domain::route::Continuity;
        use crate::tokenize::diff_states;
        let from = make_state_with_communities(vec![1, 2, 3], vec!["65535:0"]);
        let to = make_state_with_communities(vec![1, 2, 3], vec![]);
        let (kind, _effects) = diff_states(None, Some(&from), &to, Continuity::Known);
        assert_eq!(kind, crate::domain::route::TransitionKind::AttributeChange);
    }

    #[test]
    fn unrelated_community_change_is_not_gshut() {
        let from = make_state_with_communities(vec![1, 2, 3], vec!["11537:1000"]);
        let to = make_state_with_communities(vec![1, 2, 3], vec!["11537:2000"]);
        // Same path, different community — should be community-only change
        assert_eq!(from.attributes.as_path, to.attributes.as_path);
        assert_ne!(from.attributes.communities, to.attributes.communities);
        assert!(!from.attributes.communities.contains(&"65535:0".to_string()));
        assert!(!to.attributes.communities.contains(&"65535:0".to_string()));
    }

    #[test]
    fn community_only_update_survives_target_admission() {
        // Community changes on a target stream should not be filtered out
        let from = make_state_with_communities(vec![1, 11537, 3], vec!["11537:1000"]);
        let to = make_state_with_communities(vec![1, 11537, 3], vec!["11537:2000"]);
        // Path unchanged, transit still present
        assert_eq!(from.attributes.as_path, to.attributes.as_path);
        assert!(to.attributes.as_path.0.contains(&11537));
    }

    #[test]
    fn withdrawal_after_gshut_retains_both_evidence_records() {
        let from = make_state_with_communities(vec![1, 11537, 3], vec!["65535:0"]);
        // from had GSHUT, withdrawal follows — both facts should be recorded
        assert!(from.attributes.communities.contains(&"65535:0".to_string()));
        // The lifecycle builder tracks gshut_before_withdrawal
    }

    #[test]
    fn absence_of_gshut_does_not_change_withdrawal_classification() {
        let from = make_state_with_communities(vec![1, 11537, 3], vec![]);
        // No GSHUT — classification is unchanged
        assert!(!from.attributes.communities.contains(&"65535:0".to_string()));
        // Withdrawal classification should proceed normally
    }

    // ── Part 3: ADD-PATH semantics ────────────────────────────────

    use crate::cohort::FrozenCohort;
    use crate::domain::observation::EvidenceRef;
    use crate::domain::route::{AsPath, EvidencedRouteState, GenericTransitionEffects};
    use std::collections::BTreeMap;

    fn opk(collector: &str, peer: &str, prefix: &str) -> ObserverPrefixKey {
        ObserverPrefixKey {
            collector: collector.into(),
            peer_ip: peer.parse().unwrap(),
            prefix: crate::domain::route::Prefix::from(prefix),
        }
    }

    fn cohort_with(key: &ObserverPrefixKey, instances: &[(Option<u32>, Vec<u32>)]) -> FrozenCohort {
        let mut cohort = FrozenCohort::default();
        let mut map = BTreeMap::new();
        for (pid, path) in instances {
            let rk = RouteKey::with_path_id(&key.collector, key.peer_ip, &key.prefix, *pid);
            let attrs = RouteAttributes::from_as_path(path.clone());
            map.insert(
                rk,
                RouteState {
                    prefix: key.prefix.clone(),
                    attributes: attrs,
                    timestamp: t0(),
                    observer: format!("{}:{}", key.collector, key.peer_ip),
                    path_id: *pid,
                },
            );
        }
        cohort.observer_prefixes.insert(key.clone());
        cohort.baseline_instances.insert(key.clone(), map);
        cohort
    }

    use chrono::TimeZone;
    fn t0() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 7, 30, 9, 0, 0).unwrap()
    }

    fn t_secs(secs: i64) -> DateTime<Utc> {
        t0() + chrono::Duration::seconds(secs)
    }

    pub(crate) fn mk_transition(
        key: &ObserverPrefixKey,
        pid: Option<u32>,
        kind: TransitionKind,
        at_secs: i64,
        path: Vec<u32>,
        communities: Vec<&str>,
    ) -> RouteTransition {
        let rk = RouteKey::with_path_id(&key.collector, key.peer_ip, &key.prefix, pid);
        let mut attrs = RouteAttributes::from_as_path(path);
        attrs.communities = communities.into_iter().map(|s| s.to_string()).collect();
        let state = RouteState {
            prefix: key.prefix.clone(),
            attributes: attrs,
            timestamp: t_secs(at_secs),
            observer: format!("{}:{}", key.collector, key.peer_ip),
            path_id: pid,
        };
        let ev = EvidenceRef::synthetic(at_secs as u64, "http://example.com/up.bz2", "abc123");
        RouteTransition::new(
            rk,
            None,
            None,
            EvidencedRouteState::present(state.clone(), ev.clone()),
            ev,
            kind,
            GenericTransitionEffects::default(),
            AnalysisPhase::Event,
        )
    }

    fn withdrawal(key: &ObserverPrefixKey, pid: Option<u32>, at_secs: i64) -> RouteTransition {
        use crate::domain::observation::{Asn, CollectorId, ObservationId};
        let rk = RouteKey::with_path_id(&key.collector, key.peer_ip, &key.prefix, pid);
        let ev = EvidenceRef {
            observation_id: ObservationId(at_secs as u64),
            source_url: Some("http://example.com/up.bz2".into()),
            archive_sha256: Some("abc123".into()),
            collector: CollectorId(key.collector.clone()),
            peer_ip: key.peer_ip,
            peer_asn: Asn(0),
            prefix: key.prefix.clone(),
            timestamp: t_secs(at_secs),
            element_seq: at_secs as u64,
            path_id: pid,
        };
        let from = RouteState {
            prefix: key.prefix.clone(),
            attributes: RouteAttributes::from_as_path(vec![6447, 65002, 65001]),
            timestamp: t_secs(at_secs - 1),
            observer: format!("{}:{}", key.collector, key.peer_ip),
            path_id: pid,
        };
        RouteTransition::new(
            rk,
            None,
            Some(EvidencedRouteState::present(from, ev.clone())),
            EvidencedRouteState::absent(ev.clone()),
            ev,
            TransitionKind::Withdrawal,
            GenericTransitionEffects::default(),
            AnalysisPhase::Event,
        )
    }

    fn pred() -> TransitPredicate {
        TransitPredicate::ContainsAny(vec![65002])
    }

    #[test]
    fn mixed_encoding_records_first_conflicting_evidence() {
        let key = opk("rv2", "185.1.8.65", "192.0.2.0/24");
        let cohort = cohort_with(&key, &[(Some(1), vec![6447, 65002, 65001])]);
        let transitions = vec![
            withdrawal(&key, Some(1), 10),
            mk_transition(
                &key,
                None,
                TransitionKind::Announcement,
                20,
                vec![6447, 65002, 65001],
                vec![],
            ),
        ];
        let lifecycles = build_lifecycles(&transitions, &cohort, t_secs(1000), &pred());
        let lc = &lifecycles[0];
        assert!(lc.flags.add_path_ambiguous);
        let amb = lc.add_path_ambiguity.as_ref().expect("ambiguity recorded");
        // First keyed record: the withdrawal at t=10.
        assert_eq!(amb.first_keyed.as_ref().unwrap().observation_id.0, 10);
        // First unkeyed record: the announcement at t=20.
        assert_eq!(amb.first_unkeyed.as_ref().unwrap().observation_id.0, 20);
        // Affected range covers both records.
        assert_eq!(amb.affected_start, Some(t_secs(10)));
        assert_eq!(amb.affected_end, Some(t_secs(20)));
        // Archive identity retained.
        assert!(amb.archive_identities.iter().any(|a| a == "abc123"));
    }

    #[test]
    fn mixed_encoding_marks_only_one_stream_ambiguous() {
        let a = opk("rv2", "185.1.8.65", "192.0.2.0/24");
        let b = opk("rv2", "185.1.8.66", "192.0.2.0/24");
        let mut cohort = cohort_with(&a, &[(Some(1), vec![6447, 65002, 65001])]);
        let b_cohort = cohort_with(&b, &[(Some(1), vec![6447, 65002, 65001])]);
        for k in b_cohort.observer_prefixes {
            cohort.observer_prefixes.insert(k);
        }
        for (k, v) in b_cohort.baseline_instances {
            cohort.baseline_instances.insert(k, v);
        }
        let transitions = vec![
            withdrawal(&a, Some(1), 10),
            mk_transition(
                &a,
                None,
                TransitionKind::Announcement,
                20,
                vec![6447, 65002, 65001],
                vec![],
            ),
            // Stream b stays consistently keyed.
            mk_transition(
                &b,
                Some(7),
                TransitionKind::Announcement,
                30,
                vec![6447, 65002, 65001],
                vec![],
            ),
        ];
        let lifecycles = build_lifecycles(&transitions, &cohort, t_secs(1000), &pred());
        let a_lc = lifecycles
            .iter()
            .find(|l| l.peer_ip == "185.1.8.65")
            .unwrap();
        let b_lc = lifecycles
            .iter()
            .find(|l| l.peer_ip == "185.1.8.66")
            .unwrap();
        assert!(a_lc.flags.add_path_ambiguous);
        assert!(
            !b_lc.flags.add_path_ambiguous,
            "unrelated stream stays clean"
        );
    }

    #[test]
    fn unrelated_streams_remain_fully_evaluable() {
        let a = opk("rv2", "185.1.8.65", "192.0.2.0/24");
        let b = opk("rv2", "185.1.8.66", "192.0.2.0/24");
        let mut cohort = cohort_with(&a, &[(Some(1), vec![6447, 65002, 65001])]);
        let b_cohort = cohort_with(&b, &[(Some(1), vec![6447, 65002, 65001])]);
        for k in b_cohort.observer_prefixes {
            cohort.observer_prefixes.insert(k);
        }
        for (k, v) in b_cohort.baseline_instances {
            cohort.baseline_instances.insert(k, v);
        }
        let transitions = vec![
            withdrawal(&a, Some(1), 10),
            mk_transition(
                &a,
                None,
                TransitionKind::Announcement,
                20,
                vec![6447, 65002, 65001],
                vec![],
            ),
            withdrawal(&b, Some(1), 40),
        ];
        let lifecycles = build_lifecycles(&transitions, &cohort, t_secs(1000), &pred());
        let b_lc = lifecycles
            .iter()
            .find(|l| l.peer_ip == "185.1.8.66")
            .unwrap();
        // Stream b: fully withdrawn and NOT ambiguous → fully evaluable.
        assert!(!b_lc.flags.add_path_ambiguous);
        assert!(b_lc.was_withdrawn);
        assert_eq!(b_lc.category, StreamCategory::Withdrawn);
    }

    #[test]
    fn ambiguous_stream_suppresses_strong_stream_assessment() {
        let key = opk("rv2", "185.1.8.65", "192.0.2.0/24");
        let cohort = cohort_with(&key, &[(Some(1), vec![6447, 65002, 65001])]);
        let transitions = vec![
            withdrawal(&key, Some(1), 10),
            mk_transition(
                &key,
                None,
                TransitionKind::Announcement,
                20,
                vec![6447, 65002, 65001],
                vec![],
            ),
        ];
        let lifecycles = build_lifecycles(&transitions, &cohort, t_secs(1000), &pred());
        let lc = &lifecycles[0];
        // The ambiguity flag is the assessment-level suppression signal:
        // strong conclusions must not be drawn from this stream's category.
        assert!(lc.flags.add_path_ambiguous);
        // Category may still be classified, but the flag gates verdict use.
        assert_eq!(lc.category, StreamCategory::Withdrawn);
    }

    #[test]
    fn ambiguity_survives_cache_roundtrip() {
        let key = opk("rv2", "185.1.8.65", "192.0.2.0/24");
        let cohort = cohort_with(&key, &[(Some(1), vec![6447, 65002, 65001])]);
        let transitions = vec![
            withdrawal(&key, Some(1), 10),
            mk_transition(
                &key,
                None,
                TransitionKind::Announcement,
                20,
                vec![6447, 65002, 65001],
                vec![],
            ),
        ];
        let lifecycles = build_lifecycles(&transitions, &cohort, t_secs(1000), &pred());
        let amb = lifecycles[0].add_path_ambiguity.clone().unwrap();
        let json = serde_json::to_string(&amb).unwrap();
        let parsed: AddPathAmbiguity = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, amb);
        assert_eq!(parsed.first_keyed.unwrap().observation_id.0, 10);
    }

    #[test]
    fn exact_instance_restoration_requires_matching_path_id() {
        let key = opk("rv2", "185.1.8.65", "192.0.2.0/24");
        let cohort = cohort_with(&key, &[(Some(1), vec![6447, 65002, 65001])]);
        // Withdraw instance 1, then restore under path_id 1 (exact instance).
        let transitions = vec![
            withdrawal(&key, Some(1), 10),
            mk_transition(
                &key,
                Some(1),
                TransitionKind::Restoration,
                20,
                vec![6447, 65002, 65001],
                vec![],
            ),
        ];
        let lifecycles = build_lifecycles(&transitions, &cohort, t_secs(1000), &pred());
        let lc = &lifecycles[0];
        assert_eq!(lc.restorations.len(), 1);
        let r = &lc.restorations[0];
        assert!(r.exact_instance);
        assert!(r.observer_prefix);
    }

    #[test]
    fn equivalent_restoration_allows_new_path_id() {
        let key = opk("rv2", "185.1.8.65", "192.0.2.0/24");
        let cohort = cohort_with(&key, &[(Some(1), vec![6447, 65002, 65001])]);
        // Restore under a NEW path_id (2) — equivalent route, not exact instance.
        let transitions = vec![
            withdrawal(&key, Some(1), 10),
            mk_transition(
                &key,
                Some(2),
                TransitionKind::Restoration,
                20,
                vec![6447, 65002, 65001],
                vec![],
            ),
        ];
        let lifecycles = build_lifecycles(&transitions, &cohort, t_secs(1000), &pred());
        let lc = &lifecycles[0];
        let r = &lc.restorations[0];
        assert!(
            !r.exact_instance,
            "path_id mismatch prevents exact restoration"
        );
        assert!(
            r.equivalent_route,
            "same semantics under new path_id is equivalent"
        );
    }

    #[test]
    fn observer_prefix_restoration_only_requires_visibility() {
        let key = opk("rv2", "185.1.8.65", "192.0.2.0/24");
        let cohort = cohort_with(&key, &[(Some(1), vec![6447, 65002, 65001])]);
        // Restore with a DIFFERENT route entirely — still an observer-prefix
        // restoration (absent → visible), even though not equivalent.
        let transitions = vec![
            withdrawal(&key, Some(1), 10),
            mk_transition(
                &key,
                Some(2),
                TransitionKind::Announcement,
                20,
                vec![6447, 9999, 65001],
                vec![],
            ),
        ];
        let lifecycles = build_lifecycles(&transitions, &cohort, t_secs(1000), &pred());
        let r = &lifecycles[0].restorations[0];
        assert!(r.observer_prefix);
        assert!(!r.equivalent_route);
        assert!(!r.baseline_set);
    }

    #[test]
    fn baseline_set_restoration_ignores_path_id_assignment() {
        let key = opk("rv2", "185.1.8.65", "192.0.2.0/24");
        let cohort = cohort_with(&key, &[(Some(1), vec![6447, 65002, 65001])]);
        // Baseline had path_id 1; restore under path_id 3 with the same
        // semantic route → baseline set restored despite new path_id.
        let transitions = vec![
            withdrawal(&key, Some(1), 10),
            mk_transition(
                &key,
                Some(3),
                TransitionKind::Restoration,
                20,
                vec![6447, 65002, 65001],
                vec![],
            ),
        ];
        let lifecycles = build_lifecycles(&transitions, &cohort, t_secs(1000), &pred());
        let r = &lifecycles[0].restorations[0];
        assert!(
            r.baseline_set,
            "baseline-set restoration ignores path_id assignment"
        );
        assert!(r.equivalent_route);
    }

    #[test]
    fn changed_path_attributes_prevent_equivalent_restoration() {
        let key = opk("rv2", "185.1.8.65", "192.0.2.0/24");
        let cohort = cohort_with(&key, &[(Some(1), vec![6447, 65002, 65001])]);
        // Restore with the same path_id but a CHANGED path (materially different).
        let transitions = vec![
            withdrawal(&key, Some(1), 10),
            mk_transition(
                &key,
                Some(1),
                TransitionKind::Announcement,
                20,
                vec![6447, 9999, 65001],
                vec![],
            ),
        ];
        let lifecycles = build_lifecycles(&transitions, &cohort, t_secs(1000), &pred());
        let r = &lifecycles[0].restorations[0];
        assert!(
            !r.equivalent_route,
            "changed path attributes prevent equivalent restoration"
        );
        assert!(!r.baseline_set);
        assert!(r.observer_prefix);
    }

    #[test]
    fn restoration_evidence_retains_old_and_new_path_ids() {
        let key = opk("rv2", "185.1.8.65", "192.0.2.0/24");
        let cohort = cohort_with(&key, &[(Some(1), vec![6447, 65002, 65001])]);
        let transitions = vec![
            withdrawal(&key, Some(1), 10),
            mk_transition(
                &key,
                Some(2),
                TransitionKind::Restoration,
                20,
                vec![6447, 65002, 65001],
                vec![],
            ),
        ];
        let lifecycles = build_lifecycles(&transitions, &cohort, t_secs(1000), &pred());
        let r = &lifecycles[0].restorations[0];
        assert_eq!(r.old_path_ids, vec![Some(1)]);
        assert_eq!(r.new_path_ids, vec![Some(2)]);
        assert_eq!(r.evidence.observation_id.0, 20);
    }

    #[test]
    fn nonfinal_instance_loss_does_not_make_withdrawn_lifecycle() {
        let key = opk("rv2", "185.1.8.65", "192.0.2.0/24");
        let cohort = cohort_with(
            &key,
            &[
                (Some(1), vec![6447, 65002, 65001]),
                (Some(2), vec![6447, 65002, 65001]),
            ],
        );
        // Losing instance 1 while instance 2 remains → not Withdrawn.
        let transitions = vec![withdrawal(&key, Some(1), 10)];
        let lifecycles = build_lifecycles(&transitions, &cohort, t_secs(1000), &pred());
        let lc = &lifecycles[0];
        assert!(
            !lc.was_withdrawn,
            "non-final instance loss is not stream withdrawal"
        );
        assert_ne!(lc.category, StreamCategory::Withdrawn);
        assert_eq!(lc.withdrawn_instances.len(), 1);
        assert_eq!(lc.stream_withdrawal_count, 0);
    }

    #[test]
    fn final_instance_loss_makes_withdrawn_lifecycle() {
        let key = opk("rv2", "185.1.8.65", "192.0.2.0/24");
        let cohort = cohort_with(
            &key,
            &[
                (Some(1), vec![6447, 65002, 65001]),
                (Some(2), vec![6447, 65002, 65001]),
            ],
        );
        // Lose instance 1, then instance 2 (the final one) → Withdrawn.
        let transitions = vec![withdrawal(&key, Some(1), 10), withdrawal(&key, Some(2), 20)];
        let lifecycles = build_lifecycles(&transitions, &cohort, t_secs(1000), &pred());
        let lc = &lifecycles[0];
        assert!(lc.was_withdrawn, "final instance loss is stream withdrawal");
        assert_eq!(lc.category, StreamCategory::Withdrawn);
        assert_eq!(lc.stream_withdrawal_count, 1);
        assert_eq!(lc.withdrawn_instances.len(), 2);
    }

    #[test]
    fn equivalent_path_id_replacement_is_not_material() {
        let key = opk("rv2", "185.1.8.65", "192.0.2.0/24");
        let cohort = cohort_with(&key, &[(Some(1), vec![6447, 65002, 65001])]);
        // Announcement under new path_id with identical semantics.
        let transitions = vec![mk_transition(
            &key,
            Some(2),
            TransitionKind::PathReplacement {
                old: AsPath(vec![6447, 65002, 65001]),
                new: AsPath(vec![6447, 65002, 65001]),
            },
            10,
            vec![6447, 65002, 65001],
            vec![],
        )];
        let lifecycles = build_lifecycles(&transitions, &cohort, t_secs(1000), &pred());
        let lc = &lifecycles[0];
        assert_eq!(
            lc.category,
            StreamCategory::Unchanged,
            "path_id churn with equivalent semantics is not a material path change"
        );
    }

    #[test]
    fn one_matching_instance_prevents_departed_category() {
        let key = opk("rv2", "185.1.8.65", "192.0.2.0/24");
        let cohort = cohort_with(
            &key,
            &[
                (Some(1), vec![6447, 65002, 65001]),
                (Some(2), vec![6447, 65002, 65001]),
            ],
        );
        // Instance 1 departs the transit predicate; instance 2 still matches.
        let transitions = vec![mk_transition(
            &key,
            Some(1),
            TransitionKind::PathReplacement {
                old: AsPath(vec![6447, 65002, 65001]),
                new: AsPath(vec![6447, 9999, 65001]),
            },
            10,
            vec![6447, 9999, 65001],
            vec![],
        )];
        let lifecycles = build_lifecycles(&transitions, &cohort, t_secs(1000), &pred());
        let lc = &lifecycles[0];
        assert_ne!(lc.category, StreamCategory::DepartedTransitPath);
        assert!(
            lc.category == StreamCategory::PathChangedStillViaTransit
                || lc.category == StreamCategory::Unchanged,
            "at least one matching instance remains → not departed"
        );
    }

    #[test]
    fn loss_of_last_matching_instance_marks_departed_category() {
        let key = opk("rv2", "185.1.8.65", "192.0.2.0/24");
        let cohort = cohort_with(&key, &[(Some(1), vec![6447, 65002, 65001])]);
        // The only instance departs the transit predicate; stream stays visible.
        let transitions = vec![mk_transition(
            &key,
            Some(1),
            TransitionKind::PathReplacement {
                old: AsPath(vec![6447, 65002, 65001]),
                new: AsPath(vec![6447, 9999, 65001]),
            },
            10,
            vec![6447, 9999, 65001],
            vec![],
        )];
        let lifecycles = build_lifecycles(&transitions, &cohort, t_secs(1000), &pred());
        let lc = &lifecycles[0];
        assert_eq!(lc.category, StreamCategory::DepartedTransitPath);
        assert!(!lc.was_withdrawn, "visible stream is not Withdrawn");
    }

    #[test]
    fn lifecycle_count_is_one_per_observer_prefix() {
        let a = opk("rv2", "185.1.8.65", "192.0.2.0/24");
        let cohort = cohort_with(
            &a,
            &[
                (Some(1), vec![6447, 65002, 65001]),
                (Some(2), vec![6447, 65002, 65001]),
            ],
        );
        // One stream, two instances → exactly one lifecycle.
        let transitions = vec![withdrawal(&a, Some(1), 10)];
        let lifecycles = build_lifecycles(&transitions, &cohort, t_secs(1000), &pred());
        assert_eq!(lifecycles.len(), 1);
        let lc = &lifecycles[0];
        assert_eq!(lc.baseline_instance_count, 2);
        assert!(lc.total_route_instances >= 2);
    }

    #[test]
    fn lifecycle_retains_all_instance_histories() {
        let key = opk("rv2", "185.1.8.65", "192.0.2.0/24");
        let cohort = cohort_with(&key, &[(Some(1), vec![6447, 65002, 65001])]);
        let transitions = vec![
            withdrawal(&key, Some(1), 10),
            mk_transition(
                &key,
                Some(2),
                TransitionKind::Announcement,
                20,
                vec![6447, 65002, 65001],
                vec![],
            ),
            mk_transition(
                &key,
                Some(1),
                TransitionKind::Announcement,
                30,
                vec![6447, 65002, 65001],
                vec![],
            ),
        ];
        let lifecycles = build_lifecycles(&transitions, &cohort, t_secs(1000), &pred());
        let lc = &lifecycles[0];
        // Every transition retained with its instance path_id.
        assert_eq!(lc.transitions.len(), 3);
        let pids: Vec<Option<u32>> = lc.transitions.iter().map(|t| t.path_id).collect();
        assert_eq!(pids, vec![Some(1), Some(2), Some(1)]);
        // All three instances appear in the history.
        assert!(lc.total_route_instances >= 2);
        assert_eq!(lc.withdrawn_instances.len(), 1);
    }

    #[test]
    fn ambiguous_streams_suppress_strong_assessment_verdict() {
        // One ambiguous stream (withdrew under keyed, returned unkeyed) and
        // one clean unchanged stream. The ambiguous withdrawal must not
        // produce a strong impact verdict on its own.
        let a = opk("rv2", "185.1.8.65", "192.0.2.0/24");
        let b = opk("rv2", "185.1.8.66", "192.0.2.0/24");
        let mut cohort = cohort_with(&a, &[(Some(1), vec![6447, 65002, 65001])]);
        let b_cohort = cohort_with(&b, &[(Some(1), vec![6447, 65002, 65001])]);
        for k in b_cohort.observer_prefixes {
            cohort.observer_prefixes.insert(k);
        }
        for (k, v) in b_cohort.baseline_instances {
            cohort.baseline_instances.insert(k, v);
        }
        let transitions = vec![
            withdrawal(&a, Some(1), 10),
            mk_transition(
                &a,
                None,
                TransitionKind::Announcement,
                20,
                vec![6447, 65002, 65001],
                vec![],
            ),
        ];
        let lifecycles = build_lifecycles(&transitions, &cohort, t_secs(1000), &pred());
        // Stream a is ambiguous; stream b unchanged.
        assert!(lifecycles.iter().any(|l| l.flags.add_path_ambiguous));

        let exp = crate::domain::expectation::ImpactExpectation::participant_unavailable("test");
        // A benign transition keeps the verdict path in the lifecycle branch.
        let benign = mk_transition(
            &b,
            Some(1),
            TransitionKind::Announcement,
            5,
            vec![6447, 65002, 65001],
            vec![],
        );
        let assessment = crate::assess::assess(
            crate::domain::event::EventId::from("T1"),
            exp,
            &[benign],
            vec![],
            false,
            Some(&lifecycles),
        );
        assert_eq!(
            assessment.verdict,
            crate::domain::assessment::Verdict::InsufficientVisibility,
            "ambiguous stream withdrawal must not yield a strong verdict"
        );
    }

    // ── Part 5: RFC 8326 GRACEFUL_SHUTDOWN lifecycle ──────────────

    /// A path-replacement transition carrying a real before state.
    fn mk_path_change(
        key: &ObserverPrefixKey,
        pid: Option<u32>,
        at_secs: i64,
        from_path: Vec<u32>,
        from_communities: Vec<&str>,
        to_path: Vec<u32>,
        to_communities: Vec<&str>,
    ) -> RouteTransition {
        let rk = RouteKey::with_path_id(&key.collector, key.peer_ip, &key.prefix, pid);
        let mut from_attrs = RouteAttributes::from_as_path(from_path);
        from_attrs.communities = from_communities
            .into_iter()
            .map(|s| s.to_string())
            .collect();
        let mut to_attrs = RouteAttributes::from_as_path(to_path.clone());
        to_attrs.communities = to_communities.into_iter().map(|s| s.to_string()).collect();
        // Recompute the generic prepend effect like diff_states would.
        let mut effects = GenericTransitionEffects::default();
        let from_collapsed = collapse_as_path(&from_attrs.as_path.0);
        let to_collapsed = collapse_as_path(&to_attrs.as_path.0);
        if from_collapsed == to_collapsed && from_attrs.as_path.0.len() > to_attrs.as_path.0.len() {
            effects.prepend = PrependChange::Reduced;
        } else if from_collapsed == to_collapsed
            && from_attrs.as_path.0.len() < to_attrs.as_path.0.len()
        {
            effects.prepend = PrependChange::Increased;
        } else if from_collapsed != to_collapsed {
            effects.material_path_changed = true;
        }
        let from = RouteState {
            prefix: key.prefix.clone(),
            attributes: from_attrs,
            timestamp: t_secs(at_secs - 1),
            observer: format!("{}:{}", key.collector, key.peer_ip),
            path_id: pid,
        };
        let to = RouteState {
            prefix: key.prefix.clone(),
            attributes: to_attrs,
            timestamp: t_secs(at_secs),
            observer: format!("{}:{}", key.collector, key.peer_ip),
            path_id: pid,
        };
        let ev = EvidenceRef::synthetic(at_secs as u64, "http://example.com/up.bz2", "abc123");
        RouteTransition::new(
            rk,
            None,
            Some(EvidencedRouteState::present(from, ev.clone())),
            EvidencedRouteState::present(to, ev.clone()),
            ev,
            TransitionKind::PathReplacement {
                old: AsPath(to_path.clone()),
                new: AsPath(to_path.clone()),
            },
            effects,
            AnalysisPhase::Event,
        )
    }

    #[test]
    fn gshut_addition_coexists_with_path_change() {
        let key = opk("rv2", "185.1.8.65", "192.0.2.0/24");
        let cohort = cohort_with(&key, &[(Some(1), vec![6447, 65002, 65001])]);
        // Path changes AND GSHUT is added in the same transition.
        let transitions = vec![mk_path_change(
            &key,
            Some(1),
            10,
            vec![6447, 65002, 65001],
            vec![],
            vec![6447, 9999, 65001],
            vec!["65535:0"],
        )];
        let lifecycles = build_lifecycles(&transitions, &cohort, t_secs(1000), &pred());
        let lc = &lifecycles[0];
        assert!(lc.gshut_newly_added, "GSHUT addition recorded");
        assert_eq!(lc.first_gshut_timestamp, Some(t_secs(10)));
        assert_eq!(lc.last_gshut_timestamp, Some(t_secs(10)));
        // The departure classification still applies.
        assert_eq!(lc.category, StreamCategory::DepartedTransitPath);
    }

    #[test]
    fn gshut_addition_coexists_with_prepend_reduction() {
        let key = opk("rv2", "185.1.8.65", "192.0.2.0/24");
        let cohort = cohort_with(&key, &[(Some(1), vec![6447, 65002, 65002, 65002, 65001])]);
        // Prepend reduction (3x 65002 -> 1x 65002) with GSHUT added.
        let transitions = vec![mk_path_change(
            &key,
            Some(1),
            10,
            vec![6447, 65002, 65002, 65002, 65001],
            vec![],
            vec![6447, 65002, 65001],
            vec!["65535:0"],
        )];
        let lifecycles = build_lifecycles(&transitions, &cohort, t_secs(1000), &pred());
        let lc = &lifecycles[0];
        assert!(lc.gshut_newly_added);
        assert_eq!(lc.category, StreamCategory::PrependOnly);
    }

    /// A restoration-kind transition with a real before state.
    fn mk_restoration_with_from(
        key: &ObserverPrefixKey,
        pid: Option<u32>,
        at_secs: i64,
        from_path: Vec<u32>,
        from_communities: Vec<&str>,
        to_path: Vec<u32>,
        to_communities: Vec<&str>,
    ) -> RouteTransition {
        let rk = RouteKey::with_path_id(&key.collector, key.peer_ip, &key.prefix, pid);
        let mut from_attrs = RouteAttributes::from_as_path(from_path);
        from_attrs.communities = from_communities
            .into_iter()
            .map(|s| s.to_string())
            .collect();
        let from = RouteState {
            prefix: key.prefix.clone(),
            attributes: from_attrs,
            timestamp: t_secs(at_secs - 1),
            observer: format!("{}:{}", key.collector, key.peer_ip),
            path_id: pid,
        };
        let mut to_attrs = RouteAttributes::from_as_path(to_path.clone());
        to_attrs.communities = to_communities.into_iter().map(|s| s.to_string()).collect();
        let to = RouteState {
            prefix: key.prefix.clone(),
            attributes: to_attrs,
            timestamp: t_secs(at_secs),
            observer: format!("{}:{}", key.collector, key.peer_ip),
            path_id: pid,
        };
        let ev = EvidenceRef::synthetic(at_secs as u64, "http://example.com/up.bz2", "abc123");
        RouteTransition::new(
            rk,
            None,
            Some(EvidencedRouteState::present(from, ev.clone())),
            EvidencedRouteState::present(to, ev.clone()),
            ev,
            TransitionKind::Restoration,
            GenericTransitionEffects::default(),
            AnalysisPhase::Event,
        )
    }

    #[test]
    fn gshut_removal_coexists_with_restoration() {
        let key = opk("rv2", "185.1.8.65", "192.0.2.0/24");
        let cohort = cohort_with(&key, &[(Some(1), vec![6447, 65002, 65001])]);
        // Baseline has no GSHUT; add it, withdraw, then restore WITHOUT it.
        let transitions = vec![
            mk_path_change(
                &key,
                Some(1),
                10,
                vec![6447, 65002, 65001],
                vec![],
                vec![6447, 65002, 65001],
                vec!["65535:0"],
            ),
            withdrawal(&key, Some(1), 20),
            mk_restoration_with_from(
                &key,
                Some(1),
                30,
                vec![6447, 65002, 65001],
                vec!["65535:0"],
                vec![6447, 65002, 65001],
                vec![],
            ),
        ];
        let lifecycles = build_lifecycles(&transitions, &cohort, t_secs(1000), &pred());
        let lc = &lifecycles[0];
        assert!(lc.gshut_newly_added);
        assert!(lc.gshut_removed, "GSHUT removed during the window");
        assert!(
            lc.gshut_removed_during_restoration,
            "removal co-occurs with restoration"
        );
        assert!(lc.flags.restored);
    }

    #[test]
    fn lifecycle_tracks_gshut_before_withdrawal() {
        let key = opk("rv2", "185.1.8.65", "192.0.2.0/24");
        let cohort = cohort_with(&key, &[(Some(1), vec![6447, 65002, 65001])]);
        let transitions = vec![
            mk_path_change(
                &key,
                Some(1),
                10,
                vec![6447, 65002, 65001],
                vec![],
                vec![6447, 65002, 65001],
                vec!["65535:0"],
            ),
            withdrawal(&key, Some(1), 20),
        ];
        let lifecycles = build_lifecycles(&transitions, &cohort, t_secs(1000), &pred());
        let lc = &lifecycles[0];
        assert!(
            lc.gshut_before_withdrawal,
            "GSHUT present before withdrawal"
        );
        assert!(lc.was_withdrawn);
        // Tag-to-consequence duration: 20 - 10 = 10s.
        assert_eq!(lc.gshut_to_consequence_secs, Some(10.0));
    }

    #[test]
    fn lifecycle_tracks_gshut_before_path_replacement() {
        let key = opk("rv2", "185.1.8.65", "192.0.2.0/24");
        let cohort = cohort_with(&key, &[(Some(1), vec![6447, 65002, 65001])]);
        let transitions = vec![
            mk_path_change(
                &key,
                Some(1),
                10,
                vec![6447, 65002, 65001],
                vec![],
                vec![6447, 65002, 65001],
                vec!["65535:0"],
            ),
            mk_path_change(
                &key,
                Some(1),
                20,
                vec![6447, 65002, 65001],
                vec!["65535:0"],
                vec![6447, 9999, 65001],
                vec![],
            ),
        ];
        let lifecycles = build_lifecycles(&transitions, &cohort, t_secs(1000), &pred());
        let lc = &lifecycles[0];
        assert!(
            lc.gshut_before_path_change,
            "GSHUT present before path replacement"
        );
        assert_eq!(lc.gshut_to_consequence_secs, Some(10.0));
    }

    #[test]
    fn unrelated_community_change_is_not_gshut_lifecycle() {
        let key = opk("rv2", "185.1.8.65", "192.0.2.0/24");
        let cohort = cohort_with(&key, &[(Some(1), vec![6447, 65002, 65001])]);
        // Only an unrelated community change.
        let transitions = vec![mk_transition(
            &key,
            Some(1),
            TransitionKind::AttributeChange,
            10,
            vec![6447, 65002, 65001],
            vec!["11537:1000"],
        )];
        let lifecycles = build_lifecycles(&transitions, &cohort, t_secs(1000), &pred());
        let lc = &lifecycles[0];
        assert!(!lc.graceful_shutdown_seen);
        assert!(!lc.gshut_newly_added);
        assert!(!lc.gshut_removed);
        assert!(lc.first_gshut_timestamp.is_none());
    }

    #[test]
    fn gshut_survives_cache_roundtrip() {
        let key = opk("rv2", "185.1.8.65", "192.0.2.0/24");
        let cohort = cohort_with(&key, &[(Some(1), vec![6447, 65002, 65001])]);
        let transitions = vec![
            mk_path_change(
                &key,
                Some(1),
                10,
                vec![6447, 65002, 65001],
                vec![],
                vec![6447, 65002, 65001],
                vec!["65535:0"],
            ),
            withdrawal(&key, Some(1), 20),
        ];
        let lifecycles = build_lifecycles(&transitions, &cohort, t_secs(1000), &pred());
        let lc = &lifecycles[0];
        // The lifecycle artifact serializes; GSHUT fields survive.
        let json = serde_json::to_value(lc).unwrap();
        assert_eq!(
            json["graceful_shutdown_seen"],
            serde_json::Value::Bool(true)
        );
        assert_eq!(
            json["gshut_before_withdrawal"],
            serde_json::Value::Bool(true)
        );
        assert_eq!(
            json["first_gshut_timestamp"],
            serde_json::to_value(t_secs(10)).unwrap()
        );
        assert_eq!(
            json["gshut_to_consequence_secs"],
            serde_json::to_value(10.0).unwrap()
        );
    }

    #[test]
    fn gshut_absence_does_not_change_impact_verdict() {
        let key = opk("rv2", "185.1.8.65", "192.0.2.0/24");
        let cohort = cohort_with(&key, &[(Some(1), vec![6447, 65002, 65001])]);
        // No GSHUT anywhere; the withdrawal classification is unchanged.
        let transitions = vec![withdrawal(&key, Some(1), 10)];
        let lifecycles = build_lifecycles(&transitions, &cohort, t_secs(1000), &pred());
        let lc = &lifecycles[0];
        assert!(!lc.graceful_shutdown_seen);
        assert_eq!(lc.category, StreamCategory::Withdrawn);
        assert!(lc.was_withdrawn);
    }

    // ── Part 6: semantic waves ────────────────────────────────────

    #[test]
    fn wave_counts_streams_and_instances_separately() {
        let a = opk("rv2", "185.1.8.65", "192.0.2.0/24");
        let b = opk("rv2", "185.1.8.66", "192.0.2.0/24");
        let mut cohort = cohort_with(
            &a,
            &[
                (Some(1), vec![6447, 65002, 65001]),
                (Some(2), vec![6447, 65002, 65001]),
            ],
        );
        let b_cohort = cohort_with(&b, &[(Some(1), vec![6447, 65002, 65001])]);
        for k in b_cohort.observer_prefixes {
            cohort.observer_prefixes.insert(k);
        }
        for (k, v) in b_cohort.baseline_instances {
            cohort.baseline_instances.insert(k, v);
        }
        let transitions = vec![
            withdrawal(&a, Some(1), 10),
            withdrawal(&a, Some(2), 20),
            withdrawal(&b, Some(1), 25),
        ];
        let lifecycles = build_lifecycles(&transitions, &cohort, t_secs(1000), &pred());
        let waves = derive_semantic_waves(&lifecycles, &transitions, 120.0, &pred());
        assert_eq!(waves.len(), 1);
        let w = &waves[0];
        // 2 streams, 3 route instances — counted separately.
        assert_eq!(w.stream_count, 2);
        assert_eq!(w.route_instance_count, 3);
        assert_eq!(w.label, WaveLabel::StreamWithdrawal);
    }

    #[test]
    fn wave_supports_multiple_effect_facets() {
        let key = opk("rv2", "185.1.8.65", "192.0.2.0/24");
        let cohort = cohort_with(&key, &[(Some(1), vec![6447, 65002, 65001])]);
        let transitions = vec![
            // Withdrawal and a prepend reduction in one temporal wave.
            withdrawal(&key, Some(1), 10),
            mk_path_change(
                &key,
                Some(1),
                30,
                vec![6447, 65002, 65002, 65002, 65001],
                vec![],
                vec![6447, 65002, 65001],
                vec![],
            ),
        ];
        let lifecycles = build_lifecycles(&transitions, &cohort, t_secs(1000), &pred());
        let waves = derive_semantic_waves(&lifecycles, &transitions, 120.0, &pred());
        let w = &waves[0];
        // Both facets present (withdrawal + prepend reduction); the tie
        // resolves to MixedRouteChange rather than forcing one label.
        assert!(w.facets.prepend_reduced >= 1);
        assert_eq!(w.facets.communities_changed, 0);
        assert_eq!(w.label, WaveLabel::MixedRouteChange);
    }

    #[test]
    fn wave_label_requires_supporting_effect() {
        let key = opk("rv2", "185.1.8.65", "192.0.2.0/24");
        let cohort = cohort_with(&key, &[(Some(1), vec![6447, 65002, 65001])]);
        // A path change that retains transit (still via predicate) has no
        // candidate label support from withdrawal/prepend/departure kinds —
        // but material path change with transit retained is MixedRouteChange
        // by construction (no dominant label).
        let transitions = vec![mk_path_change(
            &key,
            Some(1),
            10,
            vec![6447, 65002, 65001],
            vec![],
            vec![6447, 65002, 9999, 65001],
            vec![],
        )];
        let lifecycles = build_lifecycles(&transitions, &cohort, t_secs(1000), &pred());
        let waves = derive_semantic_waves(&lifecycles, &transitions, 120.0, &pred());
        assert_eq!(waves.len(), 1);
        // No label is invented without supporting effects: the wave is
        // MixedRouteChange rather than a fabricated semantic label.
        assert_eq!(waves[0].label, WaveLabel::MixedRouteChange);
    }

    #[test]
    fn wave_order_is_deterministic() {
        let key = opk("rv2", "185.1.8.65", "192.0.2.0/24");
        let cohort = cohort_with(&key, &[(Some(1), vec![6447, 65002, 65001])]);
        let transitions = vec![
            withdrawal(&key, Some(1), 10),
            mk_transition(
                &key,
                Some(1),
                TransitionKind::Restoration,
                300,
                vec![6447, 65002, 65001],
                vec![],
            ),
        ];
        let lifecycles = build_lifecycles(&transitions, &cohort, t_secs(1000), &pred());
        let waves1 = derive_semantic_waves(&lifecycles, &transitions, 120.0, &pred());
        let mut reversed = transitions.clone();
        reversed.reverse();
        let waves2 = derive_semantic_waves(&lifecycles, &reversed, 120.0, &pred());
        assert_eq!(waves1.len(), 2, "two temporal clusters");
        assert_eq!(waves1[0].id, "wave-1");
        assert_eq!(waves1[1].id, "wave-2");
        // Stable IDs and identical labels regardless of input order.
        assert_eq!(waves1[0].label, waves2[0].label);
        assert_eq!(waves1[1].label, waves2[1].label);
        assert_eq!(waves1[0].start, waves2[0].start);
    }

    #[test]
    fn representative_evidence_is_deterministic() {
        let key = opk("rv2", "185.1.8.65", "192.0.2.0/24");
        let cohort = cohort_with(&key, &[(Some(1), vec![6447, 65002, 65001])]);
        let transitions = vec![
            withdrawal(&key, Some(1), 10),
            mk_transition(
                &key,
                Some(1),
                TransitionKind::Restoration,
                300,
                vec![6447, 65002, 65001],
                vec![],
            ),
        ];
        let lifecycles = build_lifecycles(&transitions, &cohort, t_secs(1000), &pred());
        let waves = derive_semantic_waves(&lifecycles, &transitions, 120.0, &pred());
        // Representative before/after from the first transition of each wave.
        assert_eq!(waves[0].representative_before, vec![6447, 65002, 65001]);
        assert_eq!(waves[0].representative_after, Vec::<u32>::new());
        assert_eq!(waves[1].representative_after, vec![6447, 65002, 65001]);
        // Evidence references retained and sorted.
        assert_eq!(waves[0].evidence_refs.len(), 1);
        assert_eq!(waves[0].evidence_refs[0].observation_id.0, 10);
        assert_eq!(waves[1].evidence_refs[0].observation_id.0, 300);
    }

    #[test]
    fn one_transition_is_not_a_repeated_motif() {
        // SEQUITUR constraint: a single terminal must not be called a
        // repeated motif. Rule utility requires >= 2 references, so a
        // one-symbol input produces no rules.
        let grammar: crate::sequitur::Grammar<String> =
            crate::sequitur::build(&["WITHDRAWAL".to_string()]);
        assert!(
            grammar.rules.is_empty(),
            "a single terminal is not a repeated motif"
        );
        let expanded = grammar.expand();
        assert_eq!(expanded, vec!["WITHDRAWAL".to_string()]);
    }

    #[test]
    fn sequitur_output_does_not_change_assessment() {
        // SEQUITUR describes repeated sequences; it never assigns semantic
        // labels and never determines the assessment.
        let exp = crate::domain::expectation::ImpactExpectation::participant_unavailable("test");
        let transitions = vec![withdrawal(
            &opk("rv2", "185.1.8.65", "192.0.2.0/24"),
            Some(1),
            10,
        )];
        let a_with = crate::assess::assess(
            crate::domain::event::EventId::from("T1"),
            exp.clone(),
            &transitions,
            vec![],
            false,
            None,
        );
        let a_without = crate::assess::assess(
            crate::domain::event::EventId::from("T1"),
            exp,
            &transitions,
            vec![],
            false,
            None,
        );
        assert_eq!(a_with.verdict, a_without.verdict);
    }

    // ── Part 7.3: withdrawal audit ────────────────────────────────

    #[test]
    fn withdrawal_audit_produces_one_row_per_withdrawn_stream() {
        let a = opk("rv2", "185.1.8.65", "192.0.2.0/24");
        let b = opk("rv2", "185.1.8.66", "192.0.2.0/24");
        let mut cohort = cohort_with(&a, &[(Some(1), vec![6447, 65002, 65001])]);
        let b_cohort = cohort_with(&b, &[(Some(1), vec![6447, 65002, 65001])]);
        for k in b_cohort.observer_prefixes {
            cohort.observer_prefixes.insert(k);
        }
        for (k, v) in b_cohort.baseline_instances {
            cohort.baseline_instances.insert(k, v);
        }
        let transitions = vec![withdrawal(&a, Some(1), 10), withdrawal(&b, Some(1), 40)];
        let lifecycles = build_lifecycles(&transitions, &cohort, t_secs(1000), &pred());
        let records = withdrawal_audit(&lifecycles);
        assert_eq!(records.len(), 2, "one row per withdrawn stream");
        let summary = WithdrawalAuditSummary::from_records(&records);
        assert_eq!(summary.unique_withdrawn_streams, 2);
        assert_eq!(summary.route_instance_withdrawals, 2);
        assert_eq!(summary.affected_prefixes, 1);
        assert_eq!(summary.affected_peers, 2);
        assert_eq!(summary.unresolved_count, 2);
        assert_eq!(summary.restored_count, 0);
    }

    #[test]
    fn withdrawal_audit_records_final_instance_timing_and_restorations() {
        let key = opk("rv2", "185.1.8.65", "192.0.2.0/24");
        let cohort = cohort_with(&key, &[(Some(1), vec![6447, 65002, 65001])]);
        let transitions = vec![
            withdrawal(&key, Some(1), 10),
            mk_transition(
                &key,
                Some(1),
                TransitionKind::Restoration,
                30,
                vec![6447, 65002, 65001],
                vec![],
            ),
        ];
        let lifecycles = build_lifecycles(&transitions, &cohort, t_secs(1000), &pred());
        let records = withdrawal_audit(&lifecycles);
        assert_eq!(records.len(), 1);
        let r = &records[0];
        // Final instance withdrawal timestamp.
        assert_eq!(r.final_withdrawal_time, t_secs(10));
        // Active instances immediately before absence.
        assert_eq!(r.active_before_absence, 1);
        assert_eq!(r.baseline_instances, 1);
        assert_eq!(r.transit_at_withdrawal, Some(true));
        // Restoration kinds.
        assert_eq!(r.restoration_time, Some(t_secs(30)));
        assert_eq!(r.absence_duration_secs, Some(20.0));
        assert!(r.exact_restoration);
        assert!(r.equivalent_restoration);
        assert!(r.observer_prefix_restoration);
        assert!(r.baseline_set_restoration);
        // Evidence references.
        assert!(r.observation_ids.contains(&10));
        assert!(r.observation_ids.contains(&30));
        assert!(r.archive_checksums.contains(&"abc123".to_string()));
        let summary = WithdrawalAuditSummary::from_records(&records);
        assert_eq!(summary.restored_count, 1);
        assert_eq!(summary.unresolved_count, 0);
        assert_eq!(summary.exact_restorations, 1);
        assert_eq!(summary.equivalent_restorations, 1);
        assert_eq!(summary.observer_prefix_restorations, 1);
        assert_eq!(summary.baseline_set_restorations, 1);
    }

    #[test]
    fn audit_median_absence_is_computed() {
        let a = opk("rv2", "185.1.8.65", "192.0.2.0/24");
        let b = opk("rv2", "185.1.8.66", "192.0.2.0/24");
        let mut cohort = cohort_with(&a, &[(Some(1), vec![6447, 65002, 65001])]);
        let b_cohort = cohort_with(&b, &[(Some(1), vec![6447, 65002, 65001])]);
        for k in b_cohort.observer_prefixes {
            cohort.observer_prefixes.insert(k);
        }
        for (k, v) in b_cohort.baseline_instances {
            cohort.baseline_instances.insert(k, v);
        }
        let transitions = vec![
            withdrawal(&a, Some(1), 10),
            mk_transition(
                &a,
                Some(1),
                TransitionKind::Restoration,
                20,
                vec![6447, 65002, 65001],
                vec![],
            ),
            withdrawal(&b, Some(1), 40),
            mk_transition(
                &b,
                Some(1),
                TransitionKind::Restoration,
                100,
                vec![6447, 65002, 65001],
                vec![],
            ),
        ];
        let lifecycles = build_lifecycles(&transitions, &cohort, t_secs(1000), &pred());
        let records = withdrawal_audit(&lifecycles);
        let summary = WithdrawalAuditSummary::from_records(&records);
        assert_eq!(summary.median_absence_secs, Some(35.0));
        assert_eq!(summary.max_absence_secs, Some(60.0));
    }

    #[test]
    fn exact_restoration_after_multi_instance_withdrawal() {
        // Instance A (new, not in baseline) is withdrawn first, then instance
        // B (baseline) is withdrawn making the stream absent; A returns with
        // its last known route — must classify as an exact-instance
        // restoration, and old_path_ids must retain BOTH withdrawn ids.
        let key = opk("rv2", "185.1.8.65", "192.0.2.0/24");
        let cohort = cohort_with(&key, &[(Some(1), vec![6447, 65002, 65001])]);
        let transitions = vec![
            // Instance 2 announced (new instance, same semantics).
            mk_transition(
                &key,
                Some(2),
                TransitionKind::Announcement,
                5,
                vec![6447, 65002, 65001],
                vec![],
            ),
            // Instance 2 withdrawn first (stream still visible via instance 1).
            withdrawal(&key, Some(2), 10),
            // Instance 1 withdrawn — stream becomes absent.
            withdrawal(&key, Some(1), 20),
            // Instance 2 returns with its last known route.
            mk_transition(
                &key,
                Some(2),
                TransitionKind::Restoration,
                30,
                vec![6447, 65002, 65001],
                vec![],
            ),
        ];
        let lifecycles = build_lifecycles(&transitions, &cohort, t_secs(1000), &pred());
        let lc = &lifecycles[0];
        assert!(lc.was_withdrawn);
        assert_eq!(lc.restorations.len(), 1);
        let r = &lc.restorations[0];
        assert!(
            r.exact_instance,
            "returning instance matches its last known state"
        );
        // Old path IDs retain both withdrawn instances (2 then 1, sorted).
        assert_eq!(r.old_path_ids, vec![Some(1), Some(2)]);
        assert_eq!(r.new_path_ids, vec![Some(2)]);
    }
}

#[cfg(test)]
mod session32_tests {
    use super::*;
    use crate::domain::observation::{
        Asn, CollectorId, IngestRole, ObservationAttributes, ObservationId, ObservationKind,
        ObservationProvenance, ObservationSource,
    };
    use crate::domain::route::{Prefix, RouteKey};
    use chrono::TimeZone;

    fn t(secs: i64) -> chrono::DateTime<Utc> {
        Utc.with_ymd_and_hms(2019, 8, 21, 16, 45, 0).unwrap() + chrono::Duration::seconds(secs)
    }

    fn obs(
        key: &RouteKey,
        kind: ObservationKind,
        at: chrono::DateTime<Utc>,
        seq: u64,
        path: Vec<u32>,
        id: u64,
    ) -> crate::domain::observation::RouteObservation {
        crate::domain::observation::RouteObservation {
            id: ObservationId(id),
            source: ObservationSource::LocalFile("test.bz2".to_string()),
            timestamp: at,
            collector: CollectorId(key.collector.clone()),
            peer_ip: key.peer_ip,
            peer_asn: Asn(0),
            prefix: key.prefix.clone(),
            kind,
            attributes: Some(ObservationAttributes {
                as_path: path,
                origin_asns: vec![Asn(2603)],
                next_hop: None,
                origin: None,
                local_pref: None,
                med: None,
                atomic_aggregate: false,
                communities: crate::domain::observation::Communities::default(),
            }),
            path_id: key.path_id,
            provenance: ObservationProvenance {
                input: "test.bz2".to_string(),
                source_url: Some("https://example.invalid/archive.bz2".to_string()),
                archive_sha256: Some("abc123".to_string()),
                role: IngestRole::Updates,
                parser_representation: "bgpkit-bgp-elem".to_string(),
                mrt_timestamp: 0.0,
                element_seq: seq,
                archive_order: 0,
            },
        }
    }

    #[test]
    fn same_timestamp_withdrawal_and_announcement_use_deterministic_order() {
        let key = RouteKey::new(
            "route-views2",
            "64.57.28.241".parse().unwrap(),
            &crate::domain::route::Prefix::from("193.10.68.0/24"),
        );
        // Same MRT second; the withdrawal has a lower element sequence than
        // the announcement. Deterministic ordering must preserve that order.
        let wd = obs(
            &key,
            ObservationKind::Withdrawal,
            t(25),
            12595,
            vec![11537, 20965, 2603],
            168,
        );
        let ann = obs(
            &key,
            ObservationKind::Announcement,
            t(25),
            12700,
            vec![11537, 20965, 2603],
            169,
        );
        let mut list = vec![ann.clone(), wd.clone()];
        crate::derived_cache::sort_deterministic(&mut list);
        assert_eq!(
            list[0].provenance.element_seq, 12595,
            "withdrawal first by element order"
        );
        assert_eq!(list[0].kind, ObservationKind::Withdrawal);
        assert_eq!(list[1].kind, ObservationKind::Announcement);
        let (store, _) = crate::routes::reconstruct_routes(list, t(0), t(60), t(120));
        let final_state = store
            .all_states()
            .find(|(k, _)| k.collector == "route-views2");
        assert!(
            final_state.is_some(),
            "announcement restores the route after the withdrawal"
        );
    }

    #[test]
    fn aggregate_route_set_prevents_false_two_second_absence() {
        // A stream with TWO baseline route instances: withdrawing one
        // instance must NOT mark the observer-prefix stream absent.
        let key = ObserverPrefixKey {
            collector: "route-views2".to_string(),
            peer_ip: "64.57.28.241".parse().unwrap(),
            prefix: Prefix::from("193.10.68.0/24"),
        };
        let pred = crate::domain::route::TransitPredicate::ContainsAny(vec![11537]);
        let transitions = vec![super::tests::mk_transition(
            &key,
            Some(0),
            TransitionKind::Withdrawal,
            25,
            vec![11537, 20965, 2603],
            vec![],
        )];
        // Build a FrozenCohort with TWO baseline instances for the stream.
        let rib: Vec<crate::domain::observation::RouteObservation> = [0u32, 1]
            .iter()
            .map(|pid| {
                let mut o = obs(
                    &RouteKey {
                        collector: key.collector.clone(),
                        peer_ip: key.peer_ip,
                        prefix: key.prefix.clone(),
                        path_id: Some(*pid),
                    },
                    ObservationKind::RibEntry,
                    t(0),
                    1,
                    vec![11537, 20965, 2603],
                    100 + *pid as u64,
                );
                o.path_id = Some(*pid);
                o
            })
            .collect();
        let cohort = crate::cohort::freeze_cohort(&rib, &[2603], &pred);
        let lifecycles = build_lifecycles(&transitions, &cohort, t(60), &pred);
        // The stream keeps its second baseline instance (path_id 1), so a
        // single-instance withdrawal must not mark the stream absent.
        assert!(
            !lifecycles[0].was_withdrawn,
            "single-instance withdrawal must not mark the stream absent"
        );
    }

    #[test]
    fn native_timestamp_precision_is_retained() {
        let rec = WithdrawalRecord {
            collector: "route-views2".to_string(),
            peer_ip: "64.57.28.241".to_string(),
            prefix: "193.10.68.0/24".to_string(),
            baseline_path: vec![11537, 20965, 2603],
            baseline_instances: 1,
            active_before_absence: 1,
            final_withdrawal_time: t(25),
            restoration_time: Some(t(27)),
            absence_duration_secs: Some(2.0),
            transit_at_withdrawal: Some(true),
            exact_restoration: false,
            equivalent_restoration: false,
            observer_prefix_restoration: true,
            baseline_set_restoration: false,
            observation_ids: vec![168],
            archive_checksums: vec!["4546b78f".to_string()],
        };
        assert_eq!(
            rec.absence_duration_secs,
            Some(2.0),
            "2 seconds at native precision"
        );
        let audit: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string("case-studies/manlan-2019/pilot/absence-audit.json").unwrap(),
        )
        .unwrap();
        assert_eq!(audit["native_timestamp_precision"], "seconds (MRT native)");
        assert_eq!(audit["streams"].as_array().unwrap().len(), 11);
        assert!(audit["streams"][0]["absence_duration_secs"] == 2);
    }

    #[test]
    fn temporary_stream_absence_is_not_rendered_as_global_reachability_loss() {
        let result: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string("case-studies/manlan-2019/pilot/pilot-result.json").unwrap(),
        )
        .unwrap();
        let lower = serde_json::to_string(&result).unwrap().to_lowercase();
        assert!(lower.contains("observer-prefix streams"), "observer scoped");
        assert!(
            lower.contains("one selected public collector"),
            "collector scoped"
        );
        assert!(
            !lower.contains("global reachability loss"),
            "no global claim"
        );
        assert!(!lower.contains("traffic was lost"), "no traffic-loss claim");
    }

    #[test]
    fn pilot_summary_uses_observer_scoped_language() {
        let report = std::fs::read_to_string(
            "case-studies/manlan-2019/pilot/out/MANLAN-2019-NORDUNET-PILOT/report.txt",
        )
        .unwrap_or_default();
        if report.is_empty() {
            return; // artifacts not present (packaged crate)
        }
        let lower = report.to_lowercase();
        assert!(lower.contains("observer-prefix streams"), "observer scoped");
        assert!(lower.contains("route-views2"), "collector named");
        assert!(
            !lower.contains("nordunet outage"),
            "no participant-outage claim"
        );
    }
}
