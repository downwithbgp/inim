// Sources — network-specific adapters.
pub mod grnoc;
pub mod internet2;

use crate::domain::event::EventId;
use crate::domain::expectation::ImpactExpectation;
use crate::profiles::NetworkProfile;

/// Derive the event identity and expectation from a ticket fixture.
///
/// Tries the Internet2 ticket format first (legacy fixtures), then the
/// generic GRNOC record format with the network profile inferred from the
/// title: Indiana GigaPOP titles select the IndianaGigaPop profile, all
/// other GRNOC titles use the Internet2 profile.
pub fn derive_expectation_from_fixture(path: &str) -> Result<(EventId, ImpactExpectation), String> {
    if let Ok(ticket) = internet2::ticket::parse_ticket_fixture(path) {
        let expectation = internet2::ticket::derive_expectation(&ticket);
        return Ok((ticket.id, expectation));
    }

    let record = grnoc::GrnocRecord::from_file(path)?;
    let profile = if record.short_description.contains("GigaPOP") {
        NetworkProfile::IndianaGigaPop
    } else {
        NetworkProfile::Internet2
    };
    let ctx = crate::profiles::apply_profile(&record, profile);
    Ok((EventId::from(record.number.as_str()), ctx.expectation))
}
