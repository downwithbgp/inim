//! Derived caches — disposable, reproducible from source archives.
//!
//! RIB preflight cache: keyed by archive SHA-256 + collector + predicate hash.
//! UPDATE observation cache: keyed by archive SHA-256 + frozen TargetSet hash.
//!
//! All caches are atomic (temp → flush → rename) and invalidated on:
//! source SHA change, predicate/TargetSet change, schema version change,
//! parser version mismatch, missing metadata, decode failure,
//! content-hash validation failure.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::Write;
use std::net::IpAddr;
use std::path::{Path, PathBuf};

use crate::domain::observation::RouteObservation;
use crate::domain::route::Prefix;
use crate::target::PreflightCounts;

/// Schema version — bump on any format change.
pub const RIB_CACHE_SCHEMA_VERSION: u32 = 1;
#[allow(dead_code)]
const UPDATE_CACHE_SCHEMA_VERSION: u32 = 1;

/// Parser version pinned (bgpkit-parser not exposing a version const).
pub const PARSER_VERSION: &str = "0.19.0";

// ── RIB preflight cache ────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RibCacheEntry {
    pub schema_version: u32,
    pub parser_version: String,
    pub source_url: String,
    pub source_sha256: String,
    pub collector: String,
    pub predicate_repr: String,
    pub preflight: PreflightCounts,
    pub frozen_streams: Vec<CachedTargetStream>,
    pub baseline_observations: Vec<RouteObservation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedTargetStream {
    pub peer_ip: IpAddr,
    pub peer_asn: u32,
    pub prefix: Prefix,
    pub baseline_as_path: Vec<u32>,
}

/// Build a RIB cache key from the source SHA, collector, and predicate components.
pub fn rib_cache_key(
    source_sha: &str,
    collector: &str,
    origin_asns: &[u32],
    transit_asn: u32,
    manifest_revision: u32,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(source_sha.as_bytes());
    hasher.update(b"|");
    hasher.update(collector.as_bytes());
    hasher.update(b"|");
    for asn in origin_asns {
        hasher.update(asn.to_le_bytes());
    }
    hasher.update(b"|");
    hasher.update(transit_asn.to_le_bytes());
    hasher.update(b"|");
    hasher.update(manifest_revision.to_le_bytes());
    hasher.update(b"|");
    hasher.update(RIB_CACHE_SCHEMA_VERSION.to_le_bytes());
    hasher.update(b"|");
    hasher.update(PARSER_VERSION.as_bytes());
    bytes_to_hex(&hasher.finalize()[..16])
}

/// Check whether a cached RIB entry is valid (schema version, parser version, source SHA match).
fn rib_cache_valid(cached: &RibCacheEntry, source_sha: &str) -> bool {
    cached.schema_version == RIB_CACHE_SCHEMA_VERSION
        && cached.parser_version == PARSER_VERSION
        && cached.source_sha256 == source_sha
}

/// Load a RIB cache entry if valid; returns None on miss, corruption, or invalidation.
pub fn load_rib_cache(cache_dir: &Path, key: &str, source_sha: &str) -> Option<RibCacheEntry> {
    let path = rib_cache_path(cache_dir, key);
    let content = std::fs::read_to_string(&path).ok()?;
    let entry: RibCacheEntry = serde_json::from_str(&content).ok()?;
    if !rib_cache_valid(&entry, source_sha) {
        let _ = std::fs::remove_file(&path);
        return None;
    }
    // Content hash check: re-serialize and verify against stored content
    let recomputed = serde_json::to_string(&entry).ok()?;
    if bytes_to_hex(&Sha256::digest(recomputed.as_bytes())[..16])
        != bytes_to_hex(&Sha256::digest(content.as_bytes())[..16])
    {
        let _ = std::fs::remove_file(&path);
        return None;
    }
    Some(entry)
}

