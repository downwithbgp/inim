//! Phase-conditioned summaries over one complete AnalysisRun
//! .
//!
//! Read-only derivation: given one run and the reviewed case-study phases,
//! each phase is summarized from the run's stored transitions (imported
//! from `transitions.json`), stream lifecycle summaries, and semantic wave
//! summaries. Route reconstruction remains continuous across phase
//! boundaries — the run's state is never reset at a phase boundary, so a
//! lifecycle that crosses a boundary keeps its full shape. Counts are
//! observer-stream counts (distinct collector/peer/prefix streams), and
//! every transition belongs to at most one phase (by `occurred_utc`);
//! transitions outside every reviewed phase are reported explicitly.

use rusqlite::Connection;

use super::domain::CaseStudyPhase;

/// One phase-conditioned summary for a run.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PhaseSummary {
    pub phase_id: i64,
    pub label: String,
    pub start_utc: String,
    pub end_utc: String,
    /// Distinct observer streams visible (not absent) at phase start.
    pub active_streams_entering: usize,
    /// Distinct streams with an announcement transition in this phase.
    pub announcements: usize,
    /// Distinct streams with a withdrawal transition in this phase.
    pub withdrawals: usize,
    /// Distinct streams with a material path-replacement in this phase.
    pub path_changes: usize,
    /// Distinct streams whose reviewed lifecycle departed the transit,
    /// assigned to the phase of their last departure transition.
    pub transit_departures: usize,
    /// Distinct streams restored in this phase.
    pub restorations: usize,
    /// Semantic wave ids whose start falls in this phase.
    pub semantic_waves: Vec<String>,
    pub first_evidence_utc: Option<String>,
    pub last_evidence_utc: Option<String>,
    /// Evidence observation ids retained for this phase (sorted, unique).
    pub evidence_observation_ids: Vec<i64>,
}

/// Phase summaries for one run.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RunPhaseSummaries {
    pub run_id: i64,
    pub run_label: String,
    /// Transitions outside every reviewed phase (never silently dropped).
    pub outside_phases: usize,
    pub phases: Vec<PhaseSummary>,
}

/// A transition row lifted for summary computation.
struct TRow {
    kind: String,
    occurred_utc: String,
    stream: String,
    observation_id: Option<i64>,
}

/// Load a run's transitions in deterministic order.
fn run_transitions(conn: &Connection, run_id: i64) -> Result<Vec<TRow>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT kind, occurred_utc, collector, peer_ip, prefix, observation_id
             FROM run_transitions WHERE run_id = ?1 ORDER BY seq",
        )
        .map_err(|e| format!("catalog read failed: {e}"))?;
    let rows = stmt
        .query_map([run_id], |r| {
            Ok(TRow {
                kind: r.get(0)?,
                occurred_utc: r.get(1)?,
                stream: format!(
                    "{}|{}|{}",
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?
                ),
                observation_id: r.get(5)?,
            })
        })
        .map_err(|e| format!("catalog read failed: {e}"))?;
    rows.collect::<Result<_, _>>()
        .map_err(|e| format!("catalog read failed: {e}"))
}

fn list_phases(conn: &Connection, case_study_id: i64) -> Result<Vec<CaseStudyPhase>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, case_study_id, label, start_utc, end_utc, start_precision,
                    end_precision, description, source_document_id, source_page_or_section,
                    review_status, sort_order
             FROM case_study_phases WHERE case_study_id = ?1 ORDER BY sort_order",
        )
        .map_err(|e| format!("catalog read failed: {e}"))?;
    let rows = stmt
        .query_map([case_study_id], |r| {
            Ok(CaseStudyPhase {
                id: r.get(0)?,
                case_study_id: r.get(1)?,
                label: r.get(2)?,
                start_utc: r.get(3)?,
                end_utc: r.get(4)?,
                start_precision: r.get(5)?,
                end_precision: r.get(6)?,
                description: r.get(7)?,
                source_document_id: r.get(8)?,
                source_page_or_section: r.get(9)?,
                review_status: r.get(10)?,
                sort_order: r.get(11)?,
            })
        })
        .map_err(|e| format!("catalog read failed: {e}"))?;
    rows.collect::<Result<_, _>>()
        .map_err(|e| format!("catalog read failed: {e}"))
}

