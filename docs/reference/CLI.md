# inim CLI reference

Verified against the current binary (`inim --help` at the 2026-08
documentation conformance audit). The binary's help text is the
authority for options and defaults; this reference records the
verified surface and classifies each command by its side effects.
`scripts/audit-docs.sh` checks that documented command examples still
exist in the binary's help.

## Program

Single binary `inim` (clap v4). Global flags: `-h/--help`,
`-V/--version`.

### Exit codes

| Code | Meaning |
|---|---|
| 0 | success: plan produced (even `Blocked`), analysis completed, or command ok |
| 1 | malformed input / invalid options / most catalog command errors |
| 2 | analysis incomplete (infrastructure failure during analysis); clap parse errors also exit 2 |
| 3 | plan produced but blocked (no analysis ran); `analysis-job queue` rejects an invalid plan with 3 |
| 4 | `analysis-job queue` — identical job already active (duplicate) |
| 5 | worker execution or publication failure (`worker` returns the inner code when 0 or 2, otherwise 5) |

### Side-effect classes

| Class | Meaning | Commands |
|---|---|---|
| Read-only local | no mutation, no network | `plan`, `compare`, `analysis-plan show`, `analysis-job list/show/audit`, `catalog workbench`, `finding-audit`, `finding-chronology-audit`, `relationships audit`, `corpus-export`, `demo verify`, `project-scope show/audit`, `worker --show-execution-plan` |
| Local catalog mutation | mutates the local SQLite catalog only | `catalog init`, `catalog import`, `catalog case-study import/link-run/pilot-result/apply-research/plan`, `catalog document import`, `catalog corpus-review`, `analysis-job queue/cancel/retry/cleanup` |
| Public-source synchronization | contacts GRNOC Public Task Viewer (live mode) | `catalog sync grnoc` (without `--source-dir`) |
| Archive acquisition + analysis | contacts RouteViews/RIS brokers and archives | `analyze --manifest` |
| Analysis execution | runs the analysis pipeline | `analyze`, `worker` (claims queued jobs) |
| Demo/evaluation generation | builds offline demo or evaluation material | `demo init`, `scripts/evaluator-bootstrap.sh`, `scripts/build-evaluation-pack.sh`, `scripts/build-evaluation-answer-key.py` |
| Audit | read-only reporting | `analysis-job audit`, `relationships audit`, `project-scope audit`, `finding-audit`, `finding-chronology-audit`, `catalog session-audit` |
| Cleanup | dry-run default; deletes only with `--apply` | `analysis-job cleanup` |

There is no hidden network access: every command that acquires data or
contacts a source says so in its help text.

## Top-level commands

### `inim plan` — produce an analysis plan (read-only, no network)

`-e/--event PATH` (required), `-m/--manifest PATH` (required),
`-o/--out DIR` (plan artifacts; stdout only when absent).

Planning runs before any Broker query, archive download, cache lookup,
or MRT parsing. Prints `Ready` or `Blocked` plus the plan JSON. Exit 0
even for a Blocked plan; exit 1 for malformed input.

### `inim migrate-manifest` — legacy manifest conversion (offline)

`--input PATH` (required), `--output PATH` (required),
`--statement TEXT`, `--reviewed-by NAME`, `--date DATE` (all three or
none). Converts schema v1 shortcut fields to the canonical
`TransitPredicateMapping`; never invents unresolved ASNs.

### `inim compare` — compare two completed analyses (read-only)

`--a DIR`, `--b DIR` (required; each holds `report.json`),
`--blocked DIR` (optional blocked-plan artifact),
`-o/--out DIR` (required). Writes `comparison.json` +
`comparison.txt`. Observer-scoped; no severity score.

### `inim analyze` — one-off analysis (network in real mode)

