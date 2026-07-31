//! Monocle adapter — wraps BGPKIT Monocle SearchLens for broker discovery.
//!
//! Only this module imports `monocle::` types. inim's existing parser
//! pipeline (ObservationStream) handles MRT parsing — Monocle provides
//! broker integration with query caching where available.

use monocle::lens::parse::ParseFilters;
use monocle::lens::search::{SearchDumpType, SearchFilters};

/// Build Monocle SearchFilters for an UPDATE search.
pub fn build_update_search(
    project: &str,
    collectors: &[String],
    start_ts: &str,
    end_ts: &str,
) -> SearchFilters {
    SearchFilters {
        parse_filters: ParseFilters {
            start_ts: Some(start_ts.to_string()),
            end_ts: Some(end_ts.to_string()),
            ..Default::default()
        },
        collector: Some(collectors.join(",")),
        project: Some(project.to_string()),
        dump_type: SearchDumpType::Updates,
    }
}

/// Discover MRT archives via Monocle broker.
/// Returns (url, collector_id) pairs sorted deterministically.
pub fn discover_archives(filters: &SearchFilters) -> Result<Vec<(String, String)>, String> {
    let broker = filters
        .build_broker()
        .map_err(|e| format!("monocle broker build: {e}"))?;
    let items = broker
        .query()
        .map_err(|e| format!("monocle broker query: {e}"))?;
    let mut results: Vec<(String, String)> = items
        .into_iter()
        .map(|item| (item.url, item.collector_id))
        .collect();
    results.sort_by(|a, b| a.1.cmp(&b.1).then_with(|| a.0.cmp(&b.0)));
    Ok(results)
}
