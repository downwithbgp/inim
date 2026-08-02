//! Historical collector-session audit (Session 35, Part 3).
//!
//! For every RouteViews and RIS collector considered by the 2019 pilot,
//! derive the historical peer sessions from the BASELINE RIB's actual MRT
//! peer metadata (peer IP + peer ASN from the MRT header, address family,
//! origin-matching route counts, distinct prefixes, path-class membership
//! against the reviewed service planes). Current peer lists are supporting
//! context only and never override these rows.
//!
//! The audit consumes the versioned origin-scoped source extraction cache:
//! a RIB is parsed once and its origin-matching observations are reused by
//! the audit, the origin-only inventory, and every plane-specific run.

use crate::catalog::netprofile::{
    audit_sessions, CollectorLocationRegistry, PathEvidence, PeerInventoryAccumulator,
    PeerInventoryRow, ServicePlaneProfile, SessionAuditRow,
};
use crate::domain::observation::IngestRole;
use crate::ingest::IngestContext;
use crate::ingest::ObservationStream;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// One discovered baseline RIB for the audit.
#[derive(Debug, Clone)]
pub struct RibSource {
    pub family: String,
    pub collector: String,
    pub local_path: PathBuf,
    pub rib_timestamp_utc: String,
}

/// Options for a session audit run.
#[derive(Debug, Clone)]
pub struct SessionAuditOptions {
    pub profile: ServicePlaneProfile,
    pub registry: CollectorLocationRegistry,
    /// (cache directory, source family) pairs to scan for baseline RIBs.
    pub caches: Vec<(PathBuf, String)>,
    /// Filename date filter, e.g. "20190821".
    pub date: String,
    pub origin_asns: Vec<u32>,
    pub jobs: usize,
    /// Directory under which `extracted/` lives (the shared cache root).
    pub extraction_cache: PathBuf,
}

/// Discover baseline RIBs under the cache directories.
///
/// Deterministic: per collector, the earliest RIB file whose name contains
/// the date filter wins; collectors are sorted by (family, collector).
pub fn discover_ribs(opts: &SessionAuditOptions) -> Result<Vec<RibSource>, String> {
    let mut out: Vec<RibSource> = Vec::new();
    for (cache_dir, family) in &opts.caches {
        let entries = std::fs::read_dir(cache_dir)
            .map_err(|e| format!("cannot read cache dir {}: {e}", cache_dir.display()))?;
        let mut collectors: Vec<String> = Vec::new();
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                collectors.push(e.file_name().to_string_lossy().to_string());
            }
        }
        collectors.sort();
        for collector in collectors {
            // Nested layout: <cache>/<collector>/rib/...
            let rib_dir = cache_dir.join(&collector).join("rib");
            if rib_dir.is_dir() {
                collect_rib_candidates(rib_dir, &opts.date, family, &collector, &mut out);
            }
        }
        // Flat layout: <cache>/rib/... with the collector named after the
        // cache directory itself (RouteViews collectors are cached flat).
        let flat_rib = cache_dir.join("rib");
        if flat_rib.is_dir() {
            let collector = cache_dir
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            if !collector.is_empty() {
                collect_rib_candidates(flat_rib, &opts.date, family, &collector, &mut out);
            }
        }
    }
    out.sort_by_key(|a| (a.family.clone(), a.collector.clone()));
    Ok(out)
}

/// Collect the earliest date-matching RIB file in one `rib/` directory.
fn collect_rib_candidates(
    rib_dir: PathBuf,
    date: &str,
    family: &str,
    collector: &str,
    out: &mut Vec<RibSource>,
) {
    let mut candidates: Vec<(String, PathBuf)> = Vec::new();
    if let Ok(rd) = std::fs::read_dir(&rib_dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.is_file() {
                let name = e.file_name().to_string_lossy().to_string();
                if name.contains(date) && !name.ends_with(".sha256") {
                    candidates.push((name.clone(), p));
                }
            }
        }
    }
    candidates.sort();
    if let Some((name, path)) = candidates.into_iter().next() {
        out.push(RibSource {
            family: family.to_string(),
            collector: collector.to_string(),
            local_path: path,
            rib_timestamp_utc: filename_timestamp_utc(&name),
        });
    }
}

