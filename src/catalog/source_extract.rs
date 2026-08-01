//! Versioned, origin-scoped source-extraction cache.
//!
//! After a RIB parse, the origin-matching observations (pre-predicate) are
//! persisted keyed by (source sha, source family, collector, SORTED origin
//! set, parser version, extraction schema version) — deliberately NOT by
//! transit predicate. A later request for the same source+origin loads the
//! extraction instead of re-decompressing and re-parsing the archive;
//! predicate filtering and cohort admission then run identically in memory,
//! so standalone and reused outputs are identical while the expensive parse
//! happens once per source.
//!
//! The extraction is origin-scoped (a few hundred routes per RIB), never a
//! full-table BGP warehouse, and its rows are full observations (peer IP,
//! peer ASN, prefix, complete AS path, path id) so consumers compute
//! path-class membership AFTER load.
//!
//! Evidence identity is derived from observation content downstream, so
//! cache hits never change evidence ids.

use crate::domain::observation::RouteObservation;
use sha2::Digest;

use std::path::{Path, PathBuf};

/// Extraction schema version; bumping it invalidates all old extractions.
pub const EXTRACTION_SCHEMA_VERSION: u32 = 1;

/// Deterministic identity for one source extraction.
///
/// The origin set is canonicalized (sorted, deduped) so that equivalent
/// manifests share one extraction. The transit predicate is NOT part of
/// the identity — that is the point: independent selectors over the same
/// source reuse one parse.
pub fn extraction_key(
    source_sha: &str,
    source_family: &str,
    collector: &str,
    origin_asns: &[u32],
) -> String {
    let mut sorted: Vec<u32> = origin_asns.to_vec();
    sorted.sort_unstable();
    sorted.dedup();

    let mut hasher = <sha2::Sha256 as sha2::Digest>::new();
    hasher.update(source_sha.as_bytes());
    hasher.update(b"|");
    hasher.update(source_family.as_bytes());
    hasher.update(b"|");
    hasher.update(collector.as_bytes());
    hasher.update(b"|");
    for asn in &sorted {
        hasher.update(asn.to_le_bytes());
    }
    hasher.update(b"|");
    hasher.update(EXTRACTION_SCHEMA_VERSION.to_le_bytes());
    hasher.update(b"|");
    hasher.update(crate::derived_cache::PARSER_VERSION.as_bytes());
    bytes_to_hex(&hasher.finalize()[..16])
}

/// Path of the extraction file for a key.
pub fn extraction_path(cache_dir: &Path, key: &str) -> PathBuf {
    cache_dir.join("extracted").join(format!("{key}.json.gz"))
}

/// Load a cached source extraction, if present and readable.
pub fn load_origin_extraction(cache_dir: &Path, key: &str) -> Option<Vec<RouteObservation>> {
    let path = extraction_path(cache_dir, key);
    let file = std::fs::File::open(&path).ok()?;
    let reader = std::io::BufReader::new(file);
    let gz = flate2::read::GzDecoder::new(reader);
    serde_json::from_reader(gz).ok()
}

/// Persist a source extraction (origin-matching, pre-predicate rows).
pub fn save_origin_extraction(
    cache_dir: &Path,
    key: &str,
    observations: &[RouteObservation],
) -> Result<(), String> {
    let path = extraction_path(cache_dir, key);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("cannot create extraction dir: {e}"))?;
    }
    // Atomic-ish write: temp file then rename, so a crashed write never
    // leaves a corrupt extraction that silently disables reuse.
    let tmp = path.with_extension("json.gz.tmp");
    {
        let file = std::fs::File::create(&tmp)
            .map_err(|e| format!("cannot create extraction file: {e}"))?;
        let mut writer = flate2::write::GzEncoder::new(
            std::io::BufWriter::new(file),
            flate2::Compression::default(),
        );
        serde_json::to_writer(&mut writer, observations)
            .map_err(|e| format!("cannot serialize extraction: {e}"))?;
        writer
            .finish()
            .map_err(|e| format!("cannot finish extraction write: {e}"))?;
    }
    std::fs::rename(&tmp, &path).map_err(|e| format!("cannot finalize extraction: {e}"))?;
    Ok(())
}

