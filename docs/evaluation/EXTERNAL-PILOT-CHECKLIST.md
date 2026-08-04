# External pilot checklist

Operational checklist for running an external NOC alpha evaluation
session. The facilitator follows this from preparation through
follow-up.

## Before the session

- [ ] Choose the repository commit SHA to evaluate (record it).
- [ ] Build the deterministic demo at that commit
      (`scripts/evaluator-bootstrap.sh --db <path> --port 8080`).
- [ ] Verify project scope (`inim project-scope audit --db <path> --root .`).
- [ ] Verify all scenario URLs return HTTP 200 (see the bootstrap
      output; the evaluation manifest lists the URLs).
- [ ] Print or copy the response sheet for the evaluator
      (`docs/evaluation/evaluator/NOC-ALPHA-RESPONSE-SHEET.md`).
- [ ] Prepare the facilitator answer key
      (`scripts/build-evaluation-answer-key.sh` output).
- [ ] Confirm the environment contains no confidential network data.
- [ ] Confirm the server is running in read-only mode on loopback.
- [ ] Have the task booklet and glossary available for the evaluator.

## During the session

- [ ] Record the start time.
- [ ] Avoid teaching answers; use the facilitator guide's probe
      questions when the evaluator is stuck.
- [ ] Record interventions (terminology clarification, navigation hint,
      task clarification).
- [ ] Record exact misleading wording the evaluator reports or shows.
- [ ] Avoid collecting private network details; remind the evaluator the
      response sheet is anonymized.

## After the session

- [ ] Stop the server.
- [ ] Save sanitized session notes.
- [ ] Classify findings against `docs/evaluation/FEEDBACK-TRIAGE.md`.
- [ ] Open public issues only after sanitization.
- [ ] Update `docs/evaluation/PILOT-REGISTRY.md`.
- [ ] Do not implement feedback immediately without triage; record it in
      the post-session decision template first.

## Rollback: P0 semantic error discovered during a session

If the evaluator discovers a P0 semantic error (wrong route fact,
wrong timestamp, wrong path, wrong observer eligibility, incorrect
evidence reference, data corruption, project-scope bypass, security
defect):

1. Stop using the affected scenario immediately.
2. Preserve the session notes verbatim.
3. Open a correctness issue with the reproduction evidence.
4. Do not generalize the affected result to other scenarios.
5. Follow `docs/evaluation/ALPHA-FREEZE.md` — correctness fixes are
   allowed during the freeze.