/// Load waves (id, start) for a run.
fn run_waves(conn: &Connection, run_id: i64) -> Result<Vec<(String, String)>, String> {
    let mut stmt = conn
        .prepare("SELECT wave_id, start FROM semantic_wave_summaries WHERE run_id = ?1")
        .map_err(|e| format!("catalog read failed: {e}"))?;
    let rows = stmt
        .query_map([run_id], |r| Ok((r.get(0)?, r.get(1)?)))
        .map_err(|e| format!("catalog read failed: {e}"))?;
    rows.collect::<Result<_, _>>()
        .map_err(|e| format!("catalog read failed: {e}"))
}

/// Summarize one complete run by the case study's reviewed phases.
///
/// Continuous semantics: stream visibility is walked across the whole run
/// (starting visible in baseline); a stream withdrawn before a phase start
/// and restored after it is not counted as active entering that phase, and
/// its withdrawal/restoration are counted in their own phases.
pub fn summarize_run(
    conn: &Connection,
    run_id: i64,
    case_study_id: i64,
) -> Result<RunPhaseSummaries, String> {
    let phases = list_phases(conn, case_study_id)?;
    let transitions = run_transitions(conn, run_id)?;
    let waves = run_waves(conn, run_id)?;
    let run_label: String = conn
        .query_row(
            "SELECT started_at FROM analysis_runs WHERE id = ?1",
            [run_id],
            |r| r.get(0),
        )
        .map_err(|e| format!("catalog read failed: {e}"))?;

    // Index transitions by phase (each transition belongs to at most one
    // phase by occurred_utc; boundaries are [start, end)).
    let phase_idx: Vec<Option<usize>> = transitions
        .iter()
        .map(|t| {
            phases
                .iter()
                .position(|p| p.start_utc <= t.occurred_utc && t.occurred_utc < p.end_utc)
        })
        .collect();
    let outside_phases = phase_idx.iter().filter(|i| i.is_none()).count();

    // Per-phase per-kind distinct-stream sets + evidence + first/last.
    let mut announcements: Vec<std::collections::BTreeSet<String>> =
        vec![std::collections::BTreeSet::new(); phases.len()];
    let mut withdrawals: Vec<std::collections::BTreeSet<String>> =
        vec![std::collections::BTreeSet::new(); phases.len()];
    let mut path_changes: Vec<std::collections::BTreeSet<String>> =
        vec![std::collections::BTreeSet::new(); phases.len()];
    let mut restorations: Vec<std::collections::BTreeSet<String>> =
        vec![std::collections::BTreeSet::new(); phases.len()];

    for (i, t) in transitions.iter().enumerate() {
        let Some(pi) = phase_idx[i] else { continue };
        match t.kind.as_str() {
            "Announcement" => {
                announcements[pi].insert(t.stream.clone());
            }
            "Withdrawal" => {
                withdrawals[pi].insert(t.stream.clone());
            }
            "PathReplacement" => {
                path_changes[pi].insert(t.stream.clone());
            }
            "Restoration" | "ReturnToBaseline" => {
                restorations[pi].insert(t.stream.clone());
            }
            _ => {}
        }
    }

    let mut summaries: Vec<PhaseSummary> = phases
        .iter()
        .enumerate()
        .map(|(pi, p)| PhaseSummary {
            phase_id: p.id,
            label: p.label.clone(),
            start_utc: p.start_utc.clone(),
            end_utc: p.end_utc.clone(),
            active_streams_entering: 0,
            announcements: announcements[pi].len(),
            withdrawals: withdrawals[pi].len(),
            path_changes: path_changes[pi].len(),
            transit_departures: 0,
            restorations: restorations[pi].len(),
            semantic_waves: Vec::new(),
            first_evidence_utc: None,
            last_evidence_utc: None,
            evidence_observation_ids: Vec::new(),
        })
        .collect();

    for (i, t) in transitions.iter().enumerate() {
        let Some(pi) = phase_idx[i] else { continue };
        let s = &mut summaries[pi];
        if s.first_evidence_utc.is_none() || s.first_evidence_utc.as_deref() > Some(&t.occurred_utc)
        {
            s.first_evidence_utc = Some(t.occurred_utc.clone());
        }
        if s.last_evidence_utc.as_deref() < Some(&t.occurred_utc) {
            s.last_evidence_utc = Some(t.occurred_utc.clone());
        }
        if let Some(oid) = t.observation_id {
            s.evidence_observation_ids.push(oid);
        }
    }
    for s in &mut summaries {
        s.evidence_observation_ids.sort_unstable();
        s.evidence_observation_ids.dedup();
    }

    // Active streams entering each phase: continuous visibility walk over
    // the whole run. Every observer stream starts visible in baseline; a
    // withdrawal hides a stream until it is restored, regardless of phase
    // boundaries.
    {
        let mut all_streams: std::collections::BTreeSet<String> = {
            let mut stmt = conn
                .prepare(
                    "SELECT collector, peer_ip, prefix FROM stream_lifecycle_summaries
                     WHERE run_id = ?1",
                )
                .map_err(|e| format!("catalog read failed: {e}"))?;
            let rows = stmt
                .query_map([run_id], |r| {
                    Ok(format!(
                        "{}|{}|{}",
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?
                    ))
                })
                .map_err(|e| format!("catalog read failed: {e}"))?;
            rows.collect::<Result<_, _>>()
                .map_err(|e| format!("catalog read failed: {e}"))?
        };
        // Also include any transition stream not in the lifecycle table.
        for t in &transitions {
            all_streams.insert(t.stream.clone());
        }
        let mut visible: std::collections::BTreeSet<String> = all_streams;
        let mut sorted: Vec<&TRow> = transitions.iter().collect();
        sorted.sort_by(|a, b| a.occurred_utc.cmp(&b.occurred_utc));
        let mut pos = 0usize;
        for (pi, p) in phases.iter().enumerate() {
            while pos < sorted.len() && sorted[pos].occurred_utc < p.start_utc {
                let t = sorted[pos];
                match t.kind.as_str() {
                    "Withdrawal" => {
                        visible.remove(&t.stream);
                    }
                    "Announcement" | "Restoration" | "ReturnToBaseline" => {
                        visible.insert(t.stream.clone());
                    }
                    _ => {}
                }
                pos += 1;
            }
            summaries[pi].active_streams_entering = visible.len();
        }
    }

    // Semantic waves by phase start.
    for (wave_id, start) in waves {
        if let Some(pi) = phases
            .iter()
            .position(|p| p.start_utc <= start && start < p.end_utc)
        {
            summaries[pi].semantic_waves.push(wave_id);
        }
    }

    Ok(RunPhaseSummaries {
        run_id,
        run_label,
        outside_phases,
        phases: summaries,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::db;
    use crate::catalog::domain::*;
    use crate::catalog::store;

    fn open_temp_db() -> (tempfile::TempDir, Connection) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("catalog.sqlite");
        let conn = db::open_catalog(&path).unwrap();
        (dir, conn)
    }

    fn seed_run(conn: &Connection) -> i64 {
        let e = store::upsert_event(conn, "grnoc", "T1", "2019-08-22T00:00:00Z").unwrap();
        let sid = store::insert_snapshot(
            conn,
            e,
            &crate::catalog::tests::sample_snapshot(e, r#"{"title":"t"}"#),
        )
        .unwrap();
        let mid = store::insert_manifest_revision(
            conn,
            &crate::catalog::tests::sample_manifest_revision(e, sid, r#"{"o":1}"#),
        )
        .unwrap();
        let pid =
            store::insert_plan(conn, &crate::catalog::tests::sample_plan(mid, "Ready")).unwrap();
        store::insert_run(
            conn,
            &crate::catalog::tests::sample_run(pid, "2019-08-21T02:00:00Z"),
        )
        .unwrap()
    }

    fn seed_case_study(conn: &Connection) -> i64 {
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
            interconnection_context: None,
        };
        let cs_id = store::insert_case_study(conn, &cs).unwrap();
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
        cs_id
    }

    fn seed_stream(
        conn: &Connection,
        run_id: i64,
        collector: &str,
        peer: &str,
        prefix: &str,
        category: &str,
    ) {
        store::insert_streams(
            conn,
            run_id,
            &[StreamLifecycleSummary {
                id: 0,
                run_id,
                collector: collector.to_string(),
                peer_ip: peer.to_string(),
                prefix: prefix.to_string(),
                category: category.to_string(),
                baseline_instances: 1,
                max_active_instances: 1,
                transition_count: 0,
                withdrawn: false,
                restored: false,
                transit_state: "Retained".to_string(),
                add_path_ambiguous: false,
                evidence_refs: "[]".to_string(),
                first_change_utc: None,
                restoration_time_utc: None,
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
        stream: &str,
        oid: Option<i64>,
    ) {
        let parts: Vec<&str> = stream.split('|').collect();
        store::insert_run_transition(
            conn,
            &RunTransitionRecord {
                id: 0,
                run_id,
                seq,
                kind: kind.to_string(),
                occurred_utc: at.to_string(),
                run_phase: "Event".to_string(),
                collector: parts[0].to_string(),
                peer_ip: parts[1].to_string(),
                prefix: parts[2].to_string(),
                path_id: None,
                material_path_changed: kind == "PathReplacement",
                communities_changed: false,
                announced: kind == "Announcement",
                withdrawn: kind == "Withdrawal",
                observation_id: oid,
                archive_sha256: None,
            },
        )
        .unwrap();
    }

    #[test]
    fn phase_summary_does_not_require_duplicate_full_evidence() {
        let (_dir, conn) = open_temp_db();
        // The run_transitions index stores only summary/evidence-lookup
        // fields — full before/after route states stay in the immutable
        // artifacts and are never duplicated in SQLite.
        let cols: Vec<String> = conn
            .prepare("PRAGMA table_info(run_transitions)")
            .unwrap()
            .query_map([], |r| r.get(1))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        for forbidden in [
            "before_path",
            "after_path",
            "from_path",
            "to_path",
            "raw_payload",
            "evidence_json",
        ] {
            assert!(
                !cols.iter().any(|c| c.contains(forbidden)),
                "index must not duplicate full evidence: {forbidden}"
            );
        }
        for required in [
            "run_id",
            "seq",
            "kind",
            "occurred_utc",
            "collector",
            "peer_ip",
            "prefix",
            "observation_id",
        ] {
            assert!(
                cols.iter().any(|c| c == required),
                "index must keep {required}"
            );
        }
    }

    #[test]
    fn phase_summary_uses_continuous_run_state() {
        let (_dir, conn) = open_temp_db();
        let cs_id = seed_case_study(&conn);
        let run_id = seed_run(&conn);
        seed_stream(&conn, run_id, "rv2", "1.1.1.1", "192.0.2.0/24", "Unchanged");
        seed_stream(
            &conn,
            run_id,
            "rv2",
            "2.2.2.2",
            "198.51.100.0/24",
            "Unchanged",
        );
        // S2 withdrawn at 09:00, restored at 11:00 (crosses the 10:00 boundary).
        insert_t(
            &conn,
            run_id,
            0,
            "Withdrawal",
            "2019-08-21T09:00:00Z",
            "rv2|2.2.2.2|198.51.100.0/24",
            Some(1),
        );
        insert_t(
            &conn,
            run_id,
            1,
            "Restoration",
            "2019-08-21T11:00:00Z",
            "rv2|2.2.2.2|198.51.100.0/24",
            Some(2),
        );
        let summary = summarize_run(&conn, run_id, cs_id).unwrap();
        assert_eq!(summary.outside_phases, 0);
        let p1 = &summary.phases[0];
        assert_eq!(p1.label, "first");
        assert_eq!(p1.active_streams_entering, 2);
        assert_eq!(p1.withdrawals, 1);
        assert_eq!(p1.restorations, 0);
        let p2 = &summary.phases[1];
        // The stream is still absent at 10:00 (continuous state, no reset).
        assert_eq!(p2.active_streams_entering, 1);
        assert_eq!(p2.restorations, 1);
        assert_eq!(p2.withdrawals, 0);
    }

    #[test]
    fn lifecycle_crossing_phase_boundary_is_not_split_incorrectly() {
        let (_dir, conn) = open_temp_db();
        let cs_id = seed_case_study(&conn);
        let run_id = seed_run(&conn);
        seed_stream(
            &conn,
            run_id,
            "rv2",
            "2.2.2.2",
            "198.51.100.0/24",
            "Unchanged",
        );
        insert_t(
            &conn,
            run_id,
            0,
            "Withdrawal",
            "2019-08-21T09:00:00Z",
            "rv2|2.2.2.2|198.51.100.0/24",
            Some(1),
        );
        insert_t(
            &conn,
            run_id,
            1,
            "Restoration",
            "2019-08-21T11:00:00Z",
            "rv2|2.2.2.2|198.51.100.0/24",
            Some(2),
        );
        let summary = summarize_run(&conn, run_id, cs_id).unwrap();
        // One continuous lifecycle: exactly one withdrawal and one
        // restoration across the whole run, counted once each.
        let total_withdrawals: usize = summary.phases.iter().map(|p| p.withdrawals).sum();
        let total_restorations: usize = summary.phases.iter().map(|p| p.restorations).sum();
        assert_eq!(total_withdrawals, 1);
        assert_eq!(total_restorations, 1);
    }

    #[test]
    fn phase_counts_are_observer_stream_counts() {
        let (_dir, conn) = open_temp_db();
        let cs_id = seed_case_study(&conn);
        let run_id = seed_run(&conn);
        seed_stream(&conn, run_id, "rv2", "1.1.1.1", "192.0.2.0/24", "Unchanged");
        seed_stream(
            &conn,
            run_id,
            "rv2",
            "2.2.2.2",
            "198.51.100.0/24",
            "Unchanged",
        );
        // Two withdrawal transitions for the SAME observer stream (ADD-PATH
        // instances) plus one for another stream → stream count is 2.
        insert_t(
            &conn,
            run_id,
            0,
            "Withdrawal",
            "2019-08-21T05:00:00Z",
            "rv2|1.1.1.1|192.0.2.0/24",
            Some(1),
        );
        insert_t(
            &conn,
            run_id,
            1,
            "Withdrawal",
            "2019-08-21T05:05:00Z",
            "rv2|1.1.1.1|192.0.2.0/24",
            Some(2),
        );
        insert_t(
            &conn,
            run_id,
            2,
            "Withdrawal",
            "2019-08-21T05:10:00Z",
            "rv2|2.2.2.2|198.51.100.0/24",
            Some(3),
        );
        let summary = summarize_run(&conn, run_id, cs_id).unwrap();
        assert_eq!(
            summary.phases[0].withdrawals, 2,
            "distinct observer streams, not transitions"
        );
    }

    #[test]
    fn same_transition_is_not_double_counted() {
        let (_dir, conn) = open_temp_db();
        let cs_id = seed_case_study(&conn);
        let run_id = seed_run(&conn);
        seed_stream(&conn, run_id, "rv2", "1.1.1.1", "192.0.2.0/24", "Unchanged");
        // A transition exactly on the 10:00 boundary belongs to the second
        // phase only ([start, end) semantics).
        insert_t(
            &conn,
            run_id,
            0,
            "Withdrawal",
            "2019-08-21T10:00:00Z",
            "rv2|1.1.1.1|192.0.2.0/24",
            Some(1),
        );
        let summary = summarize_run(&conn, run_id, cs_id).unwrap();
        assert_eq!(summary.phases[0].withdrawals, 0);
        assert_eq!(summary.phases[1].withdrawals, 1);
        let total: usize = summary.phases.iter().map(|p| p.withdrawals).sum();
        assert_eq!(total, 1, "the transition is counted exactly once");
        assert_eq!(summary.outside_phases, 0);
    }

    #[test]
    fn phase_without_bgp_changes_remains_valid() {
        let (_dir, conn) = open_temp_db();
        let cs_id = seed_case_study(&conn);
        let run_id = seed_run(&conn);
        seed_stream(&conn, run_id, "rv2", "1.1.1.1", "192.0.2.0/24", "Unchanged");
        // No transitions at all.
        let summary = summarize_run(&conn, run_id, cs_id).unwrap();
        for p in &summary.phases {
            assert_eq!(p.active_streams_entering, 1);
            assert_eq!(p.announcements, 0);
            assert_eq!(p.withdrawals, 0);
            assert_eq!(p.path_changes, 0);
            assert_eq!(p.restorations, 0);
            assert!(p.first_evidence_utc.is_none());
            assert!(p.last_evidence_utc.is_none());
        }
        assert_eq!(summary.outside_phases, 0);
    }

    #[test]
    fn inherited_impairment_is_not_counted_as_new_phase_transition() {
        let (_dir, conn) = open_temp_db();
        let cs_id = seed_case_study(&conn);
        let run_id = seed_run(&conn);
        seed_stream(&conn, run_id, "rv2", "1.1.1.1", "192.0.2.0/24", "Unchanged");
        // Withdrawn in phase 1 (09:00); still absent through phase 2.
        insert_t(
            &conn,
            run_id,
            0,
            "Withdrawal",
            "2019-08-21T09:00:00Z",
            "rv2|1.1.1.1|192.0.2.0/24",
            Some(1),
        );
        let summary = summarize_run(&conn, run_id, cs_id).unwrap();
        let p2 = &summary.phases[1];
        // The impairment is inherited: not a new phase-2 transition, and the
        // stream is not active entering phase 2.
        assert_eq!(
            p2.withdrawals, 0,
            "inherited impairment is not a new transition"
        );
        assert_eq!(p2.active_streams_entering, 0);
        assert!(p2.first_evidence_utc.is_none());
    }

    #[test]
    fn restoration_phase_can_close_prior_phase_lifecycle() {
        let (_dir, conn) = open_temp_db();
        let cs_id = seed_case_study(&conn);
        let run_id = seed_run(&conn);
        seed_stream(&conn, run_id, "rv2", "1.1.1.1", "192.0.2.0/24", "Unchanged");
        // Lifecycle: withdrawal in phase 1, restoration in phase 2 — the
        // restoration CLOSES the phase-1 lifecycle (one lifecycle total).
        insert_t(
            &conn,
            run_id,
            0,
            "Withdrawal",
            "2019-08-21T09:00:00Z",
            "rv2|1.1.1.1|192.0.2.0/24",
            Some(1),
        );
        insert_t(
            &conn,
            run_id,
            1,
            "Restoration",
            "2019-08-21T11:00:00Z",
            "rv2|1.1.1.1|192.0.2.0/24",
            Some(2),
        );
        let summary = summarize_run(&conn, run_id, cs_id).unwrap();
        assert_eq!(summary.phases[0].withdrawals, 1);
        assert_eq!(summary.phases[1].restorations, 1);
        let total_lifecycle_events: usize = summary
            .phases
            .iter()
            .map(|p| p.withdrawals + p.restorations + p.announcements + p.path_changes)
            .sum();
        assert_eq!(total_lifecycle_events, 2, "one lifecycle, two transitions");
    }

    #[test]
    fn phase_boundary_does_not_reset_baseline() {
        let (_dir, conn) = open_temp_db();
        let cs_id = seed_case_study(&conn);
        let run_id = seed_run(&conn);
        seed_stream(&conn, run_id, "rv2", "1.1.1.1", "192.0.2.0/24", "Unchanged");
        seed_stream(
            &conn,
            run_id,
            "rv2",
            "2.2.2.2",
            "198.51.100.0/24",
            "Unchanged",
        );
        // No transitions at all: both streams stay active in every phase —
        // the baseline is never reset and no phase invents changes.
        let summary = summarize_run(&conn, run_id, cs_id).unwrap();
        for p in &summary.phases {
            assert_eq!(p.active_streams_entering, 2);
            assert_eq!(
                p.withdrawals + p.restorations + p.announcements + p.path_changes,
                0
            );
        }
    }

    #[test]
    fn one_lifecycle_can_span_multiple_phases() {
        let (_dir, conn) = open_temp_db();
        let cs_id = seed_case_study(&conn);
        let run_id = seed_run(&conn);
        seed_stream(&conn, run_id, "rv2", "1.1.1.1", "192.0.2.0/24", "Unchanged");
        // Withdrawn 09:00 (phase 1), restored 12:00 (phase 2): the same
        // lifecycle spans both phases without being split or duplicated.
        insert_t(
            &conn,
            run_id,
            0,
            "Withdrawal",
            "2019-08-21T09:00:00Z",
            "rv2|1.1.1.1|192.0.2.0/24",
            Some(1),
        );
        insert_t(
            &conn,
            run_id,
            1,
            "ReturnToBaseline",
            "2019-08-21T12:00:00Z",
            "rv2|1.1.1.1|192.0.2.0/24",
            Some(2),
        );
        let summary = summarize_run(&conn, run_id, cs_id).unwrap();
        assert_eq!(summary.phases[0].withdrawals, 1);
        assert_eq!(summary.phases[1].restorations, 1);
        // The restored stream is active again in any later phase.
        let total: usize = summary
            .phases
            .iter()
            .map(|p| p.withdrawals + p.restorations)
            .sum();
        assert_eq!(total, 2);
    }

    #[test]
    fn phase_summary_retains_evidence_links() {
        let (_dir, conn) = open_temp_db();
        let cs_id = seed_case_study(&conn);
        let run_id = seed_run(&conn);
        seed_stream(&conn, run_id, "rv2", "1.1.1.1", "192.0.2.0/24", "Unchanged");
        insert_t(
            &conn,
            run_id,
            0,
            "Withdrawal",
            "2019-08-21T05:00:00Z",
            "rv2|1.1.1.1|192.0.2.0/24",
            Some(101),
        );
        insert_t(
            &conn,
            run_id,
            1,
            "Announcement",
            "2019-08-21T05:10:00Z",
            "rv2|1.1.1.1|192.0.2.0/24",
            Some(101),
        );
        insert_t(
            &conn,
            run_id,
            2,
            "PathReplacement",
            "2019-08-21T05:20:00Z",
            "rv2|1.1.1.1|192.0.2.0/24",
            Some(102),
        );
        let summary = summarize_run(&conn, run_id, cs_id).unwrap();
        let p1 = &summary.phases[0];
        assert_eq!(p1.evidence_observation_ids, vec![101, 102]);
        assert_eq!(p1.path_changes, 1);
        assert_eq!(
            p1.first_evidence_utc.as_deref(),
            Some("2019-08-21T05:00:00Z")
        );
        assert_eq!(
            p1.last_evidence_utc.as_deref(),
            Some("2019-08-21T05:20:00Z")
        );
    }
}
