# Dated audits index

`docs/audits/` holds **dated audit records**: execution records of a
review performed at a specific repository state. They are historical
documents, not current normative specifications. A dated audit may
report facts and measurements true at its commit; current normative
semantics live in the documents linked from `docs/README.md`. Where an
audit's conclusion was later superseded, the index and the audit
header say so.

| Audit | Date | Subject | Historical commit | Current relevance | Superseded by |
|---|---|---|---|---|---|
| `2026-08-repository-truth-audit.md` (+ `repository-inventory.json`) | 2026-08 | repository truth audit (all tracked files) | `0517aac` | inventory source; render is drift-checked | — |
| `2026-08-clean-clone.md` | 2026-08 | clean-clone acceptance baseline | pre-`91ac498` | historical baseline | `2026-08-documentation-clean-clone.md` |
| `2026-08-documentation-clean-clone.md` | 2026-08 | clean-clone documentation verification | `91ac498` | current clean-clone record | — |
| `2026-08-evaluator-journey.md` | 2026-08 | evaluator journey review | pre-`91ac498` | evaluation-kit quality record | — |
| `2026-08-evaluation-task-answerability.md` | 2026-08 | task answerability review | pre-`91ac498` | evaluation-kit quality record | — |
| `2026-08-evaluation-accessibility.md` | 2026-08 | accessibility review (no WCAG claim) | pre-`91ac498` | scope of accessibility checks | — |
| `2026-08-evaluation-procedural-dry-run.md` | 2026-08 | internal procedural dry run | pre-`91ac498` | explicitly **not** an external session | — |
| `2026-08-evaluator-bootstrap.md` | 2026-08 | bootstrap verification | pre-`91ac498` | bootstrap contract | — |
| `2026-08-fresh-event-discovery.md` | 2026-08 | GRNOC discovery probes | pre-`91ac498` | source behavior record | — |
| `2026-08-fresh-event-candidates.md` | 2026-08 | fresh event candidates | pre-`91ac498` | candidate record | — |
| `2026-08-non-noaa-ip-event-candidates.md` | 2026-08 | candidate shortlist (allowlisted) | pre-`91ac498` | why excluded records are absent | — |
| `2026-08-grnoc-catalog-reconciliation.md` | 2026-08 | corpus reconciliation | pre-`91ac498` | corpus behavior record | — |
| `2026-08-manlan-ticket-readiness.md` | 2026-08 | MAN LAN ticket readiness | pre-`91ac498` | reviewed roles record | — |
| `2026-08-project-scope-noaa-removal.md` | 2026-08 | exclusion decision (allowlisted) | pre-`91ac498` | exclusion provenance | — |
| `2026-08-second-network-neutrality.md` | 2026-08 | source-neutrality audit | pre-`91ac498` | neutrality contract | — |
| `2026-08-smithville-source-refresh.md` | 2026-08 | Smithville refresh evidence | pre-`91ac498` | identity + cutoff provenance | — |
| `2026-08-incident-family-deferral.md` | 2026-08 | deferral decision | pre-`91ac498` | deferral rationale | — |
| `external-links-2026-08.md` | 2026-08 | external link status record | pre-`91ac498` | nonblocking; not CI-failing | — |
| `2026-08-internal-evaluator-findings.md` | 2026-08 | internal evaluator walkthrough findings | `94f8aad` | motivating audit for the workbench corrections | — |
| `2026-08-documentation-inventory.md` | 2026-08 | documentation-surface inventory | `91ac498` | checked lists (drift-guarded) | — |
| `2026-08-specification-coverage.md` | 2026-08 | specification coverage matrix | `91ac498` | navigation aid; not normative | — |
| `2026-08-documentation-spec-conformance.md` | 2026-08 | final documentation conformance audit | `91ac498` | this session's audit | — |
| `2026-08-wirthian-design-recovery.md` | 2026-08 | as-built computational-model recovery (reconstruction/falsification/synthesis) | `92f83d8` | current normative model in `docs/computational-model.md` + `docs/design/` | — |

## Rules

- Dated audits are **never rewritten** to look current. Correct them
  only for factual typos; record follow-ups in the index or a new
  audit.
- Do not update old measured values to current values.
- The rendered repository-truth audit is regenerated from
  `repository-inventory.json` by `scripts/build-repo-audit.py`;
  `scripts/audit-docs.sh` verifies it is up to date.
