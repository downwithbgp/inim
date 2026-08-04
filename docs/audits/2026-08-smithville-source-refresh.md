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
