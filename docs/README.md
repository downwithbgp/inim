# inim — Documentation Map

This file is the entry point for reading the repository's documentation.
It defines which documents are authoritative for which facts, and where a
new contributor should look first.

## Documentation authority model

Facts in this repository have an explicit authority hierarchy. A
document lower in the hierarchy may explain, summarize, or interpret
facts from a higher level — it may never silently contradict them.

1. **Canonical protocol evidence** — immutable MRT-derived artifacts,
   archive checksums, transition and lifecycle evidence. These files are
   never hand-edited.
2. **Source declarations** — ticket snapshots, operator reports, and
   public source documents as acquired.
3. **Reviewed configuration and interpretation** — manifests, network
   profiles, reviewed case-study claims, target and predicate mappings,
   ASN identity reviews.
4. **Derived analysis** — findings, comparisons, workbench view models,
   and text/JSON reports generated from evidence.
5. **Explanatory documentation** — README, DESIGN, DOMAIN,
   OBSERVABILITY, DATA_PROVENANCE, UX, source documentation. These must
   describe the current implementation and evidence, and may never
   override canonical evidence.
6. **Historical rationale** — ADRs and dated decision records. These
   record what was decided when; later ADRs supersede them, but the
   original text is not rewritten.

Two additional classes are defined for completeness: **evaluation
material** (task booklets, scenario manifests, facilitator guides —
evaluation configuration, never a semantic specification) and
**dated audits** (`docs/audits/`, execution records of a dated review,
never current normative semantics). A dated audit may report
historical execution facts; it must not redefine current normative
semantics. An evaluator task document is not a semantic specification.
The evidence-derived answer key is generated from canonical artifacts;
it is not itself canonical route evidence.

Generated outputs identify their generator, schema version, and source
evidence or run. ADRs retain their original decision and carry a status
(`Accepted`, `Superseded`, `Partially superseded`) plus links to
follow-up ADRs.

## Where to read about what

| Topic | Document |
|---|---|
| Product purpose and current scope | `README.md` (root) |
| Current public status | `docs/STATUS.md` |
| Architecture and design decisions | `docs/DESIGN.md` |
| Domain model: identities, units, transitions | `docs/DOMAIN.md` |
| Evidence and provenance policy | `docs/DATA_PROVENANCE.md` |
| Observability limits (what BGP evidence can and cannot show) | `docs/OBSERVABILITY.md` |
| Operator UX and workbench design | `docs/UX.md` |
| Local operations: event → plan → job → workbench | `docs/OPERATIONS.md` |
| CLI reference (verified) | `docs/reference/CLI.md` |
| HTTP API reference (verified) | `docs/reference/API.md` |
| Web route reference (maintainers) | `docs/reference/WEB-ROUTES.md` |
| Catalog database reference | `docs/reference/CATALOG-SCHEMA.md` |
| Analysis artifact reference | `docs/reference/ARTIFACTS.md` |
| Schema/version matrix | `docs/reference/SCHEMA-VERSIONS.md` |
| Performance measurements | `docs/BENCHMARK.md` |
| As-built computational model (program equation, inputs, effects) | `docs/computational-model.md` |
| Design recovery: data structures, algorithms, invariants, state machines, matrix | `docs/design/` |
| Data sources: GRNOC, RouteViews, RIPE RIS | `docs/sources/` |
| Terminology (normative definitions) | `docs/GLOSSARY.md` |
| Case studies | `case-studies/` (per-case README files) |
| Release process and packaging | `RELEASING.md`, `CHANGELOG.md` |
| Contribution policy | `CONTRIBUTING.md` |
| Historical decisions | `docs/ADRs/` (index: `docs/ADRs/README.md`) |
| Dated audits (index) | `docs/audits/README.md` |
| Audit trail of this repository truth audit | `docs/audits/` |

## External alpha evaluation

Audiences are explicit: **evaluators** use the task booklet, glossary,
and response sheet; **facilitators** use the guide, answer key, notes
template, and triage; the **project owner** uses the freeze policy,
registry, and decision gate. Evaluators are never required to read
architecture or observability documentation before the tasks.

