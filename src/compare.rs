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
    /// Human label for the ticket expectation.
    pub expectation_human: String,
    /// Concise observed-signature sentence.
    pub observed_signature: String,
    /// Assessment statement relative to the ticket expectation.
    pub assessment: String,
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
    /// Archive coverage statement from report.json.
    pub coverage: String,
    pub verdict: String,
    pub limitations: Vec<String>,
}

/// A planning-status summary for a blocked event (no BGP analysis).
#[derive(Debug, Clone, Serialize)]
pub struct BlockedPlanSummary {
    pub event_id: String,
    pub expectation_human: String,
    pub plan_status: String,
    pub reason: String,
}

/// Extract a blocked-plan summary from analysis_plan.json.
pub fn load_blocked_plan_summary(plan_json: &Path) -> Result<BlockedPlanSummary, String> {
    let content = std::fs::read_to_string(plan_json)
        .map_err(|e| format!("cannot read {}: {e}", plan_json.display()))?;
    let val: serde_json::Value =
        serde_json::from_str(&content).map_err(|e| format!("invalid plan JSON: {e}"))?;
    let schema = val
        .get("schema_version")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    if schema != crate::schema::ANALYSIS_PLAN_SCHEMA_VERSION as u64 {
        return Err(format!(
            "{}: plan schema v{schema} is not current (v{}); comparison requires current-schema plans",
            plan_json.display(),
            crate::schema::ANALYSIS_PLAN_SCHEMA_VERSION
        ));
    }
    Ok(BlockedPlanSummary {
        event_id: str_val(&val, &["event_id"]),
        expectation_human: expectation_human_label(&str_val(&val, &["expectation", "kind"])),
        plan_status: match val.get("plan") {
            Some(serde_json::Value::String(s)) => s.clone(),
            Some(v) => serde_json::to_string(v).unwrap_or_else(|_| "unknown".into()),
            None => "unknown".into(),
        },
        reason: str_val(&val, &["reason"]),
    })
}

/// Map an internal expectation-kind name to its human label.
fn expectation_human_label(kind: &str) -> String {
    match kind {
        "Redundant" => "Redundant attachment".to_string(),
        "NonRedundant" => "Loss of reachability".to_string(),
        "ParticipantRelationshipUnavailable" => "Relationship unavailable".to_string(),
        "PeerRelationshipUnavailable" => "Peer unavailable".to_string(),
        _ => kind.to_string(),
    }
}

/// The comparison artifact for two events, plus an optional blocked
/// planning-status entry.
#[derive(Debug, Clone, Serialize)]
pub struct ComparisonArtifact {
    pub schema_version: u32,
    pub a: EventSummary,
    pub b: EventSummary,
    pub blocked: Option<BlockedPlanSummary>,
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
        expectation_kind: expectation_kind.clone(),
        expectation_human: expectation_human_label(&expectation_kind),
        observed_signature: str_val(&val, &["result", "verdict_label"]),
        assessment: str_val(&val, &["assessment", "statement"]),
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
        coverage: str_val(&sig, &["observer_scope", "archive_coverage"]),
        limitations,
    })
}

/// Short observed-signature for the lead table.
fn short_signature(e: &EventSummary) -> String {
    if e.observed_signature.is_empty() {
        e.verdict.clone()
    } else {
        e.observed_signature.clone()
    }
}

/// Short assessment label for the lead table.
fn short_assessment(statement: &str) -> String {
    if statement.is_empty() {
        return "Unknown".to_string();
    }
    if statement.to_lowercase().starts_with("not assessable") {
        return "Not assessable".to_string();
    }
    if statement.to_lowercase().starts_with("indeterminate") {
        return "Indeterminate".to_string();
    }
    if statement.to_lowercase().starts_with("partially consistent") {
        return "Partially consistent".to_string();
    }
    if statement.to_lowercase().starts_with("inconsistent") {
        return "Inconsistent".to_string();
    }
    if statement.to_lowercase().starts_with("consistent") {
        return "Consistent".to_string();
    }
    "Unknown".to_string()
}

