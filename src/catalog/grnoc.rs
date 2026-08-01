//! GRNOC catalog source — the first `EventCatalogSource` adapter.
//!
//! Source-neutral catalog semantics live in the generic layer; GRNOC
//! Public Task Viewer conventions (title interpretation, expectations)
//! stay in this adapter and the existing `sources::grnoc` parser.

use std::path::Path;

use crate::domain::event::EventId;

use super::domain::CatalogSourceItem;

/// A source-neutral catalog item producer.
pub trait EventCatalogSource {
    /// Source identifier, e.g. "grnoc-public-task-viewer".
    fn source(&self) -> &str;

    /// Produce immutable source items. Per-item failures are reported in
    /// the result, not fatal to the whole sync.
    fn list_items(&self) -> Result<Vec<CatalogSourceItem>, String>;
}

/// Parser identity used for snapshots produced by this adapter.
pub const GRNOC_PARSER_VERSION: &str = "grnoc-record-1";

/// GRNOC Public Task Viewer adapter over local JSON record files.
///
/// Each file is one `GnocRecord`. The adapter is source-neutral upstream
/// of normalization: the normalized event carries only generic fields
/// (id, title, start/end, lifecycle) — no convention interpretation.
pub struct GrnocCatalogSource {
    /// Directory containing one GRNOC JSON record per file.
    pub source_dir: std::path::PathBuf,
    /// Timestamp recorded for fetched items (RFC3339 UTC).
    pub fetched_at: String,
}

impl GrnocCatalogSource {
    pub fn new(source_dir: std::path::PathBuf, fetched_at: String) -> Self {
        GrnocCatalogSource {
            source_dir,
            fetched_at,
        }
    }
}

impl EventCatalogSource for GrnocCatalogSource {
    fn source(&self) -> &str {
        "grnoc-public-task-viewer"
    }

    fn list_items(&self) -> Result<Vec<CatalogSourceItem>, String> {
        let mut items = Vec::new();
        let entries = std::fs::read_dir(&self.source_dir).map_err(|e| {
            format!(
                "cannot read GRNOC source directory {}: {e}",
                self.source_dir.display()
            )
        })?;
        let mut paths: Vec<std::path::PathBuf> = entries
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().map(|x| x == "json").unwrap_or(false))
            .collect();
        paths.sort();
        for path in paths {
            match crate::sources::grnoc::GrnocRecord::from_file(path.to_string_lossy().as_ref()) {
                Ok(record) => {
                    let external_id = record.number.clone();
                    let raw = std::fs::read_to_string(&path).unwrap_or_else(|_| "{}".to_string());
                    let normalized = serde_json::json!({
                        "id": record.number,
                        "title": record.short_description,
                        "task_type": record.task_type,
                        "category": record.category,
                        "start": record.start,
                        "end": record.end,
                        "state": record.state,
                        "priority": record.priority,
                        "description": record.description,
                        "source_url": record.source_url,
                        "timezone": record.timezone,
                    });
                    items.push(CatalogSourceItem {
                        source: self.source().to_string(),
                        external_id,
                        fetched_at: self.fetched_at.clone(),
                        source_url: record.source_url.clone(),
                        raw_payload: raw,
                        normalized_json: normalized.to_string(),
                    });
                }
                Err(e) => {
                    eprintln!("  grnoc: skipping {} ({e})", path.display());
                }
            }
        }
        Ok(items)
    }
}

/// Build a `CatalogSourceItem` from a GRNOC fixture file path.
pub fn source_item_from_fixture(
    path: &Path,
    fetched_at: &str,
) -> Result<CatalogSourceItem, String> {
    let record = crate::sources::grnoc::GrnocRecord::from_file(path.to_string_lossy().as_ref())?;
    let raw = std::fs::read_to_string(path)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let normalized = serde_json::json!({
        "id": record.number,
        "title": record.short_description,
        "task_type": record.task_type,
        "category": record.category,
        "start": record.start,
        "end": record.end,
        "state": record.state,
        "priority": record.priority,
        "description": record.description,
        "source_url": record.source_url,
        "timezone": record.timezone,
    });
    Ok(CatalogSourceItem {
        source: "grnoc-public-task-viewer".to_string(),
        external_id: record.number.clone(),
        fetched_at: fetched_at.to_string(),
        source_url: record.source_url.clone(),
        raw_payload: raw,
        normalized_json: normalized.to_string(),
    })
}

/// Normalized event fields extracted from a source item (generic only).
pub fn normalized_fields(item: &CatalogSourceItem) -> serde_json::Value {
    serde_json::from_str(&item.normalized_json).unwrap_or(serde_json::Value::Null)
}

/// External event id for the catalog event (source-neutral: id string).
pub fn event_external_id(item: &CatalogSourceItem) -> String {
    normalized_fields(item)
        .get("id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| item.external_id.clone())
}

/// EventId helper for tests and tooling.
pub fn event_id_from_item(item: &CatalogSourceItem) -> EventId {
    EventId::from(event_external_id(item).as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixture_item_has_expected_fields() {
        let item = source_item_from_fixture(
            Path::new("tests/fixtures/grnoc/INC0301970.json"),
            "2026-07-31T00:00:00Z",
        )
        .unwrap();
        assert_eq!(item.source, "grnoc-public-task-viewer");
        assert_eq!(item.external_id, "INC0301970");
        let n = normalized_fields(&item);
        assert_eq!(n["title"], "Outage - Indiana GigaPOP Peer Smithville");
        assert_eq!(n["end"], serde_json::Value::Null);
        assert_eq!(event_external_id(&item), "INC0301970");
    }

    #[test]
    fn malformed_file_is_reported_per_item() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("bad.json"), "not json").unwrap();
        std::fs::write(
            dir.path().join("good.json"),
            r#"{"number":"T1","short_description":"x","start":"2026-07-01T00:00:00Z"}"#,
        )
        .unwrap();
        let source =
            GrnocCatalogSource::new(dir.path().to_path_buf(), "2026-07-31T00:00:00Z".into());
        let items = source.list_items().unwrap();
        // The good item survives; the malformed one is skipped per item.
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].external_id, "T1");
    }
}
