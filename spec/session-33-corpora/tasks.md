# Session 33 — tasks: GRNOC event corpus + BGP-correlation workspace

## T1. Part 1 — Protocol audit + fixtures. Gate: /review.
- docs/sources/GRNOC_PUBLIC_TASK_VIEWER.md (observation date 2026-08-01;
  methods: curl probes + JS bundle analysis + JSON API probes).
- Fixtures tests/fixtures/grnoc/viewer/*.json from real public responses
  (INC0227937, INC0301970, CHG0038258, malformed); provenance-documented.
- Extend `GnocRecord` parser: unix-epoch fields (work_start/work_end/
  opened_at/start_date/end_date), state/priority code maps, category,
  u_maintenance_type, u_outgoing_notification_text; keep legacy aliases.
- Required tests: viewer_response_fixture_parses,
  missing_optional_field_is_preserved_as_absent,
  unknown_source_fields_do_not_corrupt_normalized_event,
  malformed_response_produces_item_scoped_failure.
- Gate: T, F, C. Commit.

## T2. Part 2 — Conservative access policy. Gate: /review.
- src/catalog/access.rs: AccessPolicy {max_concurrency=1,
  requests_per_second=0.25, burst=1, max_requests=100}; User-Agent
  (inim version + purpose + optional contact); conditional requests
  (If-None-Match / If-Modified-Since); Retry-After; exponential backoff
  with bounded jitter; stop conditions (repeated 429/403, robots
  prohibition, unexpected auth, schema incompatibility); permanent 404
  not retried; budget exhaustion pauses cleanly (frontier persists in
  ticket_discoveries; budget state is per-run in memory — resume = next
  run continues the persisted frontier).
- Validators are best-effort: the live API sends no ETag/Last-Modified,
  so missing validators degrade to full fetch + content-SHA dedup (the
  existing sync semantics); that is not an error. Tests use a local mock
  HTTP server (no live network in default suite).
- State/priority "code maps" are LOSSLESS label translations (raw code →
  stable label); never a computed priority ordering or severity score.
- Required tests: default_rate_is_conservative,
  concurrency_never_exceeds_configured_limit,
  request_budget_stops_sync_cleanly, retry_after_is_honored,
  etag_generates_conditional_request,
  not_modified_does_not_create_snapshot,
  repeated_forbidden_response_stops_sync,
  permanent_not_found_is_not_retried_indefinitely, jitter_is_bounded,
  sync_can_resume_after_budget_exhaustion.
- Gate: T, F, C. Commit.

## T3. Part 3 — Discovery modes + provenance. Gate: /review.
- src/catalog/discovery.rs: DiscoveryProvenance (AnalystSeed,
  DocumentReference, TicketDescriptionReference, PublicSearchResult,
  CaseStudyReference, OtherReviewedSource); modes: seed list, reference
  expansion, public search/list; frontier; budget applies to expansion.
- Migration v4: ticket_discoveries table (external_id, provenance,
  source snapshot/document, discovered_at, status).
- No blind numeric enumeration anywhere.
- Required tests: seed_discovery_records_provenance,
  document_reference_is_not_a_source_snapshot,
  description_reference_enters_fetch_frontier,
  duplicate_discoveries_merge_provenance,
  no_default_numeric_enumeration_exists,
  request_budget_applies_to_reference_expansion.
- Gate: T, F, C. Commit.

## T4. Part 4 — Source-fetch provenance. Gate: /review.
- Migration v4 (same transaction): new table `snapshot_fetches` — one row
  PER FETCH attempt: event_id, sync_run_id, fetched_at, source_url,
  http_status, content_type, etag, last_modified, acquisition_method,
  retry_count, snapshot_id (NULL when the fetch produced no new content,
  e.g. 304), conditional_requested. `event_snapshots` stays pure
  content-addressed immutability (UNIQUE event_id+sha256 preserved);
  fetch metadata is per-fetch, never mutated on the snapshot row.
- Conditional 304 inserts a snapshot_fetches row with http_status 304 and
  NO new snapshot; changed payload creates a new immutable snapshot + a
  linked fetch row; old snapshot stays linked to historical runs (runs
  link via manifest_revisions.snapshot_id — unchanged).
- Required tests: conditional_not_modified_preserves_existing_snapshot,
  changed_etag_and_payload_create_new_snapshot,
  fetch_metadata_does_not_include_sensitive_headers,
  source_payload_hash_is_reproducible,
  old_snapshot_remains_linked_to_historical_run.
- Gate: T, F, C. Commit.

## T5. Parts 5 + 12-prelude — Live GRNOC viewer adapter + MAN LAN retrieval.
- src/catalog/grnoc_viewer.rs: polite live adapter (EventCatalogSource)
  using /api/get_incidents + /api/get_change_requests; TASK unsupported
  (records remain unresolved); domain list for scoped search only.
- Search constraint: the adapter NEVER issues empty/broad queries;
  search requires an explicit number (exact lookup) or an explicit
  reviewed domain + query string. Audit doc records the exact SPA
  request shape and the 403-on-unscoped-incidents observation.
- Case-study seed import (12 AAR IDs); retrieved tickets link existing
  document references; source timing vs AAR timing kept distinct.
- Required tests: all_manlan_seed_ids_are_requested_once,
  retrieved_ticket_links_existing_document_reference,
  source_timing_and_aar_timing_remain_distinct,
  missing_public_ticket_remains_unresolved,
  case_study_link_does_not_depend_on_title_matching.
- Gate: T, F, C. Commit.

## T6. Parts 6 + 7 — Reference extraction + relationship graph.
- src/catalog/references.rs: conservative INC/CHG/TASK regex; exact
  source span; neutral relationships (References,
  TracksRemainingImpactIn, SupersededBy, RelatedChange, RelatedIncident,
  RelatedTask, UnknownReference).
- Migration v4: ticket_relationships (from_event, to_event nullable /
  to_external, kind, evidence_kind, source_snapshot, source_document,
  reviewed_status, note); evidence kinds explicit vs derived; bounded
  adjacency traversal.
- Required tests: explicit_ticket_reference_is_extracted,
  exact_source_span_is_preserved,
  tracking_language_supports_tracks_remaining_impact,
  bare_identifier_defaults_to_references,
  numeric_similarity_creates_no_edge,
  relationships_retain_snapshot_provenance;
  explicit_and_derived_edges_are_distinct,
  unresolved_edge_can_later_link_to_catalog_event,
  relationship_import_is_idempotent,
  conflicting_reviewed_relationship_is_not_overwritten,
  temporal_overlap_does_not_become_causal_edge,
  graph_traversal_is_bounded.
- Gate: T, F, C. Commit.

## T7. Parts 8 + 9 — Analyzability queue + candidate grouping.
- src/catalog/analyzability.rs: readiness states (NotReviewed …
  AnalysisFailed); separate from lifecycle/sync/BGP verdict; reasons.
- src/catalog/grouping.rs: IncidentGroupCandidate (members, evidence,
  confidence ExplicitlyLinked/StrongCandidate/WeakCandidate/Rejected,
  review status); no auto-merge; rejection persists until new evidence.
- Required tests (12): acquired_ticket_without_review_is_not_ready,
  reviewed_mapping_without_predicate_needs_predicate,
  non_bgp_service_can_be_marked_not_applicable,
  completed_analysis_is_distinct_from_ticket_closed_state,
  changed_snapshot_marks_analysis_stale,
  inferred_entity_candidate_is_not_reviewed_mapping;
  explicit_reference_creates_strong_group_candidate,
  shared_document_supports_group_candidate,
  temporal_overlap_alone_remains_weak, analyst_can_reject_candidate,
  rejected_candidate_is_not_regenerated_without_new_evidence,
  group_does_not_replace_individual_events.
- Gate: T, F, C. Commit.

## T8. Parts 10 + 11 — RIS inventory + shared archive planning.
- Audit bgpkit-broker source families; source family field
  (RouteViews/RipeRis) in planner; RIS URL/cadence support; ADR if full
  RIS support needs new behavior; Ready/Unsupported status.
- src/catalog/batch.rs: CorrelationBatch (group by family/collector/
  URL/horizon; unique RIB + UPDATE sets; consumers; archives avoided;
  estimated bytes; parse operations; deterministic; evidence identity
  independent of batch membership).
- Required tests (11): routeviews_and_ris_collectors_have_distinct_identity,
  ris_archive_plan_uses_correct_cadence,
  source_family_appears_in_observer_scope,
  report_does_not_call_ris_observer_routeviews,
  mixed_source_plan_is_deterministic;
  overlapping_events_share_raw_archive_plan,
  nonoverlapping_events_do_not_share_unneeded_archives,
  archive_reuse_does_not_merge_event_evidence,
  event_results_are_identical_batched_or_standalone,
  evidence_ids_do_not_depend_on_batch_membership,
  batch_plan_is_deterministic.
- Gate: T, F, C. Commit.

## T9. Part 12 + 18 — Bounded live pilot + MAN LAN validation.
- Pilot: 12 MAN LAN IDs, policy defaults, record request/status counts,
  elapsed, rate, bytes, new/changed/unchanged events, references.
- Validation: 12 AAR refs; retrieved/unresolved counts; explicit
  cross-ticket refs; source-vs-AAR timing diffs; roles; group evidence;
  analyzability states; NORDUnet pilot linkage.
- Optional second pilot (≤100) only if terms/robots/errors allow; record
  decision. No aggressive retries.
- Gate: pilot log + validation report. Commit.

## T10. Parts 13 + 14 — Corpus web pages + API.
- Parts 13 + 14 — Corpus web pages + API. New top-level routes /corpus,
  /analysis-queue, /incident-candidates, /archive-batches sit alongside
  the existing top-level /events and /case-studies routes (consistent
  with the current flat route layout rather than nesting under /catalog).
- Routes: /corpus, /corpus/sync-runs, /events/{id}/relationships,
  /analysis-queue, /incident-candidates, /archive-batches; event detail
  additions; no crawling/analysis on GET.
- API: /api/v1/corpus/status, /api/v1/corpus/sync-runs,
  /api/v1/events/{id}/relationships, /api/v1/analysis-queue,
  /api/v1/incident-candidates, /api/v1/archive-batches (envelope +
  pagination; no cookies/raw headers/absolute paths/unreviewed truth).
- Gate: web tests green; T, F, C. Commit.

## T11. Part 15 + 17 — CLI + export policy.
- inim catalog sync grnoc --seed/--case-study/--expand-references/
  --max-requests/--requests-per-second/--dry-run/--show-access-policy;
  relationships rebuild; analysis-queue; archive-batches plan;
  corpus export-metadata (metadata-only default; no raw payloads).
- No "download everything" default.
- Gate: CLI tests; T, F, C. Commit.

## T12. Part 16 + 19 + 20 — Bulk draft, screenshots, docs.
- docs/sources/GRNOC_BULK_ACCESS_REQUEST.md (created, NOT sent).
- Screenshots: corpus dashboard, MAN LAN relationships, analysis queue,
  incident candidates, archive-batch plan.
- Docs: README.md, DESIGN, DOMAIN, DATA_PROVENANCE, OBSERVABILITY,
  LOCAL-CATALOG-AND-WEB ADR, CASE-STUDY-LAYER ADR, RELEASING.md.
- Gate: docs reviewed; screenshots exist. Commit.

## T13. Part 21 — Quality gates + completion report + memory.
- cargo fmt --check; cargo test; cargo test --release; clippy -D
  warnings; cargo deny licenses; cargo deny bans; cargo package.
- Confirm: no live-network test in default suite; no crawl on GET; no
  numeric enumeration; rate/budget enforced; no corpus in crate (fixture-
  scale, provenance-documented public responses in tests/fixtures are
  explicitly in scope; "no corpus" means no bulk corpus); no verdict
  from ticket text alone; MIT intact.
- Report exact HEAD/commits/test count + completion report. Memory.
