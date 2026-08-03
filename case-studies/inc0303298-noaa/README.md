# NOAA I2 participant event — INC0303298 (2026-08-03)

Narrow-scope case study: one fresh IP-layer I2 participant event,
selected through the bounded viewer discovery and executed through the
durable job workflow.

## Reviewed relationship

- **Event:** INC0303298 — "Brief Outage - I2 Participant NOAA
  (KANS-WASH)" (GRNOC Public Task Viewer, 2026-08-03, immutable
  snapshot; the source description is Sub5 boilerplate only).
- **Relationship type:** IP participant relationship (I2 Participant;
  parenthesized site code (KANS-WASH) → redundant-attachment
  expectation under the reviewed title convention).
- **Target:** NOAA, origin AS 270 (ARIN RDAP autnum AS270, registrant
  NASA-Z, registration 1989-02-24).
- **Reviewed plane:** Internet2 R&E AS 11537.
- **Applicability:** PotentiallyVisibleInPublicBgp (reviewed
  2026-08-03; ticket_reviews).

## Selection provenance

Bounded viewer discovery (`docs/audits/2026-08-fresh-event-discovery.md`,
8 requests, no throttling). Event-date preflight (route-views2
rib.20260803.0800): 3,350 origin-matching routes, 284 qualifying
observer-prefix streams across 142 distinct prefixes via AS11537.
Candidates INC0303260 (I2 PX peer Amazon; no direct AS11164 baseline in
the rrc11 bview — 0 of 157,959 origin routes) and INC0303264 (I2
Participant Cloudflare; no qualifying baseline under AS11537 — 0 of
40,780 — or AS11164 at route-views2) were blocked and not executed.

## Result (run 4, job 54102844457bbe058a441bef707a4ccc)

Observed result: **No route-state change observed** — across 284
selected observer-prefix streams at route-views2, no announcements,
withdrawals, path changes, or community changes during the event
analysis window (19 UPDATE archives, 3,622,810 parsed elements, 0
transitions; all 284 streams Unchanged).

Expectation assessment: **Consistent with the reviewed expectation
(redundant-attachment).** The selected redundant attachment held in
public BGP throughout the reviewed window.

Operational interpretation limit: this does not establish whether the
documented operator action occurred, whether other NOAA routes changed
outside the selected observer scope, or whether traffic was affected.

## Evidence

- `out/INC0303298/` — report.json (schema v3), lifecycle.json (284
  streams, category Unchanged), transitions.json (0), limitations.json,
  archive_manifest.json (1 RIB + 19 UPDATE archives), performance.json,
  execution_metadata.json (plan hash
  `7ea5d6678d6010c0f83d8fea27973f206c7d816a8d7955263983c956792e0070`).
- Reviewed manifest: `manifests/INC0303298.json`.
- Source record fixture: `tests/fixtures/grnoc/INC0303298.json`.
- Raw MRT archives are NOT tracked; they remain runtime cache material.

## Finding chronology audit

`finding-chronology-audit.json` (derived from lifecycle.json): 142
distinct prefixes across 2 route-views2 peers (137.164.16.84 and
163.253.3.14; 284 streams), 0 transitions total — the no-change
chronology is complete with no hidden mixed-path ambiguity.
