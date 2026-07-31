//! Fixture loader — loads synthetic route observations for testing
//! and demonstration purposes.

use chrono::{DateTime, Utc};
use std::net::IpAddr;

use crate::domain::observation::{
    Asn, CollectorId, Communities, EvidenceRef, IngestRole, ObservationAttributes, ObservationId,
    ObservationKind, ObservationProvenance, ObservationSource, RouteObservation,
};
use crate::domain::route::Prefix;

/// Build a synthetic RIB observation establishing baseline state.
pub fn make_synthetic_rib(
    prefix: &str,
    collector: &str,
    peer_ip: &str,
    peer_asn: u32,
    as_path: Vec<u32>,
    timestamp: DateTime<Utc>,
    id: u64,
) -> RouteObservation {
    RouteObservation {
        id: ObservationId(id),
        source: ObservationSource::LocalFile("synthetic".into()),
        timestamp,
        collector: CollectorId(collector.into()),
        peer_ip: peer_ip.parse::<IpAddr>().unwrap(),
        peer_asn: Asn(peer_asn),
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
        provenance: ObservationProvenance::synthetic(IngestRole::Rib, id),
    }
}

/// Build a synthetic announcement observation.
pub fn make_synthetic_announcement(
    prefix: &str,
    collector: &str,
    peer_ip: &str,
    peer_asn: u32,
    as_path: Vec<u32>,
    timestamp: DateTime<Utc>,
    id: u64,
) -> RouteObservation {
    RouteObservation {
        id: ObservationId(id),
        source: ObservationSource::LocalFile("synthetic".into()),
        timestamp,
        collector: CollectorId(collector.into()),
        peer_ip: peer_ip.parse::<IpAddr>().unwrap(),
        peer_asn: Asn(peer_asn),
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
        provenance: ObservationProvenance::synthetic(IngestRole::Updates, id),
    }
}

/// Build a synthetic withdrawal observation.
pub fn make_synthetic_withdrawal(
    prefix: &str,
    collector: &str,
    peer_ip: &str,
    peer_asn: u32,
    timestamp: DateTime<Utc>,
    id: u64,
) -> RouteObservation {
    RouteObservation {
        id: ObservationId(id),
        source: ObservationSource::LocalFile("synthetic".into()),
        timestamp,
        collector: CollectorId(collector.into()),
        peer_ip: peer_ip.parse::<IpAddr>().unwrap(),
        peer_asn: Asn(peer_asn),
        prefix: Prefix::from(prefix),
        kind: ObservationKind::Withdrawal,
        attributes: None,
        provenance: ObservationProvenance::synthetic(IngestRole::Updates, id),
    }
}

/// Build a synthetic evidence reference for testing.
pub fn synthetic_evidence(id: u64) -> EvidenceRef {
    EvidenceRef::synthetic(id, "synthetic://test", "0000000000000000")
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn t() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2025, 6, 15, 5, 25, 0).unwrap()
    }

    #[test]
    fn rib_observation_has_attributes() {
        let obs = make_synthetic_rib(
            "192.0.2.0/24",
            "rv2",
            "185.1.8.65",
            6447,
            vec![6447, 11537, 1101],
            t(),
            0,
        );
        assert_eq!(obs.kind, ObservationKind::RibEntry);
        assert!(obs.attributes.is_some());
    }

    #[test]
    fn announcement_has_correct_kind() {
        let obs = make_synthetic_announcement(
            "192.0.2.0/24",
            "rv2",
            "185.1.8.65",
            6447,
            vec![6447, 11537, 1101],
            t(),
            1,
        );
        assert_eq!(obs.kind, ObservationKind::Announcement);
    }

    #[test]
    fn withdrawal_has_no_attributes() {
        let obs = make_synthetic_withdrawal("192.0.2.0/24", "rv2", "185.1.8.65", 6447, t(), 2);
        assert_eq!(obs.kind, ObservationKind::Withdrawal);
        assert!(obs.attributes.is_none());
    }

    #[test]
    fn synthetic_evidence_has_url_and_sha() {
        let ev = synthetic_evidence(42);
        assert_eq!(ev.observation_id, ObservationId(42));
        assert_eq!(ev.source_url, Some("synthetic://test".into()));
    }
}
