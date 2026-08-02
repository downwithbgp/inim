//! Catalog schema — versioned migrations.
//!
//! The registry is `PRAGMA user_version`. Each migration runs inside a
//! transaction; a fresh database applies all migrations in order, and a
//! reopened database at the current version is a no-op.

/// Current catalog schema version.
pub const CATALOG_SCHEMA_VERSION: u32 = 9;

/// Ordered migrations. Index i migrates user_version i -> i+1.
pub const MIGRATIONS: &[&str] = &[V1, V2, V3, V4, V5, V6, V7, V8, V9];

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

/// V2: multi-ticket incident case-study layer.
///
/// Case studies group reviewed operator-reported sources and link them to
/// immutable AnalysisRuns. Evidence stays owned by AnalysisRuns; these tables
/// never carry RouteObservation/RouteTransition/EvidenceRef case-study ids.
const V2: &str = r#"
CREATE TABLE case_studies (
    id            INTEGER PRIMARY KEY,
    slug          TEXT NOT NULL UNIQUE,
    title         TEXT NOT NULL,
    summary       TEXT NOT NULL,
    start_utc     TEXT,
    end_utc       TEXT,
    status        TEXT NOT NULL,
    content_sha256 TEXT NOT NULL,
    created_utc   TEXT NOT NULL,
    updated_utc   TEXT NOT NULL
);

CREATE TABLE reference_documents (
    id                   INTEGER PRIMARY KEY,
    title                TEXT NOT NULL,
    source_url           TEXT,
    doc_type             TEXT NOT NULL,
    redistribution_status TEXT NOT NULL,
    publication_date     TEXT,
    provenance           TEXT NOT NULL,
    imported_utc         TEXT NOT NULL
);

CREATE TABLE document_revisions (
    id           INTEGER PRIMARY KEY,
    document_id  INTEGER NOT NULL REFERENCES reference_documents(id),
    revision     INTEGER NOT NULL,
    sha256       TEXT NOT NULL UNIQUE,
    media_type   TEXT NOT NULL,
    page_count   INTEGER,
    local_path   TEXT,
    metadata_json TEXT,
    imported_utc TEXT NOT NULL,
    UNIQUE (document_id, revision)
);

CREATE TABLE case_study_event_links (
    id                 INTEGER PRIMARY KEY,
    case_study_id      INTEGER NOT NULL REFERENCES case_studies(id),
    catalog_event_id   INTEGER REFERENCES catalog_events(id),
    external_identifier TEXT NOT NULL,
    relationship       TEXT NOT NULL,
    reviewed_note      TEXT,
    sort_order         INTEGER NOT NULL DEFAULT 0,
    source_document_id INTEGER REFERENCES reference_documents(id),
    UNIQUE (case_study_id, external_identifier)
);

CREATE TABLE case_study_document_links (
    id             INTEGER PRIMARY KEY,
    case_study_id  INTEGER NOT NULL REFERENCES case_studies(id),
    document_id    INTEGER NOT NULL REFERENCES reference_documents(id),
    relationship   TEXT NOT NULL,
    reviewed_note  TEXT,
    UNIQUE (case_study_id, document_id, relationship)
);

CREATE TABLE case_study_phases (
    id                   INTEGER PRIMARY KEY,
    case_study_id        INTEGER NOT NULL REFERENCES case_studies(id),
    label                TEXT NOT NULL,
    start_utc            TEXT NOT NULL,
    end_utc              TEXT NOT NULL,
    start_precision      TEXT NOT NULL,
    end_precision        TEXT NOT NULL,
    description          TEXT NOT NULL,
    source_document_id   INTEGER NOT NULL REFERENCES reference_documents(id),
    source_page_or_section TEXT NOT NULL,
    review_status        TEXT NOT NULL,
    sort_order           INTEGER NOT NULL DEFAULT 0,
    UNIQUE (case_study_id, sort_order)
);

