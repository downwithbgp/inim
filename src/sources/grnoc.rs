//! Generic GRNOC Public Task Viewer record parser.
//!
//! Parses maintenance/incident records from GRNOC-operated networks
//! (Internet2, Indiana GigaPOP, etc.). Network-specific title interpretation
//! and expectation semantics live in `profiles/`.

use chrono::{DateTime, NaiveDateTime, Utc};
use serde::{Deserialize, Serialize};

/// A generic GRNOC task record — represents the published fields
/// without network-specific interpretation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrnocRecord {
    /// Task number (e.g. "INC0302574").
    #[serde(alias = "id")]
    pub number: String,
    /// Task type: Incident, Maintenance, etc.
    #[serde(alias = "type", default)]
    pub task_type: String,
    /// Short description / title.
    #[serde(alias = "title")]
    pub short_description: String,
    /// Category (e.g. "Undetermined").
    #[serde(default)]
    pub category: String,
    /// Published start time as an ISO-8601 or "YYYY-MM-DD HH:MM:SS" string.
    pub start: String,
    /// Published end time (optional — open events have no end).
    #[serde(default)]
    pub end: Option<String>,
    /// Time the ticket was opened (optional).
    #[serde(default)]
    pub opened: Option<String>,
    /// Current state (Closed, In Progress, etc.).
    #[serde(default)]
    pub state: String,
    /// Priority (High, Moderate, etc.).
    #[serde(default)]
    pub priority: String,
    /// Full description text (optional).
    #[serde(default)]
    pub description: String,
    /// Source URL or domain for provenance.
    #[serde(default)]
    pub source_url: String,
    /// Local timezone (e.g. "EDT", "EST"). Defaults to UTC if absent.
    #[serde(default)]
    pub timezone: Option<String>,
}

impl GrnocRecord {
    /// Parse from a JSON fixture file.
    pub fn from_file(path: &str) -> Result<GrnocRecord, String> {
        let content =
            std::fs::read_to_string(path).map_err(|e| format!("failed to read fixture: {e}"))?;
        serde_json::from_str(&content).map_err(|e| format!("invalid GRNOC fixture JSON: {e}"))
    }

    /// Parse the start time into UTC.
    pub fn parse_start(&self) -> Result<DateTime<Utc>, String> {
        parse_time_to_utc(&self.start, self.timezone.as_deref())
    }

    /// Parse the optional end time into UTC.
    pub fn parse_end(&self) -> Result<Option<DateTime<Utc>>, String> {
        match &self.end {
            Some(s) if !s.is_empty() => parse_time_to_utc(s, self.timezone.as_deref()).map(Some),
            _ => Ok(None),
        }
    }

    /// Parse the optional opened time into UTC.
    pub fn parse_opened(&self) -> Result<Option<DateTime<Utc>>, String> {
        match &self.opened {
            Some(s) if !s.is_empty() => parse_time_to_utc(s, self.timezone.as_deref()).map(Some),
            _ => Ok(None),
        }
    }
}

/// Parse a local datetime string into UTC, applying a timezone offset.
///
/// Supports formats:
/// - "2026-07-30 05:25:00" (with timezone hint like "EDT")
/// - "2026-07-30T05:25:00Z" (ISO-8601)
/// - "2026-07-28T04:35:00Z" (already UTC)
fn parse_time_to_utc(s: &str, tz_hint: Option<&str>) -> Result<DateTime<Utc>, String> {
    // Try ISO-8601 with Z suffix
    if s.contains('T') && (s.ends_with('Z') || s.contains('+') || s.contains('-')) {
        return chrono::DateTime::parse_from_rfc3339(s)
            .map(|dt| dt.with_timezone(&Utc))
            .or_else(|_| {
                // Try parsing as naive + Z
                let trimmed = s.trim_end_matches('Z');
                NaiveDateTime::parse_from_str(trimmed, "%Y-%m-%dT%H:%M:%S")
                    .map(|ndt| ndt.and_utc())
                    .map_err(|e| format!("cannot parse datetime '{s}': {e}"))
            });
    }

    // Try "YYYY-MM-DD HH:MM:SS" with timezone hint
    let ndt = NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S")
        .map_err(|e| format!("cannot parse datetime '{s}': {e}"))?;

    let offset_secs = timezone_offset_secs(tz_hint);
    // Subtract offset to get UTC (EDT = UTC-4, so EDT 05:25 = UTC 09:25)
    let utc = ndt - chrono::Duration::seconds(offset_secs);
    Ok(utc.and_utc())
}

/// Map a timezone abbreviation to offset in seconds (from UTC to local).
fn timezone_offset_secs(tz: Option<&str>) -> i64 {
    match tz {
        Some("EDT") => -4 * 3600, // Eastern Daylight Time = UTC-4
        Some("EST") => -5 * 3600, // Eastern Standard Time = UTC-5
        Some("CDT") => -5 * 3600,
        Some("CST") => -6 * 3600,
        Some("MDT") => -6 * 3600,
        Some("MST") => -7 * 3600,
        Some("PDT") => -7 * 3600,
        Some("PST") => -8 * 3600,
        _ => 0, // Assume UTC
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_i2_legacy_fixture() {
        // Uses 'id' alias, 'type', 'title'
        let json = r#"{
            "id": "INC0302574",
            "type": "incident",
            "title": "Brief Outage - I2 PX Peer RIPE via NYIIX (NEWA)",
            "start": "2026-07-30 05:25:00",
            "end": "2026-07-30 05:47:00",
            "timezone": "EDT",
            "description": "test"
        }"#;
        let record: GrnocRecord = serde_json::from_str(json).unwrap();
        assert_eq!(record.number, "INC0302574");
        assert_eq!(
            record.short_description,
            "Brief Outage - I2 PX Peer RIPE via NYIIX (NEWA)"
        );
        assert_eq!(record.task_type, "incident");
    }

    #[test]
    fn parse_open_event_without_end() {
        let json = r#"{
            "number": "INC0301970",
            "task_type": "Incident",
            "short_description": "Outage - Indiana GigaPOP Peer Smithville",
            "start": "2026-07-28T04:35:00Z",
            "opened": "2026-07-28T04:56:00Z",
            "state": "In Progress",
            "priority": "Moderate",
            "source_url": "https://grnoc.example.com/tasks/INC0301970"
        }"#;
        let record: GrnocRecord = serde_json::from_str(json).unwrap();
        assert_eq!(record.number, "INC0301970");
        assert!(record.end.is_none());
        assert_eq!(record.state, "In Progress");
        assert_eq!(record.priority, "Moderate");
    }

    #[test]
    fn edt_is_converted_to_utc() {
        let dt = parse_time_to_utc("2026-07-30 05:25:00", Some("EDT")).unwrap();
        // 05:25 EDT = 09:25 UTC
        assert_eq!(
            dt.format("%Y-%m-%dT%H:%M:%S").to_string(),
            "2026-07-30T09:25:00"
        );
    }

    #[test]
    fn utc_iso_passes_through() {
        let dt = parse_time_to_utc("2026-07-28T04:35:00Z", None).unwrap();
        assert_eq!(
            dt.format("%Y-%m-%dT%H:%M:%S").to_string(),
            "2026-07-28T04:35:00"
        );
    }
}
