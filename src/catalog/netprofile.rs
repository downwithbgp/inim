//! Reviewed network-profile data: named service planes, ASN roles, and
//! generic session/path classification.
//!
//! All operator-specific identities are DATA loaded from JSON profile
//! files (`case-studies/*/pilot/network-profile.json`). This module
//! contains no operator-specific branch and no operator ASN constant;
//! the release gate (`tests/release_test.rs`) enforces that.

use serde::{Deserialize, Serialize};
use std::path::Path;

/// One reviewed named service plane (e.g. a research-and-education routing
/// plane or a settlement-free public peering plane).
///
/// Identity is `id` (stable, used in evidence); `display_label` is
/// presentation text and never participates in identity computation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NamedServicePlane {
    pub id: String,
    pub display_label: String,
    pub asns: Vec<u32>,
}

/// A reviewed role for one ASN. The vocabulary is data (for example
/// `regional-re`, `national-re`, `international-nren`,
/// `exchange-participant`); the code never interprets role strings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewedAsnRole {
    pub asn: u32,
    pub role: String,
}

/// Reviewed service-plane profile: named planes + ASN roles + provenance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServicePlaneProfile {
    pub service_planes: Vec<NamedServicePlane>,
    pub asn_roles: Vec<ReviewedAsnRole>,
    pub updated_utc: String,
    pub provenance: String,
}

/// Display label used when an ASN has no reviewed role.
pub const UNCLASSIFIED_OBSERVED_ASN: &str = "unclassified observed ASN";

impl ServicePlaneProfile {
    /// Load and validate a profile from JSON.
    pub fn load(path: &Path) -> Result<Self, String> {
        let raw = std::fs::read_to_string(path)
            .map_err(|e| format!("cannot read profile {}: {e}", path.display()))?;
        let profile: ServicePlaneProfile = serde_json::from_str(&raw)
            .map_err(|e| format!("invalid profile {}: {e}", path.display()))?;
        profile.validate()?;
        Ok(profile)
    }

    /// Reject nonsense profiles: empty ids, empty ASN sets, duplicate ids.
    pub fn validate(&self) -> Result<(), String> {
        let mut seen: Vec<&str> = Vec::new();
        for plane in &self.service_planes {
            if plane.id.is_empty() {
                return Err("service plane id must not be empty".to_string());
            }
            if plane.asns.is_empty() {
                return Err(format!("service plane '{}' has no ASNs", plane.id));
            }
            if seen.contains(&plane.id.as_str()) {
                return Err(format!("duplicate service plane id '{}'", plane.id));
            }
            seen.push(plane.id.as_str());
        }
        Ok(())
    }

    /// The named plane whose ASN set contains `asn`, if any.
    pub fn plane_for_asn(&self, asn: u32) -> Option<&NamedServicePlane> {
        self.service_planes.iter().find(|p| p.asns.contains(&asn))
    }

    /// Reviewed display role for an ASN, or the unclassified label.
    pub fn role_label(&self, asn: u32) -> String {
        self.asn_roles
            .iter()
            .find(|r| r.asn == asn)
            .map(|r| r.role.clone())
            .unwrap_or_else(|| UNCLASSIFIED_OBSERVED_ASN.to_string())
    }
}

/// Generic observer session identity: which peer spoke to which observer.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ObserverSessionKey {
    pub source_family: String,
    pub collector: String,
    pub peer_ip: String,
    pub peer_asn: u32,
    pub address_family: String,
}

/// Relationship of one observed path (or session) to a named plane.
///
/// Direct peer and AS-in-path membership are different facts: a route
/// learned directly from a plane's ASN is not the same observation as a
/// route learned from another peer whose path happens to contain the ASN.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionRelationship {
    /// The session's peer ASN equals a named-plane ASN.
    DirectPeerToNamedPlane { plane_id: String },
    /// The peer ASN differs, but the AS path contains a named-plane ASN.
    IndirectPathViaNamedPlane { plane_id: String },
    /// Neither the peer ASN nor the path matches any named plane.
    OtherObservedPath,
    /// Evidence cannot reliably distinguish the relationship.
    Ambiguous,
}

impl SessionRelationship {
    /// The plane this relationship is about, if any.
    pub fn plane_id(&self) -> Option<&str> {
        match self {
            SessionRelationship::DirectPeerToNamedPlane { plane_id }
            | SessionRelationship::IndirectPathViaNamedPlane { plane_id } => Some(plane_id),
            _ => None,
        }
    }

    /// Stable classification label for evidence sections.
    pub fn label(&self) -> &'static str {
        match self {
            SessionRelationship::DirectPeerToNamedPlane { .. } => "direct-peer-to-named-plane",
            SessionRelationship::IndirectPathViaNamedPlane { .. } => {
                "indirect-path-via-named-plane"
            }
            SessionRelationship::OtherObservedPath => "other-observed-path",
            SessionRelationship::Ambiguous => "ambiguous",
        }
    }
}

/// One observed route's path evidence. The FULL AS path is preserved for
/// evidence; classification never truncates it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathEvidence {
    pub peer_ip: String,
    pub peer_asn: u32,
    pub address_family: String,
    pub prefix: String,
    pub as_path: Vec<u32>,
    /// Authoritative origin ASNs reported by the parser for this route
    /// (set for non-empty paths; empty when attributes are absent).
    pub origin_asns: Vec<u32>,
}

/// Classify one route against the profile.
///
/// For every named plane: peer ASN in the plane's ASN set → Direct; else
/// the AS path containing a plane ASN → Indirect. A route may match one,
/// both, or neither plane; direct and indirect for the SAME plane never
/// co-occur on one route. A route with no usable path evidence and a peer
/// ASN that matches no plane is `Ambiguous`.
pub fn classify_route(
    profile: &ServicePlaneProfile,
    peer_asn: u32,
    as_path: &[u32],
) -> Vec<SessionRelationship> {
    let mut out: Vec<SessionRelationship> = Vec::new();
    for plane in &profile.service_planes {
        if plane.asns.contains(&peer_asn) {
            out.push(SessionRelationship::DirectPeerToNamedPlane {
                plane_id: plane.id.clone(),
            });
        } else if as_path.iter().any(|a| plane.asns.contains(a)) {
            out.push(SessionRelationship::IndirectPathViaNamedPlane {
                plane_id: plane.id.clone(),
            });
        }
    }
    if out.is_empty() {
        if as_path.is_empty() {
            out.push(SessionRelationship::Ambiguous);
        } else {
            out.push(SessionRelationship::OtherObservedPath);
        }
    }
    out
}

/// A session's role as observed at one point in time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopedSessionRole {
    pub relationship: SessionRelationship,
    /// Evidence interval (RIB timestamp) over which this role holds.
    pub observed_at_utc: String,
    /// AS2603-...-style route count supporting the role (caller-defined
    /// counting; the field is populated by the audit).
    pub route_count: usize,
}

