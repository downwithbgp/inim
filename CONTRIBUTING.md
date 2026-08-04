# Contributing to inim

Thanks for considering a contribution. This project is a small, careful
data-analysis tool; keep contributions proportionate to the request.

## License

inim is licensed under the **MIT License** (see `LICENSE`).
SPDX-License-Identifier: MIT

By submitting a contribution you agree that it is licensed under the same
MIT License (inbound = outbound). You must have the right to submit the
work (your own original work, or work you are licensed to contribute).

There is no Contributor License Agreement.

## Before you submit

- **Quality gates must pass:**
  ```sh
  cargo fmt --check
  cargo test
  cargo test --release
  cargo test --doc
  cargo clippy --all-targets --all-features -- -D warnings
  cargo doc --no-deps --document-private-items   # warnings denied
  scripts/audit-docs.sh                          # documentation drift audit
  cargo deny check licenses && cargo deny check bans
  ```
- **Alpha-freeze awareness.** During the external alpha evaluation
  period, feature expansion is paused (see
  `docs/evaluation/ALPHA-FREEZE.md`): correctness, provenance,
  security, accessibility, and evaluator-blocking fixes remain welcome;
  new dashboards, events, aggregate metrics, and speculative conveniences
  are deferred. Feature proposals may be recorded and deferred.
- **Evaluator feedback should include the task and evidence.** Feedback
  without a task reference and an evidence reference is hard to triage
  (see `docs/evaluation/FEEDBACK-TRIAGE.md`).
- **Case-study additions require reviewed scope and provenance.**
  Screenshots alone are not sufficient evidence for a semantic
  correction.
- **No live-network tests in CI.** CI is fully offline; the
  evaluation-smoke job verifies the demo and evaluation kit without
  contacting any live source.
- **Project-scope policy must be respected.**
  `config/project-scope.toml` is the reviewed authority; excluded
  entities and source records never appear in default views or the
  evaluation material.
- **Event subjects and ASN mappings are data, not code.** A new ticket
  title, participant, or transit ASN belongs in a reviewed manifest
  (`manifests/`) with provenance — never as a special case in production
  code.
- **External fixtures require provenance.** If you add a fixture from
  upstream or public sources, record its source, license basis, and
  checksum in `tests/fixtures/README.md`.
- **Generated artifacts should not be committed casually.** Re-run
  analyses and commit current-schema outputs when an analysis changes
  materially; do not commit scratch outputs, caches, or raw MRT archives
  (`cache/` is gitignored; `out/` is excluded from the crate package).
- **Canonical evidence is immutable.** Artifacts under
  `case-studies/*/out/` are protocol evidence or derived run outputs:
  never hand-edit them, never modify archive hashes, observation IDs, or
  canonical transitions. Correct stale presentation prose or regenerate
  through the documented command; do not make generated JSON agree with
  prose by hand.
- **Documentation follows the authority model.** Current normative
  documentation must describe the current implementation and evidence;
  historical records (ADRs, dated audits, `spec/`) are exempt and are
  never rewritten to look current. See `docs/README.md` for the
  authority hierarchy and conflict-resolution order.
- **Generated documents are regenerated, not hand-edited.** Generated
  files (evaluation answer key, repository-truth audit render,
  demo manifest) carry a generation header; change the generator, then
  regenerate, and commit both together. Regeneration must be
  deterministic (no volatile timestamps).
- **Keep changes surgical.** Match the existing style; every changed line
  should trace to the request.
- **New persisted formats carry schema versions.** Bump the schema and
  reject old identity semantics rather than silently reinterpreting them.

## Public history

The repository is public at https://github.com/downwithbgp/inim.

- **Public history must not be rewritten without extraordinary reason.**
- Future work uses ordinary commits and ordinary `git revert` commits for
  published mistakes.
- **Force-pushes to `main` are prohibited.**
- Generated runtime data stays out of Git: `data/`, `cache/`, `tmp/`,
  and the top-level `out/` directory are gitignored. Reviewed evidence
  lives under `case-studies/` and is committed deliberately.

## Release checklist

See `RELEASING.md` for the release-readiness checklist (gates, license
audit, packaging, verification).

## Job workflow development

The job service (`src/catalog/jobs/service.rs`) owns all business
rules; web handlers, CLI commands, and the worker call it — never
duplicate rules in command handlers. The worker is the only component
that performs source access. Tests must stay offline: the e2e fixture
(`tests/queued_analysis_e2e_test.rs`) runs the real worker against a
tracked MRT fixture in `--offline` mode.
