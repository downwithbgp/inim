# Documentation inventory — 2026-08

Dated audit. Inventories every tracked documentation surface in the
repository at HEAD `91ac498` (2026-08-04, evaluation-kit merge). This is
an execution record, not a normative specification: current normative
definitions live in `docs/README.md` (authority model) and the documents
it links.

## Method

- Every tracked file was enumerated with `git ls-files` (442 files).
- A file is a **documentation surface** when it communicates behavior,
  meaning, or policy to a reader: Markdown documents, templates, help
  text, reviewed configuration, generated reports, workflow files,
  package metadata, and substantive Rust module/item documentation.
- Rust source files are listed here only as **module documentation
  surfaces** (module-level `//!` docs and public-item docs); the
  per-file inventory of all 442 tracked files remains
  `docs/audits/repository-inventory.json` (rendered by
  `scripts/build-repo-audit.py` into
  `docs/audits/2026-08-repository-truth-audit.md`).
- `spec/` (13 historical session specifications) is historical planning
  material, not current documentation; it is exempt from current-doc
  checks and listed in the historical section only.

## Authority classes

The repository authority model lives in `docs/README.md`. The classes
used by this inventory:

| Class | Meaning | Example |
|---|---|---|
| Canonical protocol evidence | Immutable MRT-derived artifacts; never hand-edited | `case-studies/*/out/*/lifecycle.json`, `transitions.json` |
| Canonical external-source snapshot | Immutable source ticket records as acquired | `case-studies/manlan-2019/corpus/snapshots/*.json` |
| Reviewed project policy | Reviewed owner/operator policy; versioned | `config/project-scope.toml`, `docs/evaluation/ALPHA-FREEZE.md` |
| Reviewed network profile | Reviewed ASN/plane/collector metadata | `case-studies/*/pilot/network-profile.json`, `target-research.json` |
| Reviewed case-study interpretation | Human-reviewed claims, reviews, pilot decisions | `case-studies/*/case-study.json`, `pilot-result.json`, `ticket-reviews.json` |
| Immutable plan or run artifact | One executed or queued plan/run; immutable | `analysis_plan.json`, published run dirs |
| Generated current report | Reproducible output from canonical artifacts | `report.json`/`report.txt`, `answer-key.json`, `demo-manifest.json` |
| Normative product specification | Describes current implementation contracts | `docs/DESIGN.md`, `docs/DOMAIN.md`, `docs/GLOSSARY.md` |
| Normative operational documentation | Verified operator/contributor instructions | `docs/reference/*.md`, `RELEASING.md`, `CONTRIBUTING.md` |
| Explanatory public documentation | Entry points and summaries | `README.md`, `docs/README.md` |
| Evaluation material | Evaluation configuration and facilitator/evaluator material | `evaluation/scenarios.toml`, `docs/evaluation/evaluator/*` |
| Historical decision record | Decisions recorded at a point in time; status-tracked | `docs/ADRs/*`, `docs/DECISIONS.md`, `spec/` |
| Dated audit | Execution record of a dated review; not normative | `docs/audits/*` |
| Test fixture documentation | Provenance of committed test fixtures | `tests/fixtures/README.md` |
| Packaging or legal documentation | License, notices, package metadata | `LICENSE`, `THIRD_PARTY_NOTICES.md`, `Cargo.toml` |

Conflict resolution follows `docs/README.md`: canonical evidence >
source snapshots > reviewed policy/profile/interpretation > immutable
plan/run > generated reports > normative specifications > explanatory
documentation > dated audits > historical records. A dated audit may
report historical execution facts; it never redefines current normative
semantics. An ADR records a decision at a point in time; its status
(`Accepted` / `Superseded` / `Partially superseded`) decides current
applicability. An evaluator task document is not a semantic
specification. The evidence-derived answer key is generated from
canonical artifacts; it is not itself canonical route evidence.

## Surface inventory

### Normative product specification (current, authored)

