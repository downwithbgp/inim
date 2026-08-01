# Batch archive reuse — real RouteViews/RIS execution (Session 34, Part 8)

Date: 2026-08-01 (Session 34). Runs: the RouteViews pilot (route-views2)
plus the three RIPE RIS pilots (rrc00, rrc06, rrc15) — the same reviewed
NORDUnet target and window.

## Planned vs unique

| run | planned archives | planned bytes |
|-----|-----------------:|--------------:|
| route-views2 (RouteViews pilot) | 70 | ~700 MB |
| rrc00 (RIS pilot) | 230 | ~770 MB |
| rrc06 (RIS pilot) | 230 | ~30 MB |
| rrc15 (RIS pilot) | 230 | ~460 MB |
| **total** | **760** | **~1.96 GB** |

- Planned archive requests: **760** (1 RIB + 229 UPDATE files per run).
- Unique raw archives: **760** — there is **no cross-run URL
  duplication**: every archive is collector-specific (rrc00/rrc06/rrc15/
  route-views2 have disjoint URLs), so no archive is claimed as shared
  where cohort identity differs. This matches the batch planner's rule:
  reuse is only claimed for identical (family, collector, URL) archives.
- Batch planner over the stored case-study plan (RouteViews): 548 unique
  archives, 4,932 archives avoided through reuse across the 10-event
  cohort, ~1.93 GB estimated.

## Actual reuse in execution

- **Raw cache reuse across stages**: the shared cache
  (`cache/ris-preflight`) holds **705 raw archives (~2.95 GB)** downloaded
  once. The three RIS baseline bviews (~610 MB) were downloaded during
  Part 5 collector preflight and **reused by the pilot runs without
  re-download**; the RouteViews pilot reused its Session 31 raw cache.
- **Derived-cache reuse**: the Part 5 preflight produced derived RIB
  caches keyed on (family, collector, origin ASNs, predicate, revision);
  all three RIS pilot runs hit those caches — **3 RIB parses avoided**
  (~10 min each at rrc00/rrc15 scale). Derived reuse only happened where
  the cohort/cache identity matched; nothing was reused across differing
  cohorts.
- **Event/run count**: 4 independent AnalysisRuns; each retained its own
  evidence (evidence IDs do not depend on batch membership), and each
  run's artifacts are byte-identical to a standalone run (the batch is a
  pure grouping of per-event plans — see `batch.rs` tests).

## Confirmed properties

- Raw archives downloaded once per unique source URL.
- Derived archive parsing reused only when cohort/cache identity permits.
- Each AnalysisRun remains independent; evidence IDs do not depend on
  batch membership.
- One failed/blocked event plan does not corrupt another run (blocked
  plans are planned as blocked members; successful members unchanged).
