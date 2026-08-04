# Local operations — event to completed workbench

This document defines the canonical local workflow from a catalog event
to a completed workbench, including the new durable analysis-job path
(ADR-004). It is the contract the implementation must satisfy.

## The canonical sequence

```
discover or import event
  → inspect immutable source snapshot
  → review event interpretation
  → review target origin mapping
  → review named service plane or transit predicate
  → review event lifecycle and expectation
  → define analysis window
  → choose source family
  → choose collectors
  → review warmup and cooldown
  → create immutable analysis-plan revision
  → validate plan locally
  → queue exact plan revision
  → worker claims job
  → worker discovers archives
  → worker acquires or reuses raw archives
  → worker parses baseline evidence
  → worker freezes the qualifying cohort
  → worker parses event updates
  → worker reconstructs route state
  → worker derives transitions and lifecycle
  → worker writes artifacts to staging
  → worker validates staged artifacts
  → worker atomically imports the completed run
  → web workbench links to completed run
```

## Review boundaries

The software may derive: archive URLs, cache keys, observer metadata,
transition evidence, and routing findings.

The software must not silently derive unreviewed: target origin
identity, operator relationship expectation, named service plane,
transit predicate, or network-specific title interpretation outside
reviewed conventions. A plan that is not Ready cannot be queued.

The plan review page states for every plan:

- **Reviewed input** — origin mapping, predicate/plane, lifecycle,
  expectation, and their provenance.
- **Derived execution plan** — window, warmup, cooldown, source family,
  collectors, cache estimates.
- **Unresolved requirements** — exact blocker reasons, never a bare
  "not ready".

## Queueing

Queueing is an explicit, idempotent operation on an exact immutable plan
revision:

- only one active job per exact plan revision and plan hash;
- a duplicate submit returns the existing active job;
- completed jobs never block a deliberate explicit rerun;
- queueing performs no network access and no analysis.

An open-event plan must carry an explicit reviewed analysis cutoff; a
missing cutoff is rejected at queue time (`invalid_plan: open event
requires an explicit analysis cutoff`). Open events are not
unconditionally blocked — a plan with a reviewed cutoff queues
normally and its run is Provisional ("observed through cutoff"; see
`docs/GLOSSARY.md` — Provisional analysis / snapshot cutoff).

## Execution

A separate worker process (`inim worker`) claims jobs transactionally.
Only one worker can hold a claim; leases expire and a stale lease is
marked `Failed` with `worker_lease_expired`, never silently resumed.
Cancellation is cooperative: the worker checks at stage and archive
boundaries, and a run is never published after an accepted cancellation.

Artifacts are written to `data/jobs/<job-id>/staging`, validated, then
renamed into `data/runs/<job-id>/` and imported in one catalog
transaction. An incomplete job never appears as a completed workbench.

## Failure model

Job failure is execution failure, described by a stable machine error
code (see `docs/GLOSSARY.md` — Analysis job). A completed analysis may
still have the outcome `InsufficientVisibility`; that is not a failed
job. Retry creates a new immutable attempt that reuses the same plan
revision; it never mutates the old job.

## Local operation

```
terminal 1: inim serve --db data/inim.sqlite --root . --enable-writes
terminal 2: inim worker --db data/inim.sqlite --root .
```

The server is read-only without `--enable-writes`. Write mode is
unauthenticated and loopback-only by default; see
`docs/ADRs/DURABLE-ANALYSIS-JOBS.md` for the security model.

## Retention and cleanup

Job history is deliberately retained: completed job records are run
provenance, failed job records are diagnostics. Nothing is deleted
automatically in this session.

- Completed staging directories are removed after verified publication.
- Failed/cancelled staging directories are removed unless the worker
  runs with `--keep-failed-workdir` (a developer flag, never a web
  control); the failure/cancellation summary stays in the job event
  log.
- Orphan final artifact directories and catalog runs with missing
  artifacts are reported by `inim analysis-job audit` (dry-run only) —
  never deleted automatically, because evidence may be valuable.
- Caches are never touched by job cleanup; retries reuse caches through
  their existing identity.

## Audit trail of web mutations

Every job mutation (queue, cancellation request, retry) appends to the
job event log with a source marker (`local-web` or `cli`), prior and
resulting state, and a bounded structured detail. Plan edits create a
new manifest revision whose `reviewed_at` / `reviewer` fields carry the
provenance. The CSRF token is never stored and never logged.

## Planning queue vs execution queue

- **Analysis queue** (`/analysis-queue`) is the planning queue: one row
  per event, with readiness (Needs review / Ready / Blocked) and the
  next analyst action. Unresolved candidates and running analyses are
  never merged into one status enum.
- **Analysis jobs** (`/analysis-jobs`) is the execution queue: durable
  job rows with execution state, stage, factual progress, worker
  freshness, and result links.

Plan status, job status, and run status remain distinct concepts in
code, in the UI, and in the glossary.

## Demo corpus boundary (2026-08)

`inim demo init` builds a deterministic, offline, bounded catalog from
tracked reviewed material only. The contract, encoded in the generated
`demo-manifest.json` (written next to the database, timestamp-free;
the manifest is the authoritative count source and is regenerated by
`inim demo init`):

| Included | Count | Source |
|---|---|---|
| Reviewed manifest events | 4 | `manifests/*.json` (INC0299001, INC0301970, INC0302574, INC0040293) |
| GRNOC corpus events | 9 | `case-studies/manlan-2019/corpus/` snapshots (INC0040293 is represented by its reviewed analysis event; its corpus row is skipped to avoid a duplicate) |
| Imported snapshots | 13 | manifest + corpus snapshots |
| Completed runs | 12 | reviewed out/ evidence incl. the fresh ESnet run and the MAN LAN pilot runs |
| Plans | 12 | reviewed manifest and case-study plan revisions (no automatic plans for corpus tickets) |
| Jobs | 0 | demo never queues |
| Relationships | 36 | corpus relationships (2 TASK references retained unresolved) |
| Reviews | 10 | `pilot/ticket-reviews.json` (corpus tickets only) |
| Workbenches | 5 URLs | expected_workbench_urls in the manifest |

Excluded from the crate/demo: raw MRT archives, derived caches,
runtime SQLite databases, worker staging, screenshots. The corpus
snapshots ARE packaged (public records, redistribution documented);
case-study review material in Git is packaged with it.
