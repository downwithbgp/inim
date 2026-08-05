# inim — invariant register (as-built)

Status: current normative documentation, recovered at commit `92f83d8`.
Every invariant is the result of a reconstruction + falsification pass
(see [the dated audit](../audits/2026-08-wirthian-design-recovery.md)).

Status vocabulary:

- **enforced** — a schema constraint, service-layer check, or test suite
  prevents the violation (a single test is not enough; see the
  falsification column).
- **partially enforced** — enforced on the main path but an alternate
  path exists.
- **assumed** — required for correctness but not enforced anywhere.
- **claimed** — stated in documentation/comments/names but not
  independently established.
- **unknown** — insufficient evidence.

Falsification attempt column records the concrete check or counterexample
from the session-56 falsification pass.

---

## Identity

| ID | Statement | Status | Enforcement | Falsification attempt |
|----|-----------|--------|-------------|----------------------|
| ID-1 | Route identity is ADD-PATH aware: `RouteKey = (collector, peer_ip, prefix, path_id: Option<u32>)` | enforced | `RouteKey::with_path_id` used at cohort freeze (`src/cohort.rs`), lifecycle (`src/lifecycle.rs`), caches (`src/derived_cache.rs`); tests `derived_cache.rs:1025,1246,1308` | survived; no prefix-only key in reconstruction paths |
| ID-2 | Aggregate identity `ObserverPrefixKey` (no path_id) is distinct from route-instance identity | enforced | `ObserverPrefixKey` type (`src/domain/route.rs`); stream summaries aggregate at this level | survived |
| ID-3 | Catalog event identity is `(source_kind, external_id)` | enforced | `UNIQUE (source_kind, external_id)` on `catalog_events` (`src/catalog/migrations.rs`) | survived |
| ID-4 | Source snapshot identity is `(event_id, content_sha256)` | enforced | `UNIQUE (event_id, content_sha256)` on `event_snapshots` | survived |
| ID-5 | Manifest revision identity is the payload `sha256` | enforced | `sha256 TEXT NOT NULL UNIQUE` on `manifest_revisions` | survived |
| ID-6 | Plan revision identity is the canonical plan hash | enforced | `sha256 TEXT NOT NULL UNIQUE` on `analysis_plans`; `canonical_plan_hash` (`src/catalog/jobs/plan.rs`) | survived; hash covers execution fields, ignores generated timestamps/labels (tests `plan_hash_is_deterministic`, `plan_hash_normalizes_collector_order`) |
| ID-7 | Run identity is `(plan_id, started_at)` | enforced | `UNIQUE (plan_id, started_at)` on `analysis_runs` | survived |
| ID-8 | Artifact identity is `(run_id, relative_path)` | enforced | `UNIQUE (run_id, relative_path)` on `analysis_artifacts` | survived |
| ID-9 | Job identity is the job id string; retry creates a new job linked via `original_job_id` | enforced | `retry()` inserts a new row, never mutates the old (`src/catalog/jobs/service.rs`); tests `service.rs:1575,1803,2122` | survived |
| ID-10 | A route-instance withdrawal does not imply observer-prefix absence | enforced | lifecycle absence requires loss of the final instance (`src/lifecycle.rs`); tests `nonfinal_instance_loss_does_not_make_withdrawn_lifecycle`, `cohort.rs:360` | survived |

## Provenance

