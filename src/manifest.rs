//! Reviewed event manifest — human-reviewed event context and pipeline
//! configuration that drives every real analysis.
//!
//! The manifest is the single source of truth for the analysis window,
//! target predicate, and collector selection. It is reviewed before
//! execution and must never be regenerated automatically.

use serde::{Deserialize, Serialize};

/// A reviewed event manifest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub event_id: String,
    #[serde(default)]
    pub revision: u32,
    pub event_window_utc: Window,
    pub ticket_window_local: LocalWindow,
    pub warmup_minutes: i64,
    pub cooldown_minutes: i64,
    pub target: ManifestTarget,
    pub collectors: Vec<String>,
    #[serde(default)]
    pub collectors_provenance: String,
    #[serde(default)]
    pub analyst_notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Window {
    pub start: String,
    pub end: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalWindow {
    pub start: String,
    pub end: String,
    pub timezone: String,
    #[serde(default)]
    pub timezone_note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestTarget {
    pub label: String,
    pub origin_asns: Vec<u32>,
    pub internet2_asn: u32,
    #[serde(default)]
    pub prefix_selection: String,
    #[serde(default)]
    pub prefix_selection_provenance: String,
}

impl Manifest {
    /// Load a manifest from a JSON file, with basic validation.
    pub fn load(path: &std::path::Path) -> Result<Manifest, String> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("cannot read manifest {}: {e}", path.display()))?;

        let manifest: Manifest =
            serde_json::from_str(&content).map_err(|e| format!("invalid manifest JSON: {e}"))?;

        // Validate fields
        if manifest.event_id.is_empty() {
            return Err("manifest event_id is empty".into());
        }
        if manifest.event_window_utc.start.is_empty() || manifest.event_window_utc.end.is_empty() {
            return Err("manifest event_window_utc has empty start/end".into());
        }
        if manifest.collectors.is_empty() {
            return Err("manifest collectors list is empty".into());
        }
        if manifest.warmup_minutes < 0 || manifest.cooldown_minutes < 0 {
            return Err("warmup_minutes and cooldown_minutes must be >= 0".into());
        }
        if manifest.target.origin_asns.is_empty() {
            return Err("manifest target.origin_asns is empty".into());
        }

        Ok(manifest)
    }

    /// Parse and return the UTC event window as chrono DateTime.
    pub fn event_window(
        &self,
    ) -> Result<(chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>), String> {
        let start = chrono::DateTime::parse_from_rfc3339(&self.event_window_utc.start)
            .map_err(|e| format!("invalid event start: {e}"))?
            .with_timezone(&chrono::Utc);
        let end = chrono::DateTime::parse_from_rfc3339(&self.event_window_utc.end)
            .map_err(|e| format!("invalid event end: {e}"))?
            .with_timezone(&chrono::Utc);
        Ok((start, end))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_manifest(dir: &std::path::Path, json: &str) -> std::path::PathBuf {
        let p = dir.join("manifest.json");
        std::fs::write(&p, json).unwrap();
        p
    }

    #[test]
    fn parse_inc0302574_manifest() {
        let dir = tempfile::tempdir().unwrap();
        let json = r#"{
            "event_id": "INC0302574",
            "event_window_utc": {"start": "2026-07-30T09:25:00Z", "end": "2026-07-30T09:47:00Z"},
            "ticket_window_local": {"start": "2026-07-30 05:25:00", "end": "2026-07-30 05:47:00", "timezone": "EDT"},
            "warmup_minutes": 60,
            "cooldown_minutes": 60,
            "target": {"label": "test", "origin_asns": [3333], "internet2_asn": 11537},
            "collectors": ["route-views2"]
        }"#;
        let path = write_manifest(dir.path(), json);
        let m = Manifest::load(&path).unwrap();
        assert_eq!(m.event_id, "INC0302574");
        let (start, end) = m.event_window().unwrap();
        assert_eq!(
            start.timestamp(),
            chrono::DateTime::parse_from_rfc3339("2026-07-30T09:25:00Z")
                .unwrap()
                .timestamp()
        );
        assert_eq!(
            end.timestamp(),
            chrono::DateTime::parse_from_rfc3339("2026-07-30T09:47:00Z")
                .unwrap()
                .timestamp()
        );
    }

    #[test]
    fn malformed_manifest_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_manifest(dir.path(), "not json");
        assert!(Manifest::load(&path).is_err());
    }

    #[test]
    fn empty_collectors_is_invalid() {
        let dir = tempfile::tempdir().unwrap();
        let json = r#"{
            "event_id": "INC0302574",
            "event_window_utc": {"start": "2026-07-30T09:25:00Z", "end": "2026-07-30T09:47:00Z"},
            "ticket_window_local": {"start": "2026-07-30 05:25:00", "end": "2026-07-30 05:47:00", "timezone": "EDT"},
            "warmup_minutes": 60,
            "cooldown_minutes": 60,
            "target": {"label": "test", "origin_asns": [3333], "internet2_asn": 11537},
            "collectors": []
        }"#;
        let path = write_manifest(dir.path(), json);
        assert!(Manifest::load(&path).is_err());
    }
}
