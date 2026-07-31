//! Target set — RIB preflight and route population freeze.
//!
//! Stage 1 of the real-event pipeline: scan the preceding RIB for
//! observer-prefix streams whose baseline path is relevant to the
//! event under study. Only relevant streams proceed to UPDATE ingestion.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::net::IpAddr;

use crate::domain::observation::{ObservationKind, RouteObservation};
use crate::domain::route::Prefix;

/// A frozen set of observer-prefix streams relevant to the event.
#[derive(Debug, Clone)]
#[derive(Default)]
pub struct TargetSet {
    /// Per-collector stream entries.
    pub streams: HashMap<String, Vec<TargetStream>>,
}

/// A single (peer_ip, prefix) stream identified as relevant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetStream {
    pub peer_ip: IpAddr,
    pub prefix: Prefix,
    /// The origin AS seen in the RIB.
    pub origin_as: u32,
    /// The full AS path seen in the RIB.
    pub as_path: Vec<u32>,
}

/// Scan RIB observations and freeze the target set.
///
/// Only observer-prefix streams matching ALL criteria are selected:
/// 1. Origin AS is in `target_origin_asns`
/// 2. Baseline AS path contains `required_transit_asn` (e.g. Internet2)
///
/// Collectors with no relevant streams receive an empty entry.
pub fn scan_rib_and_freeze(
    rib_observations: &[RouteObservation],
    target_origin_asns: &[u32],
    required_transit_asn: u32,
) -> TargetSet {
    let mut streams: HashMap<String, Vec<TargetStream>> = HashMap::new();

    for obs in rib_observations {
        if obs.kind != ObservationKind::RibEntry {
            continue;
        }

        let attrs = match &obs.attributes {
            Some(a) => a,
            None => continue,
        };

        // Check origin AS
        let origin = attrs.origin_asns.first().map(|a| a.0).unwrap_or(0);
        if !target_origin_asns.contains(&origin) {
            continue;
        }

        // Check transit AS in path
        if !attrs.as_path.contains(&required_transit_asn) {
            continue;
        }

        let collector = &obs.collector.0;
        streams.entry(collector.clone()).or_default().push(TargetStream {
            peer_ip: obs.peer_ip,
            prefix: obs.prefix.clone(),
            origin_as: origin,
            as_path: attrs.as_path.clone(),
        });
    }

    // Deduplicate per collector
    for entries in streams.values_mut() {
        entries.sort_by(|a, b| {
            a.peer_ip
                .to_string()
                .cmp(&b.peer_ip.to_string())
                .then_with(|| a.prefix.0.cmp(&b.prefix.0))
        });
        entries.dedup_by(|a, b| a.peer_ip == b.peer_ip && a.prefix == b.prefix);
    }

    TargetSet { streams }
}


impl TargetSet {
    /// Merge another target set into this one.
    pub fn merge(&mut self, other: &TargetSet) {
        for (collector, entries) in &other.streams {
            self.streams.entry(collector.clone()).or_default().extend(entries.iter().cloned());
        }
    }

    /// Check whether a collector has any relevant streams.
    pub fn has_relevant_streams(&self, collector: &str) -> bool {
        self.streams
            .get(collector)
            .map(|s| !s.is_empty())
            .unwrap_or(false)
    }

    /// Check whether a specific (peer_ip, prefix) is in the target set.
    pub fn contains(&self, collector: &str, peer_ip: IpAddr, prefix: &Prefix) -> bool {
        self.streams
            .get(collector)
            .map(|entries| {
                entries
                    .iter()
                    .any(|s| s.peer_ip == peer_ip && s.prefix == *prefix)
            })
            .unwrap_or(false)
    }

    /// Total number of streams across all collectors.
    pub fn total_streams(&self) -> usize {
        self.streams.values().map(|v| v.len()).sum()
    }

    /// Distinct collector count.
    pub fn collector_count(&self) -> usize {
        self.streams.len()
    }

    /// All frozen prefixes (for UPDATE filtering).
    pub fn frozen_prefixes(&self) -> HashSet<Prefix> {
        self.streams
            .values()
            .flat_map(|v| v.iter().map(|s| s.prefix.clone()))
            .collect()
    }

