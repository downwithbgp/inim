# NOC alpha evaluation — facilitator guide

How to run an external NOC alpha evaluation session. Read the task
booklet and the generated answer key before the session; do not show
either to the evaluator.

## Purpose

The evaluation answers one question per scenario: can a working network
engineer determine, from the demo workbench alone, what relationship was
reviewed, what observer evidence qualified, what route changes occurred,
which prefixes were involved, how routes ended, what was not observable,
and what inim cannot conclude?

## Evaluator prerequisites

- network engineer / NOC engineer / routing engineer / network
  operations staff
- comfortable with: prefix, AS number, AS path, BGP peer, route
  withdrawal
- no requirement: inim internals, BGPKIT, MRT encoding, RouteKey,
  ObserverPrefixKey, SQLite schema, Rust, development history

## Session length

- 3 minutes: introduction and setup
- 15–20 minutes: core tasks
- 5 minutes: evidence deep dive (optional)
- 5 minutes: debrief

Total approximately 20–30 minutes. Do not impose strict time pressure;
task durations are observations, not a contest.

## Environment preparation

- Use the commit SHA chosen for the session (see
  `docs/evaluation/EXTERNAL-PILOT-CHECKLIST.md`).
- Run `scripts/evaluator-bootstrap.sh --db <path> --port 8080`
  (read-only, loopback).
- Verify the demo (`inim demo verify`), project scope
  (`inim project-scope audit`), and scenario URLs.
- Prepare the printed response sheet and the facilitator answer key.
- Confirm no confidential network environment is visible on the screen.

## Task-selection guidance

- Core tasks: A1–A3, B1–B5, C1–C6, D1–D5, E1–E6, G1–G2.
- Optional deep-dive: B6, C7, F1–F3.
- Do not require every evaluator to complete every deep evidence task.
- If the session runs long, drop optional tasks first, then F1–F3,
  then G tasks — never drop the four core scenarios.

## When to remain silent

- During tasks, do not teach the interface.
- Do not point to the exact disclosure control.
- Do not tell the evaluator which observer matters.
- Do not correct an answer immediately unless continuing would become
  impossible.
- Do not defend the project.
- Do not debate preferences during the task.
- Do not explain implementation details unless asked after the task.

## When to clarify terminology

- If the evaluator asks what a term means, give a neutral definition
  (the glossary text is safe) and record the intervention as
  "terminology clarification".
- Do not connect the term to the specific answer.

## When an evaluator is stuck

Ask:

- "What evidence would you look for?"
- "Which part of the page appears relevant?"

Do not say:

- "Click Route sequence"
- "Look at the observer episodes table"

## How to record hesitation

- Record the task, the moment, and the term or control involved.
- Record the time cost if it is notable.
- Do not interpret hesitation as preference.

## How to record incorrect conclusions

- Record the evaluator's exact statement where practical (label:
  Quote).
- Record what the evidence actually shows (label: Observation).
- Record your interpretation separately (label: Inference).
- Do not merge the three.

## How to avoid teaching the interface

- Before the session, agree with yourself which interventions are
  allowed: terminology clarification, navigation hint (pointing to a
  page, not a control), task clarification (restating the question).
- Every intervention is recorded on the response sheet.

## Debrief prompts

- Which result did you trust least?
- Which term was ambiguous?
- Which page appeared more certain than its evidence?
- What did you expect to find but could not?
- What would you check next internally?
- Which detail was unnecessary for the decision?
- Did you ever confuse collector, observer peer, and target?
- Did you ever confuse event baseline and pre-finding state?

Do not ask leading praise questions ("Wasn't that clear?"). Do not ask
broad satisfaction questions as the primary instrument; they may be
optional closing questions only.

## Common interpretation traps (per scenario)

See the generated answer key's per-scenario sections for: likely
confusion, evidence needed, and unsupported stronger conclusion. Do not
probe for these explicitly; they are for recognizing errors when they
occur.

## After the session

1. Complete the session-notes template
   (`docs/evaluation/facilitator/SESSION-NOTES-TEMPLATE.md`).
2. Classify findings (`docs/evaluation/FEEDBACK-TRIAGE.md`).
3. Record decisions (`docs/evaluation/facilitator/POST-SESSION-DECISION.md`).
4. Update the pilot registry
   (`docs/evaluation/PILOT-REGISTRY.md`).
5. Do not implement feedback immediately without triage.

## Layer-2 fabric terminology (2026-08)

When an evaluator works the NORDUnet scenario, the facilitator holds
these reviewed truths and may clarify them without leading answers:

- MAN LAN is **Layer-2 fabric context**: it has no ASN for the case
  study, does not speak BGP, does not originate routes, and does not
  appear as an AS-path hop.
- **NORDUnet AS2603** is the analyzed BGP target — one attached
  network. The completed pilot is NORDUnet-target-scoped public-BGP
  analysis during the operator-reported Layer-2 incident; it is not
  MAN LAN BGP analysis and not a complete analysis of all connectors.
- Observed AS paths in the diagrams are **public-collector evidence**
  (what one observer received), never switch-fabric state.
- **Layer-2 attachment and AS-path adjacency are different evidence
  classes**: attachment does not prove BGP adjacency, route export, a
  commercial relationship, traffic flow, or active state.
