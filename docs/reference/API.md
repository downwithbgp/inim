# inim HTTP API reference

**Status: public alpha.** The API is versioned (`/api/v1`) but the
project makes no long-term compatibility promise beyond the current
alpha. `scripts/audit-docs.sh` checks this route table against the
router definitions.

The server is `inim serve` (see `docs/reference/CLI.md`). It is
read-only by default; mutation endpoints return 404 when write mode is
disabled. All timestamps are UTC. Paths are repository-relative; no
absolute path is ever returned.

## Versioning and envelope

- All JSON endpoints live under the prefix **`/api/v1`**
  (`API_VERSION = 1`).
- Read endpoints wrap payloads as
  `{"api_version": 1, "data": {...}}`; most `data` payloads carry a
  `schema_version` at their root.
- Errors use
  `{"api_version": 1, "error": {"message": "<text>"}}`. There is no
  machine `code` field in the HTTP error envelope; stable machine
  strings appear in job event data as `message_code` (see Job API).
- Write success returns
  `{"api_version": 1, "result": "queued"|"already_queued"|"cancelled"|"cancel_requested"|"retry_created", "job_id": ...}`.

## Read endpoints (GET/HEAD)

| Route | Purpose |
|---|---|
| `/api/v1/events` | event list; pagination `page` (0-based, default 0), `per_page` (default 25, 1–200); scope-filtered |
| `/api/v1/events/{event_id}` | event detail |
| `/api/v1/events/{event_id}/workbench` | event workbench view model |
| `/api/v1/events/{event_id}/relationships` | event relationship graph |
| `/api/v1/analyses/{run_id}` | run detail (scope-checked) |
| `/api/v1/analyses/{run_id}/streams` | stream list; `page`/`per_page`, `category`, `collector` filters |
| `/api/v1/analyses/{run_id}/observer-episodes` | episode rows |
| `/api/v1/analyses/{run_id}/regional-breadth` | regional breadth |
| `/api/v1/case-studies` | case-study list; `page`/`per_page` |
| `/api/v1/case-studies/{slug}` | case-study detail |
| `/api/v1/case-studies/{slug}/timeline` | timeline |
| `/api/v1/case-studies/{slug}/comparison` | comparison matrix |
| `/api/v1/case-studies/{slug}/workbench` | case-study workbench view model |
| `/api/v1/catalog/status` | catalog status |
| `/api/v1/corpus/status` | corpus status |
| `/api/v1/corpus/sync-runs` | sync-run records |
| `/api/v1/analysis-queue` | planning queue |
| `/api/v1/analysis-jobs` | job list |
| `/api/v1/analysis-jobs/{job_id}` | job detail with events |
| `/api/v1/analysis-plans/{plan_revision_id}` | plan revision detail |
| `/api/v1/incident-candidates` | candidate groups |
| `/api/v1/archive-batches` | archive batches |

## Write endpoints (POST — write mode only)

| Route | Purpose |
|---|---|
| `/events/{event_id}/analysis-plan` | edit/review a plan (web form) |
| `/analysis-plans/{plan_revision_id}/queue` | queue an exact plan revision |
| `/analysis-jobs/{job_id}/cancel` | request cooperative cancellation |
| `/analysis-jobs/{job_id}/retry` | create a new attempt |
| `/api/v1/analysis-plans/{plan_revision_id}/queue` | API queue |
| `/api/v1/analysis-jobs/{job_id}/cancel` | API cancel |
| `/api/v1/analysis-jobs/{job_id}/retry` | API retry |

## Write-mode requirements

- **Write mode disabled** (default): every POST returns **404 Not
  Found** (web: plain text; API: JSON error `writes are disabled on
  this server`). Mutation controls are not rendered on GET pages.
- **CSRF:** every POST requires the header **`X-Inim-CSRF`** with the
  process-lifetime token (web forms may alternatively send `_csrf` as a
  form field; the header wins when both are present). Missing or
  mismatched token → **403 Forbidden**.
- **Body size:** router-wide limit of **64 KiB** for mutation bodies;
  over-limit requests are rejected (413).
- **Non-loopback:** write mode on a non-loopback bind additionally
  requires `--allow-unauthenticated-writes` at server startup.

## Status codes

| Code | Meaning |
|---|---|
| 200 | success (reads and writes) |
| 400 | bad request: pagination out of range (`per_page must be between 1 and 200`), plan not Ready, blocked plan, rejected edit, service errors on queue/cancel/retry |
| 403 | invalid or missing CSRF token |
| 404 | not found; excluded (project-scope) items are indistinguishable from nonexistent; API POSTs when writes are disabled |
| 405 | wrong method on a known path |
| 413 | mutation body over 64 KiB |
| 500 | internal/database error |

## Project-scope behavior

- Excluded events are **omitted** from list endpoints and return
  **404** on detail/workbench routes (indistinguishable from
  nonexistent events — exclusion is policy, not a verdict).
- Excluded plan revisions cannot be queued or retried; the worker
  rechecks scope after claim and cancels pre-execution jobs
  (`excluded_by_project_scope`).

## Job API machine strings

Job events carry a stable `message_code` vocabulary:
`source_discovery_failed`, `archive_not_found`, `archive_rate_limited`,
`archive_forbidden`, `archive_checksum_mismatch`, `archive_not_cached`,
`baseline_parse_failed`, `update_parse_failed`,
`evidence_derivation_failed`, `artifact_validation_failed`,
`artifact_publication_failed`, `catalog_import_failed`,
`worker_lease_expired`, `cancelled`, `excluded_by_project_scope`,
`internal_error`, plus plan-level `invalid_plan`,
`incompatible_plan_schema`, and `project_scope_excluded`.

## Duplicate queue behavior

Queueing is idempotent: a duplicate submit of the same exact plan
revision and canonical plan hash returns the existing active job with
`"result": "already_queued"` (HTTP 200), not an error.

## Notes

- GET requests never mutate, never execute analysis, and never acquire
  archives.
- The web UI route tree (HTML pages) is documented separately in
  `docs/reference/WEB-ROUTES.md`.
