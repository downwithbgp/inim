# Smithville source refresh and relationship review — 2026-08-04

Dated execution audit for the Session 50 Indiana GigaPOP / Smithville
peer-event analysis. No session narrative in normative docs.

## Exact source refresh (Part 2)

- External ID: **INC0301970** (exact source-record lookup only; no ID
  enumeration, no broad sync).
- Requests: **1** (HTTP 200); throttle responses: 0; rate: 0.42 req/s
  (ceiling 5.0); budget 5.
- Source URL: `https://ticket-viewer.grnoc.iu.edu/tickets/INC0301970/`
- Retrieval timestamp: 2026-08-04T00:01:37Z
- Raw snapshot SHA-256: `d911687c634a5efa7eafbea5816c4aa376f61c7c8fc14bd3611042873696de77`
  (previous reviewed fixture SHA-256:
  `70a14fa805fdc283da2f4ead80affb6f8c44f05fb16fb9153ffdb484eae5950b` —
  content changed: the live description is empty; the older fixture
  carried a hand-written description sentence; title, start, opened,
  state are consistent).
- Refreshed fields (authoritative): title **"Outage - Indiana GigaPOP
  Peer Smithville"**; category Undetermined; state code 2 = In Progress;
  priority 3; opened 2026-07-28T04:56:38Z; work_start
  2026-07-28T04:35:26Z; **work_end empty — the event remains OPEN**.

## Lifecycle (Part 3)

- Lifecycle: **Open** (source state In Progress; no end).
- No end time is invented.
- Analysis end: **explicit snapshot cutoff = 2026-08-04T00:01:37Z**
  (the refresh retrieval timestamp, reviewed).
- Any executed analysis is **Provisional** and states "observed through
  cutoff"; a later source refresh creates a new snapshot, a new plan
  revision, a new job, and a new run — the provisional run is never
  mutated.

## Relationship interpretation (Part 4)

- Named managed network: **Indiana GigaPOP** (GRNOC-operated).
- Relationship class: **Peer**.
- Named counterparty: **Smithville**.
- Attachment qualifier: **none** in the title.
- Reviewed expectation (generic title convention, Indiana GigaPOP
  profile): **the complete named Indiana GigaPOP peer relationship was
  reported unavailable**; externally visible route availability may
  change during the event.
- Explicit non-conclusions: this does NOT establish Smithville was
  globally single-homed, that Smithville had no other upstreams, that
  all Smithville services were unreachable, that traffic was
  interrupted, or that the event affected the global Internet.

## Route-selection question (Part 7)

The analysis question is NOT "were AS11550-origin routes visible
somewhere". It is: **were AS11550-origin routes selected through the
reviewed Indiana GigaPOP–Smithville peer relationship, and did their
selected route states change during the operator-reported event?**

The reviewed representation is chosen from event-date MRT evidence (the
observed path adjacency for AS11550-origin routes) corroborated by RIR
registration; OriginOnly is not used as a fallback.

## Event-date baseline preflight (2026-07-28) — decisive evidence

Baseline RIBs/bviews (event start 2026-07-28T04:35:26Z; latest
pre-event archives): route-views2 rib.20260728.0400.bz2, route-views6
rib.20260728.0400.bz2, rrc00/rrc06/rrc11 bview.20260728.0000.gz (all
acquired through the cache layer; integrity sidecars written). No
UPDATE archives were acquired.

| Collector | Family | Announces parsed | AS11550 routes | Distinct prefixes | AS19782 in any path | Direct AS19782 sessions |
|---|---|---|---|---|---|---|
| route-views2 | RouteViews | 18,226,403 | 221 | 13 (IPv4) | 2,452 | 0 |
| rrc00 | RIPE RIS | 55,255,901 | 546 | 13 (IPv4) | 6,704 | 0 |
| rrc06 | RIPE RIS | 6,700,798 | 65 | 13 (IPv4) | 839 | 0 |
| rrc11 | RIPE RIS | — | 91 | 13 (IPv4) | — | 0 |
| route-views6 | RouteViews (v6) | 4,854,128 | 0 | 0 | — | 0 |

All AS11550 paths traverse commercial transit (AS174 Cogent, AS1299
Telia, AS19151 BroadbandONE — the latter confirmed by ARIN RDAP as a
commercial hosting provider, NOT the managed network). Smithville's
observed multi-transit set positively corroborates the
no-global-single-homing caveat.

**Determination (deliverable B):** the named Indiana GigaPOP–Smithville
peer relationship is NOT assessable through the selected public
collectors at the event date:

- Classification per collector: TargetPresentRelationshipAbsent (target
  routes present; the reviewed relationship absent from their paths)
  and RequiredSessionAbsent (no direct AS19782 observer session exists
  at any selected collector).
- No AS11550 path traverses AS19782 (0 of 923 AS11550 routes across
  the IPv4 collectors).
- IPv6: no AS11550 origin visibility (0 of 4.85M announces on the
  IPv6-only collector) — the IPv6 family has no qualifying baseline.
- No plan can be Ready on this evidence; no UPDATE acquisition is
  justified; no mapping, predicate, or end time was guessed.

## Same-network bounded fallback search (Part 27)

Indiana GigaPOP domain surface (2 searches, HTTP 200, no throttle):
8 records. IP-layer peer events: INC0301970 (Smithville — primary,
blocked above) and INC0285525 "Outage - Indiana GigaPOP Peer Akamai"
(open, work_start 2026-05-13T16:51:58Z, no end — the window to any
cutoff is unbounded, far above the 72-archive execution limit).
Remaining records are DDoS mitigations and alarms (excluded classes).
No same-network fallback is independently Ready; no event was
executed.

## Final session outcome

- Primary deliverable: **B** — precise, evidence-backed determination
  that the named relationship is not assessable through the selected
  public collectors.
- No event executed; no UPDATE archives acquired; no blocked Internet2
  candidate revisited; project-scope policy unchanged.
