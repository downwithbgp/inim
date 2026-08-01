# Session 32 — tasks

## T1. Spec + review — gate: /review pass.

## T2. Temporal interpretation (Parts 1, 11)
- pilot-result.json corrected wording (Before {4m35s}; no action attribution);
  ComparisonRow.temporal_detail; 5 required tests.
- Gate: T, F, C. Commit.

## T3. Two-second absence audit (Part 2)
- audit from immutable artifacts → absence-audit.json; wording tests (5).
- Gate: T, F, C. Commit.

## T4. performance.json instrumentation (Part 3)
- stage + per-archive metrics; new artifact; excluded from equivalence; 4 tests.
- Gate: T, F, C. Commit.

## T5. Concurrency environment (Part 4)
- --show-execution-plan; --parse-jobs/--download-jobs; jobs=0 rejected; 5 tests.
- Gate: T, F, C. Commit.

## T6. Parallelism + memory (Parts 6, 7, 8)
- queue-based parse workers; parallel downloads with overlap; bounded
  queues; cleanup on failure; 15 required tests.
- Gate: T, F, C. Commit.

## T7. Benchmark + default selection (Parts 5, 9)
- /usr/bin/time -v runs jobs 1..24 (local raw caches, rebuilt derived);
  table + goals + chosen default.
- Gate: measurements recorded. Commit benchmark script if added.

## T8. Pilot rerun equivalence (Part 10)
- jobs=1 / default / 24; substantive artifacts identical; runtimes.
- Gate: equivalence diff verified.

## T9. Screenshot harness (Part 12)
- scripts/screenshot-review.sh; tmp/ui-review/ gitignored + package-excluded;
  screenshots generated; script checks in release test.
- Gate: screenshots exist; F, C. Commit.

## T10. Docs (Part 14) + gates (Part 15) + completion report.