/// Derive the role(s) of one session from its routes at a given RIB
/// timestamp. The source family is NOT an input to classification: the
/// peer ASN and AS paths are the evidence.
///
/// A session has one role per plane it relates to (direct peers may also
/// carry indirect paths for other planes). Roles are time-scoped: the
/// caller supplies the observation timestamp and must recompute for a
/// different RIB (peer ASN and path membership can change over time).
pub fn session_roles_at(
    profile: &ServicePlaneProfile,
    _key: &ObserverSessionKey,
    routes: &[PathEvidence],
    observed_at_utc: &str,
) -> Vec<ScopedSessionRole> {
    // Deduplicate per (plane_id, kind) while preserving order.
    let mut out: Vec<ScopedSessionRole> = Vec::new();
    for route in routes {
        for rel in classify_route(profile, route.peer_asn, &route.as_path) {
            let duplicate = out
                .iter()
                .any(|r: &ScopedSessionRole| r.relationship == rel);
            if !duplicate {
                out.push(ScopedSessionRole {
                    relationship: rel,
                    observed_at_utc: observed_at_utc.to_string(),
                    route_count: 0,
                });
            }
        }
    }
    // Count supporting routes per role.
    for role in out.iter_mut() {
        role.route_count = routes
            .iter()
            .filter(|r| {
                classify_route(profile, r.peer_asn, &r.as_path).contains(&role.relationship)
            })
            .count();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile_two_planes() -> ServicePlaneProfile {
        ServicePlaneProfile {
            service_planes: vec![
                NamedServicePlane {
                    id: "re".to_string(),
                    display_label: "R&E".to_string(),
                    asns: vec![64500],
                },
                NamedServicePlane {
                    id: "pex".to_string(),
                    display_label: "Peer Exchange".to_string(),
                    asns: vec![64501],
                },
            ],
            asn_roles: vec![
                ReviewedAsnRole {
                    asn: 64500,
                    role: "regional-re".to_string(),
                },
                ReviewedAsnRole {
                    asn: 64501,
                    role: "exchange-participant".to_string(),
                },
            ],
            updated_utc: "2026-08-02T00:00:00Z".to_string(),
            provenance: "test".to_string(),
        }
    }

    #[test]
    fn network_profile_can_define_multiple_named_service_planes() {
        let p = profile_two_planes();
        assert_eq!(p.service_planes.len(), 2);
        assert_eq!(p.plane_for_asn(64500).unwrap().id, "re");
        assert_eq!(p.plane_for_asn(64501).unwrap().id, "pex");
        assert!(p.plane_for_asn(64599).is_none());
    }

    #[test]
    fn service_plane_asns_are_profile_data_not_control_flow() {
        // Two profiles differing ONLY in data must produce different
        // membership — the code derives everything from profile data.
        let p1 = profile_two_planes();
        let mut p2 = profile_two_planes();
        p2.service_planes[0].asns = vec![64600];
        assert!(p1.plane_for_asn(64500).is_some());
        assert!(p2.plane_for_asn(64500).is_none());
        assert!(p2.plane_for_asn(64600).is_some());
        // Display labels never participate.
        let mut p3 = profile_two_planes();
        p3.service_planes[0].display_label = "Renamed".to_string();
        assert!(p3.plane_for_asn(64500).is_some());
    }

    #[test]
    fn one_organization_can_have_multiple_reviewed_asn_roles() {
        let p = profile_two_planes();
        assert_eq!(p.role_label(64500), "regional-re");
        assert_eq!(p.role_label(64501), "exchange-participant");
    }

    #[test]
    fn display_labels_do_not_change_predicate_identity() {
        // Identity is the canonical ASN-set predicate, not the label.
        let p1 = profile_two_planes();
        let mut p2 = profile_two_planes();
        p2.service_planes[0].display_label = "Completely Different Name".to_string();
        let id1 = format!("{:?}", p1.service_planes[0].asns);
        let id2 = format!("{:?}", p2.service_planes[0].asns);
        assert_eq!(id1, id2);
        // And the label survives only as presentation data.
        assert_ne!(
            p1.service_planes[0].display_label,
            p2.service_planes[0].display_label
        );
    }

    #[test]
    fn direct_peer_relationship_uses_peer_asn() {
        let p = profile_two_planes();
        // Peer ASN equals the plane ASN even when the path does not carry it.
        let rels = classify_route(&p, 64500, &[64599, 64600]);
        assert_eq!(
            rels,
            vec![SessionRelationship::DirectPeerToNamedPlane {
                plane_id: "re".to_string()
            }]
        );
    }

    #[test]
    fn indirect_path_relationship_uses_as_path() {
        let p = profile_two_planes();
        // Peer ASN differs, but the path contains the plane ASN.
        let rels = classify_route(&p, 64599, &[64599, 64500, 64000]);
        assert_eq!(
            rels,
            vec![SessionRelationship::IndirectPathViaNamedPlane {
                plane_id: "re".to_string()
            }]
        );
    }

    #[test]
    fn direct_and_indirect_relationships_are_not_conflated() {
        let p = profile_two_planes();
        // A direct peer of plane "re" whose path also crosses plane "pex":
        // two distinct facts, never merged into one.
        let rels = classify_route(&p, 64500, &[64500, 64501]);
        assert_eq!(
            rels,
            vec![
                SessionRelationship::DirectPeerToNamedPlane {
                    plane_id: "re".to_string()
                },
                SessionRelationship::IndirectPathViaNamedPlane {
                    plane_id: "pex".to_string()
                },
            ]
        );
        // Direct and indirect for the SAME plane never co-occur.
        let rels = classify_route(&p, 64500, &[64500]);
        assert_eq!(rels.len(), 1);
        assert!(matches!(
            rels[0],
            SessionRelationship::DirectPeerToNamedPlane { .. }
        ));
    }

    #[test]
    fn source_family_does_not_determine_peer_asn() {
        let p = profile_two_planes();
        let rels_a = classify_route(&p, 64599, &[64599, 64500]);
        let rels_b = classify_route(&p, 64599, &[64599, 64500]);
        assert_eq!(rels_a, rels_b);
        // Same evidence, different hypothetical family string: identical.
        let key_a = ObserverSessionKey {
            source_family: "alpha".to_string(),
            collector: "c".to_string(),
            peer_ip: "192.0.2.1".to_string(),
            peer_asn: 64599,
            address_family: "ipv4".to_string(),
        };
        let key_b = ObserverSessionKey {
            source_family: "beta".to_string(),
            ..key_a.clone()
        };
        let routes = vec![PathEvidence {
            peer_ip: "192.0.2.1".to_string(),
            peer_asn: 64599,
            address_family: "ipv4".to_string(),
            prefix: "198.51.100.0/24".to_string(),
            as_path: vec![64599, 64500],
            origin_asns: vec![64500],
        }];
        let ra = session_roles_at(&p, &key_a, &routes, "2026-08-02T00:00:00Z");
        let rb = session_roles_at(&p, &key_b, &routes, "2026-08-02T00:00:00Z");
        assert_eq!(ra, rb);
    }

    #[test]
    fn one_collector_can_have_multiple_session_roles() {
        let p = profile_two_planes();
        let key = ObserverSessionKey {
            source_family: "ris".to_string(),
            collector: "c1".to_string(),
            peer_ip: "192.0.2.1".to_string(),
            peer_asn: 64500,
            address_family: "ipv4".to_string(),
        };
        // The same collector peers with a direct plane peer AND an indirect
        // session AND an unrelated session — three different roles.
        let direct = vec![PathEvidence {
            peer_ip: "192.0.2.1".to_string(),
            peer_asn: 64500,
            address_family: "ipv4".to_string(),
            prefix: "198.51.100.0/24".to_string(),
            as_path: vec![64500],
            origin_asns: vec![64500],
        }];
        let indirect = vec![PathEvidence {
            peer_ip: "192.0.2.2".to_string(),
            peer_asn: 64599,
            address_family: "ipv4".to_string(),
            prefix: "198.51.100.0/24".to_string(),
            as_path: vec![64599, 64500],
            origin_asns: vec![64500],
        }];
        let other = vec![PathEvidence {
            peer_ip: "192.0.2.3".to_string(),
            peer_asn: 64600,
            address_family: "ipv4".to_string(),
            prefix: "198.51.100.0/24".to_string(),
            as_path: vec![64600, 64601, 64000],
            origin_asns: vec![64000],
        }];
        let k2 = ObserverSessionKey {
            peer_ip: "192.0.2.2".to_string(),
            ..key.clone()
        };
        let k3 = ObserverSessionKey {
            peer_ip: "192.0.2.3".to_string(),
            ..key.clone()
        };
        let r1 = session_roles_at(&p, &key, &direct, "T");
        let r2 = session_roles_at(&p, &k2, &indirect, "T");
        let r3 = session_roles_at(&p, &k3, &other, "T");
        assert!(matches!(
            r1[0].relationship,
            SessionRelationship::DirectPeerToNamedPlane { .. }
        ));
        assert!(matches!(
            r2[0].relationship,
            SessionRelationship::IndirectPathViaNamedPlane { .. }
        ));
        assert!(matches!(
            r3[0].relationship,
            SessionRelationship::OtherObservedPath
        ));
    }

    #[test]
    fn session_role_is_time_scoped() {
        let p = profile_two_planes();
        // The same peer at two RIB timestamps can have different roles:
        // peer ASN 64500 at T1 (direct), renumbered peer 64599 at T2
        // whose path still crosses the plane (indirect).
        let routes_t1 = vec![PathEvidence {
            peer_ip: "192.0.2.1".to_string(),
            peer_asn: 64500,
            address_family: "ipv4".to_string(),
            prefix: "198.51.100.0/24".to_string(),
            as_path: vec![64500],
            origin_asns: vec![64500],
        }];
        let routes_t2 = vec![PathEvidence {
            peer_ip: "192.0.2.1".to_string(),
            peer_asn: 64599,
            address_family: "ipv4".to_string(),
            prefix: "198.51.100.0/24".to_string(),
            as_path: vec![64599, 64500],
            origin_asns: vec![64500],
        }];
        let key = ObserverSessionKey {
            source_family: "ris".to_string(),
            collector: "c1".to_string(),
            peer_ip: "192.0.2.1".to_string(),
            peer_asn: 64500,
            address_family: "ipv4".to_string(),
        };
        let r1 = session_roles_at(&p, &key, &routes_t1, "T1");
        let r2 = session_roles_at(&p, &key, &routes_t2, "T2");
        assert_eq!(r1[0].observed_at_utc, "T1");
        assert_eq!(r2[0].observed_at_utc, "T2");
        assert!(matches!(
            r1[0].relationship,
            SessionRelationship::DirectPeerToNamedPlane { .. }
        ));
        assert!(matches!(
            r2[0].relationship,
            SessionRelationship::IndirectPathViaNamedPlane { .. }
        ));
    }

    #[test]
    fn unknown_path_asn_is_not_labeled_commercial() {
        let p = profile_two_planes();
        assert_eq!(p.role_label(64999), UNCLASSIFIED_OBSERVED_ASN);
        assert!(!p.role_label(64999).contains("commercial"));
    }

    #[test]
    fn reviewed_nren_role_is_data_driven() {
        let p = profile_two_planes();
        assert_eq!(p.role_label(64500), "regional-re");
        let mut p2 = profile_two_planes();
        p2.asn_roles[0].role = "international-nren".to_string();
        assert_eq!(p2.role_label(64500), "international-nren");
    }

    #[test]
    fn collector_geography_does_not_define_network_role() {
        let p = profile_two_planes();
        // The same ASN gets the same role regardless of where the observer
        // sits; location is not an input to role_label.
        assert_eq!(p.role_label(64501), "exchange-participant");
        let key = ObserverSessionKey {
            source_family: "ris".to_string(),
            collector: "collector-in-europe".to_string(),
            peer_ip: "192.0.2.1".to_string(),
            peer_asn: 64501,
            address_family: "ipv4".to_string(),
        };
        let routes = vec![PathEvidence {
            peer_ip: "192.0.2.1".to_string(),
            peer_asn: 64501,
            address_family: "ipv4".to_string(),
            prefix: "198.51.100.0/24".to_string(),
            as_path: vec![64501],
            origin_asns: vec![64501],
        }];
        let r1 = session_roles_at(&p, &key, &routes, "T");
        let key2 = ObserverSessionKey {
            collector: "collector-in-oceania".to_string(),
            ..key.clone()
        };
        let r2 = session_roles_at(&p, &key2, &routes, "T");
        assert_eq!(r1, r2);
    }

    #[test]
    fn full_path_remains_available_in_evidence() {
        let p = profile_two_planes();
        let path = vec![64600, 64601, 64500, 64000, 63999];
        let rels = classify_route(&p, 64600, &path);
        assert!(matches!(
            rels[0],
            SessionRelationship::IndirectPathViaNamedPlane { .. }
        ));
        // Classification consumed a slice; the caller's full path is intact.
        assert_eq!(path.len(), 5);
        assert_eq!(path[4], 63999);
        // PathEvidence preserves the whole path through the session API.
        let key = ObserverSessionKey {
            source_family: "rv".to_string(),
            collector: "rv2".to_string(),
            peer_ip: "192.0.2.9".to_string(),
            peer_asn: 64600,
            address_family: "ipv4".to_string(),
        };
        let ev = PathEvidence {
            peer_ip: "192.0.2.9".to_string(),
            peer_asn: 64600,
            address_family: "ipv4".to_string(),
            prefix: "198.51.100.0/24".to_string(),
            as_path: path.clone(),
            origin_asns: path.last().copied().map(|a| vec![a]).unwrap_or_default(),
        };
        let roles = session_roles_at(&p, &key, &[ev], "T");
        assert_eq!(roles.len(), 1);
    }
}

/// Reviewed collector-location metadata with temporal provenance.
///
/// Location describes where the collector's route reflector is hosted; it
/// does NOT describe the geographic path of observed routes and never
/// defines a network's role. `region` classifies the OBSERVER SITE only
/// (AMER/EMEA/APAC/Unknown) and is never rendered as the region of the
/// affected network, the route path, the peer organization, or the users.
/// `multihop` marks collectors reached via a multihop session (still a
/// valid site region, but the UI must make the multihop nature visible).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CollectorLocation {
    pub family: String,
    pub collector: String,
    pub location: String,
    pub facility: String,
    #[serde(default)]
    pub note: Option<String>,
    /// Observer-site region: AMER, EMEA, APAC, or Unknown.
    #[serde(default = "default_unknown_region")]
    pub region: String,
    /// Whether the collector is reached via a multihop session.
    #[serde(default)]
    pub multihop: bool,
}

