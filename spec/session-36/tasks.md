# Session 36 — NOC incident workbench and historical RRC11/I2PX audit

Starting HEAD: `1cc9120` (797 tests). Follows the user's 17-part brief; each
part's required tests are listed verbatim. All existing tests must stay green
(797 baseline).

## Hard constraints (from repo gates)

- `tests/release_test.rs::production_source_contains_no_internet2_specific_plane_branch`:
  - `11164` and `i2px`: **zero occurrences anywhere in src/** (production or tests).
  - `11537` and `internet2`: live hit set in src/ must EQUAL the frozen set.
    → **New src files must not contain any of: `11164`, `i2px`, `11537`, `internet2`.**
    Required tests whose NAMES contain `as11164` MUST live in `tests/`
    (integration tests), not in src/. Plane labels/ASNs enter the UI only via
    runtime data (network-profile.json, session-audit, collector-locations).
- No BGP parsing / archive reads on the HTTP request path (existing rule).
- No severity score, no causation claims, no traffic-loss claims.
- MIT licensing intact; no screenshots packaged; no publish/tag/push.

## Data facts (verified at session start)

- RRC11 2019-08-21 baseline already on disk:
  `cache/ris-preflight/rrc11/rib/bview.20190821.0000.gz` (+ sidecar sha).
- `session-audit-2019.json` already contains origin-scoped rrc11 rows (18):
  NO peer ASN 11164 among AS2603-origin sessions; all paths `neither_plane`.
  rrc11 preflight: 106 AS2603-origin routes, 0 transit-matching (ContainsAny
  [11537]). Current RIPE peer list says RRC11/NYIIX has direct peer AS11164 —
  that is CONTEXT, not 2019 evidence; the bview peer table is the evidence.
- Catalog DB (`data/inim.sqlite`) has 10 runs; manlan-2019 case study links
  runs 7-10 (RE plane runs: rrc00/rrc06/rrc15/route-views2) with role
  PilotObservation. RIS runs 4-6 are NOT linked (superseded by RE plane runs).
- `stream_lifecycle_summaries` has NO peer_asn column; peer ASN comes from
  reviewed data files (session-audit-2019.json) via `session_context.rs`.
- Web: axum + askama; `src/catalog/web/{mod,server,handlers,view,api}.rs`;
  templates in `src/catalog/web/templates/`; CSS inline in view.rs `APP_CSS`.
- Screenshot harness: `scripts/screenshot-review.sh` (playwright, viewports).

## Design decisions

### D1 — Full peer inventory for the RRC11 baseline (Part 1)
Add a peer-inventory capability to the session-audit machinery
(`src/catalog/session_audit.rs` + `netprofile.rs`):
- New `run_peer_inventory(opts)` that parses the RIB WITHOUT the origin
  filter (empty `origin_asn_filters` → all routes) and aggregates per
  (peer_ip, peer_asn, address_family): total route count, AS2603-origin
  route count, distinct AS2603 prefixes, path-class counts.
- The inventory parse MUST NOT write to the origin-scoped extraction
  cache (empty-origin keys would pollute the cache namespace): parse
  directly, aggregate in memory, cache nothing.
- CLI: `inim catalog session-audit --peer-inventory` (or a dedicated
  `--full-inventory` flag) writing a `PeerInventoryRow` JSON. The report
  explicitly labels itself "selected observers" scope, never "all RIS".
- Output artifact: `case-studies/manlan-2019/pilot/rrc11-audit-2019.json`
  + `.md` with: baseline bview timestamp, peer IP, peer ASN, address
  family, direct AS11164 session present/absent, routes received from
  AS11164 (any origin), AS2603-origin route count, distinct AS2603
  prefixes, path distribution, qualifying observer-prefix streams.
- Token discipline: the gate is CASE-INSENSITIVE (`to_lowercase`
  contains), so src/ identifiers must avoid `11164`, `i2px`, `11537`,
  `internet2` in ANY case. Part 1 tests go in `tests/rrc11_audit_test.rs`
  (token names allowed there).

### D2 — Direct I2PX pilot decision (Part 2)
Decision rule, data-driven (never a literal ASN in src/):
- Load the reviewed network profile (network-profile.json) at runtime;
  the decision "session with peer ASN ∈ plane ASNs for the I2PX plane"
  compares parsed peer ASNs against profile data.
- If the bview peer table contains a session whose peer ASN is the I2PX
  plane's reviewed ASN AND that session carries qualifying AS2603-origin
  routes → run the reviewed pilot window (2019-08-21 16:00–17:30Z)
  through RRC11 with a new manifest
  `MANLAN-2019-NORDUNET-PILOT-I2PX-RRC11.json` (pattern of the existing
  I2PX manifests), downloading rrc11 updates for the window only.
- Else produce the exact blocking reason report
  (`rrc11-i2px-audit-2019.json`/`.md`): "Direct I2PX session present, but
  no qualifying NORDUnet baseline visibility" or the accurate equivalent.
- A unit test seeds profile data with a synthetic I2PX plane ASN and
  asserts the decision function keys off the profile, not a literal.
- Never broaden the target; never merge with the R&E-plane runs.
- Expected outcome (from the origin-scoped audit): no direct AS11164
  session with AS2603 routes → report-only.

### D3 — ObserverEpisode presentation model (Parts 3, 4, 6, 9, 11, 12)
New generic module `src/catalog/workbench.rs` (token-free; generic plane
labels from data):
- `EffectKind`: `TemporaryStreamAbsence | RouteWithdrawal | PathReplacement
  | NamedPlaneDeparture | NamedPlaneReturn | PrependChange |
  MixedRouteChange | NoRouteStateChange` — presentation groupings ONLY,
  derived from existing `stream_lifecycle_summaries` fields (category,
  withdrawn, restored, transit_state), `run_transitions` kinds, and
  `semantic_wave_summaries` labels. No new route-transition semantics.
- `RelationshipKind`: `Direct | Indirect | Other | Ambiguous` (maps from
  netprofile `SessionRelationship`).
- `CoverageStatus`: `NoChange | NoBaselineVisibility | IncompleteCoverage`.
- `ObserverEpisode` struct per brief schema. Grouping: one episode per
  (run, observer session = collector+peer_ip, signature = effect kind +
  named plane); deterministic ordering (first change, region, collector,
  peer ASN). `named_path_plane` label comes from the manifest
  path_classifiers/transit_predicate + network-profile data at runtime.
- `peer_asn` is `Option<u32>` resolved from reviewed session-audit data
  (pilot); when unavailable, display peer IP with "peer ASN unreviewed".
- Sentence renderer `render_episode_sentence` (Part 4): precise verbs,
  collector-site vs peer-location separation, stream/prefix units, no
  traffic loss, no causation.
- `RegionObservationSummary` (Part 6): eligible/changed/unchanged/
  no-baseline/incomplete sessions, streams, prefixes, first change, last
  restoration — per observer-site region from collector-locations.json.
  Timestamp source for first_change/last_restoration is
  `run_transitions.occurred_utc` (plus stream summary restoration
  evidence); never interpolated between observations.
- Timeline model (Part 9): lanes per observer session; markers for
  analysis-window boundaries, operator anchors (case-study phases), first
  change, absence interval, path-change interval, restoration interval,
  unresolved end state. No interpolation; exact timestamps only.
- Investigation cues (Part 11): templates bound to observed facts
  (session, interval, prefixes, plane) — labeled "investigation cues".
- Shared by web workbench, text report CLI, JSON API (Part 12).

### D4 — Regions (Part 5)
Data-driven: add `region` ("AMER"/"EMEA"/"APAC"/"Unknown") and `multihop`
(bool, rrc00=true) fields to `collector-locations.json`; extend
`CollectorLocation`/`CollectorLocationRegistry` in netprofile.rs. Region
classifies the OBSERVER SITE only. Time scope: `as_of` already present;
registry lookup is time-scoped.

### D5 — Workbench UI (Parts 7, 8, 10)
- Routes: `GET /events/{event_id}/workbench` (event's own runs) and
  `GET /case-studies/{slug}/workbench` (linked runs; MAN LAN uses runs
  7-10). Same `IncidentWorkbenchViewModel`.
- NOC HCI: rectangular panels, square corners, thin borders, strong
  headers, compact line height, monospaced timestamps/AS paths, dense
  sortable tables, fixed headers, text status, restrained colors,
  underlined links, visible focus, keyboard accessible. Server-rendered;
  small progressive JS for sort/filter/expand/copy (no SPA).
- Prefix drill-down: episode expansion shows grouped prefixes with
  baseline path (lifecycle.json artifact), changed path/absence
  (transitions), first change, restoration, plane before/after, evidence
  refs. Never loads raw MRT.
- API: `/api/v1/events/{event_id}/workbench`,
  `/api/v1/analyses/{run_id}/observer-episodes`,
  `/api/v1/analyses/{run_id}/regional-breadth` (existing envelope, no
  absolute paths).

### D6 — Performance (Part 13)
- Workbench reads only: catalog tables, reviewed data files,
  immutable artifacts (lifecycle.json). No analysis/parse on request path.
- Measure per request: SQL query count, DB time, model time, render time,
  response size (timing capture in view model builder; exposed in tests).
- `EXPLAIN QUERY PLAN` for the main workbench queries; add SQLite indexes
  ONLY if a query plan demonstrates a need (migration V8).
- Demo catalog target: median < 100 ms, worst < 250 ms.

### D7 — Case-study validation (Part 14)
- INC0302574 → no-change workbench (run 2). INC0299001 → partial-impact
  workbench (run 1). MAN LAN → multi-observer workbench (runs 7-10 +
  RRC11 I2PX result from Part 2, shown as NoBaselineVisibility or a run).
  No incident-wide MAN LAN verdict.

## Implementation order (each step commits green)

1. **Part 1**: peer-inventory machinery + CLI + rrc11-audit artifacts +
   `tests/rrc11_audit_test.rs` (5 required tests). Run inventory against
   the cached bview.
2. **Part 2**: I2PX decision artifact (+ pilot run only if qualifying
   baseline exists; expected report-only).
3. **Part 5**: collector-locations.json region+multihop; netprofile.rs
   region lookup; 5 required tests.
4. **Part 3**: `src/catalog/workbench.rs` — ObserverEpisode model,
   grouping, derivation; 6 required tests.
5. **Part 4**: sentence renderer; 6 required tests.
6. **Part 6**: regional breadth; 5 required tests.
7. **Part 9**: timeline model; 5 required tests.
8. **Part 11**: investigation cues; 4 required tests.
9. **Part 7+8+10+12**: workbench page + CSS + drill-down + API + text
   report CLI (`inim catalog workbench --event/--case-study`).
10. **Part 13**: query plans, bounded queries, timing capture, 5 required
    tests (web tests, token-free names).
11. **Part 14**: render/validate the three workbenches; rebuild demo DB.
12. **Part 15**: screenshots to tmp/ui-review/session-36/ (7 pages × 3
    viewports) via the existing harness; report facts only.
13. **Part 16**: docs (README, DESIGN, DOMAIN, OBSERVABILITY,
    DATA_PROVENANCE, 2 ADRs).
14. **Part 17**: full gates (fmt, test, test --release, clippy -D
    warnings, deny licenses, deny bans, cargo package) + confirmations.

## Required tests (verbatim from the brief)

Part 1: `selected_observer_audit_is_not_rendered_as_all_ris_audit`,
`historical_rrc11_peer_identity_comes_from_bview`,
`current_peer_list_is_context_not_historical_evidence`,
`direct_as11164_session_is_distinct_from_as11164_in_path`,
`absent_as2603_visibility_is_distinct_from_absent_as11164_session`
→ tests/rrc11_audit_test.rs.

Part 3: `episode_groups_same_observer_and_signature`,
`different_peers_at_one_collector_remain_separate`,
`direct_and_indirect_observations_remain_separate`,
`episode_counts_distinct_prefixes_and_streams_separately`,
`episode_restoration_uses_existing_lifecycle_evidence`,
`episode_generation_is_deterministic`.

Part 4: `sentence_distinguishes_collector_site_from_peer_location`,
`sentence_uses_effect_specific_verb`,
`sentence_distinguishes_visibility_restoration_from_baseline_restoration`,
`sentence_names_stream_and_prefix_units_when_they_differ`,
`sentence_never_claims_traffic_loss`, `sentence_never_claims_causation`.

Part 5: `region_classifies_observer_site_only`,
`peer_region_is_not_inferred_from_collector_region`,
`multihop_collector_is_visibly_labeled`,
`unknown_location_maps_to_unknown_region`,
`historical_location_metadata_is_time_scoped`.

Part 6: `breadth_always_has_visible_denominator`,
`no_change_and_no_baseline_are_distinct`,
`incomplete_coverage_is_not_counted_as_unchanged`,
`regional_summary_uses_observer_site_region`,
`broader_observation_is_not_rendered_as_greater_severity`.

Part 9: `operator_and_bgp_timeline_markers_are_distinct`,
`timeline_does_not_interpolate_unobserved_state`,
`lane_identity_is_observer_session`, `event_order_is_preserved`,
`unresolved_episode_has_no_fabricated_restoration`.

Part 11: `investigation_cue_is_traceable_to_observation`,
`cue_does_not_name_unreviewed_device`, `cue_does_not_claim_root_cause`,
`cue_uses_reviewed_plane_or_attachment_identity`.

Part 13: `workbench_get_performs_no_analysis`,
`workbench_get_performs_no_archive_parse`,
`workbench_query_count_is_bounded`,
`observer_episode_query_uses_expected_index`,
`workbench_result_is_deterministic`.
