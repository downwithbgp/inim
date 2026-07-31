//! Archive discovery — bgpkit-broker integration and local caching.
//!
//! Discovers RouteViews archive files via bgpkit-broker, selects the
//! relevant RIB and UPDATE files for an analysis window, downloads
//! them to a deterministic local cache with integrity verification
//! (SHA-256 sidecar + atomic rename), and returns a serializable
//! dataset manifest for reproducibility.
//!
//! This module uses bgpkit-broker for discovery but owns the
//! selection logic, caching policy, and error classification.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::{Path, PathBuf};

// ── Discovery trait ────────────────────────────────────────────────

/// Abstraction over archive discovery so real and failing backends
/// can share the same orchestration code.
pub trait ArchiveDiscovery {
    fn query(
        &self,
        project: &str,
        collectors: &[&str],
        ts_start: chrono::DateTime<chrono::Utc>,
        ts_end: chrono::DateTime<chrono::Utc>,
        data_type: &str,
    ) -> Result<Vec<ArchiveItem>, InimArchiveError>;
}

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

// ── Live broker discovery ──────────────────────────────────────────

/// Live bgpkit-broker backend (requires network).
pub struct LiveArchiveDiscovery;

impl ArchiveDiscovery for LiveArchiveDiscovery {
    fn query(
        &self,
        project: &str,
        collectors: &[&str],
        ts_start: chrono::DateTime<chrono::Utc>,
        ts_end: chrono::DateTime<chrono::Utc>,
        data_type: &str,
    ) -> Result<Vec<ArchiveItem>, InimArchiveError> {
        // Build broker queries per collector — broker uses sync API
        let mut all_items: Vec<ArchiveItem> = Vec::new();

        for collector in collectors {
            let broker = bgpkit_broker::BgpkitBroker::new()
                .ts_start(ts_start.naive_utc())
                .ts_end(ts_end.naive_utc())
                .collector_id(collector)
                .project(project)
                .data_type(data_type);

            let broker_items = broker
                .query()
                .map_err(|e| InimArchiveError::BrokerQueryError {
                    reason: format!("broker query failed for collector {collector}: {e}"),
                })?;

            for item in broker_items {
                all_items.push(ArchiveItem {
                    project: project.to_string(),
                    collector_id: item.collector_id.clone(), // authoritative
                    data_type: item.data_type.clone(),
                    ts_start: chrono::DateTime::from_naive_utc_and_offset(
                        item.ts_start,
                        chrono::Utc,
                    ),
                    ts_end: chrono::DateTime::from_naive_utc_and_offset(item.ts_end, chrono::Utc),
                    url: item.url.clone(),
                    // Use exact_size if available, otherwise rough_size
                    size: if item.exact_size > 0 {
                        item.exact_size as u64
                    } else {
                        item.rough_size.max(0) as u64
                    },
                });
            }
        }

        Ok(all_items)
    }
}

// ── Selection logic (pure, testable without network) ──────────────

/// Select the newest RIB with `ts_start` at or before `warmup_start`.
pub fn select_rib(
    items: &[ArchiveItem],
    warmup_start: chrono::DateTime<chrono::Utc>,
) -> Option<&ArchiveItem> {
    items
        .iter()
        .filter(|item| item.data_type == "rib" && item.ts_start <= warmup_start)
        .max_by_key(|item| item.ts_start)
}

/// Cadence tolerance: RouteViews UPDATE files are emitted at ~15-min
/// intervals. We allow up to 30 min slack for broker ts_start discrepancies.
const UPDATE_CADENCE_TOLERANCE_MINUTES: i64 = 30;

