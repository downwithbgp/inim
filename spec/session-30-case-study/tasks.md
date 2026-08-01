# Session 30 — tasks

Gate legend: T = `cargo test` (new tests listed), F = `cargo fmt --check`,
C = `cargo clippy --all-targets --all-features -- -D warnings`.

## T1. Spec + design (this directory) — gate: /review pass on spec

## T2. Schema + domain + association semantics (Parts 1, 2, 16)
- migration V2 (11 tables), domain types (neutral names), store inserts,
  delete-rejection FK.
- Tests: case_study_can_link_multiple_catalog_events,
  one_event_can_participate_in_multiple_case_studies,
  case_study_can_link_multiple_analysis_runs,
  analysis_evidence_remains_owned_by_analysis_run,
  deleting_referenced_event_is_rejected,
  case_study_phase_requires_source_provenance,
  case_study_order_is_deterministic,
  document_reference_does_not_fabricate_ticket_snapshot,
  missing_historical_ticket_can_remain_document_referenced,
  independently_retrieved_ticket_has_separate_provenance,
  relationship_type_is_not_inferred_from_ticket_number_prefix_alone.
- Gate: T, F, C. Commit.

## T3. Transitions artifact + run_transitions import (Part 9 foundation)
- write_transitions in output.rs (schema const), import parses
  transitions.json into run_transitions.
- Tests: transitions artifact roundtrip; import populates rows; conflict on
  (run_id, seq).
- Gate: T, F, C. Commit.

## T4. Reference-document import (Part 3)
- `catalog document import` (sha, media allowlist, page count/metadata
  best-effort, catalog-relative path under data/documents/, dedup by sha,
  new revision on change).
- Tests: document_import_calculates_sha256,
  identical_document_import_is_idempotent,
  changed_document_creates_distinct_record,
  document_path_is_catalog_relative,
  document_is_not_in_crate_package, unsupported_document_type_fails_cleanly.
- Gate: T, F, C. Commit.

## T5. Case-study import (Parts 4-6 validations, 15)
- Depends on T2's schema (document-link FKs) and T4's document records.
- case-study.json parser + validation (phases need source provenance,
  UTC times, claim observability explicit), transactional upsert with
  immutability (slug+content sha).
- Tests: case_study_import_is_idempotent,
  case_study_import_links_existing_event,
  case_study_import_preserves_unresolved_ticket_reference,
  invalid_phase_provenance_rejects_import,
  conflicting_immutable_case_study_revision_is_rejected,
  phase_times_are_utc, phase_ranges_do_not_overlap_unintentionally,
  phase_provenance_identifies_document_section,
  retrospective_belief_is_not_rendered_as_measured_fact,
  case_study_timeline_is_deterministic, claim_observability_is_explicit,
  not_directly_visible_claim_is_not_reported_as_bgp_absence,
  indirect_visibility_uses_cautious_language,
  observed_bgp_change_does_not_prove_reported_mechanism,
  no_bgp_change_does_not_refute_l2_incident.
- Gate: T, F, C. Commit.

## T6. Archive planner (Part 8)
- `catalog case-study plan`: horizon (warmup 2 h / incident / cooldown ≥ 2 h),
  expected files, sizes best-effort, collectors, coverage, blocked targets +
  reasons, status Draft. No downloads.
- Tests: plan computes expected files without network; unresearched target is
  blocked with reason; horizon boundaries recorded; no download attempted.
- Gate: T, F, C. Commit.

## T7. Phase-conditioned summaries (Part 9)
- read-only derivation from run_transitions + lifecycle + wave summaries;
  continuous state across phases.
- Tests: phase_summary_uses_continuous_run_state,
  lifecycle_crossing_phase_boundary_is_not_split_incorrectly,
  phase_counts_are_observer_stream_counts,
  same_transition_is_not_double_counted,
  phase_without_bgp_changes_remains_valid,
  phase_summary_retains_evidence_links.
- Gate: T, F, C. Commit.

## T8. Comparison model (Part 10)
- Tests: comparison_preserves_operator_and_bgp_sources,
  temporal_overlap_is_not_causal_confirmation,
  nonobservable_claim_has_no_false_negative,
  comparison_can_show_no_observed_counterpart,
  multiple_analysis_runs_can_contribute_to_one_phase.
- Gate: T, F, C. Commit.

## T9. Web pages + API + document serving (Parts 11-13)
- Routes, templates (list/detail), API endpoints, /documents/:id serving.
- Tests: case_study_page_separates_reported_and_observed,
  case_study_page_shows_document_provenance,
  case_study_page_shows_related_ticket_roles,
  unresolved_target_research_is_visible,
  no_analysis_case_study_has_no_bgp_verdict,
  nonobservable_conditions_are_not_shown_as_missed_detections,
  document_route_rejects_path_traversal,
  document_route_does_not_expose_absolute_path,
  missing_document_file_is_reported_cleanly,
  hash_mismatch_is_reported,
  unapproved_media_type_is_not_served_inline, API envelope/pagination tests.
- Gate: T, F, C. Commit.

## T10. MAN LAN data + demo import (Parts 4-7, 14)
- case-studies/manlan-2019/case-study.json + README.md; import into
  data/inim.sqlite; document record from metadata (local_path NULL); plan
  command produces Draft plan.
- Gate: T, F, C. Commit.

## T11. Documentation + ADR (Part 17)
- README, DESIGN, DOMAIN, DATA_PROVENANCE, OBSERVABILITY, new
  docs/ADRs/CASE-STUDY-LAYER.md, update LOCAL-CATALOG-AND-WEB.md.
- Gate: F, C. Commit.

## T12. Quality gates + package + completion report (Part 19)
- fmt, test (debug+release), clippy all targets/features, deny licenses/bans,
  cargo package. release_test additions: case-study metadata packaged, PDF
  not packaged, neutrality scan of src/.
- Confirm: no absolute local paths exposed; no HTTP request triggers
  historical analysis; MIT intact. No publish/tag/push.
