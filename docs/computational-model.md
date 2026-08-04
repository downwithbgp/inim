# inim — computational model (as-built)

Status: current normative documentation, recovered from implementation at
commit `92f83d8` by the Wirthian design-recovery audit
([dated audit](audits/2026-08-wirthian-design-recovery.md)).
Labels: OBSERVED (code/schema/test), INFERRED (strong support, not
enforced), CLAIMED (documented, not established), UNKNOWN.

## What inim is

inim is a single-binary Rust application (one library crate, one `inim`
binary) that explains externally visible BGP route-state changes around
operator-declared network events. It is a local analysis and review
system: it never measures traffic or service state, never contacts the
event source at analysis time, and it publishes its conclusions as
immutable per-run artifacts plus derived catalog projections.

## Principal inputs

1. **Source records** — external event bytes: GRNOC public task-viewer
   records, Internet2 ticket fixtures, tracked immutable source snapshots
   (`<case-study>/<EVENT>.source.json` + optional `.meta.json`).
2. **Reviewed interpretation** — reviewed manifest revisions
   (`manifests/*.json`, schema v2), ticket reviews, case-study data files
   (`case-studies/*/case-study.json`), network profiles
   (`src/profiles/`), project-scope policy (`config/project-scope.toml`).
3. **Public BGP archives** — RouteViews and RIPE RIS RIB/UPDATE archives,
   acquired by the worker at execution time (never at review time).
4. **Evaluation material** — scenario manifests
   (`evaluation/scenarios.toml`), the deterministic demo manifest, and
   task documents.

## Principal data structures

The complete catalog is in [design/data-structures.md](design/data-structures.md).
The central ones:

- `RouteObservation` / `RouteKey` (collector, peer IP, prefix, optional
  path ID) — normalized BGP observation and ADD-PATH-aware route-instance
  identity (`src/domain/observation.rs`, `src/domain/route.rs`).
- `FrozenCohort` — baseline observer-prefix admission
  (`src/cohort.rs`).
- `StreamLifecycle` — per-observer-prefix lifecycle (`src/lifecycle.rs`).
- `AnalysisPlan` / `ArchivePlan` — pre-execution plans (`src/plan.rs`,
  `src/catalog/archive_plan.rs`).
- `AnalysisJob` / `JobState` — durable execution state
  (`src/catalog/jobs/mod.rs`).
- `Verdict` / `ObservedResultKind` / `ExpectationAssessmentKind` —
  result vocabulary (`src/domain/assessment.rs`).
- `AnalysisRun` / `AnalysisArtifact` — published evidence records
  (SQLite tables `analysis_runs`, `analysis_artifacts`).
- `RoutingFinding` — operator-facing presentation model
  (`src/catalog/workbench.rs`).

## Transformations

The pipeline is described in [design/algorithms.md](design/algorithms.md).
The main transformations:

1. **Ingest** — MRT elements → `RouteObservation` (the only module that
   imports `bgpkit-parser`; `src/ingest/mod.rs`).
2. **Preflight** — RIB observations → per-collector frozen cohort
   (`src/orchestrate.rs` phase A, `src/cohort.rs`).
3. **Reconstruct** — observations → route-instance state → transitions
   (`src/routes.rs`), then observer-prefix aggregation → `StreamLifecycle`
   (`src/lifecycle.rs`).
4. **Group** — transitions → semantic waves (`src/waves.rs`) and
   operator findings (`select_principal_findings`,
   `src/catalog/workbench.rs`).
5. **Derive** — transitions/lifecycles + reviewed expectation → verdict,
   observed result, expectation assessment (`src/assess.rs`,
   `src/domain/assessment.rs`).
6. **Publish** — staged outputs → validated immutable run directory →
   catalog rows (`src/catalog/jobs/publish.rs`).
7. **Project** — catalog + evidence → workbench/event/run pages,
   path/fabric diagrams, API responses (`src/catalog/web/*`), and the
   evaluation answer key (`scripts/build-evaluation-answer-key.py`).

## What is persisted

- **SQLite catalog** (`inim catalog init`): events, snapshots, manifest
  revisions, plans, runs, artifacts, stream/wave summaries, case studies,
  tickets, relationships, reviews, jobs, heartbeats. Schema migrations
  via `PRAGMA user_version` (`src/catalog/migrations.rs`).
- **Immutable run directories** on the filesystem under a catalog root
  (`data/runs/<job-id>/<event>/…` for worker publication; reviewed
  evidence under `case-studies/*/out` and `case-studies/*/pilot/out` are
  tracked in Git).
- **Derived caches** (gitignored): RIB/UPDATE caches and source
  extractions under a cache root (`src/derived_cache.rs`,
  `src/catalog/source_extract.rs`).

## What is immutable

- Source snapshots (append-only rows keyed by content SHA-256;
  `UNIQUE (event_id, content_sha256)`).
