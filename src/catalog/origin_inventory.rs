//! Origin-only inventory.
//!
//! Classifies ALL origin-matching baseline routes at each selected
//! observer against the manifest's named path classifiers (a route may
//! match one, both, or neither named class), with NO cohort verdict.
//! Cohort selection and path classification are deliberately separate:
//! the inventory never selects or freezes streams, and the manifest's
//! cohort predicate never filters what the inventory sees.

use crate::catalog::netprofile::ServicePlaneProfile;
use crate::catalog::session_audit::SessionAuditOptions;
use crate::catalog::session_audit::{discover_ribs, load_origin_routes, RibSource};
use crate::plan::NamedPathClassifier;
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::Path;

/// Classification of one origin-matching route against the classifiers.
#[derive(Debug, Clone, Serialize)]
pub struct RouteClass {
    pub prefix: String,
    pub peer_ip: String,
    pub peer_asn: u32,
    pub address_family: String,
    pub as_path: Vec<u32>,
    /// Classifier ids this route matches (one, both, or neither).
    pub matched_classifier_ids: Vec<String>,
}

/// Per-collector inventory.
#[derive(Debug, Clone, Serialize)]
pub struct CollectorInventory {
    pub source_family: String,
    pub collector: String,
    pub location: String,
    pub rib_timestamp_utc: String,
    pub origin_matching_routes: usize,
    pub distinct_prefixes: usize,
    /// Classifier id -> route count (a route may appear in several).
    pub per_classifier_routes: BTreeMap<String, usize>,
    /// Routes matching no classifier.
    pub neither_classifier: usize,
    /// Routes matching every classifier (only meaningful with >=2).
    pub all_classifiers: usize,
}

/// Build the origin-only inventory for the manifest's collectors.
///
/// Deterministic: rows sorted by (family, collector); classifier counts
/// keyed by classifier id in manifest order (BTreeMap keeps ids sorted).
#[allow(clippy::too_many_arguments)] // CLI passthrough; each maps to one flag
pub fn build_inventory(
    profile: &ServicePlaneProfile,
    registry: &crate::catalog::netprofile::CollectorLocationRegistry,
    cache_dir: &Path,
    family: &str,
    collectors: &[String],
    origin_asns: &[u32],
    classifiers: &[NamedPathClassifier],
    extraction_cache: &Path,
    jobs: usize,
) -> Result<Vec<CollectorInventory>, String> {
    // Discover the baseline RIBs the same way the session audit does.
    let opts = SessionAuditOptions {
        profile: profile.clone(),
        registry: registry.clone(),
        caches: vec![(cache_dir.to_path_buf(), family.to_string())],
        date: "20190821".to_string(),
        origin_asns: origin_asns.to_vec(),
        jobs,
        extraction_cache: extraction_cache.to_path_buf(),
    };
    // Restrict discovery to the requested collectors.
    let ribs: Vec<RibSource> = discover_ribs(&opts)?
        .into_iter()
        .filter(|r| collectors.contains(&r.collector))
        .collect();

    let mut out: Vec<CollectorInventory> = Vec::new();
    for rib in &ribs {
        let (_, routes) = load_origin_routes(&opts, rib)?;
        let mut inv = CollectorInventory {
            source_family: rib.family.clone(),
            collector: rib.collector.clone(),
            location: registry
                .location(&rib.family, &rib.collector)
                .map(|c| c.location.clone())
                .unwrap_or_else(|| "unknown".to_string()),
            rib_timestamp_utc: rib.rib_timestamp_utc.clone(),
            origin_matching_routes: routes.len(),
            distinct_prefixes: {
                let mut prefixes: Vec<&str> = routes.iter().map(|r| r.prefix.as_str()).collect();
                prefixes.sort_unstable();
                prefixes.dedup();
                prefixes.len()
            },
            per_classifier_routes: BTreeMap::new(),
            neither_classifier: 0,
            all_classifiers: 0,
        };
        let (per_classifier, neither, all) = inventory_counts(classifiers, &routes);
        inv.per_classifier_routes = per_classifier;
        inv.neither_classifier = neither;
        inv.all_classifiers = all;
        out.push(inv);
    }
    Ok(out)
}

/// Pure classification counts over origin-matching routes: every route is
/// counted (routes matching no classifier land in `neither`), a route may
/// match several classifiers, and `all` counts routes matching every
/// classifier. The inventory NEVER drops a route because it fails a
/// cohort predicate — cohort selection and classification stay separate.
pub fn inventory_counts(
    classifiers: &[NamedPathClassifier],
    routes: &[crate::catalog::netprofile::PathEvidence],
) -> (BTreeMap<String, usize>, usize, usize) {
    let mut per_classifier: BTreeMap<String, usize> = BTreeMap::new();
    let mut neither = 0usize;
    let mut all = 0usize;
    for route in routes {
        let matched: Vec<&str> = classifiers
            .iter()
            .filter(|c| c.predicate.evaluate(&route.as_path))
            .map(|c| c.id.as_str())
            .collect();
        if matched.is_empty() {
            neither += 1;
        }
        if !classifiers.is_empty() && matched.len() == classifiers.len() {
            all += 1;
        }
        for id in matched {
            *per_classifier.entry(id.to_string()).or_insert(0) += 1;
        }
    }
    (per_classifier, neither, all)
}