    /// Number of distinct peers across all streams.
    pub fn distinct_peers(&self) -> usize {
        let peers: HashSet<IpAddr> = self
            .streams
            .values()
            .flat_map(|v| v.iter().map(|s| s.peer_ip))
            .collect();
        peers.len()
    }
}

/// Admission function for UPDATE observations.
///
/// Applies the two-layer filter:
/// 1. Prefix must be in the frozen prefix set (fast rejection).
/// 2. The full (collector, peer_ip, prefix) key must be in the target set.
///
/// Withdrawals (no attributes) are admitted by key alone.
/// Origin/AS-path filtering is NEVER applied to updates — that's
/// a RIB-only predicate.
pub fn admit_observation(
    obs: &RouteObservation,
    target: &TargetSet,
    frozen_prefixes: &HashSet<Prefix>,
) -> bool {
    // Prefix filter (fast path)
    if !frozen_prefixes.contains(&obs.prefix) {
        return false;
    }
    // Full key filter
    target.contains(&obs.collector.0, obs.peer_ip, &obs.prefix)
}

/// Counts produced by a RIB preflight scan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreflightCounts {
    pub collectors_requested: usize,
    pub collectors_with_usable_ribs: usize,
    pub origin_matching_routes: usize,
    pub transit_matching_routes: usize,
    pub frozen_streams: usize,
    pub distinct_prefixes: usize,
    pub distinct_peers: usize,
}

