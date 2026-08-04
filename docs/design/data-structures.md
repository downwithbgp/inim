# inim — data-structure catalog (as-built)

Status: current normative documentation, recovered at commit `92f83d8`.
Structures are grouped by computational role, not by source file. Only
structures that explain the program are listed; a secondary list covers
supporting implementation structures.

Labels: OBSERVED (code/schema), INFERRED (strong support, not enforced),
CLAIMED (documented, not established), UNKNOWN.

## 1. External and normalized source records

| Structure | Concept | Identity | Lifetime | Authority | Mutability | Invariants | Principal algorithms | Evidence |
|-----------|---------|----------|----------|-----------|------------|------------|----------------------|----------|
| `RouteObservation` | One normalized BGP observation (RIB entry, announcement, withdrawal, session boundary) | `ObservationId` (run-local u64) | analysis run | `src/ingest/mod.rs` (conversion boundary) | immutable in memory | carries `collector`, `peer_ip`, `peer_asn`, `prefix`, `path_id`, `provenance` | MRT parse + normalize | `src/domain/observation.rs` |
| `RouteKey` | Route-instance identity | `(collector, peer_ip, prefix, path_id)` | analysis run | `src/domain/route.rs` | immutable | ADD-PATH aware; hashes/orders | cohort freeze, lifecycle, caches | `src/domain/route.rs` |
| `ObserverPrefixKey` | Observer-prefix aggregate identity | `(collector, peer_ip, prefix)` | analysis run | `src/domain/route.rs` | immutable | no path_id; distinct from RouteKey | stream aggregation | `src/domain/route.rs` |
| `EventSnapshot` | Immutable source record row | `(event_id, content_sha256)` | catalog lifetime | catalog insert (sync/import) | append-only | never updated/deleted; raw payload preserved | snapshot insert/dedupe | SQLite `event_snapshots`; `src/catalog/store.rs` |

## 2. Reviewed interpretation and policy

| Structure | Concept | Identity | Lifetime | Authority | Mutability | Invariants | Principal algorithms | Evidence |
|-----------|---------|----------|----------|-----------|------------|------------|----------------------|----------|
| `Manifest` (schema v2) | Reviewed analysis manifest | `(event_id, payload sha256)` | catalog lifetime | reviewed tracked file `manifests/*.json` | immutable revision | open events require `analysis_end_utc`; `TransitPredicateMapping` status Reviewed for ready plans | planning, migration | `src/manifest.rs` |
| `TransitPredicateMapping` | Reviewed transit predicate + provenance | embedded in manifest | catalog lifetime | reviewed | immutable | `is_ready()` = Reviewed + predicate present | plan readiness, archive selection | `src/manifest.rs` |
| `TicketReview` | Reviewed interpretation of a source record (roles, applicability, entity mapping) | `(external_id, reviewed_at)` | catalog lifetime | reviewed JSON import (`corpus-review`) | append-only revisions | role/applicability vocab validated | review import, analyzability | `src/catalog/review.rs` |
| `CaseStudyDataFile` | Reviewed case-study data file (entity taxonomy, interconnection context, documents, links) | `slug` | catalog lifetime | tracked `case-studies/*/case-study.json` | immutable revision | attachments list is the only fabric input | case-study import | `src/catalog/case_study_import.rs` |
| `ProjectScope` | Reviewed project-scope exclusion policy | config file identity | process + catalog | `config/project-scope.toml` | reviewed file | exact-normalized matching only | scope filtering, queue/worker checks | `src/catalog/scope.rs` |
| `NetworkProfile` | Source/network profile (Internet2, Indiana GigaPOP) | enum variant | compile-time | `src/profiles/` | code | title convention → expectation | expectation derivation | `src/profiles/mod.rs` |

## 3. Planning

