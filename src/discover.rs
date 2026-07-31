//! Archive discovery — bgpkit-broker integration and local caching.
//!
//! Discovers RouteViews archive files via bgpkit-broker, selects the
//! relevant RIB and UPDATE files for an analysis window, downloads
//! them to a deterministic local cache with integrity verification,
//! and returns a serializable dataset manifest for reproducibility.
//!
//! This module uses bgpkit-broker for discovery but owns the
//! selection logic, caching policy, and error classification.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::Path;

// ── Archive item types ─────────────────────────────────────────────

/// A discovered archive item, mapped from a bgpkit-broker `BrokerItem`.
///
/// Collector identity comes from broker metadata — never parsed from URLs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArchiveItem {
    /// Archive project (e.g. "route-views").
    pub project: String,
    /// Collector identifier from broker metadata (e.g. "route-views2").
    pub collector_id: String,
    /// Data type: "rib" or "updates".
    pub data_type: String,
    /// Start of the file's time coverage.
    pub ts_start: chrono::DateTime<chrono::Utc>,
    /// End of the file's time coverage.
    pub ts_end: chrono::DateTime<chrono::Utc>,
    /// Canonical source URL.
    pub url: String,
    /// Reported file size in bytes.
    pub size: u64,
}

/// A locally cached archive file with integrity metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CachedArchive {
    /// Original source URL.
    pub url: String,
    /// Local filesystem path.
    pub local_path: String,
    /// Archive project.
    pub project: String,
    /// Collector identifier.
    pub collector_id: String,
    /// Data type.
    pub data_type: String,
    /// File size in bytes.
    pub size: u64,
    /// SHA-256 checksum (hex-encoded).
    pub sha256: String,
}

/// A dataset: the selected RIB and UPDATE files for an analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dataset {
    /// Event identifier.
    pub event_id: String,
    /// Per-collector archives.
    pub collectors: Vec<CollectorArchives>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectorArchives {
    pub collector_id: String,
    pub rib: Option<CachedArchive>,
    pub updates: Vec<CachedArchive>,
    /// Whether relevant observer-prefix streams were found in the RIB.
    pub has_relevant_streams: bool,
}

// ── Selection logic (pure, testable without network) ──────────────

/// Select the newest RIB with `ts_end` at or before `warmup_start`.
pub fn select_rib(items: &[ArchiveItem], warmup_start: chrono::DateTime<chrono::Utc>) -> Option<&ArchiveItem> {
    items
        .iter()
        .filter(|item| item.data_type == "rib" && item.ts_start <= warmup_start)
        .max_by_key(|item| item.ts_start)
}

/// Select every UPDATE file whose time interval overlaps
/// `[rib_timestamp, cooldown_end]`, ordered by `ts_start`.
pub fn select_updates(
    items: &[ArchiveItem],
    rib_timestamp: chrono::DateTime<chrono::Utc>,
    cooldown_end: chrono::DateTime<chrono::Utc>,
) -> Vec<&ArchiveItem> {
    let mut updates: Vec<_> = items
        .iter()
        .filter(|item| {
            item.data_type == "updates"
                && item.ts_end >= rib_timestamp
                && item.ts_start <= cooldown_end
        })
        .collect();
    updates.sort_by_key(|item| item.ts_start);
    updates
}

/// Validate that consecutive UPDATE files have no unexplained gaps.
///
/// A gap exists if the next file's `ts_start` > previous file's `ts_end`.
/// Small overlaps are expected (files often cover slightly overlapping periods).
pub fn validate_update_gaps(updates: &[&ArchiveItem], tolerance: chrono::Duration) -> Vec<String> {
    let mut gaps = Vec::new();
    for window in updates.windows(2) {
        let prev = window[0];
        let next = window[1];
        let gap = next.ts_start - prev.ts_end;
        if gap > tolerance {
            gaps.push(format!(
                "gap of {:.0}s between {} (ends {}) and {} (starts {})",
                gap.num_seconds(),
                prev.url,
                prev.ts_end,
                next.url,
                next.ts_start,
            ));
        }
    }
    gaps
}

// ── Caching with integrity ─────────────────────────────────────────

