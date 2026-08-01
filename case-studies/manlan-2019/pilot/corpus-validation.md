# MAN LAN corpus state — validation (Session 33, Part 18)

Date: 2026-08-01. Catalog: `data/inim.sqlite` (schema v6). All counts
below are read from the catalog after the bounded pilot.

## AAR references and retrieval

- **12 ticket references from the AAR** (case-study event links).
- **10 public tickets retrieved** via the polite viewer adapter
  (all INC/CHG numbers; 2019 records remain public).
- **2 unresolved public tickets**: `TASK0038206`, `TASK0038211` — the
  viewer endpoints do not serve TASK-prefixed records (audited); they
  remain unresolved document references with no fabricated events.
- **10 case-study links resolved** by identifier; 2 remain unresolved.

## Explicit cross-ticket references

One explicit edge, from the fetched CHG0038258 public description
("…are being tracked in Internet2 ticket INC0040257"):

- CHG0038258 → INC0040257, `TracksRemainingImpactIn`,
  evidence `ExplicitTicketText`, snapshot provenance retained, resolved
  to the catalog event for INC0040257.

Derived candidates remain visibly distinct: 20 `DerivedTemporalOverlap`
edges (all pairs among the overlapping 2019-08-21 tickets) with the
neutral `TemporalOverlap` kind — never causal.

## Source-versus-AAR timing differences

Both timings are preserved; nothing is reconciled.

| Ticket | Source (planned) | Source (actual) | AAR phase |
|---|---|---|---|
| CHG0038258 | 2019-08-21T04:00–13:00Z | 2019-08-21T04:38:38–13:00Z | Scheduled migration 04:00–10:00Z |
| CHG0038386 | (fetched record) | (fetched record) | Rollback at 18:01Z |
| INC0040272 | (fetched record) | (fetched record) | Traffic-replication incident 14:14–18:01Z |

The AAR phase boundaries and the ticket source timings are distinct
values; the case study keeps its reviewed AAR phases while each ticket
keeps its independent source snapshot.

## Individual ticket roles (as retrieved)

- CHG0038258 — Maintenance 1 of 2 (primary change; tracks INC0040257)
- CHG0038386 — Emergency maintenance (rollback change)
- INC0040257 — Outage resolved, MAN LAN & IP various participants
- INC0040258 — I2 Optical MAN LAN to WIX interconnect outage
- INC0040272 — I2 Optical participant NORDUnet incident
- INC0040289 — I2 PX participant NORDUnet availability
- INC0040290 — MAN LAN participant Ixia outage
- INC0040291 — MAN LAN optical participant ESnet outage
- INC0040293 — I2 Optical participant ESnet outage
- INC0040318 — MAN LAN CPU alarm

## Incident-group evidence

- 1 `ExplicitlyLinked` group: CHG0038258 ↔ INC0040257 (explicit text).
- 1 `StrongCandidate` group: the 10 retrieved tickets via reviewed
  case-study membership (`SharedCaseStudy`).
- 10 `WeakCandidate` overlap groups (`DerivedTemporalOverlap` only).

## BGP analyzability state

All 10 retrieved tickets: `NotReviewed` (acquired, never reviewed — no
entity mapping, predicate, or window has been approved). The three
repository events keep their prior states (`AnalysisComplete` for the
two analyzed events, `NeedsTransitPredicate` for the blocked one).
Nothing was auto-analyzed; no ASN mapping or predicate was inferred.

## Linked NORDUnet pilot run

Run 3 (`PilotObservation`, Complete) remains linked to the case study —
the NORDUnet analysis stays associated with its exact AnalysisRun and
the reviewed case study, not causally attached to every related ticket.

## Batch plan

The stored case-study archive plan (Draft, RouteViews) yields one
CorrelationBatch: 10 events, 548 unique archives, 4932 archives avoided
through reuse, ~1.93 GB estimated, 548 expected parse operations,
deterministic, nothing downloaded.

## Boundaries honored

No synthetic event replaced the 12 tickets; each source ticket is
independently revisioned; no BGP verdict was generated from ticket text
alone; no unrestricted crawl, speculative mapping, or automatic causal
correlation occurred.
