# RIPE RIS collector selection — NORDUnet pilot (Session 34, Part 5)

**Date:** 2026-08-01 (Session 34)
**Target:** NORDUnet, origin AS2603, transit predicate `ContainsAny[11537]`
**Window:** 2019-08-21 16:00–17:30 UTC (warmup 840 min → baseline 02:00 UTC;
cooldown 60 min)
**Method:** metadata probe (archive existence + coverage) then reviewed RIB
preflight per candidate: `bview.20190821.0000.gz` (the 8-hour-grid baseline
at/before warmup) parsed through the shared pipeline with the reviewed
origin/predicate filters.

Raw preflight numbers: `ris-collector-preflight.json` (this directory).

## Candidate collectors (historically available, 2019-08-21)

All candidates below had a `bview.20190821.0000.gz` and full UPDATE
coverage (`updates.20190821.0130.gz` … `updates.20190821.1830.gz`, both
HTTP 200). `rrc22` is excluded on metadata: its 2019-08 bview is a 3.9 KB
stub with no usable baseline RIB.

| collector | bview size | AS2603 origin routes | AS11537-in-path routes | frozen streams | verdict |
|-----------|-----------:|---------------------:|-----------------------:|---------------:|---------|
| rrc00 | 439 MB | 619 | 11 | 11 | **selected** |
| rrc01 | 222 MB | 279 | 0 | 0 | rejected |
| rrc03 | 199 MB | 254 | 0 | 0 | rejected |
| rrc04 | 28 MB | 75 | 0 | 0 | rejected |
| rrc05 | 38 MB | 66 | 0 | 0 | rejected |
| rrc06 | 19 MB | 39 | 12 | 12 | **selected** |
| rrc07 | 45 MB | 90 | 0 | 0 | rejected |
| rrc10 | 131 MB | 193 | 0 | 0 | rejected |
| rrc11 | 52 MB | 106 | 0 | 0 | rejected |
| rrc12 | 209 MB | 239 | 0 | 0 | rejected |
| rrc13 | 66 MB | 150 | 0 | 0 | rejected |
| rrc14 | 52 MB | 104 | 0 | 0 | rejected |
| rrc15 | 154 MB | 247 | 24 | 24 | **selected** |
| rrc16 | 30 MB | 64 | 0 | 0 | rejected |
| rrc20 | 211 MB | 289 | 0 | 0 | rejected |
| rrc21 | 136 MB | 229 | 0 | 0 | rejected |
| rrc22 | 3.9 KB | — | — | — | rejected (metadata: stub) |
| rrc23 | 25 MB | 52 | 0 | 0 | rejected |
| rrc24 | 13 MB | 125 | 0 | 0 | rejected |

## Selected collectors and rationale

1. **rrc00** (Amsterdam, RIPE NCC) — the largest RIPE RIS route
   collector by peer count; 619 AS2603-origin routes, 11 with AS11537 in
   path. European hub with maximal peer diversity; archive volume
   ~764 MB (bview 439 MB + ~204 update files ≈ 324 MB).
2. **rrc06** (Otemachi, Tokyo, Japan; DIX-IE/JPIX) — Asian vantage; 39
   AS2603-origin routes, 12 with AS11537 in path. Small archive volume
   (~29 MB total), complete coverage. Geographic diversity against rrc00.
   (Location corrected: rrc06 is the Tokyo collector, not a US collector;
   see collector-locations.json, as-of 2019-09-05.)
3. **rrc15** (São Paulo, Brazil) — South American vantage; 247
   AS2603-origin routes, 24 with AS11537 in path — the highest
   qualifying stream count. Archive volume ~461 MB.

Selection criteria applied: qualifying visibility (nonzero AS11537-in-path
streams), peer diversity (rrc00 largest), geographic diversity (Europe /
Asia / South America), manageable archive volume, complete
archive coverage (verified 01:30–18:30 UTC updates).

## Rejected collectors and reasons

- **rrc01, rrc03, rrc04, rrc05, rrc07, rrc10, rrc11, rrc12, rrc13,
  rrc14, rrc16, rrc20, rrc21, rrc23, rrc24** — preflight found **zero**
  AS2603-origin routes with AS11537 in path at the pre-window baseline
  (`bview.20190821.0000.gz`). Without baseline visibility for the
  reviewed predicate, an observer-prefix stream cannot be frozen, so
  these collectors cannot produce evidence for the reviewed target.
  Not selected solely because they exist.
- **rrc22** — metadata: the 2019-08 `bview.20190821.0000.gz` is a
  3.9 KB stub (no usable baseline RIB).

## Caveats

- Baseline visibility is measured at one bview (00:00 UTC). A rejected
  collector could have gained AS11537-in-path visibility later in the
  day; the selection is for the reviewed pre-window baseline only.
- AS11537 (Internet2) peers with a limited RIS footprint; the three
  selected collectors are the ones with qualifying routes in this
  baseline.
