# Pre-pilot invariant closure — incremental design checksum (2026-08)

Historical audit, not normative. Applies the documented design-checksum
method (see `docs/computational-model.md`) to the correction range
`b2fde8161aef361a99b3df6f50c2eba7f9c0ee31..9a6354c20be5d80cb0d9a22bfa49199aa5866fd5`
(Session 57, pre-pilot invariant closure). It is not a full design-recovery
rerun; no external evaluator session occurred; no canonical analysis was
rerun.

## Data structures changed

| Change | Before | After | Evidence | Compatibility | Tests |
|--------|--------|-------|----------|---------------|-------|
| Artifact containment primitive | inline absolute/parent checks in `resolve_artifact` | `is_safe_relative_path` shared primitive (rejects empty, absolute, parent traversal, drive-letter/UNC prefixes, backslash separators) | `src/catalog/artifact_path.rs` | none (internal) | `is_safe_relative_path_rejects_escape_forms` |
| Run API view model | `run.verdict` + `run.assessment` raw | + structured `observed_result {kind,label}` and `expectation_assessment {kind,label}`; raw fields documented legacy | `load_run_json` (`src/catalog/web/view.rs`), `src/catalog/web/api.rs` | additive (new fields) | `api_exposes_structured_observed_result` |

## Semantic identity changed

| Change | Before | After | Evidence | Compatibility | Tests |
|--------|--------|-------|----------|---------------|-------|
| Canonical plan identity | `CanonicalPlan.analysis_end` = declared `event_window_utc.end` (empty for open events); the reviewed cutoff did not participate in the plan hash | `analysis_end` = reviewed `analysis_end_utc` for open events; the cutoff participates in plan identity | `CanonicalPlan::from_manifest` (`src/catalog/jobs/plan.rs`) | plan hashes of newly queued open-event runs differ; stored historical rows unchanged | `cutoff_participates_in_plan_hash_when_semantic` |

## Invariants changed

| Change | Before | After | Evidence | Compatibility | Tests |
|--------|--------|-------|----------|---------------|-------|
| Continuity gate ordering (F-1) | empty-finding fallback ran before the continuity gate | continuity gate runs before any finding-cardinality result derivation | `derive_verdict` (`src/assess.rs`) | completed-run verdict for gaps+zero-transitions changes from no-change to `InsufficientVisibility`; no tracked run demonstrated affected | `continuity_failure_precedes_empty_finding_fallback`, `continuity_gate_decision_table` |
| Artifact resolver equivalence (F-2/F-3) | four resolvers; workbench missed some reviewed trees; demo fallback unvalidated | one resolver + one containment primitive on every consumer | `src/catalog/artifact_path.rs`, `src/catalog/workbench.rs`, `src/catalog/demo.rs`, `src/catalog/jobs/publish.rs` | all tracked artifact rows still resolve | `all_artifact_consumers_agree_on_validity`, `workbench_and_demo_resolver_equivalent` |
| Open-event cutoff readiness (F-4) | open manifest could store a Ready plan without a reviewed cutoff/provenance | load rejects; plan Blocks; queue/worker require cutoff regardless of declared end; import requires recorded provenance | `src/manifest.rs`, `src/catalog/import.rs`, `src/catalog/jobs/plan.rs`, `src/worker.rs` | malformed open manifests now rejected at import; tracked manifests already conform | `open_event_ready_plan_requires_cutoff`, `open_event_ready_plan_requires_cutoff_provenance` |
| Standalone scope boundary (F-5) | `inim analyze` never loaded project scope | `analyze_scope_block` rejects excluded subjects before planning/source access | `src/main.rs`, `docs/DOMAIN.md` | blocked analyze now exits `EXIT_ANALYSIS_BLOCKED`; no exclusions changed | `standalone_analyze_scope_boundary_is_explicit` |
| Observed-result projection (F-6) | unrecognized stored verdicts rendered verbatim in the observed-result slot; API exposed raw verdict strings | neutral fallback label; structured API projections; raw fields documented legacy | `src/catalog/web/view.rs`, `src/catalog/web/api.rs`, `docs/reference/API.md` | additive API fields; historical rows readable | `legacy_verdict_does_not_override_current_projection` |
| Changelog session-narrative (F-8) | `Session 55` heading tripped the docs audit; CI red | product-language heading; regression test | `CHANGELOG.md`, `tests/release_test.rs` | release-facing content preserved | `changelog_contains_no_session_narrative` |

## Algorithms changed

| Change | Before | After | Evidence | Compatibility | Tests |
|--------|--------|-------|----------|---------------|-------|
| Verdict derivation | empty-findings early return before continuity gate | continuity gate first | `derive_verdict` (`src/assess.rs`) | see F-1 invariant row | `continuity_gate_decision_table` |
| Artifact resolution | consumer-specific candidate lists | one candidate search + containment validation | `resolve_artifact` | see F-2/F-3 | `git_checkout_and_packaged_source_resolver_equivalent` |

## State transitions changed

| Change | Before | After | Evidence | Compatibility | Tests |
|--------|--------|-------|----------|---------------|-------|
| Plan readiness for open events | Ready without cutoff possible via import | Blocked (`MissingAnalysisEndForOpenTicket`) | `build_plan_record` (`src/catalog/import.rs`) | catalog rows for invalid legacy input read Blocked | `open_event_ready_plan_requires_cutoff` |
| Standalone analyze exit | proceeded to planning/analysis regardless of scope | exits `EXIT_ANALYSIS_BLOCKED` before source access for excluded subjects | `cmd_analyze` (`src/main.rs`) | documented exit code | `project_scope_checked_before_network_access_where_applicable` |

## Effects changed

- Standalone `analyze` now reads `config/project-scope.toml` (filesystem read) before any planning.
- No network effect changed: scope checks and cutoff gates occur before broker discovery on every path.
- No new write effects: blocked analyze writes no outputs.

## Authority boundaries changed

- Artifact path authority: `resolve_artifact` + `is_safe_relative_path` are now the single authority for analysis artifacts; document serving uses the same primitive plus its canonical check.
- Run-result authority: the structured `observed_result`/`expectation_assessment` projections are the current interpretation; the stored `verdict`/`assessment` fields are explicitly legacy.
- Scope authority: `config/project-scope.toml` applies to every first-party execution path (catalog workflow and standalone analyze).

## Information-loss boundaries changed

- Observed-result projection no longer forwards raw/unrecognized stored verdict strings into the observed-result slot (a neutral fallback is used); the raw value remains available as the legacy field.
- No canonical evidence, snapshot, or report was rewritten; historical runs remain readable.

## Complexity changed

- `resolve_artifact` adds one canonicalization per existing candidate (bounded by the small candidate set; 118 demo-catalog rows verified).
- No unbounded structures introduced.

## Statements

- No full design-recovery rerun.
- No external evaluator session (pilot registry unchanged: zero).
- No canonical analysis rerun; no source contacted; no archive acquired.
