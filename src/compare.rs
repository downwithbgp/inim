//! Event comparison — deterministic side-by-side artifacts for two
//! completed event analyses.
//!
//! Reads each event's report.json (current schema only) and merges the
//! observed-event-signature and mechanism-hint summaries into a
//! comparison artifact. Never adds a severity score; uses observer-scoped
//! language throughout.

use serde::Serialize;
use std::path::Path;

/// Schema version of the comparison artifact.
pub const COMPARISON_ARTIFACT_SCHEMA_VERSION: u32 = crate::schema::COMPARISON_SCHEMA_VERSION;

/// A summarized event for comparison, extracted from report.json.
#[derive(Debug, Clone, Serialize)]
pub struct EventSummary {
    pub event_id: String,
    pub expectation_kind: String,
    pub expectation_provenance: String,
    pub ticket_lifecycle: String,
    pub transit_predicate: String,
    pub collectors: Vec<String>,
    pub observer_prefix_streams: usize,
    pub route_instances: usize,
    pub multiple_instance_streams: usize,
    pub unchanged: usize,
    pub prepend_only: usize,
    pub material_changes: usize,
    pub withdrawals: usize,
    pub transit_departures: usize,
    pub restorations: usize,
    pub gshut_streams: usize,
    pub semantic_waves: Vec<(String, String)>,
    pub verdict: String,
    pub limitations: Vec<String>,
}

/// The comparison artifact for two events.
#[derive(Debug, Clone, Serialize)]
pub struct ComparisonArtifact {
    pub schema_version: u32,
    pub a: EventSummary,
    pub b: EventSummary,
}

fn num(val: &serde_json::Value, path: &[&str]) -> usize {
    let mut cur = val;
    for p in path {
        cur = match cur.get(*p) {
            Some(v) => v,
            None => return 0,
        };
    }
    cur.as_u64().unwrap_or(0) as usize
}

fn str_val(val: &serde_json::Value, path: &[&str]) -> String {
    let mut cur = val;
    for p in path {
        cur = match cur.get(*p) {
            Some(v) => v,
            None => return String::new(),
        };
    }
    cur.as_str().unwrap_or("").to_string()
}

fn str_list(val: &serde_json::Value, path: &[&str]) -> Vec<String> {
    let mut cur = val;
    for p in path {
        cur = match cur.get(*p) {
            Some(v) => v,
            None => return vec![],
        };
    }
    cur.as_array()
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default()
}

/// Extract an event summary from a report.json (current schema).
pub fn load_event_summary(report_json: &Path) -> Result<EventSummary, String> {
    let content = std::fs::read_to_string(report_json)
        .map_err(|e| format!("cannot read {}: {e}", report_json.display()))?;
    let val: serde_json::Value =
        serde_json::from_str(&content).map_err(|e| format!("invalid report JSON: {e}"))?;

    // Schema check: only current report schema is parsed.
    let schema = val
        .get("schema_version")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    if schema != crate::schema::REPORT_SCHEMA_VERSION as u64 {
        return Err(format!(
            "{}: report schema v{schema} is not current (v{}); comparison requires current-schema reports",
            report_json.display(),
            crate::schema::REPORT_SCHEMA_VERSION
        ));
    }

    let sig = val
        .get("observed_event_signature")
        .cloned()
        .unwrap_or_default();
    let hints = val
        .get("observable_mechanism_hints")
        .cloned()
        .unwrap_or_default();

    let outcome = val.get("outcome").cloned().unwrap_or_default();
    let verdict = str_val(&outcome, &["assessment", "verdict"]);
    let expectation_kind = str_val(&outcome, &["assessment", "expectation", "kind"]);
    let expectation_provenance = str_val(&outcome, &["assessment", "expectation", "provenance"]);
    let limitations = str_list(&val, &["limitations"]);

    let waves = val
        .get("observed_event_signature")
        .and_then(|s| s.get("semantic_waves"))
        .and_then(|w| w.as_array())
        .map(|arr| {
            arr.iter()
                .map(|w| (str_val(w, &["id"]), str_val(w, &["label"])))
                .collect()
        })
        .unwrap_or_default();

    let gshut_streams = num(&hints, &["rfc8326", "gshut_streams"]);

    Ok(EventSummary {
        event_id: str_val(&val, &["event_id"]),
        expectation_kind,
        expectation_provenance,
        ticket_lifecycle: str_val(&sig, &["ticket_lifecycle"]),
        transit_predicate: str_val(&sig, &["transit_predicate"]),
        collectors: str_list(&sig, &["observer_scope", "collectors"]),
        observer_prefix_streams: num(
            &sig,
            &["observer_scope", "baseline_observer_prefix_streams"],
        ),
        route_instances: num(&sig, &["observer_scope", "baseline_route_instances"]),
        multiple_instance_streams: num(&sig, &["observer_scope", "multiple_instance_streams"]),
        unchanged: num(&sig, &["stream_lifecycle", "unchanged"]),
        prepend_only: num(&sig, &["stream_lifecycle", "prepend_only"]),
        material_changes: num(&sig, &["stream_lifecycle", "material_path_changes"]),
        withdrawals: num(&sig, &["stream_lifecycle", "withdrawn_streams"]),
        transit_departures: num(&sig, &["stream_lifecycle", "streams_departing_transit"]),
        restorations: num(&sig, &["stream_lifecycle", "restored_streams"]),
        gshut_streams,
        semantic_waves: waves,
        verdict,
        limitations,
    })
}

