# Session 34 — Reviewed multi-observer analysis

HEAD baseline: 2331b54 + preflight fix 17936f2 (incident-neutrality gate).
Goal: convert the Session 33 corpus into one reviewed, multi-source
operational analysis (RouteViews + RIPE RIS), with a comparison layer that
never merges evidence or claims global confirmation.

## Constraints (from the session brief)

- No broad corpus crawl; no bulk-access request sent; no HTTP GET starts
  sync/analysis; no new lifecycle categories; no causal scoring; no new DB
  layer (extend corpus.sqlite); no broad web redesign.
- Source tickets remain immutable snapshots; reviewed roles never overwrite
  source task types; TASK0038206/TASK0038211 stay unresolved references.
- RIS runs are independent AnalysisRuns; no merged verdict across
  collectors; forbidden phrasings: "globally confirmed", "complete outage",
  "traffic loss confirmed", "operator action confirmed".

## Design decisions

- **D1 (Part 1)** New table `ticket_reviews` in corpus.sqlite (migration V7):
  `id, catalog_event_id UNIQUE NOT NULL, external_id, reviewed_roles_json,
  entity_labels_json, linked_change_ids_json, analysis_applicability,
  applicability_rationale, relationship_to_case_study, review_status,
  reviewer, reviewed_at, provenance_json, source_document_id`. Roles
  vocabulary (fixed): ChangeWindow, PrimaryIncident, ParticipantImpact,
  AlarmOrTelemetry, RollbackOrRecovery, OperationalTask, Other. Provenance
  JSON is per-field: every interpretation either cites the source snapshot
  field or the AAR (source_document_id + section) — a missing source field
  stays missing without cited provenance. Reviewed data file:
  `case-studies/manlan-2019/pilot/ticket-reviews.json`, imported by CLI
  `inim catalog corpus review <file>`. Never touches event_snapshots.
- **D2 (Part 2)** New relationship kinds (constants in domain.rs, stored as
  strings): `RollbackFor`, `ParticipantImpactDuring`, `AlarmDuring`,
  `OperationalTaskDuring`. Reviewed edges use evidence_kind
  `AnalystReviewed` (note cites exact source wording/AAR section) or
  `ReferenceDocument` (AAR). `extract_relationships_from_snapshots` is
  untouched — reviewed edges are inserted only by the review importer;
  existing idempotency (never overwrite reviewed edges) preserved. Edges to
  TASK0038206/TASK0038211 keep `to_event_id NULL` (unresolved document
  references; no snapshots manufactured). Graph audit: CLI `inim catalog
  relationships audit` + web page listing source node, dest node, kind,
  evidence kind, exact source, review status.
- **D3 (Part 3)** New confidence `TemporalCoincidence`. Grouping becomes
  PER-UNORDERED-PAIR generation: for each pair with ≥1 supporting signal,
  the evidence list is the union of ALL signals (explicit text, shared
  case study, shared reviewed entity label, shared normalized asset,
  shared maintenance/change id, temporal overlap) and the category is the
  best applicable (ExplicitlyLinked > StrongCandidate > WeakCandidate >
  TemporalCoincidence) — one candidate per pair, never a duplicate pair.
  Before inserting a pair's merged candidate, suppress Unreviewed derived
  rows for the SAME pair whose signal set is a strict subset (their
  evidence survives inside the merged row — provenance is preserved, not
  deleted). Rejected rows are never touched: a rejected fingerprint stays
  suppressed until the merged fingerprint changes (new evidence).
  TemporalCoincidence (overlap alone) is hidden from the default
  incident-candidates view, still queryable via `?include=temporal`.
  `store::insert_group_candidate` becomes an upsert: on fingerprint
  conflict, reclassify only Unreviewed non-Rejected derived rows
  (confidence+label update). Existing corpus rows reclassify on regen.
  NOTE: existing test `temporal_overlap_alone_remains_weak` is updated to
  assert TemporalCoincidence (the brief supersedes it).
- **D4 (Part 4)** `Manifest` gains `#[serde(default)] source_family: String`
  ("RouteViews" | "RipeRis"); add `SourceFamily::from_str` (tolerant).
  orchestrate.rs replaces the literal `"routeviews"` broker project with
  `family.broker_project()` (2 sites) and the "No selected RouteViews
  observer" limitation becomes family-labeled. derived_cache.rs adds the
  family into `rib_cache_key` and `update_cache_key` inputs (structural
  identity: (family, collector) — collision impossible). output.rs
  limitation text becomes family-labeled. Report naming: family label
  ("RIPE RIS" vs "RouteViews") flows into report limitations. New real
  fixture: one small 2019 RIS updates file under tests/fixtures/ris/
  (provenance documented in tests/fixtures/README.md).
- **D5 (Part 5)** Reviewed candidate collector list for Aug 2019
  (rrc00, rrc01, rrc03-rrc07, rrc10-rrc16, rrc20-rrc24). Live metadata
  probe + RIB preflight per candidate. RIS bviews sit on the 8-hour
  00/08/16 grid: for warmup_start 02:00 UTC the pre-window baseline is
  `bview.20190821.0000.gz` (NOT a 0200 stamp — that is the RouteViews
  convention). Report per candidate: baseline RIB, AS2603 origin routes,
  ContainsAny[11537] routes, observer-prefix stream count, peer count,
  estimated update count/volume. Selection: small reviewed set by
  qualifying visibility, peer diversity, geographic diversity, volume,
  completeness. Rejected collectors recorded with reasons in
  `case-studies/manlan-2019/pilot/ris-collector-selection.md`.
