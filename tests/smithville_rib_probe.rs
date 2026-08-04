//! Event-date RIB probe for the Smithville peer-relationship review
//! (Session 50). Downloads the baseline RIB/bview via the cache layer,
//! parses it, and prints AS11550-origin paths + peer ASNs.
//!
//! This probe is a RESEARCH tool for the reviewed manifest build; it is
//! not part of the offline CI suite (it requires network access), so it
//! is marked #[ignore].

use inim::discover::{cache_archive, ArchiveItem};

#[test]
#[ignore = "live research probe; requires network access"]
fn probe_event_date_as11550_paths() {
    let collectors: Vec<(String, String, String, String, String)> = vec![
        (
            "route-views2".into(),
            "routeviews".into(),
            "rib".into(),
            "2026-07-28T04:00:00Z".into(),
            "http://archive.routeviews.org/bgpdata/2026.07/RIBS/rib.20260728.0400.bz2".into(),
        ),
        (
            "rrc00".into(),
            "ris".into(),
            "rib".into(),
            "2026-07-28T00:00:00Z".into(),
            "https://data.ris.ripe.net/rrc00/2026.07/bview.20260728.0000.gz".into(),
        ),
        (
            "rrc06".into(),
            "ris".into(),
            "rib".into(),
            "2026-07-28T00:00:00Z".into(),
            "https://data.ris.ripe.net/rrc06/2026.07/bview.20260728.0000.gz".into(),
        ),
    ];
    let cache_dir = std::path::Path::new("data/s50-cache");
    for (collector, project, dtype, ts, url) in collectors {
        let ts_utc: chrono::DateTime<chrono::Utc> = ts.parse().unwrap();
        let item = ArchiveItem {
            project,
            collector_id: collector.clone(),
            data_type: dtype,
            ts_start: ts_utc,
            ts_end: ts_utc,
            url: url.clone(),
            size: 0,
        };
        let cached = match cache_archive(&item, cache_dir, false) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("{collector}: cache failed: {e}");
                continue;
            }
        };
        eprintln!(
            "{collector}: cached {} ({} bytes)",
            cached.local_path, cached.size
        );
        let bytes = match std::fs::read(&cached.local_path) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("{collector}: read failed: {e}");
                continue;
            }
        };
        // Keep the ORIGINAL extension: BgpkitParser sniffs the file
        // extension to decide decompression (.gz/.bz2).
        let ext = std::path::Path::new(&url)
            .extension()
            .map(|e| e.to_string_lossy().to_string())
            .unwrap_or_else(|| "mrt".to_string());
        let tmp = std::env::temp_dir().join(format!("s50-probe-{}.{ext}", std::process::id()));
        std::fs::write(&tmp, &bytes).unwrap();
        let parser: bgpkit_parser::BgpkitParser<_> =
            bgpkit_parser::BgpkitParser::new(tmp.to_str().unwrap()).unwrap();
        let mut paths: std::collections::HashMap<Vec<u32>, usize> =
            std::collections::HashMap::new();
        let mut peers: std::collections::HashMap<u32, usize> = std::collections::HashMap::new();
        let mut prefixes: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut total = 0usize;
        let mut announces = 0usize;
        let mut any_11550_in_path = 0usize;
        let mut any_19782_in_path = 0usize;
        let mut peer_19782 = 0usize;
        for e in parser {
            if e.elem_type == bgpkit_parser::models::ElemType::ANNOUNCE {
                announces += 1;
                let as_path: Vec<u32> = e
                    .as_path
                    .as_ref()
                    .map(|ap| {
                        ap.iter_segments()
                            .flat_map(|seg| match seg {
                                bgpkit_parser::models::AsPathSegment::AsSequence(v)
                                | bgpkit_parser::models::AsPathSegment::AsSet(v)
                                | bgpkit_parser::models::AsPathSegment::ConfedSequence(v)
                                | bgpkit_parser::models::AsPathSegment::ConfedSet(v) => {
                                    v.iter().map(|a| u32::from(*a)).collect::<Vec<u32>>()
                                }
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                let origin_match = e
                    .origin_asns
                    .as_ref()
                    .map(|v| v.iter().any(|a| u32::from(*a) == 11550))
                    .unwrap_or(false);
                let tail_match = as_path.last().map(|o| *o == 11550).unwrap_or(false);
                if as_path.contains(&11550) {
                    any_11550_in_path += 1;
                }
                if as_path.contains(&19782) {
                    any_19782_in_path += 1;
                }
                if u32::from(e.peer_asn) == 19782 {
                    peer_19782 += 1;
                }
                if origin_match || tail_match {
                    if as_path.contains(&19782) {
                        eprintln!("  !! AS11550-origin path contains IGP AS19782: {as_path:?}");
                    }
                    total += 1;
                    prefixes.insert(e.prefix.to_string());
                    *paths.entry(as_path.clone()).or_insert(0) += 1;
                    *peers.entry(u32::from(e.peer_asn)).or_insert(0) += 1;
                }
            }
        }
        eprintln!("  (paths containing 11550 anywhere: {any_11550_in_path}; paths containing IGP AS19782 anywhere: {any_19782_in_path}; direct peer sessions with AS19782: {peer_19782})");
        println!("== {collector} ==");
        println!("  total announces parsed: {announces}");
        println!(
            "  AS11550-origin routes: {total}; distinct prefixes: {}",
            prefixes.len()
        );
        println!("  peer ASNs: {peers:?}");
        let mut sorted: Vec<_> = paths.into_iter().collect();
        sorted.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
        for (path, n) in sorted.iter().take(8) {
            println!("    path {path:?} x{n}");
        }
    }
}