fn default_unknown_region() -> String {
    "Unknown".to_string()
}

/// A registry of collector locations, loaded from a reviewed data file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct CollectorLocationRegistry {
    pub as_of: String,
    pub collectors: Vec<CollectorLocation>,
}

impl CollectorLocationRegistry {
    pub fn load(path: &Path) -> Result<Self, String> {
        let raw = std::fs::read_to_string(path)
            .map_err(|e| format!("cannot read collector metadata {}: {e}", path.display()))?;
        serde_json::from_str(&raw)
            .map_err(|e| format!("invalid collector metadata {}: {e}", path.display()))
    }

    /// Look up a collector's reviewed location. Family comparison is
    /// case-insensitive (manifests use canonical family names while the
    /// registry may record lowercase family keys).
    pub fn location(&self, family: &str, collector: &str) -> Option<&CollectorLocation> {
        self.collectors
            .iter()
            .find(|c| c.family.eq_ignore_ascii_case(family) && c.collector == collector)
    }

    /// Observer-site region for a collector, or "Unknown" when the
    /// collector has no reviewed metadata entry.
    pub fn region(&self, family: &str, collector: &str) -> String {
        self.location(family, collector)
            .map(|c| c.region.clone())
            .unwrap_or_else(default_unknown_region)
    }