fn bytes_to_hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn obs(peer_asn: u32, prefix: &str) -> RouteObservation {
        use crate::domain::observation::{
            Asn, CollectorId, IngestRole, ObservationAttributes, ObservationKind,
            ObservationProvenance, ObservationSource,
        };
        RouteObservation {
            id: crate::domain::observation::ObservationId(0),
            source: ObservationSource::LocalFile("rib.gz".to_string()),
            timestamp: chrono::Utc::now(),
            collector: CollectorId("c1".to_string()),
            peer_ip: "192.0.2.1".parse().unwrap(),
            peer_asn: Asn(peer_asn),
            prefix: crate::domain::route::Prefix(prefix.to_string()),
            kind: ObservationKind::Announcement,
            attributes: Some(ObservationAttributes {
                origin_asns: vec![Asn(64500)],
                as_path: vec![peer_asn, 64500],
                next_hop: None,
                origin: None,
                local_pref: None,
                med: None,
                atomic_aggregate: false,
                communities: Default::default(),
            }),
            path_id: None,
            provenance: ObservationProvenance {
                input: "rib.gz".to_string(),
                source_url: Some("https://example.test/rib.gz".to_string()),
                archive_sha256: Some("sha".to_string()),
                role: IngestRole::Rib,
                parser_representation: "bgpkit-bgp-elem".to_string(),
                mrt_timestamp: 0.0,
                element_seq: 0,
                archive_order: 0,
            },
        }
    }

    #[test]
    fn extraction_key_canonicalizes_origin_order() {
        let a = extraction_key("sha1", "ris", "rrc00", &[64500, 64501]);
        let b = extraction_key("sha1", "ris", "rrc00", &[64501, 64500]);
        assert_eq!(a, b, "origin order must not change the extraction identity");
        let c = extraction_key("sha1", "ris", "rrc00", &[64500]);
        assert_ne!(a, c, "origin set is part of the identity");
        let d = extraction_key("sha1", "routeviews", "rrc00", &[64500, 64501]);
        assert_ne!(a, d, "family is part of the identity");
        let e = extraction_key("sha1", "ris", "rrc01", &[64500, 64501]);
        assert_ne!(a, e, "collector is part of the identity");
        let f = extraction_key("sha2", "ris", "rrc00", &[64500, 64501]);
        assert_ne!(a, f, "source sha is part of the identity");
    }

    #[test]
    fn extraction_roundtrip_preserves_observations() {
        let dir = std::env::temp_dir().join(format!("inim-extract-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let key = extraction_key("sha1", "ris", "rrc00", &[64500]);
        let rows = vec![obs(64600, "198.51.100.0/24"), obs(64601, "2001:db8::/48")];
        save_origin_extraction(&dir, &key, &rows).unwrap();
        let loaded = load_origin_extraction(&dir, &key).expect("extraction must load");
        assert_eq!(loaded, rows, "roundtrip must preserve every observation");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn extraction_key_does_not_include_predicate() {
        // The identity must NOT depend on the transit predicate — two
        // plane-specific runs over the same source share one extraction.
        let a = extraction_key("sha1", "ris", "rrc00", &[64500]);
        assert_eq!(a, extraction_key("sha1", "ris", "rrc00", &[64500]));
    }
}

#[cfg(test)]
mod reuse_tests {
    use super::*;
    use crate::domain::observation::{
        Asn, CollectorId, IngestRole, ObservationAttributes, ObservationKind,
        ObservationProvenance, ObservationSource,
    };
    use crate::domain::route::{Prefix, TransitPredicate};
    use crate::target::{scan_rib_and_freeze, TargetSet};

    fn rib_obs(peer_asn: u32, prefix: &str, path: Vec<u32>) -> RouteObservation {
        RouteObservation {
            id: crate::domain::observation::ObservationId(0),
            source: ObservationSource::LocalFile("rib.gz".to_string()),
            timestamp: chrono::Utc::now(),
            collector: CollectorId("c1".to_string()),
            peer_ip: "192.0.2.1".parse().unwrap(),
            peer_asn: Asn(peer_asn),
            prefix: Prefix(prefix.to_string()),
            kind: ObservationKind::RibEntry,
            attributes: Some(ObservationAttributes {
                origin_asns: vec![Asn(64500)],
                as_path: path,
                next_hop: None,
                origin: None,
                local_pref: None,
                med: None,
                atomic_aggregate: false,
                communities: Default::default(),
            }),
            path_id: None,
            provenance: ObservationProvenance {
                input: "rib.gz".to_string(),
                source_url: Some("https://example.test/rib.gz".to_string()),
                archive_sha256: Some("sha".to_string()),
                role: IngestRole::Rib,
                parser_representation: "bgpkit-bgp-elem".to_string(),
                mrt_timestamp: 0.0,
                element_seq: 0,
                archive_order: 0,
            },
        }
    }

    fn temp_cache(tag: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("inim-extract-reuse-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn plane(target: &TargetSet, collector: &str) -> Vec<(String, String)> {
        let mut out: Vec<(String, String)> = target
            .streams
            .get(collector)
            .map(|v| {
                v.iter()
                    .map(|s| (s.peer_ip.to_string(), s.prefix.0.clone()))
                    .collect()
            })
            .unwrap_or_default();
        out.sort();
        out
    }

    #[test]
    fn same_rib_is_not_reparsed_for_two_plane_batch_when_reuse_is_safe() {
        let cache = temp_cache("same_rib_is_not_reparsed_for_two_plane_batch_when_reuse_is_safe");
        let origins = [64500u32];
        // Plane A run parses and persists the origin extraction.
        let key = extraction_key("sha-rib", "ris", "rrc00", &origins);
        let obs = vec![
            rib_obs(64600, "198.51.100.0/24", vec![64600, 64501]),
            rib_obs(64600, "198.51.100.0/24", vec![64600, 64500]),
        ];
        save_origin_extraction(&cache, &key, &obs).unwrap();
        // Plane B run over the same source+origin: the extraction loads
        // WITHOUT parsing (no MRT file exists locally — loading is the
        // only path that can succeed).
        let loaded = load_origin_extraction(&cache, &key).expect("plane B reuses extraction");
        assert_eq!(loaded.len(), 2);
        // The extraction exists exactly once (parse-once invariant).
        assert!(crate::catalog::source_extract::extraction_path(&cache, &key).exists());
        let _ = std::fs::remove_dir_all(&cache);
    }

    #[test]
    fn reused_source_parse_does_not_merge_cohorts() {
        let cache = temp_cache("reused_source_parse_does_not_merge_cohorts");
        let origins = [64500u32];
        let re = TransitPredicate::ContainsAny(vec![64501]);
        let pex = TransitPredicate::ContainsAny(vec![64502]);
        let obs = vec![
            rib_obs(64600, "198.51.100.1/24", vec![64600, 64501]), // matches re only
            rib_obs(64600, "198.51.100.2/24", vec![64600, 64502]), // matches pex only
            rib_obs(64600, "198.51.100.3/24", vec![64600, 64501, 64502]), // both
        ];
        let key = extraction_key("sha-rib", "ris", "rrc00", &origins);
        save_origin_extraction(&cache, &key, &obs).unwrap();
        let loaded = load_origin_extraction(&cache, &key).unwrap();

        let re_set = scan_rib_and_freeze(&loaded, &origins, &re);
        let pex_set = scan_rib_and_freeze(&loaded, &origins, &pex);
        let re_streams = plane(&re_set, "c1");
        let pex_streams = plane(&pex_set, "c1");
        // Each cohort is its own independent set — nothing merged.
        assert_eq!(re_streams.len(), 2, "re cohort: both + re-only");
        assert_eq!(pex_streams.len(), 2, "pex cohort: both + pex-only");
        assert_eq!(re_set.frozen_prefixes().len(), 2);
        assert_eq!(pex_set.frozen_prefixes().len(), 2);
        let _ = std::fs::remove_dir_all(&cache);
    }

    #[test]
    fn standalone_and_reused_outputs_are_identical() {
        let cache = temp_cache("standalone_and_reused_outputs_are_identical");
        let origins = [64500u32];
        let pred = TransitPredicate::ContainsAny(vec![64501]);
        let obs = vec![
            rib_obs(64600, "198.51.100.0/24", vec![64600, 64501]),
            rib_obs(64601, "198.51.100.0/24", vec![64601, 64502]),
            rib_obs(64600, "2001:db8::/48", vec![64600, 64501]),
        ];
        let key = extraction_key("sha-rib", "ris", "rrc00", &origins);
        save_origin_extraction(&cache, &key, &obs).unwrap();
        let reused = load_origin_extraction(&cache, &key).unwrap();

        // Standalone path: the parser-produced vec. Reused path: the
        // round-tripped vec. Downstream admission must be identical.
        let a = scan_rib_and_freeze(&obs, &origins, &pred);
        let b = scan_rib_and_freeze(&reused, &origins, &pred);
        assert_eq!(plane(&a, "c1"), plane(&b, "c1"));
        assert_eq!(a.frozen_prefixes(), b.frozen_prefixes());
        assert_eq!(
            crate::derived_cache::targetset_hash(&a),
            crate::derived_cache::targetset_hash(&b)
        );
        let _ = std::fs::remove_dir_all(&cache);
    }

    #[test]
    fn performance_metadata_does_not_change_evidence_ids() {
        // Evidence ids are assigned deterministically from observation
        // content AFTER sorting; parse-vs-reuse changes only performance
        // metadata (timings/counters), never the content.
        let cache = temp_cache("performance_metadata_does_not_change_evidence_ids");
        let origins = [64500u32];
        let obs = vec![
            rib_obs(64600, "198.51.100.0/24", vec![64600, 64501]),
            rib_obs(64601, "198.51.100.0/24", vec![64601, 64502]),
        ];
        let key = extraction_key("sha-rib", "ris", "rrc00", &origins);
        save_origin_extraction(&cache, &key, &obs).unwrap();
        let reused = load_origin_extraction(&cache, &key).unwrap();

        let mut a = obs;
        let mut b = reused;
        crate::derived_cache::sort_deterministic(&mut a);
        crate::derived_cache::sort_deterministic(&mut b);
        crate::derived_cache::assign_deterministic_ids(&mut a);
        crate::derived_cache::assign_deterministic_ids(&mut b);
        let ids_a: Vec<u64> = a.iter().map(|o| o.id.0).collect();
        let ids_b: Vec<u64> = b.iter().map(|o| o.id.0).collect();
        assert_eq!(ids_a, ids_b, "evidence ids must not depend on cache path");
        // Archive metrics differ (volatile), evidence ids do not.
        assert_eq!(a, b);
        let _ = std::fs::remove_dir_all(&cache);
    }
}