| Path | Audience | Source of truth | Review status |
|---|---|---|---|
| `docs/README.md` | everyone | itself (authority model) + implementation | reviewed 2026-08 |
| `docs/GLOSSARY.md` | everyone | implementation + evidence vocabulary | reviewed 2026-08 |
| `docs/DESIGN.md` | contributors | implementation | reviewed 2026-08 |
| `docs/DOMAIN.md` | contributors | implementation | reviewed 2026-08 |
| `docs/OBSERVABILITY.md` | operators/analysts | implementation + evidence model | reviewed 2026-08 |
| `docs/OPERATIONS.md` | operators | implementation (job service) | reviewed 2026-08 |
| `docs/DATA_PROVENANCE.md` | contributors | implementation + evidence store | reviewed 2026-08 |
| `docs/UX.md` | contributors | accepted workbench baseline | reviewed 2026-08 |
| `docs/sources/GRNOC_PUBLIC_TASK_VIEWER.md` | contributors | source adapter + dated protocol probes | reviewed 2026-08 |
| `docs/sources/GRNOC_BULK_ACCESS_REQUEST.md` | contributors | draft request (not sent) | reviewed 2026-08 |

### Normative operational and reference documentation

| Path | Audience | Source of truth | Review status |
|---|---|---|---|
| `docs/reference/CLI.md` | operators/contributors | `inim --help` (checked) | added 2026-08 |
| `docs/reference/API.md` | operators/contributors | router definitions (checked) | added 2026-08 |
| `docs/reference/WEB-ROUTES.md` | maintainers | router definitions (checked) | added 2026-08 |
| `docs/reference/CATALOG-SCHEMA.md` | maintainers | migrations (checked) | added 2026-08 |
| `docs/reference/SCHEMA-VERSIONS.md` | maintainers | schema constants (checked) | added 2026-08 |
| `README.md` | users/evaluators | implementation + evidence | reviewed 2026-08 |
| `CONTRIBUTING.md` | contributors | repository policy | reviewed 2026-08 |
| `RELEASING.md` | maintainers | release policy | reviewed 2026-08 |
| `CHANGELOG.md` | users | git history | reviewed 2026-08 |
| `docs/STATUS.md` | users/evaluators | implementation state | added 2026-08 |

### Reviewed project policy and configuration

| Path | Class | Schema | Status |
|---|---|---|---|
| `config/project-scope.toml` | Reviewed project policy | v1 | unchanged 2026-08 |
| `docs/evaluation/ALPHA-FREEZE.md` | Reviewed project policy | — | active |
| `manifests/INC0299001.json` | Reviewed configuration | manifest v2 | reviewed |
| `manifests/INC0301970.json` | Reviewed configuration | manifest v2 | reviewed |
| `manifests/INC0302574.json` | Reviewed configuration | manifest v2 | reviewed |
| `manifests/INC0040293.json` | Reviewed configuration | manifest v2 | reviewed |

### Reviewed case-study interpretation (authored)

| Path | Audience | Class |
|---|---|---|
| `case-studies/manlan-2019/README.md` | analysts | Reviewed interpretation |
| `case-studies/manlan-2019/pilot/*.md` (8 files) | analysts | Reviewed interpretation (decision records) |
| `case-studies/inc0299001/README.md` | analysts | Reviewed interpretation |
| `case-studies/inc0302574/README.md` | analysts | Reviewed interpretation |
| `case-studies/manlan-esnet-2019/README.md` | analysts | Reviewed interpretation |
| `case-studies/indiana-gigapop-smithville-2026/README.md` | analysts | Reviewed interpretation (provisional) |

### Evaluation material