`-e/--event PATH` (required), `-m/--manifest PATH` (optional: selects
the real-analysis path with broker discovery and RouteViews/RIS
acquisition; without it a built-in synthetic demonstration runs),
`-c/--cache DIR` (default `./cache`), `-o/--out DIR` (default `./out`),
`--no-derived-cache`, `--preflight-only` (Stage A: stop after discovery
+ RIB preflight), `--origin-inventory` (classify origin-matching
baseline routes one/both/neither; requires `--manifest`),
`--profile PATH`, `--parse-jobs N` (default 0 = follow `--jobs`),
`--download-jobs N` (default 2), `--show-execution-plan`,
`--rebuild-update-caches`, `--rebuild-derived-cache`,
`-j/--jobs N` (default 8; 0 rejected — use `--parse-jobs`).

Exit: 0 completed, 1 malformed input, 2 incomplete, 3 blocked plan
(zero Broker/MRT work). A blocked plan performs no downloads.

### `inim serve` — local web server (read-only by default)

`--db PATH` (required), `--root DIR` (default `.`),
`--bind ADDR` (default `127.0.0.1:8080`), `--allow-non-loopback`,
`--enable-writes`, `--allow-unauthenticated-writes`.

The server never executes analysis; a separate `inim worker` executes
queued jobs. Write mode is unauthenticated, loopback-only by default,
CSRF-protected, and intended for trusted local use. A non-loopback
bind with writes requires `--allow-non-loopback` **and**
`--allow-unauthenticated-writes`.

### `inim analysis-plan show` — plan review (read-only)

`--db PATH` (required), `--event ID` (required), `--json`.

### `inim analysis-job …` — durable jobs (catalog mutation, no execution, no network)

| Subcommand | Options | Behavior |
|---|---|---|
| `queue` | `--db`, `--plan ID` | queue an exact plan revision; idempotent (one active job per plan+hash); exit 4 on duplicate, 3 on invalid plan |
| `list` | `--db`, `--state STATE` | list jobs (execution state) |
| `show` | `--db`, `--job ID` | one job with recent events |
| `cancel` | `--db`, `--job ID` | cooperative cancellation; queued jobs cancel directly |
| `retry` | `--db`, `--job ID` | new immutable attempt for Failed/Cancelled jobs |
| `audit` | `--db`, `--root DIR` (default `.`) | report stale/expired leases and orphaned artifacts |
| `cleanup` | `--db`, `--root DIR` (default `.`), `--older-than AGE` (default `7d`), `--apply` | delete terminal-job staging only; **dry-run by default**; never deletes runs, referenced artifacts, caches, or tracked evidence |

### `inim worker` — durable job execution (separate process)

`--db PATH` (required), `--root DIR` (default `.`; staging under
`data/jobs`, runs under `data/runs`), `--worker-id ID`,
`--poll-interval DURATION` (default `2s`), `--max-jobs N` (default 1),
`--download-jobs N` (default 2), `--parse-jobs N` (default 8),
`--once`, `--offline`, `--show-execution-plan`,
`--keep-failed-workdir`.

Mutates the catalog (claim/progress/publication) and may access
configured public archive sources (RouteViews, RIPE RIS) unless
`--offline` is set. `--offline` rejects any job requiring uncached
network acquisition. `--once` claims and executes at most one job.
`--keep-failed-workdir` is a developer flag, never a web control.

### `inim demo init` / `inim demo verify` — offline demo (no network)

`init`: `--db PATH`, `--root DIR` (default `.`), `--force`. Builds a
deterministic catalog from tracked reviewed material; refuses to
overwrite an existing database unless `--force`.
`verify`: `--db PATH`, `--root DIR`. Checks expected events,
workbenches, artifact references, no source access, no absolute-path
leaks.

### `inim project-scope show` / `audit` — read-only policy admin

`show`: `--root DIR` (default `.`). `audit`: `--db PATH`,
`--root DIR`. The tracked policy file
(`config/project-scope.toml`) is the reviewed authority; these
commands never modify the policy and never delete catalog records.

## `inim catalog …` — catalog administration

