# MAN LAN Core Node Hardware Upgrade (2019-08-21)

Reviewed case-study data for the operator-reported incident. The source of
record is the operator-authored After Action Report:

- **URL:** https://docs.globalnoc.iu.edu/uploads/c5/88/c5881bec35cb83807dd4b0a7ee32effe/MANLAN-20190821-Postmortem.pdf
- **SHA-256:** `d29df26a269962afeb4c671063ea64dec6103e226c039e5939d5af99eedd7114`
- **Pages:** 15 (per the review brief)
- **Redistribution status:** Unknown — the PDF must not enter the crate
  package until redistribution rights are explicitly established.

The local PDF was **not available** at import time. The document record is
metadata-only; attach the file later with:

```
inim catalog document import --db data/inim.sqlite \
    --file /path/to/MANLAN-20190821-Postmortem.pdf \
    --source-url https://docs.globalnoc.iu.edu/uploads/c5/88/c5881bec35cb83807dd4b0a7ee32effe/MANLAN-20190821-Postmortem.pdf
```

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

- **Document**: the AAR PDF is attached (SHA-256 verified, 15 pages,
  catalog-relative storage under `data/documents/d29df26a2699/`).
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
  complete MAN LAN incident verdict.
- **Full incident-wide BGP analysis**: **not executed**. No whole-incident
  public-BGP conclusion exists.

The AAR lists multiple contributors; this data file does not reproduce
contributor names in the primary UI.

## Visual review (Session 32)

`scripts/screenshot-review.sh` captures fixed-viewport screenshots of the
deterministic demo catalog (loopback only) to `tmp/ui-review/` (gitignored,
excluded from the package). Screenshots are for EXTERNAL computer-vision
review — visual quality is not self-certified here.
