//! Reviewed session context for the case-study comparison view.
//!
//! Loads the reviewed data files of a case study (network profile,
//! historical session audit, cross-observer matrix) and joins them onto
//! comparison rows: historically correct collector location, peer ASN
//! from the MRT header, direct/indirect relationship to the named
//! planes, the named plane of the run's cohort, and the reviewed cohort
//! predicate. All identities and labels come from the DATA files — this
//! module contains no operator-specific branch.

use crate::catalog::netprofile::{ServicePlaneProfile, SessionAuditRow};
use std::collections::HashMap;
use std::path::Path;

/// Per-run facts joined onto comparison rows.
pub struct SessionContext {
    /// (normalized family, collector, peer ip) -> joined facts.
    lookup_map: HashMap<SessionKey, SessionFacts>,
    /// Explainer lines for the case-study first screen.
    explainer: Vec<String>,
}

type SessionKey = (String, String, String);

/// Facts joined onto one comparison row.
type SessionFacts = (String, String, String, String, String);

fn normalize_family(family: &str) -> String {
    let f = family.to_lowercase().replace(' ', "");
    if f.contains("ris") {
        "riperis".to_string()
    } else {
        f
    }
}

impl SessionContext {
    /// Load the reviewed session context for a case-study slug. Returns
    /// None when the data files are absent (other case studies).
    pub fn load_for_slug(slug: &str) -> Option<SessionContext> {
        let pilot = Path::new("case-studies").join(slug).join("pilot");
        let profile_path = pilot.join("network-profile.json");
        let audit_path = pilot.join("session-audit-2019.json");
        let matrix_path = pilot.join("cross-observer-matrix.json");
        if !profile_path.is_file() || !audit_path.is_file() || !matrix_path.is_file() {
            return None;
        }
        let profile = ServicePlaneProfile::load(&profile_path).ok()?;
        let audit_raw = std::fs::read_to_string(&audit_path).ok()?;
        let audit: Vec<SessionAuditRow> = serde_json::from_str(&audit_raw).ok()?;
        let matrix: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&matrix_path).ok()?).ok()?;

        // Per-collector cohort predicate + plane label from the matrix.
        let mut predicate_by_collector: HashMap<String, String> = HashMap::new();
        let mut plane_by_collector: HashMap<String, String> = HashMap::new();
        if let Some(runs) = matrix["re_plane_runs"].as_array() {
            for run in runs {
                let collector = run["collector"].as_str().unwrap_or("");
                let predicate = run["cohort_predicate"].as_str().unwrap_or("");
                let plane = matrix["re_plane_label"].as_str().unwrap_or("");
                predicate_by_collector.insert(collector.to_string(), predicate.to_string());
                plane_by_collector.insert(collector.to_string(), plane.to_string());
            }
        }

        let mut lookup_map = HashMap::new();
        for row in &audit {
            let relationships = row.relationship_displays(&profile);
            let key = (
                normalize_family(&row.source_family),
                row.collector.clone(),
                row.peer_ip.clone(),
            );
            let plane = plane_by_collector
                .get(&row.collector)
                .cloned()
                .unwrap_or_default();
            let predicate = predicate_by_collector
                .get(&row.collector)
                .cloned()
                .unwrap_or_default();
            lookup_map.insert(
                key,
                (
                    row.location.clone(),
                    format!("{}", row.peer_asn),
                    relationships.join("; "),
                    plane,
                    predicate,
                ),
            );
        }

        let mut plane_labels: Vec<String> = profile
            .service_planes
            .iter()
            .map(|p| p.display_label.clone())
            .collect();
        plane_labels.sort();

        let explainer = vec![
            format!(
                "{} exposes distinct routing planes; the collectors observe different sessions and policy views, so disagreement among them is expected and analytically useful.",
                plane_labels.join(" and ")
            ),
            "Direct peer ASN membership and AS-in-path membership are different evidence classes.".to_string(),
            "Absence of baseline visibility at an observer is not evidence of no change there.".to_string(),
        ];

        let _ = &plane_labels;
        Some(SessionContext {
            lookup_map,
            explainer,
        })
    }

    /// Join one comparison row; None when the session is not in the audit.
    pub fn lookup(
        &self,
        family: &str,
        collector: &str,
        peer: &str,
    ) -> Option<(&str, &str, &str, &str, &str)> {
        let key = (
            normalize_family(family),
            collector.to_string(),
            peer.to_string(),
        );
        self.lookup_map
            .get(&key)
            .map(|(a, b, c, d, e)| (a.as_str(), b.as_str(), c.as_str(), d.as_str(), e.as_str()))
    }

    /// Explainer lines for the case-study first screen.
    pub fn planes_explainer(&self) -> Vec<String> {
        self.explainer.clone()
    }

    /// Narrow cross-plane conclusion built from the reviewed matrix.
    ///
    /// Structure (Session 35, Part 12): different views; the RouteViews
    /// observer's temporary absence; indirect RIS departures without
    /// complete stream absence; direct peer-exchange observations
    /// reported separately (here: not historically available); the views
    /// must not be combined as equivalent measurements.
    pub fn conclusion(&self, fallback: String) -> String {
        let matrix_path = Path::new("case-studies")
            .join("manlan-2019")
            .join("pilot")
            .join("cross-observer-matrix.json");
        let Ok(raw) = std::fs::read_to_string(&matrix_path) else {
            return fallback;
        };
        let Ok(matrix) = serde_json::from_str::<serde_json::Value>(&raw) else {
            return fallback;
        };
        let re_label = matrix["re_plane_label"]
            .as_str()
            .unwrap_or("the reviewed R&E plane");
        let pex_label = matrix["pex_plane_label"]
            .as_str()
            .unwrap_or("the reviewed peer-exchange plane");
        let Some(runs) = matrix["re_plane_runs"].as_array() else {
            return fallback;
        };

        let rv_absent = runs.iter().any(|r| {
            r["collector"] == "route-views2"
                && r["temporary_stream_absences"].as_u64().unwrap_or(0) > 0
        });
        let ris_departed = runs.iter().any(|r| {
            r["collector"].as_str().unwrap_or("").starts_with("rrc")
                && r["re_plane_departures"].as_u64().unwrap_or(0) > 0
        });
        let pex_baseline = matrix["pex_plane_preflights"]
            .as_array()
            .map(|rows| {
                rows.iter()
                    .any(|r| r["qualifying_frozen_streams"].as_u64().unwrap_or(0) > 0)
            })
            .unwrap_or(false);

        let event_label = matrix["reviewed_target"]
            .as_str()
            .unwrap_or("the reviewed routing event");
        let mut sentences: Vec<String> = Vec::new();
        sentences.push(format!(
            "RouteViews and RIS exposed different views of the {event_label} routing event."
        ));
        if rv_absent {
            sentences.push(format!(
                "The RouteViews observer selected through the {re_label} plane saw a temporary observer-stream absence."
            ));
        }
        if ris_departed {
            sentences.push(format!(
                "Some RIS observers saw indirect {re_label}-path departures without complete stream absence."
            ));
        }
        if pex_baseline {
            sentences.push(format!(
                "Direct {pex_label} observations, where historically available, are reported separately."
            ));
        } else {
            sentences.push(format!(
                "Direct {pex_label} observations were not historically available at the selected collectors and are reported separately."
            ));
        }
        sentences.push(
            "These are different routing-policy views and must not be combined as if they were equivalent measurements.".to_string(),
        );
        sentences.join(" ")
    }
}
