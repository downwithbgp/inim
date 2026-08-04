# inim web route reference (maintainers)

Server-rendered HTML route tree of `inim serve`, verified against the
router at the 2026-08 documentation conformance audit. The router
(`src/catalog/web/mod.rs`) is the authority. This is a maintainer
reference; no permanent URL compatibility is promised beyond the
current alpha. `scripts/audit-docs.sh` checks the routes against the
router metadata.

## Read-only HTML pages (GET)

| Route | Page |
|---|---|
| `/` | dashboard (alias `/catalog`) |
| `/events` | event list |
| `/events/{event_id}` | event detail |
| `/events/{event_id}/workbench` | event workbench (primary result page) |
| `/events/{event_id}/relationships` | event relationship graph |
| `/events/{event_id}/analysis-plan` | plan review (read-only) |
| `/case-studies` | case-study list |
| `/case-studies/{slug}` | case-study detail |
| `/case-studies/{slug}/workbench` | case-study workbench |
| `/analyses/{run_id}` | analysis run detail |
| `/analyses/{run_id}/streams` | run stream list |
| `/corpus` | corpus overview |
| `/corpus/sync-runs` | sync-run records |
| `/corpus/relationships` | reviewed relationship graph audit |
| `/analysis-queue` | planning queue |
| `/analysis-jobs` | job index (active/queued/failed/completed) |
| `/analysis-jobs/{job_id}` | job detail (state, stage, progress, events, heartbeat) |
| `/incident-candidates` | candidate groups |
| `/archive-batches` | shared archive batches |
| `/documents/{document_id}` | served reference document (containment-checked, SHA-verified) |
| `/static/app.css` | stylesheet |

All GET routes are read-only: they never execute analysis, never
acquire archives, and never mutate the catalog. Excluded (project
scope) items return 404 or are omitted from lists.

## Write-mode-only controls (POST)

| Route | Purpose | Rendered when |
|---|---|---|
| `/events/{event_id}/analysis-plan` | plan edit/review | write mode |
| `/analysis-plans/{plan_revision_id}/queue` | queue plan revision | write mode |
| `/analysis-jobs/{job_id}/cancel` | cancellation request | write mode |
| `/analysis-jobs/{job_id}/retry` | retry (new attempt) | write mode |

With write mode disabled these POST routes do not exist (404), no
mutation controls render, and pages show the `--enable-writes` hint.

## Query parameters (workbench and lists)

- Workbench: `?changed=1`, `?kind=absent|path|plane|unchanged|withdrawn|prepend|mixed`,
  `?region=AMER|EMEA|APAC`, `?rel=direct|indirect`, `?expand=1`,
  `?episode=`, `?prefixes=`, `?view=timeline`.
- Event list: `lifecycle`, `status`, `expectation`, `source`,
  `date_from`, `date_to`, `q`.
- Filters are deterministic query state; they never mutate.

## Evaluator URLs (verified against the deterministic demo)

The five scenario starting URLs resolve on the read-only demo:

- `/case-studies/manlan-2019/workbench`
- `/events/INC0299001/workbench`
- `/events/INC0302574/workbench`
- `/events/INC0301970/workbench`
- `/events/INC0040293/workbench`

`scripts/audit-docs.sh` and the CI evaluation-smoke job verify these
against `evaluation/scenarios.toml`.
