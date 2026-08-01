# Session 32 — pilot validity, multicore performance, UI review harness (design)

## Facts established at start

- `--jobs` semantics today: "1=serial, 0=auto (min(available_parallelism,4)),
  default 1". Per-archive parse parallelism EXISTS (`process_tasks_parallel`,
  chunk-based, joins in archive order, deterministic ids assigned after
  `sort_deterministic`). **Downloads are strictly serial** (Phase B1
  `cache_archive` loop) — the pilot's 141.5 s was dominated by serial
  ~0.6 GiB downloads (network-bound, hence low CPU). Host: 12 logical CPUs
  (brief says 24-core server; report actual). No cgroup v2 limit file found.
- Playwright 1.61.1 + chromium installed (`~/.cache/ms-playwright`,
  chromium-1228) — screenshot harness viable via npx; no browser runtime
  dependency for Rust.
- Pilot facts (Session 31): 11/33 streams absent 16:45:25Z, restored
  16:45:27Z (2 s), 1 peer (64.57.28.241), 11 prefixes; 30 path replacements
  16:45:26–17:02:03Z; baseline return 17:02:19Z. AAR: flap report 15:33,
  disable 16:50, re-enable 20:48. Current wording wrongly implies
  "temporally consistent with the disable" — must become Before {4m35s}
  with no action-attribution.

## Part 1+11 — Temporal interpretation correction

- `case-studies/manlan-2019/pilot/pilot-result.json`: replace
  "Before / within stated precision" wording with explicit
  "Before { duration: 4 minutes 35 seconds }"; interpretation = "transient
  route-state disruption during the broader reported instability period;
  not attributed specifically to either interface action". Finding keeps
  single-target/collector/window scope.
- Comparison model: add `temporal_detail` to ComparisonRow (e.g. "BGP first
  absence 16:45:25Z precedes the reported 16:50 disable by 4m 35s;
  restoration 17:02:19Z precedes the reported 20:48 re-enable"). Before/After
  relations expose order and delta; Overlapping allowed for broad
  instability intervals; no "consequence of" wording ever.
- 5 required tests (wording + order preservation).

## Part 2 — Two-second absence audit

Audit the 11 streams from the immutable artifacts (evidence appendix +
withdrawal audit + lifecycle + transitions; no new entities). For each
stream: collector/peer/prefix, baseline instance set, last withdrawal, first
subsequent instance, absence at native (second) precision, path before/after,
transit state, archive URL+sha, element order, evidence ids. Determine:
single peer? identical timestamps? prefix families? alternative path_id
instances? same-MRT-second withdrawal+announcement ordering? ADD-PATH
aggregate state? Wording becomes "temporary observer-stream absence" unless
evidence supports more. Audit record written to
`case-studies/manlan-2019/pilot/absence-audit.json` (reviewed data).
5 required tests: deterministic same-second ordering, aggregate route set
prevents false absence, native precision retained, no global-loss wording,
observer-scoped summary language.

## Part 3 — performance.json instrumentation

Add a `PerformanceReport` (stages: planning, broker, cache lookup, download,
decompress+parse, normalize, admission, derived-cache write, merge+sort,
reconstruction, lifecycle, waves, outputs, catalog import where applicable)
with wall time, input bytes, output counts, worker counts, cache hits/misses.
Per-archive rows: archive identity (URL+sha), compressed bytes, parse time,
element count, admitted observations, cache-write time. Written to
`performance.json` (new artifact kind, schema v1) — NEVER part of
substantive equivalence checks; never in report.json/verdict. 4 required
tests.

## Part 4 — Concurrency environment

`--show-execution-plan`: prints logical CPUs, `available_parallelism()`,
`--jobs`, effective parse/download/cache-writer counts, cgroup/affinity
limits when detectable (/sys/fs/cgroup/cpu.max v2, cpu.cfs_quota_us v1,
/proc/self/status Cpus_allowed_list). `--jobs 0` becomes an explicit error
(previously "auto") — acknowledged as a breaking CLI change (pre-1.0);
`--parse-jobs`/`--download-jobs` are the replacement; the error message
says so. 5 required tests.

## Part 5+9 — Benchmark + goals

Local raw-cache parse runs (all archives cached; `--rebuild-derived-cache`
forces re-parse; no network) with jobs 1/2/4/8/12/16/24 via
`/usr/bin/time -v`; record wall/user/sys/CPU%/max RSS, archives/s, MiB/s,
elements/s, obs/s, substantive artifact hash. Also cold-network and
derived-warm runs for contrast. Goals: materially >1-core utilization; ≥4x
wall-clock at best safe worker count if parse-bound; if disk-bound, prove
with CPU%/I/O/plateau. Default parse jobs chosen from measured throughput +
memory. Best worker count + diminishing point + default recorded in docs.

## Part 6+7+8 — Parallelism, download split, memory

- Bounded work queues: TWO queues — download queue sized `download_jobs`
  (conservative default 2) and parse queue sized `parse_jobs` (default from
  measurement); pipeline overlap (download N while parsing M).
  **Determinism invariant: `archive_order` is assigned from the discovery
  sort order BEFORE any download task is dispatched** (never after
  completion), so parallel completion cannot reorder archives. Worst-case
  retained memory ~ `(download_jobs + parse_jobs) ×
  max_archive_decompressed_size` + merged observation vec. Cache writes
  atomic
  (temp+rename, existing behavior), failures keep archive identity, retries
  dedupe (sha keyed), merge deterministic (archive_order slots; ids assigned
  after `sort_deterministic` — unchanged).
- Replace chunk-based parse fan-out with queue-based workers pulling tasks;
  each worker owns parser/decompression state (already true per-file).
- Memory: bounded queue capacity = worker counts; per-archive results
  retained until ordered merge; no unversioned temp evidence; cleanup on
  failure; document strategy. No global result mutex.
- Required tests: 6 (concurrency boundary) + 5 (download/parse split) + 4
  (memory).

## Part 10 — Pilot rerun equivalence

Rerun NORDUnet pilot jobs=1 / selected default / jobs=24, local raw caches,
rebuilt derived caches. Substantive artifacts (report, transitions,
lifecycle, waves, evidence, withdrawal audit, comparison) must be identical;
only performance.json differs. Report runtimes/speedup.

## Part 12 — Screenshot harness

`scripts/screenshot-review.sh`: deterministic demo catalog (init+import),
`inim serve` on 127.0.0.1 (loopback), npx playwright screenshot at viewports
1440×900 / 1280×800 / 390×844 for: dashboard, events, RIPE event detail, UVA
analysis detail, blocked event detail, MAN LAN case study, NORDUnet pilot
section, stream-lifecycle table → `tmp/ui-review/*.png` (gitignored;
excluded from package via `tmp/` in Cargo exclude; catalog import unaffected
— it scans manifests/ and out/ only). Trap kills the server on failure;
clear "browser unavailable" message when no browser. Script checks asserted
by a release test (loopback, deterministic catalog, gitignore, package
exclusion, cleanup).

## Part 14 — Docs

README, DESIGN, DATA_PROVENANCE, OBSERVABILITY, ADR-003, RELEASING:
concurrency boundary, sequential reconstruction, worker selection, benchmark
method, acquisition-vs-parse timing, memory constraints, performance
metadata vs substantive output, exact pilot timing interpretation,
screenshot workflow.

## Part 13/15 — Focus + gates

No schema/entity/design rewrites; full gate chain; confirm no result changes
by job count, no full MAN LAN run, no screenshots in package, no browser
runtime dependency, MIT intact, HTTP read-only.
