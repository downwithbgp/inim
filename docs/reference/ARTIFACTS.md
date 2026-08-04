# inim analysis artifact reference

Concise reference for the artifacts produced by an analysis run.
Producer is the analysis pipeline (`src/execution.rs` →
`src/orchestrate.rs` → `src/output.rs`, publication in
`src/catalog/jobs/publish.rs`); schema versions are defined in
`src/schema.rs` (see `docs/reference/SCHEMA-VERSIONS.md`). Paths are
repository-relative (run-relative inside a run directory); no absolute
paths are written. Artifacts are immutable once published; generated
reports are derived from canonical evidence and are not themselves
canonical evidence.

## Normal completed-run artifact set

Written into the run directory (`data/runs/<job-id>/<event-id>/` for
queued runs; `<out>/<event-id>/` for direct `analyze`):

| Artifact | Purpose | Schema | Required fields |
|---|---|---|---|
| `report.json` | machine-readable report: observed signature, result, assessment, mechanism hints, limitations | v3 (current); v1–v2 frozen legacy | `schema_version`, `event_id`, `result`, `assessment`, `outcome`, `observed_event_signature`, `observable_mechanism_hints`, `limitations` |
| `report.txt` | human-readable report | unversioned | same content rendered |
| `archive_manifest.json` | every source archive: URL, local basename, collector, type, size, SHA-256 | v1 | `schema_version`, `event_id`, `archives[]` |
| `evidence_appendix.jsonl` | one line per transition: baseline/before/after states + triggering evidence (observation id, source URL, archive SHA, collector, peer, prefix, timestamp, element seq, path_id) | v1 | per-line records |
| `lifecycle.json` | per-stream lifecycle: baseline path, transitions, first change, restoration time, cooldown | v1 | `lifecycles[]` |
| `transitions.json` | compact per-transition records (kind, occurred UTC, phase, key, effects, observation id) | v1 | `transitions[]` |
| `semantic_waves.json` | wave boundaries and facet counts | v1 | `waves[]` |
| `withdrawal_audit.json` | withdrawn-stream audit | v1 | `summary`, `records[]` |
| `limitations.json` | limitations list | unversioned | `limitations[]` |
| `performance.json` | stage wall-clock timings, per-archive parse metrics | v1 | volatile; excluded from equivalence checks |
| `execution_metadata.json` | plan hash, job id, attempt, worker id, requested-by, timestamps | v1 | `metadata_schema_version`, `plan_hash`, `job_id`, `attempt` |

The catalog records each artifact's kind, **relative path**, media
type, schema version, SHA-256, and size in `analysis_artifacts`.

## Insufficient-visibility artifact set (no qualifying baseline)

The pipeline writes the same standard shapes with empty content — no
findings or timeline scaffolding are invented:

`report.json` (v3, `verdict: insufficient_visibility`), `report.txt`,
`limitations.json`, `archive_manifest.json` (the acquired baseline
archives only), `transitions.json` (empty), `semantic_waves.json`
(empty), `lifecycle.json` (empty), `withdrawal_audit.json` (zero
summary), `evidence_appendix.jsonl` (empty), plus
`execution_metadata.json` and `performance.json` from the worker.

> Historical note: the tracked Smithville run
> (`case-studies/indiana-gigapop-smithville-2026/out/INC0301970/`) was
> published before the minimal set gained the empty lifecycle,
> withdrawal-audit, and evidence-appendix files; its directory contains
> the earlier subset exactly as published (report/limitations/archive
> manifest/transitions/semantic waves/execution metadata). The
> case-study README documents the tracked directory; the writer
> contract above is current behavior.

## Zero-stream and blocked-plan artifacts

- A completed run with a qualifying cohort but zero route-state changes
  is a **normal completed run** (full artifact set; `NoRouteStateChange`
  observed signature) — not the insufficient-visibility set.
- A **blocked plan** (`inim plan` before acquisition) writes
  `analysis_plan.json` / `analysis_plan.txt` (status `Blocked`, reason,
  zero broker calls, zero MRT files examined) and `limitations.json` —
  no observational artifacts exist because no observation ran.
- A **preflight probe** (`--preflight-only`) prints its JSON to stdout;
  it does not produce a run directory.

## Optional and derived artifacts

| Artifact | Producer | Notes |
|---|---|---|
| `comparison.json` / `comparison.txt` | `inim compare` | per-claim comparison; never `ConfirmedCause` |
| `cross-observer-matrix.json` | `scripts/build-cross-observer-matrix.py` | per-observer × prefix rows |
| `finding-audit.json`, `finding-chronology-audit.json` | `catalog finding-audit` / `finding-chronology-audit` | read-only derivations from lifecycle evidence |
| `relationship-audit.json` | reviewed audit | e.g. INC0302574 bview audit |
| `absence-audit.json` | `scripts/audit_pilot_absence.py` | pilot absence verification |
| `assessment-audit.json` | `scripts/audit-esnet-assessment.py` | optical supporting-run audit |
| `pilot-result.json`, `session-audit-2019.json`, `rrc11-audit-2019.json`, `ris-collector-selection.md` | reviewed analysis | MAN LAN pilot decision records |

## Archived legacy artifacts

Old-schema artifacts are archived (e.g.
`out/archive/pre-observer-prefix-schema/`) and are never parsed as
current. Dated audits and the repository truth audit
(`docs/audits/2026-08-repository-truth-audit.md`) classify them as
historical. The archived MAN LAN `-RE-*` pilot directories predate the
current `-RIS-*` naming and are retained as historical evidence.

## Hash and provenance fields

- Every archive in `archive_manifest.json` carries URL + SHA-256; raw
  caches keep a `.sha256` sidecar verified on reuse.
- Evidence references carry observation id plus archive/URL provenance
  (see `docs/DATA_PROVENANCE.md`).
- Artifacts do **not** carry a generator field; generator identity
  (software version, parser identity) lives in the immutable
  `analysis_runs` row and the run's `stderr.log`.

## Package boundary

The crate package excludes `stderr.log` (and all runtime material).
The two tracked run `stderr.log` files are therefore absent from a
demo built from **packaged source**, so its `demo-manifest.json`
reports 116 imported artifacts instead of 118 from the git tree, and
an answer key generated from a packaged demo differs from the tracked
answer key only in the `demo_manifest` summary fields (artifact count
and the demo-manifest SHA-256) — never in scenario answers. The
tracked answer key is generated from the git tree.
