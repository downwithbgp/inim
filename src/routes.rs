//! Route state reconstruction.
//!
//! Seeds initial state from RIB observations, applies UPDATE observations
//! in deterministic order, and emits kind-less `StateChange` values with
//! evidenced baseline/before/after states plus triggering observation.
//! Classification is performed in tokenize::diff.

use std::collections::HashMap;

use crate::domain::observation::{EvidenceRef, ObservationKind, RouteObservation};
use crate::domain::route::{
    AnalysisPhase, Continuity, EvidencedRouteState, RouteAttributes, RouteKey, RouteState,
    StateChange,
};

#[derive(Debug, Clone)]
pub struct RouteStateStore {
    states: HashMap<RouteKey, RouteState>,
    event_baseline: HashMap<RouteKey, EvidencedRouteState>,
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

    pub fn seed_from_rib(&mut self, obs: &RouteObservation) {
        let key = observation_key(obs);
        let state = observation_to_state(obs);
        self.states.insert(key, state);
    }

    pub fn apply_update(
        &mut self,
        obs: &RouteObservation,
        phase: AnalysisPhase,
    ) -> Option<StateChange> {
        let key = observation_key(obs);
        let evidence = EvidenceRef::from_observation(obs);

        match obs.kind {
            ObservationKind::RibEntry => {
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

                if let Some(ref old) = old_state {
                    if old.attributes == new_state.attributes {
                        return None;
                    }
                }

                self.states.insert(key.clone(), new_state.clone());

                let before = old_state.map(|s| EvidencedRouteState::present(s, evidence.clone()));
                let after = EvidencedRouteState::present(new_state, evidence.clone());

                let baseline = self.event_baseline.get(&key).cloned();

                Some(StateChange::new(
                    key, baseline, before, after, evidence, continuity, phase,
                ))
            }
            ObservationKind::Withdrawal => {
                let old_state = self.states.remove(&key);
                let continuity = self
                    .continuity
                    .get(&obs.collector.0)
                    .copied()
                    .unwrap_or(Continuity::Known);

                old_state.map(|from| {
                    let before = EvidencedRouteState::present(from, evidence.clone());
                    let after = EvidencedRouteState::absent(evidence.clone());
                    let baseline = self.event_baseline.get(&key).cloned();

                    StateChange::new(
                        key,
                        baseline,
                        Some(before),
                        after,
                        evidence,
                        continuity,
                        phase,
                    )
                })
            }
        }
    }

    pub fn freeze_event_baseline(&mut self) {
        // Create evidenced baseline snapshots from current state
        self.event_baseline.clear();
        for (key, state) in &self.states {
            // Use a synthetic evidence for the baseline freeze itself
            let evidence = EvidenceRef::synthetic(0, "baseline-freeze", "0000");
            self.event_baseline.insert(
                key.clone(),
                EvidencedRouteState::present(state.clone(), evidence),
            );
        }
    }

    pub fn current_state(&self, key: &RouteKey) -> Option<&RouteState> {
        self.states.get(key)
    }

    pub fn all_states(&self) -> impl Iterator<Item = (&RouteKey, &RouteState)> {
        self.states.iter()
    }

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

pub fn reconstruct_routes(
    observations: impl IntoIterator<Item = RouteObservation>,
    event_start: chrono::DateTime<chrono::Utc>,
    event_end: chrono::DateTime<chrono::Utc>,
    cooldown_end: chrono::DateTime<chrono::Utc>,
) -> (RouteStateStore, Vec<StateChange>) {
    let mut store = RouteStateStore::new();
    let mut changes: Vec<StateChange> = Vec::new();
    let mut baseline_frozen = false;

    for obs in observations {
        match obs.kind {
            ObservationKind::RibEntry => {
                store.seed_from_rib(&obs);
            }
            ObservationKind::SessionBoundary => {
                store.apply_update(&obs, AnalysisPhase::Event);
            }
            _ => {
                // Before event start: warm-up (silent, no transitions emitted)
                if obs.timestamp < event_start {
                    store.apply_update(&obs, AnalysisPhase::Warmup);
                    continue;
                }

                // Freeze baseline at first event-period observation
                if !baseline_frozen {
                    store.freeze_event_baseline();
                    baseline_frozen = true;
                }

                // After cooldown end: ignore
                if obs.timestamp > cooldown_end {
                    continue;
                }

                // Event or cooldown: emit transitions with correct phase
                let phase = if obs.timestamp <= event_end {
                    AnalysisPhase::Event
                } else {
                    AnalysisPhase::Cooldown
                };

                if let Some(change) = store.apply_update(&obs, phase) {
                    changes.push(change);
                }
            }
        }
    }

    (store, changes)
}

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
    use crate::domain::observation::{
        Asn, CollectorId, Communities, IngestRole, ObservationAttributes, ObservationId,
        ObservationProvenance, ObservationSource,
    };
    use crate::domain::route::Prefix;
    use chrono::{TimeZone, Utc};

