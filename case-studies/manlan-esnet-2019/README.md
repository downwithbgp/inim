# MAN LAN ESnet participant event — INC0040293 (2019-08-21)

Narrow-scope case study: one ESnet participant event from the MAN LAN
corpus, analyzed as the Session 47 fresh-event validation.

## Scope

- **Event:** INC0040293 — "Outage Resolved - I2 Optical Participant
  ESnet" (GRNOC Public Task Viewer, immutable snapshot tracked under
  `case-studies/manlan-2019/corpus/snapshots/INC0040293.json`).
- **Reviewed window:** 2019-08-21 16:36:38Z – 20:25:24Z (the ticket's
  own work window), warmup 60 min, cooldown 60 min.
- **Target:** ESnet, origin AS 293 — historically reviewed (2019-dated
  PeeringDB capture + ARIN RDAP autnum ESNET, registration
  1997-06-16); see `case-studies/manlan-2019/target-research.json`.
- **Reviewed plane:** Internet2 R&E AS 11537 — empirically validated
  by the event-date preflight: the 2019-08-21 route-views2 RIB shows
  exactly one peer (64.57.28.241) announcing the three ESnet prefixes
  with the direct path `[11537, 293]` (3 qualifying observer-prefix
  streams). rrc06 and rrc15 see ESnet via other transit (target
  present, predicate absent).
- **Collector:** route-views2 only (the qualifying observer).
- **Manifest:** `manifests/INC0040293.json` (reviewed; provenance in
  the manifest's predicate statement).

## Result (run provenance: job ce258f4d8a07d7395d9d751f8d7512b2, run 4)

Verdict: **Unexpected continued reviewed-transit path** — across the 3
eligible observer-prefix streams, no announcements, withdrawals, path
changes, or community changes occurred during the event analysis
window. The reviewed-plane path `[11537, 293]` remained stable at the
single qualifying observer throughout the MAN LAN ESnet outage.

This is a **no-change case**: it documents the exact eligible observer
evidence and does not claim global impact. The AAR-documented ESnet
interface disable did not manifest as a public-BGP route change at
route-views2.

## Evidence

- `out/INC0040293/` — report.json (schema v2), lifecycle.json
  (3 streams, category Unchanged, baseline `[11537, 293]`),
  transitions.json (0 total), limitations.json (collector retention),
  archive_manifest.json (1 RIB + 80 UPDATE archives, ~281 MB, cache
  identity recorded), execution_metadata.json (plan hash
  `adcffe6f92434800a9a0ada6af0b116c831a9979745bddb8b42ba513863e7c58`).
- Raw MRT archives are NOT tracked; they remain runtime cache material.

## Finding chronology audit

No route transitions were derived; the chronology audit is therefore
empty of transitions. Eligible observer evidence: 3 streams
(route-views2 / 64.57.28.241 / 134.55.0.0/16, 192.107.175.0/24,
192.188.24.0/22), each with baseline `[11537, 293]`, 1 baseline
instance, 0 transitions, not withdrawn, not restored (nothing to
restore). ADD-PATH ambiguity: none.
