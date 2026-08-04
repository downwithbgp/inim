# Specification coverage matrix — 2026-08

Dated audit. Maps every current specification area to its normative
document, implementation authority, test authority, and schema/artifact.
The matrix is a navigation aid for maintainers; the normative documents
themselves are authoritative. A row marked **gap** means behavior exists
with no documented contract — the gap is resolved by the referenced
follow-up, not by this matrix.

## Matrix

| Specification area | Normative document | Implementation module | Principal types | Tests | Schema/artifact | Status | Gaps | Duplicated descriptions |
|---|---|---|---|---|---|---|---|---|
| Product purpose and scope | `README.md`, `docs/STATUS.md` | — | — | — | — | current | none | README vs STATUS overlap is intentional (entry point vs status page) |
| Domain model (identities, units, transitions) | `docs/DOMAIN.md`, `docs/GLOSSARY.md` | `src/domain/`, `src/lifecycle.rs`, `src/cohort.rs` | `RouteKey`, `ObserverPrefixKey`, `RouteTransition`, `StreamCategory` | `tests/route_identity_test.rs`, `tests/lifecycle_reconstruction_test.rs`, `src/lifecycle.rs` unit tests | route/lifecycle artifacts (v1) | current | none | GLOSSARY restates DOMAIN terms by design (term authority) |
| Architecture | `docs/DESIGN.md` | all modules | — | architecture-level integration tests | — | current | none | README summarizes DESIGN (intentional) |
| Observability limits | `docs/OBSERVABILITY.md` | `src/observability.rs` | mechanism hint types | `tests/observability_test.rs` | report limitations section | current | none | OBSERVABILITY restates some GLOSSARY distinctions (intentional) |
| Data provenance | `docs/DATA_PROVENANCE.md` | `src/schema.rs`, `src/catalog/store.rs` | schema constants, artifact rows | provenance tests | all versioned artifacts | current | none | none |
| Project scope | `config/project-scope.toml`, `docs/GLOSSARY.md` (Project scope), `docs/DOMAIN.md` (Project scope) | `src/catalog/scope.rs`, `src/catalog/jobs/plan.rs` | `ScopeConfig`, `ProjectScope` | `tests/project_scope_policy_test.rs`, `tests/project_scope_enforcement_test.rs`, `tests/second_network_semantics_test.rs` | `config/project-scope.toml` (schema v1) | current | none | scope semantics repeated in DESIGN, DOMAIN, GLOSSARY, OPERATIONS — normative source is `config/project-scope.toml` + GLOSSARY; other docs summarize and link |
| Network profiles | `docs/DESIGN.md` (Plane-aware analysis), `docs/DOMAIN.md` (Reviewed service-plane model) | `src/profiles/`, `src/catalog/netprofile.rs`, `src/conventions/` | `NamedServicePlane`, `ReviewedAsnRole`, `SessionRelationship` | `src/catalog/netprofile.rs` unit tests | `case-studies/*/pilot/network-profile.json` | current | none | none |
| Event interpretation | `docs/DOMAIN.md` (TicketReview), `docs/DATA_PROVENANCE.md` (Reviewed ticket interpretation) | `src/catalog/review.rs`, `src/catalog/relationships.rs` | `TicketReview`, `ReviewProvenance`, `TicketRelationship` | `tests/*review*` | `ticket_reviews` (V7) | current | none | none |
| Plan readiness | `docs/OPERATIONS.md`, `docs/DESIGN.md` (planning) | `src/plan.rs`, `src/catalog/analyzability.rs` | `AnalysisPlanStatus`, `Analyzability` | `src/plan.rs` unit tests, blocked-plan tests | `analysis_plan.json` (schema v1), `analysis_plans` table | current | none | readiness vocabulary in GLOSSARY + DOMAIN (Analyzability) |
| Route-selection semantics | `docs/DOMAIN.md` (TransitPredicate), `docs/GLOSSARY.md` (Transit predicate) | `src/plan.rs`, `src/manifest.rs` | `TransitPredicate` (ContainsAny/ContainsAll/Adjacent), `TransitPredicateMapping` | predicate unit tests, manifest migration tests | manifest (schema v2) | current | path normalization details (AS sets, confederation, prepending) are implemented in `src/ingest/`/`src/domain/path.rs` but not separately documented — see the reference note in `docs/DOMAIN.md` | predicate semantics repeated in DOMAIN + GLOSSARY (term authority) |
| Observer eligibility | `docs/OBSERVABILITY.md` (coverage reasons), `docs/DESIGN.md` (coverage reasons) | `src/catalog/workbench.rs`, `src/cohort.rs` | `CoverageStatus`, `CoverageReason` | workbench tests | stream lifecycle summaries | current | none | coverage vocabulary in GLOSSARY + OBSERVABILITY + DESIGN (intentional) |
| Route and stream identity | `docs/DOMAIN.md` (Identity model), `docs/GLOSSARY.md` (Routing identities) | `src/domain/` (RouteKey), `src/cohort.rs` (ObserverPrefixKey) | `RouteKey`, `ObserverPrefixKey`, `path_id` | `tests/route_identity_test.rs`, ADD-PATH tests | observation schema v2, caches v2 | current | none | none |
| Lifecycle reconstruction | `docs/DOMAIN.md` (AnalysisPhase, Stream lifecycle), `docs/DESIGN.md` (Historical validation) | `src/lifecycle.rs` | `StreamLifecycle`, `AnalysisPhase`, restoration kinds | `tests/lifecycle_reconstruction_test.rs` | lifecycle.json (v1) | current | none | none |
| Findings | `docs/UX.md` (RoutingFinding), `docs/DESIGN.md` (RoutingFinding derivation) | `src/catalog/workbench.rs`, `src/catalog/finding_audit` | `RoutingFinding`, `ObserverEpisode`, `EffectKind`, `EndState` | workbench + finding-audit tests | `finding-audit.json`, `finding-chronology-audit.json` | current | none | none |
| Restoration classes | `docs/GLOSSARY.md` (Restoration), `docs/DOMAIN.md` (Restoration kinds) | `src/lifecycle.rs` | restoration kinds | lifecycle tests | lifecycle.json | current | none | none |
| Observed results and expectation assessment | `docs/DOMAIN.md` (Verdict, AnalysisOutcome), `docs/DESIGN.md` (Presentation semantics) | `src/outcome.rs`, `src/assess.rs` | `Verdict`, `AnalysisOutcome`, result/assessment fields | `src/outcome.rs` tests, report tests | report.json (schema v3 current; v2 frozen legacy) | current | none | result vs assessment distinction also in GLOSSARY (No route-state change) |
| Job state machine | `docs/GLOSSARY.md` (Analysis job), `docs/OPERATIONS.md` (Execution, Failure model) | `src/catalog/jobs/` (mod.rs, service.rs) | `JobState`, legal transitions | `src/catalog/jobs/mod.rs` unit tests, `tests/job_workflow_tests` | `analysis_jobs`, `analysis_job_events` (V10) | current | none | transitions table is implicit in `legal_transition`; documented in GLOSSARY/OPERATIONS prose |
| Worker leases | `docs/GLOSSARY.md` (Worker lease), `docs/OPERATIONS.md` (Execution) | `src/catalog/jobs/service.rs`, `src/worker.rs` | lease/heartbeat config | worker tests | `worker_heartbeats` (V10) | current | none | none |
| Staging and publication | `docs/GLOSSARY.md` (Staging artifact), `docs/OPERATIONS.md` (Execution) | `src/catalog/jobs/publish.rs`, `src/worker.rs` | staging/run paths | publication tests | `data/jobs/<id>/staging`, `data/runs/<id>/` | current | none | none |
| Catalog behavior | `docs/DATA_PROVENANCE.md` (Local event catalog), `docs/OPERATIONS.md` | `src/catalog/` | catalog tables | catalog tests | catalog schema v10 | current | none | `docs/reference/CATALOG-SCHEMA.md` is the reference |
| API behavior | `docs/reference/API.md` | `src/catalog/web/api.rs`, `handlers.rs`, `job_handlers.rs` | envelope types | `src/catalog/web/tests.rs` | API v1 | current | none | none |
| CLI behavior | `docs/reference/CLI.md` | `src/main.rs` | clap command tree | CLI integration tests | — | current | none | README CLI overview summarizes CLI.md |
| Web routes | `docs/reference/WEB-ROUTES.md` | `src/catalog/web/mod.rs` | router | web tests | — | current | none | none |
| Demo behavior | `docs/OPERATIONS.md` (Demo corpus boundary), `README.md` (Quick start) | `src/catalog/demo.rs` | demo import | `tests/demo_test.rs` | demo-manifest.json (v1) | current | none | none |
| Evaluation freeze | `docs/evaluation/ALPHA-FREEZE.md` | — | — | `scripts/audit_docs.py` guards | — | active | none | freeze referenced from CONTRIBUTING + PR template (intentional) |
| Source adapters (GRNOC) | `docs/sources/GRNOC_PUBLIC_TASK_VIEWER.md` | `src/catalog/grnoc.rs`, `grnoc_viewer.rs`, `access.rs` | `EventCatalogSource` | fixture-based sync tests | `event_snapshots`, `snapshot_fetches` (V4) | current | none | none |
| Source families (RouteViews/RIS) | `docs/DESIGN.md` (Observer families), `docs/OBSERVABILITY.md` (Observer sources) | `src/discover.rs`, `src/orchestrate.rs`, `src/sources/` | `SourceFamily` | RIS fixture tests | derived caches v2 | current | RIS archive cadence numbers are not asserted by tests (they come from broker metadata) — documented as source-contract, not promise | none |

## Required checks satisfied

- specification_coverage_matrix_lists_every_normative_area — every area
  enumerated by the 2026-08 documentation audit appears above.
- no_specification_area_has_no_normative_document — every row has a
  normative document column.
- duplicated_normative_text_identified — the "Duplicated descriptions"
  column records intentional restatements; see
  `docs/audits/2026-08-documentation-spec-conformance.md` for what was
  consolidated.

## Notes

- This matrix does not claim formal verification. Test authority
  columns name the primary suites; exhaustive test enumeration lives in
  the test sources.
- Schema versions listed here are current at this audit; the checked
  registry is `docs/reference/SCHEMA-VERSIONS.md`.