| ID | Statement | Status | Enforcement | Falsification attempt |
|----|-----------|--------|-------------|----------------------|
| PR-1 | Source snapshots are immutable | enforced (rows); partially (event row) | no UPDATE/DELETE on `event_snapshots`; `UNIQUE (event_id, content_sha256)`; inserts dedupe by sha (`src/catalog/store.rs`); test `ticket_edit_creates_new_snapshot` (`src/catalog/sync.rs`) | survived; caveat: `catalog_events.last_seen` is updated by `upsert_event` |
| PR-2 | Reviewed interpretation never alters source bytes | enforced | import writes DB rows only (`src/catalog/import.rs`); test `reviewed_interpretation_does_not_modify_source_snapshot` (`src/catalog/review.rs`) | survived |
| PR-3 | Plan hash covers all execution-relevant fields | enforced | `canonical_plan_hash` (`src/catalog/jobs/plan.rs`) + tests `plan_hash_changes_for_execution_field`, `plan_hash_ignores_generated_timestamp` | survived |
| PR-4 | Published artifact bytes match the recorded SHA-256 | partially enforced | import rejects hash mismatch (`tests/import.rs artifact_hash_mismatch_is_rejected`); run page re-verifies; not enforced on every serving path | survived (see AR-2 for resolver divergence) |
| PR-5 | Cutoff (analysis end) has reviewed provenance | partially enforced | meta sidecar carries `fetched_at_utc`; absence yields a generic fallback sentence (`src/catalog/web/view.rs`) | narrowed: `analysis_end_utc` without provenance is representable |
| PR-6 | Open events require an explicit analysis cutoff with provenance | enforced | `Manifest::validate` rejects open manifests without `analysis_end_utc` (`src/manifest.rs`); `build_plan_record` Blocks (`MissingAnalysisEndForOpenTicket`) (`src/catalog/import.rs`); queue and worker require the cutoff regardless of any declared end (`src/catalog/jobs/plan.rs`, `src/worker.rs`); import requires recorded provenance (sidecar `cutoff_provenance` or analyst note); cutoff participates in the canonical plan hash | resolved in Session 57: tests `open_event_ready_plan_requires_cutoff`, `open_event_ready_plan_requires_cutoff_provenance`, `worker_missing_cutoff_fails_loudly`, `source_fetch_time_not_implicitly_analysis_cutoff`, `cutoff_participates_in_plan_hash_when_semantic` |

## Temporal consistency

| ID | Statement | Status | Enforcement | Falsification attempt |
|----|-----------|--------|-------------|----------------------|
| TC-1 | Event baseline is distinct from pre-finding state | enforced | both are distinct fields/labels in presentation (`EventBaseline`, `Pre-finding` in `comparison_states`, `src/catalog/web/path_diagram.rs`) | survived |
| TC-2 | Analysis-final state is not assumed to equal baseline | enforced | final state is recorded separately; test `final_path_not_assumed_baseline` (`src/catalog/web/path_diagram.rs`) | survived |
| TC-3 | Zero is distinct from not-applicable | enforced (Option types) | `Option<DateTime>`, `Option<f64>` throughout lifecycle; absent vs zero distinguished in renderers | survived |
| TC-4 | Run staleness never invalidates an old run | enforced | `CatalogStatus::Stale` derived, never mutates runs (`src/catalog/status.rs`) | survived |
| TC-5 | Gap/unknown continuity suppresses strong verdicts | enforced | continuity gate in `derive_verdict` runs BEFORE result derivation from finding cardinality (`src/assess.rs`); an empty finding set cannot bypass failed continuity | resolved in Session 57: tests `continuity_failure_precedes_empty_finding_fallback`, `empty_findings_do_not_imply_no_change_without_continuity`, `continuity_gate_decision_table` |

## Project scope

| ID | Statement | Status | Enforcement | Falsification attempt |
|----|-----------|--------|-------------|----------------------|
| SC-1 | Project-scope exclusion does not alter canonical evidence | enforced | scope only filters views/queries; exclusion never rewrites artifacts (`src/catalog/scope.rs`, `src/catalog/web/view.rs`) | survived |
| SC-2 | Excluded events are hidden from default web/API views | enforced | view-layer scope filters + `demo verify` checks excluded events absent from demo | survived (experiment: excluded count 0 in demo audit) |
| SC-3 | Scope is rechecked before source access on every first-party execution path | enforced | worker recheck after claim (`src/worker.rs`); queue-time and retry checks (`src/catalog/jobs/plan.rs`, `src/catalog/jobs/service.rs`); standalone `analyze` applies `analyze_scope_block` before any planning or source access (`src/main.rs`) | resolved in Session 57: tests `standalone_analyze_scope_boundary_is_explicit`, `project_scope_checked_before_network_access_where_applicable` |
| SC-4 | An excluded plan cannot be queued | enforced | queue validates plan hash against scope (`src/main.rs`, `src/catalog/jobs/plan.rs`) | survived |
| SC-5 | Scope matching is exact-normalized | enforced | `normalize_exact` (`src/catalog/scope.rs`) | survived |

