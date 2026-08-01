//! Case-study comparison model (Session 30, Part 10).
//!
//! A reviewed presentation model pairing operator-reported claims with
//! public-BGP observation derived from linked AnalysisRuns. A comparison is
//! reviewed interpretation — never a causal foreign-key relationship:
//! temporal consistency never proves causation, and `ConfirmedCause` is
//! never produced. Non-observable claims are reported as
//! NotDirectlyObservable, not as missed detections; with no linked run the
//! honest result is Indeterminate ("historical analysis not yet executed").

use rusqlite::Connection;

use super::domain::*;
use super::phase_summary::{self, RunPhaseSummaries};

pub const RELATION_BEFORE: &str = "Before";
pub const RELATION_DURING: &str = "During";
pub const RELATION_AFTER: &str = "After";
pub const RELATION_OVERLAPPING: &str = "Overlapping";
pub const RELATION_NO_OBSERVED_COUNTERPART: &str = "NoObservedCounterpart";
pub const RELATION_NOT_DIRECTLY_OBSERVABLE: &str = "NotDirectlyObservable";
pub const RELATION_INDETERMINATE: &str = "Indeterminate";

/// One comparison row: operator-reported vs public-BGP vs interpretation.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ComparisonRow {
    pub claim_id: i64,
    pub claim_type: String,
    /// Reviewed operator wording (with qualification).
    pub operator_report: String,
    /// The operator-reported time anchor, when present.
    pub operator_time: Option<String>,
    /// Evidence-derived BGP observation (or planning status).
    pub bgp_observation: String,
    /// Explicit relationship label from the vocabulary.
    pub interpretation: String,
    /// Visibility limitation.
    pub limitation: String,
    /// Runs that contributed BGP evidence to this row.
    pub contributing_run_ids: Vec<i64>,
}

/// Load linked run ids for a case study (deterministic order).
pub fn linked_runs(conn: &Connection, case_study_id: i64) -> Result<Vec<i64>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT run_id FROM case_study_analysis_links
             WHERE case_study_id = ?1 ORDER BY run_id",
        )
        .map_err(|e| format!("catalog read failed: {e}"))?;
    let rows = stmt
        .query_map([case_study_id], |r| r.get(0))
        .map_err(|e| format!("catalog read failed: {e}"))?;
    rows.collect::<Result<_, _>>()
        .map_err(|e| format!("catalog read failed: {e}"))
}

/// Load claims for a case study (deterministic order).
pub fn list_claims(conn: &Connection, case_study_id: i64) -> Result<Vec<CaseStudyClaim>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, case_study_id, claim_type, claim_text, qualification,
                    source_document_id, source_page_or_section, review_status,
                    time_or_phase, observability, observability_rationale, sort_order
             FROM case_study_claims WHERE case_study_id = ?1 ORDER BY sort_order",
        )
        .map_err(|e| format!("catalog read failed: {e}"))?;
    let rows = stmt
        .query_map([case_study_id], |r| {
            Ok(CaseStudyClaim {
                id: r.get(0)?,
                case_study_id: r.get(1)?,
                claim_type: r.get(2)?,
                claim_text: r.get(3)?,
                qualification: r.get(4)?,
                source_document_id: r.get(5)?,
                source_page_or_section: r.get(6)?,
                review_status: r.get(7)?,
                time_or_phase: r.get(8)?,
                observability: r.get(9)?,
                observability_rationale: r.get(10)?,
                sort_order: r.get(11)?,
            })
        })
        .map_err(|e| format!("catalog read failed: {e}"))?;
    rows.collect::<Result<_, _>>()
        .map_err(|e| format!("catalog read failed: {e}"))
}

/// A claim's time anchor, if any: a point time or a phase window.
#[derive(Debug, Clone)]
enum ClaimWindow {
    Point(String),
    Phase(usize),
}

