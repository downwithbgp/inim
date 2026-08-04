# inim catalog schema reference

Reference for maintainers. The authoritative schema is the ordered
migration list in `src/catalog/migrations.rs`; this document summarizes
the current state at the abstraction level needed to work on the
catalog. It is not a column-by-column duplicate of the migrations.

## Current schema version

**v10** (`CATALOG_SCHEMA_VERSION = 10`, registry `PRAGMA user_version`).

## Migration policy

- `src/catalog/migrations.rs` holds ordered migrations `V1..V10`;
  index `i` migrates `user_version i → i+1`.
- A fresh database applies all migrations in order; each migration runs
  inside a transaction.
- A reopened database at the current version is a no-op. A database
  **newer** than supported is rejected
  (`catalog database schema vN is newer than supported v10`).
- Old migrations are never rewritten; historical integrity is
  preserved. Schema changes add a new migration.
- Connection policy (`src/catalog/db.rs`): `PRAGMA foreign_keys = ON`,
  `PRAGMA journal_mode = WAL`, `busy_timeout` 5 s.

## Tables by migration

| Migration | Tables created | Purpose |
|---|---|---|
| V1 | `catalog_events`, `event_snapshots`, `manifest_revisions`, `analysis_plans`, `analysis_runs`, `analysis_artifacts`, `stream_lifecycle_summaries`, `semantic_wave_summaries`, `catalog_sync_runs` | source-neutral event catalog, immutable snapshots/revisions/plans/runs, artifact rows, derived summaries |
| V2 | `case_studies`, `reference_documents`, `document_revisions`, `case_study_event_links`, `case_study_document_links`, `case_study_phases`, `case_study_analysis_links`, `case_study_claims`, `case_study_targets`, `case_study_analysis_plans`, `run_transitions` | incident case-study layer (schema v2) |
| V3 | — (ALTER) | `case_study_targets.research_updated_utc`, `path_predicate_status` |
| V4 | `ticket_discoveries`, `snapshot_fetches` | corpus discovery provenance + per-fetch HTTP provenance |
| V5 | `ticket_relationships` | reviewed + derived relationship edges |
| V6 | `incident_group_candidates` | candidate incident groups |
| V7 | `ticket_reviews` | reviewed interpretation layer (roles, provenance) |
| V8 | — (ALTER) | `stream_lifecycle_summaries.first_change_utc`, `restoration_time_utc` |
| V9 | `observer_session_metadata` (+ `analysis_runs.classification`) | observed peer ASNs from baseline RIBs; run role classification |
| V10 | `analysis_jobs`, `analysis_job_events`, `worker_heartbeats` | durable job state machine, append-only job events, worker heartbeats |

## Important relationships (foreign keys)

- `event_snapshots.event_id → catalog_events.id`; snapshots are
  content-addressed (`UNIQUE (event_id, content_sha256)`).
- `manifest_revisions.event_id → catalog_events.id`,
  `manifest_revisions.snapshot_id → event_snapshots.id`;
  `sha256` is unique.
- `analysis_plans.manifest_revision_id → manifest_revisions.id`;
  `sha256` unique (plan hash).
- `analysis_runs.plan_id → analysis_plans.id`
  (`UNIQUE (plan_id, started_at)`); the run records software version,
  parser identity, cache/report schema versions, verdict, assessment.
- `analysis_artifacts.run_id → analysis_runs.id`; artifact rows store
  kind, **relative path**, media type, schema version, SHA-256, size.
- `run_transitions.run_id → analysis_runs.id` (compact transition
  index rebuilt from `transitions.json`).
- `analysis_jobs.plan_revision_id → analysis_plans.id`
  (`ON DELETE RESTRICT`); `original_job_id → analysis_jobs.id`
  (retry chain); `completed_run_id → analysis_runs.id`.
- `analysis_job_events.job_id → analysis_jobs.id`
  (`ON DELETE CASCADE`), append-only (`PRIMARY KEY (job_id, sequence)`,
  `WITHOUT ROWID`).

## Immutability boundaries

Immutability is enforced by application policy plus schema constraints,
not by SQL triggers:

- deduplication by content hash (`event_snapshots`, `manifest_revisions`,
  `analysis_plans` payload SHA-256);
- insert-only store helpers for snapshots/revisions/runs;
- import-time hash-conflict rejection (a conflicting immutable
  revision is rejected);
- `ON DELETE RESTRICT` on job references;
- `run_transitions` is a derived compact index, rebuilt from the
  immutable `transitions.json` artifact.

## Indexes

Useful for maintainers (verified by EXPLAIN QUERY PLAN in the
workbench performance work): `idx_snapshots_event`,
`idx_manifest_event`, `idx_plans_manifest`, `idx_runs_plan`,
`idx_artifacts_run`, `idx_streams_run`, `idx_waves_run`,
`idx_run_transitions_run`, `idx_jobs_active (state, requested_at)`,
`idx_jobs_plan (plan_revision_id)`, `idx_jobs_run`,
`idx_jobs_original`, `idx_worker_heartbeat`.

## Deletion policy

- Nothing is deleted automatically at runtime except job staging.
- `inim analysis-job cleanup` deletes **only** staging directories of
  terminal (`Failed`/`Cancelled`/unreferenced) jobs older than
  `--older-than` (default 7d), with path containment and a
  terminal-state re-check, and only with `--apply` (dry-run default).
  It never deletes runs, referenced artifacts, caches, or tracked
  evidence.
- Orphans (run directories or catalog runs with missing artifacts) are
  reported by `inim analysis-job audit`; they are never deleted
  automatically.

## Project-scope overlay

The project-scope policy (`config/project-scope.toml`, schema v1) is
loaded once per process through the shared service
(`src/catalog/scope.rs`) by the web app, CLI, worker, and demo.
Exclusions are applied as exact-match filters in queries (external
source ID, reviewed entity name, reviewed ASN, exact alias): excluded
events are hidden from default views and API lists, excluded plans
cannot be queued/retried, and the worker rechecks scope after claim.
The policy itself is never stored in the database, and exclusion never
deletes immutable history — the read-only `inim project-scope audit`
reports it.

## Demo import behavior

`inim demo init` builds a fresh catalog from tracked reviewed material:
manifest events, corpus snapshots, reviewed case-study metadata, and
completed run artifacts. It never queues jobs, never accesses the
network, and writes a deterministic `demo-manifest.json` next to the
database (timestamp-free; absolute paths rejected). Import precedence:
tracked case-study evidence wins over runtime stubs.

## Runtime boundary

The SQLite catalog is a **runtime** artifact. No runtime database is
tracked in Git; the crate package excludes `*.sqlite*`, `data/`,
`cache/`, and `tmp/`. Catalog databases are created locally with
`inim catalog init` or `inim demo init`.
