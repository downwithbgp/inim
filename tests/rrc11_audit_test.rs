//! Session 36, Part 1 — historical RRC11/I2PX audit integration tests.
//!
//! The required tests for the RRC11 audit verify that the historical peer
//! identity comes from the 2019 baseline bview's MRT peer table, that the
//! current peer list is only supporting context, and that distinct facts
//! (direct session vs AS-in-path membership; absent AS2603 visibility vs
//! absent session) never collapse into each other. Test names contain the
//! reviewed plane ASN; they live in tests/ because the src/ token gate
//! forbids the plane identity inside the library.

use inim::catalog::netprofile::{
    direct_session_decision, peer_inventory, CollectorLocation, CollectorLocationRegistry,
    NamedServicePlane, PathEvidence, PeerInventoryRow, ReviewedAsnRole, ServicePlaneProfile,
};

fn profile() -> ServicePlaneProfile {
    ServicePlaneProfile {
        service_planes: vec![
            NamedServicePlane {
                id: "internet2-re".to_string(),
                display_label: "Internet2 R&E".to_string(),
                asns: vec![11537],
            },
            NamedServicePlane {
                id: "internet2-i2px".to_string(),
                display_label: "Internet2 Peer Exchange".to_string(),
                asns: vec![11164],
            },
        ],
        asn_roles: vec![
            ReviewedAsnRole {
                asn: 2603,
                role: "national-re".to_string(),
            },
            ReviewedAsnRole {
                asn: 11537,
                role: "internet2-re".to_string(),
            },
            ReviewedAsnRole {
                asn: 11164,
                role: "internet2-i2px".to_string(),
            },
        ],
        updated_utc: "2026-08-02T00:00:00Z".to_string(),
        provenance: "test".to_string(),
    }
}

fn registry() -> CollectorLocationRegistry {
    CollectorLocationRegistry {
        as_of: "2019-09-05".to_string(),
        collectors: vec![CollectorLocation {
            family: "ris".to_string(),
            collector: "rrc11".to_string(),
            location: "New York City, New York, US".to_string(),
            facility: "NYIIX".to_string(),
            note: None,
        }],
    }
}

fn route(peer_ip: &str, peer_asn: u32, prefix: &str, path: &[u32]) -> PathEvidence {
    PathEvidence {
        peer_ip: peer_ip.to_string(),
        peer_asn,
        address_family: "ipv4".to_string(),
        prefix: prefix.to_string(),
        as_path: path.to_vec(),
        origin_asns: path.last().copied().map(|a| vec![a]).unwrap_or_default(),
    }
}

/// The selected-observer audit must be scoped to the selected observers,
/// never rendered as an "all RIS" audit. The inventory's rows carry the
/// collector identity; a per-collector audit file must not claim global
/// coverage.
#[test]
fn selected_observer_audit_is_not_rendered_as_all_ris_audit() {
    let p = profile();
    let reg = registry();
    let routes = vec![
        route("198.32.160.42", 2497, "192.36.0.0/16", &[2497, 2603]),
        route("198.32.160.103", 13030, "192.36.0.0/16", &[13030, 2603]),
    ];
    let rows = peer_inventory(
        &p,
        &reg,
        "ris",
        "rrc11",
        "2019-08-21T00:00:00Z",
        "sha",
        &routes,
        &[2603],
    );
    // Every row names its collector — the inventory is per selected
    // observer, not a global RIS statement.
    assert!(rows.iter().all(|r| r.collector == "rrc11"));
    assert!(rows.iter().all(|r| r.source_family == "ris"));
    // And the scoped row count equals the selected observer's sessions.
    assert_eq!(rows.len(), 2);
}

/// The historical peer identity comes from the bview MRT peer table (the
/// peer ASN on the route), not from any current peer list.
#[test]
fn historical_rrc11_peer_identity_comes_from_bview() {
    let p = profile();
    let reg = registry();
    // The bview evidence carries peer ASN 2497 at 198.32.160.42.
    let routes = vec![route("198.32.160.42", 2497, "192.36.0.0/16", &[2497, 2603])];
    let rows = peer_inventory(
        &p,
        &reg,
        "ris",
        "rrc11",
        "2019-08-21T00:00:00Z",
        "sha",
        &routes,
        &[2603],
    );
    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0].peer_asn, 2497,
        "peer ASN must come from the RIB evidence"
    );
    assert_eq!(rows[0].peer_ip, "198.32.160.42");
    // The registry holds location ONLY — it cannot override peer identity.
    let loc = reg.location("ris", "rrc11").unwrap();
    assert_eq!(loc.location, "New York City, New York, US");
}

