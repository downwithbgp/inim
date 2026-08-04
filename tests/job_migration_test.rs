//! Migration and schema tests for the durable analysis-job layer (V10).

use inim::catalog::db;
use inim::catalog::migrations::{CATALOG_SCHEMA_VERSION, MIGRATIONS};

fn open_temp_db() -> (tempfile::TempDir, rusqlite::Connection) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("catalog.sqlite");
    let conn = db::open_catalog(&path).unwrap();
    (dir, conn)
}

/// Build a database at exactly the previous schema version (v9) by
/// applying the first nine migrations and recording the version.
fn open_v9_db() -> (tempfile::TempDir, rusqlite::Connection) {
    // Build a schema-v9 database directly from the migration slice.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("catalog.sqlite");
    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    for m in &MIGRATIONS[..9] {
        conn.execute_batch(m).unwrap();
    }
    conn.execute_batch("PRAGMA user_version = 9").unwrap();
    (dir, conn)
}

#[test]
fn empty_v9_database_migrates_to_v10() {
    let (dir, conn) = open_v9_db();
    assert_eq!(db::current_version(&conn).unwrap(), 9);
    db::migrate(&conn).unwrap();
    assert_eq!(db::current_version(&conn).unwrap(), CATALOG_SCHEMA_VERSION);
    let jobs: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='analysis_jobs'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let events: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='analysis_job_events'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let hb: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='worker_heartbeats'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!((jobs, events, hb), (1, 1, 1));
    // V11 adds the reviewed interconnection-context column.
    let ctx_col: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('case_studies') WHERE name = 'interconnection_context'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(ctx_col, 1);
    drop(dir);
}

#[test]
fn migration_batch_is_transactional() {
    // A V10 batch that fails mid-way must leave no partial tables and no
    // version bump. We simulate by running the migration SQL minus its
    // last statement inside a transaction and rolling back.
    let (dir, conn) = open_v9_db();
    let v10 = MIGRATIONS[MIGRATIONS.len() - 2]; // V10: the last table-creating migration
                                                // Split off the final CREATE INDEX (worker heartbeat freshness).
    let idx = "CREATE INDEX idx_worker_heartbeat ON worker_heartbeats(last_heartbeat);";
    let partial = v10.replace(
        idx,
        "CREATE INDEX idx_worker_heartbeat_broken ON missing_table(x);",
    );
    let tx = conn.unchecked_transaction().unwrap();
    let failed = tx.execute_batch(&partial).is_err();
    // The transaction is never committed; roll back explicitly.
    tx.rollback().ok();
    assert!(failed, "the broken batch must fail");
    let jobs: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='analysis_jobs'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(jobs, 0, "partial migration must not leave tables behind");
    assert_eq!(db::current_version(&conn).unwrap(), 9);
    drop(dir);
}

#[test]
fn incompatible_future_schema_is_rejected() {
    let (dir, conn) = open_temp_db();
    conn.execute_batch(&format!(
        "PRAGMA user_version = {}",
        CATALOG_SCHEMA_VERSION + 1
    ))
    .unwrap();
    let err = db::migrate(&conn).unwrap_err();
    assert!(err.contains("newer than supported"), "{err}");
    let path = dir.path().join("catalog.sqlite");
    let err = db::open_catalog_readonly(&path).unwrap_err();
    assert!(err.contains("incompatible"), "{err}");
}

#[test]
fn job_tables_have_required_foreign_keys() {
    let (_dir, conn) = open_temp_db();
    let sql: String = conn
        .query_row(
            "SELECT sql FROM sqlite_master WHERE type='table' AND name='analysis_jobs'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(
        sql.contains("REFERENCES analysis_plans(id) ON DELETE RESTRICT"),
        "plan linkage must be RESTRICT: {sql}"
    );
    assert!(
        sql.contains("REFERENCES analysis_runs(id) ON DELETE RESTRICT"),
        "run linkage must be RESTRICT: {sql}"
    );
    let events: String = conn
        .query_row(
            "SELECT sql FROM sqlite_master WHERE type='table' AND name='analysis_job_events'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(
        events.contains("REFERENCES analysis_jobs(id) ON DELETE CASCADE"),
        "events must cascade with the job: {events}"
    );
}

#[test]
fn foreign_keys_enforce_job_relationships() {
    let (_dir, conn) = open_temp_db();
    // A job referencing a nonexistent plan is rejected.
    let err = conn
        .execute(
            "INSERT INTO analysis_jobs (id, plan_revision_id, requested_by, requested_at, state, plan_hash)
             VALUES ('job-x', 999999, 'cli', '2026-08-01T00:00:00Z', 'Queued', 'h')",
            [],
        )
        .unwrap_err();
    assert!(
        err.to_string().contains("FOREIGN KEY"),
        "expected FK violation: {err}"
    );
}

