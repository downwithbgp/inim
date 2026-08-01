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
- queue-based parse workers; parallel downloads with overlap (archive_order
  pre-assigned from discovery order); two bounded queues; cleanup on
  failure.
- 15 required tests: Part 6 (archive_parse_tasks_execute_concurrently,
  each_worker_owns_independent_parser_state,
  parallel_results_merge_in_archive_order,
  jobs_one_and_jobs_twenty_four_have_identical_substantive_artifacts,
  evidence_ids_are_independent_of_worker_completion_order,
  lifecycle_results_are_independent_of_job_count); Part 7
  (download_limit_is_respected, parse_limit_is_independent_of_download_limit,
  archive_failure_does_not_cancel_completed_cache_entries,
  retried_archive_does_not_duplicate_results,
  pipeline_overlap_does_not_change_final_artifacts); Part 8
  (parser_work_queue_is_bounded, failed_run_cleans_temporary_files,
  high_job_count_does_not_change_cache_schema, memory_strategy_is_documented).
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
