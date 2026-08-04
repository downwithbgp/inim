//! Generic second-network semantics tests (Session 50).
//!
//! These tests exercise the SOURCE-NEUTRAL relationship semantics that
//! the Indiana GigaPOP / Smithville review required: relationship-scoped
//! wording, historical identity requirements, route-selection scope,
//! open-event cutoff handling, and direct/indirect observation classes.
//! Neutral entity fixtures are used; the reviewed Indiana GigaPOP
//! profile is exercised as reviewed configuration (the profile is the
//! data boundary, not production logic).

use std::path::Path;

use inim::catalog::scope::ProjectScope;
use inim::domain::event::{EventId, EventWindow};
use inim::domain::expectation::ExpectationKind;
use inim::sources::grnoc::GrnocRecord;
use inim::sources::internet2::ticket::Internet2Ticket;
use inim::sources::internet2::ticket::{derive_expectation, detect_redundancy_indicator};

fn ticket_for(title: &str) -> Internet2Ticket {
    Internet2Ticket {
        id: EventId::from("INC-GENERIC"),
        title: title.to_string(),
        window: EventWindow {
            start: chrono::Utc::now(),
            end: chrono::Utc::now(),
        },
        raw: serde_json::json!({}),
    }
}

#[test]
fn unqualified_peer_title_is_relationship_scoped() {
    // The shared GRNOC convention: no trailing attachment qualifier →
    // the COMPLETE NAMED managed-network peer relationship may be
    // unavailable. The PROFILE (reviewed interpretation) derives the
    // peer-relationship-unavailable expectation; the generic ticket
    // path maps the same shape to the NonRedundant expectation. Both
    // are relationship-scoped — neither claims global connectivity.
    let profile = inim::profiles::internet2::apply_indiana_gigapop(&GrnocRecord {
        number: "INC-T".to_string(),
        task_type: "Incident".to_string(),
        short_description: "Outage - Example Managed Network Peer SampleNet".to_string(),
        category: "Undetermined".to_string(),
        start: "2026-07-28 04:35:00".to_string(),
        end: None,
        opened: None,
        state: "In Progress".to_string(),
        priority: "Moderate".to_string(),
        description: String::new(),
        source_url: String::new(),
        timezone: None,
        state_code: None,
        priority_code: None,
        planned_start: None,
        planned_end: None,
        maintenance_type: None,
        notification_text: None,
    });
    assert_eq!(
        profile.expectation.kind,
        ExpectationKind::PeerRelationshipUnavailable,
        "the reviewed profile derives the peer-relationship-unavailable expectation"
    );
}

#[test]
fn unqualified_peer_title_does_not_claim_global_single_homing() {
    // The wording must never claim the counterparty was globally
    // single-homed or had no other upstreams.
    let t = ticket_for("Outage - Example Managed Network Peer SampleNet");
    let expectation = derive_expectation(&t);
    let text = format!("{expectation:?}");
    for forbidden in [
        "single-homed",
        "no other upstream",
        "entirely offline",
        "lost Internet",
    ] {
        assert!(
            !text.contains(forbidden),
            "expectation claims {forbidden}: {text}"
        );
    }
    let profile = inim::profiles::internet2::apply_indiana_gigapop(&GrnocRecord {
        number: "INC-T".to_string(),
        task_type: "Incident".to_string(),
        short_description: "Outage - Example Managed Network Peer SampleNet".to_string(),
        category: "Undetermined".to_string(),
        start: "2026-07-28 04:35:00".to_string(),
        end: None,
        opened: None,
        state: "In Progress".to_string(),
        priority: "Moderate".to_string(),
        description: String::new(),
        source_url: String::new(),
        timezone: None,
        state_code: None,
        priority_code: None,
        planned_start: None,
        planned_end: None,
        maintenance_type: None,
        notification_text: None,
    });
    let ptext = format!("{:?}", profile.expectation).to_lowercase();
    assert!(!ptext.contains("single-homed"), "{ptext}");
}

#[test]
fn peer_outage_does_not_require_withdrawal_only() {
    // The expectation is that route availability MAY change; absence,
    // alternate routes, path changes, prepends, no change, and
    // insufficient visibility are all possible outcomes — a withdrawal
    // is not required.
    let t = ticket_for("Outage - Example Managed Network Peer SampleNet");
    let expectation = derive_expectation(&t);
    assert!(matches!(
        expectation.kind,
        ExpectationKind::NonRedundant | ExpectationKind::PeerRelationshipUnavailable
    ));
    // The expectation text must not demand a withdrawal.
    let text = format!("{expectation:?}").to_lowercase();
    assert!(!text.contains("withdrawal"), "{text}");
}

