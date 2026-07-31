//! Per-stream lifecycle analysis — audit what happened to every frozen
//! observer-route stream, classify the shape of path changes, and
//! produce withdrawal audits and semantic wave descriptions.
//!
//! All classifications are derived interpretations layered over the
//! auditable RouteTransition evidence. Raw before/after paths are never
//! modified or replaced.

use chrono::{DateTime, Utc};
use serde::Serialize;
use std::collections::{HashMap, HashSet};

use crate::domain::route::{AnalysisPhase, RouteKey, RouteState, RouteTransition, TransitionKind};
use crate::target::TargetSet;

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

/// Classify a path change between two states with respect to a required transit ASN.
///
/// `from` and `to` are the before/after route states. `required_transit` is the
/// ASN that must be present for the path to be considered "via the required transit".
pub fn classify_path_change(
    from: &RouteState,
    to: &RouteState,
    required_transit: u32,
) -> PathShapeChange {
    let from_path = &from.attributes.as_path.0;
    let to_path = &to.attributes.as_path.0;

    let from_has = from_path.contains(&required_transit);
    let to_has = to_path.contains(&required_transit);

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

// ── Stream classification ──────────────────────────────────────────

/// Primary category for a frozen observer-route stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub enum StreamCategory {
    /// No transitions observed for this stream.
    Unchanged,
    /// Only prepend changes (collapsed-equivalent paths).
    PrependOnly,
    /// Path changed materially but still via the required transit ASN.
    PathChangedStillViaInternet2,
    /// Path departed the required transit ASN.
    DepartedInternet2Path,
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
}

// ── Stream lifecycle ───────────────────────────────────────────────

/// Complete lifecycle record for a single frozen observer-route stream.
#[derive(Debug, Clone, Serialize)]
pub struct StreamLifecycle {
    /// Collector identifier.
    pub collector: String,
    /// Peer IP address.
    pub peer_ip: String,
    /// BGP prefix.
    pub prefix: String,
    /// Baseline AS path (from frozen RIB).
    pub baseline_path: Vec<u32>,
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
    /// Whether the route was ever withdrawn.
    pub was_withdrawn: bool,
    /// Whether a replacement path appeared (after withdrawal or path change).
    pub replacement_appeared: bool,
    /// Whether the replacement path retained the required transit ASN.
    pub replacement_retained_transit: Option<bool>,
    /// Whether prepending changed.
    pub prepending_changed: bool,
    /// Cooldown-phase transitions.
    pub cooldown_transitions: Vec<LifecycleTransition>,
    /// Final state at end of observation window.
    pub final_state: Option<RouteState>,
    /// Whether the event baseline was restored.
    pub baseline_restored: bool,
    /// Restoration timestamp (first restoration after withdrawal/departure).
    pub restoration_time: Option<DateTime<Utc>>,
    /// Total affected duration: first change → restoration or cooldown_end.
    pub affected_duration_secs: Option<f64>,
}

/// A lightweight transition record for the lifecycle.
#[derive(Debug, Clone, Serialize)]
pub struct LifecycleTransition {
    pub timestamp: DateTime<Utc>,
    pub phase: AnalysisPhase,
    pub kind: String,
    pub before_path: Vec<u32>,
    pub after_path: Vec<u32>,
    pub path_shape: Option<PathShapeChange>,
    pub observation_id: u64,
    pub archive_sha256: Option<String>,
}

// ── Withdrawal audit ───────────────────────────────────────────────

/// A single withdrawn stream record.
#[derive(Debug, Clone, Serialize)]
pub struct WithdrawalRecord {
    pub collector: String,
    pub peer_ip: String,
    pub prefix: String,
    pub baseline_path: Vec<u32>,
    pub withdrawal_time: DateTime<Utc>,
    pub restoration_time: Option<DateTime<Utc>>,
    pub absence_duration_secs: Option<f64>,
    /// Whether another selected observer still advertised the prefix at withdrawal time.
    /// Computed from the transition timeline; documented as approximate.
    pub another_observer_still_advertised: Option<bool>,
    pub restored_to_baseline: Option<bool>,
    pub observation_ids: Vec<u64>,
    pub archive_checksums: Vec<String>,
}

// ── Semantic waves ─────────────────────────────────────────────────

