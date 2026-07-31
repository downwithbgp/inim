//! Route state reconstruction.
//!
//! Seeds initial state from RIB observations, applies UPDATE observations
//! in deterministic (timestamp, sequence-number) order, and emits kind-less
//! `StateChange` values. Classification is performed in tokenize::diff —
//! this module does not classify.

use std::collections::HashMap;

use crate::domain::observation::{ObservationKind, RouteObservation};
use crate::domain::route::{
    Continuity, RouteAttributes, RouteKey, RouteState, StateChange,
};

// ── Reconstruction context ─────────────────────────────────────────

/// Tracks the state of all known routes across collectors and peers.
#[derive(Debug, Clone)]
pub struct RouteStateStore {
    states: HashMap<RouteKey, RouteState>,
    /// Baseline states frozen at event start, keyed by RouteKey.
    event_baseline: HashMap<RouteKey, RouteState>,
    /// Whether continuity is currently known for each collector.
    continuity: HashMap<String, Continuity>,
}

impl RouteStateStore {
    pub fn new() -> Self {
        RouteStateStore {
            states: HashMap::new(),
            event_baseline: HashMap::new(),
            continuity: HashMap::new(),
        }
    }

    /// Seed initial state from RIB observations. Emits NO transitions.
    ///
    /// RIB elements establish the starting state. They must not be
    /// counted as maintenance impact.
    pub fn seed_from_rib(&mut self, obs: &RouteObservation) {
        let key = observation_key(obs);
        let state = observation_to_state(obs);
        self.states.insert(key, state);
    }

    /// Apply a single update observation.
    ///
    /// Returns a StateChange if the observation causes a state transition,
    /// or None if it's a duplicate or baseline-establishing entry.
    ///
    /// Session boundaries set continuity to Unknown for the collector.
    pub fn apply_update(&mut self, obs: &RouteObservation) -> Option<StateChange> {
        let key = observation_key(obs);

        match obs.kind {
            ObservationKind::RibEntry => {
                // RIB entries during update phase are treated as state seeding.
                let state = observation_to_state(obs);
                self.states.insert(key.clone(), state);
                None
            }
            ObservationKind::SessionBoundary => {
                self.continuity
                    .insert(obs.collector.0.clone(), Continuity::Unknown);
                None
            }
            ObservationKind::Announcement => {
                let new_state = observation_to_state(obs);
                let old_state = self.states.get(&key).cloned();
                let continuity = self
                    .continuity
                    .get(&obs.collector.0)
                    .copied()
                    .unwrap_or(Continuity::Known);

                // Check for exact duplicate
                if let Some(ref old) = old_state {
                    if old.attributes == new_state.attributes {
                        return None;
                    }
                }

                self.states.insert(key.clone(), new_state.clone());
                Some(StateChange::new(key, old_state, new_state, continuity))
            }
            ObservationKind::Withdrawal => {
                let old_state = self.states.remove(&key);
                let continuity = self
                    .continuity
                    .get(&obs.collector.0)
                    .copied()
                    .unwrap_or(Continuity::Known);

                old_state.map(|from| {
                    let to = RouteState {
                        prefix: obs.prefix.clone(),
                        attributes: RouteAttributes::from_as_path(vec![]),
                        timestamp: obs.timestamp,
                        observer: format!("{}:{}", obs.collector.0, obs.peer_ip),
                    };
                    StateChange::new(key, Some(from), to, continuity)
                })
            }
        }
    }

    /// Freeze the current state as the event baseline.
    ///
    /// Called after warm-up period, before event-period observations.
    pub fn freeze_event_baseline(&mut self) {
        self.event_baseline = self.states.clone();
    }

    /// Get the frozen event baseline for a route key.
    pub fn event_baseline_state(&self, key: &RouteKey) -> Option<&RouteState> {
        self.event_baseline.get(key)
    }

    /// Get the current state for a route key.
    pub fn current_state(&self, key: &RouteKey) -> Option<&RouteState> {
        self.states.get(key)
    }

    /// All current states (for inspection).
    pub fn all_states(&self) -> impl Iterator<Item = (&RouteKey, &RouteState)> {
        self.states.iter()
    }

    /// Mark a collector's continuity as Unknown.
    pub fn mark_discontinuous(&mut self, collector: &str) {
        self.continuity
            .insert(collector.to_string(), Continuity::Unknown);
    }
}