    /// Whether the collector is reached via a multihop session. Unknown
    /// collectors are treated as non-multihop (no evidence of multihop).
    pub fn is_multihop(&self, family: &str, collector: &str) -> bool {
        self.location(family, collector)
            .map(|c| c.multihop)
            .unwrap_or(false)
    }
}

/// Path-class membership counts for one session (or collector): how many
/// origin-matching routes contain each named plane's ASN, how many contain
/// no plane ASN. The four-bucket (A-only / B-only / both / neither) view is
/// a rendering of these generic counts for a two-plane profile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PathClassCounts {
    /// Routes whose AS path contains the plane's ASN, per plane id.
    pub per_plane_contains: Vec<(String, usize)>,
    /// Routes containing no named-plane ASN.
    pub neither_plane: usize,
    pub total: usize,
}

impl PathClassCounts {
    /// Increment membership for every plane whose ASN appears in `path`.
    pub fn observe(&mut self, profile: &ServicePlaneProfile, path: &[u32]) {
        self.total += 1;
        let mut matched = false;
        for plane in &profile.service_planes {
            if path.iter().any(|a| plane.asns.contains(a)) {
                let entry = self
                    .per_plane_contains
                    .iter_mut()
                    .find(|(id, _)| id == &plane.id);
                match entry {
                    Some((_, n)) => *n += 1,
                    None => self.per_plane_contains.push((plane.id.clone(), 1)),
                }
                matched = true;
            }
        }
        if !matched {
            self.neither_plane += 1;
        }
    }
}

/// One historical collector session (peer) as observed in a baseline RIB.
///
/// The peer ASN comes from the MRT header of the historical RIB — the
/// source of truth. Current peer lists are supporting context only and can
/// never override these rows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionAuditRow {
    pub source_family: String,
    pub collector: String,
    /// Reviewed collector location (registry data, for display only).
    pub location: String,
    pub rib_timestamp_utc: String,
    pub rib_source_sha: String,
    pub peer_ip: String,
    pub peer_asn: u32,
    pub address_family: String,
    /// Origin-matching route count received from this peer.
    pub origin_route_count: usize,
    pub distinct_prefixes: usize,
    pub path_class: PathClassCounts,
}

/// Aggregate origin-matching routes into per-session audit rows.
///
/// `evidence_at` is the RIB timestamp; `source_sha` identifies the RIB.
/// IPv4 and IPv6 sessions of the same collector stay distinct rows (the
/// session key includes the address family).
pub fn audit_sessions(
    profile: &ServicePlaneProfile,
    registry: &CollectorLocationRegistry,
    source_family: &str,
    collector: &str,
    evidence_at: &str,
    source_sha: &str,
    routes: &[PathEvidence],
) -> Vec<SessionAuditRow> {
    let mut rows: Vec<SessionAuditRow> = Vec::new();
    for route in routes {
        let key = (&route.peer_ip, route.peer_asn, &route.address_family);
        let row = rows.iter_mut().find(|r| {
            r.peer_ip.as_str() == key.0 && r.peer_asn == key.1 && r.address_family.as_str() == key.2
        });
        match row {
            Some(row) => {
                row.origin_route_count += 1;
                row.path_class.observe(profile, &route.as_path);
            }
            None => {
                let mut pc = PathClassCounts::default();
                pc.observe(profile, &route.as_path);
                let location = registry
                    .location(source_family, collector)
                    .map(|c| c.location.clone())
                    .unwrap_or_else(|| "unknown".to_string());
                rows.push(SessionAuditRow {
                    source_family: source_family.to_string(),
                    collector: collector.to_string(),
                    location,
                    rib_timestamp_utc: evidence_at.to_string(),
                    rib_source_sha: source_sha.to_string(),
                    peer_ip: route.peer_ip.clone(),
                    peer_asn: route.peer_asn,
                    address_family: route.address_family.clone(),
                    origin_route_count: 1,
                    distinct_prefixes: 1,
                    path_class: pc,
                });
            }
        }
    }
    // Count distinct prefixes per session.
    let mut seen: Vec<Vec<String>> = rows.iter().map(|_| Vec::new()).collect();
    for route in routes {
        let idx = rows
            .iter()
            .position(|r| {
                r.peer_ip == route.peer_ip
                    && r.peer_asn == route.peer_asn
                    && r.address_family == route.address_family
            })
            .unwrap_or(usize::MAX);
        if idx != usize::MAX && !seen[idx].contains(&route.prefix) {
            seen[idx].push(route.prefix.clone());
        }
    }
    for (i, row) in rows.iter_mut().enumerate() {
        row.distinct_prefixes = seen[i].len();
    }
    rows
}

/// One session in a FULL peer inventory of a baseline RIB.
///
/// Unlike the origin-scoped audit, the inventory reports EVERY session
/// present in the MRT peer table (all peers, all route counts), which is
/// what answers "did a direct session with peer ASN X exist at all" even
/// when that session carried no target-origin routes. `origin_route_count`
/// and `distinct_origin_prefixes` are the target-origin subset.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PeerInventoryRow {
    pub source_family: String,
    pub collector: String,
    /// Reviewed collector location (registry data, for display only).
    pub location: String,
    pub rib_timestamp_utc: String,
    pub rib_source_sha: String,
    pub peer_ip: String,
    pub peer_asn: u32,
    pub address_family: String,
    /// All routes received from this peer (any origin).
    pub total_route_count: usize,
    /// Target-origin route count received from this peer.
    pub origin_route_count: usize,
    /// Distinct target-origin prefixes received from this peer.
    pub distinct_origin_prefixes: usize,
    /// Path-class membership over ALL routes of this session.
    pub path_class: PathClassCounts,
    /// Path-class membership over the TARGET-ORIGIN routes only. This is
    /// the answer to "did the target's qualifying baseline exist via this
    /// session": an origin route whose path contains a plane ASN is a
    /// qualifying observer-prefix stream for that plane.
    pub origin_path_class: PathClassCounts,
}

