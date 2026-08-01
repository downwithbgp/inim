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
use crate::domain::route::{Prefix, RouteKey, TransitPredicate};
pub use crate::schema::{
    OBSERVATION_SCHEMA_VERSION, RIB_CACHE_SCHEMA_VERSION, UPDATE_CACHE_SCHEMA_VERSION,
};
use crate::target::{PreflightCounts, TargetSet};

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
    /// Reviewed entity identity (origin ASNs) used for the preflight.
    pub entity_origin_asns: Vec<u32>,
    /// Canonical TransitPredicate identity used for the preflight.
    pub transit_predicate_identity: String,
    /// Frozen sorted ObserverPrefixKey cohort identity.
    pub cohort_identity: String,
    /// Every baseline RouteKey including path_id.
    pub baseline_route_keys: Vec<RouteKey>,
    pub preflight: PreflightCounts,
    pub frozen_streams: Vec<CachedTargetStream>,
    /// Evidenced baseline route states (admitted RIB observations).
    pub baseline_observations: Vec<RouteObservation>,
    /// Payload checksum over the baseline observations.
    pub payload_checksum: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedTargetStream {
    pub peer_ip: IpAddr,
    pub peer_asn: u32,
    pub prefix: Prefix,
    pub baseline_as_path: Vec<u32>,
}

/// Canonical string identity of a TransitPredicate.
///
/// The identity includes the variant and its ordered ASN values, so any
/// predicate change invalidates dependent caches. Serialization is the
/// canonical representation: `ContainsAny[1,2]`, `ContainsAll[1]`, `Adjacent(1,2)`.
pub fn transit_predicate_identity(predicate: &TransitPredicate) -> String {
    match predicate {
        TransitPredicate::ContainsAny(asns) => {
            format!(
                "ContainsAny[{}]",
                asns.iter()
                    .map(|a| a.to_string())
                    .collect::<Vec<_>>()
                    .join(",")
            )
        }
        TransitPredicate::ContainsAll(asns) => {
            format!(
                "ContainsAll[{}]",
                asns.iter()
                    .map(|a| a.to_string())
                    .collect::<Vec<_>>()
                    .join(",")
            )
        }
        TransitPredicate::Adjacent(a, b) => format!("Adjacent({a},{b})"),
    }
}

