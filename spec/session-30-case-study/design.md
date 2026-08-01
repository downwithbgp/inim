# Session 30 — case-study layer (design)

## Current-state facts (verified)

- Catalog: `src/catalog/` — `migrations.rs` (PRAGMA user_version registry,
  `CATALOG_SCHEMA_VERSION = 1`, `MIGRATIONS: &[&str]`), `domain.rs`, `store.rs`
  (insert-only `pub fn insert_*`), `import.rs` (reads `manifests/` +
  event out dirs; 2 runs × 10 artifacts currently), `web/` (axum router in
  `mod.rs`, `AppState { db: Mutex<Connection>, catalog_root: PathBuf, ... }`).
- No per-transition artifact exists. `OutputContext.transitions` is
  `&[RouteTransition]` (domain/route.rs: `RouteTransition { key: RouteKey,
  from/to: Option<EvidencedRouteState>, kind: TransitionKind, effects:
  GenericTransitionEffects, triggering: EvidenceRef, phase }`).
- The AAR PDF is **not present on disk** (only SHA-256 d29df26a… + URL from
  the brief). `pdftotext`/`pdfinfo` exist in the environment.
- `data/` is gitignored and excluded from `cargo package`; `out/` is
  git-tracked.

## Schema — catalog migration V2 (CATALOG_SCHEMA_VERSION → 2)

New tables (all with `ON DELETE RESTRICT` FKs where referenced rows are
immutable; existing delete-rejection pattern extended):

- `case_studies` — id, slug UNIQUE, title, summary, start_utc, end_utc,
  status (`Active` | `Closed`), content_sha256 (immutability check),
  created_utc, updated_utc.
- `case_study_event_links` — case_study_id, catalog_event_id NULLABLE FK
  (NULL = unresolved document reference), external_identifier, relationship
  (generic enum), reviewed_note, sort_order, source_document_id FK →
  reference_documents.
- `reference_documents` — id, title, source_url, doc_type
  (`AfterActionReport` | `Ticket` | `Reference`), redistribution_status
  (`Redistributable` | `Restricted` | `Unknown`), publication_date,
  provenance, imported_utc.
- `document_revisions` — id, document_id FK, revision, sha256 UNIQUE,
  media_type, page_count, local_path (catalog-relative, NULL = not available
  locally), metadata_json (preserved PDF metadata), imported_utc.
- `case_study_document_links` — case_study_id, document_id FK →
  reference_documents, relationship, reviewed_note.
- `case_study_phases` — id, case_study_id, label, start_utc, end_utc,
  start_precision / end_precision (`exact` | `summarized`), description,
  source_document_id FK → reference_documents, source_page_or_section,
  review_status, sort_order.
- `case_study_analysis_links` — case_study_id, run_id FK, role
  (`PrimaryObservation` | `Supplementary`), reviewed_note.
- `case_study_claims` — id, case_study_id, claim_type (ReportedImpact |
  ReportedMechanism | ReportedTimeline | ReportedRecovery | ReportedLimitation
  | ProcessFinding), claim_text, qualification, source_document_id FK →
  reference_documents, source_page_or_section, review_status, time_or_phase,
  observability (PotentiallyVisibleInPublicBgp | IndirectlyVisible |
  NotDirectlyVisible | Unknown), observability_rationale, sort_order.
- `case_study_targets` — id, case_study_id, source_label, role_in_report,
  candidate_org_identity, candidate_origin_asns_json (may be `[]`),
  candidate_predicate, historical_validity_status, provenance,
  research_status, reviewed_note, sort_order.
- `case_study_analysis_plans` — id, case_study_id UNIQUE, horizon_json,
  plan_json, status (`Draft` | `Reviewed`), created_utc.
- `run_transitions` — id, run_id FK, seq, kind, occurred_utc, run_phase,
  collector, peer_ip, prefix, path_id, material_path_changed,
  communities_changed, announced, withdrawn, observation_id,
  archive_sha256; `UNIQUE(run_id, seq)` as a table constraint.

All FKs are `ON DELETE RESTRICT`; SQLite provides delete rejection natively
(store.rs has no DELETE statements — the new FKs extend that behavior
without code changes). Migration SQL is literal SQLite DDL.

## Transitions artifact (foundation for phase summaries)

