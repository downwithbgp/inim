# ADR: RIPE RIS observer support — audit result and scope

**Status:** Accepted (planning/discovery ready; execution supported)

**Date:** 2026-08-01 (Session 33, Part 10); execution status updated
2026-08-01 (Session 34, Part 4)

## Context

The corpus correlation planner must be able to consider both RouteViews
and RIPE RIS collectors as observer sources. Before extending support,
the existing BGP acquisition abstraction was audited:

1. **bgpkit-broker discovery** already supports both families:
   `project("routeviews")` and `project("riperis")` are both valid
   broker projects (verified in the crate source). Collector ids are
   family-scoped (`route-views2` vs `rrc00`).
2. **MRT parsing is source-neutral**: both families publish BGP4MP MRT
   archives (RIS `bview`/`updates` files use the same BGP4MP format as
   RouteViews `rib`/`updates` files). The existing parser consumes
   either; the committed `mrt/update-example.gz` fixture is format-
   compatible with RIS archives (upstream RIS files are additionally
   gzip-compressed, which the parser already handles).
3. **Report terminology is source-neutral**: the analyst-facing report
   renders observers as `collector:peer` strings from evidence records;
   it never hard-codes "RouteViews".
4. **The archive planner was RouteViews-specific**: URL builder
   (`archive.routeviews.org`), RIB cadence (2-hour `rib.*.bz2`), and
   update cadence were hard-coded.

## Decision

- Introduce a `SourceFamily` enum (`RouteViews`, `RipeRis`) as part of
  collector identity. A collector identifier is only meaningful
  together with its family.
- The archive planner becomes family-aware:
  - RouteViews: `http://archive.routeviews.org/...`, `rib.*.bz2` on a
    2-hour grid, `updates.*.bz2` every 5 minutes (unchanged).
  - RIPE RIS: `https://data.ris.ripe.net/{rrc}/...`, `bview.*.gz` on an
    8-hour grid (00/08/16), `updates.*.gz` every 5 minutes.
- `CollectorPlan` carries `source_family`; the plan's observer scope
  reports it. Stored plans predating this change default to RouteViews
  on read.
- **Execution (download + parse) of RIS archives is supported** (Session
  34, Part 4): the reviewed manifest carries `source_family` (default
  `RouteViews`, so pre-existing manifests parse unchanged); the
  orchestrator discovers through the family's broker project
  (`routeviews`/`riperis`); derived RIB/UPDATE caches are keyed on
  (family, collector) so identities can never collide; reports name the
  family (`RIPE RIS` vs `RouteViews`); mixed-source archive ordering is
  deterministic (total order on (ts_start, url)). A real 2019 RIS
  update fixture (`tests/fixtures/ris/`) exercises the shared
  ingestion path. RIS planning was already `Ready`; execution is no
  longer `Unsupported`.

## Consequences

- Mixed-family plans are deterministic and distinct: `rrc00` can never
  be confused with `route-views2`.
- Corpus archive batching (CorrelationBatch) groups by family +
  collector + URL, so a raw archive is downloaded once per unique URL
  regardless of how many event cohorts need it.
- No report or verdict change: RIS observers are rendered under their
  own collector names; the report never labels a RIS observer as
  RouteViews.
- No live RIS analysis was executed in Session 33 (per scope). Session 34
  executed the reviewed NORDUnet pilot against selected RIS collectors
  (see `case-studies/manlan-2019/pilot/`).