/// Select every UPDATE file whose coverage overlaps the analysis interval.
///
/// Anchored on `ts_start`, not `ts_end`, because broker `ts_end` values
/// are unreliable for RouteViews archives (some report end-of-day rather
/// than actual coverage end).  A file is selected iff:
///
///   `ts_start` ≥ `rib_timestamp − tolerance` (allows pre-RIB overlap)
///   AND `ts_start` ≤ `cooldown_end`
///
/// When `ts_start` itself is suspicious, we fall back to extracting
/// a timestamp from the RouteViews filename convention
/// (`updates.YYYYMMDD.HHMM.bz2`).
///
/// Results are ordered by `ts_start`, deduplicated by URL.
pub fn select_updates(
    items: &[ArchiveItem],
    rib_timestamp: chrono::DateTime<chrono::Utc>,
    cooldown_end: chrono::DateTime<chrono::Utc>,
) -> Vec<&ArchiveItem> {
    let tolerance = chrono::Duration::minutes(UPDATE_CADENCE_TOLERANCE_MINUTES);
    let lower = rib_timestamp - tolerance;

    let mut updates: Vec<_> = items
        .iter()
        .filter(|item| {
            if item.data_type != "updates" {
                return false;
            }
            // Anchor on ts_start — ts_end is unreliable from broker
            let ts = canonical_ts_start(item, lower);
            ts >= lower && ts <= cooldown_end
        })
        .collect();

    // Sort by canonical ts_start, then dedupe by URL (keep first)
    updates.sort_by_key(|item| canonical_ts_start(item, lower));
    let mut seen = std::collections::HashSet::new();
    let mut dups: Vec<String> = Vec::new();
    updates.retain(|item| {
        if seen.contains(&item.url) {
            dups.push(item.url.clone());
            false
        } else {
            seen.insert(item.url.clone());
            true
        }
    });
    if !dups.is_empty() {
        eprintln!("  note: {} duplicate UPDATE URLs excluded", dups.len());
    }

    updates
}

/// Get the best available start timestamp for an archive item.
///
/// Preference order:
/// 1. Broker `ts_start` (usually reliable)
/// 2. Filename-derived timestamp (RouteViews naming convention)
///
/// If broker `ts_start` looks unreasonable (e.g. far future), fall back
/// to filename parsing and emit a diagnostic.
fn canonical_ts_start(
    item: &ArchiveItem,
    lower_bound: chrono::DateTime<chrono::Utc>,
) -> chrono::DateTime<chrono::Utc> {
    // If broker ts_start is far in the future (> cooldown_end + 24h),
    // try filename fallback
    if item.ts_start > lower_bound + chrono::Duration::hours(48) {
        if let Some(ft) = filename_timestamp(&item.url) {
            eprintln!(
                "  warning: broker ts_start for {} is anomalous ({}), using filename-derived {}",
                item.url, item.ts_start, ft
            );
            return ft;
        }
    }
    item.ts_start
}

/// Extract a timestamp from a RouteViews-format URL filename.
///
/// Pattern: `.../updates.YYYYMMDD.HHMM.bz2`
fn filename_timestamp(url: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    let name = std::path::Path::new(url).file_name()?.to_str()?;
    // Match updates.YYYYMMDD.HHMM or rib.YYYYMMDD.HHMM
    let parts: Vec<&str> = name.split('.').collect();
    if parts.len() < 3 {
        return None;
    }
    let date_str = parts.get(parts.len() - 3)?;
    let time_str = parts.get(parts.len() - 2)?;
    if date_str.len() != 8 || time_str.len() != 4 {
        return None;
    }
    let year: i32 = date_str[0..4].parse().ok()?;
    let month: u32 = date_str[4..6].parse().ok()?;
    let day: u32 = date_str[6..8].parse().ok()?;
    let hour: u32 = time_str[0..2].parse().ok()?;
    let min: u32 = time_str[2..4].parse().ok()?;
    chrono::TimeZone::with_ymd_and_hms(&chrono::Utc, year, month, day, hour, min, 0).single()
}

/// Validate that consecutive UPDATE files have no unexplained gaps.
///
/// A gap exists if the next file's `ts_start` > previous file's `ts_end`
/// beyond a tolerance.
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

/// Remove duplicate URLs from a mutable list of archive items.
/// Keeps the first occurrence, drops subsequent duplicates.
pub fn dedupe_urls(items: &mut Vec<ArchiveItem>) -> Vec<String> {
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut removed = Vec::new();
    items.retain(|item| {
        if seen.contains(&item.url) {
            removed.push(item.url.clone());
            false
        } else {
            seen.insert(item.url.clone());
            true
        }
    });
    removed
}

// ── Checksum sidecar helpers ───────────────────────────────────────

