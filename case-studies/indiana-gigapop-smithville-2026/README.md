# Indiana GigaPOP peer relationship: Smithville — INC0301970 (2026-07-28)

Narrow-scope case study: the named Indiana GigaPOP–Smithville peer
relationship, the first second-network (non-Internet2) analysis. The
event remains OPEN; the result is provisional and the case study is
labeled provisional.

## Named relationship

- **Managed network:** Indiana GigaPOP (GRNOC-operated).
- **Relationship type:** Peer.
- **Counterparty:** Smithville.
- **Attachment qualifier:** none — under the shared GRNOC title
  convention, the COMPLETE NAMED Indiana GigaPOP peer relationship was
  reported unavailable.
- **Ticket horizon:** work_start 2026-07-28T04:35:26Z; source state In
  Progress; no published end (open event).
- **Source snapshot:** fetched at 2026-08-04T00:01:37Z (the exact
  source refresh retrieval, reviewed); source lifecycle at that
  snapshot: Open (state In Progress, no end). The tracked immutable
  snapshot `INC0301970.source.json` is the refreshed snapshot; the
  older tracked offline fixture is not the reviewed snapshot.
- **Analysis horizon:** the reviewed event window from
  2026-07-28T04:35:00Z (reviewed window start; the source work_start is
  04:35:26Z) through the reviewed snapshot cutoff 2026-08-04T00:01:37Z
  (the exact source refresh retrieval, reviewed). Result is
  Provisional. The analysis cutoff is the reviewed snapshot cutoff —
  the reviewed end of the provisional analysis window, not a fixture
  fetch time; its provenance is recorded in
  `INC0301970.source.json.meta.json` and
  `docs/audits/2026-08-smithville-source-refresh.md`.

## Reviewed identities

- **Smithville / AS11550:** Smithville Digital LLC (ARIN RDAP autnum
  AS11550, registration 1998-09-30; PeeringDB Smithville Digital, LLC).
  HistoricallyReviewed candidate confirmed 2026-08-04.
- **Indiana GigaPOP / AS19782:** INDIANAGIGAPOP (ARIN RDAP autnum
  AS19782, registration 2001-02-14, c/o Indiana University; routing POC
  "I-Light and Indiana GigaPOP"; PeeringDB Indiana GigaPOP).
  HistoricallyReviewed managed-network identity confirmed 2026-08-04.
- **Route-selection representation:** the named peer relationship =
  adjacency between AS19782 and AS11550 (the narrowest representation
  supported by the reviewed identities and the ticket; the predicate
  model supports `Adjacent`).

## Observer scope and result

Selected public collectors (event-date baselines 2026-07-28):
route-views2 rib.20260728.0200 (plus rib.20260728.0400 in the research
probe), route-views6 rib.20260728.0200, rrc00/rrc06/rrc11
bview.20260728.0000.

- AS11550 announces the same 13 IPv4 prefixes via Cogent (AS174),
  Telia (AS1299), and BroadbandONE (AS19151) transit ONLY — the
  observed multi-transit set corroborates the no-global-single-homing
  caveat.
- **ZERO** AS11550 paths traverse AS19782; **ZERO** direct AS19782
  observer sessions exist at any selected collector; **ZERO** AS11550
  origin visibility on the IPv6-only collector.
- **Result: Insufficient qualifying visibility** — the named
  Indiana GigaPOP–Smithville peer relationship is NOT assessable
  through the selected public collectors at the event date
  (TargetPresentRelationshipAbsent + RequiredSessionAbsent).
- The run (durable job, report artifacts in `out/INC0301970/`)
  published WITHOUT UPDATE acquisition (the zero-baseline stop is by
  design).

## Explicit non-conclusions

This analysis does NOT establish: that Smithville was globally
single-homed; that Smithville had no other upstreams; that all
Smithville services were unreachable; that traffic was interrupted;
that the event affected the global Internet; or that the peer
relationship did not change in ways invisible to the selected public
collectors.

## Evidence

- `INC0301970.source.json` — immutable refreshed source snapshot
  (raw viewer record, SHA-256
  `d911687c634a5efa7eafbea5816c4aa376f61c7c8fc14bd3611042873696de77`).
- `out/INC0301970/` — report.json/report.txt, limitations.json,
  archive_manifest.json (baseline RIBs only), execution_metadata.json,
  semantic_waves.json, transitions.json (empty — no qualifying
  streams).
- `docs/audits/2026-08-smithville-source-refresh.md` — refresh,
  lifecycle, identity, and event-date preflight evidence.
- `docs/audits/2026-08-second-network-neutrality.md` — production
  source-neutrality audit.
- Raw MRT archives are NOT tracked (runtime cache material).
