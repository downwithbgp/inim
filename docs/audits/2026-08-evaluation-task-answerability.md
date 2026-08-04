# Evaluation task answerability audit — 2026-08-04

For every task in `docs/evaluation/evaluator/NOC-ALPHA-TASKS.md`,
verified against the deterministic demo at commit `041fdf8`:

- the answer exists in the demo,
- the answer can be found through visible UI,
- the answer does not require reading source code, SQL, or raw
  artifact files (except tasks that explicitly test evidence
  navigation),
- the task does not require hidden development knowledge,
- the task has one evidence-supported interpretation or explicitly
  permits multiple operational answers.

Facilitator audit: answer locations are named here. The evaluator
booklet never names controls.

## Section A — Basic orientation (NORDUnet workbench)

| Task | Starting page | Answer location | Disclosure depth | Authoritative artifact | Ambiguity | Remediation |
|---|---|---|---|---|---|---|
| A1 | `/case-studies/manlan-2019/workbench` | page title + scope line | 0 | `pilot-result.json` | none | — |
| A2 | same | principal finding card + route-sequence + ASN identity notes | 0–1 | `cross-observer-matrix.json` | none | — |
| A3 | same | collector site shown with peer IP/ASN on the same card | 0 | `cross-observer-matrix.json` | the card does not state "site ≠ peer location" explicitly; the distinction is the expected judgment | facilitator probe; answer-key non-conclusion |

## Section B — NORDUnet route change

| Task | Answer location | Disclosure depth | Authoritative artifact | Ambiguity | Remediation |
|---|---|---|---|---|---|
| B1 | principal finding card time `16:45:25 UTC` | 0 | RV2 lifecycle/`pilot-result.json` | none | — |
| B2 | same card: 11 prefixes, example prefixes, peer `64.57.28.241` | 0–1 (Prefixes disclosure) | `cross-observer-matrix.json` | none | — |
| B3 | route sequence: baseline → absent → first return path | 1 (Route sequence) | RV2 lifecycle | "first route after visibility returned" is a named step | — |
| B4 | route-sequence restoration column + finding sentence | 1 | RV2 lifecycle | exact-baseline restoration range is split across the sentence and the sequence | — |
| B5 | route sequence "Analysis end" row | 1 | RV2 lifecycle | none | — |
| B6 | observer comparison by region / routing findings (rrc00/rrc06/rrc15) | 1 | `cross-observer-matrix.json` | any of the three RIS observers is correct | task permits multiple answers |

## Section C — UVA chronology

| Task | Answer location | Disclosure depth | Authoritative artifact | Ambiguity | Remediation |
|---|---|---|---|---|---|
| C1 | principal finding card + route sequence "Event baseline" | 1 | `finding-chronology-audit.json` | paths render as named segments (`AS225×7`) | — |
| C2 | route sequence "Pre-withdrawal route" | 1 | same | none | — |
| C3 | "Earlier change" link on the card | 1 | same | none | — |
| C4 | card sentence + route sequence (07:33:59.462Z, 54 ms) | 0–1 | same | exact timestamps are in the route sequence | — |
| C5 | route sequence "First route after return" | 1 | same | none | — |
| C6 | card final-state sentence ("matches the pre-finding route, not the event baseline") | 0 | same | none | — |
| C7 | prefix drill-down `137.54.122.0/23` (121 s absence vs 54 ms) | 1–2 | same | the outlier's distinguishing feature is the absence duration, visible in the drill-down | task text already asks for the differing lifecycle |

## Section D — I2PX not-assessable

| Task | Answer location | Disclosure depth | Authoritative artifact | Ambiguity | Remediation |
|---|---|---|---|---|---|
| D1 | no-visibility primary block ("Named relationship") | 0 | `relationship-audit.json` | none | — |
| D2 | "Public-collector eligibility" list (rrc11/rrc14 sessions) | 0 | same | none | — |
| D3 | eligibility rows show `0` origin routes; assessment sentence states why | 0–1 | same | none | — |
| D4 | "Supporting R&E observation" block states it does not assess the named relationship | 1 (collapsed) | `report.json` | none | — |
| D5 | assessment sentence ("cannot be assessed") | 0 | same | none | — |

## Section E — Smithville second-network

| Task | Answer location | Disclosure depth | Authoritative artifact | Ambiguity | Remediation |
|---|---|---|---|---|---|
| E1 | page title (Indiana GigaPOP Peer Smithville); AS relationship on the analysis-plan page (Adjacent(19782, 11550)) | 1–2 | `manifests/INC0301970.json` | the ASN-level relationship requires the plan page | recorded as expected navigation depth; facilitator may probe |
| E2 | Context "Lifecycle: Open" + provisional line | 0–1 | manifest `open: true` | none | — |
| E3 | provisional line names the cutoff `2026-08-04T00:01:37Z` | 0 | manifest `analysis_end_utc` | none | — |
| E4 | analysis-plan page analyst notes (AS11550 visible via transit; 0 paths via AS19782; 0 direct sessions) | 2 (workbench → Ticket interpretation → Analysis workflow/plan) | manifest analyst notes | deep; the workbench itself does not show these facts | **product change made**: provisional cutoff + no-change wording fixed; the preflight facts remain on the plan page (recorded, not moved) |
| E5 | observed-result line "No qualifying baseline existed…" + report assessment | 0 | `report.json` | the "no change vs insufficient visibility" distinction is the expected judgment | **product change made**: zero-eligible insufficiency no longer renders as "no route-state change" |
| E6 | any of the non-conclusion statements (plan page "DETERMINATION", answer-key non-conclusions) | 1–2 | manifest | multiple correct answers | task permits multiple answers |

## Section F — Evidence navigation (optional)

| Task | Answer location | Disclosure depth | Authoritative artifact | Ambiguity | Remediation |
|---|---|---|---|---|---|
| F1 | prefix drill-down Evidence cell (`collector:peer prefix at ts (archive_sha256:…)`); analysis detail page lists artifacts with the same SHA-256 | 1–2 | lifecycle transitions + `archive_manifest.json` | the archive identity is a SHA-256; the analysis page maps it to the artifact path | **product change made**: imported runs now carry per-transition evidence references |
| F2 | Route sequence / prefix drill-down | 1 | lifecycle | none | — |
| F3 | Context → Ticket interpretation (source snapshot history) / Analysis details (manifest revisions) | 1 | snapshot + manifest revisions | none | — |

## Section G — Operational follow-up

Open answers by design; no single required answer. Verified that each
scenario provides enough route-change or insufficiency content for a
plausible internal follow-up (BGP session state, interface counters,
prefix-level checks for UVA; session inventory or private peering
evidence for Smithville).

## Section H — Optical scope (optional)

| Task | Answer location | Disclosure depth | Authoritative artifact | Ambiguity | Remediation |
|---|---|---|---|---|---|
| H1 | event detail page ("not directly assessable with public BGP"; "contemporaneous supporting observation with scope mismatch") | 1 (workbench → Ticket interpretation) | `report.json` + ticket-readiness audit | none | — |

## Summary

- Tasks reviewed: 33 (A1–A3, B1–B6, C1–C7, D1–D5, E1–E6, F1–F3,
  G1–G2, H1)
- Tasks changed: 0 (wording verified against the demo)
- Product changes required: 3 — provisional cutoff rendering
  (E3), zero-eligible insufficiency wording (E5), imported evidence
  references (F1). All are evidence/terminology correctness fixes
  permitted by the freeze.
- Tasks removed: 0
- Deepest required navigation: 2 hops (Smithville E4 via the plan
  page); recorded as an expected observation for the "hidden too
  deeply" debrief question, not as a blocker.