| Structure | Concept | Identity | Lifetime | Authority | Mutability | Invariants | Principal algorithms | Evidence |
|-----------|---------|----------|----------|-----------|------------|------------|----------------------|----------|
| `AnalysisPlan` | Pre-execution plan | plan payload sha256 | catalog lifetime | planning algorithm | immutable revision | `AnalysisPlanStatus::{Ready, Blocked}` | plan_from_manifest | `src/plan.rs` |
| `AnalysisPlanRecord` | Stored plan row | `sha256 UNIQUE` | catalog lifetime | import/queue | append-only | blocked plans not queueable | readiness derivation | SQLite `analysis_plans` |
| `ArchivePlan` / `CollectorPlan` / `ExpectedFile` | Archive selection plan per collector/family | case study | catalog lifetime | `build_plan_for_families` | Draft until saved | family-correct URLs/cadence | archive planning | `src/catalog/archive_plan.rs` |
| `BlockedTarget` | Reviewed target that cannot be analyzed | source label | plan lifetime | reviewed target status | immutable | only HistoricallyReviewed targets enter | target coverage | `src/catalog/archive_plan.rs` |
| `Analyzability` | Derived readiness record | `event_id` | derived on read | `derive_analyzability` | never stored | 14-state readiness vocabulary | readiness derivation | `src/catalog/analyzability.rs` |

## 4. Durable execution

| Structure | Concept | Identity | Lifetime | Authority | Mutability | Invariants | Principal algorithms | Evidence |
|-----------|---------|----------|----------|-----------|------------|------------|----------------------|----------|
| `AnalysisJob` | Durable execution state for one plan revision | job id string | catalog lifetime | job service | state machine | 16 `JobState` values; forward-only transitions; terminal jobs immutable | queue, claim, lease, cancel, retry | `src/catalog/jobs/mod.rs`, `service.rs` |
| `JobEvent` | Append-only job event log | `(job_id, sequence)` | catalog lifetime | job service | append-only | structured detail bounded (4096 B) | event append | `src/catalog/jobs/mod.rs` |
| `WorkerHeartbeat` | Worker liveness | `worker_id` | catalog lifetime | worker | updateable | lease 90 s, heartbeat 15 s | heartbeat, stale detection | `src/catalog/jobs/service.rs` |
| `RunRecord` | Published analysis run row | `(plan_id, started_at)` | catalog lifetime | publication | immutable | status/vote distinct from outcome | import, status derivation | SQLite `analysis_runs` |
| `ArtifactRecord` | Published artifact row | `(run_id, relative_path)` | catalog lifetime | publication | immutable | sha256 recorded; relative path | artifact import/audit | SQLite `analysis_artifacts` |

## 5. Protocol evidence and route identity

| Structure | Concept | Identity | Lifetime | Authority | Mutability | Invariants | Principal algorithms | Evidence |
|-----------|---------|----------|----------|-----------|------------|------------|----------------------|----------|
| `FrozenCohort` | Admitted baseline observer-prefix keys + baseline instances | set of `ObserverPrefixKey` | analysis run | `freeze_cohort` | immutable after freeze | admission requires origin + transit predicate | cohort freeze | `src/cohort.rs` |
| `RouteState` | One route instance at a time | `RouteKey` | analysis run | reconstruction | mutated by transitions | carries timestamp, attributes, path_id | route-state machine | `src/domain/route.rs`, `src/routes.rs` |
| `RouteTransition` | State change between two route states | sequence + kind | analysis run | reconstruction | append | `TransitionKind` vocabulary + orthogonal `GenericTransitionEffects` | diff/tokenize, waves, lifecycle | `src/domain/route.rs`, `src/tokenize.rs` |
| `RouteStateMachine` | In-memory map of current states | `HashMap<RouteKey, RouteState>` | analysis run | `src/routes.rs` | mutated | event baseline map retained | reconstruction | `src/routes.rs` |

## 6. Transitions and lifecycle

| Structure | Concept | Identity | Lifetime | Authority | Mutability | Invariants | Principal algorithms | Evidence |
|-----------|---------|----------|----------|-----------|------------|------------|----------------------|----------|
| `StreamLifecycle` | Full lifecycle of one observer-prefix stream | `(collector, peer_ip, prefix)` | analysis run | lifecycle derivation | immutable after derivation | category + flags; retains all `RouteKey` histories | lifecycle derivation | `src/lifecycle.rs` |
| `LifecycleTransition` | Lightweight transition record | timestamp + phase + kind | analysis run | lifecycle derivation | immutable | phase vocabulary | lifecycle derivation | `src/lifecycle.rs` |
| `StreamCategory` | Primary classification of a stream | enum | analysis run | lifecycle derivation | immutable | Unchanged/PrependOnly/PathChangedStillViaTransit/DepartedTransitPath/Withdrawn | finding derivation | `src/lifecycle.rs` |
| `StreamFlags` | Secondary flags (restored, not_restored, multiple_cycles, add_path_ambiguous) | enum fields | analysis run | lifecycle derivation | immutable | ambiguity suppresses strong conclusions | restoration classification | `src/lifecycle.rs` |
| `StreamRestoration` | Restoration event record | per stream | analysis run | lifecycle derivation | immutable | restoration kinds | restoration classification | `src/lifecycle.rs` |

