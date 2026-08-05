//! Reviewed event manifest — human-reviewed event context and pipeline
//! configuration that drives every real analysis.
//!
//! The manifest is the single source of truth for the analysis window,
//! target predicate, and collector selection. It is reviewed before
//! execution and must never be regenerated automatically.

use serde::{Deserialize, Serialize};

use crate::domain::route::TransitPredicate as Predicate;
use crate::plan::{NamedPathClassifier, Provenance, TransitPredicateMapping};
use crate::schema::MANIFEST_SCHEMA_VERSION as CURRENT_MANIFEST_SCHEMA_VERSION;

/// Schema version of the reviewed manifest format — canonical definition
/// lives in `src/schema.rs` (single registry).
///
/// v1: single-ASN shortcut fields (`managed_network_asn`, `internet2_asn`).
/// v2: canonical `TransitPredicateMapping`; legacy shortcut fields rejected.
pub const MANIFEST_SCHEMA_VERSION: u32 = CURRENT_MANIFEST_SCHEMA_VERSION;

/// Error returned when a legacy (pre-canonical) manifest is loaded.
pub const LEGACY_MANIFEST_REQUIRES_MIGRATION: &str = "LegacyManifestRequiresMigration";

/// A reviewed event manifest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub event_id: String,
    #[serde(default)]
    pub revision: u32,
    /// Manifest schema version. Defaults to 1 (legacy) when absent.
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    pub event_window_utc: Window,
    pub ticket_window_local: LocalWindow,
    /// Whether the ticket is open (no published end time).
    #[serde(default)]
    pub open: bool,
    /// Reviewed analysis end for open tickets.
    #[serde(default)]
    pub analysis_end_utc: Option<String>,
    #[serde(default)]
    pub warmup_minutes: i64,
    #[serde(default)]
    pub cooldown_minutes: i64,
    pub target: ManifestTarget,
    pub collectors: Vec<String>,
    /// BGP archive source family: "RouteViews" (default) or "RipeRis".
    /// Collector identifiers are only meaningful together with their
    /// family (`rrc00` exists only in RIPE RIS, `route-views2` only in
    /// RouteViews).
    #[serde(default = "default_source_family")]
    pub source_family: String,
    #[serde(default)]
    pub collectors_provenance: String,
    #[serde(default)]
    pub analyst_notes: Vec<String>,
}

fn default_source_family() -> String {
    crate::catalog::archive_plan::SourceFamily::RouteViews
        .as_str()
        .to_string()
}

fn default_schema_version() -> u32 {
    1
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
    /// Legacy single-ASN shortcut — rejected on load; migration only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub managed_network_asn: Option<u32>,
    /// Legacy single-ASN shortcut — rejected on load; migration only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub internet2_asn: Option<u32>,
    #[serde(default)]
    pub transit_predicate: TransitPredicateMapping,
    /// Named path classifiers for classification-only reporting
    /// (origin inventories, cross-observer comparisons). Never used for
    /// cohort selection.
    #[serde(default)]
    pub path_classifiers: Vec<NamedPathClassifier>,
    #[serde(default)]
    pub prefix_selection: String,
    #[serde(default)]
    pub prefix_selection_provenance: String,
}