#[test]
fn completed_run_linkage_is_constrained() {
    let (dir, conn) = open_temp_db();
    // Build the minimal chain: event -> snapshot -> manifest -> plan -> run.
    conn.execute(
        "INSERT INTO catalog_events (source_kind, external_id, first_seen, last_seen)
         VALUES ('local-repository', 'EVT', '2026-08-01T00:00:00Z', '2026-08-01T00:00:00Z')",
        [],
    )
    .unwrap();
    let event_id = conn.last_insert_rowid();
    conn.execute(
        "INSERT INTO event_snapshots (event_id, fetched_at, source_url, content_sha256, raw_payload, normalized_json, parser_version)
         VALUES (?1, '2026-08-01T00:00:00Z', 'file:///x', 's', '{}', '{}', 't')",
        [event_id],
    )
    .unwrap();
    let snapshot_id = conn.last_insert_rowid();
    conn.execute(
        "INSERT INTO manifest_revisions (event_id, snapshot_id, manifest_schema, payload, sha256, review_status)
         VALUES (?1, ?2, 2, '{}', 'ms', 'Reviewed')",
        rusqlite::params![event_id, snapshot_id],
    )
    .unwrap();
    let mr_id = conn.last_insert_rowid();
    conn.execute(
        "INSERT INTO analysis_plans (manifest_revision_id, plan_schema, payload, sha256, status, created_at)
         VALUES (?1, 1, '{}', 'ps', 'Ready', '2026-08-01T00:00:00Z')",
        [mr_id],
    )
    .unwrap();
    let plan_id = conn.last_insert_rowid();
    conn.execute(
        "INSERT INTO analysis_runs (plan_id, software_version, parser_identity, cache_schema_version,
             report_schema_version, status, started_at)
         VALUES (?1, '0.1.0', 'p', 1, 1, 'Complete', '2026-08-01T00:00:00Z')",
        [plan_id],
    )
    .unwrap();
    let run_id = conn.last_insert_rowid();
    conn.execute(
        "INSERT INTO analysis_jobs (id, plan_revision_id, requested_by, requested_at, state, plan_hash, completed_run_id)
         VALUES ('job-1', ?1, 'cli', '2026-08-01T00:00:00Z', 'Completed', 'h', ?2)",
        rusqlite::params![plan_id, run_id],
    )
    .unwrap();
    // The immutable run must not be deletable while referenced.
    let err = conn
        .execute("DELETE FROM analysis_runs WHERE id = ?1", [run_id])
        .unwrap_err();
    assert!(
        err.to_string().contains("FOREIGN KEY"),
        "run delete must be blocked: {err}"
    );
    // The plan must not be deletable while referenced.
    let err = conn
        .execute("DELETE FROM analysis_plans WHERE id = ?1", [plan_id])
        .unwrap_err();
    assert!(
        err.to_string().contains("FOREIGN KEY"),
        "plan delete must be blocked: {err}"
    );
    drop(dir);
}

#[test]
fn claim_query_uses_expected_index() {
    let (_dir, conn) = open_temp_db();
    let idx: String = conn
        .query_row(
            "SELECT sql FROM sqlite_master WHERE type='index' AND name='idx_jobs_active'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(
        idx.contains("analysis_jobs(state, requested_at)"),
        "unexpected idx_jobs_active definition: {idx}"
    );
    let mut plan = conn
        .prepare(
            "EXPLAIN QUERY PLAN
             SELECT id FROM analysis_jobs
             WHERE state = 'Queued'
             ORDER BY requested_at ASC, id ASC LIMIT 1",
        )
        .unwrap();
    let rows: Vec<String> = plan
        .query_map([], |r| r.get::<_, String>(3))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    let joined = rows.join(" | ");
    assert!(
        joined.contains("idx_jobs_active"),
        "claim query must use idx_jobs_active, got: {joined}"
    );
}

// ── Part 2: migration preservation (v9 -> v10) ──────────────────────

use inim::catalog::migrations::MIGRATIONS as ALL_MIGRATIONS;

