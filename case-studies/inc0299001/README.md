# INC0299001 — UVA participant-unavailability event (2026-07-14)

Reviewed case-study evidence for the UVA event. This directory contains
the canonical run artifacts (`out/INC0299001/`), the reviewed ASN
identities (`asn-identities.json`), the reviewed peer metadata
(`peer-metadata.json`), and the checked per-prefix chronology audit
(`finding-chronology-audit.json`). This README records the final checked
chronology; every claim below traces to those artifacts.

## Result

**Partial routing impact observed**: 13 of 48 selected observer-prefix
streams at route-views2 became **temporarily absent** and later
returned; among the remaining 35 streams, 22 showed prepend-only
changes, 11 had other material path changes while retaining the
reviewed transit, and 2 remained visible after departing that transit.
The run distinguishes 214 route-instance transitions across the 48
streams (ADD-PATH-aware). Assessment against the ticket expectation:
partially consistent with the participant-relationship-unavailable
expectation.

## Final checked chronology (route-views2, peer 163.253.3.14)

For the 11-prefix absence group:

- **Event baseline and pre-finding route are distinct.** The event
  baseline was `AS11537 AS40220 AS225×7`; before the withdrawal the
  route had already reduced to `AS225×1`.
- **Prepender reduction while visible**: at 07:24:47 the origin
  prepending reduced from `AS225×7` to `AS225×1` while the routes
  remained visible.
- **Withdrawal occurred later**: at 07:33:59.462Z the 11 prefixes were
  withdrawn.
- **11-prefix absence lasted 54 ms** at the selected observer:
  restoration at 07:33:59.516Z (withdrawal 07:33:59.462Z → return
  07:33:59.516Z).
- **The first returned path matched the event baseline** (`AS225×7`),
  not the pre-withdrawal route.
- **The final route matched the pre-withdrawal state, not the event
  baseline**: at 07:36:00 the route settled back to `AS225×1`, which
  equals the pre-withdrawal state; it is never labeled an exact
  event-baseline restoration.
- **The 12th prefix had a materially different chronology**:
  `137.54.122.0/23` returned on `AS225×1` (the pre-withdrawal state),
  not on the event baseline, and is grouped separately.

## Counts are distinct

| Unit | Count |
|---|---|
| Observer sessions (unique) | 4 |
| Observer episodes | 7 |
| Selected observer-prefix streams | 48 |
| Distinct prefixes (union across peers) | 12 |

Observer-session, episode/finding, stream, and distinct-prefix counts
measure different things and are never conflated.

## Evidence

- `out/INC0299001/lifecycle.json` — canonical per-stream lifecycle
  evidence (baseline path, transitions, withdrawal/restoration
  timestamps, cooldown transitions).
- `finding-chronology-audit.json` — the checked per-prefix chronology
  audit read from the lifecycle artifact (withdrawal timestamps,
  first returned path, source archive identities).
- `out/INC0299001/report.json` / `report.txt` — the generated report
  (schema v2).
- `asn-identities.json`, `peer-metadata.json` — reviewed ASN identities
  and observed peer metadata for the event-date sessions.
