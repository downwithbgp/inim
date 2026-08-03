# MAN LAN ESnet participant event — INC0040293 (2019-08-21)

Narrow-scope case study: one ESnet participant event from the MAN LAN
corpus, with a preserved contemporaneous BGP observation.

## Reviewed relationship

- **Event:** INC0040293 — "Outage Resolved - I2 Optical Participant
  ESnet" (GRNOC Public Task Viewer, immutable snapshot tracked under
  `case-studies/manlan-2019/corpus/snapshots/INC0040293.json`).
- **Relationship type:** I2 **optical participant** relationship
  (2026-08-02 reviewed correction; see
  `case-studies/manlan-2019/pilot/ticket-reviews.json`). The ticket
  text attributes the outage to internal ESnet testing. Public BGP
  does **not** directly observe the optical interface.
- **Reviewed applicability:** `NotDirectlyObservableInPublicBgp`
  (ticket_reviews). INC0040293 is not an Internet2 IP participant or
  I2PX BGP relationship.

## Preserved supporting BGP observation (scope mismatch)

A contemporaneous AS293/AS11537 analysis was executed before the
relationship correction. The plan, job, run, and BGP artifacts are
immutable and preserved; their role is now:

**Contemporaneous supporting BGP observation with scope mismatch.**

- **Observed result:** no route-state change observed — route-views2
  continued to receive the same three selected ESnet-origin prefixes
  through Internet2 R&E AS11537 throughout the reviewed event window
  (3 streams, baseline `[11537, 293]`, 0 transitions across 80 UPDATE
  archives, 46,230,865 parsed elements; run 4).
- **This does not assess the optical participant interface named by
  the ticket. It does not establish that the interface remained
  available. It does not establish that other ESnet services or
  traffic were unaffected.**
- **Ticket-level result:** the named optical relationship is not
  directly assessable with public BGP.
- Checked audit: `assessment-audit.json` (derived from the canonical
  artifacts by `scripts/audit-esnet-assessment.py`).

## Scope

- **Reviewed window:** 2019-08-21 16:36:38Z – 20:25:24Z (the ticket's
  own work window), warmup 60 min, cooldown 60 min.
- **Target:** ESnet, origin AS 293 — historically reviewed (2019-dated
  PeeringDB capture + ARIN RDAP autnum ESNET, registration
  1997-06-16); see `case-studies/manlan-2019/target-research.json`.
- **Reviewed plane (supporting run):** Internet2 R&E AS 11537 —
  empirically validated by the event-date preflight: the 2019-08-21
  route-views2 RIB shows exactly one peer (64.57.28.241) announcing
  the three ESnet prefixes with the direct path `[11537, 293]` (3
  qualifying observer-prefix streams). rrc06 and rrc15 see ESnet via
  other transit (target present, predicate absent) and are **not**
  counted as unchanged observers.
- **Collector:** route-views2 only (the qualifying observer).
- **Manifest:** `manifests/INC0040293.json` (reviewed; immutable;
  the historical plan keeps its Reviewed status while the reviewed
  applicability record governs current presentation).

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

This chronology describes the supporting BGP observation only; it is
not an assessment of the optical relationship (see above).
