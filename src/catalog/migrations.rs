//! Catalog schema — versioned migrations.
//!
//! The registry is `PRAGMA user_version`. Each migration runs inside a
//! transaction; a fresh database applies all migrations in order, and a
//! reopened database at the current version is a no-op.

/// Current catalog schema version.
pub const CATALOG_SCHEMA_VERSION: u32 = 1;

/// Ordered migrations. Index i migrates user_version i -> i+1.
pub const MIGRATIONS: &[&str] = &[V1];

const V1: &str = r#"
CREATE TABLE catalog_events (
    id          INTEGER PRIMARY KEY,
    source_kind TEXT NOT NULL,
    external_id TEXT NOT NULL,
    first_seen  TEXT NOT NULL,
    last_seen   TEXT NOT NULL,
    UNIQUE (source_kind, external_id)
);

CREATE TABLE event_snapshots (
    id               INTEGER PRIMARY KEY,
    event_id         INTEGER NOT NULL REFERENCES catalog_events(id),
    fetched_at       TEXT NOT NULL,
    source_url       TEXT NOT NULL,
    content_sha256   TEXT NOT NULL,
    raw_payload      TEXT NOT NULL,
    normalized_json  TEXT NOT NULL,
    parser_version   TEXT NOT NULL,
    UNIQUE (event_id, content_sha256)
);

CREATE TABLE manifest_revisions (
    id              INTEGER PRIMARY KEY,
    event_id        INTEGER NOT NULL REFERENCES catalog_events(id),
    snapshot_id     INTEGER NOT NULL REFERENCES event_snapshots(id),
    manifest_schema INTEGER NOT NULL,
    payload         TEXT NOT NULL,
    sha256          TEXT NOT NULL UNIQUE,
    review_status   TEXT NOT NULL,
    reviewed_at     TEXT,
    reviewer        TEXT,
    UNIQUE (event_id, sha256)
);

CREATE TABLE analysis_plans (
    id                   INTEGER PRIMARY KEY,
    manifest_revision_id INTEGER NOT NULL REFERENCES manifest_revisions(id),
    plan_schema          INTEGER NOT NULL,
    payload              TEXT NOT NULL,
    sha256               TEXT NOT NULL UNIQUE,
    status               TEXT NOT NULL,
    block_reason         TEXT,
    created_at           TEXT NOT NULL
);

CREATE TABLE analysis_runs (
    id                   INTEGER PRIMARY KEY,
    plan_id              INTEGER NOT NULL REFERENCES analysis_plans(id),
    software_version     TEXT NOT NULL,
    git_revision         TEXT,
    parser_identity      TEXT NOT NULL,
    cache_schema_version INTEGER NOT NULL,
    report_schema_version INTEGER NOT NULL,
    status               TEXT NOT NULL,
    started_at           TEXT NOT NULL,
    completed_at         TEXT,
    runtime_secs         REAL,
    verdict              TEXT,
    assessment           TEXT,
    UNIQUE (plan_id, started_at)
);

CREATE TABLE analysis_artifacts (
    id             INTEGER PRIMARY KEY,
    run_id         INTEGER NOT NULL REFERENCES analysis_runs(id),
    kind           TEXT NOT NULL,
    relative_path  TEXT NOT NULL,
    media_type     TEXT NOT NULL,
    schema_version INTEGER,
    sha256         TEXT NOT NULL,
    size           INTEGER NOT NULL,
    created_at     TEXT NOT NULL,
    UNIQUE (run_id, relative_path)
);

CREATE TABLE stream_lifecycle_summaries (
    id                  INTEGER PRIMARY KEY,
    run_id              INTEGER NOT NULL REFERENCES analysis_runs(id),
    collector           TEXT NOT NULL,
    peer_ip             TEXT NOT NULL,
    prefix              TEXT NOT NULL,
    category            TEXT NOT NULL,
    baseline_instances  INTEGER NOT NULL,
    max_active_instances INTEGER NOT NULL,
    transition_count    INTEGER NOT NULL,
    withdrawn           INTEGER NOT NULL,
    restored            INTEGER NOT NULL,
    transit_state       TEXT NOT NULL,
    add_path_ambiguous  INTEGER NOT NULL,
    evidence_refs       TEXT NOT NULL
);

CREATE TABLE semantic_wave_summaries (
    id             INTEGER PRIMARY KEY,
    run_id         INTEGER NOT NULL REFERENCES analysis_runs(id),
    wave_id        TEXT NOT NULL,
    label          TEXT NOT NULL,
    start          TEXT NOT NULL,
    peak_start     TEXT NOT NULL,
    peak_end       TEXT NOT NULL,
    end            TEXT NOT NULL,
    stream_count   INTEGER NOT NULL,
    instance_count INTEGER NOT NULL
);

CREATE TABLE catalog_sync_runs (
    id               INTEGER PRIMARY KEY,
    source           TEXT NOT NULL,
    started_at       TEXT NOT NULL,
    completed_at     TEXT,
    status           TEXT NOT NULL,
    events_examined  INTEGER NOT NULL,
    new_events       INTEGER NOT NULL,
    changed_events   INTEGER NOT NULL,
    unchanged_events INTEGER NOT NULL,
    failures         INTEGER NOT NULL
);

CREATE INDEX idx_snapshots_event ON event_snapshots(event_id, fetched_at);
CREATE INDEX idx_manifest_event ON manifest_revisions(event_id);
CREATE INDEX idx_plans_manifest ON analysis_plans(manifest_revision_id);
CREATE INDEX idx_runs_plan ON analysis_runs(plan_id, started_at);
CREATE INDEX idx_artifacts_run ON analysis_artifacts(run_id);
CREATE INDEX idx_streams_run ON stream_lifecycle_summaries(run_id);
CREATE INDEX idx_waves_run ON semantic_wave_summaries(run_id);
"#;
