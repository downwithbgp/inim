# Documentation and specification conformance audit — 2026-08

Dated audit. Records the documentation and specification conformance
review performed after the evaluation-kit milestone. This document is
an execution record, not a normative specification; current normative
sources are linked from `docs/README.md`.

## Starting state

- Starting commit: `91ac498c5a1aa0793f027445dec59f257c212b73`
  (evaluation-kit merge, 2026-08-04)
- Branch: `session-52-documentation-spec-conformance`
- Rust `1.97.1`, Cargo `1.97.1`; test count at start: 1369 passing
- Catalog schema v10; report schema v3 (v1–v2 frozen legacy); API v1;
  project-scope policy v1; scenario manifest v1; answer key v1
- Open PRs: none; Dependabot PRs: none

## Inventory

- 442 tracked files at start; 451 after this audit's additions
  (STATUS.md, six reference docs, two dated audits, audits index,
  inventory entries)
- Documentation surfaces classified: 75 tracked Markdown (excluding
  `spec/`), 55 files under `docs/`, 3 evaluation files, 4 GitHub files
- Surface inventory: `docs/audits/2026-08-documentation-inventory.md`
  (checked lists drift-guarded by `scripts/audit-docs.py`)
- Authority model: `docs/README.md` (15 classes, explicit conflict
  resolution; evaluation material and dated audits are not normative)
- Specification coverage matrix:
  `docs/audits/2026-08-specification-coverage.md` (27 areas, all with
  normative documents)

## Schema versions found (implementation authority)

| Surface | Version |
|---|---|
| Catalog database | v10 (`src/catalog/migrations.rs`) |
| Manifest | v2 |
| RIB / UPDATE caches, RouteObservation | v2 each |
| Cohort identity, source-extraction, evidence appendix, archive manifest, lifecycle, transitions, withdrawal audit, semantic wave, comparison, analysis plan, execution metadata, performance, project scope, case-study data, target research, demo manifest, scenario manifest, answer key | v1 each |
| Report | v3 current; v1–v2 frozen legacy |

Full registry: `docs/reference/SCHEMA-VERSIONS.md` (checked against
constants by the drift guard).

## CLI verification

- 45 `--help` captures from the current debug binary; every documented
  command and option verified
- Reference: `docs/reference/CLI.md`; exit codes 0–5 documented and
  verified; network-access and mutation markers complete; worker
  defaults (poll 2s, max-jobs 1, download-jobs 2, parse-jobs 8) match
  help
- Drift guard verifies every `inim <command>` form in the reference
  against the binary help tree

## API and web route verification

- 47 read routes and 7 write routes documented
  (`docs/reference/API.md`, `docs/reference/WEB-ROUTES.md`)
- Verified against `src/catalog/web` router source: write gating (404
  when disabled), CSRF header `X-Inim-CSRF` (403), 64 KiB body limit
  (413), error envelope `{api_version, error: {message}}`, scope 404s,
  duplicate-queue `already_queued`, pagination 1–200
- Drift guard compares the documented route tables with the router
  source bidirectionally

## Catalog and artifact verification

- Catalog reference: `docs/reference/CATALOG-SCHEMA.md` (v10, tables by
  migration V1–V10, foreign keys, PRAGMAs: WAL, foreign_keys ON,
  busy_timeout 5 s; immutability boundaries; cleanup policy; demo
  import behavior; runtime boundary)
- Artifact reference: `docs/reference/ARTIFACTS.md` — normal completed
  set, insufficient-visibility minimal set (verified against
  `write_insufficient_visibility_artifacts`), blocked-plan set,
  optional/derived/archived sets; generated reports are not canonical
  evidence
- Historical note added: the tracked Smithville run directory predates
  the current minimal-set extension (empty lifecycle/withdrawal
  audit/appendix) and is documented as the earlier subset exactly as
  published

## Case-study verification (all counts from artifacts)

| Case | Checked against | Result |
|---|---|---|
| MAN LAN / NORDUnet | `pilot-result.json`, `ris-pilot-*.json`, rrc15 `lifecycle.json`/`transitions.json` | 11/33 @ 16:45:25Z, 2 s absence; 30 path replacements; rrc00 11/11 no change; rrc06 12/12 @ 16:45:44Z; rrc15 13/24, return 17:03:32Z, cooldown re-change 17:52:16Z preserved; no prepend-change claims |
| UVA | `finding-chronology-audit.json`, `report.json` | baseline AS225×7, pre-withdrawal AS225×1, 54 ms absence (07:33:59.462→.516Z), first return ×7, final ×1, 12th prefix distinct; schema v2 label correct |
| INC0302574 (I2PX) | `relationship-audit.json` | RRC11+RRC14 direct AS11164 sessions, zero AS3333-origin via them, no AS11164 in path, decision insufficient-visibility, supporting run classified `supporting-re-plane` |
| ESnet optical | `assessment-audit.json` | scope-mismatched supporting observation only; no interface/traffic/less-impact claims |
| Smithville | `INC0301970.source.json`, `manifests/INC0301970.json`, `report.json` | open event, cutoff 2026-08-04T00:01:37Z, Adjacent(19782,11550), InsufficientVisibility (schema v3); **corrected** analysis-horizon start to the reviewed window (04:35:00Z; source work_start 04:35:26Z noted separately) |