impl PreflightCounts {
    pub fn from_target_set(
        target: &TargetSet,
        collectors_requested: usize,
        origin_matching_routes: usize,
        transit_matching_routes: usize,
    ) -> Self {
        PreflightCounts {
            collectors_requested,
            collectors_with_usable_ribs: target.collector_count(),
            origin_matching_routes,
            transit_matching_routes,
            frozen_streams: target.total_streams(),
            distinct_prefixes: target.frozen_prefixes().len(),
            distinct_peers: target.distinct_peers(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::observation::{
        Asn, CollectorId, Communities, ObservationAttributes, ObservationId,
        ObservationProvenance, ObservationSource,
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
    ) -> RouteObservation {
        let origin = *as_path.last().unwrap_or(&0);
        RouteObservation {
            id: ObservationId(0),
            source: ObservationSource::LocalFile("test.mrt".into()),
            timestamp: t(),
            collector: CollectorId(collector.into()),
            peer_ip: peer_ip.parse().unwrap(),
            peer_asn: Asn(6447),
            prefix: Prefix::from(prefix),
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
            provenance: ObservationProvenance {
                source_url: None,
                archive_sha256: None,
                input: "rib.mrt".into(),
                role: crate::domain::observation::IngestRole::Rib,
                parser_representation: "bgpkit-bgp-elem".into(),
                mrt_timestamp: 0.0,
                element_seq: 0,
            },
        }
    }

    fn make_update_obs(
        collector: &str,
        peer_ip: &str,
        prefix: &str,
        as_path: Option<Vec<u32>>,
    ) -> RouteObservation {
        let origin = as_path.as_ref().and_then(|p| p.last().copied());
        let kind = if as_path.is_some() {
            ObservationKind::Announcement
        } else {
            ObservationKind::Withdrawal
        };
        RouteObservation {
            id: ObservationId(0),
            source: ObservationSource::LocalFile("test.mrt".into()),
            timestamp: t(),
            collector: CollectorId(collector.into()),
            peer_ip: peer_ip.parse().unwrap(),
            peer_asn: Asn(6447),
            prefix: Prefix::from(prefix),
            kind,
            attributes: as_path.map(|ap| ObservationAttributes {
                as_path: ap.clone(),
                origin_asns: origin.map(|o| vec![Asn(o)]).unwrap_or_default(),
                next_hop: Some(peer_ip.parse().unwrap()),
                origin: Some("IGP".into()),
                local_pref: Some(100),
                med: None,
                atomic_aggregate: false,
                communities: Communities::new(),
            }),
            provenance: ObservationProvenance {
                source_url: None,
                archive_sha256: None,
                input: "test.mrt".into(),
                role: crate::domain::observation::IngestRole::Updates,
                parser_representation: "bgpkit-bgp-elem".into(),
                mrt_timestamp: 0.0,
                element_seq: 0,
            },
        }
    }

    fn make_target() -> TargetSet {
        let obs = vec![make_rib(
            "rv2",
            "185.1.8.65",
            "193.0.0.0/21",
            vec![6447, 11537, 3333],
        )];
        scan_rib_and_freeze(&obs, &[3333], 11537)
    }

    #[test]
    fn target_set_frozen_from_rib_scan() {
        let obs = vec![
            make_rib("rv2", "185.1.8.65", "193.0.0.0/21", vec![6447, 11537, 3333]),
            make_rib("rv2", "185.1.8.65", "193.0.8.0/21", vec![6447, 3356, 3333]),
            make_rib("rv2", "185.1.8.65", "192.0.2.0/24", vec![6447, 11537, 1101]),
            make_rib("rv6", "2001:7f8:4::1", "193.0.0.0/21", vec![6447, 11537, 3333]),
        ];
        let target = scan_rib_and_freeze(&obs, &[3333], 11537);
        assert_eq!(target.total_streams(), 2);
        assert!(target.has_relevant_streams("rv2"));
        assert!(target.contains("rv2", "185.1.8.65".parse().unwrap(), &Prefix::from("193.0.0.0/21")));
        assert!(!target.contains("rv2", "185.1.8.65".parse().unwrap(), &Prefix::from("193.0.8.0/21")));
        assert!(target.has_relevant_streams("rv6"));
        assert!(!target.has_relevant_streams("rrc00"));
    }

    #[test]
    fn rib_only_preflight_skips_collectors_without_relevant_streams() {
        let obs = vec![make_rib(
            "rv2", "185.1.8.65", "192.0.2.0/24", vec![6447, 3356, 1101],
        )];
        let target = scan_rib_and_freeze(&obs, &[3333], 11537);
        assert_eq!(target.total_streams(), 0);
        assert!(!target.has_relevant_streams("rv2"));
    }

    #[test]
    fn target_set_deduplicates_streams() {
        let obs = vec![
            make_rib("rv2", "185.1.8.65", "193.0.0.0/21", vec![6447, 11537, 3333]),
            make_rib("rv2", "185.1.8.65", "193.0.0.0/21", vec![6447, 11537, 3333]),
        ];
        let target = scan_rib_and_freeze(&obs, &[3333], 11537);
        assert_eq!(target.total_streams(), 1);
    }

    #[test]
    fn update_withdrawal_survives_prefix_filter() {
        let target = make_target();
        let frozen = target.frozen_prefixes();
        let obs = make_update_obs("rv2", "185.1.8.65", "193.0.0.0/21", None);
        assert!(admit_observation(&obs, &target, &frozen));
    }

    #[test]
    fn update_origin_change_survives_prefix_filter() {
        let target = make_target();
        let frozen = target.frozen_prefixes();
        let obs = make_update_obs("rv2", "185.1.8.65", "193.0.0.0/21",
            Some(vec![6447, 3356, 1101]));
        assert!(admit_observation(&obs, &target, &frozen));
    }

    #[test]
    fn unrelated_peer_for_target_prefix_is_rejected() {
        let target = make_target();
        let frozen = target.frozen_prefixes();
        let obs = make_update_obs("rv2", "192.168.1.1", "193.0.0.0/21",
            Some(vec![6447, 11537, 3333]));
        assert!(!admit_observation(&obs, &target, &frozen));
    }

    #[test]
    fn target_stream_key_includes_collector_peer_prefix() {
        let target = make_target();
        let frozen = target.frozen_prefixes();
        let obs = make_update_obs("rrc00", "185.1.8.65", "193.0.0.0/21", None);
        assert!(!admit_observation(&obs, &target, &frozen));
    }

    #[test]
    fn preflight_counts_are_computed() {
        let target = make_target();
        let counts = PreflightCounts::from_target_set(&target, 2, 3, 2);
        assert_eq!(counts.collectors_requested, 2);
        assert_eq!(counts.frozen_streams, 1);
        assert!(counts.distinct_prefixes > 0);
    }
}
