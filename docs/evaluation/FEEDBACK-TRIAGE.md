# Evaluation feedback triage

How feedback from external NOC alpha evaluation sessions is classified,
prioritized, and acted on. Applies to issue-form submissions and to
facilitator-recorded session findings.

## Categories

| Category | Meaning |
|---|---|
| Correctness | Wrong route fact, wrong timestamp, wrong path, wrong observer eligibility, incorrect evidence reference, data corruption, project-scope bypass, security defect |
| Evidence/provenance | Evidence link broken, provenance unclear, artifact reference misleading |
| Scope overstatement | Result language implies traffic impact, root cause, global reachability, or total target connectivity that the evidence does not support |
| Terminology | A term is unclear, ambiguous, or misleading to the evaluator |
| Navigation | A control, link, or page is hard to find or confusing to reach |
| Accessibility | Keyboard, screen-reader, focus, contrast, or target-size defect |
| Installation/bootstrap | Setup, build, or demo initialization failure |
| Performance | Slow page, slow workflow, measurable delay |
| Feature request | Something the evaluator wanted that does not exist |
| Aesthetic preference | Styling, color, animation, layout taste |

## Priority rules

### P0 — act immediately

- wrong route fact
- wrong timestamp
- wrong path
- wrong observer eligibility
- incorrect evidence reference
- data corruption
- project-scope bypass
- security defect

### P1 — fix before the next evaluation session

- evaluator cannot complete a core task
- misleading result language
- missing critical limitation
- inaccessible control
- evaluator cannot start demo

### P2 — group across evaluators before implementing

- repeated hesitation
- unnecessary navigation
- confusing secondary terminology
- slow but usable workflow

### P3 — record, do not act on immediately

- aesthetic preference
- speculative feature request
- optional convenience

## During the alpha freeze

- P0 and P1 may trigger implementation changes (see
  `docs/evaluation/ALPHA-FREEZE.md` — these are the accepted change
  categories).
- P2 should be grouped across evaluators before implementation.
- P3 is recorded but not acted on immediately.

## Priority is not confidence

Do not use evaluator confidence alone as a priority. A high-confidence
wrong conclusion is more serious than low confidence in a correct
answer. Record confidence as a session observation; classify the
finding by its category and its impact on the task outcome.

## Issue labels

Labels used for evaluation feedback (created 2026-08-04):

- `alpha-evaluation`
- `correctness`
- `terminology`
- `evidence`
- `accessibility`
- `evaluator-blocking`

No automatic labeling is performed.

## Milestone

A GitHub milestone "Public alpha evaluation" exists for grouping real
feedback (created 2026-08-04, no completion date). It is repository
administration, not a product dependency; the session does not fail if
it is unavailable elsewhere.

## Workflow

1. Record the finding (session notes or issue form).
2. Classify: category + priority.
3. If P0/P1: open an issue with reproduction evidence (sanitized of
   private data), or fix directly when the evidence is reproducible and
   canonical truth is clear.
4. If P2/P3: record in session notes; group after several sessions.
5. Update `docs/evaluation/PILOT-REGISTRY.md` with issue links.