CREATE TABLE case_study_analysis_links (
    id            INTEGER PRIMARY KEY,
    case_study_id INTEGER NOT NULL REFERENCES case_studies(id),
    run_id        INTEGER NOT NULL REFERENCES analysis_runs(id),
    role          TEXT NOT NULL,
    reviewed_note TEXT,
    UNIQUE (case_study_id, run_id, role)
);

CREATE TABLE case_study_claims (
    id                    INTEGER PRIMARY KEY,
    case_study_id         INTEGER NOT NULL REFERENCES case_studies(id),
    claim_type            TEXT NOT NULL,
    claim_text            TEXT NOT NULL,
    qualification         TEXT,
    source_document_id    INTEGER NOT NULL REFERENCES reference_documents(id),
    source_page_or_section TEXT NOT NULL,
    review_status         TEXT NOT NULL,
    time_or_phase         TEXT,
    observability         TEXT NOT NULL,
    observability_rationale TEXT NOT NULL,
    sort_order            INTEGER NOT NULL DEFAULT 0,
    UNIQUE (case_study_id, sort_order)
);

CREATE TABLE case_study_targets (
    id                        INTEGER PRIMARY KEY,
    case_study_id             INTEGER NOT NULL REFERENCES case_studies(id),
    source_label              TEXT NOT NULL,
    role_in_report            TEXT NOT NULL,
    candidate_org_identity    TEXT,
    candidate_origin_asns_json TEXT,
    candidate_predicate       TEXT,
    historical_validity_status TEXT NOT NULL,
    provenance                TEXT,
    research_status           TEXT NOT NULL,
    reviewed_note             TEXT,
    sort_order                INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE case_study_analysis_plans (
    id             INTEGER PRIMARY KEY,
    case_study_id  INTEGER NOT NULL REFERENCES case_studies(id),
    horizon_json   TEXT NOT NULL,
    plan_json      TEXT NOT NULL,
    status         TEXT NOT NULL,
    created_utc    TEXT NOT NULL,
    UNIQUE (case_study_id)
);

CREATE TABLE run_transitions (
    id                    INTEGER PRIMARY KEY,
    run_id                INTEGER NOT NULL REFERENCES analysis_runs(id),
    seq                   INTEGER NOT NULL,
    kind                  TEXT NOT NULL,
    occurred_utc          TEXT NOT NULL,
    run_phase             TEXT NOT NULL,
    collector             TEXT NOT NULL,
    peer_ip               TEXT NOT NULL,
    prefix                TEXT NOT NULL,
    path_id               INTEGER,
    material_path_changed INTEGER NOT NULL DEFAULT 0,
    communities_changed   INTEGER NOT NULL DEFAULT 0,
    announced             INTEGER NOT NULL DEFAULT 0,
    withdrawn             INTEGER NOT NULL DEFAULT 0,
    observation_id        INTEGER,
    archive_sha256        TEXT,
    UNIQUE (run_id, seq)
);

CREATE INDEX idx_cs_links_event ON case_study_event_links(case_study_id, sort_order);
CREATE INDEX idx_cs_links_event_lookup ON case_study_event_links(catalog_event_id);
CREATE INDEX idx_cs_docs_case ON case_study_document_links(case_study_id);
CREATE INDEX idx_cs_phases_case ON case_study_phases(case_study_id, sort_order);
CREATE INDEX idx_cs_links_run ON case_study_analysis_links(case_study_id);
CREATE INDEX idx_cs_links_run_lookup ON case_study_analysis_links(run_id);
CREATE INDEX idx_cs_claims_case ON case_study_claims(case_study_id, sort_order);
CREATE INDEX idx_cs_targets_case ON case_study_targets(case_study_id, sort_order);
CREATE INDEX idx_doc_revisions_doc ON document_revisions(document_id, revision);
CREATE INDEX idx_run_transitions_run ON run_transitions(run_id, occurred_utc);
"#;

/// V3: research-progress columns on analysis targets.
/// Research state is applied by the reviewed apply-research flow; these are
/// audit fields for the documented research-field mutation exception.
const V3: &str = r#"
ALTER TABLE case_study_targets ADD COLUMN research_updated_utc TEXT;
ALTER TABLE case_study_targets ADD COLUMN path_predicate_status TEXT;
"#;

/// V4: corpus discovery + per-fetch provenance.
///
/// `ticket_discoveries` records how each ticket identifier entered the
/// corpus (analyst seed, document reference, description reference,
/// public search result, case-study reference) with its discovery
/// provenance. `snapshot_fetches` records one row PER FETCH attempt with
/// the HTTP metadata; `event_snapshots` stays pure content-addressed
/// immutability (a 304 or unchanged payload inserts a fetch row with
/// `snapshot_id` NULL — no duplicate snapshot is created).
const V4: &str = r#"
CREATE TABLE ticket_discoveries (
    id                 INTEGER PRIMARY KEY,
    source_kind        TEXT NOT NULL,
    external_id        TEXT NOT NULL,
    provenance         TEXT NOT NULL,
    source_snapshot_id INTEGER REFERENCES event_snapshots(id),
    source_document_id INTEGER REFERENCES reference_documents(id),
    discovered_at      TEXT NOT NULL,
    status             TEXT NOT NULL DEFAULT 'Pending',
    UNIQUE (source_kind, external_id, provenance)
);

CREATE TABLE snapshot_fetches (
    id                    INTEGER PRIMARY KEY,
    event_id              INTEGER NOT NULL REFERENCES catalog_events(id),
    sync_run_id           INTEGER NOT NULL REFERENCES catalog_sync_runs(id),
    fetched_at            TEXT NOT NULL,
    source_url            TEXT NOT NULL,
    http_status           INTEGER NOT NULL,
    content_type          TEXT,
    etag                  TEXT,
    last_modified         TEXT,
    acquisition_method    TEXT NOT NULL,
    retry_count           INTEGER NOT NULL DEFAULT 0,
    snapshot_id           INTEGER REFERENCES event_snapshots(id),
    conditional_requested INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX idx_discoveries_status ON ticket_discoveries(source_kind, status);
CREATE INDEX idx_discoveries_id ON ticket_discoveries(external_id);
CREATE INDEX idx_fetches_event ON snapshot_fetches(event_id, fetched_at);
CREATE INDEX idx_fetches_sync ON snapshot_fetches(sync_run_id);
"#;

/// V5: ticket relationship graph.
///
/// Edges retain their provenance: the snapshot or document that asserted
/// them, the evidence kind (explicit text vs derived overlap), and a
/// review status. Derived edges stay visibly distinct from explicit
/// ones. `to_event_id` is NULL until the external identifier resolves to
/// a catalog event; SQLite treats NULLs as distinct in UNIQUE, so the
/// dedup index uses COALESCE to keep imports idempotent.
const V5: &str = r#"
CREATE TABLE ticket_relationships (
    id                 INTEGER PRIMARY KEY,
    from_event_id      INTEGER NOT NULL REFERENCES catalog_events(id),
    to_event_id        INTEGER REFERENCES catalog_events(id),
    to_external_id     TEXT NOT NULL,
    relationship_kind  TEXT NOT NULL,
    evidence_kind      TEXT NOT NULL,
    source_snapshot_id INTEGER REFERENCES event_snapshots(id),
    source_document_id INTEGER REFERENCES reference_documents(id),
    reviewed_status    TEXT NOT NULL DEFAULT 'Unreviewed',
    note               TEXT,
    created_utc        TEXT NOT NULL
);

CREATE UNIQUE INDEX uq_relationship_dedup ON ticket_relationships(
    from_event_id, to_external_id, relationship_kind, evidence_kind,
    COALESCE(source_snapshot_id, 0), COALESCE(source_document_id, 0)
);
CREATE INDEX idx_rel_from ON ticket_relationships(from_event_id);
CREATE INDEX idx_rel_to ON ticket_relationships(to_event_id);
CREATE INDEX idx_rel_external ON ticket_relationships(to_external_id);
"#;

/// V6: candidate incident groups.
///
/// Groups are suggestions with categorical confidence; they never
/// replace individual CatalogEvents. The evidence fingerprint makes
/// regeneration idempotent and keeps rejected groups suppressed until
/// the evidence actually changes.
const V6: &str = r#"
CREATE TABLE incident_group_candidates (
    id                   INTEGER PRIMARY KEY,
    label                TEXT NOT NULL,
    member_ids_json      TEXT NOT NULL,
    evidence_json        TEXT NOT NULL,
    confidence           TEXT NOT NULL,
    review_status        TEXT NOT NULL DEFAULT 'Unreviewed',
    evidence_fingerprint TEXT NOT NULL UNIQUE,
    created_utc          TEXT NOT NULL,
    updated_utc          TEXT NOT NULL
);

CREATE INDEX idx_groups_confidence ON incident_group_candidates(confidence);
"#;

/// V7: reviewed ticket interpretations.
///
/// A reviewed interpretation is analyst-reviewed case-study context for
/// one catalog ticket. It is stored SEPARATELY from the source snapshot:
/// `event_snapshots.raw_payload`/`normalized_json` are never modified by
/// review. Every interpretation field carries per-field provenance; a
/// value inferred from a reference document (e.g. the AAR) must cite that
/// document (`source_document_id`). Missing source fields stay missing —
/// they are never backfilled without cited provenance.
const V7: &str = r#"
CREATE TABLE ticket_reviews (
    id                        INTEGER PRIMARY KEY,
    catalog_event_id          INTEGER NOT NULL REFERENCES catalog_events(id),
    external_id               TEXT NOT NULL,
    reviewed_roles_json       TEXT NOT NULL,
    entity_labels_json        TEXT NOT NULL,
    linked_change_ids_json    TEXT NOT NULL,
    analysis_applicability    TEXT NOT NULL,
    applicability_rationale   TEXT NOT NULL,
    relationship_to_case_study TEXT NOT NULL,
    review_status             TEXT NOT NULL,
    reviewer                  TEXT NOT NULL,
    reviewed_at               TEXT NOT NULL,
    provenance_json           TEXT NOT NULL,
    source_document_id        INTEGER REFERENCES reference_documents(id),
    UNIQUE (catalog_event_id)
);

CREATE INDEX idx_reviews_external ON ticket_reviews(external_id);
"#;

/// V8: lifecycle timestamps on stream summaries.
///
/// Per-stream first-change and restoration timestamps come from the
/// immutable lifecycle.json evidence (the analysis artifacts), so the
/// workbench can render change/restoration intervals even for runs whose
/// transition index is absent or bounded. Timestamps are evidence, never
/// interpolated.
const V8: &str = r#"
ALTER TABLE stream_lifecycle_summaries ADD COLUMN first_change_utc TEXT;
ALTER TABLE stream_lifecycle_summaries ADD COLUMN restoration_time_utc TEXT;
"#;

/// V9 — observed peer-session metadata + run classification.
///
/// `observer_session_metadata` records the OBSERVED peer ASN per
/// (collector, peer IP, address family) from baseline RIB evidence,
/// time-scoped by the RIB timestamp. It is an observed protocol fact,
/// distinct from reviewed organization labels. `analysis_runs.
/// classification` labels a run's role for its ticket relationship
/// (e.g. "primary" vs "supporting-re-plane").
const V9: &str = r#"
CREATE TABLE observer_session_metadata (
    id             INTEGER PRIMARY KEY,
    source_family  TEXT NOT NULL,
    collector      TEXT NOT NULL,
    peer_ip        TEXT NOT NULL,
    address_family TEXT NOT NULL,
    peer_asn       INTEGER NOT NULL,
    valid_from     TEXT NOT NULL,
    valid_to       TEXT,
    source_archive TEXT NOT NULL,
    source_sha256  TEXT NOT NULL,
    UNIQUE (collector, peer_ip, address_family, peer_asn, source_archive)
);
CREATE INDEX idx_session_metadata_lookup ON observer_session_metadata(collector, peer_ip);
ALTER TABLE analysis_runs ADD COLUMN classification TEXT;
"#;