/// Build a schema-v9 database with representative pre-v10 catalog data
/// (generic fixture identities): a GRNOC catalog event with an
/// immutable snapshot, a ticket relationship, a reviewed role, a plan
/// revision, and a completed analysis run.
fn open_v9_db_with_data() -> (tempfile::TempDir, rusqlite::Connection) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("catalog.sqlite");
    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    for m in &ALL_MIGRATIONS[..9] {
        conn.execute_batch(m).unwrap();
    }
    conn.execute_batch("PRAGMA user_version = 9").unwrap();

    // GRNOC catalog event + immutable snapshot.
    conn.execute(
        "INSERT INTO catalog_events (source_kind, external_id, first_seen, last_seen)
         VALUES ('grnoc-public-task-viewer', 'CORPUS-TICKET-001', '2019-08-21T16:00:00Z', '2019-08-21T20:00:00Z')",
        [],
    )
    .unwrap();
    let event_id = conn.last_insert_rowid();
    let raw = r#"{"number":"CORPUS-TICKET-001","short_description":"Generic corpus ticket","description":"public record body"}"#;
    conn.execute(
        "INSERT INTO event_snapshots (event_id, fetched_at, source_url, content_sha256, raw_payload, normalized_json, parser_version)
         VALUES (?1, '2019-08-21T16:00:00Z', 'https://example.invalid/tickets/CORPUS-TICKET-001/', ?2, ?3, ?4, 'viewer-1')",
        rusqlite::params![
            event_id,
            inim::catalog::document::hex_sha256(raw.as_bytes()),
            raw,
            r#"{"number":"CORPUS-TICKET-001"}"#,
        ],
    )
    .unwrap();
    let snapshot_id = conn.last_insert_rowid();

    // Ticket relationship (explicit, unreviewed is fine; must survive).
    conn.execute(
        "INSERT INTO ticket_relationships (from_event_id, to_event_id, to_external_id, relationship_kind,
             evidence_kind, source_snapshot_id, reviewed_status, note, created_utc)
         VALUES (?1, NULL, 'CORPUS-TICKET-002', 'TracksRemainingImpactIn', 'ExplicitTicketText',
             ?2, 'Unreviewed', 'extracted from snapshot', '2026-08-01T00:00:00Z')",
        rusqlite::params![event_id, snapshot_id],
    )
    .unwrap();

    // Reviewed role (V7 ticket_reviews).
    conn.execute(
        "INSERT INTO ticket_reviews (catalog_event_id, external_id, reviewed_roles_json, entity_labels_json,
             linked_change_ids_json, analysis_applicability, applicability_rationale,
             relationship_to_case_study, review_status, reviewer, reviewed_at, provenance_json)
         VALUES (?1, 'CORPUS-TICKET-001', '[\"PrimaryIncident\"]', '[\"generic participant\"]',
             '[]', 'PotentiallyVisibleInPublicBgp', 'generic rationale', 'PrimaryIncident', 'Reviewed',
             'fixture-reviewer', '2019-08-21T00:00:00Z', '{}')",
        [event_id],
    )
    .unwrap();

    // Manifest revision + plan + completed run.
    let manifest_payload = serde_json::json!({
        "event_id": "CORPUS-TICKET-001", "revision": 1, "schema_version": 2, "open": false,
        "event_window_utc": {"start": "2019-08-21T16:00:00Z", "end": "2019-08-21T20:00:00Z"},
        "ticket_window_local": {"start": "", "end": "", "timezone": "UTC"},
        "target": {"label": "Generic target", "origin_asns": [64500],
            "transit_predicate": {"status": "Unresolved"}},
        "collectors": ["route-views2"], "source_family": "RouteViews"
    })
    .to_string();
    conn.execute(
        "INSERT INTO manifest_revisions (event_id, snapshot_id, manifest_schema, payload, sha256, review_status)
         VALUES (?1, ?2, 2, ?3, ?4, 'Unresolved')",
        rusqlite::params![event_id, snapshot_id, manifest_payload, "preserve-msha"],
    )
    .unwrap();
    let mid = conn.last_insert_rowid();
    conn.execute(
        "INSERT INTO analysis_plans (manifest_revision_id, plan_schema, payload, sha256, status, block_reason, created_at)
         VALUES (?1, 1, '{}', 'preserve-psha', 'Blocked', 'MissingReviewedTransitPredicate', '2019-08-21T00:00:00Z')",
        [mid],
    )
    .unwrap();
    let plan_id = conn.last_insert_rowid();
    conn.execute(
        "INSERT INTO analysis_runs (plan_id, software_version, parser_identity, cache_schema_version,
             report_schema_version, status, started_at, completed_at, verdict, assessment)
         VALUES (?1, '0.1.0', 'p', 1, 2, 'Complete', '2019-08-21T16:00:00Z', '2019-08-21T20:00:00Z',
             'NoRouteStateChange', 'generic assessment')",
        [plan_id],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO analysis_artifacts (run_id, kind, relative_path, media_type, sha256, size, created_at)
         VALUES (?1, 'report', 'CORPUS-TICKET-001/report.json', 'application/json', 'abc', 3, '2019-08-21T00:00:00Z')",
        [conn.last_insert_rowid()],
    )
    .unwrap();
    (dir, conn)
}

