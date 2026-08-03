# ADR-004 — Durable local analysis jobs and worker boundary

Status: Accepted (2026-08) · Extends: ADR-002 (local catalog + web), ADR-003 (case-study layer)

## Context

The catalog and workbench let an operator inspect completed analyses, but
moving from a catalog event to a completed workbench requires manual
coordination of several CLI commands and repository-specific paths. The
web server is read-only by design and must stay that way by default.

This ADR introduces a durable, explicit, auditable analysis-job workflow:
a reviewed immutable plan revision is queued, a separate worker process
claims and executes it, and completed runs publish atomically into the
existing immutable catalog. The accepted workbench design stays frozen.

## Decisions

### Why HTTP handlers do not execute analysis

The web server must never parse MRT, download archives, or run analysis.
A request handler that executes analysis would couple request lifetime to
hours-long work, block the single catalog connection, make cancellation
impossible, and turn GET/POST side effects into an operational hazard.
The web layer may only validate a mutation request and create or
transition job records through the shared domain service. All source
access and analysis happens in the worker process.

### Why SQLite is sufficient

One local operator, one host, one worker, at most a handful of active
jobs. The catalog database already holds the events, manifests, plans,
and runs; jobs are a small set of rows plus an append-only event log.
SQLite with WAL, a busy timeout, and short transactions handles this
workload with zero new infrastructure. A message broker or job queue
crate would add deployment and licensing surface without any required
capability.

### Why jobs are durable

The job row and its event log live in the same SQLite catalog as the
evidence they produce. A worker restart, host reboot, or cancelled
terminal session must not lose a queued analysis. Durability also gives
an audit trail: who requested what, when, and what the worker observed.

### Why one local worker is the initial model

The worker executes one job at a time (`--max-jobs 1` default) with
bounded download/parse parallelism. Two workers are prevented from
claiming the same job by a transactional `BEGIN IMMEDIATE` claim. The
single-worker model is the reviewed alpha shape; a second worker later
only needs the same claim protocol, not a new design.

### Why completed runs remain immutable

Analysis runs and their artifacts are canonical evidence. The job's
`completed_run_id` links to an immutable `analysis_runs` row; jobs,
plans, and runs are never mutated after completion. Retry creates a new
job and a new run; it never rewrites the old one.

### Why job state differs from analysis outcome

Job state is execution state: Queued, Claimed, stage names, Completed,
Cancelled, Failed. The analysis outcome is a property of the published
run (e.g. `InsufficientVisibility`), which can be a perfectly valid
completed job. Conflating them would make a valid no-visibility analysis
look like a worker failure. A job fails only when execution itself
failed; the run carries the semantic outcome.

### Why retries create new execution attempts

A failed or cancelled job is immutable. Retry inserts a new job row that
links back via `original_job_id`, increments `attempt`, and reuses the
same immutable plan revision. Raw and derived caches may be reused
through their existing identity; staging directories are never reused.
This preserves the failed attempt as diagnostics while making recovery
explicit.

### Why there is no automatic target inference

The software may derive archive URLs, cache keys, observer metadata,
transition evidence, and routing findings. It must never silently derive
the reviewed inputs: target origin identity, operator relationship
expectation, named service plane, or transit predicate. Those come only
from reviewed manifests. Plans that are not Ready cannot be queued, and
free-form origin-ASN entry is marked NeedsReview until explicitly
reviewed.

### Why web writes are disabled by default

The server stays read-only unless started with `--enable-writes`. Write
mode is intended for trusted local use only: default bind remains
loopback, a non-loopback bind with writes requires
`--allow-unauthenticated-writes`, every mutation POST requires a
process-lifetime CSRF token, and bodies are size-bounded. There is no
authentication; write mode must never be exposed to untrusted networks.

## Consequences

- New schema objects: `analysis_jobs`, `analysis_job_events`,
  `worker_heartbeats` (migration V10).
- New CLI: `inim worker` (separate process), `inim analysis-plan show`,
  `inim analysis-job queue|list|show|cancel|retry`.
- New web routes: plan review, job list/detail, queue/cancel/retry POSTs,
  versioned API twins.
- Staging: artifacts are written under `data/jobs/<job-id>/staging`,
  validated, then renamed into `data/runs/<job-id>/` before the catalog
  import transaction — an incomplete job never appears as a run.
- Direct synchronous `inim analyze` remains for development and
  controlled one-off analysis, sharing the same execution service and
  artifact validator; the job worker is the production path.
- No new runtime dependency is required for the job model (identifiers
  come from SQLite `randomblob`, not a UUID crate).

## Follow-ups (dated)

None yet. If a second worker is adopted, this ADR's claim semantics
already cover it; only the stale-lease review policy would be revisited.
