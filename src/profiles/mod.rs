//! Network profiles — network-specific interpretation rules.
//!
//! Each profile supplies: network identity, network ASNs, title parser,
//! expectation derivation, target predicate construction, reviewed
//! participant mappings, and supported verdict semantics.
//!
//! This is a simple enum-based dispatch, not a general plugin framework.

use serde::{Deserialize, Serialize};

use crate::domain::expectation::ImpactExpectation;
use crate::sources::grnoc::GrnocRecord;

pub mod internet2;

/// The network profile to apply when interpreting an event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum NetworkProfile {
    /// Internet2 (AS11537) — parenthesized site-code convention.
    #[serde(alias = "internet2")]
    #[default]
    Internet2,
    /// Indiana GigaPOP — no parenthesized convention confirmed.
    #[serde(alias = "indiana-gigapop")]
    IndianaGigaPop,
}

/// Profile-supplied result.
pub struct ProfileContext {
    /// The derived impact expectation.
    pub expectation: ImpactExpectation,
    /// Origin ASNs to filter for.
    pub origin_asns: Vec<u32>,
    /// The required transit ASN (Internet2 or equivalent).
    pub transit_asn: u32,
    /// Warmup minutes.
    pub warmup_minutes: i64,
    /// Cooldown minutes.
    pub cooldown_minutes: i64,
    /// Suggested collectors.
    pub collectors: Vec<String>,
}

/// Derive the profile context from a GRNOC record and profile.
pub fn apply_profile(record: &GrnocRecord, profile: NetworkProfile) -> ProfileContext {
    match profile {
        NetworkProfile::Internet2 => internet2::apply(record),
        NetworkProfile::IndianaGigaPop => crate::profiles::internet2::apply_indiana_gigapop(record),
    }
}