- Reviewed manifest revisions (keyed by `sha256`, `UNIQUE`).
- Analysis plan revisions (keyed by `sha256`, `UNIQUE`).
- Published analysis runs and their artifact rows (immutable by
  convention; import rejects hash mismatches and conflicting immutables).
- Completed / Cancelled / Failed jobs (mutation is rejected by the
  service layer; retry creates a new job linked via `original_job_id`).

## External effects

- Filesystem writes: run publication, staging, demo catalog creation,
  document import, cache writes.
- SQLite writes: catalog import, job lifecycle, sync, review.
- Network reads: archive acquisition and GRNOC sync — **execution-time
  only**, owned by the worker (`--offline` disables it).
- Subprocess: `git rev-parse --short HEAD` for import provenance.
- HTTP responses: the read-only-by-default web server
  (`inim serve`); all mutations are POST and require `--enable-writes`.
- Terminal output: CLI commands.
- No telemetry, no authentication, no external service calls except the
  two source families above.

## What inim produces

Per run: a validated immutable artifact set (report, transitions,
lifecycle, semantic waves, withdrawal audit, evidence appendix, archive
manifest, limitations) plus catalog projections and workbench pages. Per
event: a derived readiness record and, when reviewed and analyzed, an
observed result (one of route-state changes observed, no route-state
change observed, insufficient qualifying visibility, analysis incomplete)
and an expectation assessment (consistent, partially consistent, less or
more externally visible change, not assessable).

## What inim deliberately does not conclude

- It does not conclude traffic, circuit, or service impact; the scope
  statement on every observed result says observation is limited to
  externally exported BGP route state at selected public-BGP observer
  sessions.
- It does not conclude about events whose reviewed relationship is not
  directly observable in public BGP, or that lack reviewed origin
  attribution.
- It does not fabricate conclusions for excluded project-scope material.
- It does not turn infrastructure failure into a routing verdict:
  `AnalysisOutcome::Incomplete` never renders as a visibility statement.

## Program equation

> The program is fundamentally a set of **immutable evidence and
> reviewed-interpretation** data structures (source snapshots, manifest
> revisions, plans, published runs and artifacts, plus policy and
> case-study overlays), transformed by **ingestion, cohort freezing,
> route reconstruction, lifecycle and finding derivation, verdict
> derivation, publication, and catalog projection** algorithms, under
> the invariants **of identity (ADD-PATH-aware route identity), source
> and plan immutability, provenance, temporal consistency, project
> scope, observer eligibility, artifact integrity, and read-only
> presentation**, in order to produce **observer-scoped explanations of
> public BGP route-state change around operator-declared events, with
> reviewed expectations assessed and presentation kept distinct from
> evidence**.

This equation is the synthesis of the reconstruction and falsification
passes; the dated audit records the claims that did not survive.

## Reading path

Ordered by conceptual dependency (about 12 entries; symbols are current
at the pinned commit):

1. `src/domain/route.rs` — `RouteKey`, `ObserverPrefixKey`,
   `RouteTransition`, `TransitionKind` (route-instance identity).
2. `src/domain/observation.rs` — `RouteObservation`, `ObservationKind`
   (ingestion boundary).
3. `src/domain/assessment.rs` — `Verdict`, `ObservedResultKind`,
   `ExpectationAssessmentKind` (result vocabulary).
4. `src/manifest.rs` — `Manifest`, `TransitPredicateMapping` (reviewed
   interpretation, schema v2).
5. `src/plan.rs` — `AnalysisPlan`, `AnalysisBlockReason` (planning).
6. `src/catalog/archive_plan.rs` — `ArchivePlan`, `CollectorPlan`
   (archive selection).
7. `src/cohort.rs` — `FrozenCohort`, `freeze_cohort` (baseline
   admission).
8. `src/lifecycle.rs` — `StreamLifecycle`, `StreamCategory`
   (lifecycle derivation).
9. `src/catalog/jobs/mod.rs` — `JobState`, `legal_transition`
   (durable execution).
10. `src/catalog/jobs/publish.rs` — `validate_staged`,
    `publish_staged_run` (publication).
11. `src/catalog/artifact_path.rs` — `resolve_artifact` (artifact
    resolution authority).
12. `src/catalog/workbench.rs` — `RoutingFinding`,
    `select_principal_findings` (operator-facing presentation).

## Future incremental audit format (design checksum)

Documented maintenance method only — no automation in this session.

For any future commit range `A..B`, report only changes to:

- **data structures** (structs, enums, tables, JSON records, directory
  conventions);
- **semantic identity** (keys, normalization, hash inputs);
- **invariants** (additions, removals, enforcement changes);
- **algorithms** (new, renamed, reordered, complexity changes);
- **state transitions** (state sets, legal transitions, recovery);
- **effects** (new filesystem/SQLite/network/subprocess effects);
- **authority boundaries** (who writes a fact, who projects it);
- **information-loss boundaries** (where precision is discarded);
- **complexity** (cardinalities, memory, streaming behavior).

For each change record: **before**, **after**, **evidence** (path +
symbol or named test), **compatibility consequence**, **tests**.
