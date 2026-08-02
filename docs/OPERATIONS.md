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
