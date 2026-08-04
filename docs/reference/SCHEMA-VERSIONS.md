# Schema/version matrix — 2026-08

Every independently versioned surface in the repository. The
implementation authority is the constant or generator named in the
"Implementation authority" column; the matrix is checked against those
constants by `scripts/audit-docs.py`. Formats without a version
constant are marked **unversioned** — the matrix never invents a
version for them.

## Versioned surfaces

| Format | Current version | Implementation authority | Compatibility policy | Producer | Consumer | Tracked examples | Generated or authored |
|---|---|---|---|---|---|---|---|
| Catalog database | v11 | `src/catalog/migrations.rs` (`CATALOG_SCHEMA_VERSION`) | ordered migrations; future schema rejected at open | `catalog init`, `demo init` | web, CLI, worker | none (runtime) | generated at runtime |
| Manifest (analysis plan input) | v2 | `src/schema.rs` (`MANIFEST_SCHEMA_VERSION`) | v1 rejected with `LegacyManifestRequiresMigration`; offline `migrate-manifest` | authored | `plan`, `analyze`, catalog import | `manifests/*.json` | authored |
| RIB derived cache | v2 | `src/schema.rs` (`RIB_CACHE_SCHEMA_VERSION`) | mismatch → invalidated and rebuilt atomically | orchestrator | preflight/execution | none (runtime) | generated at runtime |
| UPDATE derived cache | v2 | `src/schema.rs` (`UPDATE_CACHE_SCHEMA_VERSION`) | mismatch → invalidated and rebuilt atomically | orchestrator | execution | none (runtime) | generated at runtime |
| RouteObservation | v2 | `src/schema.rs` (`OBSERVATION_SCHEMA_VERSION`) | ADD-PATH-aware identity; old versions never reused | ingest | derived caches | none (runtime) | generated at runtime |
| Frozen cohort identity | v1 | `src/schema.rs` (`COHORT_IDENTITY_SCHEMA_VERSION`) | part of cache identity | preflight | execution | none (runtime) | generated at runtime |
| Source-extraction cache | v1 | `src/catalog/source_extract.rs` (`EXTRACTION_SCHEMA_VERSION`) | bump invalidates all old extractions | orchestrator | plane runs/audits | none (runtime) | generated at runtime |
| Report | v3 (current); v1–v2 frozen legacy | `src/schema.rs` (`REPORT_SCHEMA_VERSION`) | current output dirs contain only current schemas; older reports archived | analysis pipeline | workbench, API, case studies | `case-studies/*/out/*/report.json` (v2 frozen; Smithville v3 current) | generated |
| Evidence appendix | v1 | `src/schema.rs` (`EVIDENCE_APPENDIX_SCHEMA_VERSION`) | current only | analysis pipeline | audits | `case-studies/*/out/*/evidence_appendix.jsonl` | generated |
| Archive manifest | v1 | `src/schema.rs` (`ARCHIVE_MANIFEST_SCHEMA_VERSION`) | current only | analysis pipeline | provenance | `case-studies/*/out/*/archive_manifest.json` | generated |
| Lifecycle artifact | v1 | `src/schema.rs` (`LIFECYCLE_ARTIFACT_SCHEMA_VERSION`) | current only | analysis pipeline | workbench, chronology audits | `case-studies/*/out/*/lifecycle.json` | generated |
| Transitions artifact | v1 | `src/schema.rs` (`TRANSITIONS_ARTIFACT_SCHEMA_VERSION`) | current only | analysis pipeline | phase summaries | `case-studies/*/out/*/transitions.json` | generated |
| Withdrawal audit | v1 | `src/schema.rs` (`WITHDRAWAL_AUDIT_SCHEMA_VERSION`) | current only | analysis pipeline | audits | `case-studies/*/out/*/withdrawal_audit.json` | generated |
| Semantic wave artifact | v1 | `src/schema.rs` (`SEMANTIC_WAVE_SCHEMA_VERSION`) | current only | analysis pipeline | workbench | `case-studies/*/out/*/semantic_waves.json` | generated |
| Comparison artifact | v1 | `src/schema.rs` (`COMPARISON_SCHEMA_VERSION`) | current only | `compare`, cross-observer matrix | comparisons | `case-studies/manlan-2019/pilot/cross-observer-matrix.json` | generated |
| Analysis-plan artifact | v1 | `src/schema.rs` (`ANALYSIS_PLAN_SCHEMA_VERSION`) | current only | `plan` | plan review | blocked-plan artifacts | generated |
| Execution metadata | v1 | `src/catalog/jobs/publish.rs` (`EXECUTION_METADATA_SCHEMA_VERSION`) | current only | worker publication | run provenance | `case-studies/*/out/*/execution_metadata.json` | generated |
| Performance metadata | v1 | `src/perf.rs` (`PERFORMANCE_SCHEMA_VERSION`) | volatile; excluded from equivalence checks | pipeline | benchmarks | `performance.json` in runs | generated |
| Project-scope policy | v1 | `src/catalog/scope.rs` (`SCOPE_CONFIG_SCHEMA_VERSION`) | unsupported version → startup hard error | authored | web, CLI, worker, demo | `config/project-scope.toml` | authored |
| Case-study data file | v1 | `src/catalog/case_study_import.rs` (`CASE_STUDY_DATA_SCHEMA_VERSION`) | unsupported version rejected | authored | case-study import | `case-studies/*/case-study.json` | authored |
| Target-research record | v1 | `src/catalog/target_research.rs` (`TARGET_RESEARCH_SCHEMA_VERSION`) | unsupported version rejected | authored + `apply-research` | case-study import | `case-studies/manlan-2019/target-research.json` | authored |
| Demo manifest | v1 | `src/catalog/demo.rs` (inline `schema_version`) | deterministic, timestamp-free | `demo init` | demo verify, answer-key generator, CI | none (runtime) | generated at runtime |
| Evaluation scenario manifest | v1 | `evaluation/scenarios.toml` (`schema_version`) | drift-guarded; no answers | authored | bootstrap, CI | `evaluation/scenarios.toml` | authored |
| Evaluation answer key | v1 | `scripts/build-evaluation-answer-key.py` (`SCHEMA_VERSION`) | regenerated deterministically; drift-checked in CI | generator script | facilitators | `evaluation/generated/answer-key.json` | generated |
| Absence audit | v1 | `scripts/audit_pilot_absence.py` (inline) | reviewed derivation | script | evidence QA | `case-studies/manlan-2019/pilot/absence-audit.json` | generated |
| ESnet assessment audit | v1 | `scripts/audit-esnet-assessment.py` (`audit_schema_version`) | reviewed derivation | script | evidence QA | `case-studies/manlan-esnet-2019/assessment-audit.json` | generated |
| Cross-observer matrix | v1 | `scripts/build-cross-observer-matrix.py` (inline) | reviewed derivation | script | comparisons | `case-studies/manlan-2019/pilot/cross-observer-matrix.json` | generated |
| API envelope | v1 (`api_version`) | `src/catalog/web/api.rs` (`API_VERSION`) | alpha; no long-term promise | web server | API clients | none | runtime |

