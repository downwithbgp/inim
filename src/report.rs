//! Report module — deterministic terminal and JSON output.
//!
//! Renders completed assessments. Must not perform analysis or derive
//! verdicts.

use std::io::Write;

use crate::domain::assessment::EventAssessment;
use crate::waves;

/// Render a human-readable terminal report.
pub fn render_terminal(assessment: &EventAssessment, data_note: &str) -> String {
    let mut buf = Vec::new();

    writeln!(buf, "{}", "=".repeat(60)).ok();
    writeln!(buf, "{}", assessment.event_id).ok();
    writeln!(buf, "{}", assessment.expectation.description).ok();
    writeln!(buf, "{}", "=".repeat(60)).ok();
    writeln!(buf, "Data source: {data_note}").ok();
    writeln!(buf).ok();

    writeln!(buf, "Declared expectation").ok();
    writeln!(buf, "  Kind:       {:?}", assessment.expectation.kind).ok();
    writeln!(
        buf,
        "  Provenance: {}",
        assessment.expectation.provenance
    )
    .ok();
    writeln!(buf).ok();

    writeln!(buf, "Observed").ok();
    writeln!(
        buf,
        "  Transitions:  {}",
        assessment.evidence.first().map(|e| e.description.as_str()).unwrap_or("0")
    ).ok();

    for wave in &assessment.waves {
        writeln!(buf, "  {}", wave.label).ok();
        if let Some(ref motif) = wave.motif {
            let class = waves::classify_motif(motif);
            writeln!(buf, "    {}", class.heading()).ok();
            writeln!(buf, "      {}", motif.expanded).ok();
            writeln!(buf, "    Structure").ok();
            for line in &motif.structure {
                writeln!(buf, "      {}", line).ok();
            }
            writeln!(buf, "    Occurrences").ok();
            writeln!(buf, "      {}", motif.occurrences).ok();
            writeln!(buf, "    Covered transitions").ok();
            writeln!(
                buf,
                "      {} of {}",
                motif.covered_terminals, motif.total_terminals
            ).ok();
            writeln!(buf, "    Representative evidence").ok();
            for er in &motif.evidence_ranges {
                writeln!(
                    buf,
                    "      {} {} [{} → {}]",
                    er.observer, er.prefix,
                    er.time_start.format("%H:%M:%S"),
                    er.time_end.format("%H:%M:%S"),
                ).ok();
            }
        }
    }
    writeln!(buf).ok();

    writeln!(buf, "Verdict").ok();
    writeln!(buf, "  {}", assessment.verdict).ok();
    writeln!(buf).ok();

    writeln!(buf, "Evidence").ok();
    for ev in &assessment.evidence {
        writeln!(buf, "  {}", ev.description).ok();
        for rec in &ev.source_records {
            writeln!(buf, "    - {}", rec).ok();
        }
    }
    writeln!(buf).ok();

    writeln!(
        buf,
        "Generated at: {}",
        assessment.generated_at
    ).ok();

    String::from_utf8(buf).unwrap_or_else(|_| "report encoding error".into())
}

/// Render a structured JSON report.
pub fn render_json(assessment: &EventAssessment, data_note: &str) -> serde_json::Value {
    let mut val = serde_json::to_value(assessment).unwrap_or(serde_json::Value::Null);
    if let serde_json::Value::Object(ref mut map) = val {
        map.insert("data_source".to_string(), serde_json::Value::String(data_note.to_string()));
    }
    val
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::assessment::{Evidence, Verdict};
    use crate::domain::event::EventId;
    use crate::domain::expectation::{ExpectationKind, ImpactExpectation};
    use chrono::{TimeZone, Utc};

    fn sample_assessment() -> EventAssessment {
        EventAssessment {
            event_id: EventId::from("CHG0107955"),
            expectation: ImpactExpectation {
                kind: ExpectationKind::Redundant,
                description: "Parenthesized site code (NEWY32AOA) indicates expected redundancy"
                    .into(),
                provenance: "Internet2 title convention".into(),
            },
            verdict: Verdict::ExpectedRedundantImpact,
            evidence: vec![
                Evidence {
                    description: "Total route transitions observed: 3".into(),
                    source_records: vec!["rv2:AS6447 2025-06-15T05:25:18Z".into()],
                },
                Evidence {
                    description: "Withdrawals: 0".into(),
                    source_records: vec![],
                },
            ],
            waves: vec![],
            generated_at: Utc.with_ymd_and_hms(2025, 6, 15, 5, 47, 0).unwrap(),
        }
    }

    #[test]
    fn terminal_report_contains_verdict() {
        let report = render_terminal(&sample_assessment(), "SYNTHETIC (test)");
        assert!(report.contains("CHG0107955"));
        assert!(report.contains("EXPECTED REDUNDANT IMPACT"));
        assert!(report.contains("NEWY32AOA"));
        assert!(report.contains("Total route transitions observed: 3"));
        assert!(report.contains("Data source: SYNTHETIC"));
    }

    #[test]
    fn terminal_report_contains_evidence() {
        let report = render_terminal(&sample_assessment(), "test");
        assert!(report.contains("Total route transitions observed"));
        assert!(report.contains("rv2:AS6447"));
    }

    #[test]
    fn json_report_roundtrips() {
        let json = render_json(&sample_assessment(), "test-fixture");
        assert!(json.get("data_source").and_then(|v| v.as_str()) == Some("test-fixture"));
        let parsed: EventAssessment = serde_json::from_value(json).unwrap();
        assert_eq!(parsed.event_id.0, "CHG0107955");
        assert_eq!(parsed.verdict, Verdict::ExpectedRedundantImpact);
    }
}
