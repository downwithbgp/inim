# inim — state machines (as-built)

Status: current normative documentation, recovered at commit `92f83d8`.
Each state machine is described separately; distinct axes are never
merged (analysis status `Complete` ≠ source lifecycle `Open` ≠ observed
result `InsufficientVisibility`).

---

## SM1. Source event lifecycle (derived catalog status)

Derived, never stored as a single truth field (`src/catalog/status.rs`).
States are `CatalogStatus`: `Discovered`, `NeedsReview`, `Ready`,
`Blocked`, `Running`, `Complete`, `Failed`, `Stale`.

| From | Event | Guard | To | Effect | Enforcement |
|------|-------|-------|----|--------|-------------|
| (initial) | source record imported | — | `Discovered` | event row | import |
| `Discovered` | reviewed manifest exists | manifest present | `NeedsReview` | — | derivation |
| `NeedsReview` | manifest reviewed; plan ready | ready plan | `Ready` | — | derivation |
| `NeedsReview` | manifest reviewed; plan blocked | blocked plan | `Blocked` | blocker reason | derivation |
| `Ready` | job claimed/executing | active run | `Running` | — | derivation |
| `Ready`/`Running` | run completes | completed run for latest inputs | `Complete` | — | derivation |
| any | run fails | latest run failed | `Failed` | — | derivation |
| `Complete` | snapshot/manifest changed after run | newer inputs | `Stale` | — | derivation |

Deterministic precedence (highest wins): Running → Failed → Stale →
Blocked → Complete → Ready → NeedsReview → Discovered. A completed
historical run stays completed; `Stale` never invalidates it.

## SM2. Plan readiness (analysis-plan and readiness vocabulary)

`AnalysisPlanStatus` (`src/plan.rs`): `Ready` | `Blocked { reason }`
with five `AnalysisBlockReason` variants. The catalog-level readiness
vocabulary has 14 states (`src/catalog/analyzability.rs`):
`NotReviewed`, `NeedsEntityMapping`, `NeedsTransitPredicate`,
`NeedsAnalysisWindow`, `NotApplicableToPublicBgp`,
`ReadyForArchivePlanning`, `ArchivePlanReady`,
`InsufficientBaselineVisibility`, `NotDirectlyObservableInPublicBgp`,
`NotOriginAttributable`, `AnalysisComplete`, `AnalysisStale`,
`AnalysisFailed`, `AnalysisRunning`.

| From | Event | Guard | To | Effect | Enforcement |
|------|-------|-------|----|--------|-------------|
| (initial) | no manifest | — | `NotReviewed` | — | derivation |
| `NotReviewed` | manifest exists, no origin mapping | — | `NeedsEntityMapping` | — | derivation |
| `NeedsEntityMapping` | origin mapping reviewed, predicate unresolved | — | `NeedsTransitPredicate` | — | derivation |
| `NeedsTransitPredicate` | predicate reviewed, no window | — | `NeedsAnalysisWindow` | — | derivation |
| `NeedsAnalysisWindow` | ready plan, no archive plan | — | `ReadyForArchivePlanning` | — | derivation |
| `ReadyForArchivePlanning` | case-study archive plan stored | — | `ArchivePlanReady` | — | derivation |
| any (ready) | blocked plan | — | `Blocked`-family states | reason | derivation |
| any | reviewed applicability = not observable | — | `NotDirectlyObservableInPublicBgp` | — | derivation (authoritative over derived readiness) |
| any | active run | — | `AnalysisRunning` | — | derivation |
| any | completed run for latest inputs | — | `AnalysisComplete` | — | derivation |

Readiness is a **projection over stored state**, not one algorithm
(distributed across `derive_status`, `derive_analyzability`, and plan
checks).

## SM3. Analysis job lifecycle

States (`src/catalog/jobs/mod.rs`): `Queued`, `Claimed`,
`DiscoveringArchives`, `AcquiringArchives`, `ParsingBaseline`,
`FreezingCohort`, `ParsingUpdates`, `ReconstructingRoutes`,
`DerivingEvidence`, `RenderingArtifacts`, `ValidatingArtifacts`,
`PublishingRun`, `Completed`, `CancelRequested`, `Cancelled`, `Failed`.

| From | Event | Guard | To | Effect | Enforcement |
|------|-------|-------|----|--------|-------------|
| (initial) | queue | plan ready + in scope + no active duplicate | `Queued` | job row + event | `service::queue` |
| `Queued` | claim | lease acquired | `Claimed` | lease, worker id | `claim_next` (BEGIN IMMEDIATE) |
| `Queued` | cancel | — | `Cancelled` | terminal | `request_cancel` |
| `Queued` | scope exclusion | recheck | `Cancelled` | terminal | `cancel_scope_excluded` |
| executing stages | stage advance | forward in fixed order | next stage | progress | `transition`/`stage_advance` |
| executing stages | cancel observed | — | `CancelRequested` | cooperative stop | `observe_cancel` |
| executing stages | failure | — | `Failed` | error code + summary | `fail` |
| `CancelRequested` | cancel confirmed | — | `Cancelled` | terminal | service |
| `CancelRequested` | failure | — | `Failed` | terminal | service |
| `PublishingRun` | publication complete | validation passed | `Completed` | run linkage | `complete_job` |
| `PublishingRun` | publication failure | — | `Failed` | staging preserved | `fail_job` |