| Document | Audience |
|---|---|
| Alpha freeze policy (`ALPHA-FREEZE.md` — what may change during evaluation) | contributors, project owner |
| Evaluator task booklet (`evaluator/NOC-ALPHA-TASKS.md`) | evaluators |
| Evaluator glossary (`evaluator/TERMS.md`) | evaluators |
| Manual response sheet (`evaluator/NOC-ALPHA-RESPONSE-SHEET.md`) | evaluators |
| Facilitator guide (`facilitator/NOC-ALPHA-FACILITATOR-GUIDE.md`) | facilitators |
| Session-notes template (`facilitator/SESSION-NOTES-TEMPLATE.md`) | facilitators |
| Post-session decision template (`facilitator/POST-SESSION-DECISION.md`) | facilitators |
| Generated answer key (`evaluation/generated/answer-key.md`, JSON authoritative) | facilitators |
| Answer-key generation (`scripts/build-evaluation-answer-key.py`) | facilitators |
| Feedback triage (`FEEDBACK-TRIAGE.md`) | facilitators, project owner |
| Pilot registry (`PILOT-REGISTRY.md`) | project owner |
| External pilot checklist (`EXTERNAL-PILOT-CHECKLIST.md`) | facilitators |
| Post-pilot decision gate (`POST-PILOT-DECISION-GATE.md`) | project owner |
| Evaluation data handling (`EVALUATION-DATA-HANDLING.md`) | evaluators, facilitators |
| Invitation draft (`NOC-ALPHA-INVITATION.md`) | project owner |
| Scenario manifest (`evaluation/scenarios.toml`) | facilitators, CI |
| Evaluator bootstrap (`scripts/evaluator-bootstrap.sh`) | evaluators |
| Evaluation pack builder (`scripts/build-evaluation-pack.sh`) | facilitators |

The `docs/audits/` directory holds the dated evaluation audits
(evaluator journey, task answerability, accessibility, procedural dry
run). The audit index is `docs/audits/README.md`.

## CI jobs

CI (`.github/workflows/ci.yml`) runs these offline jobs on every push
and pull request. No live GRNOC, RouteViews, RIPE RIS, PeeringDB, or
RIR access happens in CI; no runtime data is uploaded; actions are
full-SHA pinned with reviewed release comments (policy in the workflow
file, which is the authority for pins).

| Job | What it runs |
|---|---|
| `fmt` | `cargo fmt --check` |
| `test` | `cargo test` + `cargo test --doc` |
| `clippy` | `cargo clippy --all-targets --all-features -- -D warnings` |
| `docs` | `cargo doc --no-deps --document-private-items` (warnings denied) + `scripts/audit-docs.sh` |
| `license` | `cargo deny check licenses` + `cargo deny check bans` |
| `package` | `cargo package` + packaged-runtime-material exclusion check |
| `queued-analysis-smoke` | offline queued-analysis e2e, project-scope smoke, demo init/verify on a fresh demo |
| `evaluation-smoke` | fresh demo + answer-key regeneration drift + evaluation pack + read-only server scenario URLs + database-hash check + excluded-material scan |

## Read order for a new contributor

1. `README.md` — what the project is, what it can and cannot conclude.
2. `docs/GLOSSARY.md` — the exact meaning of the core terms.
3. `docs/DESIGN.md` — how the pieces fit together.
4. `docs/DOMAIN.md` — the data model in detail.
5. `docs/OBSERVABILITY.md` — what the evidence actually means.
6. `docs/DATA_PROVENANCE.md` — where every artifact comes from.
7. One case study (`case-studies/manlan-2019/README.md` is the most
   complete) to see the evidence model in practice.

## Non-documentation directories

| Path | Contents |
|---|---|
| `src/` | Rust implementation (see `src/lib.rs` for module overview) |
| `tests/` | Integration tests and release/audit gates |
| `scripts/` | Developer tooling (audit renderers, screenshot harnesses) |
| `spec/` | Historical session specifications (not current documentation) |
| `manifests/` | Reviewed analysis manifests |
| `case-studies/` | Case-study evidence trees (canonical + reviewed) |