impl ComparisonArtifact {
    /// Build a comparison from two event summaries.
    pub fn new(a: EventSummary, b: EventSummary) -> Self {
        ComparisonArtifact {
            schema_version: COMPARISON_ARTIFACT_SCHEMA_VERSION,
            a,
            b,
        }
    }

    /// Render a deterministic human-readable comparison.
    pub fn render_text(&self) -> String {
        let mut buf = String::new();
        buf.push_str(&format!(
            "Event comparison: {} vs {}\n",
            self.a.event_id, self.b.event_id
        ));
        buf.push_str(&format!("Comparison schema: v{}\n", self.schema_version));
        buf.push('\n');

        let rows: Vec<(String, String, String)> = vec![
            (
                "Expectation".into(),
                self.a.expectation_kind.clone(),
                self.b.expectation_kind.clone(),
            ),
            (
                "Lifecycle".into(),
                self.a.ticket_lifecycle.clone(),
                self.b.ticket_lifecycle.clone(),
            ),
            (
                "Convention provenance".into(),
                self.a.expectation_provenance.clone(),
                self.b.expectation_provenance.clone(),
            ),
            (
                "TransitPredicate".into(),
                self.a.transit_predicate.clone(),
                self.b.transit_predicate.clone(),
            ),
            (
                "Collectors".into(),
                self.a.collectors.join(","),
                self.b.collectors.join(","),
            ),
            (
                "Observer-prefix streams".into(),
                self.a.observer_prefix_streams.to_string(),
                self.b.observer_prefix_streams.to_string(),
            ),
            (
                "Route instances".into(),
                self.a.route_instances.to_string(),
                self.b.route_instances.to_string(),
            ),
            (
                "Multiple-instance streams".into(),
                self.a.multiple_instance_streams.to_string(),
                self.b.multiple_instance_streams.to_string(),
            ),
            (
                "Unchanged".into(),
                self.a.unchanged.to_string(),
                self.b.unchanged.to_string(),
            ),
            (
                "Prepend-only".into(),
                self.a.prepend_only.to_string(),
                self.b.prepend_only.to_string(),
            ),
            (
                "Material changes".into(),
                self.a.material_changes.to_string(),
                self.b.material_changes.to_string(),
            ),
            (
                "Withdrawals (selected streams)".into(),
                self.a.withdrawals.to_string(),
                self.b.withdrawals.to_string(),
            ),
            (
                "Transit departures".into(),
                self.a.transit_departures.to_string(),
                self.b.transit_departures.to_string(),
            ),
            (
                "Restorations".into(),
                self.a.restorations.to_string(),
                self.b.restorations.to_string(),
            ),
            (
                "GSHUT streams".into(),
                self.a.gshut_streams.to_string(),
                self.b.gshut_streams.to_string(),
            ),
            (
                "Semantic waves".into(),
                self.a
                    .semantic_waves
                    .iter()
                    .map(|(id, l)| format!("{id}:{l}"))
                    .collect::<Vec<_>>()
                    .join(","),
                self.b
                    .semantic_waves
                    .iter()
                    .map(|(id, l)| format!("{id}:{l}"))
                    .collect::<Vec<_>>()
                    .join(","),
            ),
            (
                "Final assessment".into(),
                self.a.verdict.clone(),
                self.b.verdict.clone(),
            ),
        ];

        let label_w = rows.iter().map(|(l, _, _)| l.len()).max().unwrap_or(8) + 2;
        let a_w = rows
            .iter()
            .map(|(_, a, _)| a.len())
            .max()
            .unwrap_or(self.a.event_id.len())
            + 2;
        let header = format!(
            "{:<label_w$}{:<a_w$}{}",
            "Metric", self.a.event_id, self.b.event_id
        );
        buf.push_str(&header);
        buf.push('\n');
        buf.push_str(&"-".repeat(header.len()));
        buf.push('\n');
        for (label, a, b) in rows {
            buf.push_str(&format!("{label:<label_w$}{a:<a_w$}{b}\n"));
        }
        buf.push('\n');
        buf.push_str("Limitations (observer-scoped, no severity score):\n");
        for (label, e) in [("a", &self.a), ("b", &self.b)] {
            buf.push_str(&format!("  {label} ({}):\n", e.event_id));
            for l in &e.limitations {
                buf.push_str(&format!("    • {l}\n"));
            }
        }
        buf
    }

