# inim — algorithm–data-structure matrix (as-built)

Status: current normative documentation, recovered at commit `92f83d8`.
For each major algorithm: structures read, structures mutated, structures
produced, invariants relied upon, effects, principal tests.

Legend: R = read, M = mutated, P = produced. Invariant ids refer to
[invariants.md](invariants.md).

## Source / review

| Algorithm | Structures read | M | P | Invariants relied upon | Effects | Principal tests |
|-----------|-----------------|---|----|------------------------|---------|-----------------|
| A1 MRT parse + normalize | MRT archives | — | `RouteObservation` | ID-1 | file read | `src/ingest/mod.rs` |
| A16 catalog import precedence | manifests, out dirs, snapshots, fixtures | catalog rows | event/snapshot/plan/run/artifact rows | PR-1, PR-2 | DB write, git subprocess | `import_completed_event_creates_analysis_run`, `repeated_import_is_idempotent`, `artifact_hash_mismatch_is_rejected` |
| A21 case-study projection | case-study.json, documents | catalog rows | case study, links, plans | PR-2, PV-7 | DB write | `src/catalog/case_study_import.rs` tests |

## Planning / policy

| Algorithm | Structures read | M | P | Invariants relied upon | Effects | Principal tests |
|-----------|-----------------|---|----|------------------------|---------|-----------------|
| A10 plan construction | manifest | — | `AnalysisPlan` | PR-6 | none (no network) | `src/plan.rs` |
| A11 archive selection | case study, targets, family constants | — | `ArchivePlan` | SC-* | none | `src/catalog/archive_plan.rs` |
| A12 canonical plan hashing | manifest payload | — | plan sha256 | PR-3 | none | `plan_hash_is_deterministic`, `plan_hash_normalizes_collector_order` |
| A18 project-scope matching | `config/project-scope.toml` | — | scope decision | SC-1..SC-5 | none | `project_scope_policy_test.rs`, `project_scope_enforcement_test.rs` |
| A19 readiness/status derivation | snapshots, manifests, plans, runs, reviews | — | `CatalogStatus`, `Analyzability` | TC-4 | DB read | `src/catalog/status.rs` |

## Execution / publication

| Algorithm | Structures read | M | P | Invariants relied upon | Effects | Principal tests |
|-----------|-----------------|---|----|------------------------|---------|-----------------|
| A13 job queue/claim/lease/retry | `analysis_jobs`, events | job rows | job events | JB-1..JB-6, ID-9 | DB write | `two_workers_cannot_claim_same_job`, `worker_claims_oldest_job_deterministically`, `expired_lease_is_detected_and_not_auto_resumed`, `illegal_state_transition_is_rejected` |
| A14 analysis execution | archives, caches | caches, staging | transitions/lifecycles/waves | RR-*, TC-5 | network, file write | `tests/job_workflow_tests.rs`, `tests/queued_analysis_e2e_test.rs` |
| A15 artifact validation/publication | staging dir, `analysis_runs` | catalog rows, final dir | run + artifacts | AR-3..AR-6, PR-4 | file rename, DB write | `incomplete_stage_is_not_visible_as_run`, `publication_is_idempotent_for_same_job`, `invalid_artifact_blocks_publication` |
| A23 insufficient-visibility artifact set | RIB metadata | out dir | 9-file artifact set | RR-2 | file write | `empty_preflight_returns_insufficient_visibility` |

## BGP reconstruction

| Algorithm | Structures read | M | P | Invariants relied upon | Effects | Principal tests |
|-----------|-----------------|---|----|------------------------|---------|-----------------|
| A2 RIB preflight + cohort freeze | `RouteObservation` | — | `FrozenCohort` | RR-1, RR-2 | none | `src/cohort.rs` tests |
| A3 derived caches / extraction | archives, caches | cache files | cached entries | ID-1, PR-4 | file write | `derived_cache.rs` schema-version tests |
| A4 route-state reconstruction | observations | in-memory state | `RouteTransition` | ID-1, ID-10, RR-5 | none | `src/routes.rs`, lifecycle instance tests |
| A5 lifecycle derivation | transitions, cohort | — | `StreamLifecycle` | ID-10, RR-3, RR-4 | none | `nonfinal_instance_loss_does_not_make_withdrawn_lifecycle` |
| A6 diff/tokenize | `RouteState` | — | `TransitionKind` | RR-5 | none | `src/tokenize.rs` |

## Findings / results

| Algorithm | Structures read | M | P | Invariants relied upon | Effects | Principal tests |
|-----------|-----------------|---|----|------------------------|---------|-----------------|
| A7 wave detection | transitions | — | `ImpactWave` | determinism | none | `src/waves.rs` |
| A8 verdict/result derivation | transitions, lifecycles, expectation | — | `Verdict` + projections | FR-1..FR-6, TC-5 | none | `empty_transitions_is_no_impact`, `unknown_continuity_suppresses_strong_verdict` |
| A9 finding grouping | lifecycles, transitions, reviewed labels | — | `RoutingFinding` | FR-4, FR-5, FR-6 | none | `principal_findings_prefer_observer_diversity` |

## Presentation / evaluation

| Algorithm | Structures read | M | P | Invariants relied upon | Effects | Principal tests |
|-----------|-----------------|---|----|------------------------|---------|-----------------|
| A17 artifact resolution | catalog rows, filesystem | — | resolved path | AR-1, AR-2 | file stat | `artifact_path.rs` containment tests |
| A20 demo init/verify | tracked reviewed trees | demo DB | demo catalog + manifest | PV-2, PV-3, SC-2 | file write | `demo verify` gates; CI evaluation-smoke |
| A22 answer-key generation | tracked artifacts, demo manifest | — | answer key | PV-3 | file write | CI drift check |
| Path/fabric diagrams | lifecycle evidence, reviewed attachments | — | SVG | PV-4, PV-5, PV-6 | HTTP response | `fabric_diagram_contains_no_fabric_asn`, `observed_path_not_labeled_commercial_relationship`, `final_path_not_assumed_baseline` |

---

## Coupling observations

- **Excessive fan-in**: `AnalysisPlan`/`AnalysisPlanRecord` is read by
  readiness derivation, queue, worker, workbench, plan review page, and
  audit tooling; its payload is the authority for plan semantics.
- **Excessive fan-out**: `resolve_artifact` is the documented authority
  but is not the only resolver (AR-1) — the artifact path fact is
  duplicated across resolver implementations.
- **Algorithms coupled to unrelated representations**: `derive_analyzability`
  reads `analysis_plans.payload` JSON via string parsing
  (`serde_json::from_str`) rather than a typed projection; `derive_status`
  re-parses manifest payloads.
- **Algorithms reading both canonical and presentation structures**:
  workbench loaders read `stream_lifecycle_summaries` (projection) and
  `lifecycle.json` (canonical) and reconcile them for exact path
  evidence — a duplication that requires synchronization discipline.
- **Mutable structures participating in semantic identity**: none of the
  identity-bearing types are mutable after insertion; the only mutable
  identity-adjacent row is `catalog_events.last_seen` (see PR-1).
- **Duplicated structures requiring synchronization**: run evidence
  exists both as canonical artifact JSON and as projected SQLite rows
  (`stream_lifecycle_summaries`, `semantic_wave_summaries`,
  `run_transitions`); import asserts counts match and can be rebuilt
  from artifacts (`transition_import_is_streamed_or_bounded`,
  `transition_index_can_be_rebuilt_from_artifact`).