/// A semantically-labeled wave describing a distinct phase of the event.
#[derive(Debug, Clone, Serialize)]
pub struct SemanticWave {
    /// Label derived from the dominant transition class.
    pub label: String,
    /// First observation timestamp.
    pub first: DateTime<Utc>,
    /// Peak observation interval (start of densest cluster).
    pub peak_start: DateTime<Utc>,
    /// Last observation timestamp.
    pub last: DateTime<Utc>,
    /// Number of unique streams affected.
    pub unique_streams: usize,
    /// Number of unique prefixes affected.
    pub unique_prefixes: usize,
    /// Number of unique peers.
    pub unique_peers: usize,
    /// Dominant transition classification.
    pub dominant_class: String,
    /// Representative before/after AS paths.
    pub representative_before: Vec<u32>,
    pub representative_after: Vec<u32>,
    /// Duration in seconds.
    pub duration_secs: f64,
}

// ── Builder ────────────────────────────────────────────────────────

/// Build per-stream lifecycle records from transitions and the frozen target set.
pub fn build_lifecycles(
    transitions: &[RouteTransition],
    target_set: &TargetSet,
    cooldown_end: DateTime<Utc>,
    required_transit: u32,
) -> Vec<StreamLifecycle> {
    // Group transitions by stream key
    let mut stream_transitions: HashMap<RouteKey, Vec<&RouteTransition>> = HashMap::new();
    for t in transitions {
        stream_transitions.entry(t.key.clone()).or_default().push(t);
    }

    // Sort each group by timestamp
    for v in stream_transitions.values_mut() {
        v.sort_by_key(|t| t.to.timestamp());
    }

    // Build set of frozen keys from the target set
    let frozen_keys: HashSet<RouteKey> = target_set
        .streams
        .iter()
        .flat_map(|(collector, streams)| {
            streams
                .iter()
                .map(move |s| RouteKey::new(collector, s.peer_ip, &s.prefix))
        })
        .collect();

    // For frozen keys with no transitions, create Unchanged lifecycle
    let mut lifecycles: Vec<StreamLifecycle> = Vec::new();
    let mut seen_keys: HashSet<RouteKey> = HashSet::new();

    for t in transitions {
        if !seen_keys.insert(t.key.clone()) {
            continue;
        }
        let stream_t = &stream_transitions[&t.key];
        let lifecycle = build_one_lifecycle(&t.key, stream_t, cooldown_end, required_transit);
        lifecycles.push(lifecycle);
    }

    // Add unchanged frozen streams
    for key in &frozen_keys {
        if !seen_keys.contains(key) {
            // Find baseline path from target set
            let baseline = find_baseline_path(target_set, key);
            lifecycles.push(StreamLifecycle {
                collector: key.collector.clone(),
                peer_ip: key.peer_ip.to_string(),
                prefix: key.prefix.0.clone(),
                baseline_path: baseline,
                category: StreamCategory::Unchanged,
                flags: StreamFlags::default(),
                first_change: None,
                transitions: vec![],
                min_absence_secs: None,
                max_absence_secs: None,
                was_withdrawn: false,
                replacement_appeared: false,
                replacement_retained_transit: None,
                prepending_changed: false,
                cooldown_transitions: vec![],
                final_state: None,
                baseline_restored: true,
                restoration_time: None,
                affected_duration_secs: None,
            });
        }
    }

    lifecycles
}

fn find_baseline_path(target_set: &TargetSet, key: &RouteKey) -> Vec<u32> {
    if let Some(streams) = target_set.streams.get(&key.collector) {
        for s in streams {
            if s.peer_ip == key.peer_ip && s.prefix == key.prefix {
                return s.as_path.clone();
            }
        }
    }
    vec![]
}