## Evaluation-kit verification

- `evaluation/scenarios.toml` (schema v1): 5 scenarios, all paths
  verified against the deterministic demo (200, no write controls,
  database hash unchanged after browse)
- Task booklet: 27 core + 6 optional tasks; no answer leakage; no
  source-code/SQL/history requirement; 20–30 minute duration stated as
  an estimate with setup separate
- Answer key: regeneration byte-clean (two runs, `diff` empty);
  generation header (generator, schema v1, source demo-manifest SHA);
  not labeled canonical evidence
- Pilot registry: zero external sessions; internal dry run excluded
- Bootstrap and pack builders documented; pack SHA256SUMS explicitly
  not a signature; static HTML export decision documented (none
  produced)
- Freeze: active; references commit `91ac498` instead of session
  narrative; allowed-change categories include documentation
  maintenance

## Rustdoc and comment audit

- `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps
  --document-private-items`: zero warnings
- Module docs present for all major modules (`lib.rs`, domain, catalog,
  catalog/jobs, worker, execution, lifecycle, schema, web, sources)
- Two `///` test-module headers and seven plain test comments in
  `src/catalog/web/tests.rs` scrubbed of session numbers; one stale
  TODO in `main.rs` replaced with a factual comment
- TODO inventory: `src/orchestrate.rs:500` retained (specific and
  still valid: preflight `CachedTargetStream.peer_asn` is not captured
  from observations); no completed TODOs remain
- Reviewed manifest payloads and generated reviewed pilot documents
  still contain session-era wording — these are frozen evidence or
  reviewed artifacts and were **not** modified (changing them would
  change plan hashes or reviewed files)

## Corrections made (documentation-only)

- README: Cargo network disclosure; MRT/RIS/RIB first-use expansions;
  CSRF expansion
- DESIGN: measured page latency moved to BENCHMARK.md
- OPERATIONS: open-event cutoff requirement; demo corpus counts
  corrected from the generated manifest (12 runs, 12 plans, 10
  reviews, 5 workbenches)
- CHANGELOG: INC0302574 described as not-assessable (not "no impact")
- GLOSSARY: R&E/I2PX expansions; exact job-state vocabulary
- ALPHA-FREEZE + superseded protocol: merge-commit reference
- Smithville README: analysis-horizon start
- CONTRIBUTING: authority model + generated-file policy
- RELEASING: project-scope and documentation validation section
- TERMS.md, API.md: acronym expansions
- Script headers: usage text added; session narrative and
  agent-oriented wording removed (reviewed artifacts untouched)
- docs/README.md: reference docs, CI job table, audits index link
- New: STATUS.md, six reference docs, audits index, two dated audits

## Behavior corrections

None. Every change in this session is documentation-only. No
implementation behavior was altered; no canonical evidence, plan,
manifest, or reviewed artifact changed.

## Drift guards added

`scripts/audit-docs.py` (all offline, deterministic, path-aware):
documentation inventory checked lists; anchor resolution; CLI reference
commands; API/web route tables vs router; schema matrix vs constants;
specification coverage areas; STATUS.md invariants; prohibited current
claims (incident-wide verdict, stale label, Smithville no-change,
optical assessment, beta status); scenario paths vs answer-key demo
URLs; fixture-family documentation; script shebang/usage; CI job
documentation; job-state vocabulary; artifact-set reference. The audit
script remains the single documentation-audit framework; CI jobs are
unchanged except the docs job now exercises the extended script.

## Verification results

- Local gates: `cargo test` 1369/0, `cargo test --doc` 0/0, `cargo doc`
  warnings denied, `scripts/audit-docs.sh` ok, demo init/verify ok,
  answer-key regeneration clean, evaluation pack built, read-only
  smoke ok (5 scenario URLs, excluded 404, db hash unchanged),
  project-scope audit 0 excluded
- Packaged source: `cargo package` list verified (no runtime state, no
  excluded material outside allowlisted dated audits); extracted crate
  links resolve; packaged runtime has no git requirement
- Clean clone: see `docs/audits/2026-08-clean-clone.md` and the
  clean-clone section of this audit (filled at the end of the run)

## Remaining documentation debt

- `docs/BENCHMARK.md` memory figures are recorded in MB as measured by
  the benchmark tooling (dated measurements; not converted to MiB)
- Dated pilot decision documents (`case-studies/manlan-2019/pilot/*.md`)
  and reviewed manifests retain historical session-era wording; they
  are historical records or frozen evidence and are intentionally not
  normalized
- The `docs/evaluation/NOC-ALPHA-EVALUATION.md` protocol is retained as
  a superseded historical document rather than deleted
- The orchestrate.rs `peer_asn` capture TODO remains open (retained
  with precise meaning; not implemented during the freeze)

## Non-goals respected

No event added, no source refreshed, no archive acquired, no analysis
executed or queued, no worker started, no canonical evidence or plan
changed, no project-scope policy change, no allowlist expansion, no
excluded material introduced, no runtime SQLite/raw MRT/cache/
screenshot/private note committed, no release/tag created, no history
rewritten, no external evaluation claimed (registry remains zero).
