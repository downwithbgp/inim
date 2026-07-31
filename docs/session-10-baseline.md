# Session 10 Baseline — INC0302574

**Date:** 2026-07-31
**HEAD:** 9baf621

## Run Configuration

- `cargo run --release -- analyze --event tests/fixtures/internet2/INC0302574.json --manifest manifests/INC0302574.json --cache cache/ --out out/INC0302574-baseline/`
- RIB cache: route-views2 warm (cache hit), route-views6 cold (first parse)
- UPDATE cache: none
- Serial execution (no --jobs flag)

## Metrics

| Metric | Value |
|--------|-------|
| Total wall time (stage timings) | 107.6s |
| Total wall clock (time -v) | 1:48.03 |
| RIB parse time (rv6, cold) | 35.8s |
| RIB cache hit (rv2, warm) | 0.0s |
| UPDATE parse time (total) | 65.8s |
| UPDATE files | 28 (14 per collector) |
| Avg per UPDATE archive | ~2.3s |
| Total BgpElem parsed | ~5.2M |
| Peak memory (RSS) | 91,040 KB (~89 MB) |
| User CPU | 86.36s |
| System CPU | 1.68s |
| % CPU | 81% |

## Preflight

| Metric | Value |
|--------|-------|
| Collectors requested | 2 (route-views2, route-views6) |
| Collectors retained | 2 |
| Frozen streams (rv2) | 18 |
| Frozen streams (rv6) | 19 |
| Total frozen streams | 37 |
| Distinct prefixes | 13 |
| Distinct peers | 7 |

## Admission

| Metric | Value |
|--------|-------|
| Target-prefix matches | 0 |
| Collector+prefix matches | 0 |
| Full TargetKey matches | 0 |
| Admitted announcements | 0 |
| Admitted withdrawals | 0 |

## Outcome

- **Verdict:** NoObservableBgpImpact
- **State changes:** 0
- **Transitions:** 0
- **Waves:** 0
- **Motifs:** 0