/// Build a RIB cache key from the source SHA, collector, and predicate components.
///
/// `source_family` is part of the identity: a collector identifier is only
/// meaningful together with its family (RouteViews vs RIPE RIS), so caches
/// can never collide across families.
pub fn rib_cache_key(
    source_sha: &str,
    collector: &str,
    origin_asns: &[u32],
    transit_predicate: &TransitPredicate,
    manifest_revision: u32,
    source_family: &str,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(source_sha.as_bytes());
    hasher.update(b"|");
    hasher.update(source_family.as_bytes());
    hasher.update(b"|");
    hasher.update(collector.as_bytes());
    hasher.update(b"|");
    for asn in origin_asns {
        hasher.update(asn.to_le_bytes());
    }
    hasher.update(b"|");
    hasher.update(transit_predicate_identity(transit_predicate).as_bytes());
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
    // Payload checksum over the baseline observations
    let payload_json = serde_json::to_string(&entry.baseline_observations).ok()?;
    let computed_hash = bytes_to_hex(&Sha256::digest(payload_json.as_bytes())[..16]);
    if computed_hash != entry.payload_checksum {
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
    /// Frozen cohort identity (ObserverPrefixKey values) for this cache.
    pub cohort_identity: String,
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

/// Compute the frozen cohort identity hash.
///
/// Hashes the sorted ObserverPrefixKey values ONLY. The hash must not vary
/// because baseline path IDs are ordered differently — instance assignment
/// does not change the stream set.
pub fn cohort_hash(cohort: &crate::cohort::FrozenCohort) -> String {
    let mut entries: Vec<(String, String, String)> = cohort
        .observer_prefixes
        .iter()
        .map(|k| {
            (
                k.collector.clone(),
                k.peer_ip.to_string(),
                k.prefix.0.clone(),
            )
        })
        .collect();
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

// ── Deterministic observation identity ─────────────────────────────

/// Deterministic comparator implementing the documented identity ordering:
/// collector, timestamp, archive order, element sequence, peer IP, prefix,
/// path_id (None < Some(id)).
///
/// Completion order (serial vs parallel) never changes the identity order.
pub fn deterministic_observation_order(
    a: &RouteObservation,
    b: &RouteObservation,
) -> std::cmp::Ordering {
    a.collector
        .0
        .cmp(&b.collector.0)
        .then_with(|| a.timestamp.cmp(&b.timestamp))
        .then_with(|| a.provenance.archive_order.cmp(&b.provenance.archive_order))
        .then_with(|| a.provenance.element_seq.cmp(&b.provenance.element_seq))
        .then_with(|| a.peer_ip.to_string().cmp(&b.peer_ip.to_string()))
        .then_with(|| a.prefix.0.cmp(&b.prefix.0))
        .then_with(|| path_id_key(&a.path_id).cmp(&path_id_key(&b.path_id)))
}

/// Path ID ordering key: None < Some(id).
pub fn path_id_key(path_id: &Option<u32>) -> (u8, u32) {
    match path_id {
        None => (0, 0),
        Some(id) => (1, *id),
    }
}

/// Sort observations in deterministic identity order.
pub fn sort_deterministic(observations: &mut [RouteObservation]) {
    observations.sort_by(deterministic_observation_order);
}

/// Assign deterministic sequential ObservationIds in identity order.
///
/// IDs are assigned AFTER sorting, so serial and parallel completion
/// produce identical IDs. Different route instances (e.g. different
/// path_id) receive different IDs.
pub fn assign_deterministic_ids(observations: &mut [RouteObservation]) {
    for (i, obs) in observations.iter_mut().enumerate() {
        obs.id = crate::domain::observation::ObservationId(i as u64);
    }
}

/// Build an UPDATE cache key from archive SHA, collector, TargetSet hash,
/// and schema versions. `source_family` is part of the identity so
/// RouteViews and RIPE RIS caches can never collide.
pub fn update_cache_key(
    source_sha: &str,
    collector: &str,
    targetset_hash: &str,
    source_family: &str,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(source_sha.as_bytes());
    hasher.update(b"|");
    hasher.update(source_family.as_bytes());
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
    use std::collections::BTreeMap;

    // ── RIB cache tests ──────────────────────────────────────────

    #[test]
    fn rib_cache_key_changes_with_origin_asn_change() {
        let k1 = rib_cache_key(
            "sha",
            "rv2",
            &[3333],
            &TransitPredicate::ContainsAny(vec![11537]),
            1,
            "RouteViews",
        );
        let k2 = rib_cache_key(
            "sha",
            "rv2",
            &[3333, 225],
            &TransitPredicate::ContainsAny(vec![11537]),
            1,
            "RouteViews",
        );
        assert_ne!(k1, k2);
    }

    #[test]
    fn rib_cache_key_changes_with_transit_asn_change() {
        let k1 = rib_cache_key(
            "sha",
            "rv2",
            &[3333],
            &TransitPredicate::ContainsAny(vec![11537]),
            1,
            "RouteViews",
        );
        let k2 = rib_cache_key(
            "sha",
            "rv2",
            &[3333],
            &TransitPredicate::ContainsAny(vec![11538]),
            1,
            "RouteViews",
        );
        assert_ne!(k1, k2);
    }

    #[test]
    fn rib_cache_key_changes_with_sha_change() {
        let k1 = rib_cache_key(
            "abc",
            "rv2",
            &[3333],
            &TransitPredicate::ContainsAny(vec![11537]),
            1,
            "RouteViews",
        );
        let k2 = rib_cache_key(
            "def",
            "rv2",
            &[3333],
            &TransitPredicate::ContainsAny(vec![11537]),
            1,
            "RouteViews",
        );
        assert_ne!(k1, k2);
    }

    #[test]
    fn rib_cache_key_changes_with_revision_change() {
        let k1 = rib_cache_key(
            "sha",
            "rv2",
            &[3333],
            &TransitPredicate::ContainsAny(vec![11537]),
            1,
            "RouteViews",
        );
        let k2 = rib_cache_key(
            "sha",
            "rv2",
            &[3333],
            &TransitPredicate::ContainsAny(vec![11537]),
            2,
            "RouteViews",
        );
        assert_ne!(k1, k2);
    }

    #[test]
    fn rib_cache_hit_and_miss() {
        let dir = tempfile::tempdir().unwrap();
        let key = rib_cache_key(
            "test_sha",
            "rv2",
            &[3333],
            &TransitPredicate::ContainsAny(vec![11537]),
            1,
            "RouteViews",
        );

        // Miss when no cache exists
        assert!(load_rib_cache(dir.path(), &key, "test_sha").is_none());

        // Save and hit
        let entry = RibCacheEntry {
            schema_version: RIB_CACHE_SCHEMA_VERSION,
            parser_version: PARSER_VERSION.into(),
            source_url: "http://example.com/rib.bz2".into(),
            source_sha256: "test_sha".into(),
            collector: "rv2".into(),
            predicate_repr: "origin=3333 transit=ContainsAny[11537]".into(),
            entity_origin_asns: vec![3333],
            transit_predicate_identity: "ContainsAny[11537]".into(),
            cohort_identity: "cohort-test".into(),
            baseline_route_keys: vec![],
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
            payload_checksum: crate::derived_cache::compute_payload_checksum(&[]),
        };
        save_rib_cache(dir.path(), &key, &entry).unwrap();
        assert!(load_rib_cache(dir.path(), &key, "test_sha").is_some());

        // Miss with wrong SHA
        assert!(load_rib_cache(dir.path(), &key, "wrong_sha").is_none());
    }

    #[test]
    fn rib_cache_invalidates_on_schema_change() {
        let dir = tempfile::tempdir().unwrap();
        let key = rib_cache_key(
            "sha",
            "rv2",
            &[3333],
            &TransitPredicate::ContainsAny(vec![11537]),
            1,
            "RouteViews",
        );
        let entry = RibCacheEntry {
            schema_version: 999, // wrong
            parser_version: PARSER_VERSION.into(),
            source_url: "u".into(),
            source_sha256: "sha".into(),
            collector: "rv2".into(),
            predicate_repr: "x".into(),
            entity_origin_asns: vec![],
            transit_predicate_identity: "x".into(),
            cohort_identity: "x".into(),
            baseline_route_keys: vec![],
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
            payload_checksum: crate::derived_cache::compute_payload_checksum(&[]),
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
                path_id: None,
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
                path_id: None,
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
                path_id: None,
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
                path_id: None,
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
                path_id: None,
            }],
        );
        assert_ne!(targetset_hash(&t1), targetset_hash(&t2));
    }

    // ── UPDATE cache key tests ───────────────────────────────────

    #[test]
    fn update_cache_key_changes_with_sha_change() {
        let k1 = update_cache_key("sha1", "rv2", "tshash", "RouteViews");
        let k2 = update_cache_key("sha2", "rv2", "tshash", "RouteViews");
        assert_ne!(k1, k2);
    }

    #[test]
    fn update_cache_key_changes_with_collector_change() {
        let k1 = update_cache_key("sha", "rv2", "tshash", "RouteViews");
        let k2 = update_cache_key("sha", "rv6", "tshash", "RouteViews");
        assert_ne!(k1, k2);
    }

    #[test]
    fn update_cache_key_changes_with_targetset_hash_change() {
        let k1 = update_cache_key("sha", "rv2", "hash1", "RouteViews");
        let k2 = update_cache_key("sha", "rv2", "hash2", "RouteViews");
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
            cohort_identity: tshash.into(),
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
        let key = update_cache_key("test_sha", "rv2", "tshash", "RouteViews");
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
        let key = update_cache_key("test_sha", "rv2", "tshash", "RouteViews");
        let entry = make_test_entry("test_sha", "tshash", "rv2", vec![]);
        save_update_cache(dir.path(), "test_sha", &key, &entry).unwrap();

        let loaded = load_update_cache(dir.path(), &key, "test_sha");
        assert!(loaded.is_some());
        assert_eq!(loaded.unwrap().record_count, 0);
    }

    #[test]
    fn update_cache_preserves_admission_counters() {
        let dir = tempfile::tempdir().unwrap();
        let key = update_cache_key("test_sha", "rv2", "tshash", "RouteViews");
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
        let key = update_cache_key("sha_old", "rv2", "tshash", "RouteViews");
        let entry = make_test_entry("sha_old", "tshash", "rv2", vec![make_test_obs(0)]);
        save_update_cache(dir.path(), "sha_old", &key, &entry).unwrap();

        // Same key but different SHA → should miss
        assert!(load_update_cache(dir.path(), &key, "sha_new").is_none());
    }

    #[test]
    fn update_cache_invalidates_on_target_set_change() {
        let dir = tempfile::tempdir().unwrap();
        let key_old = update_cache_key("test_sha", "rv2", "tshash_old", "RouteViews");
        let key_new = update_cache_key("test_sha", "rv2", "tshash_new", "RouteViews");
        let entry = make_test_entry("test_sha", "tshash_old", "rv2", vec![make_test_obs(0)]);
        save_update_cache(dir.path(), "test_sha", &key_old, &entry).unwrap();

        // Different targetset hash → different key → miss
        assert!(load_update_cache(dir.path(), &key_new, "test_sha").is_none());
    }

    #[test]
    fn update_cache_invalidates_on_schema_change() {
        let dir = tempfile::tempdir().unwrap();
        let key = update_cache_key("test_sha", "rv2", "tshash", "RouteViews");
        let mut entry = make_test_entry("test_sha", "tshash", "rv2", vec![make_test_obs(0)]);
        entry.schema_version = 999; // wrong
        save_update_cache(dir.path(), "test_sha", &key, &entry).unwrap();

        assert!(load_update_cache(dir.path(), &key, "test_sha").is_none());
    }

    #[test]
    fn truncated_update_cache_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let key = update_cache_key("test_sha", "rv2", "tshash", "RouteViews");
        let mut entry = make_test_entry("test_sha", "tshash", "rv2", vec![make_test_obs(0)]);
        // record_count says 2 but only 1 observation
        entry.record_count = 2;
        save_update_cache(dir.path(), "test_sha", &key, &entry).unwrap();

        assert!(load_update_cache(dir.path(), &key, "test_sha").is_none());
    }

    #[test]
    fn corrupt_update_cache_is_recomputed() {
        let dir = tempfile::tempdir().unwrap();
        let key = update_cache_key("test_sha", "rv2", "tshash", "RouteViews");
        let mut entry = make_test_entry("test_sha", "tshash", "rv2", vec![make_test_obs(0)]);
        // Mess up the checksum
        entry.payload_checksum = "00000000000000000000000000000001".into();
        save_update_cache(dir.path(), "test_sha", &key, &entry).unwrap();

        assert!(load_update_cache(dir.path(), &key, "test_sha").is_none());
    }

    #[test]
    fn cached_observation_retains_complete_provenance() {
        let dir = tempfile::tempdir().unwrap();
        let key = update_cache_key("test_sha", "rv2", "tshash", "RouteViews");
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
        let key = update_cache_key("test_sha", "rv2", "tshash", "RouteViews");
        let obs = vec![make_test_obs(0), make_test_obs(1)];
        let entry = make_test_entry("test_sha", "tshash", "rv2", obs.clone());
        save_update_cache(dir.path(), "test_sha", &key, &entry).unwrap();

        let loaded = load_update_cache(dir.path(), &key, "test_sha").unwrap();
        assert_eq!(loaded.observations, obs);
    }

    // ── Part 4: schema versioning + deterministic identity ────────

    #[test]
    fn old_rib_schema_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let key = rib_cache_key(
            "sha",
            "rv2",
            &[3333],
            &TransitPredicate::ContainsAny(vec![11537]),
            1,
            "RouteViews",
        );
        let mut entry = RibCacheEntry {
            schema_version: RIB_CACHE_SCHEMA_VERSION,
            parser_version: PARSER_VERSION.into(),
            source_url: "u".into(),
            source_sha256: "sha".into(),
            collector: "rv2".into(),
            predicate_repr: "x".into(),
            entity_origin_asns: vec![3333],
            transit_predicate_identity: "ContainsAny[11537]".into(),
            cohort_identity: "c".into(),
            baseline_route_keys: vec![],
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
            payload_checksum: compute_payload_checksum(&[]),
        };
        entry.schema_version = 1; // pre-ADD-PATH schema
        save_rib_cache(dir.path(), &key, &entry).unwrap();
        assert!(
            load_rib_cache(dir.path(), &key, "sha").is_none(),
            "old RIB schema must be rejected"
        );
    }

    #[test]
    fn rib_cache_preserves_multiple_instances_per_stream() {
        let dir = tempfile::tempdir().unwrap();
        let key = rib_cache_key(
            "sha",
            "rv2",
            &[3333],
            &TransitPredicate::ContainsAny(vec![11537]),
            1,
            "RouteViews",
        );
        let mut obs1 = make_test_obs(0);
        obs1.path_id = Some(1);
        let mut obs2 = make_test_obs(1);
        obs2.path_id = Some(2);
        let mut entry = RibCacheEntry {
            schema_version: RIB_CACHE_SCHEMA_VERSION,
            parser_version: PARSER_VERSION.into(),
            source_url: "u".into(),
            source_sha256: "sha".into(),
            collector: "rv2".into(),
            predicate_repr: "x".into(),
            entity_origin_asns: vec![3333],
            transit_predicate_identity: "ContainsAny[11537]".into(),
            cohort_identity: "c".into(),
            baseline_route_keys: vec![
                RouteKey::with_path_id(
                    "rv2",
                    "185.1.8.65".parse().unwrap(),
                    &Prefix::from("193.0.0.0/21"),
                    Some(1),
                ),
                RouteKey::with_path_id(
                    "rv2",
                    "185.1.8.65".parse().unwrap(),
                    &Prefix::from("193.0.0.0/21"),
                    Some(2),
                ),
            ],
            preflight: PreflightCounts {
                collectors_requested: 0,
                collectors_with_usable_ribs: 0,
                origin_matching_routes: 0,
                transit_matching_routes: 0,
                frozen_streams: 1,
                distinct_prefixes: 1,
                distinct_peers: 1,
            },
            frozen_streams: vec![],
            baseline_observations: vec![obs1, obs2],
            payload_checksum: compute_payload_checksum(&[]),
        };
        entry.payload_checksum = compute_payload_checksum(&entry.baseline_observations);
        save_rib_cache(dir.path(), &key, &entry).unwrap();
        let loaded = load_rib_cache(dir.path(), &key, "sha").unwrap();
        // Two instances preserved for one stream.
        assert_eq!(loaded.baseline_observations.len(), 2);
        assert_eq!(loaded.baseline_route_keys.len(), 2);
        let pids: Vec<Option<u32>> = loaded
            .baseline_observations
            .iter()
            .map(|o| o.path_id)
            .collect();
        assert_eq!(pids, vec![Some(1), Some(2)]);
    }

    #[test]
    fn rib_cache_preserves_path_id() {
        let dir = tempfile::tempdir().unwrap();
        let key = rib_cache_key(
            "sha",
            "rv2",
            &[3333],
            &TransitPredicate::ContainsAny(vec![11537]),
            1,
            "RouteViews",
        );
        let mut obs = make_test_obs(7);
        obs.path_id = Some(4242);
        let entry = RibCacheEntry {
            schema_version: RIB_CACHE_SCHEMA_VERSION,
            parser_version: PARSER_VERSION.into(),
            source_url: "u".into(),
            source_sha256: "sha".into(),
            collector: "rv2".into(),
            predicate_repr: "x".into(),
            entity_origin_asns: vec![3333],
            transit_predicate_identity: "ContainsAny[11537]".into(),
            cohort_identity: "c".into(),
            baseline_route_keys: vec![],
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
            baseline_observations: vec![obs],
            payload_checksum: compute_payload_checksum(&[]),
        };
        // recompute checksum properly for the save
        let mut entry = entry;
        entry.payload_checksum = compute_payload_checksum(&entry.baseline_observations);
        save_rib_cache(dir.path(), &key, &entry).unwrap();
        let loaded = load_rib_cache(dir.path(), &key, "sha").unwrap();
        assert_eq!(loaded.baseline_observations[0].path_id, Some(4242));
    }

    #[test]
    fn rib_cache_preserves_all_evidence() {
        let dir = tempfile::tempdir().unwrap();
        let key = rib_cache_key(
            "sha",
            "rv2",
            &[3333],
            &TransitPredicate::ContainsAny(vec![11537]),
            1,
            "RouteViews",
        );
        let obs = make_test_obs(3);
        let mut entry = RibCacheEntry {
            schema_version: RIB_CACHE_SCHEMA_VERSION,
            parser_version: PARSER_VERSION.into(),
            source_url: "http://example.com/rib.bz2".into(),
            source_sha256: "sha".into(),
            collector: "rv2".into(),
            predicate_repr: "x".into(),
            entity_origin_asns: vec![3333],
            transit_predicate_identity: "ContainsAny[11537]".into(),
            cohort_identity: "c".into(),
            baseline_route_keys: vec![],
            preflight: PreflightCounts {
                collectors_requested: 1,
                collectors_with_usable_ribs: 1,
                origin_matching_routes: 1,
                transit_matching_routes: 1,
                frozen_streams: 1,
                distinct_prefixes: 1,
                distinct_peers: 1,
            },
            frozen_streams: vec![],
            baseline_observations: vec![obs.clone()],
            payload_checksum: compute_payload_checksum(&[]),
        };
        entry.payload_checksum = compute_payload_checksum(&entry.baseline_observations);
        save_rib_cache(dir.path(), &key, &entry).unwrap();
        let loaded = load_rib_cache(dir.path(), &key, "sha").unwrap();
        // Source URL, SHA, collector, entity, predicate identity preserved.
        assert_eq!(loaded.source_url, "http://example.com/rib.bz2");
        assert_eq!(loaded.source_sha256, "sha");
        assert_eq!(loaded.collector, "rv2");
        assert_eq!(loaded.entity_origin_asns, vec![3333]);
        assert_eq!(loaded.transit_predicate_identity, "ContainsAny[11537]");
        assert_eq!(loaded.preflight.frozen_streams, 1);
    }

    #[test]
    fn cohort_hash_ignores_instance_order() {
        // Same ObserverPrefixKey set, different baseline instance path-ID
        // ordering → identical cohort hash.
        let mut c1 = crate::cohort::FrozenCohort::default();
        let k = crate::domain::route::ObserverPrefixKey {
            collector: "rv2".into(),
            peer_ip: "185.1.8.65".parse().unwrap(),
            prefix: Prefix::from("193.0.0.0/21"),
        };
        c1.observer_prefixes.insert(k.clone());
        let mut c2 = crate::cohort::FrozenCohort::default();
        c2.observer_prefixes.insert(k.clone());
        // Instance ordering differs (baseline_instances not populated in c2
        // would change nothing for the stream-set hash).
        let mut b1 = BTreeMap::new();
        b1.insert(
            RouteKey::with_path_id("rv2", "185.1.8.65".parse().unwrap(), &k.prefix, Some(2)),
            crate::domain::route::RouteState {
                prefix: k.prefix.clone(),
                attributes: crate::domain::route::RouteAttributes::from_as_path(vec![1, 2, 3]),
                timestamp: chrono::Utc::now(),
                observer: "rv2:185.1.8.65".into(),
                path_id: Some(2),
            },
        );
        c1.baseline_instances.insert(k.clone(), b1.clone());
        let mut b2 = BTreeMap::new();
        b2.insert(
            RouteKey::with_path_id("rv2", "185.1.8.65".parse().unwrap(), &k.prefix, Some(1)),
            crate::domain::route::RouteState {
                prefix: k.prefix.clone(),
                attributes: crate::domain::route::RouteAttributes::from_as_path(vec![1, 2, 3]),
                timestamp: chrono::Utc::now(),
                observer: "rv2:185.1.8.65".into(),
                path_id: Some(1),
            },
        );
        c2.baseline_instances.insert(k.clone(), b2);
        assert_eq!(
            cohort_hash(&c1),
            cohort_hash(&c2),
            "cohort hash must not vary with baseline path-ID ordering"
        );
    }

    #[test]
    fn cohort_hash_changes_when_stream_set_changes() {
        let mut c1 = crate::cohort::FrozenCohort::default();
        c1.observer_prefixes
            .insert(crate::domain::route::ObserverPrefixKey {
                collector: "rv2".into(),
                peer_ip: "185.1.8.65".parse().unwrap(),
                prefix: Prefix::from("193.0.0.0/21"),
            });
        let mut c2 = c1.clone();
        c2.observer_prefixes
            .insert(crate::domain::route::ObserverPrefixKey {
                collector: "rv2".into(),
                peer_ip: "185.1.8.66".parse().unwrap(),
                prefix: Prefix::from("193.0.0.0/21"),
            });
        assert_ne!(cohort_hash(&c1), cohort_hash(&c2));
    }

    #[test]
    fn predicate_identity_change_invalidates_rib_cache() {
        // Different predicate → different cache key → the old cache is a miss.
        let dir = tempfile::tempdir().unwrap();
        let key_old = rib_cache_key(
            "sha",
            "rv2",
            &[3333],
            &TransitPredicate::ContainsAny(vec![11537]),
            1,
            "RouteViews",
        );
        let key_new = rib_cache_key(
            "sha",
            "rv2",
            &[3333],
            &TransitPredicate::ContainsAny(vec![11538]),
            1,
            "RouteViews",
        );
        assert_ne!(key_old, key_new);
        let mut entry = RibCacheEntry {
            schema_version: RIB_CACHE_SCHEMA_VERSION,
            parser_version: PARSER_VERSION.into(),
            source_url: "u".into(),
            source_sha256: "sha".into(),
            collector: "rv2".into(),
            predicate_repr: "x".into(),
            entity_origin_asns: vec![3333],
            transit_predicate_identity: "ContainsAny[11537]".into(),
            cohort_identity: "c".into(),
            baseline_route_keys: vec![],
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
            payload_checksum: compute_payload_checksum(&[]),
        };
        entry.payload_checksum = compute_payload_checksum(&entry.baseline_observations);
        save_rib_cache(dir.path(), &key_old, &entry).unwrap();
        assert!(
            load_rib_cache(dir.path(), &key_new, "sha").is_none(),
            "predicate identity change must invalidate the RIB cache"
        );
    }

    #[test]
    fn old_update_schema_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let key = update_cache_key("test_sha", "rv2", "tshash", "RouteViews");
        let mut entry = make_test_entry("test_sha", "tshash", "rv2", vec![make_test_obs(0)]);
        entry.schema_version = 1; // pre-ADD-PATH schema
        save_update_cache(dir.path(), "test_sha", &key, &entry).unwrap();
        assert!(load_update_cache(dir.path(), &key, "test_sha").is_none());
    }

    #[test]
    fn update_cache_preserves_path_id() {
        let dir = tempfile::tempdir().unwrap();
        let key = update_cache_key("test_sha", "rv2", "tshash", "RouteViews");
        let mut obs = make_test_obs(1);
        obs.path_id = Some(77);
        let entry = make_test_entry("test_sha", "tshash", "rv2", vec![obs]);
        save_update_cache(dir.path(), "test_sha", &key, &entry).unwrap();
        let loaded = load_update_cache(dir.path(), &key, "test_sha").unwrap();
        assert_eq!(loaded.observations[0].path_id, Some(77));
        assert_eq!(loaded.cohort_identity, "tshash");
    }

    #[test]
    fn update_cache_preserves_communities() {
        let dir = tempfile::tempdir().unwrap();
        let key = update_cache_key("test_sha", "rv2", "tshash", "RouteViews");
        let mut obs = make_test_obs(2);
        obs.attributes.as_mut().unwrap().communities =
            Communities::from_strings(vec!["65535:0".into(), "11537:100".into()]);
        let entry = make_test_entry("test_sha", "tshash", "rv2", vec![obs]);
        save_update_cache(dir.path(), "test_sha", &key, &entry).unwrap();
        let loaded = load_update_cache(dir.path(), &key, "test_sha").unwrap();
        assert_eq!(
            loaded.observations[0]
                .attributes
                .as_ref()
                .unwrap()
                .communities
                .values,
            vec!["65535:0".to_string(), "11537:100".to_string()]
        );
    }

    #[test]
    fn update_cache_preserves_complete_attributes() {
        let dir = tempfile::tempdir().unwrap();
        let key = update_cache_key("test_sha", "rv2", "tshash", "RouteViews");
        let obs = make_test_obs(3);
        let entry = make_test_entry("test_sha", "tshash", "rv2", vec![obs.clone()]);
        save_update_cache(dir.path(), "test_sha", &key, &entry).unwrap();
        let loaded = load_update_cache(dir.path(), &key, "test_sha").unwrap();
        let attrs = loaded.observations[0].attributes.as_ref().unwrap();
        assert_eq!(attrs.as_path, vec![6447, 11537, 3333]);
        assert_eq!(attrs.origin_asns, vec![Asn(3333)]);
        assert_eq!(attrs.next_hop, Some("185.1.8.65".parse().unwrap()));
        assert_eq!(attrs.origin.as_deref(), Some("IGP"));
        assert_eq!(attrs.local_pref, Some(100));
        assert_eq!(attrs.med, None);
        // archive order + element sequence preserved.
        assert_eq!(loaded.observations[0].provenance.archive_order, 0);
        assert_eq!(loaded.observations[0].provenance.element_seq, 3);
    }

    // ── Deterministic identity ────────────────────────────────────

    fn make_identity_obs(seq: u64, path_id: Option<u32>) -> RouteObservation {
        let mut obs = make_test_obs(seq);
        obs.path_id = path_id;
        obs
    }

    #[test]
    fn observation_id_changes_with_path_id() {
        let a = make_identity_obs(1, Some(1));
        let b = make_identity_obs(1, Some(2));
        let mut list = vec![a.clone(), b.clone()];
        sort_deterministic(&mut list);
        assign_deterministic_ids(&mut list);
        // Identical except path_id → distinct positions → distinct IDs.
        assert_ne!(list[0].id, list[1].id);
        assert_ne!(a.path_id, b.path_id);
        // The path-id tiebreaker orders Some(1) before Some(2).
        assert_eq!(list[0].path_id, Some(1));
        assert_eq!(list[1].path_id, Some(2));
    }

    #[test]
    fn evidence_id_changes_with_path_id() {
        let a = make_identity_obs(1, Some(1));
        let b = make_identity_obs(1, Some(2));
        let mut list = vec![a.clone(), b.clone()];
        sort_deterministic(&mut list);
        assign_deterministic_ids(&mut list);
        let ea = crate::domain::observation::EvidenceRef::from_observation(&list[0]);
        let eb = crate::domain::observation::EvidenceRef::from_observation(&list[1]);
        assert_ne!(ea.observation_id, eb.observation_id);
        assert_ne!(ea.path_id, eb.path_id);
    }

    #[test]
    fn deterministic_sort_orders_path_ids() {
        // None < Some(id) per the documented identity contract.
        let none = make_identity_obs(1, None);
        let some = make_identity_obs(1, Some(5));
        let mut list = vec![some.clone(), none.clone()];
        sort_deterministic(&mut list);
        assert_eq!(list[0].path_id, None);
        assert_eq!(list[1].path_id, Some(5));
    }

    #[test]
    fn parallel_completion_order_does_not_change_ids() {
        // Input order (serial vs parallel completion) must not change the
        // sorted identity order or the assigned IDs.
        let obs = vec![
            make_identity_obs(0, None),
            make_identity_obs(1, Some(1)),
            make_identity_obs(2, None),
            make_identity_obs(3, Some(2)),
        ];
        // serial order
        let mut serial = obs.clone();
        sort_deterministic(&mut serial);
        assign_deterministic_ids(&mut serial);
        // "parallel" shuffled order
        let mut parallel = vec![
            obs[3].clone(),
            obs[0].clone(),
            obs[2].clone(),
            obs[1].clone(),
        ];
        sort_deterministic(&mut parallel);
        assign_deterministic_ids(&mut parallel);
        let ids_serial: Vec<u64> = serial.iter().map(|o| o.id.0).collect();
        let ids_parallel: Vec<u64> = parallel.iter().map(|o| o.id.0).collect();
        assert_eq!(ids_serial, ids_parallel);
        for (a, b) in serial.iter().zip(parallel.iter()) {
            assert_eq!(a.path_id, b.path_id);
        }
    }

    #[test]
    fn serial_and_parallel_artifacts_match() {
        // End-to-end: the observation payload (as serialized into caches /
        // evidence) is identical regardless of input completion order.
        let obs = vec![
            make_identity_obs(0, None),
            make_identity_obs(1, Some(1)),
            make_identity_obs(2, None),
        ];
        let mut serial = obs.clone();
        sort_deterministic(&mut serial);
        assign_deterministic_ids(&mut serial);
        let mut parallel = vec![obs[2].clone(), obs[0].clone(), obs[1].clone()];
        sort_deterministic(&mut parallel);
        assign_deterministic_ids(&mut parallel);
        assert_eq!(serial, parallel);
        assert_eq!(
            compute_payload_checksum(&serial),
            compute_payload_checksum(&parallel)
        );
    }
}

