//! Route types — prefixes, AS paths, route state, transitions, and keys.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::net::IpAddr;

use super::observation::{Asn, EvidenceRef};

/// A BGP prefix (e.g. "192.0.2.0/24").
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Prefix(pub String);

impl From<&str> for Prefix {
    fn from(s: &str) -> Self {
        Prefix(s.to_string())
    }
}

impl std::fmt::Display for Prefix {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// An AS path (sequence of ASNs).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AsPath(pub Vec<u32>);

impl std::fmt::Display for AsPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let path: Vec<String> = self.0.iter().map(|asn| asn.to_string()).collect();
        write!(f, "{}", path.join(" "))
    }
}

/// Route attributes observed at a point in time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RouteAttributes {
    pub as_path: AsPath,
    /// The ASNs originating the route (rightmost in path).
    pub origin_asns: Vec<Asn>,
    /// Next-hop IP address.
    pub next_hop: Option<IpAddr>,
    /// BGP origin type (IGP, EGP, INCOMPLETE).
    pub origin: Option<String>,
    /// Multi-exit discriminator.
    pub med: Option<u32>,
    /// Local preference.
    pub local_pref: Option<u32>,
    /// Atomic aggregate flag.
    pub atomic_aggregate: bool,
    /// Communities as string representations (e.g. "11537:1000").
    pub communities: Vec<String>,
}

impl RouteAttributes {
    /// Create bare route attributes from an AS path.
    pub fn from_as_path(as_path: Vec<u32>) -> Self {
        let origin_asns = as_path.last().map(|&a| vec![Asn(a)]).unwrap_or_default();
        RouteAttributes {
            as_path: AsPath(as_path),
            origin_asns,
            next_hop: None,
            origin: None,
            med: None,
            local_pref: None,
            atomic_aggregate: false,
            communities: vec![],
        }
    }

    /// Create empty route attributes (for absent/withdrawn routes).
    pub fn empty() -> Self {
        RouteAttributes {
            as_path: AsPath(vec![]),
            origin_asns: vec![],
            next_hop: None,
            origin: None,
            med: None,
            local_pref: None,
            atomic_aggregate: false,
            communities: vec![],
        }
    }
}

/// A unique key identifying a route: collector + peer + prefix.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RouteKey {
    pub collector: String,
    pub peer_ip: IpAddr,
    pub prefix: Prefix,
}

impl RouteKey {
    pub fn new(collector: &str, peer_ip: IpAddr, prefix: &Prefix) -> Self {
        RouteKey {
            collector: collector.to_string(),
            peer_ip,
            prefix: prefix.clone(),
        }
    }
}

/// The state of a route as observed by a specific collector/peer at a specific time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RouteState {
    pub prefix: Prefix,
    pub attributes: RouteAttributes,
    pub timestamp: DateTime<Utc>,
    /// The observer that reported this state (collector:peer, e.g. "route-views2:AS6447").
    pub observer: String,
}

impl RouteState {
    pub fn to_key(&self) -> RouteKey {
        // Parse peer IP from observer string like "route-views2:185.1.8.65"
        let peer_ip: IpAddr = self
            .observer
            .split(':')
            .nth(1)
            .and_then(|s| s.parse().ok())
            .unwrap_or_else(|| "0.0.0.0".parse().unwrap());

        let collector = self.observer.split(':').next().unwrap_or("").to_string();

        RouteKey::new(&collector, peer_ip, &self.prefix)
    }
}

/// The kind of route transition between two states.
///
/// Classification lives only in tokenize::diff.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransitionKind {
    /// A previously absent route is announced.
    Announcement,
    /// A route is withdrawn.
    Withdrawal,
    /// An exact duplicate of the previous state (no change).
    Duplicate,
    /// The AS path changed (e.g. failover to alternate).
    PathReplacement { old: AsPath, new: AsPath },
    /// Non-path attributes changed (no path difference).
    AttributeChange,
    /// Observer session discontinuity — not a real route change.
    SessionReset,
    /// A previously withdrawn route is restored with its original path.
    Restoration,
    /// Return to event baseline after a change.
    ReturnToBaseline,
}