impl ComparisonArtifact {
    /// Build a comparison from two event summaries.
    pub fn new(a: EventSummary, b: EventSummary) -> Self {
        ComparisonArtifact {
            schema_version: COMPARISON_ARTIFACT_SCHEMA_VERSION,
            a,
            b,
            blocked: None,
        }
    }

    /// Attach a blocked planning-status entry.
    pub fn with_blocked(mut self, blocked: BlockedPlanSummary) -> Self {
        self.blocked = Some(blocked);
        self
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

        // Lead table: expectation, observed signature, assessment.
        buf.push_str(&format!(
            "{:<12}{:<26}{:<34}{}\n",
            "Event", "Ticket expectation", "Observed signature", "Assessment"
        ));
        buf.push_str(&format!(
            "{:<12}{:<26}{:<34}{}\n",
            "-----", "-----------------", "------------------", "----------"
        ));
        for e in [&self.a, &self.b] {
            let sig = short_signature(e);
            let assessment = short_assessment(&e.assessment);
            buf.push_str(&format!(
                "{:<12}{:<26}{:<34}{}\n",
                e.event_id, e.expectation_human, sig, assessment
            ));
        }
        buf.push('\n');

        // Planning status: blocked events are never presented as observed.
        if let Some(blocked) = &self.blocked {
            buf.push_str("Planning status (no BGP analysis performed)\n");
            buf.push_str(&format!(
                "{:<12}{:<26}{:<14}{}\n",
                "Event", "Ticket expectation", "Plan status", "Reason"
            ));
            buf.push_str(&format!(
                "{:<12}{:<26}{:<14}{}\n",
                "-----", "-----------------", "-----------", "------"
            ));
            buf.push_str(&format!(
                "{:<12}{:<26}{:<14}{}\n",
                blocked.event_id, blocked.expectation_human, "Blocked", blocked.reason
            ));
            buf.push('\n');
            buf.push_str(
                "The blocked event was not observed: no BGP archives were acquired and no route-state finding exists.\n",
            );
            buf.push('\n');
        }

        // Observation status and limitations.
        buf.push_str("Observation status\n");
        buf.push_str(&format!(
            "  {}: {} ({} observer-prefix streams, {} route instances, coverage: {})\n",
            self.a.event_id,
            self.a.observed_signature,
            self.a.observer_prefix_streams,
            self.a.route_instances,
            self.a.coverage
        ));
        buf.push_str(&format!(
            "  {}: {} ({} observer-prefix streams, {} route instances, coverage: {})\n",
            self.b.event_id,
            self.b.observed_signature,
            self.b.observer_prefix_streams,
            self.b.route_instances,
            self.b.coverage
        ));
        buf.push('\n');

        buf.push_str("Analysis limitations (observer-scoped, no severity score)\n");
        for (label, e) in [("a", &self.a), ("b", &self.b)] {
            buf.push_str(&format!("  {label} ({}):\n", e.event_id));
            for l in &e.limitations {
                buf.push_str(&format!("    - {l}\n"));
            }
        }
        buf.push('\n');
        buf.push_str(
            "Conclusions are scoped to externally exported BGP route state at the selected public collectors.",
        );
        buf.push('\n');
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
            "result": {
                "verdict": verdict,
                "verdict_label": "Partial routing impact observed",
                "finding": "Partial heterogeneous impact."
            },
            "assessment": {
                "statement": "Partially consistent with the participant-relationship-unavailable expectation."
            },
            "observed_event_signature": {
                "ticket_lifecycle": "Closed",
                "transit_predicate": "ContainsAny[11537]",
                "observer_scope": {
                    "collectors": ["route-views2"],
                    "baseline_observer_prefix_streams": 48,
                    "baseline_route_instances": 52,
                    "multiple_instance_streams": 3,
                    "archive_coverage": "Complete for the selected analysis plan"
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
        // The lead table is the expectation/observation/assessment view.
        assert!(text.contains("Ticket expectation"));
        assert!(text.contains("Observed signature"));
        assert!(text.contains("Assessment"));
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

    // ── Session 28: comparison redesign tests ─────────────────────

    fn blocked_plan_json() -> serde_json::Value {
        serde_json::json!({
            "schema_version": crate::schema::ANALYSIS_PLAN_SCHEMA_VERSION,
            "event_id": "INC0301970",
            "expectation": {"kind": "PeerRelationshipUnavailable"},
            "lifecycle": "Open",
            "plan": {"Blocked": {"reason": "MissingReviewedTransitPredicate"}},
            "reason": "MissingReviewedTransitPredicate",
            "broker_calls": 0,
            "mrt_files_examined": 0
        })
    }

    #[test]
    fn comparison_leads_with_expectation_observation_assessment() {
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
        let art = ComparisonArtifact::new(
            load_event_summary(&p1).unwrap(),
            load_event_summary(&p2).unwrap(),
        );
        let text = art.render_text();
        let table = text.find("Ticket expectation").unwrap();
        let status = text.find("Observation status").unwrap();
        let limits = text.find("Analysis limitations").unwrap();
        assert!(table < status && status < limits, "lead table comes first");
        // Assessment column values present.
        assert!(text.contains("Consistent") || text.contains("Partially consistent"));
    }

    #[test]
    fn blocked_event_is_distinct_from_completed_analyses() {
        let dir = tempfile::tempdir().unwrap();
        let p1 = dir.path().join("r1.json");
        let p2 = dir.path().join("r2.json");
        let pb = dir.path().join("plan.json");
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
        std::fs::write(&pb, serde_json::to_string(&blocked_plan_json()).unwrap()).unwrap();
        let art = ComparisonArtifact::new(
            load_event_summary(&p1).unwrap(),
            load_event_summary(&p2).unwrap(),
        )
        .with_blocked(load_blocked_plan_summary(&pb).unwrap());
        let text = art.render_text();
        assert!(text.contains("Planning status (no BGP analysis performed)"));
        assert!(text.contains("INC0301970"));
        assert!(text.contains("MissingReviewedTransitPredicate"));
        assert!(text.contains("The blocked event was not observed"));
        // The blocked event is never a row of observed signatures.
        let table = &text[..text.find("Planning status").unwrap()];
        assert!(
            !table.contains("INC0301970"),
            "blocked event must not appear in the observed table"
        );
    }

    #[test]
    fn comparison_contains_no_severity_score() {
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
        let art = ComparisonArtifact::new(
            load_event_summary(&p1).unwrap(),
            load_event_summary(&p2).unwrap(),
        );
        let json = serde_json::to_string(&art).unwrap();
        assert!(!json.contains("severity"), "{json}");
        assert!(!art.render_text().contains("more severe"));
    }

    #[test]
    fn comparison_uses_observer_scoped_language() {
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
        let art = ComparisonArtifact::new(
            load_event_summary(&p1).unwrap(),
            load_event_summary(&p2).unwrap(),
        );
        let text = art.render_text();
        assert!(!text.contains("global withdrawal"));
        assert!(text.contains("selected public collectors"));
        assert!(text.contains("observer-prefix streams"));
    }

    #[test]
    fn comparison_separates_blocked_and_observed_events() {
        // The planning-status entry must be separated from observed rows
        // even when the blocked event is present.
        let dir = tempfile::tempdir().unwrap();
        let p1 = dir.path().join("r1.json");
        let p2 = dir.path().join("r2.json");
        let pb = dir.path().join("plan.json");
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
        std::fs::write(&pb, serde_json::to_string(&blocked_plan_json()).unwrap()).unwrap();
        let art = ComparisonArtifact::new(
            load_event_summary(&p1).unwrap(),
            load_event_summary(&p2).unwrap(),
        )
        .with_blocked(load_blocked_plan_summary(&pb).unwrap());
        let text = art.render_text();
        let table_end = text.find("Planning status").unwrap();
        let observed_rows = &text[..table_end];
        assert!(!observed_rows.contains("INC0301970"));
        assert!(observed_rows.contains("INC1") && observed_rows.contains("INC2"));
        assert!(text[table_end..].contains("INC0301970"));
    }
}
