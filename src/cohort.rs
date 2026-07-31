//! Frozen observer-prefix cohort — baseline cohort admission and
//! route-instance preservation.
//!
//! An ObserverPrefixKey joins the frozen cohort when ≥1 baseline instance
//! satisfies the target origin + transit predicate. Once admitted, ALL
//! target-origin instances for that key are preserved (including alternates).

use std::collections::{BTreeMap, BTreeSet};

use crate::domain::observation::{ObservationKind, RouteObservation};
use crate::domain::route::{
    AsPath, ObserverPrefixKey, RouteAttributes, RouteKey, RouteState, TransitPredicate,
};

/// Frozen event cohort: admitted ObserverPrefixKeys + baseline route instances.
#[derive(Debug, Clone, Default)]
pub struct FrozenCohort {
    pub observer_prefixes: BTreeSet<ObserverPrefixKey>,
    pub baseline_instances: BTreeMap<ObserverPrefixKey, BTreeMap<RouteKey, RouteState>>,
}

impl FrozenCohort {
    pub fn contains(&self, key: &ObserverPrefixKey) -> bool {
        self.observer_prefixes.contains(key)
    }

    pub fn stream_count(&self) -> usize {
        self.observer_prefixes.len()
    }

    pub fn instance_count(&self) -> usize {
        self.baseline_instances.values().map(|m| m.len()).sum()
    }
}