## Unversioned formats (not falsely versioned)

| Format | Producer | Notes |
|---|---|---|
| `report.txt` | analysis pipeline | text rendering of the versioned report.json |
| `limitations.json` | analysis pipeline | written with the report; no separate version |
| `finding-audit.json` / `finding-chronology-audit.json` | `catalog finding-audit` / `finding-chronology-audit` | read-only derivations from lifecycle evidence; no version constant |
| `relationship-audit.json` | reviewed audit | reviewed artifact, no version constant |
| `pilot-result.json`, `network-profile.json`, `asn-identities.json`, `collector-locations.json`, `ticket-reviews.json`, `peer-metadata.json` | reviewed data | reviewed configuration, no version constant |
| `demo-manifest.json` | `demo init` | versioned as "demo manifest v1" above |
| `SHA256SUMS` (evaluation pack) | `scripts/build-evaluation-pack.sh` | integrity manifest, explicitly not a signature |
| `.sha256` sidecars (raw cache) | acquisition | per-archive integrity |

## Notes

- "Backwards compatibility" is claimed only where tests support it:
  manifest v2 is the only accepted manifest schema; catalog migrations
  are the only schema-evolution mechanism; report v2 artifacts remain
  readable as frozen legacy examples while v3 is current.
- The matrix is checked against implementation constants by
  `scripts/audit-docs.py` (schema-matrix check) and against tracked
  artifacts by the artifact-set check.
- Full semantics of each format live in the Rust types and the
  normative documents; this matrix is a version index, not a schema
  replacement.