#[test]
fn v9_to_v10_preserves_catalog_events() {
    let (dir, conn) = open_v9_db_with_data();
    inim::catalog::db::migrate(&conn).unwrap();
    assert_eq!(
        inim::catalog::db::current_version(&conn).unwrap(),
        CATALOG_SCHEMA_VERSION
    );
    let n: i64 = conn
        .query_row("SELECT COUNT(*) FROM catalog_events", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 1, "events must be retained");
    let (kind, ext): (String, String) = conn
        .query_row(
            "SELECT source_kind, external_id FROM catalog_events",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(kind, "grnoc-public-task-viewer");
    assert_eq!(ext, "CORPUS-TICKET-001");
    drop(dir);
}

#[test]
fn v9_to_v10_preserves_snapshots() {
    let (dir, conn) = open_v9_db_with_data();
    inim::catalog::db::migrate(&conn).unwrap();
    let (raw, sha): (String, String) = conn
        .query_row(
            "SELECT raw_payload, content_sha256 FROM event_snapshots",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert!(
        raw.contains("public record body"),
        "raw snapshot must be byte-identical"
    );
    assert_eq!(
        sha,
        inim::catalog::document::hex_sha256(raw.as_bytes()),
        "recorded hash must still match the preserved payload"
    );
    drop(dir);
}

#[test]
fn v9_to_v10_preserves_relationships() {
    let (dir, conn) = open_v9_db_with_data();
    inim::catalog::db::migrate(&conn).unwrap();
    let (kind, ev, rev): (String, String, String) = conn
        .query_row(
            "SELECT relationship_kind, evidence_kind, reviewed_status FROM ticket_relationships",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(kind, "TracksRemainingImpactIn");
    assert_eq!(ev, "ExplicitTicketText");
    assert_eq!(rev, "Unreviewed");
    let (appl, roles): (String, String) = conn
        .query_row(
            "SELECT analysis_applicability, reviewed_roles_json FROM ticket_reviews",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(appl, "PotentiallyVisibleInPublicBgp");
    assert!(roles.contains("PrimaryIncident"));
    drop(dir);
}

#[test]
fn v9_to_v10_does_not_create_jobs_for_old_runs() {
    let (dir, conn) = open_v9_db_with_data();
    inim::catalog::db::migrate(&conn).unwrap();
    let jobs: i64 = conn
        .query_row("SELECT COUNT(*) FROM analysis_jobs", [], |r| r.get(0))
        .unwrap();
    assert_eq!(jobs, 0, "old runs must never become jobs");
    let runs: i64 = conn
        .query_row("SELECT COUNT(*) FROM analysis_runs", [], |r| r.get(0))
        .unwrap();
    assert_eq!(runs, 1, "existing runs stay runs");
    let plans: i64 = conn
        .query_row("SELECT COUNT(*) FROM analysis_plans", [], |r| r.get(0))
        .unwrap();
    assert_eq!(plans, 1);
    let events: i64 = conn
        .query_row("SELECT COUNT(*) FROM analysis_job_events", [], |r| r.get(0))
        .unwrap();
    assert_eq!(events, 0);
    drop(dir);
}

#[test]
fn migration_is_idempotent_at_v10() {
    let (dir, conn) = open_v9_db_with_data();
    inim::catalog::db::migrate(&conn).unwrap();
    let before: i64 = conn
        .query_row("SELECT COUNT(*) FROM catalog_events", [], |r| r.get(0))
        .unwrap();
    inim::catalog::db::migrate(&conn).unwrap();
    let after: i64 = conn
        .query_row("SELECT COUNT(*) FROM catalog_events", [], |r| r.get(0))
        .unwrap();
    assert_eq!(before, after);
    assert_eq!(
        inim::catalog::db::current_version(&conn).unwrap(),
        CATALOG_SCHEMA_VERSION
    );
    drop(dir);
}

#[test]
fn migration_preserves_existing_catalog_events() {
    // Summarizing guard: the four focused preservation tests above are
    // the evidence; this test anchors the audit requirement by name.
    let (dir, conn) = open_v9_db_with_data();
    inim::catalog::db::migrate(&conn).unwrap();
    let counts: (i64, i64, i64, i64) = conn
        .query_row(
            "SELECT (SELECT COUNT(*) FROM catalog_events),
                    (SELECT COUNT(*) FROM event_snapshots),
                    (SELECT COUNT(*) FROM ticket_relationships),
                    (SELECT COUNT(*) FROM ticket_reviews)",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .unwrap();
    assert_eq!(counts, (1, 1, 1, 1));
    drop(dir);
}