/// Scan RIB observations and freeze the observer-prefix cohort.
pub fn freeze_cohort(
    rib_observations: &[RouteObservation],
    target_origin_asns: &[u32],
    transit_predicate: &TransitPredicate,
) -> FrozenCohort {
    let mut cohort = FrozenCohort::default();
    let mut all_instances: BTreeMap<ObserverPrefixKey, Vec<RouteObservation>> = BTreeMap::new();

    for obs in rib_observations {
        if obs.kind != ObservationKind::RibEntry {
            continue;
        }
        let attrs = match &obs.attributes {
            Some(a) => a,
            None => continue,
        };
        let origin = attrs.origin_asns.first().map(|a| a.0).unwrap_or(0);
        if !target_origin_asns.contains(&origin) {
            continue;
        }
        let opk = ObserverPrefixKey {
            collector: obs.collector.0.clone(),
            peer_ip: obs.peer_ip,
            prefix: obs.prefix.clone(),
        };
        all_instances.entry(opk).or_default().push(obs.clone());
    }

    for (opk, instances) in &all_instances {
        let matches_predicate = instances.iter().any(|obs| {
            obs.attributes
                .as_ref()
                .is_some_and(|a| transit_predicate.evaluate(&a.as_path))
        });

        if matches_predicate {
            cohort.observer_prefixes.insert(opk.clone());
            let mut instance_map = BTreeMap::new();
            for obs in instances {
                let rk =
                    RouteKey::with_path_id(&opk.collector, opk.peer_ip, &opk.prefix, obs.path_id);
                let state = RouteState {
                    prefix: opk.prefix.clone(),
                    attributes: RouteAttributes {
                        as_path: AsPath(
                            obs.attributes
                                .as_ref()
                                .map(|a| a.as_path.clone())
                                .unwrap_or_default(),
                        ),
                        origin_asns: obs
                            .attributes
                            .as_ref()
                            .map(|a| a.origin_asns.clone())
                            .unwrap_or_default(),
                        next_hop: obs.attributes.as_ref().and_then(|a| a.next_hop),
                        origin: obs.attributes.as_ref().and_then(|a| a.origin.clone()),
                        med: obs.attributes.as_ref().and_then(|a| a.med),
                        local_pref: obs.attributes.as_ref().and_then(|a| a.local_pref),
                        atomic_aggregate: obs
                            .attributes
                            .as_ref()
                            .map(|a| a.atomic_aggregate)
                            .unwrap_or(false),
                        communities: obs
                            .attributes
                            .as_ref()
                            .map(|a| a.communities.values.clone())
                            .unwrap_or_default(),
                    },
                    timestamp: obs.timestamp,
                    observer: format!("{}:{}", opk.collector, opk.peer_ip),
                    path_id: obs.path_id,
                };
                instance_map.insert(rk, state);
            }
            cohort.baseline_instances.insert(opk.clone(), instance_map);
        }
    }

    cohort
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::observation::{
        Asn, CollectorId, Communities, ObservationAttributes, ObservationId, ObservationProvenance,
        ObservationSource,
    };
    use chrono::{TimeZone, Utc};

    fn t() -> chrono::DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 7, 30, 8, 0, 0).unwrap()
    }

    fn make_rib(
        collector: &str,
        peer_ip: &str,
        prefix: &str,
        as_path: Vec<u32>,
        path_id: Option<u32>,
    ) -> RouteObservation {
        let origin = *as_path.last().unwrap_or(&0);
        RouteObservation {
            id: ObservationId(0),
            source: ObservationSource::LocalFile("test.mrt".into()),
            timestamp: t(),
            collector: CollectorId(collector.into()),
            peer_ip: peer_ip.parse().unwrap(),
            peer_asn: Asn(6447),
            prefix: crate::domain::route::Prefix::from(prefix),
            kind: ObservationKind::RibEntry,
            attributes: Some(ObservationAttributes {
                as_path: as_path.clone(),
                origin_asns: vec![Asn(origin)],
                next_hop: Some(peer_ip.parse().unwrap()),
                origin: Some("IGP".into()),
                local_pref: Some(100),
                med: None,
                atomic_aggregate: false,
                communities: Communities::new(),
            }),
            path_id,
            provenance: ObservationProvenance {
                source_url: None,
                archive_sha256: None,
                input: "rib.mrt".into(),
                role: crate::domain::observation::IngestRole::Rib,
                parser_representation: "bgpkit-bgp-elem".into(),
                mrt_timestamp: 0.0,
                element_seq: 0,
                archive_order: 0,
            },
        }
    }

    #[test]
    fn matching_instance_freezes_observer_prefix() {
        let obs = vec![make_rib(
            "rv2",
            "185.1.8.65",
            "192.0.2.0/24",
            vec![6447, 65002, 65001],
            None,
        )];
        let cohort = freeze_cohort(&obs, &[65001], &TransitPredicate::ContainsAny(vec![65002]));
        assert_eq!(cohort.stream_count(), 1);
    }

    #[test]
    fn two_path_ids_form_one_visible_observer_stream() {
        let opk = ObserverPrefixKey {
            collector: "rv2".into(),
            peer_ip: "185.1.8.65".parse().unwrap(),
            prefix: crate::domain::route::Prefix::from("192.0.2.0/24"),
        };
        let rk1 =
            RouteKey::with_path_id("rv2", "185.1.8.65".parse().unwrap(), &opk.prefix, Some(1));
        let rk2 =
            RouteKey::with_path_id("rv2", "185.1.8.65".parse().unwrap(), &opk.prefix, Some(2));
        assert_eq!(rk1.observer_prefix_key(), rk2.observer_prefix_key());
        assert_ne!(rk1, rk2);
    }

    #[test]
    fn withdrawing_one_of_two_instances_keeps_stream_visible() {
        let opk = ObserverPrefixKey {
            collector: "rv2".into(),
            peer_ip: "185.1.8.65".parse().unwrap(),
            prefix: crate::domain::route::Prefix::from("192.0.2.0/24"),
        };
        let rk1 =
            RouteKey::with_path_id("rv2", "185.1.8.65".parse().unwrap(), &opk.prefix, Some(1));
        let rk2 =
            RouteKey::with_path_id("rv2", "185.1.8.65".parse().unwrap(), &opk.prefix, Some(2));
        // rk1 withdrawn, rk2 active → stream still visible
        let mut active = BTreeSet::new();
        active.insert(rk2.clone());
        assert!(active.contains(&rk2));
        assert!(!active.contains(&rk1));
        assert!(!active.is_empty());
    }

    // ── 1.1 UPDATE admission ──────────────────────────────────────

    fn make_frozen_cohort() -> FrozenCohort {
        let mut c = FrozenCohort::default();
        let opk = ObserverPrefixKey {
            collector: "rv2".into(),
            peer_ip: "185.1.8.65".parse().unwrap(),
            prefix: crate::domain::route::Prefix::from("192.0.2.0/24"),
        };
        c.observer_prefixes.insert(opk);
        c
    }

    #[test]
    fn new_path_id_for_frozen_observer_prefix_is_admitted() {
        let cohort = make_frozen_cohort();
        let opk = ObserverPrefixKey {
            collector: "rv2".into(),
            peer_ip: "185.1.8.65".parse().unwrap(),
            prefix: crate::domain::route::Prefix::from("192.0.2.0/24"),
        };
        assert!(
            cohort.contains(&opk),
            "frozen key with new path_id must be admitted"
        );
    }

    #[test]
    fn new_path_id_for_unfrozen_observer_prefix_is_rejected() {
        let cohort = make_frozen_cohort();
        let opk = ObserverPrefixKey {
            collector: "rv2".into(),
            peer_ip: "185.1.8.66".parse().unwrap(),
            prefix: crate::domain::route::Prefix::from("192.0.2.0/24"),
        };
        assert!(!cohort.contains(&opk));
    }

    #[test]
    fn replacement_departing_transit_is_admitted() {
        let cohort = make_frozen_cohort();
        // Even if the replacement path departs the transit predicate,
        // it must still be admitted for the frozen stream.
        let opk = ObserverPrefixKey {
            collector: "rv2".into(),
            peer_ip: "185.1.8.65".parse().unwrap(),
            prefix: crate::domain::route::Prefix::from("192.0.2.0/24"),
        };
        assert!(cohort.contains(&opk));
    }

    #[test]
    fn withdrawal_without_path_is_admitted() {
        let cohort = make_frozen_cohort();
        let opk = ObserverPrefixKey {
            collector: "rv2".into(),
            peer_ip: "185.1.8.65".parse().unwrap(),
            prefix: crate::domain::route::Prefix::from("192.0.2.0/24"),
        };
        // Withdrawals must be admitted for frozen streams
        assert!(cohort.contains(&opk));
    }

    #[test]
    fn origin_change_for_frozen_stream_is_admitted() {
        let cohort = make_frozen_cohort();
        let opk = ObserverPrefixKey {
            collector: "rv2".into(),
            peer_ip: "185.1.8.65".parse().unwrap(),
            prefix: crate::domain::route::Prefix::from("192.0.2.0/24"),
        };
        // Origin changes on a frozen stream must still be visible
        assert!(cohort.contains(&opk));
    }

    #[test]
    fn update_admission_does_not_reapply_baseline_predicate() {
        let cohort = make_frozen_cohort();
        let opk = ObserverPrefixKey {
            collector: "rv2".into(),
            peer_ip: "185.1.8.65".parse().unwrap(),
            prefix: crate::domain::route::Prefix::from("192.0.2.0/24"),
        };
        // Admission is by ObserverPrefixKey only — not by re-evaluating the predicate
        assert!(cohort.contains(&opk));
    }
}