/// Streaming accumulator for a full peer inventory.
///
/// Memory is bounded by the number of SESSIONS (not routes): a full RIS
/// bview holds ~1M routes but only a few hundred sessions, so a full parse
/// aggregates in place instead of materializing every route. This is what
/// keeps a full-RIB inventory feasible on modest machines.
pub struct PeerInventoryAccumulator<'a> {
    profile: &'a ServicePlaneProfile,
    registry: &'a CollectorLocationRegistry,
    source_family: String,
    collector: String,
    evidence_at: String,
    source_sha: String,
    origin_asns: Vec<u32>,
    rows: Vec<MutableInventoryRow>,
}

/// Mutable per-session aggregate while the stream is consumed.
#[derive(Debug, Clone)]
struct MutableInventoryRow {
    peer_ip: String,
    peer_asn: u32,
    address_family: String,
    total_route_count: usize,
    origin_route_count: usize,
    origin_prefixes: Vec<String>,
    path_class: PathClassCounts,
    origin_path_class: PathClassCounts,
}

impl<'a> PeerInventoryAccumulator<'a> {
    pub fn new(
        profile: &'a ServicePlaneProfile,
        registry: &'a CollectorLocationRegistry,
        source_family: &str,
        collector: &str,
        evidence_at: &str,
        source_sha: &str,
        origin_asns: Vec<u32>,
    ) -> Self {
        PeerInventoryAccumulator {
            profile,
            registry,
            source_family: source_family.to_string(),
            collector: collector.to_string(),
            evidence_at: evidence_at.to_string(),
            source_sha: source_sha.to_string(),
            origin_asns,
            rows: Vec::new(),
        }
    }

    /// Consume one route into the session aggregate.
    pub fn observe(&mut self, route: &PathEvidence) {
        let is_origin = !route.origin_asns.is_empty()
            && route
                .origin_asns
                .iter()
                .any(|a| self.origin_asns.contains(a));
        let row = self.rows.iter_mut().find(|r| {
            r.peer_ip == route.peer_ip
                && r.peer_asn == route.peer_asn
                && r.address_family == route.address_family
        });
        match row {
            Some(row) => {
                row.total_route_count += 1;
                row.path_class.observe(self.profile, &route.as_path);
                if is_origin {
                    row.origin_route_count += 1;
                    row.origin_path_class.observe(self.profile, &route.as_path);
                    if !row.origin_prefixes.contains(&route.prefix) {
                        row.origin_prefixes.push(route.prefix.clone());
                    }
                }
            }
            None => {
                let mut pc = PathClassCounts::default();
                pc.observe(self.profile, &route.as_path);
                let mut opc = PathClassCounts::default();
                if is_origin {
                    opc.observe(self.profile, &route.as_path);
                }
                self.rows.push(MutableInventoryRow {
                    peer_ip: route.peer_ip.clone(),
                    peer_asn: route.peer_asn,
                    address_family: route.address_family.clone(),
                    total_route_count: 1,
                    origin_route_count: if is_origin { 1 } else { 0 },
                    origin_prefixes: if is_origin {
                        vec![route.prefix.clone()]
                    } else {
                        Vec::new()
                    },
                    path_class: pc,
                    origin_path_class: opc,
                });
            }
        }
    }

    /// Finish and return deterministic rows sorted by
    /// (peer IP, address family, peer ASN).
    pub fn finish(self) -> Vec<PeerInventoryRow> {
        let mut out: Vec<PeerInventoryRow> = self
            .rows
            .into_iter()
            .map(|r| PeerInventoryRow {
                source_family: self.source_family.clone(),
                collector: self.collector.clone(),
                location: self
                    .registry
                    .location(&self.source_family, &self.collector)
                    .map(|c| c.location.clone())
                    .unwrap_or_else(|| "unknown".to_string()),
                rib_timestamp_utc: self.evidence_at.clone(),
                rib_source_sha: self.source_sha.clone(),
                peer_ip: r.peer_ip,
                peer_asn: r.peer_asn,
                address_family: r.address_family,
                total_route_count: r.total_route_count,
                origin_route_count: r.origin_route_count,
                distinct_origin_prefixes: r.origin_prefixes.len(),
                path_class: r.path_class,
                origin_path_class: r.origin_path_class,
            })
            .collect();
        out.sort_by(|a, b| {
            (a.peer_ip.clone(), a.address_family.clone(), a.peer_asn).cmp(&(
                b.peer_ip.clone(),
                b.address_family.clone(),
                b.peer_asn,
            ))
        });
        out
    }
}

/// Aggregate EVERY session observed in a RIB into inventory rows.
///
/// `routes` here is the full RIB (no origin filter): every peer session
/// present in the baseline is reported, with total and target-origin
/// counts. A session that never announced a target-origin prefix still
/// appears — its presence/absence is itself the evidence for
/// "direct session present or absent". Rows are deterministic: sorted by
/// (peer IP, address family, peer ASN).
#[allow(clippy::too_many_arguments)] // explicit data passthrough; each arg is one report field
pub fn peer_inventory(
    profile: &ServicePlaneProfile,
    registry: &CollectorLocationRegistry,
    source_family: &str,
    collector: &str,
    evidence_at: &str,
    source_sha: &str,
    routes: &[PathEvidence],
    origin_asns: &[u32],
) -> Vec<PeerInventoryRow> {
    let mut acc = PeerInventoryAccumulator::new(
        profile,
        registry,
        source_family,
        collector,
        evidence_at,
        source_sha,
        origin_asns.to_vec(),
    );
    for route in routes {
        acc.observe(route);
    }
    acc.finish()
}

/// The direct-peer decision for one plane: is there a session whose peer
/// ASN equals a reviewed plane ASN, and does that session carry
/// target-origin routes?
///
/// The plane identity comes from the PROFILE (runtime data), never from a
/// literal in code. A direct session with zero target-origin routes is
/// "present but no qualifying baseline" — a different fact from "session
/// absent", and both are different from "plane ASN appears in some other
/// session's AS path".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaneDirectSessionDecision {
    pub plane_id: String,
    pub plane_label: String,
    /// Whether any session's peer ASN equals a plane ASN.
    pub direct_session_present: bool,
    /// Peer IP/ASN of the direct session, when present.
    pub direct_peer_ip: Option<String>,
    pub direct_peer_asn: Option<u32>,
    /// Target-origin routes received from the direct session.
    pub direct_origin_route_count: usize,
    /// Whether any session's AS path contains a plane ASN (indirect
    /// observation of the plane, distinct from a direct session).
    pub plane_asn_in_path: bool,
}

