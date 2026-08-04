# Evaluation accessibility audit — 2026-08-04

Focused accessibility review of the evaluator journey (scenario
workbenches, event detail, analysis plan, bootstrap output). The
review covered the checks below; findings are recorded with severity.
Only correctness-impacting or core-task-blocking defects were fixed;
aesthetic redesign was explicitly out of scope.

## Checks

| Check | Result | Notes |
|---|---|---|
| Page titles | PASS | `inim workbench — <subject>` on every workbench; descriptive per-page titles (`Analysis run`, `Analysis plan`, event detail) |
| Heading hierarchy | PASS | h1 (site) → h2 (page) → h3 (sections) → h4/h5 (subsections); no skipped levels found |
| Landmark structure | PASS | `<header>` + `<nav>` + `<main>` in `base.html`; content lives inside `<main>` |
| Link text | PASS | links are descriptive ("Ticket interpretation", "Analysis details", "run detail"); no bare "here"/"click" |
| Table headers | PASS | every data table has `<thead><th>`; episode rows carry `data-label` for mobile rendering |
| Details summaries | PASS | findings, prefix drill-downs, route sequence, evidence, identity notes all use native `<details>/<summary>`; keyboard operable without JS |
| Focus visibility | PASS | `a:focus-visible, button:focus-visible, summary:focus-visible { outline: 2px solid }` in the stylesheet; row focus uses `:focus-within` |
| Keyboard navigation | PASS | manual tab-through of the NORDUnet workbench: nav links, finding links, summaries, and copy buttons all reachable; copy buttons are progressive enhancement (exact data is in the page) |
| Status not conveyed by color alone | PASS | changed/unchanged rows carry text spans ("Temporarily absent", "AS path changed"); "unresolved" end states render a text tag, not just a color |
| Timestamps readable by screen reader | PASS | timestamps are plain text (`2026-07-14T07:33:59.462019920Z`), not images or canvas |
| AS path text order | PASS | named paths are ordered lists (`<ol class="wb-named-path">`) preserving left-to-right AS path order; exact numeric path follows as text |
| Prefix-table labels | PASS | columns labeled Prefix / Before path / After path / Final path / First change / Visibility restored / Exact baseline restored / Evidence |
| No hover-only content | PASS | no `title=`-only or `:hover`-only content found; all disclosures are `<details>` (click/keyboard accessible) |
| Target size | PASS | links and summaries exceed 24×24 px; copy buttons are padded |
| Auto-refresh | PASS | `meta refresh` exists only on the job detail page and only for **active** jobs (`auto_refresh = job.state.is_active()`); completed workbenches never auto-refresh |
| Mobile 390×844 | PASS | no horizontal overflow on any scenario page; first principal result visible above the fold; disclosure controls reachable (verified with Playwright) |
| Console errors | PASS | zero console errors on all scenario pages at desktop and mobile widths |
| External requests | PASS | zero external network requests from demo pages |

## Findings

1. **Low — server stop instruction.** The bootstrap prints the server
   command but not how to stop it (Ctrl-C). Not a core-task blocker;
   recorded in the facilitator guide.
2. **Low — port-conflict message.** `Address already in use` does not
   mention `--port`. The bootstrap usage text covers the flag.
   Evaluator-blocking only if the facilitator does not pre-check.
3. **Low — identity-notes disclosure label.** "ASN identity notes"
   is internal vocabulary; the label is understandable in context and
   was kept (terminology audit records it as an observation, not a
   defect).

## Fixed during this session (correctness-impacting)

- **Provisional cutoff line** (Smithville): open-event workbenches now
  render the reviewed snapshot cutoff and the provisional qualifier at
  the top of the page and in the context facts — previously the
  "Open" lifecycle was the only open-event signal.
- **No-change wording for zero-eligible insufficiency**: the
  observation-coverage line no longer says "No route-state change at
  0 of 0" when no baseline observation existed.

## Deferred findings

- None beyond the two low-severity facilitator notes above.

## Method

Automated structural checks via Playwright (Chromium 1.61) at 1440×900
and 390×844 for all five scenario pages, plus manual keyboard
tab-through and source inspection of templates and the stylesheet. No
screen reader was available; screen-reader findings are limited to
structural review.