/// Orthogonal effects that may co-occur with a primary TransitionKind.
///
/// A single BGP observation may simultaneously change the path AND add
/// a community AND modify MED. These facets are always computed, never
/// forced into a single mutually-exclusive category.
///
/// Transit-specific effects (departure, return) are NOT here — they
/// belong to `EventRelativeEffects` computed by `interpret_for_event`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct GenericTransitionEffects {
    /// Route gained the GRACEFUL_SHUTDOWN community (65535:0).
    pub graceful_shutdown_added: bool,
    /// Route lost the GRACEFUL_SHUTDOWN community (65535:0).
    pub graceful_shutdown_removed: bool,
    /// Prepend classification (None/Increased/Reduced/Indeterminate).
    pub prepend: PrependChange,
    /// Collapsed AS paths differ materially (not just prepending).
    pub material_path_changed: bool,
    /// Communities changed (any change, including GSHUT).
    pub communities_changed: bool,
    /// MED value changed.
    pub med_changed: bool,
    /// Local preference changed.
    pub local_pref_changed: bool,
    /// Origin type changed.
    pub origin_changed: bool,
    /// Next hop changed where represented.
    pub next_hop_changed: bool,
}

/// Mutually exclusive prepend change classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum PrependChange {
    /// No prepend change.
    #[default]
    None,
    /// Prepend count increased.
    Increased,
    /// Prepend count decreased.
    Reduced,
    /// Cannot safely determine (AS sets, confederation segments).
    Indeterminate,
}

/// Predicate for evaluating route paths against event-specific requirements.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransitPredicate {
    /// Path must contain at least one of the given ASNs.
    ContainsAny(Vec<u32>),
    /// Path must contain all of the given ASNs.
    ContainsAll(Vec<u32>),
    /// Two ASNs must be adjacent (unordered) in the path.
    Adjacent(u32, u32),
}

impl TransitPredicate {
    /// Evaluate this predicate against an AS path.
    pub fn evaluate(&self, path: &[u32]) -> bool {
        match self {
            TransitPredicate::ContainsAny(asns) => asns.iter().any(|a| path.contains(a)),
            TransitPredicate::ContainsAll(asns) => asns.iter().all(|a| path.contains(a)),
            TransitPredicate::Adjacent(a, b) => path
                .windows(2)
                .any(|w| (w[0] == *a && w[1] == *b) || (w[0] == *b && w[1] == *a)),
        }
    }
}

/// Event-specific routing context for interpreting transitions.
#[derive(Debug, Clone)]
pub struct EventRoutingContext {
    pub transit_predicate: TransitPredicate,
}

/// Event-relative effects computed from a transition in context.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EventRelativeEffects {
    /// Path departed the required transit.
    pub transit_departed: bool,
    /// Path returned to the required transit.
    pub transit_returned: bool,
    /// Path retained the required transit (was and still is present).
    pub transit_retained: bool,
}

/// Interpret a transition through an event-specific routing context.
pub fn interpret_for_event(
    from_path: Option<&[u32]>,
    to_path: Option<&[u32]>,
    ctx: &EventRoutingContext,
) -> EventRelativeEffects {
    let from_has = from_path
        .map(|p| ctx.transit_predicate.evaluate(p))
        .unwrap_or(false);
    let to_has = to_path
        .map(|p| ctx.transit_predicate.evaluate(p))
        .unwrap_or(false);

    EventRelativeEffects {
        transit_departed: from_has && !to_has,
        transit_returned: !from_has && to_has,
        transit_retained: from_has && to_has,
    }
}

// ── Evidenced route state ──────────────────────────────────────────

/// A route state with provenance: the state itself plus the source
/// observation that established it.
///
/// `state = None` means explicit absence (withdrawal). The `evidence`
/// records the observation that caused or confirmed it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidencedRouteState {
    pub state: Option<RouteState>,
    pub evidence: EvidenceRef,
}

impl EvidencedRouteState {
    /// Present route with announcement/RIB evidence.
    pub fn present(state: RouteState, evidence: EvidenceRef) -> Self {
        EvidencedRouteState {
            state: Some(state),
            evidence,
        }
    }

    /// Explicit absence (withdrawal) with the withdrawal observation evidence.
    pub fn absent(evidence: EvidenceRef) -> Self {
        EvidencedRouteState {
            state: None,
            evidence,
        }
    }

    /// Access the prefix.
    /// Returns a sentinel prefix for absent states.
    pub fn prefix(&self) -> Prefix {
        self.state
            .as_ref()
            .map(|s| s.prefix.clone())
            .unwrap_or_else(|| Prefix::from("0.0.0.0/0"))
    }

    /// Access the timestamp.
    /// Falls back to evidence timestamp for absent states (withdrawals).
    pub fn timestamp(&self) -> chrono::DateTime<chrono::Utc> {
        self.state
            .as_ref()
            .map(|s| s.timestamp)
            .unwrap_or(self.evidence.timestamp)
    }

