# Monocle Evaluation for inim

**Date:** 2026-07-31 (finalized Session 17)
**Monocle version:** 1.4.0 (pinned `=1.4.0`, features: `["lib"]`)
**Decision:** **REJECTED** — removed from production dependency graph
**ADR:** See [ADR-001](ADRs/MONOCLE-DATAPLANE.md)

## Executive Summary

Monocle 1.4.0 was evaluated as a potential BGP data plane for inim.
After source-level audit, the following hard blockers were identified:

1. **Raw MRT caching is CLI-only** — `--use-cache`/`--cache-dir` implementation
   lives in `src/bin/commands/search.rs` and is not exposed through the `lib`
   feature. inim would need to retain its own archive caching.
2. **Broker query caching is CLI-only** — `broker-cache.sqlite3` is managed by
   the CLI binary, not the library API.
3. **Dependency cost is significant** — duplicate bgpkit-parser (0.18 vs 0.19),
   duplicate bgpkit-broker (0.10.1 vs 0.12), triple reqwest versions.
4. **No parity proof** — without network-backed runs demonstrating identical
   observations, replacing a working pipeline is unjustified.

The decision is to **remove the Monocle dependency** and retain inim's existing
BGP data plane. The evaluation document and ADR are preserved for future
reference should a later Monocle release expose library caching.

## Dependency Impact

| Metric | Before | After |
|--------|--------|-------|
| bgpkit-parser | 0.19.0 (pinned) | 0.19.0 (inim) + 0.18.0 (monocle) |
| bgpkit-broker | 0.12.0 | 0.12.0 (inim) + 0.10.1 (monocle) |
| reqwest | 0.12 | 0.12 + 0.11 + 0.13 (triple) |
| Clean build time | ~13s release | +0s (incremental); +~90s (full) |
| Transitive deps added | — | monocle lib: oneio, ipnet, chrono-humanize, dateparser, humantime, bgpkit-commons, itertools, radar-rs, rayon, regex, tabled, json_to_table |
| Duplicate dep count | — | bgpkit-parser (2), bgpkit-broker (2), reqwest (3), oneio (3) |

**Note on parser identity:** Monocle 1.4.0 pulls bgpkit-parser 0.18.0. inim pins 0.19.0. Derived-cache keys include `PARSER_VERSION = "0.19.0"`. If Monocle becomes the parsing backend, cache keys must include a `backend_id` (e.g. "monocle-0.18.0") or the monocle path must use its own cache namespace. Current decision: cache keys unchanged; monocle observations are produced via the monocle adapter and fed into the existing pipeline. No existing caches are invalidated.

## Architecture Ownership Table

| Capability | Ownership | Notes |
|-----------|-----------|-------|
| Broker discovery | Monocle owns | monocle::SearchLens wraps bgpkit-broker with caching |
| Archive selection | Monocle owns | SearchLens accepts time range + collector filters |
| Raw MRT download/cache | Monocle owns (CLI) | `--use-cache` flag; library-mode caching TBD (Part 4) |
| Broker query cache | Monocle owns | Oneio-backed HTTP caching |
| Archive checksums | inim owns | Required for derived-cache keys and evidence provenance |
| Parallel parsing | Monocle owns | Rayon-based concurrency in SearchLens |
| Progress reporting | Monocle owns | ParseProgress callback |
| Parser filters | Monocle owns | Prefix/origin/peer/community filters |
| Exact-timestamp RIB reconstruction | Monocle owns | `monocle rib` reconstructs state at arbitrary timestamps |
| ADD-PATH handling | Shared temporarily | Both parsers flatten segments |
| Normalized observation production | inim owns | RouteObservation + provenance is inim's domain model |
| Derived event cache | inim owns | Per-archive admitted observations + admission counters |
| Route-state reconstruction | inim owns | Evidence-bearing transition timeline |
| Evidence provenance | inim owns | archive_sha256, element_seq, observation_id |
| Event lifecycle | inim owns | Per-stream lifecycle classification |
| AS/prefix enrichment | Monocle owns | as2rel, pfx2as, ASInfo database |
| RPKI | Monocle owns | RPKI validation via bgpkit-commons |
| Event assessment | inim owns | Expectation/verdict/evidence/waves |
| Reporting | inim owns | report.txt/json, evidence appendix, lifecycle.json |

## Likely Boundary

```
Monocle:                           inim:
  broker discovery ──┐
  archive selection   │
  raw MRT download    │               reviewed manifest
  parallel parsing    │                   │
  prefix filtering    │                   ▼
  progress reporting  │            target admission
  RIB reconstruction  │            derived caches
  AS/prefix/RPKI      │            event reconstruction
         │            │            lifecycle analysis
         ▼            │            waves + SEQUITUR
    RouteObservation ──┘            assessment
    (inim domain type)              reporting
```

## Cache Hierarchy

```
Monocle raw MRT + Broker cache
    ↓
inim event-specific derived cache (RIBs + UPDATEs)
    ↓
lifecycle / waves / SEQUITUR / assessment
```

Monocle does NOT provide persistent storage of admitted observations or admission counters → inim derived caches remain.

## Provenance Gaps

- **File URL in callback:** Monocle SearchLens callbacks may not expose source file URL or per-file element sequence. If unavailable, inim will track these at the adapter level using the `archive_order` and `source_url` from the task list (already tracked in inim's UPDATE task collection).
- **Deterministic file ordering:** Monocle's Rayon concurrency may process files in non-deterministic order. inim's coordinator assigns `archive_order` before dispatching and sorts results deterministically post-parse.

## Deferred Session 15 Work

- Manifest TransitPredicate support
- UVA mechanism-neutral two-layer report
- Comparison regeneration
- GRNOC architecture documentation
- Smithville unresolved-predicate validation

These remain queued behind the Monocle data-plane decision.