- **D6 (Part 6)** One manifest per selected RIS collector
  (`MANLAN-2019-NORDUNET-PILOT-RIS-<rrc>.json`): identical target
  (NORDUnet AS2603, ContainsAny[11537]) and window (16:00–17:30 UTC,
  warmup 840 min, cooldown 60 min) as the RouteViews pilot; collectors
  `[rrcXX]`, source_family RipeRis. `inim analyze` per collector →
  independent AnalysisRun; import into catalog, THEN link each run to the
  case study via the existing `inim catalog case-study link-run` CLI
  (comparison page and Part 7 depend on the links); per-collector report.
- **D7 (Part 7)** New `src/catalog/observer_compare.rs`: per normalized
  prefix × collector rows (first observed change, temporary absence, path
  replacement, transit departure, restoration, evidence availability) over
  imported run artifacts; cross-observer statements from the brief's
  vocabulary; no global-confirmation phrasing. 6 required tests.
- **D8 (Part 8)** Batch planner exercised with the RouteViews + RIS run
  definitions; raw archives reused via cache_archive sidecar dedup; derived
  cache reuse only when cohort/cache identity permits; report actual reuse
  metrics. 4 required tests.
- **D9/D10** case_study.html gains "Related public tickets" + "Public-BGP
  observer comparison" sections with the brief's exact conclusion wording;
  analysis_queue.html rows gain reviewed role, readiness detail, archive
  plan status, existing runs, next analyst action (derived, never executed
  from GET).
- **D11** GRNOC_BULK_ACCESS_REQUEST.md restructured: concise email +
  technical appendix + clearly marked user-fill section (contact email,
  repository URL, affiliation wording). Not sent.
- **D12** screenshot-review.sh extended with corpus dashboard, relationship
  graph, analysis queue, comparison, case-study page, mobile case-study.
- **D13** docs updated per brief (README, DESIGN, DOMAIN, DATA_PROVENANCE,
  OBSERVABILITY, ADR RIPE-RIS-SUPPORT, ADR CASE-STUDY-LAYER,
  GRNOC_PUBLIC_TASK_VIEWER).
- **D14** Full quality gates: fmt, cargo test (debug+release), clippy -D
  warnings, cargo deny licenses+bans, cargo package. No publish/tag/push.

## Task order (each with verification gate)

1. Migration V7 (ticket_reviews) + domain types + store/load + CLI review
   import + ticket-reviews.json for the ten tickets (roles, labels,
   provenance citing snapshot fields and AAR sections). Tests (5 required).
2. Relationship kinds + reviewed-edge import (in ticket-reviews.json or
   sibling reviewed-relationships section) + audit CLI/web. Tests (5).
3. Grouping reclassification + upsert + TemporalCoincidence hiding +
   regen on live corpus + web/API changes. Tests (6).
4. Manifest family + orchestrator family threading + cache-key family +
   output labeling + RIS fixture + tests (8) + `archive-batches plan
   --family` support.
5. RIS collector metadata probe + RIB preflight + selection report.
6. RIS manifests + per-collector runs + import + per-collector reports.
7. observer_compare module + tests (6) + comparison report for the pilot
   runs.
8. Batch exercise metrics + tests (4).
9. case_study.html + view.rs updates (related tickets, comparison,
   conclusion wording).
10. analysis_queue rows (reviewed role, readiness, plan status, runs, next
    action) + template.
11. GRNOC_BULK_ACCESS_REQUEST.md restructure.
12. Screenshot harness extension + capture.
13. Docs updates.
14. Quality gates + completion report.

## Required test names (from the brief — gate: every one exists and passes)

- Part 1: source_task_type_and_reviewed_role_are_distinct,
  aar_enrichment_requires_aar_provenance,
  reviewed_interpretation_does_not_modify_source_snapshot,
  one_ticket_can_have_multiple_supported_case_study_roles,
  missing_source_field_is_not_filled_without_provenance
- Part 2: reviewed_relationship_retains_all_supporting_sources,
  explicit_reference_has_precedence_over_temporal_candidate,
  unavailable_ticket_remains_unresolved_reference,
  one_relationship_can_have_document_and_ticket_support,
  graph_does_not_replace_individual_ticket_history
- Part 3: temporal_overlap_only_is_temporal_coincidence,
  temporal_coincidence_is_hidden_by_default,
  shared_asset_plus_overlap_can_be_weak_candidate,
  explicit_reference_is_prominent,
  rejected_candidate_remains_suppressed_without_new_evidence,
  candidate_explanation_lists_every_supporting_signal
- Part 4: ris_bview_selects_latest_pre_window_baseline,
  ris_update_selection_covers_requested_window,
  routeviews_and_ris_archive_identity_cannot_collide,
  ris_observations_enter_shared_route_model,
  ris_report_names_ripe_ris_not_routeviews,
  mixed_source_artifact_order_is_deterministic,
  ris_cache_roundtrip_preserves_source_family,
  routeviews_behavior_remains_unchanged
- Part 7: comparison_distinguishes_no_visibility_from_no_change,
  same_prefix_at_multiple_collectors_remains_separate_evidence,
  timing_differences_are_preserved,
  multi_observer_agreement_is_not_global_confirmation,
  source_family_is_visible_in_comparison,
  comparison_is_deterministic
- Part 8: shared_raw_archive_can_feed_independent_runs,
  incompatible_cohorts_do_not_share_wrong_derived_cache,
  failed_run_does_not_invalidate_successful_batch_member,
  standalone_and_batched_run_artifacts_match

## Gates

- `cargo test` green after every part (debug + release).
- Incident-neutrality release test stays green (no new incident tokens in
  src/).
- Every required test name from the brief exists and passes (list above).
- Live network only in Parts 5–6 (explicitly allowed); default `cargo test`
  never touches the network.