## 7. Findings and results

| Structure | Concept | Identity | Lifetime | Authority | Mutability | Invariants | Principal algorithms | Evidence |
|-----------|---------|----------|----------|-----------|------------|------------|----------------------|----------|
| `ImpactWave` / `WaveMotif` | Temporally concentrated transition group + SEQUITUR motif | wave id (run-local) | analysis run | `detect_waves` | immutable | deterministic gap clustering | wave detection | `src/waves.rs` |
| `RoutingFinding` | Operator-facing routing story at one observer session | `stable_id` | presentation | workbench derivation | derived on read | presentation model; exact paths in streams | finding grouping, `select_principal_findings` | `src/catalog/workbench.rs` |
| `Verdict` | Machine verdict enum | enum variant | analysis run + report | `derive_verdict` | immutable | 16 variants; `observed_result_kind` and `expectation_assessment_kind` are projections | verdict derivation | `src/domain/assessment.rs` |
| `ObservedResultKind` | Observed route-state result (4 values) | enum | analysis run + report | verdict projection | immutable | labels never contain expectation wording | result derivation | `src/domain/assessment.rs` |
| `ExpectationAssessmentKind` | Expectation assessment (7 values) | enum | analysis run + report | verdict projection | immutable | references reviewed expectation | assessment | `src/domain/assessment.rs` |
| `AnalysisOutcome` | Run outcome (completed / insufficient_visibility / incomplete) | tagged enum | analysis run + report | outcome assembly | immutable | infrastructure failure never a routing verdict | outcome assembly | `src/outcome.rs` |

## 8. Artifacts and provenance

| Structure | Concept | Identity | Lifetime | Authority | Mutability | Invariants | Principal algorithms | Evidence |
|-----------|---------|----------|----------|-----------|------------|------------|----------------------|----------|
| Immutable run directory | Published artifact set on disk | `data/runs/<job-id>/<event>/…` (worker); `case-studies/*/out/<event>/…` (reviewed, tracked) | catalog lifetime | publication | immutable | staging → validate → rename | publication | `src/catalog/jobs/publish.rs` |
| Artifact relative path | Catalog-relative path stored per artifact | unique per run | catalog lifetime | publication | immutable | no absolute/parent-relative | artifact resolution | SQLite `analysis_artifacts`; `src/catalog/artifact_path.rs` |
| `ExecutionMetadata` | Volatile metadata written into staging | plan_hash + stage | staging lifetime | worker | transient | plan hash must match queued plan | validation | `src/catalog/jobs/publish.rs` |
| `StreamPathEvidence` | Exact path evidence for one stream | `(run, collector, peer_ip, prefix)` | presentation | `load_lifecycle_evidence` | derived | exact paths retained | path diagrams | `src/catalog/web/path_diagram.rs` |

## 9. Catalog projections

| Structure | Concept | Identity | Lifetime | Authority | Mutability | Invariants | Principal algorithms | Evidence |
|-----------|---------|----------|----------|-----------|------------|------------|----------------------|----------|
| `CatalogStatus` | Derived analyst-facing status (8 values) | event | derived on read | `derive_status` | never stored | deterministic precedence | status derivation | `src/catalog/status.rs` |
| `StreamLifecycleSummary` | Projected stream summary row | `(run_id, collector, peer_ip, prefix)` | catalog lifetime | import | immutable | counts match report | import | SQLite `stream_lifecycle_summaries`; `src/catalog/import.rs` |
| `SemanticWaveSummary` | Projected wave row | `(run_id, wave_id)` | catalog lifetime | import | immutable | counts match report | import | SQLite `semantic_wave_summaries` |
| `RunTransitionRecord` | Projected transition row | `(run_id, seq)` | catalog lifetime | import | immutable | references canonical artifact | import | SQLite `run_transitions`; `src/catalog/import.rs` |
| `CaseStudyEventLink` | Case-study ↔ event link | case-study + event | catalog lifetime | case-study import | append-only | never fabricates source snapshots | case-study projection | SQLite `case_study_event_links` |