| Path | Audience | Authored/Generated | Class |
|---|---|---|---|
| `evaluation/scenarios.toml` | facilitators, CI | authored | Evaluation config (schema v1) |
| `evaluation/generated/answer-key.json` | facilitators | generated | Generated report (schema v1) |
| `evaluation/generated/answer-key.md` | facilitators | generated | Generated report |
| `docs/evaluation/evaluator/NOC-ALPHA-TASKS.md` | evaluators | authored | Evaluation material |
| `docs/evaluation/evaluator/NOC-ALPHA-RESPONSE-SHEET.md` | evaluators | authored | Evaluation material |
| `docs/evaluation/evaluator/TERMS.md` | evaluators | authored | Evaluation material |
| `docs/evaluation/facilitator/NOC-ALPHA-FACILITATOR-GUIDE.md` | facilitators | authored | Evaluation material |
| `docs/evaluation/facilitator/SESSION-NOTES-TEMPLATE.md` | facilitators | authored | Evaluation material |
| `docs/evaluation/facilitator/POST-SESSION-DECISION.md` | facilitators | authored | Evaluation material |
| `docs/evaluation/FEEDBACK-TRIAGE.md` | facilitators/owner | authored | Evaluation material |
| `docs/evaluation/PILOT-REGISTRY.md` | owner | authored | Reviewed project policy (zero external sessions) |
| `docs/evaluation/EXTERNAL-PILOT-CHECKLIST.md` | facilitators | authored | Evaluation material |
| `docs/evaluation/POST-PILOT-DECISION-GATE.md` | owner | authored | Evaluation material |
| `docs/evaluation/EVALUATION-DATA-HANDLING.md` | all | authored | Evaluation material |
| `docs/evaluation/NOC-ALPHA-INVITATION.md` | owner | authored | Evaluation material (draft) |
| `docs/evaluation/NOC-ALPHA-EVALUATION.md` | owner/facilitators | authored | Historical (superseded 2026-08-04) |
| `docs/evaluation/SECOND-NETWORK-ALPHA-HANDOFF.md` | owner/facilitators | authored | Evaluation material |

### Historical decision records

| Path | Status |
|---|---|
| `docs/ADRs/README.md` (index) | maintained |
| `docs/ADRs/LOCAL-CATALOG-AND-WEB.md` | Accepted |
| `docs/ADRs/CASE-STUDY-LAYER.md` | Accepted |
| `docs/ADRs/DURABLE-ANALYSIS-JOBS.md` | Accepted |
| `docs/ADRs/MONOCLE-DATAPLANE.md` | Rejected (Monocle not adopted) |
| `docs/ADRs/RIPE-RIS-SUPPORT.md` | Accepted |
| `docs/DECISIONS.md` | historical |
| `docs/MONOCLE_EVALUATION.md` | historical |
| `docs/REQUIREMENTS.md`, `docs/TASKS.md` | historical planning |
| `docs/session-10-baseline.md` | historical |
| `spec/` (13 files) | historical session specifications |

### Dated audits (historical execution records)

| Path | Subject |
|---|---|
| `docs/audits/2026-08-repository-truth-audit.md` | repository truth audit (rendered from inventory) |
| `docs/audits/repository-inventory.json` | machine-readable inventory (checked source) |
| `docs/audits/2026-08-clean-clone.md` | clean-clone acceptance baseline |
| `docs/audits/2026-08-documentation-clean-clone.md` | clean-clone documentation verification |
| `docs/audits/2026-08-evaluator-journey.md` | evaluator journey review |
| `docs/audits/2026-08-evaluation-task-answerability.md` | task answerability |
| `docs/audits/2026-08-evaluation-accessibility.md` | accessibility review |
| `docs/audits/2026-08-evaluation-procedural-dry-run.md` | internal procedural dry run |
| `docs/audits/2026-08-evaluator-bootstrap.md` | bootstrap verification |
| `docs/audits/2026-08-fresh-event-discovery.md` | GRNOC discovery probes |
| `docs/audits/2026-08-fresh-event-candidates.md` | candidate events |
| `docs/audits/2026-08-non-noaa-ip-event-candidates.md` | candidate shortlist |
| `docs/audits/2026-08-grnoc-catalog-reconciliation.md` | corpus reconciliation |
| `docs/audits/2026-08-manlan-ticket-readiness.md` | MAN LAN ticket readiness |
| `docs/audits/2026-08-project-scope-noaa-removal.md` | exclusion decision (allowlisted) |
| `docs/audits/2026-08-second-network-neutrality.md` | source-neutrality audit |
| `docs/audits/2026-08-smithville-source-refresh.md` | Smithville refresh evidence |
| `docs/audits/2026-08-incident-family-deferral.md` | deferral decision |
| `docs/audits/external-links-2026-08.md` | external link status record |
| `docs/audits/README.md` | dated audit index |
| `docs/audits/2026-08-documentation-inventory.md` | this document |
| `docs/audits/2026-08-specification-coverage.md` | specification coverage matrix |
| `docs/audits/2026-08-documentation-spec-conformance.md` | final conformance audit |

