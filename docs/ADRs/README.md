# Architectural Decision Records — Index

ADRs are historical records: they state what was decided, when, and why.
They are not rewritten to match the current design. Where a decision was
superseded or extended, the original text stays and a dated follow-up or
a later ADR records the change.

## Current ADR series (`docs/ADRs/`)

| Number | Title | Status | Current relevance | Superseded by |
|---|---|---|---|---|
| ADR-001 | Reject Monocle as inim's BGP data plane (`MONOCLE-DATAPLANE.md`) | Accepted | Fully applicable; bgpkit-parser + bgpkit-broker remain the data plane | — |
| ADR-002 | Local event catalog and first web interface (`LOCAL-CATALOG-AND-WEB.md`) | Accepted | Catalog + read-only web workbench are the primary interface | Extended by ADR-003, ADR-004 |
| ADR-003 | Multi-ticket incident case-study layer (`CASE-STUDY-LAYER.md`) | Accepted | Case-study layer, extended by reviewed-interpretation tables (V7) | — |
| ADR-004 | Durable local analysis jobs and worker boundary (`DURABLE-ANALYSIS-JOBS.md`) | Accepted | Queued reviewed plans, separate worker, atomic publication | — |
| — | RIPE RIS observer support (`RIPE-RIS-SUPPORT.md`) | Accepted | RIS planning and execution supported end-to-end | — |

## Earlier decision log (`docs/DECISIONS.md`)

The early project decisions live in `docs/DECISIONS.md` under their own
numbering (ADR-001 … ADR-010 plus a dated Session 25 block). That
numbering is separate from the current `docs/ADRs/` series and the two
must not be mixed.

| Number | Title | Status | Current relevance | Superseded by |
|---|---|---|---|---|
| ADR-001 | Use Rust (stable, edition 2021) | Accepted | Still current (`Cargo.toml`, edition 2021) | — |
| ADR-002 | No async code in initial version | Partially superseded (dated follow-up) | CLI analysis path stays synchronous; web layer is async | ADR-002 in `docs/ADRs/` (web) |
| ADR-003 | Module boundaries, not workspace crates | Accepted | Still current | — |
| ADR-004 | SEQUITUR as a standalone module | Accepted | Still current | — |
| ADR-005 | Parenthesized convention is Internet2-specific | Accepted | Still current | — |
| ADR-006 | chrono for timestamps | Accepted | Still current | — |
| ADR-007 | clap derive for CLI | Accepted | Still current | — |
| ADR-008 | bgpkit-parser for MRT/BGP decoding | Accepted | Still current | — |
| ADR-009 | BGPKIT Broker is the archive-discovery boundary | Accepted | Still current | — |
| ADR-010 | SEQUITUR design decisions | Accepted | Still current | — |

## Related historical records

| File | What it records |
|---|---|
| `docs/MONOCLE_EVALUATION.md` | Full Monocle evaluation behind ADR-001 (docs/ADRs) |
| `docs/session-10-baseline.md` | Dated baseline of early analysis output (historical snapshot) |
| `spec/` | Session specifications and task lists — planning history, not current documentation |