impl Manifest {
    /// Load a manifest from a JSON file, with basic validation.
    ///
    /// Legacy revisions (schema v1, or any manifest carrying the
    /// `managed_network_asn` / `internet2_asn` shortcut fields) are
    /// rejected with `LegacyManifestRequiresMigration` — they are never
    /// silently interpreted as a canonical predicate during analysis.
    pub fn load(path: &std::path::Path) -> Result<Manifest, String> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("cannot read manifest {}: {e}", path.display()))?;

        let manifest: Manifest =
            serde_json::from_str(&content).map_err(|e| format!("invalid manifest JSON: {e}"))?;

        manifest.validate()
    }

    /// Validate a parsed manifest, including schema-version policy.
    pub fn validate(&self) -> Result<Self, String> {
        // Legacy shortcut fields are never accepted for analysis.
        if self.target.managed_network_asn.is_some() || self.target.internet2_asn.is_some() {
            return Err(format!(
                "{LEGACY_MANIFEST_REQUIRES_MIGRATION}: manifest {} uses a legacy single-ASN shortcut field (managed_network_asn/internet2_asn). Migrate to TransitPredicateMapping before analysis.",
                self.event_id
            ));
        }
        if self.schema_version < MANIFEST_SCHEMA_VERSION {
            return Err(format!(
                "{LEGACY_MANIFEST_REQUIRES_MIGRATION}: manifest {} is schema v{}, current is v{MANIFEST_SCHEMA_VERSION}.",
                self.event_id, self.schema_version
            ));
        }
        if self.schema_version > MANIFEST_SCHEMA_VERSION {
            return Err(format!(
                "UnsupportedManifestRevision: manifest {} schema v{} is newer than supported v{MANIFEST_SCHEMA_VERSION}.",
                self.event_id, self.schema_version
            ));
        }

        // Validate fields
        if self.event_id.is_empty() {
            return Err("manifest event_id is empty".into());
        }
        if self.event_window_utc.start.is_empty()
            || (self.event_window_utc.end.is_empty() && !self.open)
        {
            return Err("manifest event_window_utc has empty start/end".into());
        }
        // OPEN events always require an explicit reviewed analysis
        // cutoff: the reviewed manifest is the authority for the
        // provisional analysis end. A missing or empty cutoff is
        // internally contradictory reviewed input and is rejected,
        // regardless of any declared event end.
        if self.open {
            let has_cutoff = self
                .analysis_end_utc
                .as_deref()
                .map(|c| !c.trim().is_empty())
                .unwrap_or(false);
            if !has_cutoff {
                return Err(
                    "manifest open event requires an explicit analysis_end_utc cutoff".into(),
                );
            }
        }
        if self.collectors.is_empty() {
            return Err("manifest collectors list is empty".into());
        }
        if self.warmup_minutes < 0 || self.cooldown_minutes < 0 {
            return Err("warmup_minutes and cooldown_minutes must be >= 0".into());
        }
        if self.target.origin_asns.is_empty() {
            return Err("manifest target.origin_asns is empty".into());
        }
        self.target.transit_predicate.validate()?;

        Ok(self.clone())
    }

    /// Parse and return the UTC event window as chrono DateTime.
    pub fn event_window(
        &self,
    ) -> Result<(chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>), String> {
        let start = chrono::DateTime::parse_from_rfc3339(&self.event_window_utc.start)
            .map_err(|e| format!("invalid event start: {e}"))?
            .with_timezone(&chrono::Utc);
        // OPEN events carry an empty declared end; the reviewed
        // analysis cutoff (analysis_end_utc) is the explicit analysis
        // end. A missing cutoff for an open event is a hard error.
        let end = if self.event_window_utc.end.trim().is_empty() {
            let cutoff = self
                .analysis_end_utc
                .as_deref()
                .map(|c| c.trim())
                .filter(|c| !c.is_empty())
                .ok_or_else(|| {
                    if self.open {
                        "invalid event end: open event requires an explicit analysis cutoff"
                            .to_string()
                    } else {
                        "invalid event end: event end unavailable".to_string()
                    }
                })?;
            chrono::DateTime::parse_from_rfc3339(cutoff)
                .map_err(|e| format!("invalid event end: {e}"))?
                .with_timezone(&chrono::Utc)
        } else {
            chrono::DateTime::parse_from_rfc3339(&self.event_window_utc.end)
                .map_err(|e| format!("invalid event end: {e}"))?
                .with_timezone(&chrono::Utc)
        };
        Ok((start, end))
    }

    /// Parse a legacy (pre-canonical) manifest WITHOUT validation.
    ///
    /// Migration-only entry point: normal analysis must use `load`, which
    /// rejects legacy revisions with `LegacyManifestRequiresMigration`.
    pub fn load_legacy(path: &std::path::Path) -> Result<Manifest, String> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("cannot read manifest {}: {e}", path.display()))?;
        serde_json::from_str(&content).map_err(|e| format!("invalid manifest JSON: {e}"))
    }
}