## 10. Presentation models

| Structure | Concept | Identity | Lifetime | Authority | Mutability | Invariants | Principal algorithms | Evidence |
|-----------|---------|----------|----------|-----------|------------|------------|----------------------|----------|
| View models (`DashboardView`, `EventView`, `RunView`, `WorkbenchView`, …) | Server-rendered page models | per request | request | `src/catalog/web/view.rs` loaders | read-only | scope-filtered | view construction | `src/catalog/web/view.rs` |
| `PathStateView` / `ObservedPath` / `PathNode` | AS-path diagram states | per stream | request | `comparison_states` | read-only | exact paths + compaction for display | path diagrams | `src/catalog/web/path_diagram.rs` |
| `FabricView` / `FabricAttachmentView` | Layer-2 fabric diagram | per case study | request | `interconnection_context.attachments` | read-only | only reviewed attached networks | fabric diagram | `src/catalog/web/path_diagram.rs` |

## 11. Evaluation structures

| Structure | Concept | Identity | Lifetime | Authority | Mutability | Invariants | Principal algorithms | Evidence |
|-----------|---------|----------|----------|-----------|------------|------------|----------------------|----------|
| Scenario manifest (`evaluation/scenarios.toml`) | Reviewed evaluation scenarios | schema v1 + scenario ids | tracked | reviewed | immutable | no answers embedded | task-to-page mapping | `evaluation/scenarios.toml` |
| Demo manifest | Deterministic demo catalog summary | schema v1 | tracked (generated) | `demo init` | regenerated | no timestamps; byte-deterministic | demo verification | `src/catalog/demo.rs` |
| Answer key (`evaluation/generated/answer-key.json`) | Generated evaluator answers | schema v1 | tracked (generated) | generator | regenerated | derived from artifacts; drift check in CI | answer-key generation | `scripts/build-evaluation-answer-key.py` |

## Supporting implementation structures

- `AnalysisPlan`/`EventWindow`/`TicketLifecycle`/`ImpactExpectation`
  (`src/domain/event.rs`, `src/domain/expectation.rs`) — plan inputs.
- `ObservationAttributes`/`Communities`/`ObservationProvenance`
  (`src/domain/observation.rs`) — route attribute payload.
- `IngestContext`/`InimError` (`src/ingest/mod.rs`) — parsing context.
- `CacheControl`/`CachedArchive`/`RibCacheEntry`/`UpdateCacheEntry`
  (`src/derived_cache.rs`) — cache bookkeeping.
- `TargetSet`/`TargetStream` (`src/target.rs`) — preflight target model.
- `ImpactWave`/`MotifClass`/`Sequitur` grammar (`src/waves.rs`,
  `src/sequitur/`) — wave analysis.
- `ErrorCode` constants (`src/catalog/jobs/mod.rs`) — failure taxonomy.
- `EventSnapshot`/`CatalogEvent`/`CaseStudy`/`CaseStudyTarget`
  (`src/catalog/domain.rs`) — catalog row models.
- `ProjectScope`/`ExcludedEntity`/`ExcludedSourceRecord`
  (`src/catalog/scope.rs`) — policy model.
- `FindingAudit`/`FindingChronologyAudit` (`src/catalog/workbench.rs`) —
  audit tooling models.
- `AsnIdentityRegistry`/`RelationshipView` (`src/catalog/workbench.rs`,
  `src/catalog/web/path_diagram.rs`) — identity/relationship labels.
- `PerfReport`/`ArchiveMetric`/`StageMetric` (`src/perf.rs`) — performance
  artifacts.

## Structures with unclear authority

- Event title: exists in `event_snapshots.normalized_json`, manifest
  target label, and report sections; the workbench prefers the latest
  snapshot title + reviewed entity data (INFERRED).
- Result/assessment labels: `report.json` carries both machine verdict
  names (`result.verdict`) and human labels; the API exposes stored
  strings verbatim (see [invariants FR-1](invariants.md)).

## Structures permitting invalid states

- A Ready-status plan for an open event with no cutoff is storable via
  the import path (see [invariants PR-6](invariants.md)).
- An artifact row can exist without its file and vice versa; divergence
  is detected by audit but not prevented (see AR-5).
- `catalog_events.last_seen` is mutable while snapshot rows are not
  (see PR-1).
