# Benchmark — historical measurements (2026-08)

These are **measurements, not guarantees**: single-machine timings from
the development server described below, recorded for the historical
2026-08 performance review of the RouteViews/RIS preflight and pilot
work. They depend on hardware, load, cache state, and archive content;
they are never promises about other machines and never part of
substantive artifact-equivalence checks. Raw timings:
`tmp/bench/run.log` (a local, gitignored runtime file, not part of the
repository).

The review covered the historical pilot record ("approximately 45
minutes for 18 RIS preflights; ~30 minutes each for rrc00 and rrc15
pilots; ~4 minutes for rrc06; ~1.25 GiB downloaded") and the fixes
implemented at that time. Machine: the development server below; all
runs local-cache (network acquisition excluded except where noted).
Cache paths used by the benchmark: raw archives under
`cache/ris-preflight/raw/`, RIB derived cache under
`cache/ris-preflight/rib/`, UPDATE derived cache under
`cache/ris-preflight/derived/updates/`, source-extraction cache under
`cache/ris-preflight/extracted/`.

## CPU topology — the 24-versus-12 discrepancy

| probe | result |
|---|---|
| `lscpu` | **12** CPUs, 1 socket, 6 cores/socket, 2 threads/core (Intel Xeon E5-2630L v2 @ 2.40 GHz) |
| `nproc` / `nproc --all` | 12 / 12 |
| `/proc/cpuinfo` processors | 12 |
| `std::thread::available_parallelism()` | 12 |
| process affinity (`Cpus_allowed_list`) | 0-11 |
| `taskset -pc $$` | 0-11 |
| cgroup `cpu.max` | unset (no quota) |
| cgroup `cpuset.cpus.effective` | 0-11 |

**Explanation of 24 vs 12:** the physical host is a 24-thread machine
(2 sockets × 6 cores × 2 threads of the E5-2630L v2); the **guest VM**
this project runs in is configured with **12 vCPUs** (one socket's
worth) and no cgroup quota. Every host-level and process-level probe
agrees on 12; there is no affinity/cgroup restriction inside the guest.
The earlier reports of "12 logical CPUs" were correct for this process
environment, and the 24-core figure describes the physical host, not
this guest. The process-visible count (12) is the effective
parallelism, and the jobs sweep below confirms saturation at 12
workers. The CLI default parse concurrency is a fixed **8**
(`--parse-jobs` default); the 12-worker optimum below requires an
explicit `--parse-jobs 12`. `perf::cpu_topology()` reports host vs
process vs cgroup visibility as distinct fields
(`cpu_limit_reporting_distinguishes_host_and_process_visibility`).

## Stage metrics

### Pilot record (historical, as recorded at the time; 8 parse workers)

| stage | rrc00 pilot | rrc06 pilot | rrc15 pilot |
|---|---:|---:|---:|
| broker + raw cache | 0.3 s | 0.3 s | 0.3 s |
| RIB parse (per archive metric / logs) | ~13 min (recorded; parsed once in preflight, derived-cache hit for pilots) | ~2 min | ~6 min |
| UPDATE cache+parse (wall) | 1341 s | 81 s | 1565 s |
| UPDATE parse (CPU total, 229 files) | 2673 s | 104 s | 3116 s |
| UPDATE elements parsed | 48.9 M | 1.77 M | 81.8 M |
| reconstruction / tokenize / waves / assess / outputs | < 0.1 s each | < 0.1 s | < 0.1 s |

### Current pipeline (measured clean, this machine, local cache)

| stage | rib-j1 | rib-j12 | upd-j1 | upd-j4 | upd-j12 |
|---|---:|---:|---:|---:|---:|
| broker + raw cache | 0.4 s | 0.4 s | 0.4 s | 0.4 s | 0.4 s |
| RIB parse (fresh, route-views2 `rib.20190821.0200.bz2`, 128.5 MB compressed) | 196.6 s | 189.5 s | — (derived hit) | — | — |
| RIB parse (extraction reuse) | — | — | — | — | ~1.3 s |
| UPDATE cache+parse (wall, 69 files, fresh) | — | — | 267.5 s | 72.8 s | 32.7 s |
| UPDATE user CPU | — | — | 272 s | 283 s | 302 s |
| max RSS | 24.3 MB | 24.3 MB | 45.8 MB | 44.3 MB | 76.6 MB |

Findings: the RIB parse is a **single sequential stream** — jobs do not
parallelize one RIB (j1 196.6 s vs j12 189.5 s; user ≈ wall). The
UPDATE phase parallelizes linearly up to ~12 workers (saturation;
oversubscription at 16/24 is neutral-to-slightly-worse). Normalization
and admission are inside the parse loop (admitted counts recorded per
archive); derived-cache writing is per-archive
(`derived_cache_write_secs`); deterministic merge, reconstruction,
tokenize, waves, assess, and report generation are together well under
1 s — **not** the cost driver.

## Local-cache jobs benchmark (route-views2 UPDATE pilot, 69 files, fresh)

| jobs | wall | user | sys | CPU util (user/wall) | max RSS |
|---:|---:|---:|---:|---:|---:|
| 1 | 267.5 s | 272 s | 1.1 s | 102% | 45.8 MB |
| 4 | 72.8 s | 283 s | 0.4 s | 389% | 44.3 MB |
| 8 | 38.4 s | 285 s | 0.4 s | 742% | 58.8 MB |
| 12 | 32.7 s | 302 s | 0.7 s | 923% | 76.6 MB |
| 16 | 33.8 s | 329 s | 0.9 s | 974% | 92.3 MB |
| 24 | 35.6 s | 348 s | 0.9 s | 978% | 124 MB |

12 workers is the effective optimum on this 12-vCPU guest (consistent
with the topology audit). Parsed elements per second at 12 workers:
~1.2 M elements/s (40.4 M elements / 32.7 s).

## Repeated-work audit

**Same RIB re-parsed for different selectors?** Previously yes: the RIB
derived cache was keyed by the transit predicate, so an R&E-plane
preflight and a peering-plane preflight each parsed the same
route-views2 RIB (~3 min each). **Fixed** with a versioned,
origin-scoped **source-extraction cache** (`cache/extracted/`, keyed by
source sha + family + collector + sorted origin set + parser/schema
versions — NOT by predicate): the RIB is parsed once, its
origin-matching observations are persisted, and every independent
selector (both plane preflights, the origin-only inventory, the session
audit) filters the same extraction in memory. Outputs are identical
standalone vs reused (tests:
`standalone_and_reused_outputs_are_identical`,
`same_rib_is_not_reparsed_for_two_plane_batch_when_reuse_is_safe`,
`reused_source_parse_does_not_merge_cohorts`); evidence ids are
content-derived and never change
(`performance_metadata_does_not_change_evidence_ids`). This is NOT a
full-table BGP warehouse: the extraction is origin-scoped (hundreds of
routes per RIB) and content-addressed.

**Update archives?** UPDATEs are filtered by the frozen prefix cohort,
and withdrawals carry no origin attribute, so a shareable
predicate-independent update extraction would require union-prefix
batch planning or origin-aware withdrawal handling. Measured cost of a
fresh route-views2 UPDATE parse: 32.7 s at 12 workers — small relative
to the pilot's other work and already deduplicated per cohort by the
existing update derived cache (keyed by cohort hash). Not material;
documented rather than implemented.

## Acceptance

Repeated two-plane local-cache preflight (route-views2 RIB, R&E +
peering planes):

| pair | plane 1 | plane 2 | total |
|---|---:|---:|---:|
| A — both fresh (`--no-derived-cache`) | 187.2 s | 194.9 s | **382.1 s** |
| B — first fresh, second via extraction reuse | 1.4 s (audit extraction) | 1.3 s | **2.6 s** |
| C — both reuse (repeat) | 1.2 s | 1.3 s | **2.5 s** |

**~150× improvement** on the repeated two-plane preflight (382 s → 2.5 s),
far above the 2× target. The remaining single-plane cost is the
unavoidable single-archive gzip/MRT parse + origin filter (~190 s for
the 128.5 MB route-views2 RIB), which is I/O- and decompression-bound.

Output equivalence: the R&E-plane pilot reports produced with cache
reuse match the pilot evidence recorded earlier (11/33 absent, 12/12
rrc06, 13/24 rrc15, 11/11 rrc00 — same transitions and timestamps), and
the required tests assert byte-identical outputs across cache paths and
worker counts.

## Operational-workflow measurement — 2026-08-02

Dated measurement of the queued-analysis workflow on the demo catalog
(local alpha targets, not universal guarantees). Host: Linux x86_64,
release build.

| Operation | Median | Max | Notes |
|---|---|---|---|
| Queue POST (CLI, same service as web) | 7.7 ms | 14.8 ms | target < 100 ms; duplicate submits are idempotent |
| Worker claim transaction | 5.5 ms | — | BEGIN IMMEDIATE + claim update + event, DB idle |
| Job page query set (state + events + list) | 0.07 ms | — | target < 100 ms median |
| Publication transaction | bounded by artifact hashing | — | one catalog transaction after the rename |

Query plans confirm index use: the queue-claim query scans
`idx_jobs_active (state, requested_at)` and the idempotency query scans
`idx_jobs_plan (plan_revision_id)`. Progress updates are throttled to
stage/archive boundaries and a bounded time interval, so SQLite write
contention while the worker runs stays negligible (verified by the
concurrent server-read/worker-write test).

No parser code was changed for these measurements; queue overhead is
dominated by process startup for the CLI (the web POST path is
in-process).

## GRNOC corpus + operational workflow — 2026-08-02

Dated measurement with the corpus-enriched demo (release build).

| Measurement | Result |
|---|---|
| Demo initialization (corpus + case study + runs) | 0.6 s |
| Demo database size | 0.4 MiB |
| Analysis-queue page loader (median, 20 runs) | 8.0 ms |
| Job list loader (median, 20 runs) | 0.05 ms |
| Cleanup scan, 10 synthetic terminal jobs | 15 ms |
| Cleanup scan, 100 synthetic terminal jobs | 34 ms |

Cleanup enumeration is linear in the job count and only walks the
eligible staging directories (no recursive hashing of cache trees).
Local-alpha targets remain: median GET < 100 ms, reviewed max < 250 ms.
