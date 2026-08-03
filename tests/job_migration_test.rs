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
    let (dir, conn) = open_temp_db();
    let conn = conn;
    let v9 = (CATALOG_SCHEMA_VERSION - 1) as usize;
    // Fresh DBs already migrated to the current version; rebuild at v9.
    conn.execute_batch("DROP TABLE IF EXISTS worker_heartbeats")
        .ok();
    conn.execute_batch("DROP TABLE IF EXISTS analysis_job_events")
        .ok();
    conn.execute_batch("DROP TABLE IF EXISTS analysis_jobs")
        .ok();
    conn.execute_batch(&format!("PRAGMA user_version = {}", v9))
        .unwrap();
    (dir, conn)
}

#[test]
fn empty_v9_database_migrates_to_v10() {
    let (dir, conn) = open_temp_db();
    // Strip the V10 tables to simulate a v9 database, then migrate.
    conn.execute_batch("DROP TABLE IF EXISTS worker_heartbeats")
        .ok();
    conn.execute_batch("DROP TABLE IF EXISTS analysis_job_events")
        .ok();
    conn.execute_batch("DROP TABLE IF EXISTS analysis_jobs")
        .ok();
    conn.execute_batch(&format!(
        "PRAGMA user_version = {}",
        CATALOG_SCHEMA_VERSION - 1
    ))
    .unwrap();
    assert_eq!(
        db::current_version(&conn).unwrap(),
        CATALOG_SCHEMA_VERSION - 1
    );
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
    drop(dir);
}

#[test]
fn migration_batch_is_transactional() {
    // A V10 batch that fails mid-way must leave no partial tables and no
    // version bump. We simulate by running the migration SQL minus its
    // last statement inside a transaction and rolling back.
    let (dir, conn) = open_v9_db();
    let v10 = MIGRATIONS.last().unwrap();
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
    assert_eq!(
        db::current_version(&conn).unwrap(),
        CATALOG_SCHEMA_VERSION - 1
    );
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