impl Default for RouteStateStore {
    fn default() -> Self {
        Self::new()
    }
}

// ── Observation processing ─────────────────────────────────────────

/// Process a stream of observations through the state machine,
/// applying phased semantics: warm-up → event → cool-down.
///
/// Returns all StateChanges emitted during event and cool-down phases.
/// Warm-up updates are consumed but produce no transitions.
pub fn reconstruct_routes(
    observations: impl IntoIterator<Item = RouteObservation>,
    event_start: chrono::DateTime<chrono::Utc>,
    _event_end: chrono::DateTime<chrono::Utc>,
) -> (RouteStateStore, Vec<StateChange>) {
    let mut store = RouteStateStore::new();
    let mut changes: Vec<StateChange> = Vec::new();

    for obs in observations {
        match obs.kind {
            ObservationKind::RibEntry => {
                store.seed_from_rib(&obs);
            }
            ObservationKind::SessionBoundary => {
                store.apply_update(&obs);
            }
            _ => {
                // Before event start: warm-up (apply silently, no transitions)
                if obs.timestamp < event_start {
                    store.apply_update(&obs); // warm-up, discard StateChange
                    continue;
                }

                // At event start: freeze baseline
                if obs.timestamp >= event_start && changes.is_empty() {
                    store.freeze_event_baseline();
                }

                // Event period and cool-down: emit transitions
                if let Some(change) = store.apply_update(&obs) {
                    changes.push(change);
                }
            }
        }
    }

    (store, changes)
}

// ── Helpers ────────────────────────────────────────────────────────

fn observation_key(obs: &RouteObservation) -> RouteKey {
    RouteKey::new(&obs.collector.0, obs.peer_ip, &obs.prefix)
}