#[cfg(test)]
mod session34_ris_cache_tests {
    use super::*;

    /// Derived caches are family-scoped: a RIPE RIS key round-trips its
    /// own entry and can never collide with a RouteViews key.
    #[test]
    fn ris_cache_roundtrip_preserves_source_family() {
        let dir = tempfile::tempdir().unwrap();
        let ris_key = rib_cache_key(
            "ris_sha",
            "rrc00",
            &[2603],
            &TransitPredicate::ContainsAny(vec![11537]),
            1,
            "RipeRis",
        );
        let rv_key = rib_cache_key(
            "ris_sha",
            "rrc00",
            &[2603],
            &TransitPredicate::ContainsAny(vec![11537]),
            1,
            "RouteViews",
        );
        assert_ne!(ris_key, rv_key, "family must be part of cache identity");

        let entry = RibCacheEntry {
            schema_version: RIB_CACHE_SCHEMA_VERSION,
            parser_version: PARSER_VERSION.into(),
            source_url: "https://data.ris.ripe.net/rrc00/2019.08/bview.20190821.0000.gz".into(),
            source_sha256: "ris_sha".into(),
            collector: "rrc00".into(),
            predicate_repr: "origin=2603 transit=ContainsAny[11537]".into(),
            entity_origin_asns: vec![2603],
            transit_predicate_identity: "ContainsAny[11537]".into(),
            cohort_identity: "cohort-ris".into(),
            baseline_route_keys: vec![],
            preflight: PreflightCounts {
                collectors_requested: 1,
                collectors_with_usable_ribs: 1,
                origin_matching_routes: 1,
                transit_matching_routes: 1,
                frozen_streams: 0,
                distinct_prefixes: 0,
                distinct_peers: 0,
            },
            frozen_streams: vec![],
            baseline_observations: vec![],
            payload_checksum: bytes_to_hex(&Sha256::digest(b"[]")[..16]),
        };
        save_rib_cache(dir.path(), &ris_key, &entry).unwrap();
        let loaded = load_rib_cache(dir.path(), &ris_key, "ris_sha");
        assert!(loaded.is_some(), "RIS cache must round-trip");
        assert_eq!(loaded.unwrap().source_sha256, "ris_sha");
        // The RouteViews key must NOT find the RIS entry.
        assert!(load_rib_cache(dir.path(), &rv_key, "ris_sha").is_none());

        // UPDATE cache keys are family-scoped too.
        let u_ris = update_cache_key("sha", "rrc00", "tsh", "RipeRis");
        let u_rv = update_cache_key("sha", "rrc00", "tsh", "RouteViews");
        assert_ne!(u_ris, u_rv);
    }
}
