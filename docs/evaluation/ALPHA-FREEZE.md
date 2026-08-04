# Alpha evaluation freeze policy

Normative policy for the external NOC alpha evaluation period.

The freeze begins when the session that created this document merges
(Session 51, branch `session-51-noc-alpha-evaluation`). It ends when
one of the exit conditions below is met and the project owner records
that decision in this file.

## Purpose

The project is entering a period of external evaluation by working
network engineers. During that period the product must remain stable:
an evaluator who inspects the deterministic demo must see the same
workbench semantics that the facilitator's answer key describes.
Uncontrolled feature expansion would invalidate evaluation results and
make it impossible to attribute observed confusion to the evaluated
artifact.

## Freeze layers

The freeze has four layers with different scopes:

### 1. Product freeze

No new broad product capabilities during the evaluation period.

Not accepted during the freeze, unless an exit condition has been met:

- another dashboard
- another event
- more aggregate metrics
- animation
- richer cards
- alternate color schemes
- speculative convenience
- broad refactoring
- performance optimization without measured evaluator impact
- additional source support (new RouteViews/RIS/GRNOC/PeeringDB/RIR
  capabilities)
- automated inference (automatic explanation generation, natural
  language querying, automatic scoring of evaluator prose)

The accepted compact event workbench remains the alpha baseline.
The incident-family workbench remains deferred; it is not restarted
during the freeze.

### 2. Semantic freeze

Existing evidence interpretation changes only for demonstrated
correctness defects.

"Demonstrated" means: a reproducible contradiction between the current
interpretation and the canonical tracked artifacts, or a reproducible
violation of the documented observer-scoped semantics. Aesthetic
preference, terminology taste, and speculative improvements are not
demonstrated correctness defects.

### 3. Evidence freeze

Canonical tracked artifacts remain immutable:

- immutable plans, jobs, runs, and evidence
- transitions, lifecycle, waves, report artifacts
- reviewed case-study claims and provenance

Evidence changes are not made to make evaluation material easier to
answer. The answer key must follow canonical evidence; any contradiction
between them is a P0 defect of the evaluation material, not a reason to
rewrite evidence.

### 4. Documentation maintenance

Factual and evaluator-blocking corrections remain allowed:

- factual corrections to documentation
- corrections that unblock an evaluator (setup, navigation, unclear
  error message)
- drift-guard corrections that keep CI honest

Documentation maintenance never changes evidence interpretation or
product scope.

## Change categories

### Accepted during the freeze

- evidence contradiction fixes
- semantic correctness defect fixes
- provenance defect fixes
- security defect fixes
- evaluator-blocking installation defect fixes
- evaluator-blocking navigation defect fixes
- accessibility defect fixes (correctness-impacting or core-task
  blocking)
- data-loss defect fixes
- deterministic-demo defect fixes

### Not accepted during the freeze

- aesthetic preference changes
- another dashboard
- another event
- more aggregate metrics
- animation
- richer cards
- alternate color schemes
- speculative convenience
- broad refactoring
- performance optimization without measured evaluator impact
- additional source support
- automated inference

## Required checks (maintained)

- `alpha_freeze_document_exists` — this document exists and is linked
  from `docs/README.md` and `CONTRIBUTING.md`
- `freeze_allows_correctness_fixes` — the Accepted categories above
  include correctness and security fixes
- `freeze_does_not_block_security_fixes` — security defects are always
  accepted
- `freeze_prohibits_feature_expansion_without_evidence` — the Not
  accepted categories above contain no evidence-based escape hatch
- `normative_docs_link_freeze_policy` — governance documents link this
  policy

## Exit conditions

The freeze ends when EITHER:

1. at least **three** completed external evaluation sessions are
   recorded in `docs/evaluation/PILOT-REGISTRY.md`; or
2. the project owner documents an earlier decision to end the pilot.

Three sessions are **not** statistically representative of anything.
Their purpose is directional product feedback, not usability
measurement. The exit condition exists to bound the freeze, not to
claim significance.

When the freeze ends, this document is updated to record: the date, the
exit condition met, and the decision-gate review (see
`docs/evaluation/POST-PILOT-DECISION-GATE.md`).

## How to request a freeze exception

1. Record the finding in the post-session decision template
   (`docs/evaluation/facilitator/POST-SESSION-DECISION.md`).
2. Classify it against this policy (which layer, which category).
3. For P0/P1 correctness, security, or evaluator-blocking defects:
   open an issue with reproduction evidence and fix it — no exception
   ceremony needed, the policy already allows it.
4. For anything that resembles feature expansion: record it, defer it,
   and do not implement it during the freeze.
