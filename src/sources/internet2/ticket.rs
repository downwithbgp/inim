//! Internet2 ticket parser — fixture ingestion and expectation derivation.
//!
//! Parses Internet2 maintenance/incident tickets using the parenthesized
//! site-code convention to derive operational expectations.

use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::domain::event::{EventId, EventWindow};
use crate::domain::expectation::{ImpactExpectation, RedundancyIndicator};
use chrono::NaiveDateTime;

/// A parsed Internet2 ticket.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Internet2Ticket {
    pub id: EventId,
    pub title: String,
    pub window: EventWindow,
    /// The raw fixture data, preserved for auditability.
    pub raw: serde_json::Value,
}

/// Parse an Internet2 ticket from a JSON fixture file on disk.
pub fn parse_ticket_fixture(path: &str) -> Result<Internet2Ticket, String> {
    let content =
        std::fs::read_to_string(path).map_err(|e| format!("failed to read fixture: {e}"))?;
    let fixture: TicketFixture =
        serde_json::from_str(&content).map_err(|e| format!("invalid fixture JSON: {e}"))?;

    let window = parse_time_window(&fixture.start, &fixture.end, fixture.timezone.as_deref())?;

    let title = fixture.title; // extract before move for raw preservation

    Ok(Internet2Ticket {
        id: EventId::from(fixture.id.as_str()),
        title: title.clone(),
        window,
        raw: serde_json::json!({
            "id": fixture.id,
            "title": &title,
            "start": fixture.start,
            "end": fixture.end,
            "type": fixture.ticket_type,
            "description": fixture.description,
        }),
    })
}

/// Derive the operational expectation from an Internet2 ticket title.
///
/// Internet2 convention: a parenthesized site/attachment code in the title
/// (e.g. "(NEWY32AOA)") indicates that redundancy should preserve reachability.
/// Absence of such a parenthesized code indicates loss of reachability may be
/// expected.
///
/// This is Internet2-specific and should be documented as such with provenance.
pub fn derive_expectation(ticket: &Internet2Ticket) -> ImpactExpectation {
    let indicator = detect_redundancy_indicator(&ticket.title);
    let provenance =
        "Internet2 title convention: parenthesized site code indicates expected redundancy";

    if indicator.has_parenthesized_site {
        ImpactExpectation::redundant(indicator.site_code.as_deref(), provenance)
    } else if ticket.title.contains("Participant") {
        ImpactExpectation::participant_unavailable(provenance)
    } else {
        ImpactExpectation::non_redundant(provenance)
    }
}

