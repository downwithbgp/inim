# MAN LAN Core Node Hardware Upgrade (2019-08-21)

Reviewed case-study data for the operator-reported incident. The source of
record is the operator-authored After Action Report:

- **URL:** https://docs.globalnoc.iu.edu/uploads/c5/88/c5881bec35cb83807dd4b0a7ee32effe/MANLAN-20190821-Postmortem.pdf
- **SHA-256:** `d29df26a269962afeb4c671063ea64dec6103e226c039e5939d5af99eedd7114`
- **Pages:** 15 (per the review brief)
- **Redistribution status:** Unknown — the PDF must not enter the crate
  package until redistribution rights are explicitly established.

The local PDF was **not available** at import time; the document record was
created metadata-only, and the file was attached in a later local import
(SHA-256 verified, 15 pages, catalog-relative storage under
`data/documents/d29df26a2699/`). The PDF itself is **not in this
repository** and must not enter the crate package until redistribution
rights are explicitly established.

## What this directory contains

`case-study.json` — the single canonical reviewed data file (schema v1):

- 5 reviewed phases with `exact`/`summarized` boundary precision
  (retrospective belief is never rendered as measured fact)
- 12 related ticket references (CHG/INC/TASK records listed by the AAR);
  none are independently retrieved — relationships were assigned
  conservatively from AAR context and are marked as document references
- 11 reviewed claims with qualifications, source sections, and explicit
  observability classifications (5 potentially visible, 3 indirectly
  visible, 3 not directly visible, 0 unknown)
- 10 candidate analysis targets, **all `Unresearched`** — no ASN guesses;
  current organization ASN metadata is not 2019 truth

## Status (honest by design)

- **Document**: the AAR PDF record is attached to the local catalog
  (SHA-256 verified, 15 pages, catalog-relative storage under
  `data/documents/d29df26a2699/`); the PDF itself is not in this
  repository. The source tickets and the AAR provide the **operator
  evidence**; public BGP evidence is separate and observer-specific.
- **Target research**: reviewed (2026-08-01) in `target-research.json` —
  6 HistoricallyReviewed (NORDUnet AS2603, ESnet AS293, GÉANT AS21320,
  CANARIE AS6509, TWAREN AS7539, SINET AS2907 — AS9264 positively
  excluded), 2 NotApplicableToPublicBgp (Ixia, WIX), 1
  AmbiguousServiceIdentity (NEAAR), 1 Unresolved (OMAN). No ASN is
  guessed; every reviewed mapping has dated sources.
- **Path predicate**: ContainsAny[11537] (Internet2 transit presence) is a
  candidate validated empirically by the 2019-08-21 RIB during Stage A —
  it is kept separate from origin mappings.
- **Archive plan**: Draft, corrected to the reconstruction contract — one
  baseline RIB + one validation RIB + the 5-minute UPDATE sequence
  (272 files/collector, 02:00 → 00:35 next day, proven by first/last
  records); ~1.8 GiB compressed total (was mis-reported as 3.3 GiB with 12
  interval RIBs).
- **Pilot (Stage B, Complete)**: NORDUnet (AS2603) at route-views2,
  16:00–17:30 UTC — 11/33 selected streams absent 16:45:25Z for 2 s, 30
  path replacements, full baseline return by 17:02:19Z. See
  `pilot/PILOT-SELECTION.md` and `pilot/pilot-result.json`. The pilot is a
  **single-target, single-collector, bounded-window** result — it is NOT a
  complete MAN LAN incident verdict. The **operator incident horizon**
  (the AAR's full timeline, 15:33–20:48) and the **pilot horizon**
  (16:00–17:30 UTC) are distinct and never conflated.
- **Full incident-wide BGP analysis**: **not executed**. No whole-incident
  public-BGP conclusion exists.

- **RIS collector selection**: metadata + RIB preflight of
  18 historically available collectors; only rrc00, rrc06, rrc15 had
  AS2603-origin routes with AS11537 in path at the pre-window baseline.
  Rejected collectors recorded with reasons in
  `pilot/ris-collector-selection.md`.
- **RIS pilots (Complete)**: three independent runs, same
  reviewed target and window as the RouteViews pilot — rrc00 (11/11
  streams unchanged), rrc06 (12/12 departed AS11537 transit via path
  replacement 16:45:44Z, returned to baseline by 17:02:38Z, no absence),
  rrc15 (13/24 departed transit 16:35:38–16:45:20Z; the in-window
  exact-baseline return (17:03:32Z) and the cooldown re-change
  (17:52:16Z, no restoration before the 18:30 analysis end) are both
  retained; 11/24 unchanged). Each run keeps its own evidence; no merged
  verdict. Per-collector records: `pilot/ris-pilot-rrc00.json`,
  `pilot/ris-pilot-rrc06.json`, `pilot/ris-pilot-rrc15.json`.
- **Observation classes stay distinct**: route-views2 is a **direct
  R&E** observation (peer AS11537), the RIS collectors observe
  AS11537-in-path routes **indirectly**, and the direct **I2PX** plane
  was **unavailable** at the selected observers (no qualifying
  baseline). A direct R&E observation is never relabeled as an I2PX
  observation.
- **RRC11 historical audit**: the 2019-08-21 RRC11 baseline had **no
  direct AS11164 session** in the reviewed 2019 peer table (39 sessions,
  zero with peer ASN 11164), so the direct peering-plane pilot was
  blocked (`blocked-no-direct-session`); see `pilot/rrc11-audit-2019.md`
  and `pilot/rrc11-pex-pilot-decision.md`.
- **No mechanism conclusion**: the pilot makes **no traffic-loss and no
  Layer-2 mechanism conclusion**; BGP evidence alone cannot establish
  either.
- **Reviewed interpretations**: `pilot/ticket-reviews.json` (ten
  tickets, reviewed roles + AAR-cited provenance, 13 reviewed edges).
- **Cross-observer comparison**: the RouteViews pilot and the three RIS
  pilots are compared per prefix per collector without merging evidence
  (`observer_compare`); multiple observer agreement is not global proof.
- **Full incident-wide BGP analysis**: **not executed**. No whole-incident
  public-BGP conclusion exists.

The AAR lists multiple contributors; this data file does not reproduce
contributor names in the primary UI.

## Visual review

`scripts/screenshot-review.sh` captures fixed-viewport screenshots of the
deterministic demo catalog (loopback only) to `tmp/ui-review/` (gitignored,
excluded from the package). Screenshots are for EXTERNAL computer-vision
review — visual quality is not self-certified here.

## Corpus

`corpus/` holds the ten reviewed MAN LAN public tickets as immutable
snapshots (manifest + relationships; redistribution documented in
`docs/sources/GRNOC_PUBLIC_TASK_VIEWER.md`). The demo imports them as
discovered events with reviewed roles; they never create Ready plans
or jobs automatically. INC0040293 has its own narrow analysis case
study (`case-studies/manlan-esnet-2019/`).
