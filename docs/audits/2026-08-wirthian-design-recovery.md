# Wirthian design-recovery audit (2026-08)

Historical audit, not normative. Describes the as-built computational
model of inim recovered at a pinned commit. Current normative documents
are [../computational-model.md](../computational-model.md) and
[../design/](../design/).

## Scope and method

Design-recovery audit using the expanded Wirthian formulation
(Program = data structures + invariants + algorithms + state transitions
+ effects). Three explicitly separated passes:

1. **Reconstruction** — candidate computational model built from code,
   tests, schemas, tracked artifacts.
2. **Falsification** — independent skeptical pass seeking counterexamples
   to every candidate invariant and authority claim.
3. **Synthesis** — only claims that survived, or are explicitly marked
   INFERRED/UNKNOWN, entered the maintained docs.

No implementation repair was performed in this session.

## Repository state

- Starting commit: `92f83d896faa3b4a205406ce618c7789a13c3789` (main,
  clean; matches the expected starting HEAD `92f83d8`).
- Branch: `session-56-wirthian-design-recovery`.
- Ending documentation commit before merge: recorded at PR merge.
- Environment: linux/amd64, rustc 1.97.1, cargo 1.97.1, git 2.43.0.
- Actual test count at start: 1481 passing (`cargo test --locked`).
- Doc-test count: 0 (no doctests present).
- Open PRs at start: none (11 merged, PR #11 = Session 55).

## Commands run

- `git fetch --all --prune`, `git pull --ff-only origin main`
- `cargo build --locked`, `cargo test --locked` (1481 passed),
  `cargo test --doc --locked` (0)
- `cargo package --locked --list` (463 files; 0 `data/`/`*.sqlite`)
- `scripts/audit-docs.sh` — **fails at the starting commit** (see
  Findings F-8)
- `inim demo init` / `inim demo verify` (offline, deterministic)
- `inim serve` on loopback with GET/POST probes + DB hash before/after
- `inim project-scope show/audit`, `inim analysis-job audit`
- answer-key generation ×2 (`--db`) and drift comparison
- No live-source commands were run (no GRNOC, RouteViews, RIPE RIS,
  PeeringDB, RIR contact).

## Intermediate reports (untracked, outside the repository)

- Reconstruction report: SHA-256
  `0588a82679482aa882c1808d440d6fa8d72561c63fffe920d3509b7103886142`
- Falsification report: SHA-256
  `dec7eeca1dadd77856fd991843301c4e63a0febd9d8ca6677c892ce00ef203d5`

Both reports were frozen before the next pass; neither was committed.

## Repository areas reviewed

- Entry points and CLI dispatch (`src/main.rs`), web router
  (`src/catalog/web/mod.rs`), worker (`src/worker.rs`), publication
  (`src/catalog/jobs/*`), catalog (`src/catalog/`), domain
  (`src/domain/`), ingestion (`src/ingest/`), orchestration
  (`src/orchestrate.rs`), lifecycle (`src/lifecycle.rs`), cohort
  (`src/cohort.rs`), waves/sequitur (`src/waves.rs`, `src/sequitur/`),
  output/report (`src/output.rs`, `src/report.rs`), profiles/sources
  (`src/profiles/`, `src/sources/`), scripts, evaluation material,
  case-study data, migrations, Cargo packaging, CI workflow.

## Structures recovered

Central structures (34) and supporting structures (16) are cataloged in
[../design/data-structures.md](../design/data-structures.md). Central
examples: `RouteKey`/`ObserverPrefixKey`, `RouteObservation`,
`FrozenCohort`, `StreamLifecycle`, `AnalysisPlan`, `ArchivePlan`,
`AnalysisJob`, `Verdict`/`ObservedResultKind`/`ExpectationAssessmentKind`,
`AnalysisOutcome`, `RoutingFinding`, `ProjectScope`, SQLite tables
`event_snapshots`, `manifest_revisions`, `analysis_plans`,
`analysis_runs`, `analysis_artifacts`, `analysis_jobs`.

## Algorithms recovered

23 named algorithms (A1–A23) plus distributed/unnamed algorithms are
cataloged in [../design/algorithms.md](../design/algorithms.md).

## State machines recovered

8 state machines in [../design/state-machines.md](../design/state-machines.md):
source event lifecycle (derived), plan readiness, analysis job, worker
lease, run publication, observer-prefix lifecycle, provisional open-event
analysis, project-scope recheck. All states and legal transitions were
verified against implementation (`JobState` +
`legal_transition`, `CatalogStatus` precedence, worker lease timings).

## Invariants evaluated

57 invariants are registered in
[../design/invariants.md](../design/invariants.md):
33 enforced, 5 partially enforced, 1 assumed, 2 contradicted/suppressed,
0 claimed-only, 0 unknown.

## Experiments performed (all offline)

| # | Experiment | Result |
|---|------------|--------|
| E1 | `cargo build --locked` | success |
| E2 | `cargo test --locked` | 1481 passed |
| E3 | `demo init` (fresh temp catalog) | ok; 4 events, 12 runs, GRNOC corpus imported |
| E4 | `demo verify` | ok; no source access; no absolute paths |
| E5 | read-only server on loopback + GET/POST probes + DB hash | 15 GET routes 200; POST mutations 415; DB SHA-256 unchanged; `-wal`/`-shm` sidecars created then removed at shutdown |
| E6 | answer-key generation ×2 with `--db` | byte-identical; byte-identical to tracked `answer-key.json` |
| E7 | `project-scope show/audit` on demo catalog | 0 excluded plans/jobs/runs/artifacts |
| E8 | `analysis-job audit` on demo catalog | orphan final directories reported (runtime leftovers), not deleted |
| E9 | `cargo package --list` | 463 files; no `data/`/`*.sqlite`; reviewed evidence trees packaged |
| E10 | `scripts/audit-docs.sh` at clean HEAD | **FAILS**: pre-existing `CHANGELOG.md:3` "Session 55" narrative |

## Claims contradicted

1. **"Artifact listing and access share one resolver"** — contradicted.
   At least four resolvers exist with different candidate sets/order:
   `resolve_artifact` (`src/catalog/artifact_path.rs`), a hand-rolled
   workbench resolver (`src/catalog/workbench.rs`, hardcoded four
   case-study dirs), a demo fallback (`src/catalog/demo.rs`), and a
   direct join in orphan reconciliation (`src/catalog/jobs/publish.rs`).
   Same relative path can resolve differently per consumer.
2. **"Gap/unknown continuity always suppresses strong verdicts"** —
   contradicted for the empty-transitions path: `derive_verdict`
   returns `NoObservableBgpImpact` before the continuity gate
   (`src/assess.rs:208-224`); with archive gaps and zero transitions
   the verdict is "No route-state change observed", not
   `InsufficientVisibility`. The module doc claims suppression
   (`src/assess.rs:19-20`). The gaps + empty-transitions combination is
   untested.

## Claims narrowed

3. **"Open runs always have cutoff provenance"** — a Ready-status plan
   for an open event with no cutoff is storable via the import path
   (`src/catalog/import.rs`); execution is blocked only at queue/worker
   time. `analysis_end_utc` without reviewed provenance is representable
   (meta sidecar optional; generic fallback sentence).
4. **"Artifact paths cannot escape root"** — `catalog_root.join(rel)`
   without component validation in `src/catalog/workbench.rs` and the
   demo fallback (`src/catalog/demo.rs`) permits escape for a
   crafted/legacy row; `resolve_artifact` and document serving validate.
5. **"Project-scope exclusion is rechecked before source access"** —
   enforced in the worker (after claim) and at queue/retry, but the
   standalone `inim analyze` CLI and `orchestrate.rs` never load
   `ProjectScope`.
6. **"Observed result cannot inherit expectation wording"** — clean
   presentation labels, but `report.json` `result.verdict` carries
   expectation vocabulary and the API exposes stored verdict strings
   verbatim.
7. **"Read-only browsing leaves the database unchanged"** — logical rows
   unchanged (E5), but the catalog opens read-write WAL so `-wal`/`-shm`
   sidecars are created.
8. **"Source snapshots are fully immutable"** — snapshot rows are
   immutable, but `catalog_events.last_seen` is updated by
   `upsert_event`.

## Unknowns

- Whether the gaps + empty-transitions path occurs on a real completed
  run (requires archive acquisition — not performed; the E2 test suite
  does not cover the combination).
- Whether the source-extraction reuse caveat (predicate-2-only streams
  missed when reusing an origin-keyed extraction) manifests at
  production scale.

## Discrepancies (intended vs as-built)

| Area | As implemented | As claimed | Apparently intended | Classification | Evidence | Consequence |
|------|----------------|------------|---------------------|----------------|----------|-------------|
| Continuity gate | empty-transitions returns strong verdict before gate | module doc says strong verdicts suppressed on unknown continuity | gate should precede empty case | partially enforced invariant | `src/assess.rs:208-224` vs `:13-14` | run with gaps + no changes may overstate "no route-state change" |
| Artifact resolution | 4 resolvers | single resolver authority | one resolver | duplicated authority | `artifact_path.rs`, `workbench.rs`, `demo.rs`, `publish.rs` | same path can resolve differently per consumer |
| Scope enforcement | worker/queue/retry only | "rechecked before source access" | all execution paths | abstraction leak | `src/worker.rs`, `src/main.rs` | standalone analyze unguarded |
| Open-event cutoff | storable Ready plan without cutoff via import | manifest validation requires cutoff | one enforcement point | partially enforced invariant | `src/catalog/import.rs`, `src/manifest.rs` | catalog may show Ready for un-runnable plan |
| WAL sidecars | read-write WAL open | "read-only server" | open read-only | documentation drift | `src/catalog/db.rs`, `src/catalog/web/server.rs` | -wal/-shm files appear under read-only use |
| Changelog gate | "Session 55" narrative in CHANGELOG | audit expects no session narrative in normative docs | format change should not break gate | historical residue / documentation mismatch | `CHANGELOG.md:3`, `scripts/audit_docs.py` | CI red at starting commit (F-8) |
| Extracted verdict strings | stored verdicts exposed verbatim in API | human labels from `ObservedResultKind` | machine + human separation | documentation drift | `src/catalog/web/view.rs`, `src/output.rs` | consumers may see expectation vocabulary |

## Design findings by severity

- **P0**: none demonstrated.
- **P1**:
  - F-1 Continuity-gate bypass (Claim 2): demonstrated in code, untested
    combination; affects the observed result of a completed run.
    Confidence: OBSERVED (code + module doc contradiction). Currently
    reproduced: code-path only; not reproduced on a real run.
    Smallest follow-up: a unit test for `assess(..., true, &[])` +
    decision on gate ordering.
- **P2**:
  - F-2 Duplicated artifact resolvers (Claim 1). Confidence: OBSERVED.
    Smallest follow-up: route all consumers through `resolve_artifact`
    or document divergence; add a resolver-equivalence test.
  - F-3 Unvalidated `root.join(rel)` in workbench/demo (Claim 4).
    Confidence: OBSERVED (code); exploit requires a crafted/legacy row.
    Smallest follow-up: reuse `resolve_artifact` validation.
  - F-4 Storable Ready plan for open event without cutoff (Claim 3).
    Confidence: OBSERVED. Smallest follow-up: enforce at import or
    classify such plans Blocked.
  - F-5 Standalone analyze has no scope enforcement (Claim 5).
    Confidence: OBSERVED. Smallest follow-up: load `ProjectScope` in
    `cmd_analyze` or document the boundary.
  - F-6 Expectation vocabulary in stored/API verdict strings (Claim 6).
    Confidence: OBSERVED. Smallest follow-up: API projection via
    `ObservedResultKind`/`ExpectationAssessmentKind`.
- **P3**:
  - F-7 WAL sidecars under read-only serving (Claim 7). Confidence:
    OBSERVED (experiment).
  - F-8 Pre-existing red CI from the CHANGELOG session-narrative format
    (E10). Confidence: OBSERVED (`gh run list` shows
    `92f83d89 failure CI`; the "Session 55" heading was introduced only
    by the HEAD commit). Not fixed in this session per scope rules.
- **Unknown**: F-9 extraction-reuse predicate caveat; F-10 real-run
  occurrence of F-1.

## Candidate ADR register

| Title | Decision already implicit? | Current alternatives | Consequences | Evidence | Worth formalizing? |
|-------|---------------------------|----------------------|--------------|----------|---------------------|
| Immutable evidence vs reviewed interpretation | yes — snapshots/manifests append-only, reviews separate | mutable interpretation rows | provenance preserved | `event_snapshots`, `manifest_revisions` UNIQUE hashes | yes |
| SQLite + filesystem publication boundary | yes — staging → rename → import | DB-only artifacts | orphan windows, divergence detectable | `src/catalog/jobs/publish.rs` | yes |
| Observer-prefix stream as analysis unit | yes — `ObserverPrefixKey` aggregation | route-instance-only analysis | stream-level conclusions | `src/lifecycle.rs` | yes |
| ADD-PATH-aware route identity | yes — `RouteKey.path_id` | prefix-only identity | mixed keyed/unkeyed ambiguity flagged | `src/domain/route.rs` | yes |
| Plan/job/run separation | yes — three distinct tables | merged execution record | independent axes | `analysis_plans`, `analysis_jobs`, `analysis_runs` | yes |
| Project-scope overlay | yes — `config/project-scope.toml`, exact matching | access control | overlay, not authz | `src/catalog/scope.rs` | yes |
| Source profile vs source adapter | yes — `NetworkProfile` + `SourceFamily` | hardcoded per-source logic | family-correct behavior | `src/profiles/`, `src/catalog/archive_plan.rs` | yes |
| Zero-baseline stop before UPDATE acquisition | yes — insufficient-visibility artifact set | proceed with no cohort | no fabricated findings | `src/orchestrate.rs` | yes |
| Read-only web boundary | yes — POST-only mutations, loopback default | write-enabled mode with acknowledgement | safe evaluation browsing | `src/catalog/web/mod.rs`, `src/catalog/web/server.rs` | yes |
| Presentation diagrams as non-canonical projections | yes — SVG derived from reviewed/lifecycle data | embed diagrams in canonical artifacts | layout never claims semantics | `src/catalog/web/path_diagram.rs` | yes |

## Suggested future experiments

1. **Continuity-gate combination test** — unit test
   `assess(unknown_continuity=true, transitions=[])`; expected
   `InsufficientVisibility` per the module doc. Cost: minutes.
2. **Resolver-equivalence test** — property test over artifact rows:
   every consumer resolves the same path. Cost: hours.
3. **Extraction-reuse predicate probe** — with a fixture, run analysis
   for two predicates sharing an origin set and compare cohort
   completeness. Cost: hours.
4. **Real-run F-1 probe** — acquire a run with known UPDATE gaps and no
   transitions; check the report verdict. Cost: archive acquisition
   (outside freeze).
5. **Read-only open mode** — serve the catalog via
   `open_catalog_readonly` and verify no sidecars. Cost: small.

## No-code-change statement

No production Rust behavior, schema, migration, template, CSS, API,
CLI, adapter, predicate, lifecycle algorithm, finding, result,
assessment, project-scope policy, network profile, entity taxonomy,
case-study interpretation, canonical artifact, or source snapshot was
changed in this session. No analysis was rerun, no worker executed, no
source was contacted, no runtime file was committed, and no evidence was
regenerated.

## Evaluation status

External evaluation sessions: **zero**. Pilot registry unchanged
(no navigation edit required).

## Maintained documents created

- `../computational-model.md`
- `../design/data-structures.md`
- `../design/algorithms.md`
- `../design/invariants.md`
- `../design/state-machines.md`
- `../design/algorithm-data-matrix.md`
- this audit

Navigation updated: `../README.md` (docs index), `./README.md`
(audits index). No other documentation was modified.