/// Parse a `time_or_phase` anchor: an ISO-8601 UTC time or `phase:N`.
fn parse_anchor(s: &str) -> Result<ClaimWindow, String> {
    if let Some(idx) = s.strip_prefix("phase:") {
        let n: usize = idx
            .parse()
            .map_err(|_| format!("invalid phase anchor '{s}'"))?;
        return Ok(ClaimWindow::Phase(n));
    }
    chrono::DateTime::parse_from_rfc3339(s)
        .map(|_| ClaimWindow::Point(s.to_string()))
        .map_err(|_| format!("invalid time_or_phase anchor '{s}'"))
}

/// Aggregate phase summaries across the linked runs.
struct AggregatedPhase {
    transition_count: usize,
    first_evidence_utc: Option<String>,
    contributing_runs: Vec<i64>,
}

/// Build the comparison matrix for a case study.
pub fn build_comparison(
    conn: &Connection,
    case_study_id: i64,
) -> Result<Vec<ComparisonRow>, String> {
    let claims = list_claims(conn, case_study_id)?;
    let runs = linked_runs(conn, case_study_id)?;
    let phases: Vec<(i64, String, String, String)> = {
        let mut stmt = conn
            .prepare(
                "SELECT id, label, start_utc, end_utc FROM case_study_phases
                 WHERE case_study_id = ?1 ORDER BY sort_order",
            )
            .map_err(|e| format!("catalog read failed: {e}"))?;
        let rows = stmt
            .query_map([case_study_id], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                ))
            })
            .map_err(|e| format!("catalog read failed: {e}"))?;
        rows.collect::<Result<_, _>>()
            .map_err(|e| format!("catalog read failed: {e}"))?
    };

    // Summarize every linked run once.
    let mut run_summaries: Vec<RunPhaseSummaries> = Vec::new();
    for run_id in &runs {
        let s = phase_summary::summarize_run(conn, *run_id, case_study_id)?;
        run_summaries.push(s);
    }

    // Each run's event window (from its exact plan payload). A narrow pilot
    // run must never fabricate observations for windows it did not cover.
    let mut run_windows: std::collections::HashMap<i64, (String, String)> =
        std::collections::HashMap::new();
    for run_id in &runs {
        let payload: Option<String> = conn
            .query_row(
                "SELECT p.payload FROM analysis_plans p
                 JOIN analysis_runs r ON r.plan_id = p.id WHERE r.id = ?1",
                [run_id],
                |r| r.get(0),
            )
            .ok();
        let window = payload.and_then(|p| {
            serde_json::from_str::<serde_json::Value>(&p)
                .ok()
                .and_then(|v| v.get("analysis_window").cloned())
                .and_then(|w| {
                    Some((
                        w.get("start")?.as_str()?.to_string(),
                        w.get("end")?.as_str()?.to_string(),
                    ))
                })
        });
        if let Some(w) = window {
            run_windows.insert(*run_id, w);
        }
    }

    // Aggregate per-phase across runs.
    let mut aggregated: Vec<AggregatedPhase> = Vec::new();
    for (pi, _phase) in phases.iter().enumerate() {
        let mut agg = AggregatedPhase {
            transition_count: 0,
            first_evidence_utc: None,
            contributing_runs: Vec::new(),
        };
        for rs in &run_summaries {
            if let Some(p) = rs.phases.get(pi) {
                let n = p.announcements + p.withdrawals + p.path_changes + p.restorations;
                if n > 0 {
                    agg.transition_count += n;
                    if !agg.contributing_runs.contains(&rs.run_id) {
                        agg.contributing_runs.push(rs.run_id);
                    }
                }
                if let Some(t) = &p.first_evidence_utc {
                    if agg.first_evidence_utc.is_none()
                        || agg.first_evidence_utc.as_deref() > Some(t.as_str())
                    {
                        agg.first_evidence_utc = Some(t.clone());
                    }
                }
            }
        }
        aggregated.push(agg);
    }

    // Transitions outside any phase across runs are surfaced through the
    // phase summaries (never silently dropped).
    let mut rows = Vec::new();
    for c in &claims {
        // The public-BGP comparison uses the first five claim categories;
        // process findings are not BGP-observable conditions.
        if c.claim_type == CLAIM_TYPE_PROCESS_FINDING {
            continue;
        }
        let operator_report = match &c.qualification {
            Some(q) => format!("{} (qualification: {q})", c.claim_text),
            None => c.claim_text.clone(),
        };

        // Resolve the claim window.
        let window: Option<(String, String)> = match c.time_or_phase.as_deref() {
            None => None,
            Some(anchor) => match parse_anchor(anchor) {
                Ok(ClaimWindow::Point(t)) => Some((t.clone(), t.clone())),
                Ok(ClaimWindow::Phase(pi)) => {
                    phases.get(pi).map(|(_, _l, s, e)| (s.clone(), e.clone()))
                }
                Err(_) => None,
            },
        };

        // Non-observable conditions are classified, not missed detections.
        if c.observability == OBSERVABILITY_NOT_DIRECTLY_VISIBLE {
            rows.push(ComparisonRow {
                claim_id: c.id,
                claim_type: c.claim_type.clone(),
                operator_report,
                operator_time: c.time_or_phase.clone(),
                bgp_observation: "not observable in public BGP (reviewed classification)"
                    .to_string(),
                interpretation: RELATION_NOT_DIRECTLY_OBSERVABLE.to_string(),
                limitation: c.observability_rationale.clone(),
                contributing_run_ids: Vec::new(),
            });
            continue;
        }

        // No linked runs: honest planning status, never a verdict.
        if runs.is_empty() {
            rows.push(ComparisonRow {
                claim_id: c.id,
                claim_type: c.claim_type.clone(),
                operator_report,
                operator_time: c.time_or_phase.clone(),
                bgp_observation: "no linked analysis run; historical analysis not yet executed".to_string(),
                interpretation: RELATION_INDETERMINATE.to_string(),
                limitation: "no public-BGP conclusion until historical target mappings and the archive plan are reviewed".to_string(),
                contributing_run_ids: Vec::new(),
            });
            continue;
        }

        // BGP activity relative to the claim window.
        let (rel, bgp_text, contributing) = match &window {
            Some((ws, we)) => {
                // Only runs whose event window intersects the claim window
                // may contribute an observation or a no-counterpart
                // conclusion.
                let covering: Vec<&RunPhaseSummaries> = run_summaries
                    .iter()
                    .filter(|rs| match run_windows.get(&rs.run_id) {
                        // Half-open windows, consistent with phase
                        // assignment: a run ending exactly at the claim
                        // window start does not cover it.
                        Some((s, e)) => e > ws && s <= we,
                        None => true, // unknown window: conservative
                    })
                    .collect();
                if covering.is_empty() {
                    (
                        RELATION_INDETERMINATE.to_string(),
                        "no linked analysis run covers this window; no observation was attempted"
                            .to_string(),
                        Vec::new(),
                    )
                } else {
                    let mut in_window: Vec<String> = Vec::new();
                    let mut before_window: Vec<String> = Vec::new();
                    let mut after_window: Vec<String> = Vec::new();
                    let mut runs_with_evidence: Vec<i64> = Vec::new();
                    for rs in &covering {
                        for p in &rs.phases {
                            let phase_in_window = p.end_utc > *ws && p.start_utc <= *we;
                            if phase_in_window && p.first_evidence_utc.is_some() {
                                in_window
                                    .push(p.first_evidence_utc.as_deref().unwrap().to_string());
                                if !runs_with_evidence.contains(&rs.run_id) {
                                    runs_with_evidence.push(rs.run_id);
                                }
                            }
                        }
                        // Whole-run activity before/after the window.
                        if let Some(first) =
                            rs.phases.iter().find_map(|p| p.first_evidence_utc.clone())
                        {
                            if first < *ws {
                                before_window.push(first);
                            } else if first > *we {
                                after_window.push(first);
                            }
                        }
                    }
                    if !in_window.is_empty() {
                        in_window.sort();
                        (
                            RELATION_OVERLAPPING.to_string(),
                            format!(
                                "route-state transitions observed in the window; first at {} ({} transition series)",
                                in_window[0], in_window.len()
                            ),
                            runs_with_evidence,
                        )
                    } else if !before_window.is_empty() {
                        before_window.sort();
                        (
                            RELATION_BEFORE.to_string(),
                            format!(
                                "earliest route-state transitions precede the window ({})",
                                before_window[0]
                            ),
                            runs_with_evidence,
                        )
                    } else if !after_window.is_empty() {
                        after_window.sort();
                        (
                            RELATION_AFTER.to_string(),
                            format!(
                                "earliest route-state transitions follow the window ({})",
                                after_window[0]
                            ),
                            runs_with_evidence,
                        )
                    } else {
                        (
                            RELATION_NO_OBSERVED_COUNTERPART.to_string(),
                            "no route-state transitions observed in any linked run".to_string(),
                            runs_with_evidence,
                        )
                    }
                }
            }
            None => {
                let total: usize = aggregated.iter().map(|a| a.transition_count).sum();
                if total > 0 {
                    (
                            RELATION_DURING.to_string(),
                            format!(
                                "route-state transitions observed across linked runs ({total} stream-counts); first at {}",
                                aggregated.iter().find_map(|a| a.first_evidence_utc.clone()).unwrap_or_default()
                            ),
                            aggregated.iter().flat_map(|a| a.contributing_runs.clone()).collect(),
                        )
                } else {
                    (
                        RELATION_NO_OBSERVED_COUNTERPART.to_string(),
                        "no route-state transitions observed in any linked run".to_string(),
                        Vec::new(),
                    )
                }
            }
        };

        let temporal = matches!(
            rel.as_str(),
            RELATION_OVERLAPPING | RELATION_BEFORE | RELATION_AFTER | RELATION_DURING
        );
        let limitation = if rel == RELATION_INDETERMINATE {
            "the claim window is not covered by any linked run; no observation was attempted"
                .to_string()
        } else if c.observability == OBSERVABILITY_INDIRECTLY_VISIBLE {
            format!(
                "indirect visibility only: the condition itself is not directly observable in public BGP; only exported route consequences may appear ({})",
                c.observability_rationale
            )
        } else if temporal {
            "temporal consistency does not prove causation".to_string()
        } else {
            format!(
                "no observed counterpart in the linked runs; BGP absence does not refute the reported condition ({})",
                c.observability_rationale
            )
        };
        rows.push(ComparisonRow {
            claim_id: c.id,
            claim_type: c.claim_type.clone(),
            operator_report,
            operator_time: c.time_or_phase.clone(),
            bgp_observation: bgp_text,
            interpretation: rel,
            limitation,
            contributing_run_ids: contributing,
        });
    }

    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::db;
    use crate::catalog::store;

    fn open_temp_db() -> (tempfile::TempDir, Connection) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("catalog.sqlite");
        let conn = db::open_catalog(&path).unwrap();
        (dir, conn)
    }

    fn seed_run(conn: &Connection, started: &str) -> i64 {
        let e = store::upsert_event(conn, "grnoc", "T1", "2019-08-22T00:00:00Z").unwrap();
        let sid = crate::catalog::tests::sample_snapshot(e, r#"{"title":"t"}"#);
        let sid = store::insert_snapshot(conn, e, &sid).unwrap();
        let mid = store::insert_manifest_revision(
            conn,
            &crate::catalog::tests::sample_manifest_revision(e, sid, r#"{"o":1}"#),
        )
        .unwrap();
        let pid =
            store::insert_plan(conn, &crate::catalog::tests::sample_plan(mid, "Ready")).unwrap();
        store::insert_run(conn, &crate::catalog::tests::sample_run(pid, started)).unwrap()
    }

    fn seed_phases(conn: &Connection, cs_id: i64) {
        let doc = store::insert_reference_document(
            conn,
            &ReferenceDocument {
                id: 0,
                title: "AAR".to_string(),
                source_url: Some("https://example.invalid/aar.pdf".to_string()),
                doc_type: "AfterActionReport".to_string(),
                redistribution_status: "Unknown".to_string(),
                publication_date: None,
                provenance: "p".to_string(),
                imported_utc: "2019-09-01T00:00:00Z".to_string(),
            },
        )
        .unwrap();
        for (sort, label, start, end) in [
            (0, "first", "2019-08-21T04:00:00Z", "2019-08-21T10:00:00Z"),
            (1, "second", "2019-08-21T10:00:00Z", "2019-08-21T14:00:00Z"),
        ] {
            store::insert_case_study_phase(
                conn,
                &CaseStudyPhase {
                    id: 0,
                    case_study_id: cs_id,
                    label: label.to_string(),
                    start_utc: start.to_string(),
                    end_utc: end.to_string(),
                    start_precision: PHASE_PRECISION_EXACT.to_string(),
                    end_precision: PHASE_PRECISION_EXACT.to_string(),
                    description: "d".to_string(),
                    source_document_id: doc,
                    source_page_or_section: "Timeline".to_string(),
                    review_status: "Reviewed".to_string(),
                    sort_order: sort,
                },
            )
            .unwrap();
        }
    }

    fn seed_claim(
        conn: &Connection,
        cs_id: i64,
        doc_id: i64,
        claim_type: &str,
        observability: &str,
        time_or_phase: Option<&str>,
    ) -> i64 {
        seed_claim_sort(
            conn,
            cs_id,
            doc_id,
            claim_type,
            observability,
            time_or_phase,
            0,
        )
    }

    fn seed_claim_sort(
        conn: &Connection,
        cs_id: i64,
        doc_id: i64,
        claim_type: &str,
        observability: &str,
        time_or_phase: Option<&str>,
        sort_order: i64,
    ) -> i64 {
        let claim = CaseStudyClaim {
            id: 0,
            case_study_id: cs_id,
            claim_type: claim_type.to_string(),
            claim_text: "operator reports the condition".to_string(),
            qualification: Some("reported, not measured".to_string()),
            source_document_id: doc_id,
            source_page_or_section: "Summary".to_string(),
            review_status: "Reviewed".to_string(),
            time_or_phase: time_or_phase.map(|s| s.to_string()),
            observability: observability.to_string(),
            observability_rationale: "reviewed classification".to_string(),
            sort_order,
        };
        store::insert_case_study_claim(conn, &claim).unwrap()
    }

    fn seed_stream(conn: &Connection, run_id: i64, prefix: &str) {
        store::insert_streams(
            conn,
            run_id,
            &[StreamLifecycleSummary {
                id: 0,
                run_id,
                collector: "rv2".to_string(),
                peer_ip: "1.1.1.1".to_string(),
                prefix: prefix.to_string(),
                category: "Unchanged".to_string(),
                baseline_instances: 1,
                max_active_instances: 1,
                transition_count: 0,
                withdrawn: false,
                restored: false,
                transit_state: "Retained".to_string(),
                add_path_ambiguous: false,
                evidence_refs: "[]".to_string(),
            }],
        )
        .unwrap();
    }

    fn insert_t(
        conn: &Connection,
        run_id: i64,
        seq: i64,
        kind: &str,
        at: &str,
        prefix: &str,
        oid: i64,
    ) {
        store::insert_run_transition(
            conn,
            &RunTransitionRecord {
                id: 0,
                run_id,
                seq,
                kind: kind.to_string(),
                occurred_utc: at.to_string(),
                run_phase: "Event".to_string(),
                collector: "rv2".to_string(),
                peer_ip: "1.1.1.1".to_string(),
                prefix: prefix.to_string(),
                path_id: None,
                material_path_changed: kind == "PathReplacement",
                communities_changed: false,
                announced: kind == "Announcement",
                withdrawn: kind == "Withdrawal",
                observation_id: Some(oid),
                archive_sha256: None,
            },
        )
        .unwrap();
    }

    fn base_cs(conn: &Connection) -> (i64, i64) {
        let cs = CaseStudy {
            id: 0,
            slug: "incident-x".to_string(),
            title: "Incident X".to_string(),
            summary: "s".to_string(),
            start_utc: Some("2019-08-21T04:00:00Z".to_string()),
            end_utc: Some("2019-08-21T14:00:00Z".to_string()),
            status: "Active".to_string(),
            content_sha256: "abc".to_string(),
            created_utc: "2019-09-01T00:00:00Z".to_string(),
            updated_utc: "2019-09-01T00:00:00Z".to_string(),
        };
        let cs_id = store::insert_case_study(conn, &cs).unwrap();
        seed_phases(conn, cs_id);
        let doc: i64 = conn
            .query_row("SELECT id FROM reference_documents LIMIT 1", [], |r| {
                r.get(0)
            })
            .unwrap();
        (cs_id, doc)
    }

    #[test]
    fn comparison_preserves_operator_and_bgp_sources() {
        let (_dir, conn) = open_temp_db();
        let (cs_id, doc_id) = base_cs(&conn);
        let run_id = seed_run(&conn, "2019-08-21T02:00:00Z");
        seed_stream(&conn, run_id, "192.0.2.0/24");
        insert_t(
            &conn,
            run_id,
            0,
            "Withdrawal",
            "2019-08-21T05:00:00Z",
            "192.0.2.0/24",
            1,
        );
        store::insert_case_study_analysis_link(
            &conn,
            &CaseStudyAnalysisLink {
                id: 0,
                case_study_id: cs_id,
                run_id,
                role: "PrimaryObservation".to_string(),
                reviewed_note: None,
            },
        )
        .unwrap();
        seed_claim(
            &conn,
            cs_id,
            doc_id,
            CLAIM_TYPE_REPORTED_TIMELINE,
            OBSERVABILITY_POTENTIALLY_VISIBLE,
            Some("2019-08-21T05:00:00Z"),
        );
        let rows = build_comparison(&conn, cs_id).unwrap();
        assert_eq!(rows.len(), 1);
        let row = &rows[0];
        assert!(row
            .operator_report
            .contains("operator reports the condition"));
        assert!(row.operator_report.contains("reported, not measured"));
        assert!(row
            .bgp_observation
            .contains("route-state transitions observed"));
        assert!(row.bgp_observation.contains("2019-08-21T05:00:00Z"));
        assert_eq!(row.interpretation, RELATION_OVERLAPPING);
        assert_eq!(row.contributing_run_ids, vec![run_id]);
    }

    #[test]
    fn temporal_overlap_is_not_causal_confirmation() {
        let (_dir, conn) = open_temp_db();
        let (cs_id, doc_id) = base_cs(&conn);
        let run_id = seed_run(&conn, "2019-08-21T02:00:00Z");
        seed_stream(&conn, run_id, "192.0.2.0/24");
        insert_t(
            &conn,
            run_id,
            0,
            "Withdrawal",
            "2019-08-21T05:00:00Z",
            "192.0.2.0/24",
            1,
        );
        store::insert_case_study_analysis_link(
            &conn,
            &CaseStudyAnalysisLink {
                id: 0,
                case_study_id: cs_id,
                run_id,
                role: "PrimaryObservation".to_string(),
                reviewed_note: None,
            },
        )
        .unwrap();
        seed_claim(
            &conn,
            cs_id,
            doc_id,
            CLAIM_TYPE_REPORTED_MECHANISM,
            OBSERVABILITY_POTENTIALLY_VISIBLE,
            Some("2019-08-21T05:00:00Z"),
        );
        let rows = build_comparison(&conn, cs_id).unwrap();
        let row = &rows[0];
        assert_eq!(row.interpretation, RELATION_OVERLAPPING);
        assert!(row.limitation.contains("does not prove causation"));
        assert!(!row.limitation.contains("ConfirmedCause"));
        assert!(!row.interpretation.contains("ConfirmedCause"));
    }

    #[test]
    fn nonobservable_claim_has_no_false_negative() {
        let (_dir, conn) = open_temp_db();
        let (cs_id, doc_id) = base_cs(&conn);
        let run_id = seed_run(&conn, "2019-08-21T02:00:00Z");
        seed_stream(&conn, run_id, "192.0.2.0/24");
        // The run has NO transitions at all.
        store::insert_case_study_analysis_link(
            &conn,
            &CaseStudyAnalysisLink {
                id: 0,
                case_study_id: cs_id,
                run_id,
                role: "PrimaryObservation".to_string(),
                reviewed_note: None,
            },
        )
        .unwrap();
        seed_claim(
            &conn,
            cs_id,
            doc_id,
            CLAIM_TYPE_REPORTED_MECHANISM,
            OBSERVABILITY_NOT_DIRECTLY_VISIBLE,
            Some("2019-08-21T05:00:00Z"),
        );
        let rows = build_comparison(&conn, cs_id).unwrap();
        let row = &rows[0];
        assert_eq!(row.interpretation, RELATION_NOT_DIRECTLY_OBSERVABLE);
        assert!(row.bgp_observation.contains("not observable in public BGP"));
        assert!(!row.bgp_observation.to_lowercase().contains("no bgp change"));
    }

    #[test]
    fn comparison_can_show_no_observed_counterpart() {
        let (_dir, conn) = open_temp_db();
        let (cs_id, doc_id) = base_cs(&conn);
        let run_id = seed_run(&conn, "2019-08-21T02:00:00Z");
        seed_stream(&conn, run_id, "192.0.2.0/24");
        store::insert_case_study_analysis_link(
            &conn,
            &CaseStudyAnalysisLink {
                id: 0,
                case_study_id: cs_id,
                run_id,
                role: "PrimaryObservation".to_string(),
                reviewed_note: None,
            },
        )
        .unwrap();
        seed_claim(
            &conn,
            cs_id,
            doc_id,
            CLAIM_TYPE_REPORTED_TIMELINE,
            OBSERVABILITY_POTENTIALLY_VISIBLE,
            Some("2019-08-21T05:00:00Z"),
        );
        let rows = build_comparison(&conn, cs_id).unwrap();
        let row = &rows[0];
        assert_eq!(row.interpretation, RELATION_NO_OBSERVED_COUNTERPART);
        assert!(row.limitation.contains("does not refute"));
    }

    #[test]
    fn multiple_analysis_runs_can_contribute_to_one_phase() {
        let (_dir, conn) = open_temp_db();
        let (cs_id, doc_id) = base_cs(&conn);
        let run_a = seed_run(&conn, "2019-08-21T02:00:00Z");
        let run_b = seed_run(&conn, "2019-08-21T02:30:00Z");
        seed_stream(&conn, run_a, "192.0.2.0/24");
        seed_stream(&conn, run_b, "192.0.2.0/24");
        insert_t(
            &conn,
            run_a,
            0,
            "Withdrawal",
            "2019-08-21T05:00:00Z",
            "192.0.2.0/24",
            1,
        );
        insert_t(
            &conn,
            run_b,
            0,
            "PathReplacement",
            "2019-08-21T06:00:00Z",
            "192.0.2.0/24",
            2,
        );
        for run_id in [run_a, run_b] {
            store::insert_case_study_analysis_link(
                &conn,
                &CaseStudyAnalysisLink {
                    id: 0,
                    case_study_id: cs_id,
                    run_id,
                    role: "PrimaryObservation".to_string(),
                    reviewed_note: None,
                },
            )
            .unwrap();
        }
        seed_claim(
            &conn,
            cs_id,
            doc_id,
            CLAIM_TYPE_REPORTED_TIMELINE,
            OBSERVABILITY_POTENTIALLY_VISIBLE,
            Some("phase:0"),
        );
        let rows = build_comparison(&conn, cs_id).unwrap();
        let row = &rows[0];
        assert_eq!(row.interpretation, RELATION_OVERLAPPING);
        assert!(row.contributing_run_ids.contains(&run_a));
        assert!(row.contributing_run_ids.contains(&run_b));
        assert!(row
            .bgp_observation
            .contains("first at 2019-08-21T05:00:00Z"));
    }

    #[test]
    fn pilot_scope_does_not_fabricate_observation_for_uncovered_phases() {
        let (_dir, conn) = open_temp_db();
        let (cs_id, doc_id) = base_cs(&conn);
        // Run whose plan window covers only phase 1 (04:00-10:00).
        let e = store::upsert_event(&conn, "grnoc", "T1", "2019-08-22T00:00:00Z").unwrap();
        let sid = crate::catalog::tests::sample_snapshot(e, r#"{"title":"t"}"#);
        let sid = store::insert_snapshot(&conn, e, &sid).unwrap();
        let mid = store::insert_manifest_revision(
            &conn,
            &crate::catalog::tests::sample_manifest_revision(e, sid, r#"{"o":1}"#),
        )
        .unwrap();
        let payload = serde_json::json!({
            "schema_version": 1,
            "analysis_window": {
                "start": "2019-08-21T04:00:00Z",
                "end": "2019-08-21T10:00:00Z"
            }
        });
        let pid = store::insert_plan(
            &conn,
            &AnalysisPlanRecord {
                id: 0,
                manifest_revision_id: mid,
                plan_schema: 1,
                payload: payload.to_string(),
                sha256: crate::catalog::sync::hex_sha256(&payload.to_string()),
                status: "Ready".to_string(),
                block_reason: None,
                created_at: "2019-08-22T00:00:00Z".to_string(),
            },
        )
        .unwrap();
        let run_id = store::insert_run(
            &conn,
            &crate::catalog::tests::sample_run(pid, "2019-08-21T02:00:00Z"),
        )
        .unwrap();
        seed_stream(&conn, run_id, "192.0.2.0/24");
        insert_t(
            &conn,
            run_id,
            0,
            "Withdrawal",
            "2019-08-21T05:00:00Z",
            "192.0.2.0/24",
            1,
        );
        store::insert_case_study_analysis_link(
            &conn,
            &CaseStudyAnalysisLink {
                id: 0,
                case_study_id: cs_id,
                run_id,
                role: "PilotObservation".to_string(),
                reviewed_note: None,
            },
        )
        .unwrap();
        // Claim inside the covered phase -> Overlapping.
        seed_claim(
            &conn,
            cs_id,
            doc_id,
            CLAIM_TYPE_REPORTED_TIMELINE,
            OBSERVABILITY_POTENTIALLY_VISIBLE,
            Some("phase:0"),
        );
        // Claim in the uncovered phase -> Indeterminate, never a fabricated
        // NoObservedCounterpart from an out-of-scope run.
        seed_claim_sort(
            &conn,
            cs_id,
            doc_id,
            CLAIM_TYPE_REPORTED_TIMELINE,
            OBSERVABILITY_POTENTIALLY_VISIBLE,
            Some("phase:1"),
            1,
        );
        let rows = build_comparison(&conn, cs_id).unwrap();
        let covered = rows
            .iter()
            .find(|r| r.operator_time.as_deref() == Some("phase:0"))
            .unwrap();
        assert_eq!(covered.interpretation, RELATION_OVERLAPPING);
        let uncovered = rows
            .iter()
            .find(|r| r.operator_time.as_deref() == Some("phase:1"))
            .unwrap();
        assert_eq!(uncovered.interpretation, RELATION_INDETERMINATE);
        assert!(uncovered
            .bgp_observation
            .contains("no linked analysis run covers this window"));
        assert!(uncovered
            .limitation
            .contains("no observation was attempted"));
    }

    #[test]
    fn no_analysis_case_study_has_no_bgp_verdict() {
        let (_dir, conn) = open_temp_db();
        let (cs_id, doc_id) = base_cs(&conn);
        seed_claim(
            &conn,
            cs_id,
            doc_id,
            CLAIM_TYPE_REPORTED_TIMELINE,
            OBSERVABILITY_POTENTIALLY_VISIBLE,
            Some("2019-08-21T05:00:00Z"),
        );
        let rows = build_comparison(&conn, cs_id).unwrap();
        let row = &rows[0];
        assert_eq!(row.interpretation, RELATION_INDETERMINATE);
        assert!(row.bgp_observation.contains("not yet executed"));
        assert!(row.contributing_run_ids.is_empty());
    }
}