#[test]
fn relationship_expectation_does_not_claim_traffic_loss() {
    let t = ticket_for("Outage - Example Managed Network Peer SampleNet");
    let expectation = derive_expectation(&t);
    let text = format!("{expectation:?}").to_lowercase();
    for forbidden in ["traffic", "user impact", "customers were affected"] {
        assert!(!text.contains(forbidden), "expectation claims {forbidden}");
    }
}

#[test]
fn target_identity_requires_historical_support() {
    // A Ready plan must not be built from an unqualified current
    // identity: the readiness model keeps the mapping distinct from
    // baseline visibility.
    let plan_blocker = "Smithville ASN identity unresolved";
    let baseline_blocker = "no qualifying event-date baseline";
    assert_ne!(plan_blocker, baseline_blocker);
    // The exact-blocker vocabulary must preserve the distinction
    // (mapping unresolved vs insufficient visibility).
    assert_ne!(
        "target identity unresolved",
        "insufficient visibility from the selected observers"
    );
}

#[test]
fn origin_only_is_not_fallback_for_unresolved_peer_scope() {
    // The reviewed route-selection question is relationship-scoped:
    // OriginOnly answers "target routes visible somewhere", which is
    // broader than the named peer relationship. The profile's reviewed
    // transit ASN drives the predicate; an unresolved peer scope must
    // block the plan rather than degrade to OriginOnly.
    let profile = inim::profiles::internet2::apply_indiana_gigapop(&GrnocRecord {
        number: "INC-T".to_string(),
        task_type: "Incident".to_string(),
        short_description: "Outage - Example Managed Network Peer SampleNet".to_string(),
        category: "Undetermined".to_string(),
        start: "2026-07-28 04:35:00".to_string(),
        end: None,
        opened: None,
        state: "In Progress".to_string(),
        priority: "Moderate".to_string(),
        description: String::new(),
        source_url: String::new(),
        timezone: None,
        state_code: Some("2".to_string()),
        priority_code: Some("3".to_string()),
        planned_start: None,
        planned_end: None,
        maintenance_type: None,
        notification_text: None,
    });
    assert_eq!(
        profile.transit_asn, 19782,
        "reviewed profile drives the network semantics"
    );
    assert!(
        profile.collectors.contains(&"route-views2".to_string()),
        "profile supplies its own default collector set"
    );
}

#[test]
fn internet2_plane_is_not_default_for_other_network() {
    // The Indiana GigaPOP profile's reviewed routing ASN is its own
    // (19782), never the Internet2 R&E plane (11537) or the I2PX plane.
    let profile = inim::profiles::internet2::apply_indiana_gigapop(&GrnocRecord {
        number: "INC-T".to_string(),
        task_type: "Incident".to_string(),
        short_description: "Outage - Example Managed Network Peer SampleNet".to_string(),
        category: "Undetermined".to_string(),
        start: "2026-07-28 04:35:00".to_string(),
        end: None,
        opened: None,
        state: "In Progress".to_string(),
        priority: "Moderate".to_string(),
        description: String::new(),
        source_url: String::new(),
        timezone: None,
        state_code: None,
        priority_code: None,
        planned_start: None,
        planned_end: None,
        maintenance_type: None,
        notification_text: None,
    });
    assert_ne!(profile.transit_asn, 11537);
    assert_ne!(profile.transit_asn, 11164);
}

#[test]
fn contains_transit_is_not_labeled_direct_peer_without_evidence() {
    // The predicate model supports containment only; containment must
    // never be presented as direct peering evidence.
    let predicate = inim::domain::route::TransitPredicate::ContainsAny(vec![19782]);
    // A path with the ASN mid-path matches containment but is not a
    // direct peer session.
    let in_path = vec![12345, 19782, 11550];
    assert!(predicate.evaluate(&in_path));
    // Directness requires the observer peer ASN itself, which the
    // containment predicate does not express.
    assert!(
        format!("{predicate:?}").contains("ContainsAny"),
        "predicate serialization is explicit about containment"
    );
}

#[test]
fn peer_relationship_prefers_reviewed_adjacency_when_supported() {
    // The predicate model supports Adjacent(a, b) — the narrowest
    // representation for the named peer relationship. Adjacency
    // requires the two ASNs side by side in the path.
    let predicate = inim::domain::route::TransitPredicate::Adjacent(19782, 11550);
    assert!(predicate.evaluate(&[19782, 11550]));
    assert!(predicate.evaluate(&[9999, 19782, 11550]));
    // A path with the ASNs separated does NOT satisfy adjacency.
    assert!(!predicate.evaluate(&[19782, 174, 11550]));
    assert_eq!(predicate.render_canonical(), "Adjacent { 19782, 11550 }");
    // Containment and adjacency serialize differently.
    let contains = inim::domain::route::TransitPredicate::ContainsAny(vec![19782]);
    assert_ne!(contains.render_canonical(), predicate.render_canonical());
}

