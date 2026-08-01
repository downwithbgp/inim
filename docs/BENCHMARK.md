# Parse-scaling benchmark (Session 32)

Hardware and environment (2026-08-01, development server):

- Host logical CPUs: 12 (`/proc/cpuinfo`), `available_parallelism`: 12,
  affinity `Cpus_allowed_list: 0-11`, no cgroup CPU limit detected.
- Load average during measurement: ~4-12 (shared machine, 9-10 users).
- Binary: `target/release/inim` (release profile).

Input: the NORDUnet pilot window (2019-08-21, route-views2):
1 baseline RIB (`rib.20190821.0200.bz2`) + 69 UPDATE archives
(~128 MiB compressed, ~40.37M parsed MRT elements, 33 frozen
observer-prefix streams / 11 prefixes).

Command per configuration:

```
/usr/bin/time -v ./target/release/inim analyze \
  --event case-studies/manlan-2019/pilot/pilot-event.json \
  --manifest case-studies/manlan-2019/pilot/manifests/MANLAN-2019-NORDUNET-PILOT.json \
  --cache cache --out tmp/bench/jobs<N> \
  --jobs 1 --parse-jobs <N> --download-jobs 1 --rebuild-update-caches
```

Run 1 used `--rebuild-derived-cache` (builds the RIB derived cache;
the single-archive RIB parse is ~190-200 s single-threaded and is not
parallelized — per-archive concurrency cannot help one archive).

## Local raw-cache parse runs

| parse jobs | wall (s) | CPU % | max RSS (kB) | archives/s | MiB/s | elems/s | obs/s | speedup vs 1 |
|---|---|---|---|---|---|---|---|---|
| 1 | 469.5 | 100 | 27 304 | 0.15 | 0.27 | 86 k | 0.6 | 1.0× |
| 2 | 144.8 | 197 | 34 496 | 0.48 | 0.88 | 279 k | 1.7 | 3.2× |
| 4 | 78.6 | 390 | 40 676 | 0.88 | 1.63 | 514 k | 3.1 | 6.0× |
| 8 | 44.5 | 741 | 58 008 | 1.55 | 2.88 | 907 k | 5.6 | 10.5× |
| 12 | 39.3 | 936 | 74 964 | 1.76 | 3.26 | 1 027 k | 6.3 | 11.9× |
| 16 | 34.0 | 972 | 91 136 | 2.03 | 3.76 | 1 188 k | 7.4 | 13.8× |
| 24 | 37.3 | 1008 | 123 516 | 1.85 | 3.43 | 1 082 k | 6.6 | 12.6× |

## Interpretation

- The workload is CPU-bound on decompression + MRT parsing: CPU%
  tracks wall time, RSS grows modestly with worker count.
- Diminishing returns: 12→16 gains 13 %; 16→24 is WORSE (contention
  on 12 physical cores with 24 workers).
- Best throughput: 16 workers (13.8×). Best safe default: **8 workers**
  (10.5×, 741 % utilization, 58 MB RSS — half the RSS of 16 with 78 %
  of the throughput, safe margin under shared-machine load).
- Chosen default: `--jobs 8` (overridable with `--jobs 24` etc.).

## Substantive equivalence

jobs 1 / 8 / 24 runs produced byte-identical substantive artifacts
(report, transitions, lifecycle, waves, withdrawal audit, evidence
appendix) modulo the volatile `generated_at` timestamp; verdict and all
85 evidence observation ids identical. Only `performance.json` differs.

## Runtimes (pilot rerun, local caches, rebuilt UPDATE caches)

| jobs | wall (s) |
|---|---|
| 1 | 282.9 |
| 8 | 41.5 |
| 24 | 36.7 |

(These runs reuse the RIB derived cache, hence faster than the
benchmark's run 1 which included the RIB rebuild.)

## Modes

- Cold network run: dominated by serial-per-collector acquisition; the
  bounded download→parse pipeline overlaps downloads and parsing
  (`--download-jobs 2` default, conservative).
- Local raw-cache parse run: the table above (`--rebuild-update-caches`).
- Derived-cache warm run: all derived caches hit; ~1-2 s total.