    /// Write the comparison artifacts to an output directory.
    ///
    /// Files are named `{a.event_id}-vs-{b.event_id}.json` and `.txt` per
    /// the artifact contract.
    pub fn write(&self, out_dir: &Path) -> Result<(), String> {
        std::fs::create_dir_all(out_dir)
            .map_err(|e| format!("cannot create {}: {e}", out_dir.display()))?;
        let base = format!("{}-vs-{}", self.a.event_id, self.b.event_id);
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| format!("comparison JSON serialization failed: {e}"))?;
        std::fs::write(out_dir.join(format!("{base}.json")), json)
            .map_err(|e| format!("cannot write {base}.json: {e}"))?;
        std::fs::write(out_dir.join(format!("{base}.txt")), self.render_text())
            .map_err(|e| format!("cannot write {base}.txt: {e}"))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_report(event_id: &str, verdict: &str) -> serde_json::Value {
        serde_json::json!({
            "schema_version": crate::schema::REPORT_SCHEMA_VERSION,
            "event_id": event_id,
            "observed_event_signature": {
                "ticket_lifecycle": "Closed",
                "transit_predicate": "ContainsAny[11537]",
                "observer_scope": {
                    "collectors": ["route-views2"],
                    "baseline_observer_prefix_streams": 48,
                    "baseline_route_instances": 52,
                    "multiple_instance_streams": 3
                },
                "stream_lifecycle": {
                    "unchanged": 10,
                    "prepend_only": 5,
                    "material_path_changes": 12,
                    "withdrawn_streams": 14,
                    "streams_departing_transit": 0,
                    "restored_streams": 2
                },
                "semantic_waves": [
                    {"id": "wave-1", "label": "StreamWithdrawal"}
                ]
            },
            "observable_mechanism_hints": {
                "rfc8326": {"gshut_streams": 1}
            },
            "outcome": {
                "status": "completed",
                "assessment": {
                    "verdict": verdict,
                    "expectation": {
                        "kind": "ParticipantRelationshipUnavailable",
                        "provenance": "Internet2 (GRNOC) convention"
                    }
                }
            },
            "limitations": ["Selected collectors do not provide global visibility."]
        })
    }

    #[test]
    fn event_summary_extracts_expected_fields() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("report.json");
        std::fs::write(
            &path,
            serde_json::to_string_pretty(&sample_report("INC1", "PARTIAL IMPACT")).unwrap(),
        )
        .unwrap();
        let s = load_event_summary(&path).unwrap();
        assert_eq!(s.event_id, "INC1");
        assert_eq!(s.verdict, "PARTIAL IMPACT");
        assert_eq!(s.observer_prefix_streams, 48);
        assert_eq!(s.route_instances, 52);
        assert_eq!(s.multiple_instance_streams, 3);
        assert_eq!(s.withdrawals, 14);
        assert_eq!(s.gshut_streams, 1);
        assert_eq!(
            s.semantic_waves,
            vec![("wave-1".to_string(), "StreamWithdrawal".to_string())]
        );
    }

    #[test]
    fn comparison_has_no_severity_score_and_is_deterministic() {
        let dir = tempfile::tempdir().unwrap();
        let p1 = dir.path().join("r1.json");
        let p2 = dir.path().join("r2.json");
        std::fs::write(
            &p1,
            serde_json::to_string(&sample_report("INC1", "PARTIAL IMPACT")).unwrap(),
        )
        .unwrap();
        std::fs::write(
            &p2,
            serde_json::to_string(&sample_report("INC2", "NO OBSERVABLE BGP IMPACT")).unwrap(),
        )
        .unwrap();
        let a = load_event_summary(&p1).unwrap();
        let b = load_event_summary(&p2).unwrap();
        let art = ComparisonArtifact::new(a, b);
        let text = art.render_text();
        assert!(text.contains("Withdrawals (selected streams)"));
        // Deterministic: rendering twice yields identical output.
        assert_eq!(text, art.render_text());
        let out = dir.path().join("cmp");
        art.write(&out).unwrap();
        let json = std::fs::read_to_string(out.join("INC1-vs-INC2.json")).unwrap();
        let val: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(val["schema_version"], COMPARISON_ARTIFACT_SCHEMA_VERSION);
        // No severity score is added to the artifact.
        assert!(val.get("severity").is_none());
        assert!(!json.contains("\"severity\""), "{json}");
    }

    #[test]
    fn old_schema_report_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("report.json");
        let mut v = sample_report("INC1", "PARTIAL IMPACT");
        v["schema_version"] = serde_json::json!(0);
        std::fs::write(&path, serde_json::to_string(&v).unwrap()).unwrap();
        let err = load_event_summary(&path).unwrap_err();
        assert!(err.contains("schema"), "{err}");
    }
}