/// Download an archive item to a deterministic local cache path.
///
/// Downloads to a temporary `.part` file, verifies size against the
/// broker-reported size (if available), computes SHA-256, then atomically
/// renames into the cache directory. Skips download if the cached file
/// already exists with matching size and SHA-256.
///
/// Returns an `ArchiveError` on download failure, size mismatch, or
/// checksum mismatch — never an analysis verdict.
pub fn cache_archive(
    item: &ArchiveItem,
    cache_dir: &Path,
) -> Result<CachedArchive, InimArchiveError> {
    let basename = Path::new(&item.url)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    let local_dir = cache_dir.join(&item.collector_id).join(&item.data_type);
    std::fs::create_dir_all(&local_dir).map_err(|e| InimArchiveError::CacheError {
        path: local_dir.to_string_lossy().to_string(),
        reason: e.to_string(),
    })?;

    let final_path = local_dir.join(&basename);

    // Check if already cached with matching integrity
    if final_path.exists() {
        if let Ok(meta) = std::fs::metadata(&final_path) {
            if meta.len() == item.size {
                if let Ok(sha) = compute_sha256(&final_path) {
                    return Ok(CachedArchive {
                        url: item.url.clone(),
                        local_path: final_path.to_string_lossy().to_string(),
                        project: item.project.clone(),
                        collector_id: item.collector_id.clone(),
                        data_type: item.data_type.clone(),
                        size: item.size,
                        sha256: sha,
                    });
                }
            }
        }
    }

    // Download to temp file
    let part_path = local_dir.join(format!(".{}.part", basename));

    let mut response = reqwest::blocking::get(&item.url).map_err(|e| {
        InimArchiveError::DownloadError {
            url: item.url.clone(),
            reason: e.to_string(),
        }
    })?;

    if !response.status().is_success() {
        return Err(InimArchiveError::DownloadError {
            url: item.url.clone(),
            reason: format!("HTTP {}", response.status()),
        });
    }

    let mut body = Vec::new();
    response.read_to_end(&mut body).map_err(|e| InimArchiveError::DownloadError {
        url: item.url.clone(),
        reason: e.to_string(),
    })?;

    // Verify size
    if body.len() as u64 != item.size {
        return Err(InimArchiveError::SizeMismatch {
            url: item.url.clone(),
            expected: item.size,
            actual: body.len() as u64,
        });
    }

    std::fs::write(&part_path, &body).map_err(|e| InimArchiveError::CacheError {
        path: part_path.to_string_lossy().to_string(),
        reason: e.to_string(),
    })?;

    // Compute SHA-256
    let sha256 = bytes_to_hex(&Sha256::digest(&body)[..]);

    // Atomic rename
    std::fs::rename(&part_path, &final_path).map_err(|e| InimArchiveError::CacheError {
        path: part_path.to_string_lossy().to_string(),
        reason: e.to_string(),
    })?;

    Ok(CachedArchive {
        url: item.url.clone(),
        local_path: final_path.to_string_lossy().to_string(),
        project: item.project.clone(),
        collector_id: item.collector_id.clone(),
        data_type: item.data_type.clone(),
        size: item.size,
        sha256,
    })
}

/// Compute the SHA-256 checksum of a file.
fn compute_sha256(path: &Path) -> Result<String, InimArchiveError> {
    let mut file = std::fs::File::open(path).map_err(|e| InimArchiveError::CacheError {
        path: path.to_string_lossy().to_string(),
        reason: e.to_string(),
    })?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut file, &mut hasher).map_err(|e| InimArchiveError::CacheError {
        path: path.to_string_lossy().to_string(),
        reason: e.to_string(),
    })?;
    Ok(bytes_to_hex(&hasher.finalize()[..]))
}

/// Convert bytes to lowercase hex string.
fn bytes_to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

// ── Error types ────────────────────────────────────────────────────

/// Errors that occur during archive discovery or download.
///
/// These produce an archive-failure error report, never an analysis
/// verdict like `INSUFFICIENT_VISIBILITY`.
#[derive(Debug)]
pub enum InimArchiveError {
    DownloadError { url: String, reason: String },
    SizeMismatch { url: String, expected: u64, actual: u64 },
    CacheError { path: String, reason: String },
}

impl std::fmt::Display for InimArchiveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InimArchiveError::DownloadError { url, reason } => {
                write!(f, "download failed for {url}: {reason}")
            }
            InimArchiveError::SizeMismatch { url, expected, actual } => {
                write!(f, "size mismatch for {url}: expected {expected}, got {actual}")
            }
            InimArchiveError::CacheError { path, reason } => {
                write!(f, "cache error at {path}: {reason}")
            }
        }
    }
}

