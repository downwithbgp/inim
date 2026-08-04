# inim — algorithm catalog (as-built)

Status: current normative documentation, recovered at commit `92f83d8`.
Algorithms are named even when distributed across modules. Classification:
**standard** (textbook), **domain adaptation** (standard adapted to BGP),
**repository-specific** (specific to inim), **distributed/unnamed**
(no single named implementation; spread across modules).

---

## A1. MRT parse and normalize

- Symbols: `ObservationStream::from_local_file`, `IngestContext`
  (`src/ingest/mod.rs`), consumed by `src/orchestrate.rs`.
- Purpose: convert bgpkit-parser `BgpElem` records into inim-native
  `RouteObservation` values at the single ingestion boundary.
- Inputs: MRT archive file path + role (RIB/UPDATE) + collector identity.
- Outputs: iterator of `RouteObservation` (with `ObservationId`,
  provenance, path_id).
- State read: none (streaming). State mutated: none.
- External effects: file read only.
- Preconditions: caller supplies role/collector (never inferred).
- Postconditions: bgpkit types do not leak beyond this module.
- Ordering: archive order index assigned by the coordinator.
- Determinism: deterministic per file.
- Idempotence: parse is re-runnable.
- Termination: per file, bounded.
- Failure behavior: `InimError` variants (decode, unsupported, missing
  baseline, discontinuity).
- Complexity: O(records); streaming.
- Classification: domain adaptation.

## A2. RIB preflight and frozen-cohort admission

- Symbols: `run_inner_impl` phase A (`src/orchestrate.rs`),
  `freeze_cohort` (`src/cohort.rs`).
- Purpose: select the best baseline RIB per collector (latest at/before
  warmup) and admit observer-prefix keys whose baseline has a
  target-origin instance satisfying the transit predicate.
- Inputs: RIB observations, `origin_asns`, `TransitPredicate`.
- Outputs: `FrozenCohort` (admitted keys + baseline instances),
  per-collector counts.
- State mutated: derived RIB cache and source extraction cache.
- Preconditions: RIB at/before warmup exists for the collector.
- Postconditions: only admitted streams enter reconstruction.
- Determinism: deterministic; `BTreeMap`/`BTreeSet` iteration.
- Complexity: O(RIB observations); in-memory materialized.
- Failure: missing RIB → collector skipped + limitation; zero retained
  collectors → `AnalysisOutcome::InsufficientVisibility`.
- Classification: repository-specific (domain adaptation of baseline
  qualification).

## A3. Derived RIB/UPDATE cache and source extraction

- Symbols: `rib_cache_key`, `load_rib_cache`, `load_update_cache`,
  `extraction_key`, `load_origin_extraction`
  (`src/derived_cache.rs`, `src/catalog/source_extract.rs`).
- Purpose: avoid re-parsing unchanged archives; reuse origin-matching
  observations across predicates.
- Inputs: archive sha256 + collector + origin set + predicate identity
  + manifest revision.
- Outputs: cached entries or miss.
- Invariants: schema versions gate format
  (`RIB_CACHE_SCHEMA_VERSION=2`, `UPDATE_CACHE_SCHEMA_VERSION=2`,
  `OBSERVATION_SCHEMA_VERSION=2`); a cache miss must not alter results.
- Caveat (falsification): extraction reuse is keyed on origin set only;
  a different predicate with the same origin set can miss
  predicate-2-only streams (cohort-completeness risk).
- Classification: repository-specific cache.

## A4. Route-state reconstruction

- Symbols: `RouteStateMachine` (`src/routes.rs`),
  `build_lifecycles`/per-instance simulation (`src/lifecycle.rs`).
- Purpose: apply observations to per-`RouteKey` state, emit transitions.
- Inputs: baseline + update observations in archive order.
- Outputs: `RouteTransition` sequence.
- State mutated: in-memory state maps, event-baseline map.
- Ordering: chronological per route instance; archive order for ties.
- Determinism: deterministic for a fixed input order.
- Correctness dependencies: ADD-PATH-aware identity (ID-1); duplicate
  suppression; one path-ID withdrawal does not mark stream absence
  (ID-10).
- Complexity: O(observations) with per-key hash lookups.
- Classification: domain adaptation.

## A5. Observer-prefix lifecycle derivation

- Symbols: `StreamLifecycle` builders (`src/lifecycle.rs`).
- Purpose: aggregate per-instance history into per-observer-prefix
  lifecycle: category, flags, first change, absence, restoration,
  cooldown, final state.
- Inputs: transitions + baseline instances + horizon (warmup/event/
  cooldown).
- Outputs: `StreamLifecycle` vector.
- Determinism: deterministic; chronological transitions retained.
- Correctness dependencies: stream absence requires final-instance loss;
  add-path ambiguity suppresses strong conclusions.
- Complexity: O(instances + transitions) per stream.
- Classification: repository-specific.

## A6. Transition diff and tokenization

- Symbols: `diff_states`, `tokenize`, `TransitionSymbol`
  (`src/tokenize.rs`).
- Purpose: classify a state change into `TransitionKind` (with
  orthogonal `GenericTransitionEffects`).
