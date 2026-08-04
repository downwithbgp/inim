//! Cross-observer comparison over independent AnalysisRuns
//! .
//!
//! Each run keeps its own evidence; this layer only COMPARES per
//! normalized prefix across collectors. It never merges evidence and
//! never produces a merged verdict. Permitted cross-observer statements:
//!
//! - "Observed at multiple independent public collectors"
//! - "Observed only at one selected collector"
//! - "Similar route-state change with different timing"
//! - "No counterpart at this observer"
//! - "Insufficient baseline visibility"
//!
//! Forbidden: "globally confirmed", "complete outage", "traffic loss
//! confirmed", "operator action confirmed". A prefix may have different
//! observer availability; absence of baseline visibility is never
//! counted as absence of event impact.

use rusqlite::Connection;

/// One comparison row: a normalized prefix at one collector.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ObserverComparisonRow {
    pub prefix: String,
    pub collector: String,
    /// Source family label ("RouteViews" | "RIPE RIS").
    pub family: String,
    pub peer: String,
    /// First observed route-state change in the event window.
    pub first_change_utc: Option<String>,
    /// Temporary observer-stream absence interval (withdrawal ->
    /// restoration), when present.
    pub temporary_absence: Option<String>,
    pub path_replacement: bool,
    pub transit_departure: bool,
    pub restoration_utc: Option<String>,
    /// Whether a baseline observer-prefix stream exists at this
    /// collector (evidence availability, distinct from no-change).
    pub baseline_visibility: bool,
}

/// Cross-observer statement for one normalized prefix.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PrefixStatement {
    pub prefix: String,
    /// Collectors with baseline visibility.
    pub visible_at: Vec<String>,
    /// Collectors with observed change.
    pub changed_at: Vec<String>,
    /// Cross-observer statement (allowed vocabulary only).
    pub statement: String,
    /// Timing detail when several collectors changed with different
    /// timestamps.
    pub timing_note: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ObserverComparison {
    pub rows: Vec<ObserverComparisonRow>,
    pub statements: Vec<PrefixStatement>,
}

/// Family label for a run: read from its manifest revision payload
/// (`source_family`, default RouteViews for pre-Session-34 manifests).
fn run_family(conn: &Connection, run_id: i64) -> String {
    let fam: Option<String> = conn
        .query_row(
            "SELECT m.payload FROM manifest_revisions m
             JOIN analysis_plans p ON p.manifest_revision_id = m.id
             JOIN analysis_runs r ON r.plan_id = p.id
             WHERE r.id = ?1",
            [run_id],
            |r| r.get::<_, String>(0),
        )
        .ok()
        .and_then(|payload| {
            serde_json::from_str::<serde_json::Value>(&payload)
                .ok()
                .and_then(|v| {
                    v.get("source_family")
                        .and_then(|x| x.as_str())
                        .map(String::from)
                })
        });
    match fam.as_deref() {
        Some("RipeRis") => "RIPE RIS".to_string(),
        _ => "RouteViews".to_string(),
    }
}

/// Linked runs of a case study (deterministic order).
fn linked_runs(conn: &Connection, case_study_id: i64) -> Result<Vec<i64>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT case_study_analysis_links.run_id FROM case_study_analysis_links
             WHERE case_study_id = ?1 AND case_study_analysis_links.run_id IS NOT NULL
             ORDER BY case_study_analysis_links.run_id",
        )
        .map_err(|e| format!("catalog read failed: {e}"))?;
    let rows = stmt
        .query_map([case_study_id], |r| r.get::<_, i64>(0))
        .map_err(|e| format!("catalog read failed: {e}"))?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r.map_err(|e| format!("catalog read failed: {e}"))?);
    }
    Ok(out)
}

/// One stream summary row: (collector, peer, prefix, withdrawn,
/// restored, transit_state).
type StreamRow = (String, String, String, i64, i64, String);

