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
    // ── Public-viewer raw-value preservation (optional) ──────────────
    //
    // These fields preserve the exact public-task-viewer values next to
    // the generic derived fields. The viewer emits unix-epoch strings and
    // code numbers; `start`/`end`/`opened`/`state`/`priority` hold the
    // derived stable values, and these hold the raw source values so the
    // normalization is lossless. All are optional and skipped when
    // absent, so legacy fixtures keep parsing.
    /// Raw viewer state code (e.g. "2", "-5").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state_code: Option<String>,
    /// Raw viewer priority code (e.g. "3").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority_code: Option<String>,
    /// Planned window start for change requests (raw unix string).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub planned_start: Option<String>,
    /// Planned window end for change requests (raw unix string).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub planned_end: Option<String>,
    /// Maintenance type for change requests (e.g. "Hardware").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maintenance_type: Option<String>,
    /// Outgoing notification text published with the ticket.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notification_text: Option<String>,
}

// ── Public Task Viewer response envelope ───────────────────────────
//
// The GlobalNOC Ticket Viewer SPA retrieves records from undocumented
// POST endpoints (`/api/get_incidents`, `/api/get_change_requests`)
// returning `{"total": n, "result": [...]}`. See
// docs/sources/GRNOC_PUBLIC_TASK_VIEWER.md for the protocol audit.

/// The viewer JSON response envelope.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ViewerResponse {
    #[serde(default)]
    pub total: i64,
    #[serde(default)]
    pub result: Vec<ViewerRecord>,
}

impl ViewerResponse {
    /// Parse a viewer response from JSON text.
    pub fn parse_json(s: &str) -> Result<ViewerResponse, String> {
        serde_json::from_str(s).map_err(|e| format!("invalid viewer response JSON: {e}"))
    }

    /// Parse a viewer response from a fixture file.
    pub fn from_file(path: &str) -> Result<ViewerResponse, String> {
        let content =
            std::fs::read_to_string(path).map_err(|e| format!("failed to read fixture: {e}"))?;
        ViewerResponse::parse_json(&content)
    }
}

/// One record as returned by the public task viewer API.
///
/// All fields are raw source values: timestamps are unix-epoch strings
/// ("" when unset), state/priority are code strings. Unknown fields
/// added by the viewer in the future are ignored (serde default) and do
/// not corrupt normalization.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ViewerRecord {
    #[serde(alias = "id")]
    pub number: String,
    #[serde(default)]
    pub short_description: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub u_outgoing_notification_text: String,
    /// Raw state code string.
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub category: String,
    /// Unix epoch seconds as a string; "" when unset.
    #[serde(default)]
    pub work_start: String,
    /// Unix epoch seconds as a string; "" when the event is open.
    #[serde(default)]
    pub work_end: String,
    /// Unix epoch seconds as a string; "" when unset.
    #[serde(default)]
    pub opened_at: String,
    /// Raw priority code string.
    #[serde(default)]
    pub priority: String,
    // Change-request fields (absent for incidents).
    /// Planned window start (unix epoch string; CHG only).
    #[serde(default)]
    pub start_date: String,
    /// Planned window end (unix epoch string; CHG only).
    #[serde(default)]
    pub end_date: String,
    /// Maintenance type (e.g. "Hardware"; CHG only).
    #[serde(default)]
    pub u_maintenance_type: String,
}

// ── Lossless label translations (never severity scores) ────────────
//
// The viewer returns state/priority as code numbers. The maps below are
// exactly the labels the viewer itself renders; they translate a raw
// code to a stable label without computing any ordering or score.

/// Incident state codes to labels (as rendered by the viewer).
pub fn incident_state_label(code: &str) -> Option<&'static str> {
    match code {
        "1" => Some("New"),
        "2" => Some("In Progress"),
        "3" => Some("On Hold"),
        "-1" => Some("Review Needed"),
        "-170" => Some("Custodian Review"),
        "6" => Some("Resolved"),
        "7" => Some("Closed"),
        "8" => Some("Canceled"),
        _ => None,
    }
}

/// Change-request state codes to labels (as rendered by the viewer).
pub fn change_state_label(code: &str) -> Option<&'static str> {
    match code {
        "0" => Some("Review"),
        "3" => Some("Closed"),
        "4" => Some("Canceled"),
        "7" => Some("Impact Assessment"),
        "-1" => Some("Implement"),
        "-2" => Some("Scheduled"),
        "-3" => Some("Authorized"),
        "-4" => Some("Assess"),
        "-5" => Some("New"),
        "-7" => Some("Impact Assessment"),
        _ => None,
    }
}