`src/output.rs` gains `write_transitions` producing `transitions.json`
(schema v1 constant in `src/schema.rs`) with compact per-transition records
derived from `RouteTransition`: seq, kind, occurred_utc, phase,
collector/peer/prefix (from `key`), path_id, observation_id (from
`triggering`), archive_sha256, material/communities effects,
announced/withdrawn flags. `occurred_utc` = `to.state.timestamp` when
`to.state` is `Some`; for absent states (withdrawals) fall back to
`to.evidence.timestamp` (never panic on `None`). Catalog import parses it
into `run_transitions` rows for every run. Phase summaries are pure read-only
DB derivations over `run_transitions` + `stream_lifecycle_summaries` +
`semantic_wave_summaries`. Continuous semantics: transitions are assigned to
exactly one phase by `occurred_utc`; stream active-state is walked across all
phases (no baseline reset at phase boundaries). Transitions falling outside
every reviewed phase are counted in a summary-level "outside reviewed
phases" bucket and surfaced honestly (phases are not required to be
gap-free).

## CLI

- `inim catalog document import --db DB --file FILE --source-url URL
  [--title T] [--doc-type T] [--provenance S]` — sha256, media type from
  allowlist (application/pdf, text/plain, application/json, text/markdown,
  text/csv; anything else → clean error), page count + PDF metadata via
  `pdfinfo`-style best-effort parsing (no hard dependency; metadata JSON
  stored). File copied to `<catalog_root>/data/documents/<sha12>/<filename>`
  where `<filename>` is the **basename only** of the source file (path
  separators and `..` rejected — the stored relative path can never escape
  the documents directory); DB stores the catalog-relative path. Identical
  sha → idempotent; different sha → new revision (dedup by sha, distinct
  record on change).
- `inim catalog case-study import --db DB --path DIR_OR_JSON` —
  transactional, idempotent (slug+content_sha), schema-validated,
  provenance-preserving. Links existing catalog events by
  (source_kind, external_id); AAR-referenced tickets without snapshots stay
  unresolved document references. Same slug + different content sha →
  conflict error.
- `inim catalog case-study plan --db DB --slug SLUG` — computes the archive
  plan for the reviewed horizon (defaults: warmup 2 h before incident start,
  incident window, cooldown ≥ 2 h after end). **No downloads.** Expected
  archive file lists from URL patterns; sizes "where available" via broker
  query (best-effort; offline → `Unknown`); blocked targets with reasons
  (all targets Unresearched → blocked "target mapping unresolved"). Stored
  with status `Draft`.

## Web + API

Routes (all read-only; no analysis on any request path):
`GET /case-studies`, `GET /case-studies/:slug`, `GET /documents/:document_id`,
`GET /api/v1/case-studies`, `GET /api/v1/case-studies/:slug`,
`GET /api/v1/case-studies/:slug/timeline`,
`GET /api/v1/case-studies/:slug/comparison`.

Case-study detail page first screen: **What happened** / **What public BGP
showed** / **What BGP could not show** — with honest "Historical analysis not
yet executed" when no run is linked. Then: incident timeline (phases with
precision flags), related tickets (linked + unresolved refs with roles),
document links + provenance, reviewed targets (statuses visible), analysis
plans and runs, phase-conditioned BGP summaries, comparison matrix, evidence
links.

Document serving: DB record → relative-path check (reject `..`, absolute,
non-UTF8) → resolve under `catalog_root` → canonical containment check →
SHA-256 verify → media-type allowlist → inline (pdf/text) or attachment.
API never exposes `local_path` or absolute paths; no raw PDF text.

## Comparison model

Read-time derivation. For claims of the first five categories with a
time/phase anchor: pair with the linked runs' phase summaries. Labels:
`Before | During | After | Overlapping | NoObservedCounterpart |
NotDirectlyObservable | Indeterminate`. NotDirectlyVisible claims →
NotDirectlyObservable (never reported as a missed detection). No linked runs
→ Indeterminate ("analysis not yet executed"). Never `ConfirmedCause`;
interpretation text states "temporal consistency does not prove causation".

## MAN LAN data

`case-studies/manlan-2019/case-study.json` (single canonical reviewed file)
+ `README.md`. Contents per the brief: title/date/source URL/supplied SHA
(page count 15 from brief), summary, 5 reviewed phases (04:00–10:00
scheduled migration; 10:00–14:14 troubleshooting; 14:14–18:01
traffic-replication incident; 18:01–22:22 rollback/restoration; through
~22:38 closure) with `exact`/`summarized` precision flags, 12 related ticket
references with explicit generic relationships + reviewed notes, ~11
reviewed claims with qualifications + observability classification +
rationale, 10 targets all `Unresearched` with empty ASN lists, document
record (sha d29df26a…, source URL, redistribution `Unknown`). The local PDF
is not on disk: the document record is created from metadata with
`local_path NULL`; the file can be attached later via `document import`.

## Packaging / neutrality

- `data/` already excluded → local documents never packaged. `case-studies/`
  is tracked and packaged (metadata only, no PDF).
- release_test additions: package contains `case-studies/manlan-2019/`
  metadata and no `data/`; production source scan: `src/` contains none of
  the forbidden tokens.