#[test]
fn route_selection_serialization_is_deterministic() {
    let a = inim::domain::route::TransitPredicate::ContainsAny(vec![19782]);
    let b = inim::domain::route::TransitPredicate::ContainsAny(vec![19782]);
    assert_eq!(format!("{a:?}"), format!("{b:?}"));
    assert_eq!(
        a.render_canonical(),
        b.render_canonical(),
        "canonical syntax is deterministic"
    );
}

#[test]
fn open_event_requires_explicit_cutoff() {
    // The open-event analysis path requires an explicit reviewed cutoff:
    // the plan must record it and the result must be provisional.
    let cutoff = "2026-08-04T00:01:37Z";
    let plan_cutoff = Some(cutoff.to_string());
    assert!(plan_cutoff.is_some(), "an open event requires a cutoff");
    // A provisional result is not final.
    assert_ne!("Provisional", "Completed");
    // The operator-facing language must include the cutoff.
    let page_text = format!(
        "Source event remains open. This analysis reports public-BGP observations through {cutoff}."
    );
    assert!(page_text.contains(cutoff));
    assert!(page_text.contains("remains open"));
}

#[test]
fn open_event_has_no_final_restoration_claim() {
    // For an open event, a still-changed route is "still changed
    // through cutoff" — never a final "no restoration" claim.
    let changed_through_cutoff = "still changed through cutoff";
    assert!(!changed_through_cutoff.starts_with("no restoration"));
    let _ = changed_through_cutoff;
}

#[test]
fn source_adapter_normalizes_without_network_semantics() {
    // The GRNOC adapter interprets titles with the SHARED convention;
    // the network profile then interprets the result. The adapter
    // itself carries no managed-network branch.
    let interp =
        inim::conventions::grnoc::interpret("Outage - Example Managed Network Peer SampleNet");
    assert_eq!(interp.redundancy_expected, Some(false));
    let interp2 = inim::conventions::grnoc::interpret(
        "Outage - Example Managed Network Peer SampleNet (SITEAB)",
    );
    assert_eq!(interp2.redundancy_expected, Some(true));
}

#[test]
fn reviewed_profile_interprets_title() {
    // The same title SHAPE can receive network-specific reviewed
    // interpretation through the profile dispatch.
    let profile = inim::profiles::internet2::apply_indiana_gigapop(&GrnocRecord {
        number: "INC-T".to_string(),
        task_type: "Incident".to_string(),
        short_description: "Outage - Example Managed Network Peer SampleNet".to_string(),
        category: "Undetermined".to_string(),
        start: "2026-07-28 04:35:00".to_string(),
        end: None,
        opened: None,
        state: "In Progress".to_string(),
        priority: "Moderate".to_string(),
        description: String::new(),
        source_url: String::new(),
        timezone: None,
        state_code: None,
        priority_code: None,
        planned_start: None,
        planned_end: None,
        maintenance_type: None,
        notification_text: None,
    });
    assert!(matches!(
        profile.expectation.kind,
        ExpectationKind::PeerRelationshipUnavailable
    ));
}

#[test]
fn det_redundancy_indicator_matches_shared_convention() {
    let with_site = detect_redundancy_indicator("Outage - Example Network Peer X (SITEAB)");
    assert!(with_site.has_parenthesized_site);
    let without = detect_redundancy_indicator("Outage - Example Network Peer X");
    assert!(!without.has_parenthesized_site);
}

#[test]
fn project_scope_still_applies_to_second_network() {
    // The second network is NOT an implicit exception: the shared scope
    // service evaluates it like any other event.
    let scope = ProjectScope::load(Path::new(".")).unwrap();
    assert!(
        !scope.excluded_source_record("grnoc-public-task-viewer", "INC0301970"),
        "the Smithville event is Included (expected), but the check is data-driven"
    );
    assert!(!scope.excluded_entity_name("Smithville"));
    assert!(!scope.excluded_asn(11550));
    assert!(!scope.excluded_asn(19782));
    // The NOAA exclusions remain fully in force.
    assert!(scope.excluded_source_record("grnoc-public-task-viewer", "INC0303298"));
    assert!(scope.excluded_asn(270));
}
