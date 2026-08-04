# Post-session decision template

Used by the facilitator after each evaluation session to record findings
before any implementation decision. Do not implement feedback directly
during the evaluation session.

## Per-finding record

| Field | Value |
|---|---|
| Issue | one-line description |
| Evidence from session | what was observed (Observation / Quote / Inference) |
| Affected task | task ID or scenario ID |
| Severity category | Correctness / Evidence / Scope overstatement / Terminology / Navigation / Accessibility / Installation / Performance / Feature request / Aesthetic preference |
| Reproducible without evaluator | yes / no |
| Semantic correctness | yes / no / unknown |
| Evaluator-blocking | yes / no |
| Proposed action | what change would address it |
| Freeze exception required | yes / no (see `docs/evaluation/ALPHA-FREEZE.md`) |
| Implementation deferred | yes / no |

## Immediate P0 handling

Correct a P0 immediately **only when**:

- the evidence is reproducible, and
- canonical truth is clear.

Otherwise:

1. Open an issue with the reproduction evidence (sanitized).
2. Preserve the session notes.
3. Investigate separately.

## Rules

- Do not implement feedback during the evaluation session itself.
- Do not use evaluator confidence alone as priority
  (see `docs/evaluation/FEEDBACK-TRIAGE.md`).
- P2/P3 findings are grouped across evaluators before implementation.
