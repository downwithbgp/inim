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

- Target research: **incomplete** (no historical mappings reviewed yet).
- Archive plan: produced by `inim catalog case-study plan` as a **Draft**
  (no archives downloaded).
- BGP analysis: **not executed**. No public-BGP conclusion is produced
  until historical target mappings and the archive plan are reviewed.

The AAR lists multiple contributors; this data file does not reproduce
contributor names in the primary UI.