/// Priority codes to labels (as rendered by the viewer).
pub fn priority_label(code: &str) -> Option<&'static str> {
    match code {
        "1" => Some("Critical"),
        "2" => Some("High"),
        "3" => Some("Moderate"),
        "4" => Some("Low"),
        _ => None,
    }
}

/// Task-type label from the ticket number prefix (viewer's own rule).
pub fn task_type_for_number(number: &str) -> &'static str {
    if number.to_ascii_lowercase().starts_with("inc") {
        "Incident"
    } else if number.to_ascii_lowercase().starts_with("chg") {
        "Change Request"
    } else if number.to_ascii_lowercase().starts_with("task") {
        "Task"
    } else {
        "Unknown"
    }
}

/// Convert a viewer unix-epoch string ("" when unset) to RFC3339 UTC.
fn epoch_to_rfc3339(s: &str) -> Option<String> {
    if s.is_empty() {
        return None;
    }
    let secs: i64 = s.parse().ok()?;
    chrono::DateTime::from_timestamp(secs, 0).map(|dt| dt.format("%Y-%m-%dT%H:%M:%SZ").to_string())
}

impl ViewerRecord {
    /// Convert to the generic `GnocRecord`, preserving raw viewer values
    /// in the optional raw-preservation fields.
    pub fn to_grnoc_record(&self) -> GrnocRecord {
        let is_change = self.number.to_ascii_lowercase().starts_with("chg");
        let state_label = if is_change {
            change_state_label(&self.state)
        } else {
            incident_state_label(&self.state)
        };
        GrnocRecord {
            number: self.number.clone(),
            task_type: task_type_for_number(&self.number).to_string(),
            short_description: self.short_description.clone(),
            category: self.category.clone(),
            start: epoch_to_rfc3339(&self.work_start).unwrap_or_default(),
            end: epoch_to_rfc3339(&self.work_end),
            opened: epoch_to_rfc3339(&self.opened_at),
            state: state_label.map(str::to_string).unwrap_or_else(|| {
                if self.state.is_empty() {
                    "Unknown".to_string()
                } else {
                    format!("Unknown (code {})", self.state)
                }
            }),
            priority: priority_label(&self.priority)
                .map(str::to_string)
                .unwrap_or_else(|| {
                    if self.priority.is_empty() {
                        "Unknown".to_string()
                    } else {
                        format!("Unknown (code {})", self.priority)
                    }
                }),
            description: self.description.clone(),
            source_url: format!(
                "https://ticket-viewer.grnoc.iu.edu/tickets/{}/",
                self.number
            ),
            timezone: None,
            state_code: (!self.state.is_empty()).then(|| self.state.clone()),
            priority_code: (!self.priority.is_empty()).then(|| self.priority.clone()),
            planned_start: epoch_to_rfc3339(&self.start_date),
            planned_end: epoch_to_rfc3339(&self.end_date),
            maintenance_type: (!self.u_maintenance_type.is_empty())
                .then(|| self.u_maintenance_type.clone()),
            notification_text: (!self.u_outgoing_notification_text.is_empty())
                .then(|| self.u_outgoing_notification_text.clone()),
        }
    }
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

    // ── Session 33: public-viewer response parsing (fixtures only) ──

    #[test]
    fn viewer_response_fixture_parses() {
        let resp =
            ViewerResponse::from_file("tests/fixtures/grnoc/viewer/INC0301970.json").unwrap();
        assert_eq!(resp.total, 1);
        assert_eq!(resp.result.len(), 1);
        let rec = resp.result[0].to_grnoc_record();
        assert_eq!(rec.number, "INC0301970");
        assert_eq!(rec.task_type, "Incident");
        assert_eq!(
            rec.short_description,
            "Outage - Indiana GigaPOP Peer Smithville"
        );
        assert_eq!(rec.category, "Undetermined");
        // Unix epoch work_start 1785213326 -> 2026-07-28T04:35:26Z.
        assert_eq!(rec.start, "2026-07-28T04:35:26Z");
        assert_eq!(rec.opened.as_deref(), Some("2026-07-28T04:56:38Z"));
        // State/priority are lossless label translations of the codes.
        assert_eq!(rec.state, "In Progress");
        assert_eq!(rec.state_code.as_deref(), Some("2"));
        assert_eq!(rec.priority, "Moderate");
        assert_eq!(rec.priority_code.as_deref(), Some("3"));
        assert_eq!(
            rec.source_url,
            "https://ticket-viewer.grnoc.iu.edu/tickets/INC0301970/"
        );
    }