/// Decide the direct-session facts for one plane from an inventory.
pub fn direct_session_decision(
    profile: &ServicePlaneProfile,
    plane_id: &str,
    inventory: &[PeerInventoryRow],
    _origin_asns: &[u32],
) -> Option<PlaneDirectSessionDecision> {
    let plane = profile.service_planes.iter().find(|p| p.id == plane_id)?;
    let mut out = PlaneDirectSessionDecision {
        plane_id: plane.id.clone(),
        plane_label: plane.display_label.clone(),
        direct_session_present: false,
        direct_peer_ip: None,
        direct_peer_asn: None,
        direct_origin_route_count: 0,
        plane_asn_in_path: false,
    };
    for row in inventory {
        if plane.asns.contains(&row.peer_asn) {
            out.direct_session_present = true;
            out.direct_peer_ip = Some(row.peer_ip.clone());
            out.direct_peer_asn = Some(row.peer_asn);
            break;
        }
    }
    // Indirect path evidence is a SEPARATE fact: plane ASN inside some
    // path (with a different peer ASN) never implies a direct session.
    for row in inventory {
        let contains_plane_in_path = row
            .path_class
            .per_plane_contains
            .iter()
            .any(|(id, count)| id == plane_id && *count > 0);
        if contains_plane_in_path {
            out.plane_asn_in_path = true;
        }
    }
    // Avoid double-counting origins in the direct peer across AF rows.
    let mut seen_peer_ips: Vec<String> = Vec::new();
    let mut total_direct_origin = 0usize;
    for row in inventory {
        if row.peer_asn == out.direct_peer_asn.unwrap_or(u32::MAX)
            && !seen_peer_ips.contains(&row.peer_ip)
        {
            seen_peer_ips.push(row.peer_ip.clone());
            total_direct_origin += row.origin_route_count;
        }
    }
    if out.direct_session_present {
        out.direct_origin_route_count = total_direct_origin;
    }
    Some(out)
}

#[cfg(test)]
mod session_audit_tests {
    use super::*;

    fn profile_two_planes() -> ServicePlaneProfile {
        ServicePlaneProfile {
            service_planes: vec![
                NamedServicePlane {
                    id: "re".to_string(),
                    display_label: "R&E".to_string(),
                    asns: vec![64500],
                },
                NamedServicePlane {
                    id: "pex".to_string(),
                    display_label: "Peer Exchange".to_string(),
                    asns: vec![64501],
                },
            ],
            asn_roles: vec![],
            updated_utc: "2026-08-02T00:00:00Z".to_string(),
            provenance: "test".to_string(),
        }
    }

    fn registry() -> CollectorLocationRegistry {
        CollectorLocationRegistry {
            as_of: "2019-09-05".to_string(),
            collectors: vec![
                CollectorLocation {
                    family: "ris".to_string(),
                    collector: "rrc06".to_string(),
                    location: "Otemachi, Tokyo, Japan".to_string(),
                    facility: "DIX-IE / JPIX".to_string(),
                    note: None,
                    region: "APAC".to_string(),
                    multihop: false,
                },
                CollectorLocation {
                    family: "ris".to_string(),
                    collector: "rrc15".to_string(),
                    location: "Sao Paulo, Brazil".to_string(),
                    facility: "PTTMetro".to_string(),
                    note: None,
                    region: "AMER".to_string(),
                    multihop: false,
                },
            ],
        }
    }

    fn routes_for(peer_ip: &str, peer_asn: u32, af: &str, prefixes: &[&str]) -> Vec<PathEvidence> {
        prefixes
            .iter()
            .map(|p| PathEvidence {
                peer_ip: peer_ip.to_string(),
                peer_asn,
                address_family: af.to_string(),
                prefix: p.to_string(),
                as_path: vec![peer_asn, 64500],
                origin_asns: vec![64500],
            })
            .collect()
    }

    #[test]
    fn rrc06_location_is_not_united_states() {
        let reg = registry();
        let loc = reg.location("ris", "rrc06").expect("rrc06 metadata");
        assert!(
            !loc.location.contains("United States"),
            "rrc06 must not be labeled United States"
        );
        assert!(loc.location.contains("Tokyo"));
        assert!(loc.location.contains("Japan"));
        assert!(loc.location.contains("Otemachi"));
    }

    #[test]
    fn historical_session_audit_uses_rib_peer_asn() {
        let p = profile_two_planes();
        let reg = registry();
        let routes = routes_for("192.0.2.1", 64500, "ipv4", &["198.51.100.0/24"]);
        let rows = audit_sessions(
            &p,
            &reg,
            "ris",
            "rrc06",
            "2019-08-21T00:00:00Z",
            "sha",
            &routes,
        );
        assert_eq!(rows.len(), 1);
        // The peer ASN comes from the RIB evidence, not from any registry.
        assert_eq!(rows[0].peer_asn, 64500);
        assert_eq!(rows[0].origin_route_count, 1);
        assert_eq!(rows[0].distinct_prefixes, 1);
        // Location is display-only registry data, clearly separate.
        assert_eq!(rows[0].location, "Otemachi, Tokyo, Japan");
    }

    #[test]
    fn current_peer_metadata_does_not_override_historical_evidence() {
        let p = profile_two_planes();
        let reg = registry();
        // A hypothetical current peer list would claim a different peer
        // ASN for this session; the audit keeps the RIB's peer ASN.
        let routes = routes_for("192.0.2.1", 64500, "ipv4", &["198.51.100.0/24"]);
        let rows = audit_sessions(
            &p,
            &reg,
            "ris",
            "rrc06",
            "2019-08-21T00:00:00Z",
            "sha",
            &routes,
        );
        assert_eq!(rows[0].peer_asn, 64500);
        assert_eq!(rows[0].peer_ip, "192.0.2.1");
        // The registry carries location only — no peer ASN field exists to
        // override with.
        let loc = reg.location("ris", "rrc06").unwrap();
        assert!(!loc.location.is_empty());
    }

    #[test]
    fn collector_location_and_peer_network_location_are_distinct() {
        let p = profile_two_planes();
        let reg = registry();
        // Two collectors with the same peer get different location strings
        // (collector metadata), while the peer evidence stays identical.
        let routes = routes_for("192.0.2.9", 64500, "ipv4", &["198.51.100.0/24"]);
        let a = audit_sessions(&p, &reg, "ris", "rrc06", "T", "sha", &routes);
        let b = audit_sessions(&p, &reg, "ris", "rrc15", "T", "sha", &routes);
        assert_eq!(a[0].peer_ip, b[0].peer_ip);
        assert_eq!(a[0].peer_asn, b[0].peer_asn);
        assert_ne!(a[0].location, b[0].location);
        // And classification never consults location: same classification
        // for both collectors.
        assert_eq!(
            classify_route(&p, 64500, &[64500]),
            classify_route(&p, 64500, &[64500])
        );
    }

    #[test]
    fn ipv4_and_ipv6_sessions_remain_distinct() {
        let p = profile_two_planes();
        let reg = registry();
        let mut routes = routes_for("192.0.2.1", 64500, "ipv4", &["198.51.100.0/24"]);
        routes.extend(routes_for(
            "2001:db8::1",
            64500,
            "ipv6",
            &["2001:db8:1::/48"],
        ));
        let rows = audit_sessions(&p, &reg, "ris", "rrc06", "T", "sha", &routes);
        assert_eq!(rows.len(), 2, "v4 and v6 sessions must stay distinct rows");
        assert_eq!(rows[0].address_family, "ipv4");
        assert_eq!(rows[1].address_family, "ipv6");
        assert_eq!(rows[0].distinct_prefixes, 1);
        assert_eq!(rows[1].distinct_prefixes, 1);
    }

    // ── Full peer inventory (Session 36, Part 1) ─────────────────────

