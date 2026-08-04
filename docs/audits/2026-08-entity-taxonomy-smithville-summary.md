# MAN LAN entity taxonomy and Smithville event-summary review — 2026-08-05

Internal project-owner walkthrough. This is **not** an external evaluation
session; the pilot registry remains at **zero external sessions**.

## Scope

Two evaluator-facing defects found during the internal walkthrough:

1. **MAN LAN entity taxonomy** — test/measurement equipment (Ixia) and
   entities whose attachment is not established (NEAAR, OMAN, WIX
   interconnect, TWAREN) are rendered as reviewed Layer-2 fabric
   attachments, implying they are attached networks or connectors.
2. **Smithville event summary** — the event page does not clearly
   explain the selected observer coverage, and it does not distinguish
   the imported source-snapshot fetch time from the reviewed analysis
   cutoff or show the cutoff's provenance.

## Observed facts (fresh read-only evaluator demo, 2026-08-05)

Reproduced from a clean clone at `bb894fc`, `inim demo init` + `inim
demo verify` (ok), then served read-only.

### MAN LAN case-study page (`/case-studies/manlan-2019`)

- The fabric SVG renders ten attachment nodes, each labeled
  "reviewed Layer-2 attachment (not BGP adjacency)":
  NORDUnet, ESnet, GÉANT, CANARIE, TWAREN, SINET, Ixia, NEAAR, OMAN,
  WIX interconnect.
- The attachment table (header "Attached network/connector") lists the
  same ten entries; Ixia/NEAAR/OMAN/WIX interconnect carry "no
  reviewed ASN".
- Ixia (a network test-equipment vendor, per the reviewed target
  research) is presented with the same node type as attached networks.

### Smithville event page (`/events/INC0301970`)

- Workflow line: "Provisional analysis completed — observed through
  2026-08-04T00:01:37Z"; Latest result "Insufficient qualifying
  visibility".
- Labels: "Status Complete" and "Lifecycle Open" (adjacent rows).
- Event window block: Start, "Source lifecycle: Open", "Analysis
  cutoff: 2026-08-04T00:01:37Z".
- Source snapshot history shows the imported snapshot fetched at
  **2026-07-31T00:00:00Z** (tracked offline fixture), with no statement
  of when the source lifecycle was verified relative to the cutoff.
- No observation-coverage summary: the page does not state how many
  collectors were checked, that AS11550-origin routes were visible, or
  why no UPDATE archives were acquired.

## Canonical evidence consulted (tracked, no new acquisition)

- `case-studies/manlan-2019/case-study.json` — reviewed interconnection
  context (10 attachments) and AAR-derived target roles.
- `case-studies/manlan-2019/target-research.json` — reviewed entity
  identities (2026-08-04 review): ASN labels and per-entity notes.
- `case-studies/manlan-2019/pilot/PILOT-SELECTION.md` — reviewed
  per-entity determination table (AAR-documented actions, origin
  mapping, pilot-suitability verdicts).
- `docs/audits/2026-08-smithville-source-refresh.md` — exact source
  refresh record (retrieval timestamp 2026-08-04T00:01:37Z, raw
  snapshot SHA-256, lifecycle In Progress / open, event-date baseline
  preflight table with per-collector counts).
- `manifests/INC0301970.json` — reviewed manifest: `analysis_end_utc`
  2026-08-04T00:01:37Z, `open: true`, analyst notes recording the exact
  source refresh and the preflight determination.
- `case-studies/indiana-gigapop-smithville-2026/INC0301970.source.json`
  — the immutable refreshed source snapshot (state In Progress, no
  end), committed 2026-08-04.

## Findings

1. Ixia is classified in the reviewed research as a network
   test-equipment vendor, not a network operator; it has no ASN and no
   PeeringDB network entry. Its current presentation as a Layer-2
   fabric attachment is incorrect.
2. NEAAR, OMAN, and WIX interconnect have no reviewed attachment
   evidence; TWAREN's attachment is explicitly flagged "less certain"
   in the reviewed pilot selection. None meet the reviewed
   "AttachedNetwork" bar (source mention alone is insufficient).
3. The demo imports the 2026-07-31 tracked fixture as the source
   snapshot, while the reviewed manifest's snapshot cutoff is the
   2026-08-04T00:01:37Z exact source refresh (tracked immutable
   snapshot `INC0301970.source.json`). The later reviewed snapshot is
   not imported into the demo.
4. The source lifecycle "Open" was verified at the 2026-08-04T00:01:37Z
   refresh (state In Progress, no published end), but the event page
   does not state that the lifecycle claim is anchored to that snapshot.
5. The analysis cutoff equals the reviewed snapshot cutoff (the exact
   source refresh retrieval) — it is not a fixture fetch time — and its
   provenance is recorded in the manifest analyst notes and the source
   refresh audit, but is not visible on the event page.
6. No canonical BGP evidence changed during this review; no analysis
   was rerun; no source was contacted.

## Corrections applied

See the session-55 change set: entity taxonomy in the reviewed
interconnection context (Ixia → test equipment; WIX → interconnect
context; NEAAR → service reference; OMAN and TWAREN → unresolved
mentions), fabric diagram restricted to reviewed attached networks,
corrected attachment count, Smithville observation-coverage summary,
snapshot/cutoff provenance presentation, and the demo import preferring
the latest reviewed immutable snapshot.
