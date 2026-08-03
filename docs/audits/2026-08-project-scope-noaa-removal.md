# Project-scope exclusion: NOAA removal — 2026-08-03

Dated implementation audit. This is not a normative argument and does
not speculate about the reason for the decision.

## Decision

The project owner explicitly excluded NOAA from the project. The
exclusion is a project-scope decision, recorded as reviewed policy in
`config/project-scope.toml` (schema v1): the reviewed entity NOAA with
AS270, and the exact source record INC0303298.

Project scope is distinct from analytical applicability: the exclusion
does not mark the event analytically invalid, unobservable, or failed.
It means inim intentionally does not include this entity or source
event in the active project corpus.

## Current-tree removal

- `case-studies/inc0303298-noaa/` (README, finding-chronology audit,
  run artifacts) — deleted from the current tree.
- `manifests/INC0303298.json` — deleted.
- `tests/fixtures/grnoc/INC0303298.json` — deleted.
- Demo metadata, README case-study index, repository inventory,
  discovery-audit selection rows, and count-based tests updated.
- Net removal: 18,231 tracked lines / ~150 KB of case material.

**Git history was not rewritten.** The Session 48 merge and all
intermediate commits remain in the published history; earlier versions
of the removed files remain reachable there. The current main no longer
contains the NOAA case material.

## Policy and enforcement

- `config/project-scope.toml` is the reviewed authority (exact,
  normalized matching only; precedence: external source ID, reviewed
  entity name, reviewed ASN, exact alias).
- Queue and retry refuse excluded plans with the stable machine code
  `project_scope_excluded` and operator-facing scope language.
- The worker rechecks project scope after claim and before any source
  access; a job whose event became excluded while queued is cancelled
  with `excluded_by_project_scope` (zero network, zero MRT, no run, no
  staging).
- Imports skip excluded manifests explicitly (`scope_skipped`).
- Default web/API/candidate/dashboard/jobs views omit excluded items;
  direct access to an excluded item is 404.
- `inim project-scope show` and `inim project-scope audit` are
  read-only; the audit reports excluded catalog counts and never
  deletes.

## Runtime records

Existing private runtime catalog records (the Session 48 event, plan,
job, run, and artifacts) were NOT destructively mutated. They remain
immutable historical execution records, hidden from default views. A
`project-scope audit` on the runtime catalog reports: 1 excluded event,
1 plan, 1 job, 1 run, 11 artifacts.

## Fresh demos

New demo catalogs no longer import the excluded event or its run
(4 events, 3 runs). `demo verify` fails when an excluded event is
present.

## Generic Session 48 fixes retained

Ticket-title normalization, the manifest-label expectation bug fix,
hyphenated attachment-qualifier parsing, the bounded discovery request
budget, preflight failure reporting, and the observed-result /
expectation-assessment separation all remain in place (regression
fixtures were converted to neutral identities).

## Current references to the excluded entity

Allowed references (drift-guard allowlist):

- `config/project-scope.toml` — the reviewed policy entry.
- `docs/audits/2026-08-project-scope-noaa-removal.md` — this audit.
- `tests/project_scope_policy_test.rs` — the current-policy
  integration test.