- Determinism: deterministic.
- Classification: domain adaptation.

## A7. Wave detection and motif extraction

- Symbols: `detect_waves`, `build_wave`, `sequitur_motif`
  (`src/waves.rs`, `src/sequitur/`).
- Purpose: group transitions into temporal clusters (gap threshold) and
  derive SEQUITUR motifs.
- Determinism: sorts transitions by timestamp; deterministic.
- Note: waves do NOT feed the episode presentation model
  (`src/catalog/workbench.rs`); they appear in phase summaries and
  artifacts.
- Classification: repository-specific (deterministic clustering).

## A8. Verdict and result derivation

- Symbols: `derive_verdict`, `assess` (`src/assess.rs`); projections
  `observed_result_kind`, `expectation_assessment_kind`,
  `assessment_kind` (`src/domain/assessment.rs`).
- Purpose: combine transitions/lifecycles + reviewed expectation into a
  `Verdict`, then project onto observed-result and expectation-assessment
  axes.
- Ordering: empty-transitions early return precedes the continuity gate
  — see [invariants TC-5](invariants.md) (falsification finding).
- Determinism: deterministic.
- Classification: repository-specific.

## A9. Finding grouping and principal-finding selection

- Symbols: `select_principal_findings`, `RoutingFinding` builders
  (`src/catalog/workbench.rs`).
- Purpose: group lifecycle evidence into operator-facing findings;
  select principal vs additional findings with observer-diversity
  preference.
- Determinism: deterministic (sorted selection).
- Classification: repository-specific (presentation).

## A10. Plan construction and blocking

- Symbols: `plan_from_manifest`, `plan_analysis`,
  `AnalysisPlanStatus`, `AnalysisBlockReason` (`src/plan.rs`).
- Purpose: produce a pre-execution plan or a blocker before any network
  activity.
- Blockers: `MissingReviewedEntityMapping`,
  `MissingReviewedTransitPredicate`, `MissingAnalysisEndForOpenTicket`,
  `InvalidAnalysisWindow`, `UnsupportedManifestRevision`.
- Classification: repository-specific.

## A11. Archive selection and planning

- Symbols: `build_plan_for_families`, `archive_url_for`,
  `rib_interval_secs`, `SourceFamily::{RouteViews, RipeRis}`
  (`src/catalog/archive_plan.rs`).
- Purpose: per-collector, per-family baseline RIB + optional validation
  RIB + 5-minute UPDATE sequence over the horizon; estimates bytes.
- Family differences: URL construction, RIB cadence, compression —
  encoded in `SourceFamily` helpers, not in generic code.
- Target coverage: only HistoricallyReviewed targets enter; others are
  blocked with recorded reasons.
- Determinism: schedule generated from one cadence; `BTreeSet` dedupe.
- Classification: repository-specific.

## A12. Canonical plan hashing

- Symbols: `canonical_plan_hash` (`src/catalog/jobs/plan.rs`).
- Purpose: SHA-256 over the canonical serialization of the manifest
  payload; covers execution-relevant fields, ignores generated
  timestamps and display labels; normalizes collector order.
- Tests: `plan_hash_is_deterministic`,
  `plan_hash_normalizes_collector_order`,
  `plan_hash_changes_for_execution_field`.
- Classification: repository-specific.

## A13. Durable job queue, claim, lease, cancel, retry

- Symbols: `queue`, `claim_next`, `renew_lease`, `mark_stale_leases`,
  `request_cancel`, `observe_cancel`, `retry`, `heartbeat`
  (`src/catalog/jobs/service.rs`).
- Queue: idempotency — active duplicate returns existing job.
- Claim: `BEGIN IMMEDIATE` transaction prevents double-claim; oldest
  job first.
- Lease: 90 s default; heartbeat 15 s; stale leases detected, never
  auto-resumed.
- Cancel: queued jobs cancel directly; executing jobs enter
  `CancelRequested` and observe cancellation cooperatively.
- Retry: new job with `original_job_id`, attempt+1; old job immutable.
- Tests: `two_workers_cannot_claim_same_job`,
  `worker_claims_oldest_job_deterministically`,
  `expired_lease_is_detected_and_not_auto_resumed`.
- Classification: repository-specific (SQLite-backed).

## A14. Analysis execution and archive acquisition

- Symbols: `run_inner_impl` phases B–G (`src/orchestrate.rs`),
  `LiveArchiveDiscovery` (`src/discover.rs`), `CacheScanDiscovery`
  (offline), bounded download/parse pools.
- Purpose: discover archives, cache, parse baseline + updates, freeze
  cohort, reconstruct, derive evidence, write artifacts.
- Continuity: UPDATE gaps set `any_continuity_unknown` (feeds A8).
- Cancellation: cooperative `AtomicBool` checks between stages.
- Offline mode: `--offline` + `CacheScanDiscovery` avoids network.
- Classification: repository-specific.

## A15. Artifact validation and publication

- Symbols: `write_execution_metadata`, `validate_staged`,
  `publish_staged_run`, `import_finalized_run`, `reconcile_orphans`
  (`src/catalog/jobs/publish.rs`).