/// Stream summaries of one run, keyed by (collector, prefix).
fn run_streams(conn: &Connection, run_id: i64) -> Result<Vec<StreamRow>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT collector, peer_ip, prefix, withdrawn, restored, transit_state
             FROM stream_lifecycle_summaries WHERE run_id = ?1
             ORDER BY prefix, collector, peer_ip",
        )
        .map_err(|e| format!("catalog read failed: {e}"))?;
    let rows = stmt
        .query_map([run_id], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, i64>(4)?,
                r.get::<_, String>(5)?,
            ))
        })
        .map_err(|e| format!("catalog read failed: {e}"))?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r.map_err(|e| format!("catalog read failed: {e}"))?);
    }
    Ok(out)
}

/// Transitions of one run: (kind, occurred_utc, peer, prefix).
fn run_transitions(
    conn: &Connection,
    run_id: i64,
) -> Result<Vec<(String, String, String, String)>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT kind, occurred_utc, peer_ip, prefix FROM run_transitions
             WHERE run_id = ?1 ORDER BY seq",
        )
        .map_err(|e| format!("catalog read failed: {e}"))?;
    let rows = stmt
        .query_map([run_id], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
            ))
        })
        .map_err(|e| format!("catalog read failed: {e}"))?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r.map_err(|e| format!("catalog read failed: {e}"))?);
    }
    Ok(out)
}

fn is_withdrawal(kind: &str) -> bool {
    kind.contains("Withdrawal") || kind.contains("withdrawal")
}

fn is_restoration(kind: &str) -> bool {
    kind.contains("ReturnToBaseline")
        || kind.contains("restoration")
        || kind.contains("Restoration")
        || kind == "Restored"
}

fn is_path_replacement(kind: &str) -> bool {
    kind.contains("PathReplacement") || kind.contains("path-replacement")
}

fn is_transit_departure(kind: &str) -> bool {
    kind.contains("Departure") || kind.contains("departure") || kind.contains("LeftTransit")
}