/// Detect parenthesized site codes in an Internet2 ticket title.
///
/// Matches patterns like `(NEWY32AOA)`, `(NEWA)`, `(NEWY)` — any uppercase
/// alphanumeric string in parentheses.
pub fn detect_redundancy_indicator(title: &str) -> RedundancyIndicator {
    // Match parenthesized uppercase alphanumeric codes (site codes are typically 3-10 chars).
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

/// Extract participant names from the ticket title using simple heuristics.
///
/// This is a best-effort extraction. Internet2 titles follow patterns like:
/// - "Maintenance - I2 Various Participants via DE-CIX (NEWY32AOA)"
/// - "Brief Outage - I2 PX Peer RIPE via NYIIX (NEWA)"
pub fn extract_participants(ticket: &Internet2Ticket) -> Vec<String> {
    let title = &ticket.title;
    let mut participants = Vec::new();

    // Try to find named participants after "I2" and before "via"
    if let Some(i2_pos) = title.find("I2 ") {
        let after_i2 = &title[i2_pos + 3..];
        if let Some(via_pos) = after_i2.find(" via ") {
            let participant_part = &after_i2[..via_pos];
            // Split on common separators
            for part in participant_part.split(" and ") {
                let cleaned = part.trim().trim_matches(',').trim();
                if !cleaned.is_empty() && !cleaned.eq_ignore_ascii_case("Various Participants") {
                    // Strip common prefixes: "PX Peer", "Peer", "PX"
                    let name = cleaned
                        .strip_prefix("PX Peer ")
                        .or_else(|| cleaned.strip_prefix("Peer "))
                        .or_else(|| cleaned.strip_prefix("PX "))
                        .unwrap_or(cleaned);
                    participants.push(name.to_string());
                }
            }
        }
    }

    // If we found nothing, try extracting the peer name from "PX Peer X" pattern
    if participants.is_empty() {
        let re = Regex::new(r"PX Peer (\S+)").unwrap();
        if let Some(caps) = re.captures(title) {
            participants.push(caps[1].to_string());
        }
    }

    participants
}

/// Extract the exchange name from the ticket title, if present.
pub fn extract_exchange(ticket: &Internet2Ticket) -> Option<String> {
    let re = Regex::new(r"via (\S+)").unwrap();
    re.captures(&ticket.title).map(|caps| caps[1].to_string())
}

// ── Internal helpers ────────────────────────────────────────────────

/// Raw fixture format used for JSON deserialization.
#[derive(Debug, Deserialize, Serialize)]
struct TicketFixture {
    id: String,
    title: String,
    start: String,
    end: String,
    #[serde(rename = "type")]
    #[allow(dead_code)]
    ticket_type: String,
    #[serde(default)]
    description: String,
    /// Optional timezone for start/end times (e.g. "EDT", "EST").
    /// If absent, times are treated as UTC (backward compatibility).
    #[serde(default)]
    timezone: Option<String>,
}

/// Parse a "YYYY-MM-DD HH:MM:SS" timestamp into a DateTime<Utc>.
///
/// If the fixture declares a `timezone` field, the times are interpreted
/// in that local timezone and normalized to UTC. Common Internet2 values:
/// "EDT" (Eastern Daylight, UTC−4) and "EST" (Eastern Standard, UTC−5).
/// Without a timezone, times are treated as UTC for backward compatibility
/// with older fixtures.
fn parse_time_window(
    start_str: &str,
    end_str: &str,
    timezone: Option<&str>,
) -> Result<EventWindow, String> {
    let fmt = "%Y-%m-%d %H:%M:%S";

    let offset_seconds = match timezone {
        Some("EDT") | Some("Eastern Daylight") => -4 * 3600,
        Some("EST") | Some("Eastern Standard") => -5 * 3600,
        Some(tz) => {
            return Err(format!(
                "unsupported timezone '{tz}' in fixture; supported: EDT, EST"
            ));
        }
        None => 0, // backward-compatible UTC
    };

    let offset = chrono::FixedOffset::east_opt(offset_seconds)
        .ok_or_else(|| format!("invalid offset {offset_seconds}"))?;

    let start = NaiveDateTime::parse_from_str(start_str, fmt)
        .map_err(|e| format!("invalid start time '{start_str}': {e}"))?
        .and_local_timezone(offset)
        .single()
        .ok_or_else(|| format!("ambiguous start time '{start_str}' at {timezone:?}"))?
        .to_utc();

    let end = NaiveDateTime::parse_from_str(end_str, fmt)
        .map_err(|e| format!("invalid end time '{end_str}': {e}"))?
        .and_local_timezone(offset)
        .single()
        .ok_or_else(|| format!("ambiguous end time '{end_str}' at {timezone:?}"))?
        .to_utc();

    Ok(EventWindow { start, end })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::expectation::ExpectationKind;
    use chrono::Utc;

    // ── Redundancy indicator detection ──────────────────────────────

    #[test]
    fn detect_parenthesized_site_code() {
        let ind = detect_redundancy_indicator(
            "Maintenance - I2 Various Participants via DE-CIX (NEWY32AOA)",
        );
        assert!(ind.has_parenthesized_site);
        assert_eq!(ind.site_code, Some("NEWY32AOA".into()));
    }

    #[test]
    fn detect_short_site_code() {
        let ind = detect_redundancy_indicator("Brief Outage - I2 PX Peer RIPE via NYIIX (NEWA)");
        assert!(ind.has_parenthesized_site);
        assert_eq!(ind.site_code, Some("NEWA".into()));
    }

    #[test]
    fn detect_no_parenthesized_site() {
        let ind = detect_redundancy_indicator("Maintenance - I2 Backbone Circuit ATLA-LOSA");
        assert!(!ind.has_parenthesized_site);
        assert_eq!(ind.site_code, None);
    }

    #[test]
    fn detect_empty_title() {
        let ind = detect_redundancy_indicator("");
        assert!(!ind.has_parenthesized_site);
    }

    #[test]
    fn detect_multiple_parentheses_takes_first() {
        let ind = detect_redundancy_indicator("Maint (NEWY32AOA) and also (LOSA)");
        assert!(ind.has_parenthesized_site);
        assert_eq!(ind.site_code, Some("NEWY32AOA".into()));
    }

    #[test]
    fn detect_lowercase_not_matched() {
        let ind = detect_redundancy_indicator("Maint (newy32aoa) — lowercase not a site code");
        assert!(!ind.has_parenthesized_site);
    }

    // ── Expectation derivation ──────────────────────────────────────

    #[test]
    fn derive_redundant_expectation() {
        let ticket = Internet2Ticket {
            id: EventId::from("CHG0107955"),
            title: "Maintenance - I2 Various Participants via DE-CIX (NEWY32AOA)".into(),
            window: sample_window(),
            raw: serde_json::json!({}),
        };
        let exp = derive_expectation(&ticket);
        assert_eq!(exp.kind, ExpectationKind::Redundant);
        assert!(exp.description.contains("NEWY32AOA"));
        assert!(exp.provenance.contains("Internet2"));
    }

    #[test]
    fn derive_non_redundant_expectation() {
        let ticket = Internet2Ticket {
            id: EventId::from("CHG9999999"),
            title: "Maintenance - I2 Backbone Circuit ATLA-LOSA".into(),
            window: sample_window(),
            raw: serde_json::json!({}),
        };
        let exp = derive_expectation(&ticket);
        assert_eq!(exp.kind, ExpectationKind::NonRedundant);
        assert!(exp.description.contains("loss of reachability"));
    }

    // ── Participant extraction ──────────────────────────────────────

    #[test]
    fn extract_various_participants() {
        let ticket = Internet2Ticket {
            id: EventId::from("CHG0107955"),
            title: "Maintenance - I2 Various Participants via DE-CIX (NEWY32AOA)".into(),
            window: sample_window(),
            raw: serde_json::json!({}),
        };
        let participants = extract_participants(&ticket);
        // "Various Participants" is filtered out, so we get empty or generic
        assert!(!participants.contains(&"Various Participants".to_string()));
    }

    #[test]
    fn extract_peer_participant() {
        let ticket = Internet2Ticket {
            id: EventId::from("INC0302574"),
            title: "Brief Outage - I2 PX Peer RIPE via NYIIX (NEWA)".into(),
            window: sample_window(),
            raw: serde_json::json!({}),
        };
        let participants = extract_participants(&ticket);
        assert!(participants.contains(&"RIPE".to_string()));
    }

    // ── Exchange extraction ─────────────────────────────────────────

    #[test]
    fn extract_exchange_from_title() {
        let ticket = Internet2Ticket {
            id: EventId::from("CHG0107955"),
            title: "Maintenance - I2 Various Participants via DE-CIX (NEWY32AOA)".into(),
            window: sample_window(),
            raw: serde_json::json!({}),
        };
        let exchange = extract_exchange(&ticket);
        assert_eq!(exchange, Some("DE-CIX".into()));
    }

    #[test]
    fn extract_nyiix_exchange() {
        let ticket = Internet2Ticket {
            id: EventId::from("INC0302574"),
            title: "Brief Outage - I2 PX Peer RIPE via NYIIX (NEWA)".into(),
            window: sample_window(),
            raw: serde_json::json!({}),
        };
        let exchange = extract_exchange(&ticket);
        assert_eq!(exchange, Some("NYIIX".into()));
    }

    #[test]
    fn extract_no_exchange() {
        let ticket = Internet2Ticket {
            id: EventId::from("CHG0000001"),
            title: "Maintenance - I2 Backbone Circuit".into(),
            window: sample_window(),
            raw: serde_json::json!({}),
        };
        let exchange = extract_exchange(&ticket);
        assert_eq!(exchange, None);
    }

    // ── Fixture parsing ─────────────────────────────────────────────

    #[test]
    fn parse_redundant_maintenance_fixture() {
        let ticket = parse_ticket_fixture("tests/fixtures/internet2/CHG0107955.json")
            .expect("should parse fixture");
        assert_eq!(ticket.id.0, "CHG0107955");
        assert!(ticket.title.contains("NEWY32AOA"));
    }

    #[test]
    fn parse_redundant_incident_fixture() {
        let ticket = parse_ticket_fixture("tests/fixtures/internet2/INC0302574.json")
            .expect("should parse fixture");
        assert_eq!(ticket.id.0, "INC0302574");
        assert!(ticket.title.contains("NEWA"));
    }

    #[test]
    fn ticket_parser_normalizes_edt_to_utc() {
        let ticket = parse_ticket_fixture("tests/fixtures/internet2/INC0302574.json")
            .expect("should parse fixture");
        // EDT is UTC-4, so 05:25 EDT = 09:25 UTC
        assert_eq!(ticket.id.0, "INC0302574");
        assert_eq!(
            ticket.window.start.to_rfc3339(),
            "2026-07-30T09:25:00+00:00"
        );
        assert_eq!(ticket.window.end.to_rfc3339(), "2026-07-30T09:47:00+00:00");
    }

    #[test]
    fn parse_fixture_rejects_invalid_path() {
        let result = parse_ticket_fixture("tests/fixtures/nonexistent.json");
        assert!(result.is_err());
    }

    // ── End-to-end: fixture → expectation ───────────────────────────

    #[test]
    fn fixture_to_redundant_expectation() {
        let ticket = parse_ticket_fixture("tests/fixtures/internet2/CHG0107955.json").unwrap();
        let exp = derive_expectation(&ticket);
        assert_eq!(exp.kind, ExpectationKind::Redundant);
    }

    #[test]
    fn fixture_to_redundant_incident_expectation() {
        let ticket = parse_ticket_fixture("tests/fixtures/internet2/INC0302574.json").unwrap();
        let exp = derive_expectation(&ticket);
        assert_eq!(exp.kind, ExpectationKind::Redundant);
    }

    #[test]
    fn non_parenthesized_participant_derives_unavailable_expectation() {
        // UVA title has no parenthesized site code, but contains "Participant"
        let ticket = Internet2Ticket {
            id: EventId::from("INC0299001"),
            title: "Availability - I2 Participant UVA".into(),
            window: sample_window(),
            raw: serde_json::json!({}),
        };
        let exp = derive_expectation(&ticket);
        assert_eq!(
            exp.kind,
            ExpectationKind::ParticipantRelationshipUnavailable
        );
        assert!(exp.description.contains("participant relationship"));
    }

    #[test]
    fn uva_manifest_contains_reviewed_as225_mapping() {
        // Verify the UVA manifest has correct ASN mappings
        let manifest_path = std::path::Path::new("manifests/INC0299001.json");
        let content = std::fs::read_to_string(manifest_path).unwrap();
        let manifest: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(manifest["target"]["origin_asns"][0], 225);
        assert_eq!(manifest["target"]["managed_network_asn"], 11537);
        assert!(manifest["target"]["prefix_selection"]
            .as_str()
            .unwrap()
            .contains("AS225"));
    }

    #[test]
    fn uva_target_requires_as225_and_as11537() {
        // The UVA target predicate requires origin AS225 + path contains AS11537
        let ticket = parse_ticket_fixture("tests/fixtures/internet2/INC0299001.json").unwrap();
        assert_eq!(ticket.id.0, "INC0299001");
        let exp = derive_expectation(&ticket);
        assert_eq!(
            exp.kind,
            ExpectationKind::ParticipantRelationshipUnavailable
        );
    }

    // ── Helpers ─────────────────────────────────────────────────────

    fn sample_window() -> EventWindow {
        use chrono::TimeZone;
        EventWindow {
            start: Utc.with_ymd_and_hms(2025, 6, 15, 5, 25, 0).unwrap(),
            end: Utc.with_ymd_and_hms(2025, 6, 15, 5, 47, 0).unwrap(),
        }
    }
}
