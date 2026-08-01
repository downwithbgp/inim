//! Internet2 profile — parenthesized site-code convention via shared GRNOC
//! convention layer.
//!
//! Internet2 GRNOC titles use a trailing parenthesized site/attachment code
//! to indicate expected redundancy. This interpretation is supplied by the
//! shared `conventions::grnoc` module.

use crate::conventions::grnoc::{self, NamedEntityType};
use crate::domain::expectation::ImpactExpectation;
use crate::sources::grnoc::GrnocRecord;

use super::ProfileContext;

/// Apply the Internet2 profile to a GRNOC record.
pub fn apply(record: &GrnocRecord) -> ProfileContext {
    let interp = grnoc::interpret(&record.short_description);
    let provenance =
        "Internet2 (GRNOC): trailing parenthesized site code indicates expected redundancy";

    let expectation = match interp.redundancy_expected {
        Some(true) => ImpactExpectation::redundant(
            interp
                .attachment_qualifier
                .as_ref()
                .map(|q| q.normalized.as_str()),
            provenance,
        ),
        Some(false) => match interp.named_entity_type {
            Some(NamedEntityType::Participant) => {
                ImpactExpectation::participant_unavailable(provenance)
            }
            Some(NamedEntityType::Peer) => {
                ImpactExpectation::peer_relationship_unavailable(provenance)
            }
            _ => ImpactExpectation::non_redundant(provenance),
        },
        None => {
            // Ambiguous — fall back to Unknown
            ImpactExpectation::unknown(provenance)
        }
    };

    ProfileContext {
        expectation,
        origin_asns: vec![], // caller fills from manifest
        transit_asn: 11537,
        warmup_minutes: 60,
        cooldown_minutes: 60,
        collectors: vec!["route-views2".into(), "route-views6".into()],
    }
}

/// Apply the Indiana GigaPOP profile.
///
/// Indiana GigaPOP uses the shared GRNOC naming convention. No trailing
/// parenthesized code → peer relationship may be unavailable.
/// Network/transit ASN must be reviewed before real analysis.
pub fn apply_indiana_gigapop(record: &GrnocRecord) -> ProfileContext {
    let interp = grnoc::interpret(&record.short_description);
    let provenance =
        "Indiana GigaPOP (GRNOC): trailing parenthesized site code indicates expected redundancy";

    let expectation = match interp.redundancy_expected {
        Some(true) => ImpactExpectation::redundant(
            interp
                .attachment_qualifier
                .as_ref()
                .map(|q| q.normalized.as_str()),
            provenance,
        ),
        Some(false) => {
            // For Indiana GigaPOP, no qualifier → peer unavailable, open event
            ImpactExpectation::peer_relationship_unavailable(provenance)
        }
        None => ImpactExpectation::unknown(provenance),
    };

    ProfileContext {
        expectation,
        origin_asns: vec![], // caller fills from manifest
        transit_asn: 0,      // Indiana GigaPOP transit ASN — pending review
        warmup_minutes: 60,
        cooldown_minutes: 60,
        collectors: vec!["route-views2".into(), "route-views6".into()],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redundant_with_site_code() {
        let interp = grnoc::interpret("Brief Outage - I2 PX Peer RIPE via NYIIX (NEWA)");
        assert_eq!(interp.redundancy_expected, Some(true));
        assert_eq!(interp.attachment_qualifier.unwrap().normalized, "NEWA");
    }

    #[test]
    fn participant_without_site_code() {
        let interp = grnoc::interpret("Availability - I2 Participant UVA");
        assert_eq!(interp.redundancy_expected, Some(false));
        assert_eq!(interp.named_entity_type, Some(NamedEntityType::Participant));
    }

    #[test]
    fn i2_profile_redundant() {
        let record = GrnocRecord {
            number: "INC0302574".into(),
            task_type: "incident".into(),
            short_description: "Brief Outage - I2 PX Peer RIPE via NYIIX (NEWA)".into(),
            category: "".into(),
            start: "2026-07-30 05:25:00".into(),
            end: Some("2026-07-30 05:47:00".into()),
            opened: None,
            state: "Closed".into(),
            priority: "High".into(),
            description: "".into(),
            source_url: "".into(),
            timezone: Some("EDT".into()),
            state_code: None,
            priority_code: None,
            planned_start: None,
            planned_end: None,
            maintenance_type: None,
            notification_text: None,
        };
        let ctx = apply(&record);
        assert_eq!(ctx.transit_asn, 11537);
    }

    #[test]
    fn i2_profile_participant_unavailable() {
        let record = GrnocRecord {
            number: "INC0299001".into(),
            task_type: "incident".into(),
            short_description: "Availability - I2 Participant UVA".into(),
            category: "".into(),
            start: "2026-07-14 02:35:00".into(),
            end: Some("2026-07-14 03:56:00".into()),
            opened: None,
            state: "Closed".into(),
            priority: "High".into(),
            description: "".into(),
            source_url: "".into(),
            timezone: Some("EDT".into()),
            state_code: None,
            priority_code: None,
            planned_start: None,
            planned_end: None,
            maintenance_type: None,
            notification_text: None,
        };
        let ctx = apply(&record);
        use crate::domain::expectation::ExpectationKind;
        assert_eq!(
            ctx.expectation.kind,
            ExpectationKind::ParticipantRelationshipUnavailable
        );
    }

    #[test]
    fn gigapop_peer_unavailable() {
        let record = GrnocRecord {
            number: "INC0301970".into(),
            task_type: "Incident".into(),
            short_description: "Outage - Indiana GigaPOP Peer Smithville".into(),
            category: "".into(),
            start: "2026-07-28T04:35:00Z".into(),
            end: None,
            opened: Some("2026-07-28T04:56:00Z".into()),
            state: "In Progress".into(),
            priority: "Moderate".into(),
            description: "".into(),
            source_url: "".into(),
            timezone: None,
            state_code: None,
            priority_code: None,
            planned_start: None,
            planned_end: None,
            maintenance_type: None,
            notification_text: None,
        };
        let ctx = apply_indiana_gigapop(&record);
        use crate::domain::expectation::ExpectationKind;
        assert_eq!(
            ctx.expectation.kind,
            ExpectationKind::PeerRelationshipUnavailable
        );
    }
}