/// Build the per-prefix × per-collector comparison over the case study's
/// linked runs. Deterministic: rows sorted by (prefix, collector, peer).
pub fn build_observer_comparison(
    conn: &Connection,
    case_study_id: i64,
) -> Result<ObserverComparison, String> {
    let runs = linked_runs(conn, case_study_id)?;
    let mut rows: Vec<ObserverComparisonRow> = Vec::new();
    // Map (prefix, collector) -> statement inputs.
    let mut visible: std::collections::BTreeMap<(String, String), Vec<String>> =
        std::collections::BTreeMap::new();
    let mut changed: std::collections::BTreeMap<(String, String), Vec<String>> =
        std::collections::BTreeMap::new();
    let mut first_change: std::collections::BTreeMap<(String, String), String> =
        std::collections::BTreeMap::new();

    for run_id in &runs {
        let family = run_family(conn, *run_id);
        let streams = run_streams(conn, *run_id)?;
        // One row per (collector, prefix, peer) with baseline visibility.
        let mut per_collector_prefix: std::collections::BTreeMap<(String, String), usize> =
            std::collections::BTreeMap::new();
        for (collector, peer, prefix, withdrawn, restored, transit_state) in &streams {
            let (collector, peer, prefix) = (collector.clone(), peer.clone(), prefix.clone());
            let restored_utc = if *restored > 0 {
                Some("event window".to_string())
            } else {
                None
            };
            rows.push(ObserverComparisonRow {
                prefix: prefix.clone(),
                collector: collector.clone(),
                family: family.clone(),
                peer,
                first_change_utc: None,
                temporary_absence: if *withdrawn > 0 && *restored > 0 {
                    Some("withdrawn then restored (event window)".to_string())
                } else {
                    None
                },
                path_replacement: false,
                transit_departure: *withdrawn > 0 || transit_state == "DepartedTransit",
                restoration_utc: restored_utc,
                baseline_visibility: true,
            });
            *per_collector_prefix
                .entry((collector.clone(), prefix.clone()))
                .or_insert(0) += 1;
        }
        let transitions = run_transitions(conn, *run_id)?;
        for (kind, occurred, peer, prefix) in &transitions {
            for (collector, _) in per_collector_prefix.keys() {
                let key = (collector.clone(), prefix.clone());
                // Update the matching row(s): first change, absence,
                // path replacement, transit departure.
                for row in rows.iter_mut().rev() {
                    if row.prefix != *prefix || &row.collector != collector {
                        continue;
                    }
                    if is_withdrawal(kind) && row.temporary_absence.is_none() {
                        row.temporary_absence = Some(format!("withdrawn at {occurred}"));
                    }
                    if is_restoration(kind) {
                        row.restoration_utc = Some(occurred.clone());
                    }
                    if is_path_replacement(kind) {
                        row.path_replacement = true;
                    }
                    if is_transit_departure(kind) {
                        row.transit_departure = true;
                    }
                }
                let f = first_change
                    .entry(key.clone())
                    .or_insert_with(|| occurred.clone());
                if occurred < f {
                    *f = occurred.clone();
                }
                visible.entry(key.clone()).or_default().push(peer.clone());
                changed.entry(key).or_default().push(peer.clone());
            }
        }
        // Fold first-change timestamps into the rows.
        for row in rows.iter_mut() {
            if let Some(ts) = first_change.get(&(row.collector.clone(), row.prefix.clone())) {
                row.first_change_utc = Some(ts.clone());
            }
        }
    }

    // Deterministic ordering: (prefix, collector, peer).
    rows.sort_by(|a, b| {
        (&a.prefix, &a.collector, &a.peer, &a.family).cmp(&(
            &b.prefix,
            &b.collector,
            &b.peer,
            &b.family,
        ))
    });

    // Cross-observer statements per prefix.
    let mut by_prefix: std::collections::BTreeMap<String, Vec<&ObserverComparisonRow>> =
        std::collections::BTreeMap::new();
    for r in &rows {
        by_prefix.entry(r.prefix.clone()).or_default().push(r);
    }
    let mut statements = Vec::new();
    for (prefix, group) in by_prefix {
        let visible_cols: Vec<String> = group
            .iter()
            .filter(|r| r.baseline_visibility)
            .map(|r| r.collector.clone())
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();
        let changed_cols: Vec<String> = group
            .iter()
            .filter(|r| r.first_change_utc.is_some() || r.temporary_absence.is_some())
            .map(|r| r.collector.clone())
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();
        let (statement, timing_note) = match changed_cols.len() {
            0 if visible_cols.is_empty() => (
                "Insufficient baseline visibility".to_string(),
                "no selected observer had a baseline stream for this prefix".to_string(),
            ),
            0 => (
                "No counterpart at this observer".to_string(),
                "baseline streams exist but no route-state change was observed".to_string(),
            ),
            1 => (
                "Observed only at one selected collector".to_string(),
                format!("changed at {}", changed_cols.join(", ")),
            ),
            n => {
                // Multiple collectors: different timings preserved.
                let mut times: Vec<(&str, &str)> = group
                    .iter()
                    .filter(|r| r.first_change_utc.is_some())
                    .map(|r| {
                        (
                            r.collector.as_str(),
                            r.first_change_utc.as_deref().unwrap_or(""),
                        )
                    })
                    .collect();
                times.sort_by_key(|(c, t)| (*t, *c));
                let all_same = times.windows(2).all(|w| w[0].1 == w[1].1);
                let note = if all_same {
                    format!("first observed change at {} on {}", times[0].1, times[0].0)
                } else {
                    let detail: Vec<String> =
                        times.iter().map(|(c, t)| format!("{c} at {t}")).collect();
                    format!(
                        "similar route-state change with different timing: {}",
                        detail.join(", ")
                    )
                };
                (
                    format!("Observed at {n} independent public collectors"),
                    note,
                )
            }
        };
        statements.push(PrefixStatement {
            prefix,
            visible_at: visible_cols,
            changed_at: changed_cols,
            statement,
            timing_note,
        });
    }
    statements.sort_by(|a, b| a.prefix.cmp(&b.prefix));

    Ok(ObserverComparison { rows, statements })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::db;
    use crate::catalog::domain::*;
    use crate::catalog::store;

    fn open_temp_db() -> (tempfile::TempDir, rusqlite::Connection) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("catalog.sqlite");
        let conn = db::open_catalog(&path).unwrap();
        (dir, conn)
    }

    /// Seed a case study + event + manifest revision + plan + run with
    /// streams and transitions, mirroring a real import.
    fn seed_run(
        conn: &rusqlite::Connection,
        run_id: i64,
        family: &str,
        streams: Vec<(&str, &str, &str, i64, i64, &str)>,
        transitions: Vec<(&str, &str, &str, &str)>, // (kind, utc, peer, prefix)
    ) {
        let cs = CaseStudy {
            id: 1,
            slug: "manlan-2019".to_string(),
            title: "t".to_string(),
            summary: "s".to_string(),
            start_utc: Some("2019-08-21T04:00:00Z".to_string()),
            end_utc: Some("2019-08-21T22:38:00Z".to_string()),
            status: "Active".to_string(),
            content_sha256: "abc".to_string(),
            created_utc: "2026-08-01T00:00:00Z".to_string(),
            updated_utc: "2026-08-01T00:00:00Z".to_string(),
            interconnection_context: None,
        };
        store::insert_case_study(conn, &cs).unwrap();
        let event_id =
            store::upsert_event(conn, "local-repository", "EVENT1", "2026-08-01T00:00:00Z")
                .unwrap();
        let snapshot = EventSnapshot {
            id: 0,
            event_id,
            fetched_at: "2026-08-01T00:00:00Z".to_string(),
            source_url: "https://example.invalid/e1".to_string(),
            content_sha256: "sha1".to_string(),
            raw_payload: "{}".to_string(),
            normalized_json: "{}".to_string(),
            parser_version: "test".to_string(),
        };
        let snap_id = store::insert_snapshot(conn, event_id, &snapshot).unwrap();
        let manifest = ManifestRevision {
            id: 0,
            event_id,
            snapshot_id: snap_id,
            manifest_schema: 2,
            payload: format!(
                r#"{{"event_id":"EVENT1","source_family":"{family}","collectors":[]}}"#
            ),
            sha256: format!("msha{run_id}"),
            review_status: "Reviewed".to_string(),
            reviewed_at: Some("2026-08-01T00:00:00Z".to_string()),
            reviewer: Some("test".to_string()),
        };
        let man_id = store::insert_manifest_revision(conn, &manifest).unwrap();
        let plan = AnalysisPlanRecord {
            id: 0,
            manifest_revision_id: man_id,
            plan_schema: 1,
            payload: "{}".to_string(),
            sha256: format!("psha{run_id}"),
            status: "Ready".to_string(),
            block_reason: None,
            created_at: "2026-08-01T00:00:00Z".to_string(),
        };
        let plan_id = store::insert_plan(conn, &plan).unwrap();
        let run = AnalysisRun {
            id: run_id,
            plan_id,
            software_version: "test".to_string(),
            git_revision: None,
            parser_identity: "test".to_string(),
            cache_schema_version: 1,
            report_schema_version: 1,
            status: "Complete".to_string(),
            started_at: "2026-08-01T00:00:00Z".to_string(),
            completed_at: Some("2026-08-01T00:00:01Z".to_string()),
            runtime_secs: Some(1.0),
            verdict: Some("ExpectedLossOfReachability".to_string()),
            assessment: Some("test".to_string()),
        };
        let run_id = store::insert_run(conn, &run).unwrap();
        store::insert_case_study_analysis_link(
            conn,
            &CaseStudyAnalysisLink {
                id: 0,
                case_study_id: 1,
                run_id,
                role: "PilotObservation".to_string(),
                reviewed_note: None,
            },
        )
        .unwrap();
        for (collector, peer, prefix, withdrawn, restored, transit_state) in streams {
            conn.execute(
                "INSERT INTO stream_lifecycle_summaries
                   (run_id, collector, peer_ip, prefix, category, baseline_instances,
                    max_active_instances, transition_count, withdrawn, restored,
                    transit_state, add_path_ambiguous, evidence_refs)
                 VALUES (?1,?2,?3,?4,'Changed',1,1,0,?5,?6,?7,0,'[]')",
                rusqlite::params![
                    run_id,
                    collector,
                    peer,
                    prefix,
                    withdrawn,
                    restored,
                    transit_state
                ],
            )
            .unwrap();
        }
        for (seq, (kind, utc, peer, prefix)) in transitions.iter().enumerate() {
            conn.execute(
                "INSERT INTO run_transitions
                   (run_id, seq, kind, occurred_utc, run_phase, collector, peer_ip,
                    prefix, path_id, material_path_changed, communities_changed,
                    announced, withdrawn, observation_id, archive_sha256)
                 VALUES (?1,?2,?3,?4,'Event','c',?5,?6,NULL,0,0,0,0,NULL,NULL)",
                rusqlite::params![run_id, seq as i64, kind, utc, peer, prefix],
            )
            .unwrap();
        }
    }

    #[test]
    fn comparison_distinguishes_no_visibility_from_no_change() {
        let (_dir, conn) = open_temp_db();
        // Prefix A: baseline stream but NO transitions (no change).
        seed_run(
            &conn,
            1,
            "RouteViews",
            vec![(
                "route-views2",
                "64.57.28.241",
                "2001:468:201::/48",
                0,
                0,
                "Stable",
            )],
            vec![],
        );
        // Prefix B: no stream at all (no baseline visibility).
        let comparison = build_observer_comparison(&conn, 1).unwrap();
        let a = comparison
            .statements
            .iter()
            .find(|s| s.prefix == "2001:468:201::/48")
            .unwrap();
        assert_eq!(a.statement, "No counterpart at this observer");
        assert!(!a.visible_at.is_empty());
        // No visibility is a distinct statement — never "no change".
        let prefixes: Vec<&str> = comparison
            .statements
            .iter()
            .map(|s| s.prefix.as_str())
            .collect();
        assert!(!prefixes.contains(&"10.0.0.0/24"));
        // Rows carry baseline_visibility for the visible stream.
        assert!(comparison.rows[0].baseline_visibility);
    }

    #[test]
    fn same_prefix_at_multiple_collectors_remains_separate_evidence() {
        let (_dir, conn) = open_temp_db();
        // Same prefix changed at rrc00 (RIS) and route-views2 (RouteViews).
        seed_run(
            &conn,
            1,
            "RipeRis",
            vec![(
                "rrc00",
                "1.1.1.1",
                "2001:468:201::/48",
                1,
                1,
                "DepartedTransit",
            )],
            vec![
                (
                    "Withdrawal",
                    "2019-08-21T16:45:25Z",
                    "1.1.1.1",
                    "2001:468:201::/48",
                ),
                (
                    "ReturnToBaseline",
                    "2019-08-21T17:02:19Z",
                    "1.1.1.1",
                    "2001:468:201::/48",
                ),
            ],
        );
        seed_run(
            &conn,
            2,
            "RouteViews",
            vec![(
                "route-views2",
                "64.57.28.241",
                "2001:468:201::/48",
                1,
                1,
                "DepartedTransit",
            )],
            vec![
                (
                    "Withdrawal",
                    "2019-08-21T16:45:25Z",
                    "64.57.28.241",
                    "2001:468:201::/48",
                ),
                (
                    "ReturnToBaseline",
                    "2019-08-21T17:02:19Z",
                    "64.57.28.241",
                    "2001:468:201::/48",
                ),
            ],
        );
        let comparison = build_observer_comparison(&conn, 1).unwrap();
        // Two separate rows — one per collector, each with its own family.
        let rows: Vec<_> = comparison
            .rows
            .iter()
            .filter(|r| r.prefix == "2001:468:201::/48")
            .collect();
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().any(|r| r.family == "RIPE RIS"));
        assert!(rows.iter().any(|r| r.family == "RouteViews"));
        // Statement says multiple independent public collectors — not
        // global confirmation.
        let st = comparison
            .statements
            .iter()
            .find(|s| s.prefix == "2001:468:201::/48")
            .unwrap();
        assert!(st
            .statement
            .starts_with("Observed at 2 independent public collectors"));
        assert!(!st.statement.contains("globally"));
    }

    #[test]
    fn timing_differences_are_preserved() {
        let (_dir, conn) = open_temp_db();
        seed_run(
            &conn,
            1,
            "RipeRis",
            vec![(
                "rrc00",
                "1.1.1.1",
                "2001:468:201::/48",
                1,
                1,
                "DepartedTransit",
            )],
            vec![
                (
                    "Withdrawal",
                    "2019-08-21T16:45:25Z",
                    "1.1.1.1",
                    "2001:468:201::/48",
                ),
                (
                    "ReturnToBaseline",
                    "2019-08-21T17:02:19Z",
                    "1.1.1.1",
                    "2001:468:201::/48",
                ),
            ],
        );
        seed_run(
            &conn,
            2,
            "RouteViews",
            vec![(
                "route-views2",
                "64.57.28.241",
                "2001:468:201::/48",
                1,
                1,
                "DepartedTransit",
            )],
            vec![
                (
                    "Withdrawal",
                    "2019-08-21T16:48:00Z",
                    "64.57.28.241",
                    "2001:468:201::/48",
                ),
                (
                    "ReturnToBaseline",
                    "2019-08-21T17:05:00Z",
                    "64.57.28.241",
                    "2001:468:201::/48",
                ),
            ],
        );
        let comparison = build_observer_comparison(&conn, 1).unwrap();
        let st = comparison
            .statements
            .iter()
            .find(|s| s.prefix == "2001:468:201::/48")
            .unwrap();
        assert!(
            st.timing_note.contains("different timing"),
            "{}",
            st.timing_note
        );
        assert!(st.timing_note.contains("16:45:25Z"));
        assert!(st.timing_note.contains("16:48:00Z"));
        // Per-row first changes preserve the exact timestamps.
        let rrc = comparison
            .rows
            .iter()
            .find(|r| r.collector == "rrc00")
            .unwrap();
        assert_eq!(
            rrc.first_change_utc.as_deref(),
            Some("2019-08-21T16:45:25Z")
        );
        let rv = comparison
            .rows
            .iter()
            .find(|r| r.collector == "route-views2")
            .unwrap();
        assert_eq!(rv.first_change_utc.as_deref(), Some("2019-08-21T16:48:00Z"));
    }

    #[test]
    fn multi_observer_agreement_is_not_global_confirmation() {
        let (_dir, conn) = open_temp_db();
        seed_run(
            &conn,
            1,
            "RipeRis",
            vec![(
                "rrc00",
                "1.1.1.1",
                "2001:468:201::/48",
                1,
                1,
                "DepartedTransit",
            )],
            vec![(
                "Withdrawal",
                "2019-08-21T16:45:25Z",
                "1.1.1.1",
                "2001:468:201::/48",
            )],
        );
        seed_run(
            &conn,
            2,
            "RouteViews",
            vec![(
                "route-views2",
                "64.57.28.241",
                "2001:468:201::/48",
                1,
                1,
                "DepartedTransit",
            )],
            vec![(
                "Withdrawal",
                "2019-08-21T16:45:25Z",
                "64.57.28.241",
                "2001:468:201::/48",
            )],
        );
        let comparison = build_observer_comparison(&conn, 1).unwrap();
        let st = comparison
            .statements
            .iter()
            .find(|s| s.prefix == "2001:468:201::/48")
            .unwrap();
        // The statement vocabulary never claims global proof.
        for forbidden in [
            "globally confirmed",
            "complete outage",
            "traffic loss confirmed",
            "operator action confirmed",
        ] {
            assert!(
                !st.statement.to_lowercase().contains(forbidden),
                "forbidden phrasing: {forbidden}"
            );
            assert!(
                !st.timing_note.to_lowercase().contains(forbidden),
                "forbidden phrasing in note: {forbidden}"
            );
        }
    }

    #[test]
    fn source_family_is_visible_in_comparison() {
        let (_dir, conn) = open_temp_db();
        seed_run(
            &conn,
            1,
            "RipeRis",
            vec![("rrc00", "1.1.1.1", "2001:468:201::/48", 0, 0, "Stable")],
            vec![],
        );
        seed_run(
            &conn,
            2,
            "RouteViews",
            vec![(
                "route-views2",
                "64.57.28.241",
                "2001:468:201::/48",
                0,
                0,
                "Stable",
            )],
            vec![],
        );
        let comparison = build_observer_comparison(&conn, 1).unwrap();
        let rows: Vec<_> = comparison
            .rows
            .iter()
            .filter(|r| r.prefix == "2001:468:201::/48")
            .collect();
        assert_eq!(rows.len(), 2);
        assert!(rows
            .iter()
            .any(|r| r.family == "RIPE RIS" && r.collector == "rrc00"));
        assert!(rows
            .iter()
            .any(|r| r.family == "RouteViews" && r.collector == "route-views2"));
        // The RouteViews-only pilot run (no source_family in manifest)
        // defaults to RouteViews.
        let (_dir2, conn2) = open_temp_db();
        seed_run(
            &conn2,
            3,
            "RouteViews",
            vec![(
                "route-views2",
                "64.57.28.241",
                "2001:468:201::/48",
                0,
                0,
                "Stable",
            )],
            vec![],
        );
        // A manifest without source_family:
        conn2
            .execute(
                "UPDATE manifest_revisions SET payload = '{\"event_id\":\"EVENT1\",\"collectors\":[]}'",
                [],
            )
            .unwrap();
        let comparison = build_observer_comparison(&conn2, 1).unwrap();
        assert_eq!(comparison.rows[0].family, "RouteViews");
    }

    #[test]
    fn comparison_is_deterministic() {
        let (_dir, conn) = open_temp_db();
        seed_run(
            &conn,
            1,
            "RipeRis",
            vec![
                (
                    "rrc00",
                    "1.1.1.1",
                    "2001:468:201::/48",
                    1,
                    1,
                    "DepartedTransit",
                ),
                ("rrc00", "2.2.2.2", "2001:468:202::/48", 0, 0, "Stable"),
            ],
            vec![
                (
                    "Withdrawal",
                    "2019-08-21T16:45:25Z",
                    "1.1.1.1",
                    "2001:468:201::/48",
                ),
                (
                    "ReturnToBaseline",
                    "2019-08-21T17:02:19Z",
                    "1.1.1.1",
                    "2001:468:201::/48",
                ),
            ],
        );
        seed_run(
            &conn,
            2,
            "RouteViews",
            vec![
                (
                    "route-views2",
                    "64.57.28.241",
                    "2001:468:202::/48",
                    0,
                    0,
                    "Stable",
                ),
                (
                    "route-views2",
                    "64.57.28.242",
                    "2001:468:201::/48",
                    1,
                    1,
                    "DepartedTransit",
                ),
            ],
            vec![
                (
                    "Withdrawal",
                    "2019-08-21T16:48:00Z",
                    "64.57.28.242",
                    "2001:468:201::/48",
                ),
                (
                    "ReturnToBaseline",
                    "2019-08-21T17:05:00Z",
                    "64.57.28.242",
                    "2001:468:201::/48",
                ),
            ],
        );
        let a = build_observer_comparison(&conn, 1).unwrap();
        let b = build_observer_comparison(&conn, 1).unwrap();
        assert_eq!(
            serde_json::to_string(&a).unwrap(),
            serde_json::to_string(&b).unwrap()
        );
        // Deterministic ordering: prefix, then collector.
        let prefixes: Vec<&str> = a.rows.iter().map(|r| r.prefix.as_str()).collect();
        let mut sorted = prefixes.clone();
        sorted.sort();
        assert_eq!(prefixes, sorted);
    }
}