| Subcommand | Options | Behavior |
|---|---|---|
| `init` | `--db` | initialize a new catalog (applies all migrations) |
| `import` | `--db` | import canonical manifests and analysis artifacts |
| `workbench` | `--db`, `--subject ID` | text workbench report (same model as web/API) |
| `finding-audit` | `--db`, `--subject ID`, `--out PATH` | exact finding-audit record |
| `finding-chronology-audit` | `--db`, `--subject ID`, `--out PATH` | checked per-prefix chronology audit |
| `case-study import` | `--db`, `--path` | import `case-study.json` |
| `case-study link-run` | `--db`, … | link an analysis run to a case study |
| `case-study pilot-result` | `--db`, … | apply a reviewed pilot-result record |
| `case-study apply-research` | `--db`, `--path` | apply a reviewed target-research record |
| `case-study plan` | `--db`, … | build the historical-archive plan |
| `document import` | `--db`, `--file`, `--source-url`, `--title`, `--doc-type`, `--provenance`, `--root` | import a reference document |
| `sync grnoc` | `--db`, `--source-dir`, `--seed`, `--case-study`, `--expand-references`, `--search`, `--domain`, `--max-requests`, `--requests-per-second` (default 5.0), `--allow-higher-rate`, `--contact`, `--dry-run`, `--show-access-policy`, `--show-domains` | GRNOC Public Task Viewer sync: offline with `--source-dir`; live mode otherwise (exact-ID lookups and bounded scoped search only; no enumeration) |
| `relationships rebuild` / `audit` | `--db` (+ `--source-kind` for audit) | rebuild or audit the reviewed relationship graph |
| `analysis-queue` | `--db` | corpus-level BGP-analysis readiness queue |
| `archive-batches` | `--db` | plan shared raw-archive batches |
| `corpus-export` | `--db`, `--out PATH` | metadata-only corpus export (no raw payloads) |
| `corpus-review` | `--db`, `--source-kind` (default `grnoc-public-task-viewer`), `FILE` | import reviewed interpretations (separate from snapshots) |
| `session-audit` | `--root` (default `case-studies/manlan-2019/pilot`), `--profile`, `--locations`, `--cache DIR:FAMILY` (repeatable), `--date` (default 20190821), `--origin-asns` (default 2603), `--extraction-cache`, `--jobs` (default 4), `--full-inventory`, `--out` | audit historical collector sessions from baseline RIB peer metadata |
| `session-metadata-backfill` | `--db`, `--cache DIR:FAMILY` (repeatable), `--date` | backfill observed peer-session metadata from cached baseline RIBs |

## Shell scripts (documented separately)

| Script | Purpose | Network |
|---|---|---|
| `scripts/evaluator-bootstrap.sh` | one supported evaluator bootstrap: build → demo init → verify → audit → read-only server | only Cargo dependency fetch; demo/serving is offline |
| `scripts/build-evaluation-pack.sh` | build the reproducible evaluation pack outside Git | none |
| `scripts/build-evaluation-answer-key.py` | generate the answer key from tracked artifacts | none |
| `scripts/audit-docs.sh` (+ `scripts/audit_docs.py`) | documentation drift audit | none |
| `scripts/build-repo-audit.py` | render the repository-truth audit from the inventory | none |
| `scripts/screenshot-review.sh` | loopback screenshot harness (Playwright chromium) | loopback only |
| `scripts/audit_pilot_absence.py`, `scripts/audit-esnet-assessment.py` | evidence QA derivations | none |
| `scripts/build-cross-observer-matrix.py`, `scripts/build-rrc11-audit.py`, `scripts/build-rrc11-i2px-decision.py`, `scripts/compare_runs.py`, `scripts/ris_collector_preflight.sh`, `scripts/pilot_rerun_equivalence.sh`, `scripts/bench_parse_scaling.sh`, `scripts/session35-benchmark.sh`, `scripts/screenshot-review-session*.sh` | maintainer analysis/QA tooling | none (loopback for screenshots) |

## Notes

- `--db` and `--root` values are repository-relative or operator-chosen
  paths; nothing requires an absolute path, and the demo manifest
  rejects absolute paths.
- The CLI shares the same job service as the web layer; direct CLI
  execution and queued worker execution of the same plan produce
  semantically identical evidence.
- Commands that delete (`analysis-job cleanup`) require `--apply`;
  audit commands never delete.