/// Classify a single route against the classifiers (unit-testable).
pub fn classify_route_path(classifiers: &[NamedPathClassifier], path: &[u32]) -> Vec<String> {
    classifiers
        .iter()
        .filter(|c| c.predicate.evaluate(path))
        .map(|c| c.id.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::route::TransitPredicate;

    fn classifier(id: &str, asn: u32) -> NamedPathClassifier {
        NamedPathClassifier {
            id: id.to_string(),
            display_label: id.to_string(),
            predicate: TransitPredicate::ContainsAny(vec![asn]),
        }
    }

    #[test]
    fn one_path_can_match_multiple_named_classifiers() {
        let cs = vec![classifier("re", 64500), classifier("pex", 64501)];
        let ids = classify_route_path(&cs, &[64600, 64500, 64501]);
        assert_eq!(ids, vec!["re".to_string(), "pex".to_string()]);
        let neither = classify_route_path(&cs, &[64600, 64601]);
        assert!(neither.is_empty());
        let one = classify_route_path(&cs, &[64600, 64501]);
        assert_eq!(one, vec!["pex".to_string()]);
    }

    #[test]
    fn cohort_selection_and_path_classification_are_not_silently_conflated() {
        // A route that matches NO classifier still appears in the
        // classification output — classification never filters; the
        // inventory counts it under neither_classifier.
        let cs = vec![classifier("re", 64500)];
        assert!(classify_route_path(&cs, &[64600]).is_empty());
        // And a route matching a classifier is classified regardless of
        // any cohort predicate — the function takes only the classifiers.
        assert_eq!(
            classify_route_path(&cs, &[64600, 64500]),
            vec!["re".to_string()]
        );
    }
}

#[cfg(test)]
mod counting_tests {
    use super::*;
    use crate::catalog::netprofile::PathEvidence;
    use crate::domain::route::TransitPredicate;

    fn classifier(id: &str, asn: u32) -> NamedPathClassifier {
        NamedPathClassifier {
            id: id.to_string(),
            display_label: id.to_string(),
            predicate: TransitPredicate::ContainsAny(vec![asn]),
        }
    }

    fn route(path: Vec<u32>) -> PathEvidence {
        PathEvidence {
            peer_ip: "192.0.2.1".to_string(),
            peer_asn: 64600,
            address_family: "ipv4".to_string(),
            prefix: "198.51.100.0/24".to_string(),
            as_path: path.clone(),
            origin_asns: path.last().copied().map(|a| vec![a]).unwrap_or_default(),
        }
    }

    #[test]
    fn origin_only_inventory_does_not_require_transit_match() {
        let cs = vec![classifier("re", 64500), classifier("pex", 64501)];
        // One route matches re, one matches nothing: the inventory counts
        // BOTH — no cohort filter drops the non-matching route.
        let routes = vec![route(vec![64600, 64500]), route(vec![64600, 64601])];
        let (per, neither, all) = inventory_counts(&cs, &routes);
        assert_eq!(per.get("re"), Some(&1));
        assert_eq!(neither, 1);
        assert_eq!(all, 0);
    }

    #[test]
    fn path_moving_between_planes_remains_admitted() {
        let cs = vec![classifier("re", 64500), classifier("pex", 64501)];
        // The same prefix's route INSTANCE moves from the R&E plane to the
        // PEX plane across time: both instances are admitted and counted
        // (each under its own class) — nothing is rejected for changing
        // planes.
        let routes = vec![
            route(vec![64600, 64500]),
            route(vec![64600, 64501]),
            route(vec![64600, 64500, 64501]),
        ];
        let (per, neither, all) = inventory_counts(&cs, &routes);
        assert_eq!(per.get("re"), Some(&2));
        assert_eq!(per.get("pex"), Some(&2));
        assert_eq!(all, 1, "both-planes route counted once in 'all'");
        assert_eq!(neither, 0);
    }

    #[test]
    fn plane_specific_runs_use_independent_cohorts() {
        // The two plane manifests select independent cohorts: a route in
        // the R&E cohort is admitted only when its path contains the R&E
        // plane ASN; the same route set under the PEX selector produces a
        // DIFFERENT cohort (and possibly an empty one).
        let cs = vec![classifier("re", 64500), classifier("pex", 64501)];
        let routes = vec![route(vec![64600, 64500])];
        let (per, _, _) = inventory_counts(&cs, &routes);
        assert_eq!(per.get("re"), Some(&1));
        assert!(
            !per.contains_key("pex"),
            "Peer Exchange classifier sees nothing"
        );
        // And the cohort selection itself (scan_rib_and_freeze) is
        // predicate-scoped — covered by source_extract reuse tests; here
        // we assert the two selectors are distinct predicates.
        assert_ne!(cs[0].predicate, cs[1].predicate);
    }
}
