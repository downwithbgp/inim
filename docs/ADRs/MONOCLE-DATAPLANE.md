# ADR-001: Reject Monocle as inim's BGP Data Plane

**Status:** Accepted
**Date:** 2026-07-31
**Session:** 17

## Context

Monocle 1.4.0 (BGPKIT) was evaluated as a potential replacement for inim's
BGP acquisition, parsing, search, and caching layers. Monocle provides broker
integration, parser filters, bounded parallelism, and enrichment (RPKI,
pfx2as, as2rel) in a single dependency.

## Decision

**Monocle is rejected** as inim's core BGP data plane. The dependency has been
removed from `Cargo.toml`. inim's existing pipeline (bgpkit-parser 0.19,
bgpkit-broker 0.12, custom archive caching, derived caches, event
reconstruction) remains the production path.

## Rationale

### Hard blockers

1. **Raw MRT caching is CLI-only.** Monocle's `--use-cache` and `--cache-dir`
   flags are implemented in `src/bin/commands/search.rs` and are not exposed
   through the `lib` feature. Library-mode users must implement their own
   archive download and caching. inim already has a working archive cache
   (`src/discover.rs`) — switching to Monocle would require retaining it
   anyway.

2. **Broker query caching is CLI-only.** The `broker-cache.sqlite3` database
   used for cached broker responses is managed by the CLI binary, not the
   library API.

3. **Dependency cost.** Monocle 1.4.0 pulls bgpkit-parser 0.18.0 (inim pins
   0.19.0), bgpkit-broker 0.10.1 (inim uses 0.12.0), and introduces a third
   reqwest version. This adds ~90s to a clean build and complicates the
   dependency graph without replacing any existing functionality.

4. **No parity proof.** Without network-backed runs demonstrating identical
   observations between the Monocle and inim parsing paths, replacing a
   working pipeline is unjustified.

### Provenance findings

`SearchElementBatch` (the `SearchSink` callback payload) does expose:
- `file_index: usize` — deterministic per-file ordering
- `file_url: String` — source archive identity
- `collector: String` — collector identity

This means provenance CAN be preserved if Monocle were adopted. The blocker
is caching, not provenance.

### ADD-PATH

BgpElem carries `path_id` when available. inim's current model does not have
a `path_id` field in `RouteKey` or `EvidenceRef`. This is a documented model
gap that would need resolution regardless of backend — not a Monocle-specific
blocker.

## Consequences

- inim's BGP data plane remains: bgpkit-parser 0.19, bgpkit-broker 0.12,
  custom archive cache (`src/discover.rs`), custom derived caches
  (`src/derived_cache.rs`), custom parallel parsing (`src/orchestrate.rs`).
- The `monocle` Cargo dependency has been removed.
- The `src/ingest/monocle.rs` adapter stub has been removed.
- `docs/MONOCLE_EVALUATION.md` is preserved for future reference.
- If a future Monocle release exposes library-mode raw MRT caching and
  broker-query caching, this ADR may be revisited.

## Alternatives considered

- **Adopt Monocle fully:** rejected — caching gap means inim still needs its
  own archive cache; dependency duplication not justified.
- **Monocle for enrichment only:** rejected — enrichment data (RPKI, pfx2as,
  as2rel) can be obtained via standalone `bgpkit-commons` without pulling the
  full Monocle dependency tree.
- **Defer decision:** rejected — leaving an unused production dependency and
  unproven adapter stub violates the project's "no dormant code" principle.

## Current status (2026-08-02)

Accepted and still fully applicable. The `monocle` dependency remains
removed; bgpkit-parser + bgpkit-broker plus the custom archive cache,
derived caches, and parallel pipeline remain the production data plane.

## Follow-up

Revisit only if a future Monocle release exposes library-mode raw MRT
caching and broker-query caching.
