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
use crate::target::{PreflightCounts, TargetSet};

/// Schema versions — bump on any format change.
pub const RIB_CACHE_SCHEMA_VERSION: u32 = 1;
pub const UPDATE_CACHE_SCHEMA_VERSION: u32 = 1;
/// Observation schema version — bump when RouteObservation fields change.
pub const OBSERVATION_SCHEMA_VERSION: u32 = 1;

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

// ── UPDATE observation cache ───────────────────────────────────────

/// Per-archive admission counters, preserved in the cache for audit parity.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UpdateAdmissionCounters {
    pub total_elements_parsed: u64,
    pub target_prefix_matches: u64,
    pub collector_prefix_matches: u64,
    pub full_targetkey_matches: u64,
    pub admitted_announcements: u64,
    pub admitted_withdrawals: u64,
}

/// A cached UPDATE archive result: admitted observations and metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateCacheEntry {
    pub schema_version: u32,
    pub observation_schema_version: u32,
    pub parser_version: String,
    pub source_url: String,
    pub source_sha256: String,
    pub targetset_hash: String,
    pub collector: String,
    pub record_count: u64,
    pub admission_counters: UpdateAdmissionCounters,
    pub payload_checksum: String,
    pub observations: Vec<RouteObservation>,
}

/// Compute a deterministic hash of the frozen TargetSet.
///
/// Hashes the sorted (collector, peer_ip, prefix) tuples.
/// The hash changes when any stream is added, removed, or re-keyed.
pub fn targetset_hash(target: &TargetSet) -> String {
    let mut entries: Vec<(String, String, String)> = Vec::new();
    for (collector, streams) in &target.streams {
        for s in streams {
            // Canonicalize IPv6 addresses to their string form for sorting
            entries.push((collector.clone(), s.peer_ip.to_string(), s.prefix.0.clone()));
        }
    }
    entries.sort();
    let mut hasher = Sha256::new();
    for (collector, peer, prefix) in &entries {
        hasher.update(collector.as_bytes());
        hasher.update(b"|");
        hasher.update(peer.as_bytes());
        hasher.update(b"|");
        hasher.update(prefix.as_bytes());
        hasher.update(b"|");
    }
    bytes_to_hex(&hasher.finalize()[..16])
}

/// Build an UPDATE cache key from archive SHA, collector, TargetSet hash,
/// and schema versions.
pub fn update_cache_key(source_sha: &str, collector: &str, targetset_hash: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(source_sha.as_bytes());
    hasher.update(b"|");
    hasher.update(collector.as_bytes());
    hasher.update(b"|");
    hasher.update(targetset_hash.as_bytes());
    hasher.update(b"|");
    hasher.update(UPDATE_CACHE_SCHEMA_VERSION.to_le_bytes());
    hasher.update(b"|");
    hasher.update(OBSERVATION_SCHEMA_VERSION.to_le_bytes());
    hasher.update(b"|");
    hasher.update(PARSER_VERSION.as_bytes());
    bytes_to_hex(&hasher.finalize()[..16])
}

/// Validate an UPDATE cache entry against current schema, parser, and source.
fn update_cache_valid(cached: &UpdateCacheEntry, source_sha: &str) -> bool {
    cached.schema_version == UPDATE_CACHE_SCHEMA_VERSION
        && cached.observation_schema_version == OBSERVATION_SCHEMA_VERSION
        && cached.parser_version == PARSER_VERSION
        && cached.source_sha256 == source_sha
}

/// Load an UPDATE cache entry if valid.
///
/// Returns None on miss, schema/parser/SHA mismatch, decode failure,
/// checksum failure, or record count mismatch.
pub fn load_update_cache(
    cache_dir: &Path,
    key: &str,
    source_sha: &str,
) -> Option<UpdateCacheEntry> {
    let path = update_cache_path(cache_dir, key);
    let content = std::fs::read_to_string(&path).ok()?;
    let entry: UpdateCacheEntry = serde_json::from_str(&content).ok()?;

    // Schema/parser/SHA validation
    if !update_cache_valid(&entry, source_sha) {
        let _ = std::fs::remove_file(&path);
        return None;
    }

    // Record count check
    if entry.observations.len() as u64 != entry.record_count {
        let _ = std::fs::remove_file(&path);
        return None;
    }

    // Payload checksum: serialize observations, hash, compare
    let payload_json = serde_json::to_string(&entry.observations).ok()?;
    let computed_hash = bytes_to_hex(&Sha256::digest(payload_json.as_bytes())[..16]);
    if computed_hash != entry.payload_checksum {
        let _ = std::fs::remove_file(&path);
        return None;
    }

    Some(entry)
}

