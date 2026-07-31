# inim — Internetwork Impact Monitor

inim is a reproducible, event-conditioned BGP observation system. It tests
operator-declared expectations against route behavior visible at selected
public collectors (RouteViews).

The central analytical unit is an **observer-prefix stream lifecycle
conditioned on a reviewed event manifest**: one event, one reviewed
observation plan, one frozen observer cohort, one reconstructed lifecycle
per observer-prefix stream, one evidence-scoped assessment. Implementation
complexity exists to preserve correctness, provenance, and
reproducibility.

It does not establish:

- global reachability
- traffic impact
- circuit state
- operator command usage
- causation from temporal association alone

## What inim produces

inim compares an operator-declared event expectation with externally
observable BGP route behavior at selected public collectors. Each
completed report answers: what the ticket implied, what the selected
observers showed, how the two compare, what the observation scope was,
and what the result does not prove.

### Case study: RIPE via NYIIX (INC0302574)

- redundant-attachment expectation
- 19 selected observer-prefix streams
- no route-state change observed
- consistent with expectation
- observer-scoped limitation: the negative finding does not prove
  physical redundancy

### Case study: UVA via Internet2 (INC0299001)

- participant-unavailability expectation
- 48 selected observer-prefix streams
- 13 temporarily absent and later returned
- heterogeneous changes among the remainder (22 prepend-only, 11
  material changes retaining the reviewed transit, 2 departing it)
- **Partial routing impact observed** (PartialImpact)
- the report distinguishes 214 route-instance transitions from the
  13 observer-prefix streams that became absent — a demonstration of
  ADD-PATH-aware stream analysis

A peer event without a reviewed network-path predicate (e.g. INC0301970)
is blocked before archive discovery rather than assigned a speculative
impact verdict.

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

See `docs/` for the full design, domain model, decisions, data provenance,
and observability contracts.

## License

inim is licensed under the MIT License. See LICENSE.

SPDX-License-Identifier: MIT