## Route reconstruction

| ID | Statement | Status | Enforcement | Falsification attempt |
|----|-----------|--------|-------------|----------------------|
| RR-1 | Cohort admission requires a baseline instance matching target origin AND transit predicate | enforced | `freeze_cohort` (`src/cohort.rs`) | survived |
| RR-2 | No qualifying cohort cannot produce lifecycle counts | enforced | zero retained collectors → insufficient-visibility artifact set with empty lifecycles (`src/orchestrate.rs`) | survived |
| RR-3 | Mixed keyed/unkeyed ADD-PATH encoding suppresses strong stream-level conclusions | enforced | `add_path_ambiguous` flag (`src/lifecycle.rs`) | survived |
| RR-4 | Stream absence requires loss of the final route instance | enforced | lifecycle withdrawal computation (`src/lifecycle.rs`); tests listed under ID-10 | survived |
| RR-5 | Duplicate/unchanged observations are suppressed | enforced | `TransitionKind::Duplicate` classification in `tokenize::diff` | survived |

## Findings and results

| ID | Statement | Status | Enforcement | Falsification attempt |
|----|-----------|--------|-------------|----------------------|
| FR-1 | Observed result projection never contains expectation wording | enforced | `ObservedResultKind::human_label` (`src/domain/assessment.rs`) + negative tests; run API and case-study API expose structured `observed_result` and `expectation_assessment`; an unrecognized stored value projects as a neutral label, never verbatim (`src/catalog/web/view.rs`, `src/catalog/web/api.rs`) | resolved in Session 57: the raw `verdict`/`assessment` fields remain only as documented legacy fields; tests `api_exposes_structured_observed_result`, `legacy_verdict_does_not_override_current_projection` |
| FR-2 | Insufficient visibility is distinct from no-change | enforced | distinct enum variants `InsufficientQualifyingVisibility` vs `NoRouteStateChangeObserved` | survived (see TC-5 for a derivation-path overlap) |
| FR-3 | Completed job does not imply route change | enforced | job state orthogonal to outcome; test `completed_insufficient_visibility_is_not_failed_job` | survived |
| FR-4 | Target visibility is distinct from relationship visibility | enforced | `RelationshipView.observed` hardcoded false; stream matches counted separately (`src/catalog/web/view.rs`) | survived |
| FR-5 | Direct peer session is distinct from AS-in-path evidence | enforced | `DirectPeerToNamedPlane` vs `IndirectPathViaNamedPlane` (`src/catalog/workbench.rs`); `observation_kind` derived from peer ASN membership | survived |
| FR-6 | Finding selection is deterministic | enforced | `select_principal_findings` sorts; tests assert stable selection | survived |

## Jobs and execution

| ID | Statement | Status | Enforcement | Falsification attempt |
|----|-----------|--------|-------------|----------------------|
| JB-1 | Job state transitions are explicit and forward-only | enforced | `legal_transition` + `stage_advance` (`src/catalog/jobs/mod.rs`); test `illegal_state_transition_is_rejected` | survived |
| JB-2 | One worker claims a job at a time | enforced | transactional claim `BEGIN IMMEDIATE` (`src/catalog/jobs/service.rs`); test `two_workers_cannot_claim_same_job` | survived |
| JB-3 | Stale leases are detected, not silently resumed | enforced | `mark_stale_leases`; tests `expired_lease_is_detected_and_not_auto_resumed` | survived |
| JB-4 | Retry never mutates the failed/cancelled job | enforced | `retry()` (see ID-9) | survived |
| JB-5 | A cancelled job is immutable and retryable | enforced | `is_retryable` (`src/catalog/jobs/mod.rs`); tests | survived |
| JB-6 | Job state, execution stage, analysis outcome, observed result, and expectation assessment are separate axes | enforced | distinct enums/columns; module doc `src/catalog/jobs/mod.rs` | survived |

## Artifacts and publication