Legal transitions are explicit: `legal_transition(from, to)` returns
false for every other pair (test `illegal_state_transition_is_rejected`).
Regression is never legal; advancement may skip intermediate stages.
Terminal states (`Completed`, `Cancelled`, `Failed`) are immutable;
retry creates a new job linked via `original_job_id`.

Job status ≠ execution stage ≠ analysis outcome ≠ observed result ≠
expectation assessment: a completed job may carry
`InsufficientVisibility` outcome (test
`completed_insufficient_visibility_is_not_failed_job`).

## SM4. Worker lease

| State | Transition | Guard | Enforcement |
|-------|-----------|-------|-------------|
| active (lease unexpired) | heartbeat renew | `heartbeat_at` within window | `renew_lease` |
| active | expiry | `lease_expires_at < now` | `mark_stale_leases` |
| stale (expired) | detected | — | audit marks stale; never auto-resumed (test) |

Defaults: lease 90 s, heartbeat 15 s (`src/catalog/jobs/service.rs`).
Claim uses `BEGIN IMMEDIATE` so two workers cannot claim the same job
(test `two_workers_cannot_claim_same_job`).

## SM5. Run publication

| State | Transition | Guard | Enforcement |
|-------|-----------|-------|-------------|
| staging | write artifacts + execution metadata | job claimed | worker |
| staging | validate | required artifacts + plan hash match + schema versions | `validate_staged` |
| validated | rename staging → final immutable dir | same filesystem | `publish_staged_run` |
| final dir | import rows + complete job | report readable | `import_finalized_run` + `complete_job` |
| crash between rename and import | orphan final directory | — | detected by `reconcile_orphans`, not auto-repaired |

A staged run is not visible as a run (test
`incomplete_stage_is_not_visible_as_run`); publication is idempotent for
the same job (test `publication_is_idempotent_for_same_job`).

## SM6. Observer-prefix lifecycle classification

Per-stream classification (`src/lifecycle.rs`): `StreamCategory`
(`Unchanged`, `PrependOnly`, `PathChangedStillViaTransit`,
`DepartedTransitPath`, `Withdrawn`) plus `StreamFlags` (`restored`,
`not_restored`, `multiple_cycles`, `add_path_ambiguous`).

| Condition | Category |
|-----------|----------|
| no transitions | `Unchanged` |
| only collapsed-equivalent prepend changes | `PrependOnly` |
| material path change, still via required transit ASN | `PathChangedStillViaTransit` |
| path departed required transit ASN | `DepartedTransitPath` |
| route became absent (final instance lost) | `Withdrawn` |

Restoration classification is per-`StreamRestoration`
(`src/lifecycle.rs`), with `add_path_ambiguous` suppressing strong
stream-level conclusions when keyed/unkeyed encoding is mixed.

## SM7. Provisional open-event analysis

The reviewed cutoff (`analysis_end_utc` in the manifest) defines the
analysis window for an open event. Plan lifecycle carries
`open`; the verdict carries provisionality separately
(`Verdict::ProvisionalImpactObserved` / `ProvisionalNoImpactSoFar`,
`is_provisional()`), and the run outcome's `assessment.provisional` flag
is a distinct verdict-posture flag. A stored Ready plan for an open
event with no cutoff is possible via the import path but rejected at
queue/worker time (see [invariants PR-6](invariants.md)).

## SM8. Project-scope recheck

| Point | Check | Enforcement |
|-------|-------|-------------|
| queue time | plan hash in scope | `src/catalog/jobs/plan.rs` |
| worker after claim, before source access | event/target excluded? | `src/worker.rs` (cancel_scope_excluded) |
| retry | excluded? | `src/catalog/jobs/service.rs` |
| standalone `analyze` | none | absent (falsification finding) |

Scope is not access control; it is an exact-normalized reviewed policy
overlay (see [invariants SC-1..SC-5](invariants.md)).

## Explicitly not one state machine

- `analysis status Complete` (SM3) — job terminality.
- `source lifecycle Open` (manifest field) — ticket state at snapshot.
- `observed result InsufficientVisibility` (verdict projection) —
  analysis outcome.

These three axes are orthogonal and are kept in separate structures
(`JobState`, manifest `open`, `ObservedResultKind`).