### Generated documentation (regenerable)

| Path | Generator | Schema | Regeneration |
|---|---|---|---|
| `docs/audits/repository-inventory.json` | manual + `scripts/build-repo-audit.py` | — | render checked by audit |
| `docs/audits/2026-08-repository-truth-audit.md` | `scripts/build-repo-audit.py` | — | drift-checked |
| `evaluation/generated/answer-key.json` | `scripts/build-evaluation-answer-key.py` | v1 | drift-checked in CI |
| `evaluation/generated/answer-key.md` | same | — | drift-checked |
| `case-studies/manlan-2019/pilot/absence-audit.json` | `scripts/audit_pilot_absence.py` | — | reviewed |
| `case-studies/manlan-esnet-2019/assessment-audit.json` | `scripts/audit-esnet-assessment.py` | — | reviewed |

### Packaging and legal

`LICENSE` (MIT), `THIRD_PARTY_NOTICES.md`, `Cargo.toml` (package
metadata), `Cargo.lock`, `deny.toml`, `.gitignore`.

### GitHub/community surfaces

`.github/workflows/ci.yml`, `.github/ISSUE_TEMPLATE/noc-alpha-feedback.yml`,
`.github/PULL_REQUEST_TEMPLATE.md`, `.github/dependabot.yml` — all
reviewed 2026-08.

### Rust module documentation surfaces (checked separately)

Module-level `//!` documentation exists in `src/lib.rs` and the major
modules (`domain`, `sources`, `catalog`, `catalog/jobs`, `worker`,
`execution`, `schema`, `outcome`, `plan`, `lifecycle`, `report`,
`evidence`, `web`, `evaluation`). The Rustdoc audit result is recorded
in the final conformance audit (`docs/audits/2026-08-documentation-spec-conformance.md`).

## Checked lists (drift-guarded)

The following lists are compared with `git ls-files` by
`scripts/audit-docs.py`. Every tracked Markdown file and every tracked
`docs/` file must appear in the corresponding list; every listed file
must be classified in a table above (or in `repository-inventory.json`
for non-documentation files).

### Tracked Markdown files (97, excluding `spec/`)

