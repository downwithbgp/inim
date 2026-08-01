//! Catalog database and association-semantics tests.

use rusqlite::Connection;

use crate::catalog::db;
use crate::catalog::domain::*;
use crate::catalog::migrations::CATALOG_SCHEMA_VERSION;
use crate::catalog::store;

fn open_temp_db() -> (tempfile::TempDir, Connection) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("catalog.sqlite");
    let conn = db::open_catalog(&path).unwrap();
    (dir, conn)
}

fn sample_snapshot(event_id: i64, payload: &str) -> EventSnapshot {
    EventSnapshot {
        id: 0,
        event_id,
        fetched_at: "2026-07-31T00:00:00Z".to_string(),
        source_url: "https://example.invalid/tickets/T1".to_string(),
        content_sha256: crate::catalog::sync::hex_sha256(payload),
        raw_payload: payload.to_string(),
        normalized_json: format!(r#"{{"id":"T1","payload":{payload}}}"#),
        parser_version: "test-1".to_string(),
    }
}

fn sample_manifest_revision(event_id: i64, snapshot_id: i64, payload: &str) -> ManifestRevision {
    ManifestRevision {
        id: 0,
        event_id,
        snapshot_id,
        manifest_schema: 2,
        payload: payload.to_string(),
        sha256: crate::catalog::sync::hex_sha256(payload),
        review_status: "Reviewed".to_string(),
        reviewed_at: Some("2026-07-31T00:00:00Z".to_string()),
        reviewer: Some("analyst".to_string()),
    }
}

fn sample_plan(manifest_revision_id: i64, status: &str) -> AnalysisPlanRecord {
    let payload = format!(r#"{{"manifest_revision":{manifest_revision_id},"status":"{status}"}}"#);
    AnalysisPlanRecord {
        id: 0,
        manifest_revision_id,
        plan_schema: 1,
        payload: payload.clone(),
        sha256: crate::catalog::sync::hex_sha256(&payload),
        status: status.to_string(),
        block_reason: if status == "Blocked" {
            Some("MissingReviewedTransitPredicate".to_string())
        } else {
            None
        },
        created_at: "2026-07-31T00:00:00Z".to_string(),
    }
}

fn sample_run(plan_id: i64, started_at: &str) -> AnalysisRun {
    AnalysisRun {
        id: 0,
        plan_id,
        software_version: "0.1.0".to_string(),
        git_revision: Some("abc123".to_string()),
        parser_identity: "test-parser".to_string(),
        cache_schema_version: 2,
        report_schema_version: 2,
        status: "Complete".to_string(),
        started_at: started_at.to_string(),
        completed_at: Some(started_at.to_string()),
        runtime_secs: Some(1.0),
        verdict: Some("No route-state change observed".to_string()),
        assessment: Some("Consistent".to_string()),
    }
}

#[test]
fn fresh_database_applies_all_migrations() {
    let (_dir, conn) = open_temp_db();
    assert_eq!(db::current_version(&conn).unwrap(), CATALOG_SCHEMA_VERSION);
}

#[test]
fn reopening_database_is_idempotent() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("catalog.sqlite");
    let conn = db::open_catalog(&path).unwrap();
    let v1 = db::current_version(&conn).unwrap();
    drop(conn);
    let conn2 = db::open_catalog(&path).unwrap();
    assert_eq!(db::current_version(&conn2).unwrap(), v1);
}

#[test]
fn foreign_keys_are_enabled() {
    let (_dir, conn) = open_temp_db();
    let fk: i64 = conn
        .query_row("PRAGMA foreign_keys", [], |r| r.get(0))
        .unwrap();
    assert_eq!(fk, 1);
}

#[test]
fn duplicate_source_event_is_not_created() {
    let (_dir, conn) = open_temp_db();
    let a = store::upsert_event(&conn, "grnoc", "T1", "2026-07-31T00:00:00Z").unwrap();
    let b = store::upsert_event(&conn, "grnoc", "T1", "2026-07-31T01:00:00Z").unwrap();
    assert_eq!(a, b);
    assert_eq!(db::list_events(&conn).unwrap().len(), 1);
}

#[test]
fn unchanged_snapshot_is_deduplicated() {
    let (_dir, conn) = open_temp_db();
    let e = store::upsert_event(&conn, "grnoc", "T1", "2026-07-31T00:00:00Z").unwrap();
    let s = sample_snapshot(e, r#"{"title":"x"}"#);
    let id1 = store::insert_snapshot(&conn, e, &s).unwrap();
    let id2 = store::insert_snapshot(&conn, e, &s).unwrap();
    assert_eq!(id1, id2);
    assert_eq!(db::list_snapshots(&conn, e).unwrap().len(), 1);
}

#[test]
fn changed_source_payload_creates_new_snapshot() {
    let (_dir, conn) = open_temp_db();
    let e = store::upsert_event(&conn, "grnoc", "T1", "2026-07-31T00:00:00Z").unwrap();
    let s1 = sample_snapshot(e, r#"{"title":"x"}"#);
    store::insert_snapshot(&conn, e, &s1).unwrap();
    let s2 = sample_snapshot(e, r#"{"title":"x (edited)"}"#);
    store::insert_snapshot(&conn, e, &s2).unwrap();
    assert_eq!(db::list_snapshots(&conn, e).unwrap().len(), 2);
}

#[test]
fn manifest_revision_is_immutable() {
    let (_dir, conn) = open_temp_db();
    let e = store::upsert_event(&conn, "grnoc", "T1", "2026-07-31T00:00:00Z").unwrap();
    let s = sample_snapshot(e, r#"{"title":"x"}"#);
    let sid = store::insert_snapshot(&conn, e, &s).unwrap();
    let m = sample_manifest_revision(e, sid, r#"{"origin":[1]}"#);
    let id1 = store::insert_manifest_revision(&conn, &m).unwrap();
    let id2 = store::insert_manifest_revision(&conn, &m).unwrap();
    assert_eq!(id1, id2, "identical revision deduplicates, never updates");
    assert_eq!(db::list_manifest_revisions(&conn, e).unwrap().len(), 1);
}

#[test]
fn analysis_run_references_exact_plan() {
    let (_dir, conn) = open_temp_db();
    let e = store::upsert_event(&conn, "grnoc", "T1", "2026-07-31T00:00:00Z").unwrap();
    let sid = store::insert_snapshot(&conn, e, &sample_snapshot(e, r#"{"title":"x"}"#)).unwrap();
    let mid = store::insert_manifest_revision(
        &conn,
        &sample_manifest_revision(e, sid, r#"{"origin":[1]}"#),
    )
    .unwrap();
    let pid = store::insert_plan(&conn, &sample_plan(mid, "Ready")).unwrap();
    let rid = store::insert_run(&conn, &sample_run(pid, "2026-07-31T00:00:00Z")).unwrap();
    let run = db::get_run(&conn, rid).unwrap().unwrap();
    assert_eq!(run.plan_id, pid);
    let plan = db::get_plan(&conn, run.plan_id).unwrap().unwrap();
    assert_eq!(plan.manifest_revision_id, mid);
    let manifest = db::get_manifest_revision(&conn, plan.manifest_revision_id)
        .unwrap()
        .unwrap();
    assert_eq!(manifest.snapshot_id, sid);
    assert_eq!(manifest.event_id, e);
}

#[test]
fn deleting_event_with_referenced_analysis_is_rejected() {
    let (_dir, conn) = open_temp_db();
    let e = store::upsert_event(&conn, "grnoc", "T1", "2026-07-31T00:00:00Z").unwrap();
    let sid = store::insert_snapshot(&conn, e, &sample_snapshot(e, r#"{"title":"x"}"#)).unwrap();
    let mid = store::insert_manifest_revision(
        &conn,
        &sample_manifest_revision(e, sid, r#"{"origin":[1]}"#),
    )
    .unwrap();
    let pid = store::insert_plan(&conn, &sample_plan(mid, "Ready")).unwrap();
    store::insert_run(&conn, &sample_run(pid, "2026-07-31T00:00:00Z")).unwrap();
    let err = conn
        .execute("DELETE FROM catalog_events WHERE id = ?1", [e])
        .unwrap_err();
    assert!(err.to_string().contains("FOREIGN KEY"), "{err}");
}

#[test]
fn artifact_paths_are_relative() {
    let (_dir, conn) = open_temp_db();
    let e = store::upsert_event(&conn, "grnoc", "T1", "2026-07-31T00:00:00Z").unwrap();
    let sid = store::insert_snapshot(&conn, e, &sample_snapshot(e, r#"{"title":"x"}"#)).unwrap();
    let mid = store::insert_manifest_revision(
        &conn,
        &sample_manifest_revision(e, sid, r#"{"origin":[1]}"#),
    )
    .unwrap();
    let pid = store::insert_plan(&conn, &sample_plan(mid, "Ready")).unwrap();
    let rid = store::insert_run(&conn, &sample_run(pid, "2026-07-31T00:00:00Z")).unwrap();
    store::insert_artifact(
        &conn,
        &AnalysisArtifact {
            id: 0,
            run_id: rid,
            kind: "report".into(),
            relative_path: "INC0302574/report.json".into(),
            media_type: "application/json".into(),
            schema_version: Some(2),
            sha256: "abc".into(),
            size: 10,
            created_at: "2026-07-31T00:00:00Z".into(),
        },
    )
    .unwrap();
    let artifacts = db::list_artifacts(&conn, rid).unwrap();
    assert!(
        !artifacts[0].relative_path.starts_with('/'),
        "paths are relative"
    );
    assert!(
        !artifacts[0].relative_path.contains("C:"),
        "no drive letters"
    );
}

#[test]
fn transaction_failure_does_not_leave_partial_import() {
    let (_dir, conn) = open_temp_db();
    let tx = conn.unchecked_transaction().unwrap();
    let e = store::upsert_event(&tx, "grnoc", "T1", "2026-07-31T00:00:00Z").unwrap();
    store::insert_snapshot(&tx, e, &sample_snapshot(e, r#"{"title":"x"}"#)).unwrap();
    // Force an error inside the transaction: duplicate snapshot for the
    // same (event, content_sha256) violates the unique constraint.
    let dup = sample_snapshot(e, r#"{"title":"x"}"#);
    let err = tx
        .execute(
            "INSERT INTO event_snapshots (event_id, fetched_at, source_url, content_sha256, raw_payload, normalized_json, parser_version)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                dup.event_id,
                dup.fetched_at,
                dup.source_url,
                dup.content_sha256,
                dup.raw_payload,
                dup.normalized_json,
                dup.parser_version
            ],
        )
        .unwrap_err();
    assert!(err.to_string().contains("UNIQUE"), "{err}");
    drop(tx); // rollback
    assert_eq!(
        db::list_events(&conn).unwrap().len(),
        0,
        "rollback leaves no partial import"
    );
    assert_eq!(
        db::list_snapshots(&conn, e)
            .map_err(|_| ())
            .unwrap_or_else(|_| vec![])
            .len(),
        0
    );
}

// ── Part 8: association semantics ───────────────────────────────────

#[test]
fn lifecycle_requires_analysis_run() {
    // Stream lifecycles are keyed by run_id with a FK to analysis_runs.
    let (_dir, conn) = open_temp_db();
    let err = conn
        .execute(
            "INSERT INTO stream_lifecycle_summaries
               (run_id, collector, peer_ip, prefix, category, baseline_instances,
                max_active_instances, transition_count, withdrawn, restored,
                transit_state, add_path_ambiguous, evidence_refs)
             VALUES (9999, 'rv2', '1.2.3.4', '10.0.0.0/24', 'Unchanged', 1, 1, 0, 0, 0, 'Unchanged', 0, '[]')",
            [],
        )
        .unwrap_err();
    assert!(err.to_string().contains("FOREIGN KEY"), "{err}");
}

#[test]
fn analysis_run_requires_exact_plan_revision() {
    let (_dir, conn) = open_temp_db();
    let err = conn
        .execute(
            "INSERT INTO analysis_runs
               (plan_id, software_version, parser_identity, cache_schema_version,
                report_schema_version, status, started_at)
             VALUES (4242, '0.1.0', 'p', 2, 2, 'Complete', '2026-07-31T00:00:00Z')",
            [],
        )
        .unwrap_err();
    assert!(err.to_string().contains("FOREIGN KEY"), "{err}");
}

#[test]
fn plan_requires_exact_manifest_revision() {
    let (_dir, conn) = open_temp_db();
    let err = conn
        .execute(
            "INSERT INTO analysis_plans
               (manifest_revision_id, plan_schema, payload, sha256, status, created_at)
             VALUES (9999, 1, '{}', 'sha', 'Ready', '2026-07-31T00:00:00Z')",
            [],
        )
        .unwrap_err();
    assert!(err.to_string().contains("FOREIGN KEY"), "{err}");
}

#[test]
fn manifest_references_reviewed_source_snapshot() {
    let (_dir, conn) = open_temp_db();
    let e = store::upsert_event(&conn, "grnoc", "T1", "2026-07-31T00:00:00Z").unwrap();
    let sid = store::insert_snapshot(&conn, e, &sample_snapshot(e, r#"{"title":"x"}"#)).unwrap();
    let m = sample_manifest_revision(e, sid, r#"{"origin":[1]}"#);
    let mid = store::insert_manifest_revision(&conn, &m).unwrap();
    let manifest = db::get_manifest_revision(&conn, mid).unwrap().unwrap();
    // The manifest revision points at the exact snapshot it was reviewed
    // against; the snapshot points at the event.
    let snapshots = db::list_snapshots(&conn, manifest.event_id).unwrap();
    assert!(snapshots.iter().any(|s| s.id == manifest.snapshot_id));
}

#[test]
fn overlapping_event_windows_can_have_separate_runs() {
    let (_dir, conn) = open_temp_db();
    // Two events with overlapping windows, analyzed separately: each run
    // references its own plan chain.
    let e1 = store::upsert_event(&conn, "grnoc", "T1", "2026-07-31T00:00:00Z").unwrap();
    let e2 = store::upsert_event(&conn, "grnoc", "T2", "2026-07-31T00:00:00Z").unwrap();
    let s1 = store::insert_snapshot(&conn, e1, &sample_snapshot(e1, r#"{"title":"a"}"#)).unwrap();
    let s2 = store::insert_snapshot(&conn, e2, &sample_snapshot(e2, r#"{"title":"b"}"#)).unwrap();
    let m1 = store::insert_manifest_revision(
        &conn,
        &sample_manifest_revision(e1, s1, r#"{"origin":[1]}"#),
    )
    .unwrap();
    let m2 = store::insert_manifest_revision(
        &conn,
        &sample_manifest_revision(e2, s2, r#"{"origin":[2]}"#),
    )
    .unwrap();
    let p1 = store::insert_plan(&conn, &sample_plan(m1, "Ready")).unwrap();
    let p2 = store::insert_plan(&conn, &sample_plan(m2, "Ready")).unwrap();
    let r1 = store::insert_run(&conn, &sample_run(p1, "2026-07-31T00:00:00Z")).unwrap();
    let r2 = store::insert_run(&conn, &sample_run(p2, "2026-07-31T00:00:01Z")).unwrap();
    assert_ne!(r1, r2);
    // Each event sees exactly its own run.
    assert_eq!(db::list_runs_for_event(&conn, e1).unwrap().len(), 1);
    assert_eq!(db::list_runs_for_event(&conn, e2).unwrap().len(), 1);
}

#[test]
fn same_evidence_time_can_exist_in_multiple_event_conditioned_runs() {
    let (_dir, conn) = open_temp_db();
    // The same wall-clock evidence time may appear in two separate
    // event-conditioned runs; nothing in the catalog forbids it because
    // evidence belongs to runs, not to events.
    let e = store::upsert_event(&conn, "grnoc", "T1", "2026-07-31T00:00:00Z").unwrap();
    let sid = store::insert_snapshot(&conn, e, &sample_snapshot(e, r#"{"title":"x"}"#)).unwrap();
    let m1 = store::insert_manifest_revision(
        &conn,
        &sample_manifest_revision(e, sid, r#"{"origin":[1]}"#),
    )
    .unwrap();
    let m2 = store::insert_manifest_revision(
        &conn,
        &sample_manifest_revision(e, sid, r#"{"origin":[1],"note":"reviewed twice"}"#),
    )
    .unwrap();
    let p1 = store::insert_plan(&conn, &sample_plan(m1, "Ready")).unwrap();
    let p2 = store::insert_plan(&conn, &sample_plan(m2, "Ready")).unwrap();
    let r1 = store::insert_run(&conn, &sample_run(p1, "2026-07-31T10:00:00Z")).unwrap();
    let r2 = store::insert_run(&conn, &sample_run(p2, "2026-07-31T10:00:00Z")).unwrap();
    assert_ne!(r1, r2);
    assert_eq!(db::list_runs_for_event(&conn, e).unwrap().len(), 2);
}
// ── Part 5: catalog statuses ────────────────────────────────────────

use crate::catalog::status::{self, CatalogStatus};

fn chain_with_run(
    conn: &Connection,
    snapshot_payload: &str,
    manifest_payload: &str,
    run: Option<&AnalysisRun>,
) -> i64 {
    let e = store::upsert_event(conn, "grnoc", "T1", "2026-07-31T00:00:00Z").unwrap();
    let sid = store::insert_snapshot(conn, e, &sample_snapshot(e, snapshot_payload)).unwrap();
    let mid =
        store::insert_manifest_revision(conn, &sample_manifest_revision(e, sid, manifest_payload))
            .unwrap();
    let pid = store::insert_plan(conn, &sample_plan(mid, "Ready")).unwrap();
    if let Some(r) = run {
        let mut r2 = r.clone();
        r2.plan_id = pid;
        store::insert_run(conn, &r2).unwrap();
    }
    e
}

#[test]
fn event_without_manifest_is_discovered() {
    let (_dir, conn) = open_temp_db();
    let e = store::upsert_event(&conn, "grnoc", "T1", "2026-07-31T00:00:00Z").unwrap();
    assert_eq!(
        status::derive_status(&conn, e).unwrap(),
        CatalogStatus::Discovered
    );
}

#[test]
fn unresolved_manifest_is_needs_review() {
    let (_dir, conn) = open_temp_db();
    let e = store::upsert_event(&conn, "grnoc", "T1", "2026-07-31T00:00:00Z").unwrap();
    let sid = store::insert_snapshot(&conn, e, &sample_snapshot(e, r#"{"title":"x"}"#)).unwrap();
    let mut m = sample_manifest_revision(e, sid, r#"{"origin":[1]}"#);
    m.review_status = "Unresolved".to_string();
    store::insert_manifest_revision(&conn, &m).unwrap();
    assert_eq!(
        status::derive_status(&conn, e).unwrap(),
        CatalogStatus::NeedsReview
    );
}

#[test]
fn blocked_plan_yields_blocked_status() {
    let (_dir, conn) = open_temp_db();
    let e = store::upsert_event(&conn, "grnoc", "T1", "2026-07-31T00:00:00Z").unwrap();
    let sid = store::insert_snapshot(&conn, e, &sample_snapshot(e, r#"{"title":"x"}"#)).unwrap();
    let mid = store::insert_manifest_revision(
        &conn,
        &sample_manifest_revision(e, sid, r#"{"origin":[1]}"#),
    )
    .unwrap();
    store::insert_plan(&conn, &sample_plan(mid, "Blocked")).unwrap();
    assert_eq!(
        status::derive_status(&conn, e).unwrap(),
        CatalogStatus::Blocked
    );
}

#[test]
fn completed_run_yields_complete_status() {
    let (_dir, conn) = open_temp_db();
    let run = sample_run(0, "2026-07-31T10:00:00Z");
    let e = chain_with_run(&conn, r#"{"title":"x"}"#, r#"{"origin":[1]}"#, Some(&run));
    assert_eq!(
        status::derive_status(&conn, e).unwrap(),
        CatalogStatus::Complete
    );
}

#[test]
fn ready_plan_without_run_yields_ready() {
    let (_dir, conn) = open_temp_db();
    let e = chain_with_run(&conn, r#"{"title":"x"}"#, r#"{"origin":[1]}"#, None);
    assert_eq!(
        status::derive_status(&conn, e).unwrap(),
        CatalogStatus::Ready
    );
}

#[test]
fn changed_source_snapshot_marks_latest_view_stale() {
    let (_dir, conn) = open_temp_db();
    let run = sample_run(0, "2026-07-31T10:00:00Z");
    let e = chain_with_run(&conn, r#"{"title":"x"}"#, r#"{"origin":[1]}"#, Some(&run));
    assert_eq!(
        status::derive_status(&conn, e).unwrap(),
        CatalogStatus::Complete
    );
    // Ticket edited after the completed run.
    store::insert_snapshot(&conn, e, &sample_snapshot(e, r#"{"title":"x (edited)"}"#)).unwrap();
    assert_eq!(
        status::derive_status(&conn, e).unwrap(),
        CatalogStatus::Stale
    );
}

#[test]
fn changed_manifest_marks_latest_view_stale() {
    let (_dir, conn) = open_temp_db();
    let run = sample_run(0, "2026-07-31T10:00:00Z");
    let e = chain_with_run(&conn, r#"{"title":"x"}"#, r#"{"origin":[1]}"#, Some(&run));
    // New reviewed manifest revision after the completed run.
    let sid = db::list_snapshots(&conn, e).unwrap()[0].id;
    store::insert_manifest_revision(
        &conn,
        &sample_manifest_revision(e, sid, r#"{"origin":[1],"note":"re-reviewed"}"#),
    )
    .unwrap();
    assert_eq!(
        status::derive_status(&conn, e).unwrap(),
        CatalogStatus::Stale
    );
}

#[test]
fn old_completed_run_remains_historically_complete() {
    let (_dir, conn) = open_temp_db();
    let run = sample_run(0, "2026-07-31T10:00:00Z");
    let e = chain_with_run(&conn, r#"{"title":"x"}"#, r#"{"origin":[1]}"#, Some(&run));
    // Stale at the catalog level, but the run row itself is still Complete.
    store::insert_snapshot(&conn, e, &sample_snapshot(e, r#"{"title":"x (edited)"}"#)).unwrap();
    assert_eq!(
        status::derive_status(&conn, e).unwrap(),
        CatalogStatus::Stale
    );
    let runs = db::list_runs_for_event(&conn, e).unwrap();
    assert_eq!(runs[0].status, "Complete", "historical run stays complete");
}

#[test]
fn running_status_has_documented_precedence() {
    let (_dir, conn) = open_temp_db();
    let mut run = sample_run(0, "2026-07-31T10:00:00Z");
    run.status = "Running".to_string();
    let e = chain_with_run(&conn, r#"{"title":"x"}"#, r#"{"origin":[1]}"#, Some(&run));
    // Running wins over everything else per the documented precedence.
    assert_eq!(
        status::derive_status(&conn, e).unwrap(),
        CatalogStatus::Running
    );
}
