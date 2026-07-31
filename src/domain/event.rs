//! Source-neutral operational event types.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A unique identifier for an operational event (e.g. ticket number).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EventId(pub String);

impl From<&str> for EventId {
    fn from(s: &str) -> Self {
        EventId(s.to_string())
    }
}

impl std::fmt::Display for EventId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A time window for an operational event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventWindow {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
}

/// A parsed operational event from any source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationalEvent {
    pub id: EventId,
    pub title: String,
    pub window: EventWindow,
    /// Source identifier (e.g. "internet2-grnoc").
    pub source: String,
    /// Original raw data, preserved for auditability.
    pub raw: serde_json::Value,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn sample_time() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2025, 6, 15, 5, 25, 0).unwrap()
    }

    #[test]
    fn event_id_from_str() {
        let id = EventId::from("CHG0107955");
        assert_eq!(id.0, "CHG0107955");
    }

    #[test]
    fn event_id_display() {
        let id = EventId::from("INC0302574");
        assert_eq!(format!("{id}"), "INC0302574");
    }

    #[test]
    fn event_window_construction() {
        let start = sample_time();
        let end = start + chrono::Duration::minutes(22);
        let window = EventWindow { start, end };
        assert_eq!(window.start, sample_time());
    }

    #[test]
    fn operational_event_construction() {
        let event = OperationalEvent {
            id: EventId::from("CHG0107955"),
            title: "Maintenance - I2 Various Participants via DE-CIX (NEWY32AOA)".into(),
            window: EventWindow {
                start: sample_time(),
                end: sample_time() + chrono::Duration::hours(5),
            },
            source: "internet2-grnoc".into(),
            raw: serde_json::json!({}),
        };
        assert_eq!(event.id.0, "CHG0107955");
        assert_eq!(event.source, "internet2-grnoc");
    }

    #[test]
    fn event_id_serialization_roundtrip() {
        let id = EventId::from("CHG0107955");
        let json = serde_json::to_string(&id).unwrap();
        let parsed: EventId = serde_json::from_str(&json).unwrap();
        assert_eq!(id, parsed);
    }
}