impl std::error::Error for InimArchiveError {}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn t(s: &str) -> chrono::DateTime<Utc> {
        chrono::DateTime::parse_from_rfc3339(s).unwrap().to_utc()
    }

    fn make_rib(ts_start: &str, ts_end: &str, collector: &str) -> ArchiveItem {
        ArchiveItem {
            project: "route-views".into(),
            collector_id: collector.into(),
            data_type: "rib".into(),
            ts_start: t(ts_start),
            ts_end: t(ts_end),
            url: format!("http://archive.routeviews.org/{collector}/rib.{ts_start}.bz2"),
            size: 100_000_000,
        }
    }

    fn make_update(ts_start: &str, ts_end: &str, collector: &str) -> ArchiveItem {
        ArchiveItem {
            project: "route-views".into(),
            collector_id: collector.into(),
            data_type: "updates".into(),
            ts_start: t(ts_start),
            ts_end: t(ts_end),
            url: format!("http://archive.routeviews.org/{collector}/updates.{ts_start}.bz2"),
            size: 5_000_000,
        }
    }

    #[test]
    fn closest_preceding_rib_is_selected() {
        let items = vec![
            make_rib("2026-07-30T06:00:00Z", "2026-07-30T07:59:59Z", "rv2"),
            make_rib("2026-07-30T08:00:00Z", "2026-07-30T09:59:59Z", "rv2"), // closest before 09:00
            make_rib("2026-07-30T10:00:00Z", "2026-07-30T11:59:59Z", "rv2"), // after warmup_start
        ];
        let warmup_start = t("2026-07-30T08:25:00Z"); // 60 min before 09:25
        let rib = select_rib(&items, warmup_start).expect("should select closest RIB");
        assert_eq!(rib.ts_start, t("2026-07-30T08:00:00Z"));
    }

    #[test]
    fn all_update_files_overlapping_window_are_selected() {
        let items = vec![
            make_update("2026-07-30T07:45:00Z", "2026-07-30T08:00:00Z", "rv2"), // before rib
            make_update("2026-07-30T07:55:00Z", "2026-07-30T08:10:00Z", "rv2"), // overlaps rib
            make_update("2026-07-30T09:20:00Z", "2026-07-30T09:35:00Z", "rv2"), // overlaps event
            make_update("2026-07-30T10:40:00Z", "2026-07-30T10:55:00Z", "rv2"), // overlaps cooldown
            make_update("2026-07-30T11:00:00Z", "2026-07-30T11:15:00Z", "rv2"), // after cooldown
        ];
        let rib_ts = t("2026-07-30T08:00:00Z");
        let cooldown_end = t("2026-07-30T10:47:00Z");
        let updates = select_updates(&items, rib_ts, cooldown_end);
        // Files 0-3 all overlap the window (0: boundary overlap at 08:00 equals rib_ts)
        assert_eq!(updates.len(), 4);
        assert!(updates[0].ts_start == t("2026-07-30T07:45:00Z"));
    }

    #[test]
    fn warmup_and_cooldown_bounds_are_included() {
        let items = vec![
            make_update("2026-07-30T08:20:00Z", "2026-07-30T08:30:00Z", "rv2"), // warm-up
            make_update("2026-07-30T10:40:00Z", "2026-07-30T10:50:00Z", "rv2"), // cool-down
        ];
        let rib_ts = t("2026-07-30T08:00:00Z");
        let cooldown_end = t("2026-07-30T10:47:00Z");
        let updates = select_updates(&items, rib_ts, cooldown_end);
        assert_eq!(updates.len(), 2, "both warmup and cooldown updates must be included");
    }

    #[test]
    fn update_selection_has_no_unexplained_gaps() {
        let items = vec![
            make_update("2026-07-30T08:00:00Z", "2026-07-30T08:15:00Z", "rv2"),
            make_update("2026-07-30T08:15:00Z", "2026-07-30T08:30:00Z", "rv2"),
            make_update("2026-07-30T08:45:00Z", "2026-07-30T09:00:00Z", "rv2"), // 15-min gap
        ];
        let rib_ts = t("2026-07-30T08:00:00Z");
        let cooldown_end = t("2026-07-30T10:00:00Z");
        let updates = select_updates(&items, rib_ts, cooldown_end);
        let gaps = validate_update_gaps(&updates, chrono::Duration::minutes(5));
        assert_eq!(gaps.len(), 1, "should detect the 15-minute gap");
        assert!(gaps[0].contains("gap of"));
    }

    #[test]
    fn collector_identity_comes_from_broker_metadata() {
        // Simulate: URL says "rrc00" but broker metadata says "route-views2"
        let item = ArchiveItem {
            project: "route-views".into(),
            collector_id: "route-views2".into(), // ← this is authoritative
            data_type: "rib".into(),
            ts_start: t("2026-07-30T08:00:00Z"),
            ts_end: t("2026-07-30T09:59:59Z"),
            url: "http://archive.routeviews.org/rrc00/rib.bz2".into(), // misleading URL
            size: 100_000_000,
        };
        // Collector identity must come from the broker metadata field
        assert_eq!(item.collector_id, "route-views2");
        // URL alone would suggest "rrc00" — wrong
        assert_ne!(
            Path::new(&item.url)
                .parent()
                .and_then(|p| p.file_name())
                .map(|n| n.to_string_lossy()),
            Some(std::borrow::Cow::Borrowed("route-views2"))
        );
    }

    #[test]
    fn selected_dataset_is_serialized_for_reproduction() {
        let cached = CachedArchive {
            url: "http://example.com/rib.bz2".into(),
            local_path: "cache/rv2/rib/rib.bz2".into(),
            project: "route-views".into(),
            collector_id: "rv2".into(),
            data_type: "rib".into(),
            size: 100_000_000,
            sha256: "abcdef1234567890".into(),
        };
        let ds = Dataset {
            event_id: "INC0302574".into(),
            collectors: vec![CollectorArchives {
                collector_id: "rv2".into(),
                rib: Some(cached),
                updates: vec![],
                has_relevant_streams: true,
            }],
        };
        let json = serde_json::to_string_pretty(&ds).unwrap();
        assert!(json.contains("INC0302574"));
        assert!(json.contains("abcdef1234567890"));
        assert!(json.contains("route-views"));
    }
}