/// Parse "bview.20190821.0000.gz" / "rib.20190821.0200.bz2" into UTC.
fn filename_timestamp_utc(name: &str) -> String {
    // Find the YYYYMMDD.HHMM segment.
    let bytes = name.as_bytes();
    let mut best = String::new();
    let mut i = 0;
    while i + 13 <= bytes.len() {
        let seg = &name[i..i + 13];
        let mut ok = true;
        for (k, c) in seg.bytes().enumerate() {
            if k == 8 {
                if c != b'.' {
                    ok = false;
                    break;
                }
            } else if !c.is_ascii_digit() {
                ok = false;
                break;
            }
        }
        if ok {
            best = format!(
                "{}-{}-{}T{}:{}:00Z",
                &seg[0..4],
                &seg[4..6],
                &seg[6..8],
                &seg[9..11],
                &seg[11..13]
            );
            break;
        }
        i += 1;
    }
    best
}

fn source_sha(local_path: &Path) -> Result<String, String> {
    let sidecar = format!("{}.sha256", local_path.display());
    if let Ok(raw) = std::fs::read_to_string(&sidecar) {
        let digest = raw.split_whitespace().next().unwrap_or("").to_string();
        if digest.len() == 64 {
            return Ok(digest);
        }
    }
    // Fall back to hashing the file (only when the sidecar is missing).
    use sha2::Digest;
    let mut hasher = sha2::Sha256::new();
    let file = std::fs::File::open(local_path)
        .map_err(|e| format!("cannot open {}: {e}", local_path.display()))?;
    let mut reader = std::io::BufReader::new(file);
    std::io::copy(&mut reader, &mut hasher)
        .map_err(|e| format!("cannot hash {}: {e}", local_path.display()))?;
    Ok(hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}

/// Parse one RIB (or reuse its source extraction) and return the
/// origin-matching path evidence.
pub(crate) fn load_origin_routes(
    opts: &SessionAuditOptions,
    rib: &RibSource,
) -> Result<(String, Vec<PathEvidence>), String> {
    use crate::domain::observation::{CollectorId, ObservationKind};
    let sha = source_sha(&rib.local_path)?;
    let key = crate::catalog::source_extract::extraction_key(
        &sha,
        &rib.family,
        &rib.collector,
        &opts.origin_asns,
    );
    let cached =
        crate::catalog::source_extract::load_origin_extraction(&opts.extraction_cache, &key);
    let observations = match cached {
        Some(obs) => {
            eprintln!(
                "  [{} {}] source extraction hit ({} routes)",
                rib.family,
                rib.collector,
                obs.len()
            );
            obs
        }
        None => {
            let ctx = IngestContext {
                role: IngestRole::Rib,
                collector: CollectorId(rib.collector.clone()),
                input_path: rib.local_path.clone(),
                source_url: None,
                source_sha: Some(sha.clone()),
                origin_asn_filters: opts.origin_asns.clone(),
                archive_order: 0,
            };
            let stream = ObservationStream::from_local_file(rib.local_path.clone(), ctx)
                .map_err(|e| format!("failed to open RIB {}: {e}", rib.local_path.display()))?;
            let mut obs = Vec::new();
            for result in stream {
                match result {
                    Ok(o) => obs.push(o),
                    Err(e) => {
                        eprintln!(
                            "  [{} {}] RIB parse error (skipping): {e}",
                            rib.family, rib.collector
                        );
                    }
                }
            }
            if let Err(e) = crate::catalog::source_extract::save_origin_extraction(
                &opts.extraction_cache,
                &key,
                &obs,
            ) {
                eprintln!("  warning: failed to save source extraction: {e}");
            }
            obs
        }
    };

    let mut routes = Vec::new();
    for o in observations {
        // Only RIB entries with attributes carry path evidence.
        if o.kind != ObservationKind::RibEntry {
            continue;
        }
        let attrs = match &o.attributes {
            Some(a) => a,
            None => continue,
        };
        let af = if o.prefix.0.contains(':') {
            "ipv6"
        } else {
            "ipv4"
        };
        routes.push(PathEvidence {
            peer_ip: o.peer_ip.to_string(),
            peer_asn: o.peer_asn.0,
            address_family: af.to_string(),
            prefix: o.prefix.0.clone(),
            as_path: attrs.as_path.clone(),
            origin_asns: attrs.origin_asns.iter().map(|a| a.0).collect(),
        });
    }
    Ok((sha, routes))
}

/// Parse one RIB WITHOUT any origin filter and return ALL path evidence.
///
/// Used by the full peer inventory: every session present in the baseline
/// is reported, including sessions that carried no target-origin routes.
/// The parse is deliberately NOT written to the origin-scoped extraction
/// cache (an empty-origin key would pollute the cache namespace); the
/// inventory aggregates in memory and caches nothing.
/// Stream one RIB into a peer-inventory accumulator WITHOUT materializing
/// all routes. Memory is bounded by the session count, not the route count
/// (a full RIS bview has ~1M routes but only a few hundred sessions).
/// The parse is deliberately NOT written to the origin-scoped extraction
/// cache (an empty-origin key would pollute the cache namespace).
fn stream_full_inventory(
    acc: &mut PeerInventoryAccumulator<'_>,
    rib: &RibSource,
) -> Result<(), String> {
    use crate::domain::observation::{CollectorId, ObservationKind};
    let ctx = IngestContext {
        role: IngestRole::Rib,
        collector: CollectorId(rib.collector.clone()),
        input_path: rib.local_path.clone(),
        source_url: None,
        source_sha: None,
        origin_asn_filters: vec![],
        archive_order: 0,
    };
    let stream = ObservationStream::from_local_file(rib.local_path.clone(), ctx)
        .map_err(|e| format!("failed to open RIB {}: {e}", rib.local_path.display()))?;
    for result in stream {
        match result {
            Ok(o) => {
                if o.kind != ObservationKind::RibEntry {
                    continue;
                }
                let attrs = match &o.attributes {
                    Some(a) => a,
                    None => continue,
                };
                let af = if o.prefix.0.contains(':') {
                    "ipv6"
                } else {
                    "ipv4"
                };
                acc.observe(&PathEvidence {
                    peer_ip: o.peer_ip.to_string(),
                    peer_asn: o.peer_asn.0,
                    address_family: af.to_string(),
                    prefix: o.prefix.0.clone(),
                    as_path: attrs.as_path.clone(),
                    origin_asns: attrs.origin_asns.iter().map(|a| a.0).collect(),
                });
            }
            Err(e) => {
                eprintln!(
                    "  [{} {}] RIB parse error (skipping): {e}",
                    rib.family, rib.collector
                );
            }
        }
    }
    Ok(())
}

/// Run the FULL peer inventory over all discovered baseline RIBs.
///
/// Rows are deterministic: sorted by (family, collector, peer IP, address
/// family, peer ASN) before returning.
pub fn run_peer_inventory(opts: &SessionAuditOptions) -> Result<Vec<PeerInventoryRow>, String> {
    use crate::catalog::netprofile::PeerInventoryAccumulator;
    let ribs = discover_ribs(opts)?;
    if ribs.is_empty() {
        return Err("no baseline RIBs found under the given cache directories".to_string());
    }
    eprintln!(
        "peer inventory: {} baseline RIB(s), jobs={}",
        ribs.len(),
        opts.jobs
    );

    // Each RIB is aggregated independently (one accumulator per RIB); the
    // accumulators run on the worker threads and are merged afterwards.
    type LoadResult = Result<(String, Vec<PeerInventoryRow>), String>;
    let results: Mutex<Vec<(usize, LoadResult)>> = Mutex::new(Vec::new());
    let queue: Mutex<VecDeque<usize>> = Mutex::new((0..ribs.len()).collect());
    let jobs = opts.jobs.max(1);

    std::thread::scope(|scope| {
        for _ in 0..jobs {
            let queue = &queue;
            let results = &results;
            let ribs = &ribs;
            let opts = &opts;
            scope.spawn(move || loop {
                let idx = queue.lock().unwrap().pop_front();
                let Some(idx) = idx else { break };
                let rib = &ribs[idx];
                eprintln!(
                    "  [{} {}] loading RIB {} (full parse)",
                    rib.family,
                    rib.collector,
                    rib.local_path.display()
                );
                let sha = match source_sha(&rib.local_path) {
                    Ok(s) => s,
                    Err(e) => {
                        results.lock().unwrap().push((idx, Err(e)));
                        continue;
                    }
                };
                let mut acc = PeerInventoryAccumulator::new(
                    &opts.profile,
                    &opts.registry,
                    &rib.family,
                    &rib.collector,
                    &rib.rib_timestamp_utc,
                    &sha,
                    &rib.local_path.to_string_lossy(),
                    opts.origin_asns.clone(),
                );
                let res = match stream_full_inventory(&mut acc, rib) {
                    Ok(()) => Ok((sha, acc.finish())),
                    Err(e) => Err(e),
                };
                results.lock().unwrap().push((idx, res));
            });
        }
    });

    let mut ordered: Vec<(usize, LoadResult)> = results.into_inner().unwrap();
    ordered.sort_by_key(|(idx, _)| *idx);

    let mut rows: Vec<PeerInventoryRow> = Vec::new();
    for (rib, (_, res)) in ribs.iter().zip(ordered) {
        let (_, mut collector_rows) = res.map_err(|e| format!("{}: {e}", rib.collector))?;
        rows.append(&mut collector_rows);
    }

    rows.sort_by(|a, b| {
        (
            a.source_family.clone(),
            a.collector.clone(),
            a.peer_ip.clone(),
            a.address_family.clone(),
            a.peer_asn,
        )
            .cmp(&(
                b.source_family.clone(),
                b.collector.clone(),
                b.peer_ip.clone(),
                b.address_family.clone(),
                b.peer_asn,
            ))
    });
    Ok(rows)
}

/// Run the full session audit over all discovered baseline RIBs.
///
/// Rows are deterministic: sorted by (family, collector, peer IP, address
/// family, peer ASN) before returning.
pub fn run_session_audit(opts: &SessionAuditOptions) -> Result<Vec<SessionAuditRow>, String> {
    let ribs = discover_ribs(opts)?;
    if ribs.is_empty() {
        return Err("no baseline RIBs found under the given cache directories".to_string());
    }
    eprintln!(
        "session audit: {} baseline RIB(s), jobs={}",
        ribs.len(),
        opts.jobs
    );

    type LoadResult = Result<(String, Vec<PathEvidence>), String>;
    let results: Mutex<Vec<(usize, LoadResult)>> = Mutex::new(Vec::new());
    let queue: Mutex<VecDeque<usize>> = Mutex::new((0..ribs.len()).collect());
    let jobs = opts.jobs.max(1);

    std::thread::scope(|scope| {
        for _ in 0..jobs {
            let queue = &queue;
            let results = &results;
            let ribs = &ribs;
            let opts = &opts;
            scope.spawn(move || loop {
                let idx = queue.lock().unwrap().pop_front();
                let Some(idx) = idx else { break };
                let rib = &ribs[idx];
                eprintln!(
                    "  [{} {}] loading RIB {}",
                    rib.family,
                    rib.collector,
                    rib.local_path.display()
                );
                let res = load_origin_routes(opts, rib);
                results.lock().unwrap().push((idx, res));
            });
        }
    });

    let mut ordered: Vec<(usize, LoadResult)> = results.into_inner().unwrap();
    ordered.sort_by_key(|(idx, _)| *idx);

    let mut rows: Vec<SessionAuditRow> = Vec::new();
    for (rib, (_, res)) in ribs.iter().zip(ordered) {
        let (sha, routes) = res.map_err(|e| format!("{}: {e}", rib.collector))?;
        let mut collector_rows = audit_sessions(
            &opts.profile,
            &opts.registry,
            &rib.family,
            &rib.collector,
            &rib.rib_timestamp_utc,
            &sha,
            &routes,
        );
        rows.append(&mut collector_rows);
    }

    rows.sort_by(|a, b| {
        (
            a.source_family.clone(),
            a.collector.clone(),
            a.peer_ip.clone(),
            a.address_family.clone(),
            a.peer_asn,
        )
            .cmp(&(
                b.source_family.clone(),
                b.collector.clone(),
                b.peer_ip.clone(),
                b.address_family.clone(),
                b.peer_asn,
            ))
    });
    Ok(rows)
}
/// Backfill observed peer-session metadata from cached baseline RIBs
/// (Session 38, Part 5).
///
/// Runs a FULL peer inventory (all sessions, any origin) over the given
/// cache directories for the date and records each session's OBSERVED
/// peer ASN into `observer_session_metadata`, time-scoped by the RIB
/// timestamp. Idempotent: re-running produces identical rows. The
/// inventory deliberately has no origin filter — the metadata is a
/// protocol fact about the session, independent of the analysis target.
pub fn backfill_session_metadata(
    conn: &rusqlite::Connection,
    caches: &[(std::path::PathBuf, String)],
    date: &str,
) -> Result<usize, String> {
    let opts = SessionAuditOptions {
        profile: crate::catalog::netprofile::ServicePlaneProfile {
            service_planes: Vec::new(),
            asn_roles: Vec::new(),
            updated_utc: String::new(),
            provenance: "session-metadata backfill (no plane classification)".to_string(),
        },
        registry: crate::catalog::netprofile::CollectorLocationRegistry::default(),
        caches: caches.to_vec(),
        date: date.to_string(),
        origin_asns: Vec::new(),
        jobs: 4,
        extraction_cache: caches.first().map(|(d, _)| d.clone()).unwrap_or_default(),
    };
    let rows = run_peer_inventory(&opts)?;
    let mut inserted = 0usize;
    for row in &rows {
        let metadata = crate::catalog::domain::ObserverSessionMetadata {
            id: 0,
            source_family: row.source_family.clone(),
            collector: row.collector.clone(),
            peer_ip: row.peer_ip.clone(),
            address_family: row.address_family.clone(),
            peer_asn: row.peer_asn,
            valid_from: row.rib_timestamp_utc.clone(),
            valid_to: None,
            source_archive: row.rib_source.clone(),
            source_sha256: row.rib_source_sha.clone(),
        };
        let before = crate::catalog::store::list_session_metadata(conn)?.len();
        crate::catalog::store::insert_session_metadata(conn, &metadata)?;
        let after = crate::catalog::store::list_session_metadata(conn)?.len();
        if after > before {
            inserted += 1;
        }
    }
    Ok(inserted)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filename_timestamp_parses_ris_and_routeviews_names() {
        assert_eq!(
            filename_timestamp_utc("bview.20190821.0000.gz"),
            "2019-08-21T00:00:00Z"
        );
        assert_eq!(
            filename_timestamp_utc("rib.20190821.0200.bz2"),
            "2019-08-21T02:00:00Z"
        );
        assert_eq!(
            filename_timestamp_utc("updates.20190821.1600.gz"),
            "2019-08-21T16:00:00Z"
        );
    }

    #[test]
    fn discover_ribs_picks_earliest_matching_file() {
        let dir = std::env::temp_dir().join(format!("inim-audit-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let rib_dir = dir.join("rrc00").join("rib");
        std::fs::create_dir_all(&rib_dir).unwrap();
        std::fs::write(rib_dir.join("bview.20190821.0000.gz"), "x").unwrap();
        std::fs::write(rib_dir.join("bview.20190821.0800.gz"), "y").unwrap();
        std::fs::write(rib_dir.join("bview.20190821.0000.gz.sha256"), "z").unwrap();
        let opts = SessionAuditOptions {
            profile: ServicePlaneProfile {
                service_planes: vec![],
                asn_roles: vec![],
                updated_utc: String::new(),
                provenance: String::new(),
            },
            registry: CollectorLocationRegistry {
                as_of: String::new(),
                collectors: vec![],
            },
            caches: vec![(dir.clone(), "ris".to_string())],
            date: "20190821".to_string(),
            origin_asns: vec![2603],
            jobs: 2,
            extraction_cache: dir.clone(),
        };
        let ribs = discover_ribs(&opts).unwrap();
        assert_eq!(ribs.len(), 1);
        assert_eq!(ribs[0].collector, "rrc00");
        assert!(ribs[0]
            .local_path
            .to_string_lossy()
            .ends_with("bview.20190821.0000.gz"));
        assert_eq!(ribs[0].rib_timestamp_utc, "2019-08-21T00:00:00Z");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
