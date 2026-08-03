# Fresh-event candidate review — 2026-08

Audit date: 2026-08-02. This report shortlists fresh validation events
from ALREADY AVAILABLE catalog material only: existing catalog events,
immutable snapshots, reviewed relationships, and current analysis
metadata. No broad crawl, no numeric ticket enumeration, and no new
seeds were used.

## Candidate pool (existing catalog)

The local catalog (`data/inim.sqlite`, same material as the demo
catalog) contains 17 catalog events:

| Event | Source | Reviewed mapping | Plan status |
|---|---|---|---|
| INC0299001 (UVA) | local-repository | reviewed | Ready (excluded) |
| INC0302574 (I2PX audit) | local-repository | reviewed | Ready (excluded) |
| INC0301970 (Smithville via Indiana GigaPOP) | local-repository | origin ASN 11550 reviewed; transit predicate **Unresolved** | Blocked |
| MANLAN-2019-NORDUNET-PILOT* (14 pilot variants) | local-repository | reviewed pilot mappings | Ready (excluded) |

No GRNOC Public Task Viewer events are present in the catalog, and
there are zero reviewed `ticket_relationships` rows. The candidate pool
is therefore exactly the four rows above.

## Shortlist

Per the candidate criteria, the shortlist is **one candidate**:

### Candidate 1 — INC0301970

| Field | Value |
|---|---|
| Event ID | INC0301970 |
| Source network | Internet2 (Smithville via Indiana GigaPOP) |
| Title | Participant availability via Indiana GigaPOP |
| Task type | incident |
| Lifecycle | Open (no published end) |
| Time window | start 2026-07-28T04:35:00Z; end unavailable |
| Reviewed role | participant-relationship expectation |
| Candidate target | Smithville |
| Candidate origin ASN | 11550 |
| Origin evidence | reviewed manifest `manifests/INC0301970.json` |
| Plane / predicate | **none — transit predicate Unresolved** |
| Predicate evidence | none (no reviewed provenance) |
| Source families | RouteViews |
| Collectors | route-views2, route-views6 |
| Archive estimate | not computed (no execution) |
| Expected semantic novelty | open-event provisional result (if executed) |
| Blockers | transit predicate not reviewed; open event without an explicit analysis cutoff |
| Recommendation | **not ready; do not execute** |

The other three pool rows are excluded by the session's explicit
exclusion list (INC0302574, INC0299001, the NORDUnet MAN LAN pilot).

## Selection decision

**No event meets readiness.**

Exact blockers:

1. INC0301970 — the only non-excluded candidate — has an **Unresolved
   transit predicate** (plan status `Blocked`,
   `MissingReviewedTransitPredicate`). It is also an **open event
   without a reviewed analysis cutoff**.
2. No other event in the catalog carries a reviewed target mapping with
   a reviewed plane or transit predicate outside the excluded set.
3. There is no GRNOC corpus material in the catalog to shortlist from,
   and no reviewed relationships to extend.

Per the session rules: no mapping was guessed, no readiness was
weakened, and the fresh-event run is left **unexecuted**. The queued-job
workflow was validated instead with the synthetic offline fixture
(`tests/queued_analysis_e2e_test.rs`), which exercises the identical
queue → worker → publish path.

## What would make a future candidate ready

- A reviewed transit predicate / named-plane mapping for INC0301970
  with documented provenance, and a reviewed analysis cutoff for its
  open window; or
- a bounded sync of already-discovered GRNOC relationships (explicit
  known event IDs only) followed by reviewed target mappings — never
  guessed.
