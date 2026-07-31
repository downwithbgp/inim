//! Route observation types — the normalized BGP observation model.
//!
//! These are inim-native types. bgpkit-parser types are converted to these
//! at the ingestion boundary and never leak into the rest of the codebase.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::net::IpAddr;

use super::route::Prefix;

// ── Fundamental types ──────────────────────────────────────────────

/// An Autonomous System number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Asn(pub u32);

impl std::fmt::Display for Asn {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A collector identifier (e.g. "route-views2").
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CollectorId(pub String);

impl std::fmt::Display for CollectorId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Community values carried with a route.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Communities {
    pub values: Vec<String>,
}

impl Communities {
    pub fn new() -> Self {
        Communities { values: vec![] }
    }

    pub fn from_strings(v: Vec<String>) -> Self {
        Communities { values: v }
    }
}

// ── Source and role tracking ───────────────────────────────────────

/// How the observation was sourced.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ObservationSource {
    /// A local file path.
    LocalFile(String),
}

/// The role of the input in the analysis pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum IngestRole {
    /// Input provides initial routing state (RIB).
    Rib,
    /// Input provides routing updates.
    Updates,
}

// ── Observation kind ───────────────────────────────────────────────

/// What kind of BGP observation this is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ObservationKind {
    /// A RIB entry establishing initial state.
    RibEntry,
    /// A BGP announcement.
    Announcement,
    /// A BGP withdrawal.
    Withdrawal,
    /// A session state change (peer up/down).
    SessionBoundary,
}

// ── Route observation ──────────────────────────────────────────────

/// The attributes of a route as observed in an announcement or RIB entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObservationAttributes {
    pub as_path: Vec<u32>,
    pub origin_asns: Vec<Asn>,
    pub next_hop: Option<IpAddr>,
    pub origin: Option<String>,
    pub local_pref: Option<u32>,
    pub med: Option<u32>,
    pub atomic_aggregate: bool,
    pub communities: Communities,
}

/// A single normalized route observation from any source.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RouteObservation {
    /// Unique identifier for this observation.
    pub id: ObservationId,
    /// Where the observation came from.
    pub source: ObservationSource,
    /// UTC timestamp of the observation.
    pub timestamp: DateTime<Utc>,
    /// Collector identifier.
    pub collector: CollectorId,
    /// Peer IP address.
    pub peer_ip: IpAddr,
    /// Peer ASN.
    pub peer_asn: Asn,
    /// The BGP prefix.
    pub prefix: Prefix,
    /// What kind of observation.
    pub kind: ObservationKind,
    /// Route attributes (None for withdrawals and session boundaries).
    pub attributes: Option<ObservationAttributes>,
    /// Provenance for audit trail.
    pub provenance: ObservationProvenance,
}

// ── Observation identity ───────────────────────────────────────────

/// Unique identifier for a route observation within the analysis run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ObservationId(pub u64);

impl std::fmt::Display for ObservationId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "#{}", self.0)
    }
}

// ── Provenance ─────────────────────────────────────────────────────

/// Provenance metadata for audit trail and reproducibility.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ObservationProvenance {
    /// Input file path or URL.
    pub input: String,
    /// Role of the input (RIB or updates).
    pub role: IngestRole,
    /// Parser representation used (always "bgpkit-bgp-elem" in MVP).
    pub parser_representation: String,
    /// MRT record timestamp (as f64 epoch, before conversion).
    pub mrt_timestamp: f64,
    /// Deterministic element sequence number within the input.
    pub element_seq: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn sample_time() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2025, 6, 15, 5, 25, 0).unwrap()
    }

    fn sample_obs(kind: ObservationKind) -> RouteObservation {
        RouteObservation {
            id: ObservationId(1),
            source: ObservationSource::LocalFile("test.mrt".into()),
            timestamp: sample_time(),
            collector: CollectorId("route-views2".into()),
            peer_ip: "185.1.8.65".parse().unwrap(),
            peer_asn: Asn(6447),
            prefix: Prefix::from("192.0.2.0/24"),
            kind,
            attributes: Some(ObservationAttributes {
                as_path: vec![6447, 11537, 1101],
                origin_asns: vec![Asn(1101)],
                next_hop: Some("185.1.8.65".parse().unwrap()),
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
                mrt_timestamp: 1749990300.0,
                element_seq: 42,
            },
        }
    }

    #[test]
    fn asn_display() {
        assert_eq!(format!("{}", Asn(6447)), "6447");
    }

    #[test]
    fn collector_id_display() {
        let c = CollectorId("route-views2".into());
        assert_eq!(format!("{c}"), "route-views2");
    }

    #[test]
    fn communities_default_is_empty() {
        let c = Communities::default();
        assert!(c.values.is_empty());
    }

    #[test]
    fn communities_from_strings() {
        let c = Communities::from_strings(vec!["11537:1000".into(), "6447:666".into()]);
        assert_eq!(c.values.len(), 2);
    }

    #[test]
    fn observation_announcement_has_attributes() {
        let obs = sample_obs(ObservationKind::Announcement);
        assert!(obs.attributes.is_some());
        assert_eq!(obs.attributes.unwrap().as_path.len(), 3);
    }

    #[test]
    fn observation_withdrawal_no_attributes() {
        let mut obs = sample_obs(ObservationKind::Withdrawal);
        obs.attributes = None;
        assert!(obs.attributes.is_none());
    }

    #[test]
    fn observation_rib_entry() {
        let obs = sample_obs(ObservationKind::RibEntry);
        assert_eq!(obs.kind, ObservationKind::RibEntry);
    }

    #[test]
    fn observation_session_boundary() {
        let mut obs = sample_obs(ObservationKind::SessionBoundary);
        obs.attributes = None;
        assert_eq!(obs.kind, ObservationKind::SessionBoundary);
    }

    #[test]
    fn observation_id_display() {
        assert_eq!(format!("{}", ObservationId(42)), "#42");
    }

    #[test]
    fn observation_serialization_roundtrip() {
        let obs = sample_obs(ObservationKind::Announcement);
        let json = serde_json::to_string(&obs).unwrap();
        let parsed: RouteObservation = serde_json::from_str(&json).unwrap();
        assert_eq!(obs, parsed);
    }

    #[test]
    fn provenance_roundtrip() {
        let p = ObservationProvenance {
            input: "rib.mrt.bz2".into(),
            role: IngestRole::Rib,
            parser_representation: "bgpkit-bgp-elem".into(),
            mrt_timestamp: 1749990000.0,
            element_seq: 0,
        };
        let json = serde_json::to_string(&p).unwrap();
        let parsed: ObservationProvenance = serde_json::from_str(&json).unwrap();
        assert_eq!(p, parsed);
    }

    #[test]
    fn ingest_role_serialization() {
        assert_eq!(
            serde_json::to_string(&IngestRole::Rib).unwrap(),
            "\"Rib\""
        );
        assert_eq!(
            serde_json::to_string(&IngestRole::Updates).unwrap(),
            "\"Updates\""
        );
    }
}