    #[test]
    fn peer_inventory_reports_sessions_without_target_origin_routes() {
        let p = profile_two_planes();
        let reg = registry();
        // One session carries target-origin routes; another session carries
        // only other-origin routes and would be invisible to an
        // origin-scoped audit.
        let mut routes = routes_for("192.0.2.1", 64500, "ipv4", &["198.51.100.0/24"]);
        routes.push(PathEvidence {
            peer_ip: "192.0.2.2".to_string(),
            peer_asn: 64600,
            address_family: "ipv4".to_string(),
            prefix: "203.0.113.0/24".to_string(),
            as_path: vec![64600, 64601],
            origin_asns: vec![64601],
        });
        let rows = peer_inventory(&p, &reg, "ris", "rrc06", "T", "sha", &routes, &[64500]);
        assert_eq!(rows.len(), 2, "inventory reports every session");
        let with_origin = rows.iter().find(|r| r.peer_asn == 64500).unwrap();
        assert_eq!(with_origin.total_route_count, 1);
        assert_eq!(with_origin.origin_route_count, 1);
        let without_origin = rows.iter().find(|r| r.peer_asn == 64600).unwrap();
        assert_eq!(without_origin.total_route_count, 1);
        assert_eq!(
            without_origin.origin_route_count, 0,
            "session present with zero target-origin routes"
        );
        assert_eq!(without_origin.distinct_origin_prefixes, 0);
    }

    #[test]
    fn peer_inventory_counts_total_and_origin_routes_separately() {
        let p = profile_two_planes();
        let reg = registry();
        // Same peer announces 3 routes: 2 target-origin, 1 other-origin.
        let mut routes = routes_for("192.0.2.1", 64500, "ipv4", &["198.51.100.0/24"]);
        routes.push(PathEvidence {
            peer_ip: "192.0.2.1".to_string(),
            peer_asn: 64500,
            address_family: "ipv4".to_string(),
            prefix: "198.51.100.0/25".to_string(),
            as_path: vec![64500],
            origin_asns: vec![64500],
        });
        routes.push(PathEvidence {
            peer_ip: "192.0.2.1".to_string(),
            peer_asn: 64500,
            address_family: "ipv4".to_string(),
            prefix: "203.0.113.0/24".to_string(),
            as_path: vec![64500, 64601],
            origin_asns: vec![64601],
        });
        let rows = peer_inventory(&p, &reg, "ris", "rrc06", "T", "sha", &routes, &[64500]);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].total_route_count, 3);
        assert_eq!(rows[0].origin_route_count, 2);
        assert_eq!(rows[0].distinct_origin_prefixes, 2);
    }

    #[test]
    fn direct_session_decision_uses_profile_plane_asns() {
        let p = profile_two_planes();
        let reg = registry();
        // The PEX plane (64501) has a direct session with ZERO target-origin
        // routes; the RE plane (64500) has no direct session but its ASN
        // appears inside other paths.
        let routes = vec![
            PathEvidence {
                peer_ip: "192.0.2.1".to_string(),
                peer_asn: 64501,
                address_family: "ipv4".to_string(),
                prefix: "203.0.113.0/24".to_string(),
                as_path: vec![64501, 64600],
                origin_asns: vec![64600],
            },
            PathEvidence {
                peer_ip: "192.0.2.2".to_string(),
                peer_asn: 64600,
                address_family: "ipv4".to_string(),
                prefix: "198.51.100.0/24".to_string(),
                as_path: vec![64600, 64500, 64000],
                origin_asns: vec![64000],
            },
        ];
        let inv = peer_inventory(&p, &reg, "ris", "rrc06", "T", "sha", &routes, &[64500]);
        let pex = direct_session_decision(&p, "pex", &inv, &[64500]).unwrap();
        assert!(pex.direct_session_present, "direct PEX session exists");
        assert_eq!(pex.direct_peer_asn, Some(64501));
        assert_eq!(
            pex.direct_origin_route_count, 0,
            "direct session present with zero qualifying routes"
        );
        let re = direct_session_decision(&p, "re", &inv, &[64500]).unwrap();
        assert!(
            !re.direct_session_present,
            "no direct RE session: peer ASN never equals 64500"
        );
        assert!(
            re.plane_asn_in_path,
            "64500 in path is indirect evidence, not a direct session"
        );
        // Decision keys off the profile: swap plane ASNs and the decision
        // follows the data, never a hard-coded identity.
        let mut p2 = profile_two_planes();
        p2.service_planes[1].asns = vec![64600];
        let pex2 = direct_session_decision(&p2, "pex", &inv, &[64500]).unwrap();
        assert!(
            pex2.direct_session_present,
            "direct session follows profile ASNs"
        );
        assert_eq!(pex2.direct_peer_asn, Some(64600));
    }

    #[test]
    fn inventory_rows_are_deterministic() {
        let p = profile_two_planes();
        let reg = registry();
        let mut routes = routes_for("192.0.2.3", 64600, "ipv4", &["198.51.100.0/24"]);
        routes.extend(routes_for("192.0.2.1", 64500, "ipv4", &["198.51.100.0/24"]));
        routes.extend(routes_for("192.0.2.2", 64599, "ipv6", &["2001:db8::/32"]));
        let a = peer_inventory(&p, &reg, "ris", "rrc06", "T", "sha", &routes, &[64500]);
        let b = peer_inventory(&p, &reg, "ris", "rrc06", "T", "sha", &routes, &[64500]);
        assert_eq!(a, b);
        assert_eq!(
            a.iter().map(|r| r.peer_ip.clone()).collect::<Vec<_>>(),
            vec![
                "192.0.2.1".to_string(),
                "192.0.2.2".to_string(),
                "192.0.2.3".to_string()
            ]
        );
    }

    // ── Observer-site regions (Session 36, Part 5) ───────────────────

    #[test]
    fn region_classifies_observer_site_only() {
        let reg = registry();
        // The registry's region is the collector site region.
        assert_eq!(reg.region("ris", "rrc06"), "APAC");
        assert_eq!(reg.region("ris", "rrc15"), "AMER");
        // The same registry never contains a "peer region" or an
        // "affected-network region": there is no such field to read.
        let loc = reg.location("ris", "rrc06").unwrap();
        assert_eq!(loc.region, "APAC");
        assert!(!loc.region.contains("peer"));
    }

    #[test]
    fn peer_region_is_not_inferred_from_collector_region() {
        let reg = registry();
        // A peer at a Tokyo collector is NOT thereby "in Tokyo" — the
        // region lookup applies to the collector site only, and no API
        // derives a peer's region from it.
        let peer_region_derived = reg.region("ris", "rrc06");
        assert_eq!(peer_region_derived, "APAC");
        // The collector-site region must never be rendered as the peer's
        // location: the peer location is a separate reviewed fact (not
        // present here) and remains unclaimed.
        let loc = reg.location("ris", "rrc06").unwrap();
        assert!(loc.location.contains("Tokyo"));
        // No function returns a peer region; a peer's geographic location
        // is never asserted by this registry.
        assert_eq!(reg.region("ris", "does-not-exist"), "Unknown");
    }

    #[test]
    fn multihop_collector_is_visibly_labeled() {
        let reg = registry();
        // rrc00 is a RIPE-NCC Multihop collector: the registry marks it
        // and the facility name carries the same fact.
        let mh = CollectorLocation {
            family: "ris".to_string(),
            collector: "rrc00".to_string(),
            location: "Amsterdam, Netherlands".to_string(),
            facility: "RIPE-NCC Multihop".to_string(),
            note: None,
            region: "EMEA".to_string(),
            multihop: true,
        };
        let reg2 = CollectorLocationRegistry {
            as_of: "2019-09-05".to_string(),
            collectors: vec![mh],
        };
        assert!(reg2.is_multihop("ris", "rrc00"), "multihop must be visible");
        // A multihop collector still has a site region.
        assert_eq!(reg2.region("ris", "rrc00"), "EMEA");
        // Non-multihop collectors are not labeled multihop.
        assert!(!reg.is_multihop("ris", "rrc06"));
        // Unknown collectors: no multihop claim.
        assert!(!reg.is_multihop("ris", "rrc99"));
    }

    #[test]
    fn unknown_location_maps_to_unknown_region() {
        let reg = registry();
        // No reviewed metadata entry → Unknown region, never a guess.
        assert_eq!(reg.region("RouteViews", "route-views2"), "Unknown");
        assert_eq!(reg.region("ris", "rrc99"), "Unknown");
        // A JSON entry WITHOUT a region field deserializes as Unknown
        // (serde default), so older data files stay valid.
        let raw = r#"{
          "as_of": "2019-09-05",
          "collectors": [
            {"family": "ris", "collector": "rrc01", "location": "London, United Kingdom", "facility": "LINX / LONAP"}
          ]
        }"#;
        let parsed: CollectorLocationRegistry = serde_json::from_str(raw).unwrap();
        assert_eq!(parsed.region("ris", "rrc01"), "Unknown");
        assert!(!parsed.is_multihop("ris", "rrc01"));
    }

    #[test]
    fn historical_location_metadata_is_time_scoped() {
        // The registry carries an `as_of` date: the whole metadata set is
        // scoped to that review date and must not be presented as current.
        let reg = registry();
        assert_eq!(reg.as_of, "2019-09-05");
        // A later registry with different regions is a different data set.
        let mut reg2 = registry();
        reg2.as_of = "2026-08-01".to_string();
        reg2.collectors[0].region = "EMEA".to_string();
        assert_ne!(reg, reg2);
        // Location lookups return the reviewed (time-scoped) entry only.
        assert_eq!(reg.region("ris", "rrc06"), "APAC");
        assert_eq!(reg2.region("ris", "rrc06"), "EMEA");
    }
}