- Purpose: write to `data/jobs/<job>/staging/<event>`, validate
  (required artifacts present; plan hash matches; schema versions),
  rename staging → final immutable location (same filesystem), import
  into catalog, complete job.
- Failure windows: crash between rename and import leaves an orphan
  final directory (detected by audit, not auto-repaired).
- Idempotence: publication idempotent for the same job (test).
- Classification: repository-specific.

## A16. Catalog import precedence

- Symbols: `import_repository`, `import_one`,
  `case_study_snapshot_for`, `ticket_fixture_for`
  (`src/catalog/import.rs`).
- Purpose: import tracked manifests + run outputs into the catalog.
- Snapshot precedence: (1) tracked reviewed case-study snapshot
  `<case-study>/<EVENT>.source.json` (+ meta), (2) tracked offline
  fixture, (3) manifest-derived.
- Run import: completed runs only; preflight-only dirs never imported.
- Idempotence: repeated import is idempotent (test).
- Classification: repository-specific.

## A17. Artifact resolution

- Symbols: `resolve_artifact` (`src/catalog/artifact_path.rs`).
- Purpose: resolve a catalog-relative artifact path under a root via
  conventional candidates: `<root>/<rel>`, `<root>/out/<rel>`,
  `<root>/case-studies/<slug>/out/<rel>`, `<root>/case-studies/<slug>/
  pilot/out/<rel>`; first existing wins.
- Containment: rejects absolute and parent-relative paths.
- Falsification: other modules use their own resolvers with different
  candidate sets (see [invariants AR-1](invariants.md)).
- Classification: repository-specific.

## A18. Project-scope matching

- Symbols: `ProjectScope::load`, `excluded_entity_name`,
  `excluded_asn`, `excluded_source_record`, `normalize_exact`
  (`src/catalog/scope.rs`).
- Purpose: exact-normalized matching of excluded entities/ASNs/source
  records; used by view filters, queue/worker checks, demo verify.
- Determinism: deterministic.
- Classification: repository-specific.

## A19. Readiness and status derivation

- Symbols: `derive_status` (`src/catalog/status.rs`),
  `derive_analyzability` (`src/catalog/analyzability.rs`).
- Purpose: derive `CatalogStatus` (8 values, deterministic precedence)
  and readiness (15 values) from stored inputs — never stored as truth.
- Classification: repository-specific.

## A20. Demo init and verify

- Symbols: `demo_init`, `demo_verify`, `import_pilot_runs`
  (`src/catalog/demo.rs`).
- Purpose: build a fresh deterministic offline catalog from tracked
  reviewed material only; verify expected events, artifact resolution,
  absence of exclusions and absolute paths.
- Precedence: reviewed case-study trees only; runtime `out/`/`data/`
  play no part.
- Determinism: demo-manifest has no timestamps.
- Classification: repository-specific.

## A21. Case-study projection

- Symbols: `import_case_study` (`src/catalog/case_study_import.rs`),
  `build_plan`/`apply_pilot_result` (`src/catalog/archive_plan.rs`).
- Purpose: import reviewed case-study data files, link events (never
  fabricating source snapshots), store draft archive plans and pilot
  results.
- Classification: repository-specific.

## A22. Answer-key generation

- Symbols: `scripts/build-evaluation-answer-key.py`.
- Purpose: derive evaluator answers from tracked reviewed artifacts +
  demo manifest; hard-requires artifacts; drift check in CI.
- Determinism: no randomness/timestamps; byte-deterministic (experiment).
- Classification: repository-specific.

## A23. Insufficient-visibility artifact set

- Symbols: `write_insufficient_visibility_artifacts`
  (`src/orchestrate.rs`).
- Purpose: when zero retained collectors, write the standard artifact
  shapes with empty content: `report.json`, `report.txt`,
  `limitations.json`, `archive_manifest.json`, `transitions.json`,
  `semantic_waves.json`, `lifecycle.json`, `withdrawal_audit.json`,
  `evidence_appendix.jsonl`.
- Classification: repository-specific.

---

## Distributed/unnamed algorithms

- **Event-window construction and warmup/cooldown arithmetic**: spread
  across `src/manifest.rs` (`event_window()`), `src/plan.rs`, and
  `src/catalog/archive_plan.rs`; no single named function.
- **Cross-observer aggregation**: spread across `src/waves.rs`,
  `src/catalog/observer_compare.rs`, `src/catalog/phase_summary.rs`,
  and workbench loaders.
- **Information-loss boundaries**: normalization in `src/ingest/mod.rs`,
  aggregation in `src/lifecycle.rs`, grouping in
  `src/catalog/workbench.rs` — see [the dated audit](../audits/2026-08-wirthian-design-recovery.md).

## Algorithms with significant fan-in/fan-out

- `freeze_cohort` (A2) — input to reconstruction, lifecycle, caches.
- `derive_analyzability` (A19) — reads manifests, plans, runs, reviews,
  case-study plans.
- `resolve_artifact` (A17) — read by demo verify, web run page, artifact
  audit — but not by every consumer (see AR-1).