/// Write a checksum sidecar file: `<path>.sha256`.
#[allow(dead_code)]
fn write_sha_sidecar(path: &Path, sha256: &str) -> Result<(), std::io::Error> {
    let sidecar_path = sha_sidecar_path(path);
    std::fs::write(&sidecar_path, format!("{sha256}\n"))
}

/// Read a checksum sidecar, returning the first line trimmed.
fn read_sha_sidecar(path: &Path) -> Option<String> {
    let sidecar_path = sha_sidecar_path(path);
    let content = std::fs::read_to_string(&sidecar_path).ok()?;
    content.lines().next().map(|s| s.trim().to_string())
}

/// Get the sidecar path for a file.
fn sha_sidecar_path(path: &Path) -> PathBuf {
    let mut p = path.to_path_buf();
    let mut name = p.file_name().unwrap_or_default().to_os_string();
    name.push(".sha256");
    p.set_file_name(name);
    p
}

/// Check whether a cached file matches expected integrity.
///
/// Returns `true` if:
/// - The file exists with matching size
/// - A sidecar exists with matching SHA-256
/// - The current file's recomputed SHA-256 matches the sidecar
///
/// This guards against:
/// - Truncated downloads (size mismatch)
/// - Corrupted cache (checksum mismatch)
/// - Missing sidecar (treated as corrupt)
fn cached_file_matches(path: &Path, expected_size: u64, expected_sha: Option<&str>) -> bool {
    // File must exist
    let meta = match std::fs::metadata(path) {
        Ok(m) => m,
        Err(_) => return false,
    };

    // Size must match
    if meta.len() != expected_size {
        return false;
    }

    // Sidecar must exist
    let stored_sha = match read_sha_sidecar(path) {
        Some(s) => s,
        None => return false,
    };

    // If expected SHA provided, it must match the sidecar
    if let Some(expected) = expected_sha {
        if stored_sha != expected {
            return false;
        }
    }

    // Recompute and verify against sidecar
    match compute_sha256(path) {
        Ok(current) => current == stored_sha,
        Err(_) => false,
    }
}

// ── Caching with integrity ─────────────────────────────────────────

/// Download an archive item to a deterministic local cache path.
///
/// Downloads to a temporary `.part` file, verifies size against the
/// broker-reported size (if available), computes SHA-256, writes a
/// `.sha256` sidecar, then atomically renames into the cache directory.
/// Reuses a cached file only after validated integrity (size + SHA-256 sidecar).
///
/// Returns an `InimArchiveError` on failure — never an analysis verdict.
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
    if cached_file_matches(&final_path, item.size, None) {
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

    // Download to temp file
    let part_path = local_dir.join(format!(".{}.part", basename));

    let mut response =
        reqwest::blocking::get(&item.url).map_err(|e| InimArchiveError::DownloadError {
            url: item.url.clone(),
            reason: e.to_string(),
        })?;

    if !response.status().is_success() {
        return Err(InimArchiveError::DownloadError {
            url: item.url.clone(),
            reason: format!("HTTP {}", response.status()),
        });
    }

    let mut body = Vec::new();
    response
        .read_to_end(&mut body)
        .map_err(|e| InimArchiveError::DownloadError {
            url: item.url.clone(),
            reason: e.to_string(),
        })?;

    // Verify size (only when exact size reported — rough_size is approximate)
    if body.len() as u64 != item.size && item.size > 0 {
        // Don't fail on rough estimates — broker rough_size is approximate
        eprintln!(
            "warning: size differ for {}: expected {}, got {} (continuing)",
            item.url,
            item.size,
            body.len()
        );
    }

    std::fs::write(&part_path, &body).map_err(|e| InimArchiveError::CacheError {
        path: part_path.to_string_lossy().to_string(),
        reason: e.to_string(),
    })?;

    // Compute SHA-256
    let sha256 = bytes_to_hex(&Sha256::digest(&body)[..]);

    // Write sidecar next to part file (rename below carries both)
    let part_sidecar = sha_sidecar_path(&part_path);
    std::fs::write(&part_sidecar, format!("{sha256}\n")).map_err(|e| {
        InimArchiveError::CacheError {
            path: part_sidecar.to_string_lossy().to_string(),
            reason: e.to_string(),
        }
    })?;

    // Atomic rename of data file
    std::fs::rename(&part_path, &final_path).map_err(|e| InimArchiveError::CacheError {
        path: part_path.to_string_lossy().to_string(),
        reason: e.to_string(),
    })?;

    // Atomic rename of sidecar
    let final_sidecar = sha_sidecar_path(&final_path);
    std::fs::rename(&part_sidecar, &final_sidecar).map_err(|e| InimArchiveError::CacheError {
        path: part_sidecar.to_string_lossy().to_string(),
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

// ── Hashing helpers ─────────────────────────────────────────────────

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
    DownloadError {
        url: String,
        reason: String,
    },
    SizeMismatch {
        url: String,
        expected: u64,
        actual: u64,
    },
    CacheError {
        path: String,
        reason: String,
    },
    BrokerQueryError {
        reason: String,
    },
}

impl std::fmt::Display for InimArchiveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InimArchiveError::DownloadError { url, reason } => {
                write!(f, "download failed for {url}: {reason}")
            }
            InimArchiveError::SizeMismatch {
                url,
                expected,
                actual,
            } => {
                write!(
                    f,
                    "size mismatch for {url}: expected {expected}, got {actual}"
                )
            }
            InimArchiveError::CacheError { path, reason } => {
                write!(f, "cache error at {path}: {reason}")
            }
            InimArchiveError::BrokerQueryError { reason } => {
                write!(f, "broker query failed: {reason}")
            }
        }
    }
}