/// Build a lifecycle record for a single stream from its transitions.
fn build_one_lifecycle(
    key: &RouteKey,
    transitions: &[&RouteTransition],
    cooldown_end: DateTime<Utc>,
    required_transit: u32,
) -> StreamLifecycle {
    let baseline_path = transitions
        .first()
        .and_then(|t| {
            t.event_baseline
                .as_ref()
                .and_then(|eb| eb.state.as_ref())
                .map(|s| s.attributes.as_path.0.clone())
        })
        .unwrap_or_default();

    let mut was_withdrawn = false;
    let mut replacement_appeared = false;
    let mut replacement_retained_transit: Option<bool> = None;
    let mut prepending_changed = false;
    let mut withdrawal_count = 0;
    let mut restoration_count = 0;
    let mut baseline_restored = false;
    let mut first_change: Option<DateTime<Utc>> = None;
    let mut restoration_time: Option<DateTime<Utc>> = None;
    let mut absence_intervals: Vec<f64> = Vec::new();
    let mut current_absence_start: Option<DateTime<Utc>> = None;
    let mut seen_categories: HashSet<StreamCategory> = HashSet::new();
    let mut is_absent = false;

    // Collect lifecycle transitions
    let mut lifecycle_transitions: Vec<LifecycleTransition> = Vec::new();
    let mut cooldown_transitions: Vec<LifecycleTransition> = Vec::new();
    let mut final_state: Option<RouteState> = None;

    for t in transitions {
        let phase = t.phase;
        let lct = LifecycleTransition {
            timestamp: t.to.timestamp(),
            phase,
            kind: transition_kind_str(&t.kind),
            before_path: t
                .from
                .as_ref()
                .and_then(|f| f.state.as_ref())
                .map(|s| s.attributes.as_path.0.clone())
                .unwrap_or_default(),
            after_path: t
                .to
                .state
                .as_ref()
                .map(|s| s.attributes.as_path.0.clone())
                .unwrap_or_default(),
            path_shape: path_shape_from_transition(t, required_transit),
            observation_id: t.triggering.observation_id.0,
            archive_sha256: t.triggering.archive_sha256.clone(),
        };

        if phase == AnalysisPhase::Cooldown {
            cooldown_transitions.push(lct);
        } else {
            lifecycle_transitions.push(lct);
        }

        // Track first event-window change
        if phase == AnalysisPhase::Event && first_change.is_none() {
            first_change = Some(t.to.timestamp());
        }

        // Classify and track
        match &t.kind {
            TransitionKind::Withdrawal => {
                was_withdrawn = true;
                withdrawal_count += 1;
                seen_categories.insert(StreamCategory::Withdrawn);
                if !is_absent {
                    current_absence_start = Some(t.to.timestamp());
                    is_absent = true;
                }
                // Track final state
                final_state = None;
            }
            TransitionKind::Announcement
            | TransitionKind::Restoration
            | TransitionKind::ReturnToBaseline => {
                restoration_count += 1;
                if restoration_time.is_none()
                    && (was_withdrawn
                        || seen_categories.contains(&StreamCategory::DepartedInternet2Path))
                {
                    restoration_time = Some(t.to.timestamp());
                }

                // Track baseline restoration
                if matches!(t.kind, TransitionKind::ReturnToBaseline) {
                    baseline_restored = true;
                }

                // Track absence interval end
                if is_absent {
                    if let Some(start) = current_absence_start {
                        let duration = (t.to.timestamp() - start).num_seconds() as f64;
                        if duration > 0.0 {
                            absence_intervals.push(duration);
                        }
                    }
                    is_absent = false;
                    current_absence_start = None;
                }

                if let Some(ref state) = t.to.state {
                    final_state = Some(state.clone());
                    replacement_appeared = true;
                    if replacement_retained_transit.is_none() {
                        replacement_retained_transit =
                            Some(state.attributes.as_path.0.contains(&required_transit));
                    }
                }
            }
            TransitionKind::PathChange { .. } => {
                if let Some(ref state) = t.to.state {
                    final_state = Some(state.clone());
                }
                // Classify the path change
                if let (Some(from), Some(to)) = (
                    t.from.as_ref().and_then(|f| f.state.as_ref()),
                    t.to.state.as_ref(),
                ) {
                    let shape = classify_path_change(from, to, required_transit);
                    match shape {
                        PathShapeChange::PrependReduced | PathShapeChange::PrependIncreased => {
                            prepending_changed = true;
                            seen_categories.insert(StreamCategory::PrependOnly);
                        }
                        PathShapeChange::PathChangedStillViaRequiredTransit => {
                            seen_categories.insert(StreamCategory::PathChangedStillViaInternet2);
                        }
                        PathShapeChange::PathDepartedRequiredTransit => {
                            seen_categories.insert(StreamCategory::DepartedInternet2Path);
                        }
                        PathShapeChange::PathReturnedToRequiredTransit => {
                            // Returning to transit — track restoration
                            if restoration_time.is_none() {
                                restoration_time = Some(t.to.timestamp());
                            }
                        }
                        PathShapeChange::GenericPathChange => {}
                    }
                }
            }
            _ => {}
        }
    }

    // If still absent at the end, close the absence interval at cooldown_end
    if is_absent {
        if let Some(start) = current_absence_start {
            let duration = (cooldown_end - start).num_seconds() as f64;
            if duration > 0.0 {
                absence_intervals.push(duration);
            }
        }
    }

    // Determine primary category by precedence: Withdrawn > Departed > StillVia > PrependOnly > Unchanged
    let category = if seen_categories.contains(&StreamCategory::Withdrawn) {
        StreamCategory::Withdrawn
    } else if seen_categories.contains(&StreamCategory::DepartedInternet2Path) {
        StreamCategory::DepartedInternet2Path
    } else if seen_categories.contains(&StreamCategory::PathChangedStillViaInternet2) {
        StreamCategory::PathChangedStillViaInternet2
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

    let not_restored = was_withdrawn && !baseline_restored
        || (seen_categories.contains(&StreamCategory::DepartedInternet2Path) && !baseline_restored);
    let restored = restoration_count > 0 && !not_restored;

    StreamLifecycle {
        collector: key.collector.clone(),
        peer_ip: key.peer_ip.to_string(),
        prefix: key.prefix.0.clone(),
        baseline_path,
        category,
        flags: StreamFlags {
            restored,
            not_restored,
            multiple_cycles: withdrawal_count > 1
                || (withdrawal_count > 0 && restoration_count > 1),
        },
        first_change,
        transitions: lifecycle_transitions,
        min_absence_secs: min_absence,
        max_absence_secs: max_absence,
        was_withdrawn,
        replacement_appeared,
        replacement_retained_transit,
        prepending_changed,
        cooldown_transitions,
        final_state,
        baseline_restored,
        restoration_time,
        affected_duration_secs: affected_duration,
    }
}

/// Extract path shape from a transition.
fn path_shape_from_transition(
    t: &RouteTransition,
    required_transit: u32,
) -> Option<PathShapeChange> {
    if !matches!(t.kind, TransitionKind::PathChange { .. }) {
        return None;
    }
    let from = t.from.as_ref().and_then(|f| f.state.as_ref())?;
    let to = t.to.state.as_ref()?;
    Some(classify_path_change(from, to, required_transit))
}

fn transition_kind_str(kind: &TransitionKind) -> String {
    match kind {
        TransitionKind::Announcement => "Announcement".into(),
        TransitionKind::Withdrawal => "Withdrawal".into(),
        TransitionKind::ExactDuplicate => "ExactDuplicate".into(),
        TransitionKind::PathChange { .. } => "PathChange".into(),
        TransitionKind::AttributeChange => "AttributeChange".into(),
        TransitionKind::SessionReset => "SessionReset".into(),
        TransitionKind::Restoration => "Restoration".into(),
        TransitionKind::ReturnToBaseline => "ReturnToBaseline".into(),
    }
}

// ── Withdrawal audit ───────────────────────────────────────────────

/// Produce a withdrawal audit from the lifecycle records.
pub fn withdrawal_audit(lifecycles: &[StreamLifecycle]) -> Vec<WithdrawalRecord> {
    let mut records = Vec::new();

    for lc in lifecycles {
        if !lc.was_withdrawn {
            continue;
        }

        // Find withdrawal and restoration times
        let mut withdrawal_time: Option<DateTime<Utc>> = None;
        let mut restoration_time: Option<DateTime<Utc>> = None;
        let mut observation_ids: Vec<u64> = Vec::new();
        let mut archive_checksums: Vec<String> = Vec::new();
        let mut restored_to_baseline: Option<bool> = None;

        for t in &lc.transitions {
            if t.kind == "Withdrawal" {
                if withdrawal_time.is_none() {
                    withdrawal_time = Some(t.timestamp);
                }
                observation_ids.push(t.observation_id);
                if let Some(ref sha) = t.archive_sha256 {
                    if !archive_checksums.contains(sha) {
                        archive_checksums.push(sha.clone());
                    }
                }
            }
            if t.kind == "Restoration" || t.kind == "ReturnToBaseline" {
                if restoration_time.is_none() {
                    restoration_time = Some(t.timestamp);
                }
                if t.kind == "ReturnToBaseline" {
                    restored_to_baseline = Some(true);
                }
                observation_ids.push(t.observation_id);
                if let Some(ref sha) = t.archive_sha256 {
                    if !archive_checksums.contains(sha) {
                        archive_checksums.push(sha.clone());
                    }
                }
            }
        }

        let absence_duration = match (withdrawal_time, restoration_time) {
            (Some(w), Some(r)) => Some(((r - w).num_seconds() as f64).max(0.0)),
            _ => None,
        };

        records.push(WithdrawalRecord {
            collector: lc.collector.clone(),
            peer_ip: lc.peer_ip.clone(),
            prefix: lc.prefix.clone(),
            baseline_path: lc.baseline_path.clone(),
            withdrawal_time: withdrawal_time.unwrap_or(lc.first_change.unwrap()),
            restoration_time,
            absence_duration_secs: absence_duration,
            another_observer_still_advertised: None, // Requires cross-stream analysis — documented limitation
            restored_to_baseline,
            observation_ids,
            archive_checksums,
        });
    }

    records
}

// ── Semantic waves ─────────────────────────────────────────────────

/// Derive semantic wave descriptions from lifecycle and transition data.
pub fn derive_semantic_waves(
    lifecycles: &[StreamLifecycle],
    transitions: &[RouteTransition],
    max_gap_secs: f64,
) -> Vec<SemanticWave> {
    if transitions.is_empty() {
        return vec![];
    }

    // Collect all event-window transitions sorted by timestamp
    let mut event_transitions: Vec<&RouteTransition> = transitions
        .iter()
        .filter(|t| t.phase == AnalysisPhase::Event)
        .collect();
    event_transitions.sort_by_key(|t| t.to.timestamp());

    if event_transitions.is_empty() {
        return vec![];
    }

    // Group into temporal clusters
    let mut waves: Vec<SemanticWave> = Vec::new();
    let mut current_group: Vec<&RouteTransition> = vec![event_transitions[0]];

    for window in event_transitions.windows(2) {
        let prev = window[0];
        let next = window[1];
        let gap = (next.to.timestamp() - prev.to.timestamp()).num_seconds() as f64;

        if gap <= max_gap_secs {
            current_group.push(next);
        } else {
            waves.push(build_semantic_wave(&current_group, lifecycles));
            current_group = vec![next];
        }
    }

    if !current_group.is_empty() {
        waves.push(build_semantic_wave(&current_group, lifecycles));
    }

    // Label waves sequentially
    for (i, wave) in waves.iter_mut().enumerate() {
        wave.label = format!("wave-{} {}", i + 1, wave.label);
    }

    waves
}

fn build_semantic_wave(
    group: &[&RouteTransition],
    _lifecycles: &[StreamLifecycle],
) -> SemanticWave {
    let first = group[0].to.timestamp();
    let last = group[group.len() - 1].to.timestamp();
    let mid = group[group.len() / 2].to.timestamp();

    // Count unique streams, prefixes, peers
    let mut streams = HashSet::new();
    let mut prefixes = HashSet::new();
    let mut peers = HashSet::new();
    for t in group {
        streams.insert(format!(
            "{}:{}:{}",
            t.key.collector, t.key.peer_ip, t.key.prefix.0
        ));
        prefixes.insert(t.key.prefix.0.clone());
        peers.insert(t.key.peer_ip.to_string());
    }

    // Determine dominant class
    let mut class_counts: HashMap<String, usize> = HashMap::new();
    let mut rep_before: Vec<u32> = vec![];
    let mut rep_after: Vec<u32> = vec![];
    for t in group {
        let class = transition_kind_str(&t.kind);
        *class_counts.entry(class).or_default() += 1;
        if rep_before.is_empty() {
            rep_before = t
                .from
                .as_ref()
                .and_then(|f| f.state.as_ref())
                .map(|s| s.attributes.as_path.0.clone())
                .unwrap_or_default();
            rep_after =
                t.to.state
                    .as_ref()
                    .map(|s| s.attributes.as_path.0.clone())
                    .unwrap_or_default();
        }
    }
    let dominant_class = class_counts
        .into_iter()
        .max_by_key(|(_, c)| *c)
        .map(|(k, _)| k)
        .unwrap_or_default();

    // Determine label
    let label = match dominant_class.as_str() {
        "PathChange" if rep_before.len() > rep_after.len() => "prepend reduction".to_string(),
        "PathChange" if rep_before.len() < rep_after.len() => "prepend increase".to_string(),
        "PathChange" => "path change".to_string(),
        "Withdrawal" => "withdrawal".to_string(),
        "Restoration" | "ReturnToBaseline" => "route restoration".to_string(),
        _ => dominant_class.to_lowercase(),
    };

    SemanticWave {
        label,
        first,
        peak_start: mid,
        last,
        unique_streams: streams.len(),
        unique_prefixes: prefixes.len(),
        unique_peers: peers.len(),
        dominant_class,
        representative_before: rep_before,
        representative_after: rep_after,
        duration_secs: ((last - first).num_seconds() as f64).max(0.0),
    }
}