    /// Access the observer string.
    /// Returns "absent" for absent states (withdrawals).
    pub fn observer(&self) -> &str {
        self.state
            .as_ref()
            .map(|s| s.observer.as_str())
            .unwrap_or("absent")
    }

    /// Access route attributes.
    /// Returns an empty-attributes sentinel for absent states.
    pub fn attributes(&self) -> RouteAttributes {
        self.state
            .as_ref()
            .map(|s| s.attributes.clone())
            .unwrap_or(RouteAttributes::empty())
    }
}

// ── Reconstruction primitives ──────────────────────────────────────

/// Whether observation continuity is confirmed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Continuity {
    /// Continuity is known and uninterrupted.
    Known,
    /// Continuity cannot be confirmed (session boundary, archive gap).
    Unknown,
}

/// Analysis phase: warmup, event, or cooldown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AnalysisPhase {
    Warmup,
    Event,
    Cooldown,
}

/// A kind-less state change emitted by route reconstruction.
///
/// Carries independently evidenced baseline, before, and after states —
/// plus the triggering observation. Classification happens downstream
/// in tokenize::diff.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StateChange {
    pub key: RouteKey,
    pub event_baseline: Option<EvidencedRouteState>,
    pub before: Option<EvidencedRouteState>,
    pub after: EvidencedRouteState,
    pub triggering: EvidenceRef,
    pub continuity: Continuity,
    pub phase: AnalysisPhase,
}

impl StateChange {
    pub fn new(
        key: RouteKey,
        event_baseline: Option<EvidencedRouteState>,
        before: Option<EvidencedRouteState>,
        after: EvidencedRouteState,
        triggering: EvidenceRef,
        continuity: Continuity,
        phase: AnalysisPhase,
    ) -> Self {
        StateChange {
            key,
            event_baseline,
            before,
            after,
            triggering,
            continuity,
            phase,
        }
    }
}

/// A transition from one route state to another.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RouteTransition {
    pub key: RouteKey,
    /// The event-baseline state, if a baseline was frozen.
    pub event_baseline: Option<EvidencedRouteState>,
    /// The previous state.
    pub from: Option<EvidencedRouteState>,
    /// The new state.
    pub to: EvidencedRouteState,
    /// The kind of transition that occurred.
    pub kind: TransitionKind,
    /// Orthogonal effects that co-occurred with this transition.
    #[serde(default)]
    pub effects: GenericTransitionEffects,
    /// The triggering observation.
    pub triggering: EvidenceRef,
    pub phase: AnalysisPhase,
}