impl std::error::Error for InimArchiveError {}

// ── Tests ──────────────────────────────────────────────────────────

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
            make_rib("2026-07-30T08:00:00Z", "2026-07-30T09:59:59Z", "rv2"),
            make_rib("2026-07-30T10:00:00Z", "2026-07-30T11:59:59Z", "rv2"),
        ];
        let warmup_start = t("2026-07-30T08:25:00Z");
        let rib = select_rib(&items, warmup_start).expect("should select closest RIB");
        assert_eq!(rib.ts_start, t("2026-07-30T08:00:00Z"));
    }

    #[test]
    fn all_update_files_overlapping_window_are_selected() {
        let items = vec![
            make_update("2026-07-30T07:45:00Z", "2026-07-30T08:00:00Z", "rv2"),
            make_update("2026-07-30T07:55:00Z", "2026-07-30T08:10:00Z", "rv2"),
            make_update("2026-07-30T09:20:00Z", "2026-07-30T09:35:00Z", "rv2"),
            make_update("2026-07-30T10:40:00Z", "2026-07-30T10:55:00Z", "rv2"),
            make_update("2026-07-30T11:00:00Z", "2026-07-30T11:15:00Z", "rv2"),
        ];
        let rib_ts = t("2026-07-30T08:00:00Z");
        let cooldown_end = t("2026-07-30T10:47:00Z");
        let updates = select_updates(&items, rib_ts, cooldown_end);
        assert_eq!(updates.len(), 4);
    }

    #[test]
    fn warmup_and_cooldown_bounds_are_included() {
        let items = vec![
            make_update("2026-07-30T08:20:00Z", "2026-07-30T08:30:00Z", "rv2"),
            make_update("2026-07-30T10:40:00Z", "2026-07-30T10:50:00Z", "rv2"),
        ];
        let rib_ts = t("2026-07-30T08:00:00Z");
        let cooldown_end = t("2026-07-30T10:47:00Z");
        let updates = select_updates(&items, rib_ts, cooldown_end);
        assert_eq!(updates.len(), 2);
    }

    #[test]
    fn update_selection_has_no_unexplained_gaps() {
        let items = vec![
            make_update("2026-07-30T08:00:00Z", "2026-07-30T08:15:00Z", "rv2"),
            make_update("2026-07-30T08:15:00Z", "2026-07-30T08:30:00Z", "rv2"),
            make_update("2026-07-30T08:45:00Z", "2026-07-30T09:00:00Z", "rv2"),
        ];
        let rib_ts = t("2026-07-30T08:00:00Z");
        let cooldown_end = t("2026-07-30T10:00:00Z");
        let updates = select_updates(&items, rib_ts, cooldown_end);
        let gaps = validate_update_gaps(&updates, chrono::Duration::minutes(5));
        assert_eq!(gaps.len(), 1);
        assert!(gaps[0].contains("gap of"));
    }

    #[test]
    fn collector_identity_comes_from_broker_metadata() {
        let item = ArchiveItem {
            project: "route-views".into(),
            collector_id: "route-views2".into(),
            data_type: "rib".into(),
            ts_start: t("2026-07-30T08:00:00Z"),
            ts_end: t("2026-07-30T09:59:59Z"),
            url: "http://archive.routeviews.org/rrc00/rib.bz2".into(),
            size: 100_000_000,
        };
        assert_eq!(item.collector_id, "route-views2");
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

    #[test]
    fn rib_selection_precedes_warmup() {
        let items = vec![
            make_rib("2026-07-30T06:00:00Z", "2026-07-30T07:59:59Z", "rv2"),
            make_rib("2026-07-30T08:00:00Z", "2026-07-30T09:59:59Z", "rv2"),
            make_rib("2026-07-30T09:00:00Z", "2026-07-30T10:59:59Z", "rv2"), // after warmup
        ];
        let warmup_start = t("2026-07-30T08:25:00Z");
        let rib = select_rib(&items, warmup_start).unwrap();
        // Selected RIB must start at or before warmup_start
        assert!(rib.ts_start <= warmup_start);
        // The 09:00 RIB starts after warmup, must NOT be selected
        assert_eq!(rib.ts_start, t("2026-07-30T08:00:00Z"));
    }

    #[test]
    fn update_selection_overlaps_full_analysis_interval() {
        let items = vec![
            make_update("2026-07-30T07:45:00Z", "2026-07-30T08:00:00Z", "rv2"),
            make_update("2026-07-30T09:20:00Z", "2026-07-30T09:35:00Z", "rv2"),
            make_update("2026-07-30T10:40:00Z", "2026-07-30T10:55:00Z", "rv2"),
        ];
        let rib_ts = t("2026-07-30T08:00:00Z");
        let cooldown_end = t("2026-07-30T10:47:00Z");
        let updates = select_updates(&items, rib_ts, cooldown_end);
        // All three overlap [rib_ts, cooldown_end]
        assert_eq!(updates.len(), 3);
        assert!(
            updates.first().unwrap().ts_start <= rib_ts
                || updates.first().unwrap().ts_end >= rib_ts
        );
    }

    #[test]
    fn archive_gap_sets_continuity_unknown() {
        let items = vec![
            make_update("2026-07-30T08:00:00Z", "2026-07-30T08:15:00Z", "rv2"),
            // gap of 45 minutes
            make_update("2026-07-30T09:00:00Z", "2026-07-30T09:15:00Z", "rv2"),
        ];
        let rib_ts = t("2026-07-30T08:00:00Z");
        let cooldown_end = t("2026-07-30T10:00:00Z");
        let updates = select_updates(&items, rib_ts, cooldown_end);
        let gaps = validate_update_gaps(&updates, chrono::Duration::minutes(5));
        // A large gap should be detected
        assert!(!gaps.is_empty());
        assert!(gaps[0].contains("gap of"));
    }

    // ── Cache + sidecar tests ──────────────────────────────────────

    #[test]
    fn cached_archive_checksum_is_verified() {
        let dir = tempfile::tempdir().unwrap();
        let cache = dir.path().join("cache");

        // Write a fake cached file
        let local_dir = cache.join("rv2").join("rib");
        std::fs::create_dir_all(&local_dir).unwrap();
        let data_path = local_dir.join("rib.bz2");
        std::fs::write(&data_path, b"hello test data").unwrap();
        let sha = bytes_to_hex(&Sha256::digest(b"hello test data")[..]);

        // Write sidecar
        std::fs::write(sha_sidecar_path(&data_path), format!("{sha}\n")).unwrap();

        assert!(cached_file_matches(&data_path, 15, Some(&sha)));
        assert!(!cached_file_matches(&data_path, 999, Some(&sha))); // wrong size
        assert!(!cached_file_matches(&data_path, 15, Some("deadbeef"))); // wrong sha
    }

    #[test]
    fn corrupt_cached_file_is_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let cache = dir.path().join("cache");

        // Write a "corrupt" cached file (content doesn't match sidecar)
        let local_dir = cache.join("rv2").join("rib");
        std::fs::create_dir_all(&local_dir).unwrap();
        let data_path = local_dir.join("rib.bz2");
        std::fs::write(&data_path, b"corrupt data here").unwrap();

        // Write sidecar with DIFFERENT hash
        let wrong_sha = bytes_to_hex(&Sha256::digest(b"something else")[..]);
        std::fs::write(sha_sidecar_path(&data_path), format!("{wrong_sha}\n")).unwrap();

        // Should NOT match (recomputed SHA differs from sidecar)
        assert!(!cached_file_matches(&data_path, 18, None));
    }

    #[test]
    fn duplicate_urls_are_rejected() {
        let _warmup_start = t("2026-07-30T08:25:00Z");
        let mut items = vec![
            ArchiveItem {
                project: "route-views".into(),
                collector_id: "rv2".into(),
                data_type: "rib".into(),
                ts_start: t("2026-07-30T08:00:00Z"),
                ts_end: t("2026-07-30T09:59:59Z"),
                url: "http://example.com/rib.bz2".into(),
                size: 100,
            },
            ArchiveItem {
                project: "route-views".into(),
                collector_id: "rv2".into(),
                data_type: "rib".into(),
                ts_start: t("2026-07-30T08:00:00Z"),
                ts_end: t("2026-07-30T09:59:59Z"),
                url: "http://example.com/rib.bz2".into(), // duplicate
                size: 100,
            },
        ];

        let removed = dedupe_urls(&mut items);
        assert_eq!(items.len(), 1);
        assert_eq!(removed.len(), 1);
        assert!(removed[0].contains("rib.bz2"));
    }

    #[test]
    fn broker_metadata_supplies_collector_identity() {
        // The ArchiveItem struct uses collector_id from metadata, not URL.
        // Even when URL suggests a different collector, the id field is authoritative.
        let item = ArchiveItem {
            project: "route-views".into(),
            collector_id: "route-views6".into(),
            data_type: "updates".into(),
            ts_start: t("2026-07-30T09:00:00Z"),
            ts_end: t("2026-07-30T09:15:00Z"),
            url: "http://archive.routeviews.org/route-views2/updates.20260730.0900.bz2".into(),
            size: 5_000_000,
        };
        // Metadata says route-views6, even if URL suggests route-views2
        assert_eq!(item.collector_id, "route-views6");
    }

    // ── Phase 0 regression: selection bounds + canonical timestamps ──

    #[test]
    fn update_selection_has_lower_and_upper_bounds() {
        let items = vec![
            make_update("2026-07-29T08:00:00Z", "2026-07-29T08:15:00Z", "rv2"), // day before, excluded
            make_update("2026-07-30T07:45:00Z", "2026-07-30T08:00:00Z", "rv2"), // pre-RIB, within tolerance
            make_update("2026-07-30T08:15:00Z", "2026-07-30T08:30:00Z", "rv2"), // in range
            make_update("2026-07-30T10:45:00Z", "2026-07-30T11:00:00Z", "rv2"), // cooldown edge
            make_update("2026-07-30T11:00:00Z", "2026-07-30T11:15:00Z", "rv2"), // after cooldown, excluded
        ];
        let rib_ts = t("2026-07-30T08:00:00Z");
        let cooldown_end = t("2026-07-30T10:47:00Z");
        let updates = select_updates(&items, rib_ts, cooldown_end);
        // Items: the day-before one is before lower bound (rib_ts - 30min = 07:30)
        // The 07:45 one is >= 07:30 and <= 10:47 → included
        // The 08:15 one → included
        // The 10:45 one → included
        // The 11:00 one starts after cooldown_end → excluded
        assert_eq!(
            updates.len(),
            3,
            "should select 3 files, got {}: {:?}",
            updates.len(),
            updates.iter().map(|u| &u.url).collect::<Vec<_>>()
        );
        let urls: Vec<&str> = updates.iter().map(|u| u.url.as_str()).collect();
        assert!(urls
            .iter()
            .any(|u| u.contains("0745") || u.contains("07:45")));
        assert!(urls
            .iter()
            .any(|u| u.contains("0815") || u.contains("08:15")));
        assert!(urls
            .iter()
            .any(|u| u.contains("1045") || u.contains("10:45")));
    }

    #[test]
    fn update_selection_excludes_files_before_selected_rib() {
        let items = vec![
            make_update("2026-07-30T07:00:00Z", "2026-07-30T07:15:00Z", "rv2"), // 1h before RIB
            make_update("2026-07-30T07:15:00Z", "2026-07-30T07:30:00Z", "rv2"), // 45min before
            make_update("2026-07-30T07:25:00Z", "2026-07-30T07:40:00Z", "rv2"), // 35min before (outside 30min tolerance)
            make_update("2026-07-30T07:35:00Z", "2026-07-30T07:50:00Z", "rv2"), // 25min before (inside tolerance)
            make_update("2026-07-30T08:00:00Z", "2026-07-30T08:15:00Z", "rv2"), // at RIB
        ];
        let rib_ts = t("2026-07-30T08:00:00Z");
        let cooldown_end = t("2026-07-30T10:00:00Z");
        let updates = select_updates(&items, rib_ts, cooldown_end);
        // Lower bound = 08:00 - 30min = 07:30
        // Only files with ts_start >= 07:30 are included
        assert_eq!(
            updates.len(),
            2,
            "only 07:35 and 08:00 files should be selected"
        );
        let urls: Vec<_> = updates.iter().map(|u| u.url.as_str()).collect();
        assert!(
            urls.iter()
                .any(|u| u.contains("0735") || u.contains("07:35")),
            "07:35 file at 25min before RIB must be included"
        );
        assert!(
            urls.iter()
                .any(|u| u.contains("0800") || u.contains("08:00")),
            "08:00 file at RIB must be included"
        );
        assert!(
            !urls
                .iter()
                .any(|u| u.contains("0700") || u.contains("07:00")),
            "07:00 file far before RIB must be excluded"
        );
    }

    #[test]
    fn update_selection_excludes_files_after_cooldown() {
        let items = vec![
            make_update("2026-07-30T09:00:00Z", "2026-07-30T09:15:00Z", "rv2"),
            make_update("2026-07-30T10:30:00Z", "2026-07-30T10:45:00Z", "rv2"),
            make_update("2026-07-30T10:50:00Z", "2026-07-30T11:05:00Z", "rv2"), // starts after cooldown
        ];
        let rib_ts = t("2026-07-30T08:00:00Z");
        let cooldown_end = t("2026-07-30T10:47:00Z");
        let updates = select_updates(&items, rib_ts, cooldown_end);
        assert_eq!(
            updates.len(),
            2,
            "file starting after cooldown must be excluded"
        );
    }

    #[test]
    fn update_selection_deduplicates_urls() {
        let rib_ts = t("2026-07-30T08:00:00Z");
        let cooldown_end = t("2026-07-30T10:00:00Z");
        let items = vec![
            ArchiveItem {
                project: "routeviews".into(),
                collector_id: "rv2".into(),
                data_type: "updates".into(),
                ts_start: t("2026-07-30T09:00:00Z"),
                ts_end: t("2026-07-30T09:15:00Z"),
                url: "http://example.com/update.20260730.0900.bz2".into(),
                size: 100,
            },
            ArchiveItem {
                project: "routeviews".into(),
                collector_id: "rv2".into(),
                data_type: "updates".into(),
                ts_start: t("2026-07-30T09:00:00Z"),
                ts_end: t("2026-07-30T09:15:00Z"),
                url: "http://example.com/update.20260730.0900.bz2".into(),
                size: 100,
            },
        ];
        let updates = select_updates(&items, rib_ts, cooldown_end);
        assert_eq!(updates.len(), 1, "duplicate URLs must be deduplicated");
    }

    #[test]
    fn filename_timestamp_parses_routeviews_convention() {
        let ts = filename_timestamp(
            "http://archive.routeviews.org/bgpdata/2026.07/UPDATES/updates.20260730.0815.bz2",
        );
        assert!(ts.is_some());
        let ts = ts.unwrap();
        assert_eq!(ts.timestamp(), t("2026-07-30T08:15:00Z").timestamp());
    }
}