| ID | Statement | Status | Enforcement | Falsification attempt |
|----|-----------|--------|-------------|----------------------|
| AR-1 | Artifact listing and artifact access share one resolver | enforced | `resolve_artifact` is the single resolver (`src/catalog/artifact_path.rs`); workbench coverage, demo verifier, run page, and orphan audit all use it or the shared `is_safe_relative_path` primitive | resolved in Session 57: tests `all_artifact_consumers_agree_on_validity`, `workbench_and_demo_resolver_equivalent`, `git_checkout_and_packaged_source_resolver_equivalent` |
| AR-2 | Artifact paths remain inside the configured root | enforced | `is_safe_relative_path` is the single lexical containment primitive (rejects empty, absolute, parent traversal, drive-letter/UNC prefixes, backslash separators); `resolve_artifact` adds canonical containment for existing candidates (a symlink escaping the root is not served); document serving uses the same primitive (`src/catalog/artifact_path.rs`, `src/catalog/web/view.rs`) | resolved in Session 57: tests `artifact_relative_path_resolves_inside_root`, `artifact_parent_traversal_rejected`, `artifact_absolute_path_rejected`, `artifact_symlink_escape_rejected`, `missing_artifact_distinct_from_invalid_artifact_path` |
| AR-3 | A staged run is not visible until publication | enforced | staging under `data/jobs/<job>/staging`; test `incomplete_stage_is_not_visible_as_run` (`src/catalog/jobs/publish.rs`) | survived |
| AR-4 | Publication is idempotent for the same job | enforced | test `publication_is_idempotent_for_same_job` | survived |
| AR-5 | Catalog–filesystem divergence is detectable | enforced | `reconcile_orphans` reports missing/orphaned entries (`src/catalog/jobs/publish.rs`) | survived (experiment: orphan dirs reported, not auto-deleted) |
| AR-6 | The catalog-relative path stored in the DB is the authority for locating the artifact | enforced | import rejects absolute/parent-relative paths | survived |

## Presentation and evaluation

| ID | Statement | Status | Enforcement | Falsification attempt |
|----|-----------|--------|-------------|----------------------|
| PV-1 | HTTP GET does not mutate logical state | enforced | all mutations are POST and gated (`src/catalog/web/mod.rs`, `src/catalog/web/job_handlers.rs`); experiment: DB hash unchanged after GET browsing | survived (with WAL sidecar caveat, see PV-2) |
| PV-2 | Read-only serving does not modify the demo database | partially enforced | row content unchanged (experiment); catalog opens read-write WAL so `-wal`/`-shm` sidecars appear | narrowed |
| PV-3 | Answer-key generation is deterministic | enforced | generator has no randomness/timestamps; CI drift check; experiment byte-identical | survived |
| PV-4 | The fabric diagram contains only reviewed attached networks | enforced | `FabricView` built only from `interconnection_context.attachments` (`src/catalog/web/view.rs`); tests `fabric_diagram_contains_no_fabric_asn`, `taxonomy_tests` | survived |
| PV-5 | Test equipment and other non-attached classes cannot enter the fabric or AS-path diagrams | enforced | separate reviewed lists; single production call site | survived |
| PV-6 | Diagrams are presentation projections, not canonical evidence | enforced | diagrams link to evidence refs; comment `src/catalog/web/path_diagram.rs` | survived |
| PV-7 | Source mention does not imply reviewed attachment | enforced | attachments only from the reviewed `attachments` array; no promotion path | survived |

## Previously suppressed claims

- "Artifact listing and access use the same resolver" — contradicted in
  Session 56; **resolved in Session 57** (AR-1 is now enforced; all
  consumers use the shared resolver or the containment primitive).
- "Gap/unknown continuity always suppresses strong verdicts" —
  contradicted in Session 56; **resolved in Session 57** (TC-5 is now
  enforced; the continuity gate runs before the empty-finding fallback).
- "Read-only browsing leaves the database byte-identical" — **narrowed**:
  logical rows unchanged, WAL sidecars created (PV-2); remains open
  (F-7, out of implementation scope for Session 57).

## Invariant counts (at the session-57 closing commit)

- Enforced: 53
- Partially enforced: 3
- Assumed: 0
- Claimed: 0
- Unknown: 0
- Previously suppressed (now resolved): 2
- Total table rows: 56