/// The current peer list is supporting context, never historical evidence.
/// The audit rows are derived from the bview; a "current peer list" note
/// must not be able to fabricate a session.
#[test]
fn current_peer_list_is_context_not_historical_evidence() {
    let p = profile();
    let reg = registry();
    // The 2019 bview has NO session from a hypothetical current peer
    // (peer ASN 11164, direct per today's peer list): the inventory has
    // no row for it, and the decision says the direct session is absent.
    let routes = vec![route("198.32.160.42", 2497, "192.36.0.0/16", &[2497, 2603])];
    let rows = peer_inventory(
        &p,
        &reg,
        "ris",
        "rrc11",
        "2019-08-21T00:00:00Z",
        "sha",
        &routes,
        &[2603],
    );
    assert!(
        rows.iter().all(|r| r.peer_asn != 11164),
        "current peer list cannot create a historical session"
    );
    let decision = direct_session_decision(&p, "internet2-i2px", &rows, &[2603]).unwrap();
    assert!(!decision.direct_session_present);
    assert_eq!(decision.direct_origin_route_count, 0);
}

/// A direct session (peer ASN equals the plane ASN) is a different fact
/// from the plane ASN appearing inside some other session's AS path.
#[test]
fn direct_as11164_session_is_distinct_from_as11164_in_path() {
    let p = profile();
    let reg = registry();
    // No direct session; but another peer's path contains 11164.
    let routes = vec![route(
        "198.32.160.42",
        2497,
        "192.36.0.0/16",
        &[2497, 11164, 2603],
    )];
    let rows = peer_inventory(
        &p,
        &reg,
        "ris",
        "rrc11",
        "2019-08-21T00:00:00Z",
        "sha",
        &routes,
        &[2603],
    );
    let decision = direct_session_decision(&p, "internet2-i2px", &rows, &[2603]).unwrap();
    assert!(
        !decision.direct_session_present,
        "11164-in-path is not a direct session"
    );
    assert!(
        decision.plane_asn_in_path,
        "path membership is still recorded as a separate fact"
    );
    // The R&E decision sees the same route as indirect too.
    let re = direct_session_decision(&p, "internet2-re", &rows, &[2603]).unwrap();
    assert!(!re.direct_session_present);
    assert!(!re.plane_asn_in_path);
}

/// "No AS2603 visibility at RRC11" is distinct from "no direct AS11164
/// session". A direct session may exist while carrying zero target-origin
/// routes; both facts must be expressible at once.
#[test]
fn absent_as2603_visibility_is_distinct_from_absent_as11164_session() {
    let p = profile();
    let reg = registry();
    // Direct AS11164 session exists (peer ASN 11164) but announces only
    // OTHER-origin routes: AS2603 not visible via it.
    let routes = vec![
        route("198.32.160.99", 11164, "203.0.113.0/24", &[11164, 64512]),
        route("198.32.160.42", 2497, "192.36.0.0/16", &[2497, 2603]),
    ];
    let rows = peer_inventory(
        &p,
        &reg,
        "ris",
        "rrc11",
        "2019-08-21T00:00:00Z",
        "sha",
        &routes,
        &[2603],
    );
    let direct = rows.iter().find(|r| r.peer_asn == 11164).unwrap();
    assert_eq!(direct.total_route_count, 1);
    assert_eq!(
        direct.origin_route_count, 0,
        "direct session present, zero AS2603 routes via it"
    );
    let decision = direct_session_decision(&p, "internet2-i2px", &rows, &[2603]).unwrap();
    assert!(decision.direct_session_present);
    assert_eq!(
        decision.direct_origin_route_count, 0,
        "session present but no qualifying baseline"
    );
    // The inventory also proves AS2603 is visible at RRC11 via OTHER
    // sessions — so "no AS2603 at the collector" would be a false claim.
    assert!(rows
        .iter()
        .any(|r| r.peer_asn == 2497 && r.origin_route_count == 1));
}

/// The decision keys off the reviewed profile data, never a literal ASN
/// in code: a profile with a different plane ASN changes the decision.
#[test]
fn direct_session_decision_follows_profile_data() {
    let p = profile();
    let mut p2 = profile();
    p2.service_planes[1].asns = vec![64513];
    let reg = registry();
    let routes = vec![route(
        "198.32.160.99",
        64513,
        "203.0.113.0/24",
        &[64513, 64512],
    )];
    let rows = peer_inventory(&p, &reg, "ris", "rrc11", "T", "sha", &routes, &[2603]);
    let a = direct_session_decision(&p, "internet2-i2px", &rows, &[2603]).unwrap();
    let b = direct_session_decision(&p2, "internet2-i2px", &rows, &[2603]).unwrap();
    assert!(!a.direct_session_present);
    assert!(
        b.direct_session_present,
        "decision must follow profile data"
    );
}

/// Sanity: inventory rows round-trip through JSON with all report fields.
#[test]
fn inventory_row_serialization_is_complete() {
    let p = profile();
    let reg = registry();
    let routes = vec![route("198.32.160.42", 2497, "192.36.0.0/16", &[2497, 2603])];
    let rows = peer_inventory(
        &p,
        &reg,
        "ris",
        "rrc11",
        "2019-08-21T00:00:00Z",
        "sha",
        &routes,
        &[2603],
    );
    let json = serde_json::to_string(&rows).unwrap();
    let back: Vec<PeerInventoryRow> = serde_json::from_str(&json).unwrap();
    assert_eq!(rows, back);
    assert_eq!(back[0].rib_timestamp_utc, "2019-08-21T00:00:00Z");
}