impl RouteTransition {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        key: RouteKey,
        event_baseline: Option<EvidencedRouteState>,
        from: Option<EvidencedRouteState>,
        to: EvidencedRouteState,
        triggering: EvidenceRef,
        kind: TransitionKind,
        effects: GenericTransitionEffects,
        phase: AnalysisPhase,
    ) -> Self {
        RouteTransition {
            key,
            event_baseline,
            from,
            to,
            kind,
            effects,
            triggering,
            phase,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn sample_time() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2025, 6, 15, 5, 25, 0).unwrap()
    }

    fn sample_state(prefix: &str, path: Vec<u32>, observer: &str) -> RouteState {
        RouteState {
            prefix: Prefix::from(prefix),
            attributes: RouteAttributes::from_as_path(path),
            timestamp: sample_time(),
            observer: observer.to_string(),
        }
    }

    fn synth_evidence() -> EvidenceRef {
        EvidenceRef::synthetic(0, "test", "0000000000000000")
    }

    fn synth_transition(
        from: Option<RouteState>,
        to: RouteState,
        kind: TransitionKind,
    ) -> RouteTransition {
        let key = RouteKey::new("test", "0.0.0.0".parse().unwrap(), &to.prefix);
        let evidence = synth_evidence();
        let from_ev = from.map(|s| EvidencedRouteState::present(s, evidence.clone()));
        let to_ev = EvidencedRouteState::present(to, evidence.clone());
        RouteTransition::new(
            key,
            None,
            from_ev,
            to_ev,
            evidence,
            kind,
            GenericTransitionEffects::default(),
            AnalysisPhase::Event,
        )
    }

    #[test]
    fn prefix_from_str() {
        let p = Prefix::from("192.0.2.0/24");
        assert_eq!(p.0, "192.0.2.0/24");
    }

    #[test]
    fn as_path_display() {
        let path = AsPath(vec![11537, 237, 1101]);
        assert_eq!(format!("{path}"), "11537 237 1101");
    }

    #[test]
    fn route_attributes_from_as_path() {
        let attrs = RouteAttributes::from_as_path(vec![11537, 237, 1101]);
        assert_eq!(attrs.origin_asns, vec![Asn(1101)]);
        assert_eq!(attrs.as_path.0.len(), 3);
    }

    #[test]
    fn route_attributes_empty_path() {
        let attrs = RouteAttributes::from_as_path(vec![]);
        assert!(attrs.origin_asns.is_empty());
    }

    #[test]
    fn transition_announcement() {
        let state = sample_state("192.0.2.0/24", vec![11537, 1101], "rv2:AS6447");
        let t = synth_transition(None, state, TransitionKind::Announcement);
        assert_eq!(t.kind, TransitionKind::Announcement);
        assert!(t.from.is_none());
    }

    #[test]
    fn transition_withdrawal() {
        let from = sample_state("192.0.2.0/24", vec![11537, 1101], "rv2:AS6447");
        let to = sample_state("192.0.2.0/24", vec![], "rv2:AS6447");
        let t = synth_transition(Some(from), to, TransitionKind::Withdrawal);
        assert_eq!(t.kind, TransitionKind::Withdrawal);
    }

    #[test]
    fn transition_path_change() {
        let from = sample_state("192.0.2.0/24", vec![11537, 1101], "rv2:AS6447");
        let to = sample_state("192.0.2.0/24", vec![11537, 237, 1101], "rv2:AS6447");
        let kind = TransitionKind::PathReplacement {
            old: from.attributes.as_path.clone(),
            new: to.attributes.as_path.clone(),
        };
        let t = synth_transition(Some(from), to, kind);
        assert!(matches!(t.kind, TransitionKind::PathReplacement { .. }));
    }

    #[test]
    fn transition_exact_duplicate() {
        let from = sample_state("192.0.2.0/24", vec![11537, 1101], "rv2:AS6447");
        let to = from.clone();
        let t = synth_transition(Some(from), to, TransitionKind::Duplicate);
        assert_eq!(t.kind, TransitionKind::Duplicate);
    }

    #[test]
    fn transition_restoration() {
        let from = sample_state("192.0.2.0/24", vec![11537, 237, 1101], "rv2:AS6447");
        let original = sample_state("192.0.2.0/24", vec![11537, 1101], "rv2:AS6447");
        let t = synth_transition(Some(from), original, TransitionKind::Restoration);
        assert_eq!(t.kind, TransitionKind::Restoration);
    }

    #[test]
    fn route_state_serialization_roundtrip() {
        let state = sample_state("192.0.2.0/24", vec![11537, 1101], "rv2:AS6447");
        let json = serde_json::to_string(&state).unwrap();
        let parsed: RouteState = serde_json::from_str(&json).unwrap();
        assert_eq!(state, parsed);
    }

    #[test]
    fn transition_serialization_roundtrip() {
        let from = sample_state("192.0.2.0/24", vec![11537, 1101], "rv2:AS6447");
        let to = sample_state("192.0.2.0/24", vec![11537, 237, 1101], "rv2:AS6447");
        let kind = TransitionKind::PathReplacement {
            old: from.attributes.as_path.clone(),
            new: to.attributes.as_path.clone(),
        };
        let t = synth_transition(Some(from), to, kind);
        let json = serde_json::to_string(&t).unwrap();
        let parsed: RouteTransition = serde_json::from_str(&json).unwrap();
        assert_eq!(t, parsed);
    }

    #[test]
    fn route_key_construction() {
        let key = RouteKey::new(
            "route-views2",
            "185.1.8.65".parse().unwrap(),
            &Prefix::from("192.0.2.0/24"),
        );
        assert_eq!(key.collector, "route-views2");
    }

    #[test]
    fn state_change_construction() {
        let state = sample_state("192.0.2.0/24", vec![11537, 1101], "rv2:AS6447");
        let key = RouteKey::new("rv2", "185.1.8.65".parse().unwrap(), &state.prefix);
        let ev = synth_evidence();
        let after = EvidencedRouteState::present(state, ev.clone());
        let sc = StateChange::new(
            key,
            None,
            None,
            after,
            ev,
            Continuity::Known,
            AnalysisPhase::Event,
        );
        assert_eq!(sc.continuity, Continuity::Known);
    }

    #[test]
    fn route_state_to_key() {
        let state = sample_state("192.0.2.0/24", vec![11537, 1101], "route-views2:185.1.8.65");
        let key = state.to_key();
        assert_eq!(key.collector, "route-views2");
    }
}