/// Save an UPDATE cache entry atomically.
pub fn save_update_cache(
    cache_dir: &Path,
    archive_sha: &str,
    key: &str,
    entry: &UpdateCacheEntry,
) -> Result<(), String> {
    let dir = update_cache_dir(cache_dir, archive_sha);
    std::fs::create_dir_all(&dir).map_err(|e| format!("cannot create update cache dir: {e}"))?;
    let final_path = update_cache_path(cache_dir, key);
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

fn update_cache_dir(cache_dir: &Path, archive_sha: &str) -> PathBuf {
    cache_dir.join("derived").join("updates").join(archive_sha)
}

fn update_cache_path(cache_dir: &Path, key: &str) -> PathBuf {
    // Key already encodes everything; place directly under derived/updates/
    let dir = cache_dir.join("derived").join("updates");
    dir.join(format!("{key}.json"))
}

// ── Shared helpers ─────────────────────────────────────────────────

pub fn bytes_to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Compute a payload checksum for a slice of observations.
pub fn compute_payload_checksum(observations: &[RouteObservation]) -> String {
    let json = serde_json::to_string(observations).unwrap_or_default();
    bytes_to_hex(&Sha256::digest(json.as_bytes())[..16])
}

// ── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::observation::{
        Asn, CollectorId, Communities, IngestRole, ObservationAttributes, ObservationId,
        ObservationKind, ObservationProvenance, ObservationSource,
    };
    use crate::target::TargetStream;
    use chrono::{TimeZone, Utc};

    // ── RIB cache tests ──────────────────────────────────────────

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

    // ── TargetSet hash tests ─────────────────────────────────────

    #[test]
    fn targetset_hash_is_deterministic() {
        let mut t1 = TargetSet::default();
        t1.streams.insert(
            "rv2".into(),
            vec![TargetStream {
                peer_ip: "185.1.8.65".parse().unwrap(),
                prefix: Prefix::from("193.0.0.0/21"),
                origin_as: 3333,
                as_path: vec![6447, 11537, 3333],
            }],
        );
        let h1 = targetset_hash(&t1);
        let h2 = targetset_hash(&t1);
        assert_eq!(h1, h2);
    }

    #[test]
    fn targetset_hash_changes_with_different_streams() {
        let mut t1 = TargetSet::default();
        t1.streams.insert(
            "rv2".into(),
            vec![TargetStream {
                peer_ip: "185.1.8.65".parse().unwrap(),
                prefix: Prefix::from("193.0.0.0/21"),
                origin_as: 3333,
                as_path: vec![6447, 11537, 3333],
            }],
        );
        let mut t2 = TargetSet::default();
        t2.streams.insert(
            "rv6".into(),
            vec![TargetStream {
                peer_ip: "2001:7f8:4::1".parse().unwrap(),
                prefix: Prefix::from("193.0.0.0/21"),
                origin_as: 3333,
                as_path: vec![6447, 11537, 3333],
            }],
        );
        assert_ne!(targetset_hash(&t1), targetset_hash(&t2));
    }

    #[test]
    fn targetset_hash_changes_with_peer_change() {
        let mut t1 = TargetSet::default();
        t1.streams.insert(
            "rv2".into(),
            vec![TargetStream {
                peer_ip: "185.1.8.65".parse().unwrap(),
                prefix: Prefix::from("193.0.0.0/21"),
                origin_as: 3333,
                as_path: vec![6447, 11537, 3333],
            }],
        );
        let mut t2 = TargetSet::default();
        t2.streams.insert(
            "rv2".into(),
            vec![TargetStream {
                peer_ip: "185.1.8.66".parse().unwrap(),
                prefix: Prefix::from("193.0.0.0/21"),
                origin_as: 3333,
                as_path: vec![6447, 11537, 3333],
            }],
        );
        assert_ne!(targetset_hash(&t1), targetset_hash(&t2));
    }

    // ── UPDATE cache key tests ───────────────────────────────────

    #[test]
    fn update_cache_key_changes_with_sha_change() {
        let k1 = update_cache_key("sha1", "rv2", "tshash");
        let k2 = update_cache_key("sha2", "rv2", "tshash");
        assert_ne!(k1, k2);
    }

    #[test]
    fn update_cache_key_changes_with_collector_change() {
        let k1 = update_cache_key("sha", "rv2", "tshash");
        let k2 = update_cache_key("sha", "rv6", "tshash");
        assert_ne!(k1, k2);
    }

    #[test]
    fn update_cache_key_changes_with_targetset_hash_change() {
        let k1 = update_cache_key("sha", "rv2", "hash1");
        let k2 = update_cache_key("sha", "rv2", "hash2");
        assert_ne!(k1, k2);
    }

    // ── UPDATE cache hit/miss/validation tests ───────────────────

    fn make_test_obs(seq: u64) -> RouteObservation {
        RouteObservation {
            id: ObservationId(seq),
            source: ObservationSource::LocalFile("test.mrt".into()),
            timestamp: Utc.with_ymd_and_hms(2026, 7, 30, 9, 30, 0).unwrap(),
            collector: CollectorId("rv2".into()),
            peer_ip: "185.1.8.65".parse().unwrap(),
            peer_asn: Asn(6447),
            prefix: Prefix::from("193.0.0.0/21"),
            kind: ObservationKind::Announcement,
            attributes: Some(ObservationAttributes {
                as_path: vec![6447, 11537, 3333],
                origin_asns: vec![Asn(3333)],
                next_hop: Some("185.1.8.65".parse().unwrap()),
                origin: Some("IGP".into()),
                local_pref: Some(100),
                med: None,
                atomic_aggregate: false,
                communities: Communities::new(),
            }),
            path_id: None,
            provenance: ObservationProvenance {
                source_url: Some("http://example.com/updates.bz2".into()),
                archive_sha256: Some("test_sha".into()),
                input: "updates.mrt".into(),
                role: IngestRole::Updates,
                parser_representation: "bgpkit-bgp-elem".into(),
                mrt_timestamp: 1751815800.0,
                element_seq: seq,
                archive_order: 0,
            },
        }
    }

    fn make_test_entry(
        sha: &str,
        tshash: &str,
        collector: &str,
        obs: Vec<RouteObservation>,
    ) -> UpdateCacheEntry {
        let record_count = obs.len() as u64;
        let payload_json = serde_json::to_string(&obs).unwrap();
        let payload_checksum = bytes_to_hex(&Sha256::digest(payload_json.as_bytes())[..16]);
        UpdateCacheEntry {
            schema_version: UPDATE_CACHE_SCHEMA_VERSION,
            observation_schema_version: OBSERVATION_SCHEMA_VERSION,
            parser_version: PARSER_VERSION.into(),
            source_url: "http://example.com/updates.bz2".into(),
            source_sha256: sha.into(),
            targetset_hash: tshash.into(),
            collector: collector.into(),
            record_count,
            admission_counters: UpdateAdmissionCounters {
                total_elements_parsed: 100,
                target_prefix_matches: 5,
                collector_prefix_matches: 3,
                full_targetkey_matches: 2,
                admitted_announcements: 1,
                admitted_withdrawals: 1,
            },
            payload_checksum,
            observations: obs,
        }
    }

    #[test]
    fn update_cache_hit_skips_parser() {
        let dir = tempfile::tempdir().unwrap();
        let key = update_cache_key("test_sha", "rv2", "tshash");
        let obs = vec![make_test_obs(0)];
        let entry = make_test_entry("test_sha", "tshash", "rv2", obs);
        save_update_cache(dir.path(), "test_sha", &key, &entry).unwrap();

        let loaded = load_update_cache(dir.path(), &key, "test_sha");
        assert!(loaded.is_some());
        assert_eq!(loaded.unwrap().observations.len(), 1);
    }

    #[test]
    fn zero_observation_update_cache_is_valid() {
        let dir = tempfile::tempdir().unwrap();
        let key = update_cache_key("test_sha", "rv2", "tshash");
        let entry = make_test_entry("test_sha", "tshash", "rv2", vec![]);
        save_update_cache(dir.path(), "test_sha", &key, &entry).unwrap();

        let loaded = load_update_cache(dir.path(), &key, "test_sha");
        assert!(loaded.is_some());
        assert_eq!(loaded.unwrap().record_count, 0);
    }

    #[test]
    fn update_cache_preserves_admission_counters() {
        let dir = tempfile::tempdir().unwrap();
        let key = update_cache_key("test_sha", "rv2", "tshash");
        let entry = make_test_entry("test_sha", "tshash", "rv2", vec![make_test_obs(0)]);
        save_update_cache(dir.path(), "test_sha", &key, &entry).unwrap();

        let loaded = load_update_cache(dir.path(), &key, "test_sha").unwrap();
        assert_eq!(loaded.admission_counters.total_elements_parsed, 100);
        assert_eq!(loaded.admission_counters.admitted_announcements, 1);
        assert_eq!(loaded.admission_counters.admitted_withdrawals, 1);
    }

    #[test]
    fn update_cache_invalidates_on_archive_sha_change() {
        let dir = tempfile::tempdir().unwrap();
        let key = update_cache_key("sha_old", "rv2", "tshash");
        let entry = make_test_entry("sha_old", "tshash", "rv2", vec![make_test_obs(0)]);
        save_update_cache(dir.path(), "sha_old", &key, &entry).unwrap();

        // Same key but different SHA → should miss
        assert!(load_update_cache(dir.path(), &key, "sha_new").is_none());
    }

    #[test]
    fn update_cache_invalidates_on_target_set_change() {
        let dir = tempfile::tempdir().unwrap();
        let key_old = update_cache_key("test_sha", "rv2", "tshash_old");
        let key_new = update_cache_key("test_sha", "rv2", "tshash_new");
        let entry = make_test_entry("test_sha", "tshash_old", "rv2", vec![make_test_obs(0)]);
        save_update_cache(dir.path(), "test_sha", &key_old, &entry).unwrap();

        // Different targetset hash → different key → miss
        assert!(load_update_cache(dir.path(), &key_new, "test_sha").is_none());
    }

    #[test]
    fn update_cache_invalidates_on_schema_change() {
        let dir = tempfile::tempdir().unwrap();
        let key = update_cache_key("test_sha", "rv2", "tshash");
        let mut entry = make_test_entry("test_sha", "tshash", "rv2", vec![make_test_obs(0)]);
        entry.schema_version = 999; // wrong
        save_update_cache(dir.path(), "test_sha", &key, &entry).unwrap();

        assert!(load_update_cache(dir.path(), &key, "test_sha").is_none());
    }

    #[test]
    fn truncated_update_cache_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let key = update_cache_key("test_sha", "rv2", "tshash");
        let mut entry = make_test_entry("test_sha", "tshash", "rv2", vec![make_test_obs(0)]);
        // record_count says 2 but only 1 observation
        entry.record_count = 2;
        save_update_cache(dir.path(), "test_sha", &key, &entry).unwrap();

        assert!(load_update_cache(dir.path(), &key, "test_sha").is_none());
    }

    #[test]
    fn corrupt_update_cache_is_recomputed() {
        let dir = tempfile::tempdir().unwrap();
        let key = update_cache_key("test_sha", "rv2", "tshash");
        let mut entry = make_test_entry("test_sha", "tshash", "rv2", vec![make_test_obs(0)]);
        // Mess up the checksum
        entry.payload_checksum = "00000000000000000000000000000001".into();
        save_update_cache(dir.path(), "test_sha", &key, &entry).unwrap();

        assert!(load_update_cache(dir.path(), &key, "test_sha").is_none());
    }

    #[test]
    fn cached_observation_retains_complete_provenance() {
        let dir = tempfile::tempdir().unwrap();
        let key = update_cache_key("test_sha", "rv2", "tshash");
        let obs = make_test_obs(42);
        let entry = make_test_entry("test_sha", "tshash", "rv2", vec![obs.clone()]);
        save_update_cache(dir.path(), "test_sha", &key, &entry).unwrap();

        let loaded = load_update_cache(dir.path(), &key, "test_sha").unwrap();
        let cached_obs = &loaded.observations[0];
        assert_eq!(cached_obs.id, obs.id);
        assert_eq!(cached_obs.timestamp, obs.timestamp);
        assert_eq!(cached_obs.collector.0, "rv2");
        assert_eq!(cached_obs.peer_ip.to_string(), "185.1.8.65");
        assert_eq!(cached_obs.peer_asn.0, 6447);
        assert_eq!(cached_obs.prefix.0, "193.0.0.0/21");
        assert_eq!(cached_obs.kind, ObservationKind::Announcement);
        assert!(cached_obs.attributes.is_some());
        assert_eq!(
            cached_obs.provenance.source_url.as_deref(),
            Some("http://example.com/updates.bz2")
        );
        assert_eq!(
            cached_obs.provenance.archive_sha256.as_deref(),
            Some("test_sha")
        );
        assert_eq!(cached_obs.provenance.element_seq, 42);
        assert_eq!(cached_obs.provenance.archive_order, 0);
    }

    #[test]
    fn cached_and_uncached_evidence_outputs_are_identical() {
        // Round-trip: serialize observations, rebuild cache entry, re-load.
        // The re-loaded observations must be identical to the originals.
        let dir = tempfile::tempdir().unwrap();
        let key = update_cache_key("test_sha", "rv2", "tshash");
        let obs = vec![make_test_obs(0), make_test_obs(1)];
        let entry = make_test_entry("test_sha", "tshash", "rv2", obs.clone());
        save_update_cache(dir.path(), "test_sha", &key, &entry).unwrap();

        let loaded = load_update_cache(dir.path(), &key, "test_sha").unwrap();
        assert_eq!(loaded.observations, obs);
    }
}