/// Migrate a legacy manifest to the canonical schema.
///
/// Converts the single-ASN shortcuts to a `ContainsAny` predicate:
///   `managed_network_asn`   → `ContainsAny([asn])` (Reviewed)
///   `internet2_asn` (nonzero) → `ContainsAny([asn])` (Reviewed)
///   `internet2_asn` (0/pending) → `Unresolved` (no predicate invented)
///
/// A Reviewed predicate requires analyst-confirmed provenance; the helper
/// never executes analysis and never invents ASN values for unresolved
/// mappings. Event identity and entity mapping are preserved verbatim.
pub fn migrate_manifest(
    old: &Manifest,
    provenance: Option<Provenance>,
) -> Result<Manifest, String> {
    let legacy_asn = old.target.managed_network_asn.or(old.target.internet2_asn);

    let (transit_predicate, requires_provenance) = match legacy_asn {
        Some(asn) if asn != 0 => (
            TransitPredicateMapping {
                status: crate::plan::PredicateReviewStatus::Reviewed,
                predicate: Some(Predicate::ContainsAny(vec![asn])),
                provenance: provenance.clone(),
            },
            true,
        ),
        // 0/pending legacy value: no ASN invented; mapping stays unresolved.
        _ => (
            TransitPredicateMapping {
                status: crate::plan::PredicateReviewStatus::Unresolved,
                predicate: None,
                provenance: None,
            },
            false,
        ),
    };

    if requires_provenance && provenance.is_none() {
        return Err(
            "migration to a Reviewed ContainsAny predicate requires analyst-confirmed provenance (statement, reviewed_by, date)".into(),
        );
    }

    let mut migrated = old.clone();
    migrated.schema_version = MANIFEST_SCHEMA_VERSION;
    migrated.revision = old.revision.saturating_add(1);
    migrated.target.managed_network_asn = None;
    migrated.target.internet2_asn = None;
    migrated.target.transit_predicate = transit_predicate;

    migrated.validate()
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
            "schema_version": 2,
            "event_window_utc": {"start": "2026-07-30T09:25:00Z", "end": "2026-07-30T09:47:00Z"},
            "ticket_window_local": {"start": "2026-07-30 05:25:00", "end": "2026-07-30 05:47:00", "timezone": "EDT"},
            "warmup_minutes": 60,
            "cooldown_minutes": 60,
            "target": {
                "label": "test",
                "origin_asns": [3333],
                "transit_predicate": {
                    "status": "Reviewed",
                    "predicate": {"ContainsAny": [11537]},
                    "provenance": {"statement": "t", "reviewed_by": "a", "date": "d"}
                }
            },
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
            "schema_version": 2,
            "event_window_utc": {"start": "2026-07-30T09:25:00Z", "end": "2026-07-30T09:47:00Z"},
            "ticket_window_local": {"start": "2026-07-30 05:25:00", "end": "2026-07-30 05:47:00", "timezone": "EDT"},
            "warmup_minutes": 60,
            "cooldown_minutes": 60,
            "target": {
                "label": "test",
                "origin_asns": [3333],
                "transit_predicate": {
                    "status": "Reviewed",
                    "predicate": {"ContainsAny": [11537]},
                    "provenance": {"statement": "t", "reviewed_by": "a", "date": "d"}
                }
            },
            "collectors": []
        }"#;
        let path = write_manifest(dir.path(), json);
        assert!(Manifest::load(&path).is_err());
    }

    // ── Part 2: canonical TransitPredicateMapping ─────────────────

    /// A canonical v2 manifest (no legacy fields).
    const CANONICAL_JSON: &str = r#"{
        "event_id": "INC0302574",
        "revision": 2,
        "schema_version": 2,
        "event_window_utc": {"start": "2026-07-30T09:25:00Z", "end": "2026-07-30T09:47:00Z"},
        "ticket_window_local": {"start": "2026-07-30 05:25:00", "end": "2026-07-30 05:47:00", "timezone": "EDT"},
        "warmup_minutes": 60,
        "cooldown_minutes": 60,
        "target": {
            "label": "RIPE via NYIIX",
            "origin_asns": [3333],
            "transit_predicate": {
                "status": "Reviewed",
                "predicate": {"ContainsAny": [11537]},
                "provenance": {"statement": "AS11537 = Internet2", "reviewed_by": "analyst", "date": "2026-08-01"}
            }
        },
        "collectors": ["route-views2"]
    }"#;

    /// A legacy v1 manifest using the single-ASN shortcut.
    const LEGACY_JSON: &str = r#"{
        "event_id": "INC0302574",
        "revision": 1,
        "event_window_utc": {"start": "2026-07-30T09:25:00Z", "end": "2026-07-30T09:47:00Z"},
        "ticket_window_local": {"start": "2026-07-30 05:25:00", "end": "2026-07-30 05:47:00", "timezone": "EDT"},
        "warmup_minutes": 60,
        "cooldown_minutes": 60,
        "target": {
            "label": "RIPE via NYIIX",
            "origin_asns": [3333],
            "managed_network_asn": 11537
        },
        "collectors": ["route-views2"]
    }"#;

    #[test]
    fn canonical_manifest_uses_transit_predicate_mapping() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_manifest(dir.path(), CANONICAL_JSON);
        let m = Manifest::load(&path).unwrap();
        let tp = &m.target.transit_predicate;
        assert_eq!(tp.status, crate::plan::PredicateReviewStatus::Reviewed);
        assert_eq!(
            tp.predicate,
            Some(crate::domain::route::TransitPredicate::ContainsAny(vec![
                11537
            ]))
        );
        assert!(tp.provenance.is_some());
    }

    #[test]
    fn canonical_manifest_rejects_managed_network_asn() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_manifest(dir.path(), LEGACY_JSON);
        let err = Manifest::load(&path).unwrap_err();
        assert!(err.contains(LEGACY_MANIFEST_REQUIRES_MIGRATION), "{err}");
    }

    #[test]
    fn canonical_manifest_rejects_internet2_asn() {
        let dir = tempfile::tempdir().unwrap();
        let json = LEGACY_JSON.replace("managed_network_asn", "internet2_asn");
        let path = write_manifest(dir.path(), &json);
        let err = Manifest::load(&path).unwrap_err();
        assert!(err.contains(LEGACY_MANIFEST_REQUIRES_MIGRATION), "{err}");
    }

    #[test]
    fn old_revision_returns_explicit_migration_error() {
        // A schema-v1 manifest without any shortcut is still a legacy revision.
        let dir = tempfile::tempdir().unwrap();
        let json = LEGACY_JSON.replace("\"managed_network_asn\": 11537,", "");
        let path = write_manifest(dir.path(), &json);
        let err = Manifest::load(&path).unwrap_err();
        assert!(err.contains(LEGACY_MANIFEST_REQUIRES_MIGRATION), "{err}");
    }

    #[test]
    fn migration_converts_single_asn_to_contains_any() {
        let dir = tempfile::tempdir().unwrap();
        let old = Manifest::load_legacy(&write_manifest(dir.path(), LEGACY_JSON)).unwrap();
        let provenance = Provenance {
            statement: "Legacy managed_network_asn 11537 reviewed as Internet2 transit".into(),
            reviewed_by: "analyst".into(),
            date: "2026-08-01".into(),
        };
        let migrated = migrate_manifest(&old, Some(provenance)).unwrap();
        assert_eq!(migrated.schema_version, MANIFEST_SCHEMA_VERSION);
        assert_eq!(migrated.revision, 2);
        assert_eq!(
            migrated.target.transit_predicate.predicate,
            Some(Predicate::ContainsAny(vec![11537]))
        );
        assert_eq!(
            migrated.target.transit_predicate.status,
            crate::plan::PredicateReviewStatus::Reviewed
        );
    }

    #[test]
    fn migration_preserves_event_and_entity_mapping() {
        let dir = tempfile::tempdir().unwrap();
        let old = Manifest::load_legacy(&write_manifest(dir.path(), LEGACY_JSON)).unwrap();
        let provenance = Provenance {
            statement: "reviewed".into(),
            reviewed_by: "analyst".into(),
            date: "2026-08-01".into(),
        };
        let migrated = migrate_manifest(&old, Some(provenance)).unwrap();
        assert_eq!(migrated.event_id, old.event_id);
        assert_eq!(migrated.event_window_utc.start, old.event_window_utc.start);
        assert_eq!(migrated.event_window_utc.end, old.event_window_utc.end);
        assert_eq!(migrated.target.label, old.target.label);
        assert_eq!(migrated.target.origin_asns, old.target.origin_asns);
        assert_eq!(
            migrated.target.prefix_selection,
            old.target.prefix_selection
        );
        assert_eq!(migrated.collectors, old.collectors);
        assert_eq!(migrated.warmup_minutes, old.warmup_minutes);
        assert_eq!(migrated.cooldown_minutes, old.cooldown_minutes);
    }

    #[test]
    fn migration_requires_reviewed_provenance() {
        let dir = tempfile::tempdir().unwrap();
        let old = Manifest::load_legacy(&write_manifest(dir.path(), LEGACY_JSON)).unwrap();
        let err = migrate_manifest(&old, None).unwrap_err();
        assert!(err.contains("provenance"), "{err}");
    }

    #[test]
    fn canonical_roundtrip_contains_no_legacy_fields() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_manifest(dir.path(), CANONICAL_JSON);
        let m = Manifest::load(&path).unwrap();
        let json = serde_json::to_string_pretty(&m).unwrap();
        assert!(!json.contains("managed_network_asn"), "{json}");
        assert!(!json.contains("internet2_asn"), "{json}");
        // And it reloads as canonical.
        let path2 = write_manifest(dir.path(), &json);
        let m2 = Manifest::load(&path2).unwrap();
        assert!(m2.target.transit_predicate.is_ready());
    }

    #[test]
    fn unresolved_open_manifest_has_no_predicate() {
        // INC0301970 migration: internet2_asn 0 → Unresolved, no ASN invented.
        let dir = tempfile::tempdir().unwrap();
        let json = LEGACY_JSON
            .replace("\"managed_network_asn\": 11537", "\"internet2_asn\": 0")
            .replace(
                "\"event_window_utc\": {\"start\": \"2026-07-30T09:25:00Z\", \"end\": \"2026-07-30T09:47:00Z\"},",
                "\"event_window_utc\": {\"start\": \"2026-07-28T04:35:00Z\", \"end\": \"\"},\n            \"open\": true,\n            \"analysis_end_utc\": \"2026-07-29T04:35:00Z\",",
            );
        let old = Manifest::load_legacy(&write_manifest(dir.path(), &json)).unwrap();
        let migrated = migrate_manifest(&old, None).unwrap();
        assert_eq!(
            migrated.target.transit_predicate.status,
            crate::plan::PredicateReviewStatus::Unresolved
        );
        assert!(migrated.target.transit_predicate.predicate.is_none());
        // No ASN value invented: no legacy field survives.
        let out = serde_json::to_string(&migrated).unwrap();
        assert!(!out.contains("11537"), "{out}");
    }
    #[test]
    fn routeviews_behavior_remains_unchanged() {
        // A manifest WITHOUT a source_family field is a RouteViews
        // manifest (the pre-Session-34 behavior) — the default must
        // never change for existing reviewed manifests.
        let dir = tempfile::tempdir().unwrap();
        let json = r#"{
            "event_id": "RV-TEST",
            "schema_version": 2,
            "event_window_utc": {"start": "2019-08-21T16:00:00Z", "end": "2019-08-21T17:30:00Z"},
            "ticket_window_local": {"start": "2019-08-21T12:00:00-04:00", "end": "2019-08-21T13:30:00-04:00", "timezone": "America/New_York"},
            "warmup_minutes": 840,
            "cooldown_minutes": 60,
            "target": {
                "label": "SampleNet (AS64500)",
                "origin_asns": [2603],
                "transit_predicate": {"kind": "ContainsAny", "asns": [11537]}
            },
            "collectors": ["route-views2"],
            "collectors_provenance": "reviewed"
        }"#;
        let path = write_manifest(dir.path(), json);
        let m = Manifest::load(&path).unwrap();
        assert_eq!(m.source_family, "RouteViews");
        assert_eq!(
            crate::catalog::archive_plan::SourceFamily::parse_family(&m.source_family),
            Some(crate::catalog::archive_plan::SourceFamily::RouteViews)
        );
        // RouteViews selection semantics are unchanged: latest RIB at or
        // before warmup on the 2-hour grid.
        use crate::discover::{select_rib, ArchiveItem};
        let t = |s: &str| chrono::DateTime::parse_from_rfc3339(s).unwrap().to_utc();
        let items = vec![
            ArchiveItem {
                project: "routeviews".into(),
                collector_id: "route-views2".into(),
                data_type: "rib".into(),
                ts_start: t("2019-08-20T22:00:00Z"),
                ts_end: t("2019-08-21T00:00:00Z"),
                url: "http://archive.routeviews.org/bgpdata/2019.08/RIBS/rib.20190820.2200.bz2"
                    .into(),
                size: 1,
            },
            ArchiveItem {
                project: "routeviews".into(),
                collector_id: "route-views2".into(),
                data_type: "rib".into(),
                ts_start: t("2019-08-21T00:00:00Z"),
                ts_end: t("2019-08-21T02:00:00Z"),
                url: "http://archive.routeviews.org/bgpdata/2019.08/RIBS/rib.20190821.0000.bz2"
                    .into(),
                size: 1,
            },
            ArchiveItem {
                project: "routeviews".into(),
                collector_id: "route-views2".into(),
                data_type: "rib".into(),
                ts_start: t("2019-08-21T02:00:00Z"),
                ts_end: t("2019-08-21T04:00:00Z"),
                url: "http://archive.routeviews.org/bgpdata/2019.08/RIBS/rib.20190821.0200.bz2"
                    .into(),
                size: 1,
            },
        ];
        // warmup 02:00 — the 02:00 RIB is eligible (<= warmup) and is the
        // latest; identical to pre-Session-34 behavior.
        let best = select_rib(&items, t("2019-08-21T02:00:00Z")).unwrap();
        assert_eq!(
            best.url,
            "http://archive.routeviews.org/bgpdata/2019.08/RIBS/rib.20190821.0200.bz2"
        );
        // A warmup before the first RIB still falls back to the newest
        // pre-warmup RIB — never a post-warmup file.
        let best = select_rib(&items, t("2019-08-20T22:30:00Z")).unwrap();
        assert_eq!(
            best.url,
            "http://archive.routeviews.org/bgpdata/2019.08/RIBS/rib.20190820.2200.bz2"
        );
    }
}
