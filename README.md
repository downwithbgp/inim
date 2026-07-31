# inim — Internetwork Impact Monitor

inim compares operator-declared network events with externally observable
BGP route behavior from selected public collectors (RouteViews).

It does not establish:

- global reachability
- traffic impact
- circuit state
- operator command usage
- causation from temporal association alone

## Status

- Reviewed, canonical manifests drive every real analysis.
- Blocked planning is a **plan** status, never an `AnalysisOutcome`.
- ADD-PATH-aware identity: route state is keyed by
  `RouteKey` (collector, peer IP, prefix, path_id); stream lifecycles are
  keyed by `ObserverPrefixKey` (collector, peer IP, prefix).
- All persisted formats carry schema versions; old identity semantics are
  rejected, not silently reinterpreted.

## Build / test

```sh
cargo test
cargo fmt --check
cargo clippy --all-targets -- -D warnings
```

## CLI

```
inim plan    --event <ticket.json> --manifest <manifest.json> [--out <dir>]
inim analyze --event <ticket.json> --manifest <manifest.json> [--cache <dir>] [--out <dir>] [--jobs N] [--no-derived-cache|--rebuild-derived-cache]
inim migrate-manifest --input <legacy.json> --output <canonical.json> [--statement ... --reviewed-by ... --date ...]
inim compare --a <event-out-dir> --b <event-out-dir> --out <comparison-dir>
```

### Process exit status contract

| Code | Name                    | Meaning                                                        |
|------|-------------------------|----------------------------------------------------------------|
| 0    | `EXIT_SUCCESS`          | Plan produced (even Blocked) / analysis completed              |
| 1    | `EXIT_INVALID_INPUT`    | Malformed ticket or manifest; internal planning failure        |
| 2    | `EXIT_ANALYSIS_INCOMPLETE` | Infrastructure failure during analysis                      |
| 3    | `EXIT_ANALYSIS_BLOCKED` | Plan produced but Blocked; no Broker or MRT work was performed |

These are **process** exit codes, documented constants in `main.rs` — they
are never encoded in domain enums. `AnalysisPlanStatus::Blocked` lives in
the library/domain; `AnalysisOutcome` only ever carries completed,
insufficient-visibility, or incomplete results.

## Workflow

1. **Manifest review** — a canonical manifest (schema v2) carries the
   reviewed `TransitPredicateMapping` (status, predicate, provenance).
   Legacy single-ASN shortcut fields (`managed_network_asn`,
   `internet2_asn`) are rejected with `LegacyManifestRequiresMigration`;
   use `inim migrate-manifest` offline to convert (never automatic, never
   invents unresolved ASNs).
2. **Planning precedes acquisition** — `inim plan` (and `analyze` before
   any work) parses ticket + manifest and produces an `AnalysisPlan`.
   Blocked plans (e.g. `MissingReviewedTransitPredicate`) perform **zero**
   Broker calls and **zero** MRT parses.
3. **Acquisition** — broker discovery, archive caching, derived-cache
   lookup, MRT parsing (skipped on valid cache hits).
4. **Analysis** — RIB preflight freezes the observer-prefix cohort; UPDATE
   admission; route reconstruction; tokenization; lifecycle classification
   by `ObserverPrefixKey`; semantic waves; assessment.
5. **Artifacts** — report.txt/json (observed event signature + observable
   mechanism hints + limitations), evidence appendix, archive manifest,
   lifecycle.json, semantic_waves.json, withdrawal_audit.json,
   limitations.json; optional comparison artifacts.

## Worked demonstrations

Two completed case studies are analyzed in this repository:

- **INC0302574** (RIPE via NYIIX, redundant-attachment expectation) —
  **No observable BGP impact**: no route-state changes were observed among
  the 19 selected RouteViews observer-prefix streams (19 baseline route
  instances), consistent with the redundant-attachment expectation.
- **INC0299001** (UVA via Internet2, participant-relationship expectation)
  — **Partial impact**: among 48 selected observer-prefix streams,
  22 prepend-only, 11 material changes still via transit, 2 departed the
  reviewed transit predicate, 13 withdrew from the selected streams
  (13 restored), 2 semantic waves; 214 route transitions.

Both are observer-scoped conclusions about selected public collectors —
see `out/` (current-schema artifacts) and the archived earlier analyses
under `out/archive/`.

See `docs/` for the full design, domain model, decisions, data provenance,
and observability contracts.

## License

inim is licensed under the MIT License. See LICENSE.

SPDX-License-Identifier: MIT
