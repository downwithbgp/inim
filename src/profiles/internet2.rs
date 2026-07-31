//! Internet2 profile — parenthesized site-code convention.
//!
//! Internet2 GRNOC titles use a parenthesized site/attachment code
//! (e.g. "(NEWY32AOA)") to indicate expected redundancy. Absence of
//! such a code, combined with a "Participant" keyword, indicates a
//! participant relationship unavailability may be expected.
//!
//! Also contains the Indiana GigaPOP profile (small enough to colocate).

use regex::Regex;

use crate::domain::expectation::ImpactExpectation;
use crate::sources::grnoc::GrnocRecord;

use super::ProfileContext;

/// Apply the Internet2 profile to a GRNOC record.
pub fn apply(record: &GrnocRecord) -> ProfileContext {
    let indicator = detect_redundancy_indicator(&record.short_description);
    let provenance =
        "Internet2 title convention: parenthesized site code indicates expected redundancy";

    let expectation = if indicator.has_parenthesized_site {
        ImpactExpectation::redundant(indicator.site_code.as_deref(), provenance)
    } else if record.short_description.contains("Participant") {
        ImpactExpectation::participant_unavailable(provenance)
    } else {
        ImpactExpectation::non_redundant(provenance)
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
/// **Important:** Indiana GigaPOP does NOT use the parenthesized site-code
/// convention. Events derive `OpenEvent` expectation. Network/transit ASN
/// set must be reviewed before real analysis.
pub fn apply_indiana_gigapop(_record: &GrnocRecord) -> ProfileContext {
    let provenance =
        "Indiana GigaPOP: no parenthesized site-code convention; open-event expectation";

    ProfileContext {
        expectation: ImpactExpectation::open_event(provenance),
        origin_asns: vec![], // caller fills from manifest
        transit_asn: 0,      // Indiana GigaPOP transit ASN — pending review
        warmup_minutes: 60,
        cooldown_minutes: 60,
        collectors: vec!["route-views2".into(), "route-views6".into()],
    }
}

/// Redundancy indicator detected from the title.
#[derive(Debug, Clone)]
pub struct RedundancyIndicator {
    pub has_parenthesized_site: bool,
    pub site_code: Option<String>,
}

/// Detect parenthesized site codes in an Internet2 ticket title.
pub fn detect_redundancy_indicator(title: &str) -> RedundancyIndicator {
    let re = Regex::new(r"\(([A-Z][A-Z0-9]{2,9})\)").unwrap();
    if let Some(caps) = re.captures(title) {
        RedundancyIndicator {
            has_parenthesized_site: true,
            site_code: Some(caps[1].to_string()),
        }
    } else {
        RedundancyIndicator {
            has_parenthesized_site: false,
            site_code: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redundant_with_site_code() {
        let ind = detect_redundancy_indicator("Brief Outage - I2 PX Peer RIPE via NYIIX (NEWA)");
        assert!(ind.has_parenthesized_site);
        assert_eq!(ind.site_code, Some("NEWA".into()));
    }

    #[test]
    fn participant_without_site_code() {
        let ind = detect_redundancy_indicator("Availability - I2 Participant UVA");
        assert!(!ind.has_parenthesized_site);
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
        };
        let ctx = apply(&record);
        use crate::domain::expectation::ExpectationKind;
        assert_eq!(
            ctx.expectation.kind,
            ExpectationKind::ParticipantRelationshipUnavailable
        );
    }
}