/// Save a RIB cache entry atomically.
pub fn save_rib_cache(cache_dir: &Path, key: &str, entry: &RibCacheEntry) -> Result<(), String> {
    let dir = cache_dir.join("rib");
    std::fs::create_dir_all(&dir).map_err(|e| format!("cannot create cache dir: {e}"))?;
    let final_path = rib_cache_path(cache_dir, key);
    let tmp_path = dir.join(format!(".{key}.part"));
    let content = serde_json::to_string(entry).map_err(|e| format!("serialization error: {e}"))?;
    let mut f =
        std::fs::File::create(&tmp_path).map_err(|e| format!("cannot create temp cache: {e}"))?;
    f.write_all(content.as_bytes())
        .map_err(|e| format!("cannot write cache: {e}"))?;
    f.flush().map_err(|e| format!("cannot flush cache: {e}"))?;
    std::fs::rename(&tmp_path, &final_path).map_err(|e| format!("cannot rename cache: {e}"))?;
    Ok(())
}

fn rib_cache_path(cache_dir: &Path, key: &str) -> PathBuf {
    cache_dir.join("rib").join(format!("{key}.json"))
}

// ── Shared helpers ─────────────────────────────────────────────────

fn bytes_to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

// ── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rib_cache_key_changes_with_origin_asn_change() {
        let k1 = rib_cache_key("sha", "rv2", &[3333], 11537, 1);
        let k2 = rib_cache_key("sha", "rv2", &[3333, 225], 11537, 1);
        assert_ne!(k1, k2);
    }

    #[test]
    fn rib_cache_key_changes_with_transit_asn_change() {
        let k1 = rib_cache_key("sha", "rv2", &[3333], 11537, 1);
        let k2 = rib_cache_key("sha", "rv2", &[3333], 11538, 1);
        assert_ne!(k1, k2);
    }

    #[test]
    fn rib_cache_key_changes_with_sha_change() {
        let k1 = rib_cache_key("abc", "rv2", &[3333], 11537, 1);
        let k2 = rib_cache_key("def", "rv2", &[3333], 11537, 1);
        assert_ne!(k1, k2);
    }

    #[test]
    fn rib_cache_key_changes_with_revision_change() {
        let k1 = rib_cache_key("sha", "rv2", &[3333], 11537, 1);
        let k2 = rib_cache_key("sha", "rv2", &[3333], 11537, 2);
        assert_ne!(k1, k2);
    }

    #[test]
    fn rib_cache_hit_and_miss() {
        let dir = tempfile::tempdir().unwrap();
        let key = rib_cache_key("test_sha", "rv2", &[3333], 11537, 1);

        // Miss when no cache exists
        assert!(load_rib_cache(dir.path(), &key, "test_sha").is_none());

        // Save and hit
        let entry = RibCacheEntry {
            schema_version: RIB_CACHE_SCHEMA_VERSION,
            parser_version: PARSER_VERSION.into(),
            source_url: "http://example.com/rib.bz2".into(),
            source_sha256: "test_sha".into(),
            collector: "rv2".into(),
            predicate_repr: "origin=3333 transit=11537".into(),
            preflight: PreflightCounts {
                collectors_requested: 1,
                collectors_with_usable_ribs: 1,
                origin_matching_routes: 10,
                transit_matching_routes: 5,
                frozen_streams: 5,
                distinct_prefixes: 3,
                distinct_peers: 2,
            },
            frozen_streams: vec![],
            baseline_observations: vec![],
        };
        save_rib_cache(dir.path(), &key, &entry).unwrap();
        assert!(load_rib_cache(dir.path(), &key, "test_sha").is_some());

        // Miss with wrong SHA
        assert!(load_rib_cache(dir.path(), &key, "wrong_sha").is_none());
    }

    #[test]
    fn rib_cache_invalidates_on_schema_change() {
        let dir = tempfile::tempdir().unwrap();
        let key = rib_cache_key("sha", "rv2", &[3333], 11537, 1);
        let entry = RibCacheEntry {
            schema_version: 999, // wrong
            parser_version: PARSER_VERSION.into(),
            source_url: "u".into(),
            source_sha256: "sha".into(),
            collector: "rv2".into(),
            predicate_repr: "x".into(),
            preflight: PreflightCounts {
                collectors_requested: 0,
                collectors_with_usable_ribs: 0,
                origin_matching_routes: 0,
                transit_matching_routes: 0,
                frozen_streams: 0,
                distinct_prefixes: 0,
                distinct_peers: 0,
            },
            frozen_streams: vec![],
            baseline_observations: vec![],
        };
        save_rib_cache(dir.path(), &key, &entry).unwrap();
        assert!(load_rib_cache(dir.path(), &key, "sha").is_none());
    }
}