fn observation_to_state(obs: &RouteObservation) -> RouteState {
    let attributes = obs
        .attributes
        .clone()
        .map(|a| RouteAttributes {
            as_path: crate::domain::route::AsPath(a.as_path),
            origin_asns: a.origin_asns,
            next_hop: a.next_hop,
            origin: a.origin,
            med: a.med,
            local_pref: a.local_pref,
            atomic_aggregate: a.atomic_aggregate,
            communities: a.communities.values,
        })
        .unwrap_or_else(|| RouteAttributes::from_as_path(vec![]));

    RouteState {
        prefix: obs.prefix.clone(),
        attributes,
        timestamp: obs.timestamp,
        observer: format!("{}:{}", obs.collector.0, obs.peer_ip),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use crate::domain::observation::{
        Asn, CollectorId, Communities, IngestRole, ObservationAttributes, ObservationId,
        ObservationProvenance, ObservationSource,
    };
    use crate::domain::route::Prefix;

    fn t(offset_secs: i64) -> chrono::DateTime<Utc> {
        Utc.with_ymd_and_hms(2025, 6, 15, 5, 25, 0)
            .unwrap()
            + chrono::Duration::seconds(offset_secs)
    }

    fn make_rib_obs(
        prefix: &str,
        collector: &str,
        peer_ip: &str,
        as_path: Vec<u32>,
    ) -> RouteObservation {
        RouteObservation {
            id: ObservationId(0),
            source: ObservationSource::LocalFile("test.mrt".into()),
            timestamp: t(-100),
            collector: CollectorId(collector.into()),
            peer_ip: peer_ip.parse().unwrap(),
            peer_asn: Asn(6447),
            prefix: Prefix::from(prefix),
            kind: ObservationKind::RibEntry,
            attributes: Some(ObservationAttributes {
                as_path: as_path.clone(),
                origin_asns: as_path.last().map(|&a| vec![Asn(a)]).unwrap_or_default(),
                next_hop: Some(peer_ip.parse().unwrap()),
                origin: Some("IGP".into()),
                local_pref: Some(100),
                med: None,
                atomic_aggregate: false,
                communities: Communities::new(),
            }),
            provenance: ObservationProvenance {
                input: "test.mrt".into(),
                role: IngestRole::Rib,
                parser_representation: "bgpkit-bgp-elem".into(),
                mrt_timestamp: 0.0,
                element_seq: 0,
            },
        }
    }

    fn make_announcement(
        prefix: &str,
        collector: &str,
        peer_ip: &str,
        as_path: Vec<u32>,
        at_secs: i64,
        seq: u64,
    ) -> RouteObservation {
        RouteObservation {
            id: ObservationId(seq),
            source: ObservationSource::LocalFile("test.mrt".into()),
            timestamp: t(at_secs),
            collector: CollectorId(collector.into()),
            peer_ip: peer_ip.parse().unwrap(),
            peer_asn: Asn(6447),
            prefix: Prefix::from(prefix),
            kind: ObservationKind::Announcement,
            attributes: Some(ObservationAttributes {
                as_path: as_path.clone(),
                origin_asns: as_path.last().map(|&a| vec![Asn(a)]).unwrap_or_default(),
                next_hop: Some(peer_ip.parse().unwrap()),
                origin: Some("IGP".into()),
                local_pref: Some(100),
                med: None,
                atomic_aggregate: false,
                communities: Communities::new(),
            }),
            provenance: ObservationProvenance {
                input: "test.mrt".into(),
                role: IngestRole::Updates,
                parser_representation: "bgpkit-bgp-elem".into(),
                mrt_timestamp: 0.0,
                element_seq: seq,
            },
        }
    }

    fn make_withdrawal(
        prefix: &str,
        collector: &str,
        peer_ip: &str,
        at_secs: i64,
        seq: u64,
    ) -> RouteObservation {
        RouteObservation {
            id: ObservationId(seq),
            source: ObservationSource::LocalFile("test.mrt".into()),
            timestamp: t(at_secs),
            collector: CollectorId(collector.into()),
            peer_ip: peer_ip.parse().unwrap(),
            peer_asn: Asn(6447),
            prefix: Prefix::from(prefix),
            kind: ObservationKind::Withdrawal,
            attributes: None,
            provenance: ObservationProvenance {
                input: "test.mrt".into(),
                role: IngestRole::Updates,
                parser_representation: "bgpkit-bgp-elem".into(),
                mrt_timestamp: 0.0,
                element_seq: seq,
            },
        }
    }

    // ── RIB seeding tests ─────────────────────────────────────

    #[test]
    fn rib_seed_establishes_baseline() {
        let mut store = RouteStateStore::new();
        let obs = make_rib_obs("192.0.2.0/24", "rv2", "185.1.8.65", vec![6447, 11537, 1101]);
        store.seed_from_rib(&obs);
        let key = observation_key(&obs);
        assert!(store.current_state(&key).is_some());
        assert_eq!(
            store.current_state(&key).unwrap().attributes.as_path.0,
            vec![6447, 11537, 1101]
        );
    }

    #[test]
    fn rib_seed_does_not_emit_transition() {
        let mut store = RouteStateStore::new();
        let obs = make_rib_obs("192.0.2.0/24", "rv2", "185.1.8.65", vec![6447, 11537, 1101]);
        // apply_update on RIB returns None
        let result = store.apply_update(&obs);
        assert!(result.is_none());
    }

    // ── Update tests ──────────────────────────────────────────

    #[test]
    fn announcement_changes_route_state() {
        let mut store = RouteStateStore::new();
        let rib = make_rib_obs("192.0.2.0/24", "rv2", "185.1.8.65", vec![6447, 11537, 1101]);
        store.seed_from_rib(&rib);

        let ann = make_announcement(
            "192.0.2.0/24", "rv2", "185.1.8.65",
            vec![6447, 237, 1101], // different path
            0, 1,
        );
        let change = store.apply_update(&ann);
        assert!(change.is_some());
        let sc = change.unwrap();
        assert!(sc.from.is_some());
        assert_eq!(sc.to.attributes.as_path.0, vec![6447, 237, 1101]);
    }

    #[test]
    fn withdrawal_removes_route() {
        let mut store = RouteStateStore::new();
        let rib = make_rib_obs("192.0.2.0/24", "rv2", "185.1.8.65", vec![6447, 11537, 1101]);
        store.seed_from_rib(&rib);

        let wd = make_withdrawal("192.0.2.0/24", "rv2", "185.1.8.65", 0, 1);
        let change = store.apply_update(&wd);
        assert!(change.is_some());
        let sc = change.unwrap();
        assert!(sc.from.is_some());

        // Route should be removed from store
        let key = observation_key(&wd);
        assert!(store.current_state(&key).is_none());
    }

    #[test]
    fn exact_reannouncement_is_duplicate() {
        let mut store = RouteStateStore::new();
        let rib = make_rib_obs("192.0.2.0/24", "rv2", "185.1.8.65", vec![6447, 11537, 1101]);
        store.seed_from_rib(&rib);

        // Re-announce with identical path
        let ann = make_announcement(
            "192.0.2.0/24", "rv2", "185.1.8.65",
            vec![6447, 11537, 1101],
            0, 1,
        );
        let change = store.apply_update(&ann);
        assert!(change.is_none(), "exact duplicate should not emit change");
    }

    #[test]
    fn alternate_path_then_original_is_restoration() {
        let mut store = RouteStateStore::new();
        let rib = make_rib_obs("192.0.2.0/24", "rv2", "185.1.8.65", vec![6447, 11537, 1101]);
        store.seed_from_rib(&rib);

        // Move to alternate
        let alt = make_announcement(
            "192.0.2.0/24", "rv2", "185.1.8.65",
            vec![6447, 237, 1101],
            0, 1,
        );
        let c1 = store.apply_update(&alt).unwrap();
        assert_eq!(c1.to.attributes.as_path.0, vec![6447, 237, 1101]);

        // Return to original
        let orig = make_announcement(
            "192.0.2.0/24", "rv2", "185.1.8.65",
            vec![6447, 11537, 1101],
            10, 2,
        );
        let c2 = store.apply_update(&orig).unwrap();
        assert_eq!(c2.to.attributes.as_path.0, vec![6447, 11537, 1101]);
    }

    #[test]
    fn session_boundary_breaks_continuity() {
        let mut store = RouteStateStore::new();

        let sb = RouteObservation {
            id: ObservationId(0),
            source: ObservationSource::LocalFile("test.mrt".into()),
            timestamp: t(0),
            collector: CollectorId("rv2".into()),
            peer_ip: "185.1.8.65".parse().unwrap(),
            peer_asn: Asn(6447),
            prefix: Prefix::from("0.0.0.0/0"),
            kind: ObservationKind::SessionBoundary,
            attributes: None,
            provenance: ObservationProvenance {
                input: "test.mrt".into(),
                role: IngestRole::Updates,
                parser_representation: "bgpkit-bgp-elem".into(),
                mrt_timestamp: 0.0,
                element_seq: 0,
            },
        };
        store.apply_update(&sb);
        assert_eq!(store.continuity.get("rv2"), Some(&Continuity::Unknown));
    }

    // ── Phased reconstruction tests ───────────────────────────

    #[test]
    fn warm_up_updates_do_not_emit() {
        let obs = vec![
            make_rib_obs("192.0.2.0/24", "rv2", "185.1.8.65", vec![6447, 11537, 1101]),
            make_announcement(
                "192.0.2.0/24", "rv2", "185.1.8.65",
                vec![6447, 237, 1101],
                -50, // before event start (t=0)
                1,
            ),
        ];

        let (_store, changes) = reconstruct_routes(obs, t(0), t(300));
        assert!(changes.is_empty(), "warm-up should not emit transitions");
    }

    #[test]
    fn event_period_emits_transitions() {
        let obs = vec![
            make_rib_obs("192.0.2.0/24", "rv2", "185.1.8.65", vec![6447, 11537, 1101]),
            make_announcement(
                "192.0.2.0/24", "rv2", "185.1.8.65",
                vec![6447, 237, 1101],
                10, // after event start (t=0)
                1,
            ),
        ];

        let (_store, changes) = reconstruct_routes(obs, t(0), t(300));
        assert_eq!(changes.len(), 1);
    }

    #[test]
    fn freeze_baseline_preserves_state() {
        let mut store = RouteStateStore::new();
        let rib = make_rib_obs("192.0.2.0/24", "rv2", "185.1.8.65", vec![6447, 11537, 1101]);
        store.seed_from_rib(&rib);
        store.freeze_event_baseline();

        let key = observation_key(&rib);
        let baseline = store.event_baseline_state(&key);
        assert!(baseline.is_some());
        assert_eq!(
            baseline.unwrap().attributes.as_path.0,
            vec![6447, 11537, 1101]
        );
    }
}
