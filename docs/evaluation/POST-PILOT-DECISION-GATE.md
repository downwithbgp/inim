# Post-pilot decision gate

Roadmap decision document for what happens after the external NOC
alpha evaluation period.

## When this gate opens

After **three** completed external evaluation sessions recorded in
`docs/evaluation/PILOT-REGISTRY.md`, or earlier by documented project
owner decision.

Three sessions are directional feedback, not a statistically
representative sample. The gate exists to force a recorded decision,
not to claim measurement validity.

## Review inputs

- repeated correctness errors (across evaluators)
- repeated terminology errors
- repeated navigation failures
- evidence trust (did evaluators trust the evidence references?)
- next-action usefulness (did evaluators know what to check next?)
- setup burden (clean-clone to first task)

Raw task outcomes come from the session notes; patterns are reviewed
after several sessions, not after one.

## Possible decisions

| Decision | Meaning |
|---|---|
| Continue evaluation unchanged | Keep the freeze; recruit more evaluators |
| Correct blocking semantics | Fix demonstrated semantic defects found during evaluation |
| Improve evaluator-blocking navigation | Fix navigation defects found during evaluation |
| Resume event acquisition | End the acquisition pause; add new analyzed events |
| Build incident-family workbench | Begin the deferred incident-family workbench |
| Prepare first tagged alpha release | Package and tag a first alpha release |
| Pause project | Stop active development |

## Rules

- Do not predetermine the decision.
- The incident-family workbench is **not** automatically next.
- A release is **not** automatically next.
- The decision follows observed use, recorded in session notes and the
  pilot registry.
- The decision and its rationale are recorded in this file when the
  gate closes.
