//! Report module — deterministic terminal and JSON output.
//!
//! Renders completed assessments. Must not perform analysis or derive
//! verdicts.

use serde_json::Value;
use std::io::Write;

use crate::domain::assessment::EventAssessment;

/// Render a human-readable terminal report.
pub fn render_terminal(assessment: &EventAssessment) -> String {
    let mut buf = Vec::new();

    writeln!(buf, "{}", "=".repeat(60)).ok();
    writeln!(buf, "{}", assessment.event_id).ok();
    writeln!(buf, "{}", assessment.expectation.description).ok();
    writeln!(buf, "{}", "=".repeat(60)).ok();
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
pub fn render_json(assessment: &EventAssessment) -> Value {
    serde_json::to_value(assessment).unwrap_or(Value::Null)
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
        let report = render_terminal(&sample_assessment());
        assert!(report.contains("CHG0107955"));
        assert!(report.contains("EXPECTED REDUNDANT IMPACT"));
        assert!(report.contains("NEWY32AOA"));
        assert!(report.contains("Total route transitions observed: 3"));
    }

    #[test]
    fn terminal_report_contains_evidence() {
        let report = render_terminal(&sample_assessment());
        assert!(report.contains("Total route transitions observed"));
        assert!(report.contains("rv2:AS6447"));
    }

    #[test]
    fn json_report_roundtrips() {
        let json = render_json(&sample_assessment());
        let parsed: EventAssessment = serde_json::from_value(json).unwrap();
        assert_eq!(parsed.event_id.0, "CHG0107955");
        assert_eq!(parsed.verdict, Verdict::ExpectedRedundantImpact);
    }
}