```
.github/PULL_REQUEST_TEMPLATE.md
CHANGELOG.md
CONTRIBUTING.md
README.md
RELEASING.md
THIRD_PARTY_NOTICES.md
case-studies/inc0299001/README.md
case-studies/inc0302574/README.md
case-studies/indiana-gigapop-smithville-2026/README.md
case-studies/manlan-2019/README.md
case-studies/manlan-2019/pilot/PILOT-SELECTION.md
case-studies/manlan-2019/pilot/batch-reuse-report.md
case-studies/manlan-2019/pilot/corpus-validation.md
case-studies/manlan-2019/pilot/cross-observer-matrix.md
case-studies/manlan-2019/pilot/ris-collector-selection.md
case-studies/manlan-2019/pilot/rrc11-audit-2019.md
case-studies/manlan-2019/pilot/rrc11-pex-pilot-decision.md
case-studies/manlan-2019/pilot/session-audit-2019.md
case-studies/manlan-esnet-2019/README.md
docs/ADRs/CASE-STUDY-LAYER.md
docs/ADRs/DURABLE-ANALYSIS-JOBS.md
docs/ADRs/LOCAL-CATALOG-AND-WEB.md
docs/ADRs/MONOCLE-DATAPLANE.md
docs/ADRs/README.md
docs/ADRs/RIPE-RIS-SUPPORT.md
docs/BENCHMARK.md
docs/computational-model.md
docs/DATA_PROVENANCE.md
docs/DECISIONS.md
docs/DESIGN.md
docs/design/algorithms.md
docs/design/algorithm-data-matrix.md
docs/design/data-structures.md
docs/design/invariants.md
docs/design/state-machines.md
docs/DOMAIN.md
docs/GLOSSARY.md
docs/MONOCLE_EVALUATION.md
docs/OBSERVABILITY.md
docs/OPERATIONS.md
docs/README.md
docs/REQUIREMENTS.md
docs/STATUS.md
docs/TASKS.md
docs/UX.md
docs/audits/2026-08-clean-clone.md
docs/audits/2026-08-documentation-clean-clone.md
docs/audits/2026-08-documentation-inventory.md
docs/audits/2026-08-documentation-spec-conformance.md
docs/audits/2026-08-entity-taxonomy-smithville-summary.md
docs/audits/2026-08-evaluation-accessibility.md
docs/audits/2026-08-evaluation-procedural-dry-run.md
docs/audits/2026-08-evaluation-task-answerability.md
docs/audits/2026-08-evaluator-bootstrap.md
docs/audits/2026-08-evaluator-journey.md
docs/audits/2026-08-fresh-event-candidates.md
docs/audits/2026-08-fresh-event-discovery.md
docs/audits/2026-08-grnoc-catalog-reconciliation.md
docs/audits/2026-08-incident-family-deferral.md
docs/audits/2026-08-internal-evaluator-findings.md
docs/audits/2026-08-manlan-ticket-readiness.md
docs/audits/2026-08-non-noaa-ip-event-candidates.md
docs/audits/2026-08-pre-pilot-invariant-closure.md
docs/audits/2026-08-project-scope-noaa-removal.md
docs/audits/2026-08-repository-truth-audit.md
docs/audits/2026-08-second-network-neutrality.md
docs/audits/2026-08-smithville-source-refresh.md
docs/audits/2026-08-specification-coverage.md
docs/audits/2026-08-wirthian-design-recovery.md
docs/audits/README.md
docs/audits/external-links-2026-08.md
docs/evaluation/ALPHA-FREEZE.md
docs/evaluation/EVALUATION-DATA-HANDLING.md
docs/evaluation/EXTERNAL-PILOT-CHECKLIST.md
docs/evaluation/FEEDBACK-TRIAGE.md
docs/evaluation/NOC-ALPHA-EVALUATION.md
docs/evaluation/NOC-ALPHA-INVITATION.md
docs/evaluation/PILOT-REGISTRY.md
docs/evaluation/POST-PILOT-DECISION-GATE.md
docs/evaluation/SECOND-NETWORK-ALPHA-HANDOFF.md
docs/evaluation/evaluator/NOC-ALPHA-RESPONSE-SHEET.md
docs/evaluation/evaluator/NOC-ALPHA-TASKS.md
docs/evaluation/evaluator/TERMS.md
docs/evaluation/facilitator/NOC-ALPHA-FACILITATOR-GUIDE.md
docs/evaluation/facilitator/POST-SESSION-DECISION.md
docs/evaluation/facilitator/SESSION-NOTES-TEMPLATE.md
docs/reference/API.md
docs/reference/ARTIFACTS.md
docs/reference/CATALOG-SCHEMA.md
docs/reference/CLI.md
docs/reference/SCHEMA-VERSIONS.md
docs/reference/WEB-ROUTES.md
docs/session-10-baseline.md
docs/sources/GRNOC_BULK_ACCESS_REQUEST.md
docs/sources/GRNOC_PUBLIC_TASK_VIEWER.md
evaluation/generated/answer-key.md
tests/fixtures/README.md
```







### Tracked files under `docs/` (77)