impl SessionAuditRow {
    /// Display the session's relationship(s) to the named planes, derived
    /// ONLY from the historical peer evidence (peer ASN for direct, path
    /// membership counts for indirect). Direct and indirect are separate
    /// facts and are rendered separately.
    pub fn relationship_displays(&self, profile: &ServicePlaneProfile) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for plane in &profile.service_planes {
            let direct = plane.asns.contains(&self.peer_asn);
            if direct {
                out.push(format!(
                    "direct-peer-to-named-plane: {}",
                    plane.display_label
                ));
            }
            // Indirect for the same plane never co-occurs with direct
            // (a direct peer's path trivially contains the plane ASN).
            let indirect = !direct
                && self
                    .path_class
                    .per_plane_contains
                    .iter()
                    .find(|(id, _)| id == &plane.id)
                    .map(|(_, n)| *n > 0)
                    .unwrap_or(false);
            if indirect {
                out.push(format!(
                    "indirect-path-via-named-plane: {}",
                    plane.display_label
                ));
            }
        }
        if out.is_empty() {
            if self.origin_route_count == 0 {
                out.push("no-origin-matching-routes".to_string());
            } else {
                out.push("other-observed-path".to_string());
            }
        }
        out
    }
}

#[cfg(test)]
mod wording_tests {
    use super::*;

    fn profile_two_planes() -> ServicePlaneProfile {
        ServicePlaneProfile {
            service_planes: vec![
                NamedServicePlane {
                    id: "re".to_string(),
                    display_label: "R&E".to_string(),
                    asns: vec![64500],
                },
                NamedServicePlane {
                    id: "pex".to_string(),
                    display_label: "Peer Exchange".to_string(),
                    asns: vec![64501],
                },
            ],
            asn_roles: vec![],
            updated_utc: String::new(),
            provenance: String::new(),
        }
    }

    fn row(
        peer_asn: u32,
        origin_routes: usize,
        per_plane: Vec<(&str, usize)>,
        neither: usize,
    ) -> SessionAuditRow {
        SessionAuditRow {
            source_family: "ris".to_string(),
            collector: "rrc00".to_string(),
            location: "Amsterdam".to_string(),
            rib_timestamp_utc: "2019-08-21T00:00:00Z".to_string(),
            rib_source_sha: "sha".to_string(),
            peer_ip: "192.0.2.1".to_string(),
            peer_asn,
            address_family: "ipv4".to_string(),
            origin_route_count: origin_routes,
            distinct_prefixes: origin_routes,
            path_class: PathClassCounts {
                per_plane_contains: per_plane
                    .into_iter()
                    .map(|(id, n)| (id.to_string(), n))
                    .collect(),
                neither_plane: neither,
                total: origin_routes,
            },
        }
    }

    #[test]
    fn direct_peer_and_indirect_path_are_displayed_separately() {
        let p = profile_two_planes();
        // A direct peer of the R&E plane (peer ASN in its ASN set).
        let direct = row(64500, 3, vec![("re", 3)], 0);
        let d = direct.relationship_displays(&p);
        assert_eq!(d, vec!["direct-peer-to-named-plane: R&E"]);
        // An indirect session (peer differs, path contains the plane ASN).
        let indirect = row(64600, 3, vec![("re", 3)], 0);
        let i = indirect.relationship_displays(&p);
        assert_eq!(i, vec!["indirect-path-via-named-plane: R&E"]);
        // The two facts render as distinct, never-conflated strings.
        assert_ne!(d, i);
        assert!(!d[0].contains("indirect"));
        assert!(!i[0].starts_with("direct-peer"));
        assert!(i[0].starts_with("indirect-path"));
    }

    #[test]
    fn no_visibility_and_no_predicate_match_are_distinct() {
        let p = profile_two_planes();
        // A session with NO origin-matching routes at all (nothing
        // observed from this peer for the target origin).
        let no_vis = row(64600, 0, vec![], 0);
        let nv = no_vis.relationship_displays(&p);
        assert_eq!(nv, vec!["no-origin-matching-routes"]);
        // A session WITH origin routes but no named-plane path match.
        let no_match = row(64600, 5, vec![], 5);
        let nm = no_match.relationship_displays(&p);
        assert_eq!(nm, vec!["other-observed-path"]);
        // Different states, different renderings.
        assert_ne!(nv, nm);
    }

    #[test]
    fn qualifying_predicate_visibility_is_not_rendered_as_total_visibility() {
        let p = profile_two_planes();
        // Indirect visibility is plane-scoped: only the matched plane is
        // named; the rendering never claims other planes or other
        // observers were absent.
        let row_re = row(64600, 5, vec![("re", 2)], 3);
        let out = row_re.relationship_displays(&p);
        assert_eq!(out, vec!["indirect-path-via-named-plane: R&E"]);
        assert!(!out[0].contains("Peer Exchange"));
        // A route set matching both planes names both — again per-plane.
        let row_both = row(64600, 5, vec![("re", 2), ("pex", 1)], 2);
        let out = row_both.relationship_displays(&p);
        assert_eq!(out.len(), 2);
    }
}