    #[test]
    fn missing_optional_field_is_preserved_as_absent() {
        // INC0301970 has an empty work_end -> absent end (open event).
        let resp =
            ViewerResponse::from_file("tests/fixtures/grnoc/viewer/INC0301970.json").unwrap();
        let inc = resp.result[0].to_grnoc_record();
        assert!(inc.end.is_none());
        assert!(inc.planned_start.is_none());
        assert!(inc.planned_end.is_none());
        assert!(inc.maintenance_type.is_none());
        // CHG0038258 has no category field and no notification text
        // mismatch: category absent -> empty, planned window present.
        let resp =
            ViewerResponse::from_file("tests/fixtures/grnoc/viewer/CHG0038258.json").unwrap();
        let chg = resp.result[0].to_grnoc_record();
        assert_eq!(chg.category, "");
        assert_eq!(chg.state, "Closed");
        assert_eq!(chg.state_code.as_deref(), Some("3"));
        assert_eq!(chg.maintenance_type.as_deref(), Some("Hardware"));
        // Planned window 1566360000 -> 2019-08-21T04:00:00Z.
        assert_eq!(chg.planned_start.as_deref(), Some("2019-08-21T04:00:00Z"));
        assert_eq!(chg.planned_end.as_deref(), Some("2019-08-21T13:00:00Z"));
        // Actual work window differs from planned; both preserved.
        assert_eq!(chg.start, "2019-08-21T04:38:38Z");
        assert_eq!(chg.end.as_deref(), Some("2019-08-21T13:00:00Z"));
    }

    #[test]
    fn unknown_source_fields_do_not_corrupt_normalized_event() {
        // Simulate the viewer adding a field in the future.
        let resp = ViewerResponse::parse_json(
            r#"{
                "total": 1,
                "result": [{
                    "number": "INC0099999",
                    "short_description": "Outage - Future Field Test",
                    "state": "7",
                    "category": "Circuit",
                    "work_start": "1753932540",
                    "work_end": "1753932600",
                    "opened_at": "1753933418",
                    "priority": "2",
                    "some_future_field": {"nested": [1, 2, 3]},
                    "another_unknown": "x"
                }]
            }"#,
        )
        .unwrap();
        let rec = resp.result[0].to_grnoc_record();
        // The unknown fields are ignored; normalization is unaffected.
        assert_eq!(rec.number, "INC0099999");
        assert_eq!(rec.short_description, "Outage - Future Field Test");
        assert_eq!(rec.state, "Closed");
        assert_eq!(rec.start, "2025-07-31T03:29:00Z");
        assert_eq!(rec.end.as_deref(), Some("2025-07-31T03:30:00Z"));
        assert_eq!(rec.priority, "High");
        // Round-trip through JSON keeps the generic record stable.
        let json = serde_json::to_string(&rec).unwrap();
        let back: GrnocRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(back.number, "INC0099999");
        assert_eq!(back.state, "Closed");
    }

    #[test]
    fn malformed_response_produces_item_scoped_failure() {
        // The malformed fixture is a per-item failure: the good files in
        // the same directory still parse.
        let err =
            ViewerResponse::from_file("tests/fixtures/grnoc/viewer/malformed.json").unwrap_err();
        assert!(err.contains("invalid viewer response JSON"), "{err}");
        let mut good = 0usize;
        let mut bad = 0usize;
        let dir = std::fs::read_dir("tests/fixtures/grnoc/viewer").unwrap();
        for entry in dir.flatten() {
            let path = entry.path();
            if path.extension().map(|x| x == "json").unwrap_or(false) {
                match ViewerResponse::from_file(path.to_string_lossy().as_ref()) {
                    Ok(_) => good += 1,
                    Err(_) => bad += 1,
                }
            }
        }
        assert_eq!(good, 3, "three valid viewer fixtures");
        assert_eq!(bad, 1, "exactly the malformed fixture fails");
    }
}