```
docs/ADRs/CASE-STUDY-LAYER.md
docs/ADRs/DURABLE-ANALYSIS-JOBS.md
docs/ADRs/LOCAL-CATALOG-AND-WEB.md
docs/ADRs/MONOCLE-DATAPLANE.md
docs/ADRs/README.md
docs/ADRs/RIPE-RIS-SUPPORT.md
docs/BENCHMARK.md
docs/computational-model.md
docs/DATA_PROVENANCE.md
docs/DECISIONS.md
docs/DESIGN.md
docs/design/algorithms.md
docs/design/algorithm-data-matrix.md
docs/design/data-structures.md
docs/design/invariants.md
docs/design/state-machines.md
docs/DOMAIN.md
docs/GLOSSARY.md
docs/MONOCLE_EVALUATION.md
docs/OBSERVABILITY.md
docs/OPERATIONS.md
docs/README.md
docs/REQUIREMENTS.md
docs/STATUS.md
docs/TASKS.md
docs/UX.md
docs/audits/2026-08-clean-clone.md
docs/audits/2026-08-documentation-clean-clone.md
docs/audits/2026-08-documentation-inventory.md
docs/audits/2026-08-documentation-spec-conformance.md
docs/audits/2026-08-entity-taxonomy-smithville-summary.md
docs/audits/2026-08-evaluation-accessibility.md
docs/audits/2026-08-evaluation-procedural-dry-run.md
docs/audits/2026-08-evaluation-task-answerability.md
docs/audits/2026-08-evaluator-bootstrap.md
docs/audits/2026-08-evaluator-journey.md
docs/audits/2026-08-fresh-event-candidates.md
docs/audits/2026-08-fresh-event-discovery.md
docs/audits/2026-08-grnoc-catalog-reconciliation.md
docs/audits/2026-08-incident-family-deferral.md
docs/audits/2026-08-internal-evaluator-findings.md
docs/audits/2026-08-manlan-ticket-readiness.md
docs/audits/2026-08-non-noaa-ip-event-candidates.md
docs/audits/2026-08-pre-pilot-invariant-closure.md
docs/audits/2026-08-project-scope-noaa-removal.md
docs/audits/2026-08-repository-truth-audit.md
docs/audits/2026-08-second-network-neutrality.md
docs/audits/2026-08-smithville-source-refresh.md
docs/audits/2026-08-specification-coverage.md
docs/audits/2026-08-wirthian-design-recovery.md
docs/audits/README.md
docs/audits/external-links-2026-08.md
docs/audits/repository-inventory.json
docs/evaluation/ALPHA-FREEZE.md
docs/evaluation/EVALUATION-DATA-HANDLING.md
docs/evaluation/EXTERNAL-PILOT-CHECKLIST.md
docs/evaluation/FEEDBACK-TRIAGE.md
docs/evaluation/NOC-ALPHA-EVALUATION.md
docs/evaluation/NOC-ALPHA-INVITATION.md
docs/evaluation/PILOT-REGISTRY.md
docs/evaluation/POST-PILOT-DECISION-GATE.md
docs/evaluation/SECOND-NETWORK-ALPHA-HANDOFF.md
docs/evaluation/evaluator/NOC-ALPHA-RESPONSE-SHEET.md
docs/evaluation/evaluator/NOC-ALPHA-TASKS.md
docs/evaluation/evaluator/TERMS.md
docs/evaluation/facilitator/NOC-ALPHA-FACILITATOR-GUIDE.md
docs/evaluation/facilitator/POST-SESSION-DECISION.md
docs/evaluation/facilitator/SESSION-NOTES-TEMPLATE.md
docs/reference/API.md
docs/reference/ARTIFACTS.md
docs/reference/CATALOG-SCHEMA.md
docs/reference/CLI.md
docs/reference/SCHEMA-VERSIONS.md
docs/reference/WEB-ROUTES.md
docs/session-10-baseline.md
docs/sources/GRNOC_BULK_ACCESS_REQUEST.md
docs/sources/GRNOC_PUBLIC_TASK_VIEWER.md
```







### Tracked evaluation files (3)

```
evaluation/generated/answer-key.json
evaluation/generated/answer-key.md
evaluation/scenarios.toml
```

### Tracked GitHub files (4)

```
.github/ISSUE_TEMPLATE/noc-alpha-feedback.yml
.github/PULL_REQUEST_TEMPLATE.md
.github/dependabot.yml
.github/workflows/ci.yml
```

## Required checks satisfied

- every_tracked_markdown_file_is_classified — checked list above plus
  the `repository-inventory.json` render (which covers all 442 files);
  drift guard verifies both.
- every_generated_document_has_generator — generator column above;
  answer key and repo audit carry generation headers.
- every_normative_document_has_authority_source — source-of-truth
  column above.
- historical_and_current_documents_are_distinct — historical records
  are listed separately and exempt from current-doc checks.
- evaluation_documents_are_classified_by_audience — audience column
  above.
- no_runtime_document_is_classified_as_tracked_normative_source —
  runtime state (`data/`, `cache/`, `tmp/`) is never tracked; the
  demo manifest is generated at runtime and documented as such, not
  treated as a normative source.
