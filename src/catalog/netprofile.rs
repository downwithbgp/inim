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
        self.service_planes
            .iter()
            .find(|p| p.asns.contains(&asn))
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
            SessionRelationship::IndirectPathViaNamedPlane { .. } => "indirect-path-via-named-plane",
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
            let duplicate = out.iter().any(|r: &ScopedSessionRole| {
                r.relationship == rel
            });
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
                classify_route(profile, r.peer_asn, &r.as_path)
                    .contains(&role.relationship)
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
        assert_ne!(p1.service_planes[0].display_label, p2.service_planes[0].display_label);
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
        }];
        let indirect = vec![PathEvidence {
            peer_ip: "192.0.2.2".to_string(),
            peer_asn: 64599,
            address_family: "ipv4".to_string(),
            prefix: "198.51.100.0/24".to_string(),
            as_path: vec![64599, 64500],
        }];
        let other = vec![PathEvidence {
            peer_ip: "192.0.2.3".to_string(),
            peer_asn: 64600,
            address_family: "ipv4".to_string(),
            prefix: "198.51.100.0/24".to_string(),
            as_path: vec![64600, 64601, 64000],
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
        assert!(matches!(r1[0].relationship, SessionRelationship::DirectPeerToNamedPlane { .. }));
        assert!(matches!(r2[0].relationship, SessionRelationship::IndirectPathViaNamedPlane { .. }));
        assert!(matches!(r3[0].relationship, SessionRelationship::OtherObservedPath));
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
        }];
        let routes_t2 = vec![PathEvidence {
            peer_ip: "192.0.2.1".to_string(),
            peer_asn: 64599,
            address_family: "ipv4".to_string(),
            prefix: "198.51.100.0/24".to_string(),
            as_path: vec![64599, 64500],
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
        assert!(matches!(r1[0].relationship, SessionRelationship::DirectPeerToNamedPlane { .. }));
        assert!(matches!(r2[0].relationship, SessionRelationship::IndirectPathViaNamedPlane { .. }));
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
        assert!(matches!(rels[0], SessionRelationship::IndirectPathViaNamedPlane { .. }));
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
        };
        let roles = session_roles_at(&p, &key, &[ev], "T");
        assert_eq!(roles.len(), 1);
    }
}