    fn t(offset_secs: i64) -> chrono::DateTime<Utc> {
        Utc.with_ymd_and_hms(2025, 6, 15, 5, 25, 0).unwrap()
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
            path_id: None,
            provenance: ObservationProvenance::synthetic(IngestRole::Rib, 0),
        }
    }

    fn make_announce(
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
            path_id: None,
            provenance: ObservationProvenance::synthetic(IngestRole::Updates, seq),
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
            path_id: None,
            provenance: ObservationProvenance::synthetic(IngestRole::Updates, seq),
        }
    }

    #[test]
    fn rib_seed_establishes_baseline() {
        let mut store = RouteStateStore::new();
        let obs = make_rib_obs("192.0.2.0/24", "rv2", "185.1.8.65", vec![6447, 11537, 1101]);
        store.seed_from_rib(&obs);
        assert!(store.current_state(&observation_key(&obs)).is_some());
    }

    #[test]
    fn rib_seed_does_not_emit_transition() {
        let mut store = RouteStateStore::new();
        let obs = make_rib_obs("192.0.2.0/24", "rv2", "185.1.8.65", vec![6447, 11537, 1101]);
        assert!(store.apply_update(&obs, AnalysisPhase::Event).is_none());
    }

    #[test]
    fn announcement_changes_route_state() {
        let mut store = RouteStateStore::new();
        store.seed_from_rib(&make_rib_obs(
            "192.0.2.0/24",
            "rv2",
            "185.1.8.65",
            vec![6447, 11537, 1101],
        ));
        let ann = make_announce(
            "192.0.2.0/24",
            "rv2",
            "185.1.8.65",
            vec![6447, 237, 1101],
            0,
            1,
        );
        let sc = store.apply_update(&ann, AnalysisPhase::Event).unwrap();
        assert!(sc.before.is_some());
        assert_eq!(
            sc.after.state.as_ref().unwrap().attributes.as_path.0,
            vec![6447, 237, 1101]
        );
    }

    #[test]
    fn withdrawal_removes_route() {
        let mut store = RouteStateStore::new();
        store.seed_from_rib(&make_rib_obs(
            "192.0.2.0/24",
            "rv2",
            "185.1.8.65",
            vec![6447, 11537, 1101],
        ));
        let wd = make_withdrawal("192.0.2.0/24", "rv2", "185.1.8.65", 0, 1);
        let sc = store.apply_update(&wd, AnalysisPhase::Event).unwrap();
        assert!(sc.before.is_some());
        assert!(sc.after.state.is_none()); // explicit absence
    }

    #[test]
    fn exact_reannouncement_is_duplicate() {
        let mut store = RouteStateStore::new();
        store.seed_from_rib(&make_rib_obs(
            "192.0.2.0/24",
            "rv2",
            "185.1.8.65",
            vec![6447, 11537, 1101],
        ));
        let ann = make_announce(
            "192.0.2.0/24",
            "rv2",
            "185.1.8.65",
            vec![6447, 11537, 1101],
            0,
            1,
        );
        assert!(store.apply_update(&ann, AnalysisPhase::Event).is_none());
    }

    #[test]
    fn alternate_path_then_original_is_restoration() {
        let mut store = RouteStateStore::new();
        store.seed_from_rib(&make_rib_obs(
            "192.0.2.0/24",
            "rv2",
            "185.1.8.65",
            vec![6447, 11537, 1101],
        ));
        let alt = make_announce(
            "192.0.2.0/24",
            "rv2",
            "185.1.8.65",
            vec![6447, 237, 1101],
            0,
            1,
        );
        let c1 = store.apply_update(&alt, AnalysisPhase::Event).unwrap();
        assert_eq!(
            c1.after.state.as_ref().unwrap().attributes.as_path.0,
            vec![6447, 237, 1101]
        );
        let orig = make_announce(
            "192.0.2.0/24",
            "rv2",
            "185.1.8.65",
            vec![6447, 11537, 1101],
            10,
            2,
        );
        let c2 = store.apply_update(&orig, AnalysisPhase::Event).unwrap();
        assert_eq!(
            c2.after.state.as_ref().unwrap().attributes.as_path.0,
            vec![6447, 11537, 1101]
        );
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
            path_id: None,
            provenance: ObservationProvenance::synthetic(IngestRole::Updates, 0),
        };
        store.apply_update(&sb, AnalysisPhase::Event);
        assert_eq!(store.continuity.get("rv2"), Some(&Continuity::Unknown));
    }

    #[test]
    fn warm_up_updates_do_not_emit() {
        let obs = vec![
            make_rib_obs("192.0.2.0/24", "rv2", "185.1.8.65", vec![6447, 11537, 1101]),
            make_announce(
                "192.0.2.0/24",
                "rv2",
                "185.1.8.65",
                vec![6447, 237, 1101],
                -50,
                1,
            ),
        ];
        let (_, changes) = reconstruct_routes(obs, t(0), t(300), t(600));
        assert!(changes.is_empty());
    }

    #[test]
    fn event_period_emits_transitions() {
        let obs = vec![
            make_rib_obs("192.0.2.0/24", "rv2", "185.1.8.65", vec![6447, 11537, 1101]),
            make_announce(
                "192.0.2.0/24",
                "rv2",
                "185.1.8.65",
                vec![6447, 237, 1101],
                10,
                1,
            ),
        ];
        let (_, changes) = reconstruct_routes(obs, t(0), t(300), t(600));
        assert_eq!(changes.len(), 1);
    }

    #[test]
    fn freeze_baseline_preserves_state() {
        let mut store = RouteStateStore::new();
        store.seed_from_rib(&make_rib_obs(
            "192.0.2.0/24",
            "rv2",
            "185.1.8.65",
            vec![6447, 11537, 1101],
        ));
        store.freeze_event_baseline();
        assert!(!store.event_baseline.is_empty());
    }

    // ── Phase tests ──────────────────────────────────────────────

    #[test]
    fn warmup_change_updates_baseline_but_emits_no_event_transition() {
        let obs = vec![
            make_rib_obs("192.0.2.0/24", "rv2", "185.1.8.65", vec![6447, 11537, 1101]),
            make_announce(
                "192.0.2.0/24",
                "rv2",
                "185.1.8.65",
                vec![6447, 237, 1101],
                -50,
                1,
            ),
        ];
        let (store, changes) = reconstruct_routes(obs, t(0), t(300), t(600));
        // Warmup change must NOT appear as a transition
        assert!(changes.is_empty(), "warmup must emit no transitions");
        // But state must have been updated
        let key = observation_key(&make_announce(
            "192.0.2.0/24",
            "rv2",
            "185.1.8.65",
            vec![],
            0,
            0,
        ));
        assert!(store.current_state(&key).is_some());
    }

    #[test]
    fn event_change_is_classified_as_event_impact() {
        let obs = vec![
            make_rib_obs("192.0.2.0/24", "rv2", "185.1.8.65", vec![6447, 11537, 1101]),
            make_announce(
                "192.0.2.0/24",
                "rv2",
                "185.1.8.65",
                vec![6447, 237, 1101],
                10,
                1,
            ),
        ];
        let (_, changes) = reconstruct_routes(obs, t(0), t(300), t(600));
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].phase, AnalysisPhase::Event);
    }

    #[test]
    fn post_event_change_is_classified_as_cooldown() {
        let obs = vec![
            make_rib_obs("192.0.2.0/24", "rv2", "185.1.8.65", vec![6447, 11537, 1101]),
            make_announce(
                "192.0.2.0/24",
                "rv2",
                "185.1.8.65",
                vec![6447, 237, 1101],
                10,
                1,
            ), // event
            make_announce(
                "192.0.2.0/24",
                "rv2",
                "185.1.8.65",
                vec![6447, 11537, 1101],
                360,
                2,
            ), // after event_end=300, before cooldown_end=600
        ];
        let (_, changes) = reconstruct_routes(obs, t(0), t(300), t(600));
        assert_eq!(changes.len(), 2);
        assert_eq!(changes[0].phase, AnalysisPhase::Event);
        assert_eq!(changes[1].phase, AnalysisPhase::Cooldown);
    }

    #[test]
    fn cooldown_restoration_references_event_baseline() {
        let obs = vec![
            make_rib_obs("192.0.2.0/24", "rv2", "185.1.8.65", vec![6447, 11537, 1101]),
            make_announce(
                "192.0.2.0/24",
                "rv2",
                "185.1.8.65",
                vec![6447, 237, 1101],
                10,
                1,
            ), // event: path change
            make_announce(
                "192.0.2.0/24",
                "rv2",
                "185.1.8.65",
                vec![6447, 11537, 1101],
                360,
                2,
            ), // cooldown: restoration
        ];
        let (_, changes) = reconstruct_routes(obs, t(0), t(300), t(600));
        let baseline_change = &changes[0];
        let cooldown_change = &changes[1];
        // The baseline should be set on both changes
        assert!(baseline_change.event_baseline.is_some());
        assert!(cooldown_change.event_baseline.is_some());
    }

    #[test]
    fn cooldown_instability_is_not_reported_as_during_event() {
        let obs = vec![
            make_rib_obs("192.0.2.0/24", "rv2", "185.1.8.65", vec![6447, 11537, 1101]),
            make_announce(
                "192.0.2.0/24",
                "rv2",
                "185.1.8.65",
                vec![6447, 237, 1101],
                360,
                1,
            ), // cooldown
            make_announce(
                "192.0.2.0/24",
                "rv2",
                "185.1.8.65",
                vec![6447, 3356, 1101],
                380,
                2,
            ), // also cooldown
        ];
        let (_, changes) = reconstruct_routes(obs, t(0), t(300), t(600));
        // Both are in cooldown, none in event
        for c in &changes {
            assert_eq!(c.phase, AnalysisPhase::Cooldown);
        }
    }

    #[test]
    fn update_after_cooldown_end_is_ignored() {
        let obs = vec![
            make_rib_obs("192.0.2.0/24", "rv2", "185.1.8.65", vec![6447, 11537, 1101]),
            make_announce(
                "192.0.2.0/24",
                "rv2",
                "185.1.8.65",
                vec![6447, 237, 1101],
                700,
                1,
            ), // after cooldown_end=600
        ];
        let (_, changes) = reconstruct_routes(obs, t(0), t(300), t(600));
        assert!(changes.is_empty(), "update after cooldown must be ignored");
    }

    #[test]
    fn evidence_survives_all_three_phases() {
        let obs = vec![
            make_rib_obs("192.0.2.0/24", "rv2", "185.1.8.65", vec![6447, 11537, 1101]),
            make_announce(
                "192.0.2.0/24",
                "rv2",
                "185.1.8.65",
                vec![6447, 237, 1101],
                -50,
                1,
            ), // warmup
            make_announce(
                "192.0.2.0/24",
                "rv2",
                "185.1.8.65",
                vec![6447, 3356, 1101],
                10,
                2,
            ), // event
            make_announce(
                "192.0.2.0/24",
                "rv2",
                "185.1.8.65",
                vec![6447, 11537, 1101],
                360,
                3,
            ), // cooldown
        ];
        let (_, changes) = reconstruct_routes(obs, t(0), t(300), t(600));
        // Event and cooldown changes must have triggering evidence
        for c in &changes {
            assert!(c.triggering.observation_id.0 > 0, "evidence must survive");
        }
    }
}
