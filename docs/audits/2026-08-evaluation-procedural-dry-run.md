# Evaluation procedural dry run — 2026-08-04

**This was an internal procedural verification, not external user
research.** It is not recorded in `docs/evaluation/PILOT-REGISTRY.md`
as an external session, and no evaluator feedback was fabricated.

## Method

A clean clone at commit `041fdf8` was used (the post-merge `main`
state). The full evaluation flow was walked in order: invitation
draft → bootstrap → task booklet → response sheet → facilitator answer
key. The purpose was to verify that instructions are coherent, URLs
are correct, tasks are answerable, the answer key is usable, and the
timing is plausible — not to simulate an unfamiliar user.

## Flow verification

| Step | Result |
|---|---|
| Invitation draft readable and non-promotional | PASS — states public BGP only, observer-scoped evidence, no traffic measurement; avoids banned wording |
| Bootstrap from clean clone | PASS — 4 commands total (clone, bootstrap, server, browser); URLs printed |
| Demo init/verify in the clone | PASS — 4 events, 12 runs, pilot workbenches rendered |
| All scenario URLs | PASS — HTTP 200 for all 5 scenarios + plan page + event detail |
| Task booklet walkthrough (A1–H1) | PASS — every task has an evidence-supported answer on a visible page (see task-answerability audit) |
| Answer key usable by facilitator | PASS — values match the workbench pages and the canonical artifacts (spot-checked per scenario) |
| Timing plausible | PASS — core tasks on 2 pages (NORDUnet, UVA) take well under 15 minutes; deep-dive tasks are optional |

## Procedural findings (only procedural; no fabricated confusion)

1. **Fixed during the dry run** — answer-key NORDUnet restoration
   range now reflects the 11-prefix group (16:59:26Z–17:02:03Z),
   matching the workbench sentence; previously it spanned all 33
   streams (16:45:27Z–17:02:19Z) and would have confused a facilitator
   checking the key against the page.
2. **Fixed during the dry run** — legacy `NOC-ALPHA-EVALUATION.md`
   marked superseded so a facilitator does not hand out the old task
   list alongside the booklet.
3. **Observation** — Smithville E4 requires two navigation hops
   (workbench → Ticket interpretation → Analysis workflow/plan);
   recorded in the answerability audit as expected depth, kept as an
   observation for the debrief question "Which detail is hidden too
   deeply?"
4. **Observation** — the answer key contains no subjective evaluator
   feedback and no unsupported operational conclusions (verified by
   reading the generated output).

## Corrections made

- NORDUnet restoration range in the answer key (see 1).
- Legacy protocol superseded banner (see 2).

## Explicit statement

This dry run produced **no evaluator feedback**, **no usability
claims**, and **no pilot-registry entries**. Its only outputs are this
audit and the two corrections above.
