# Session 37 — Semantic repair and NOC-grade workbench HCI

Starting HEAD: `82b75f020a97f81a92577a011fdfc37062a05e1d` (852 tests). Follows
the user's 16-part brief; each part's required tests are listed verbatim.
All existing tests must stay green (852 baseline) except where the brief
explicitly changes semantics (CoverageStatus::NoChange removal, restoration
derivation, expectation assessment source).

## Hard constraints

- `tests/release_test.rs::production_source_contains_no_internet2_specific_plane_branch`:
  `11164` and `i2px` = zero occurrences in src/; `11537`/`internet2` in src/
  must equal the frozen set. → **New src files must not contain any of
  `11164`, `i2px`, `11537`, `internet2`.** Plane labels/ASNs enter the UI
  only via runtime data (network-profile.json, manifests, pilot dir files).
- No BGP parsing / archive reads / analysis on the HTTP request path.
- No new BGP acquisition, no reruns, no analysis-conclusion changes.
- No severity score, no causation claims, no traffic-loss claims.
- MIT licensing intact; screenshots excluded from package (tmp/ already
  gitignored); no publish/tag/push.

## Verified data facts (session start)

- Case study MAN LAN links runs 3,4,5,6 (`case_study_analysis_links`).
  Streams per linked run: rrc00 11 Unchanged; rrc06 12 DepartedTransitPath
  (2 with restoration_time_utc); rrc15 13 Departed (0 rest) + 11 Unchanged;
  route-views2 33 = 11 Departed (8 rest) + 11 PathChangedStillViaTransit
  (0 rest) + 11 Withdrawn (11 rest).
- Breadth today: AMER 7 eligible/6 changed 46/57; APAC 2/2 12/12; EMEA 1/0
  0/11 → totals 10 eligible, 8 changed, 58/80 streams (brief's "46 of 57"
  is the AMER row; first-screen numbers are generated from the model).
- `analysis_runs.assessment` ALREADY holds the wanted assessments:
  run 1 "Partially consistent with the participant-relationship-unavailable
  expectation.", run 2 "Consistent with the redundant-attachment
  expectation." The workbench currently renders `manifest.target.label`
  ("RIPE via NYIIX" / "UVA via Internet2") as expectation — WRONG source.
- RRC11 regional corruption: `WorkbenchContext.no_baseline_sessions` stores
  (collector, label) but `regional_breadth` treats element 0 as REGION →
  row with region "rrc11". Registry actually maps rrc11 → AMER.
- route-views2 DepartedTransitPath streams have restoration_time_utc but
  `restored=false`; `build_episodes` only honors restoration when
  `s.restored==true` → episodes show "unresolved" despite lifecycle
  restoration evidence (brief Part 1.4).
- `case_study_workbench` handler ignores `?expand=1` (no Query param) →
  session-36 screenshots for expanded/prefix/timeline were byte-identical
  (all = ordinary page). Root cause of Part 8.
- Operator anchors exist in `pilot/pilot-result.json` operator_evidence
  (15:33 flapping, 16:50 disable, 20:48 re-enable) but are NEVER loaded
  into the workbench context → timeline has no operator markers.
- MAN LAN: no_baseline rrc11 row comes from
  `pilot/rrc11-pex-pilot-decision.json` (decision blocked-no-direct-session).
- RIPE/UVA events have NO session audit (peer ASNs not in catalog evidence)
  → peer ASN renders "unreviewed"; must become
  "peer ASN not in reviewed evidence" (observed fact vs review status split).
- Case study horizon: start 04:00 end 22:38 UTC (case-study.json); pilot
  window 16:00–17:30 (linked runs' manifest event_window_utc).

## Design decisions

### D1 — Expectation assessment source (Part 1.1)
`expectation_assessment` := first Complete run's `assessment` (already
worded exactly as the brief wants) for EVENTS. For CASE STUDIES: no
incident-wide expectation assessment exists → render
"No incident-wide expectation assessment exists; observations are limited
to the reviewed NORDUnet pilot." Never `manifest.target.label`.

### D2 — Current observed result (Part 1.2, 2)
Events keep run verdict. Case studies: generated summary from model counts
via `render_observed_result(&self) -> String`:
- changed==0 → "No route-state change at {e} of {e} eligible observer
  sessions covering {b} baseline streams."
- changed==eligible → "Route-state changes at {c} of {e} eligible observer
  sessions covering {b} streams."
- else → "Route-state changes appeared at {c} of {e} eligible observer
  sessions. {cs} of {b} baseline streams changed."
- append no-baseline sentence when `no_baseline_sessions` non-empty:
  "One additional session had no qualifying baseline." (+ note text).
Plus case-study-only scope sentence: "This is a single-target historical
pilot, not a complete MAN LAN incident assessment." (data: case-study
title prefix + "incident assessment").
`current_result` for case studies := this generated text
("Multi-observer route-state changes observed in the NORDUnet pilot" is
covered by the generated sentence); the header additionally states
"No complete MAN LAN incident-wide BGP assessment has been performed."

### D3 — Episode status split (Part 1.3)
Three separate concepts on every episode row:
- `observed_signature` = EffectKind (unchanged).
- `end_state` = NEW derived field (string enum `EndState`): derived from
  lifecycle evidence (see D4).
- `coverage_status` = CoverageStatus::Complete for built episodes
  (baseline existed); NoBaselineVisibility/IncompleteCoverage only for
  coverage-only sessions. **Remove CoverageStatus::NoChange variant** —
  it was the overloaded "Status" that showed NoChange on changed rows.
  Update all tests referencing it.

### D4 — Restoration from lifecycle evidence (Part 1.4)
`build_episodes`: derive `restoration_start/end` from
`stream.restoration_time_utc` whenever present (drop the `s.restored`
gate). New per-episode counts: `restored_stream_count` (changed streams
with restoration_time_utc). `end_state` derivation:
- NoRouteStateChange → "No route-state change"
- TemporaryStreamAbsence (all restored) → "Visibility restored on changed
  path"; RouteWithdrawal → "Absent at window end"; withdrawal mix →
  "Still changed at window end"
- path-change kinds: all changed streams restored → "Baseline restored";
  some → "Still changed at window end" (RESTORED column still shows the
  partial interval); none → "Still changed at window end"
RESTORED column := restoration interval endpoints (HH:MM:SS) or "—".

### D5 — Regional row integrity (Part 1.6)
`WorkbenchContext.no_baseline_sessions`/`incomplete_sessions` become
`Vec<CoverageSessionView>`-equivalent triples (collector, region, label)
with region resolved at load time from the registry. `regional_breadth`
takes the same triples; region keys validated ⊆ {AMER, EMEA, APAC, Unknown}
(invariant test). rrc11 row becomes AMER + NoBaselineVisibility.

### D6 — Peer evidence split (Part 1.7)
Peer ASN is an OBSERVED fact when reviewed evidence provides it; when not
available render "peer ASN not in reviewed evidence" — never
"peer ASN: unreviewed". Org label / role keep reviewed/unclassified
semantics ("organization unclassified", "role unclassified" via
profile.role_label / existing "unclassified observed ASN" data label).
Template renders "AS137 · organization unclassified · role unclassified".

### D7 — Human labels (Part 1.8)
`EffectKind::human_label()`: No route-state change / AS path changed /
Temporarily absent / Withdrawn, not restored / Left the {plane} path /
Returned to the {plane} path / Prepend changed / Mixed route-state change.
`CoverageStatus` human labels. Plane label: replace
`"reviewed transit {predicate-json}"` fallback with runtime-built label:
path_classifiers display_label when present; else if predicate ASNs map to
a profile plane display_label → "{label} path (AS{asn})"; else
"path via AS{asn}". Load `network-profile.json` for events too
(`load_registry_only` gains plane_labels map) — the ASN→plane mapping is a
stable reviewed fact (same provenance class as collector locations).
Raw enums/predicate JSON stay in `render_text` and JSON API ("technical
details"), never in primary UI tables.

### D8 — First screen (Part 2)
Compact incident header replacing the generic EVENT SUMMARY table:
title, subject kind, pilot/incident horizons (see D9), OBSERVED RESULT
(render_observed_result), SCOPE LIMIT (case studies), links to
ticket interpretation / source documents / analysis details / provenance
(case-study page links / event detail). No long narrative in the first
block. Required tests: first_screen_contains_observed_result,
first_screen_contains_observer_denominator,
case_study_first_screen_contains_scope_limit,
first_screen_does_not_contain_long_source_narrative.

### D9 — Horizons (Part 1.5, 11)
VM gains `incident_horizon_start/end` (case-study.json start/end for case
studies; empty for events) and `pilot_label` ("NORDUnet / AS2603" from
pilot-result.json target). Header shows BOTH:
- Operator incident horizon: 04:00–22:38 UTC
- Displayed BGP pilot: NORDUnet / AS2603 · 16:00–17:30 UTC
Case-study header also: linked source tickets (case-study.json
related_events), selected historical pilot, "no incident-wide BGP verdict".
Source task type for case studies := "Not applicable — multi-ticket case
study"; reviewed incident role := "Multi-ticket operator incident";
never "not reviewed" when the concept is N/A.
Required tests: case_study_does_not_render_blank_ticket_fields,
not_applicable_is_distinct_from_not_reviewed,
case_study_header_names_selected_pilot,
case_study_header_states_no_incident_wide_verdict.

### D10 — Desktop layout (Part 3)
CSS: main content max-width 1440px, page horizontal padding 12–20px,
summary+breadth full width, episode table full width, no
`overflow-wrap: anywhere` on table cells, `white-space: nowrap` on
timestamps/ASNs/prefixes/collector IDs/region codes/status labels,
wrapping only in prose columns. Sizes: body 13–14px, table 12–13px,
headings 16–18px.

### D11 — Timestamps (Part 4)
`workbench_time(ts, window_start) -> String` in workbench.rs: parse the
"YYYY-MM-DDTHH:MM:SS" prefix (tolerant of Z/+00:00/nanos); same date as
window start → "HH:MM:SS UTC"; different date → "YYYY-MM-DD HH:MM:SS UTC".
Used for display rows only; exact timestamps remain in expanded details,
JSON API, render_text, copied values. Required tests:
same_day_workbench_time_uses_hms, cross_day_time_includes_date,
exact_timestamp_remains_in_details, timezone_is_always_explicit.

### D12 — Breadth matrix (Part 5)
Columns: REGION | CHANGED/ELIGIBLE (one cell "6/7") | STREAMS CHANGED/
BASELINE ("46/57") | PREFIXES | FIRST CHANGE | RESTORED | COVERAGE GAPS
(no_baseline+incomplete count). Separate "NO QUALIFYING BASELINE" block
(region, session, note). Color cues (amber for changed, grey/green for
unchanged, hatched for no baseline) always paired with explicit text.
Explanation text must match the combined-cell rendering.

### D13 — Episode table (Part 6)
Columns: FIRST | REGION | OBSERVER (collector · site) | PEER+VIEW
("AS1916 · indirect Internet2 R&E" — relationship + plane combined) |
OBSERVED CHANGE (human label) | STREAMS | PREFIXES | RESTORED | END STATE
| DETAILS (`<details><summary>View</summary>`). Changed episodes first by
time; then unchanged, no-baseline, incomplete. No-change rows collapsed
initially but denominator visible (summary line + count). Filters:
`?changed=1` (only changed), `?region=AMER`, `?rel=direct|indirect`,
`?kind=<effect>`. Required tests: changed_rows_sort_before_unchanged_rows,
observer_and_site_are_rendered_together,
peer_asn_and_relationship_are_rendered_together,
changed_row_has_effect_specific_result,
end_state_matches_lifecycle, no_change_rows_remain_discoverable.

### D14 — Lane timeline (Part 7)
Server-rendered SVG (no chart lib): one lane per observer session + one
"Operator report" lane. BGP lane axis = pilot window (window_start..end);
operator lane axis = anchor min..max (15:33–20:48). Markers: first change
(diamond), absence interval (red bar between exact endpoints), path-change
interval (amber bar), restoration marker (green triangle), changed-at-end
(hollow square at window end). Operator markers = distinct class (blue
diamond + label). No interpolation. Text fallback table below (existing
TimelineLane table with HH:MM:SS). Operator anchors loaded from new
reviewed file `pilot/operator-anchors.json` (structured, provenance →
pilot-result.json operator_evidence). Timeline marker classes:
`.tl-op`, `.tl-bgp`, `.tl-absence`, `.tl-path`, `.tl-restore`,
`.tl-changed-end`. Required tests:
timeline_has_one_lane_per_session,
operator_markers_and_bgp_markers_have_distinct_classes,
absence_interval_has_explicit_start_and_end,
unresolved_end_state_has_no_restoration_marker,
timeline_text_fallback_contains_same_evidence.

### D15 — Drill-down states (Part 8)
Server-rendered query states (no JS required):
- `?episode=<n>`: that episode's `<details>` open (open attribute),
  containing: sentence, collector+peer identity, source family, direct/
  indirect relationship, baseline+changed path-plane state, first/last/
  restoration timestamps (exact), grouped prefix signatures (category →
  count + prefixes), representative evidence references (evidence_refs
  rendered as text).
- `?prefixes=<n>`: prefix drill-down tables open for episode n: prefix |
  baseline path (via {plane}) | change (human category) | first change |
  restoration | end state | evidence link.
- `?changed=1`: changed-only episode table.
- `?view=timeline`: episode table collapsed, timeline primary.
- Default: everything collapsed; `case_study_workbench` handler gains the
  Query param (currently ignores it).
`EpisodeStream` gains first_change_utc / restoration_time_utc passthrough
and derived `stream_end_state`. Required tests:
expanded_episode_contains_episode_specific_marker,
prefix_drilldown_contains_prefix_rows,
timeline_capture_contains_timeline_marker,
ordinary_workbench_does_not_contain_expanded_content,
drilldown_uses_no_raw_mrt_parse.

### D16 — Grouped cues (Part 9)
`build_grouped_cues(episodes, plane_label, plane_asns) -> Vec<GroupedCue>`
(group title, text with prefix count + time range + affected session
count, link target `?prefixes=<ep>`). 3–5 groups max, grouped by
operational question (advertisements to plane / alternate path selection /
restoration quality / observer disagreement), boilerplate disclaimer once
at section level. Per-episode exhaustive cues move into drill-down.

### D17 — Runs → Analysis history (Part 10)
`WorkbenchRunView` gains `source` ("{family}/{collector}") and
`completed_at`; section renamed "Analysis history", inside `<details>`
collapsed by default, columns: run | source family/collector | plane |
completed | result | details (link). No archive-coverage column on the
primary page (coverage lives on /analyses/{id}).

### D18 — Mobile (Part 12)
CSS @media (max-width: 640px): incident result+scope stay at top; tables
wrap in `.wb-scroll` (overflow-x: auto) with visible affordance
(thin scrollbar + fade hint); episode rows become definition lists via
data-label attributes (no JS); no text below 12px; timestamps/ASNs/
prefixes never break (nowrap); keyboard/screen-reader labels preserved
(th scope, aria-labels on summary).

### D19 — Harness (Part 13)
New `scripts/screenshot-review-session37.sh` (playwright chromium):
per capture set viewport explicitly, fetch page, verify content marker,
capture PNG, assert PNG width == viewport width (read IHDR via python3),
record SHA-256, assert distinct states have distinct hashes. Captures:
manlan-first, manlan-changed-table, manlan-expanded-absence (route-views2
episode, ?episode=), manlan-prefix-drilldown (?prefixes=), manlan-timeline
(?view=timeline), ripe-no-change, uva-partial-impact, manlan-mobile-390.
Viewports 1440×900, 1280×800, 390×844 (full-page allowed to exceed height;
width must match). Exit non-zero on: missing marker, duplicate hash,
width mismatch, HTTP error, expansion not occurred.

### D20 — Perf guard (Part 15)
Measure workbench GET latencies before/after via curl loop (median/max).
Retain median <100 ms, max <250 ms. Existing no-analysis/no-parse tests
stay. No new optimization beyond what the redesign needs.

## Tasks (order)

1. Workbench model: CoverageStatus::NoChange removal + Complete;
   EndState enum + derivation; restoration from lifecycle evidence
   (D3/D4); EpisodeStream timestamp passthrough + stream_end_state (D15).
2. WorkbenchContext triples + regional_breadth signature (D5);
   operator-anchors.json loader + context.operator_anchors wiring (D14);
   network-profile plane_labels for events (D7).
3. VM fields: expectation_assessment source (D1), current_result for case
   studies + render_observed_result (D2), incident_horizon + pilot_label +
   linked tickets (D9), plane_asns + grouped cues (D16), run source/
   completed_at (D17), workbench_time + human labels (D7/D11).
4. View/template: incident header (D8/D9), breadth matrix (D12), episode
   table + filters (D13), lane timeline SVG + fallback (D14), drill-down
   states + prefix tables (D15), grouped cues (D16), analysis history
   (D17), CSS desktop/mobile/sticky/focus/nav (D10/D12/D18).
5. Handler: case_study_workbench Query params (D15).
6. Harness + screenshots (D19); perf measurement (D20).
7. Quality gates (Part 16) + completion report.

## Verbatim required tests per part (from the brief)

Part 1: changed_episode_cannot_have_no_change_result,
temporary_absence_with_restoration_has_restored_end_state,
case_study_pilot_has_no_incident_wide_verdict,
expectation_assessment_uses_assessment_not_title,
pilot_window_is_distinct_from_incident_horizon, region_key_is_valid,
observed_peer_asn_is_never_rendered_as_unreviewed,
primary_ui_contains_no_raw_predicate_json,
primary_ui_contains_no_raw_internal_enum_labels.

Part 2: first_screen_contains_observed_result,
first_screen_contains_observer_denominator,
case_study_first_screen_contains_scope_limit,
first_screen_does_not_contain_long_source_narrative.

Part 4: same_day_workbench_time_uses_hms, cross_day_time_includes_date,
exact_timestamp_remains_in_details, timezone_is_always_explicit.

Part 6: changed_rows_sort_before_unchanged_rows,
observer_and_site_are_rendered_together,
peer_asn_and_relationship_are_rendered_together,
changed_row_has_effect_specific_result, end_state_matches_lifecycle,
no_change_rows_remain_discoverable.

Part 7: timeline_has_one_lane_per_session,
operator_markers_and_bgp_markers_have_distinct_classes,
absence_interval_has_explicit_start_and_end,
unresolved_end_state_has_no_restoration_marker,
timeline_text_fallback_contains_same_evidence.

Part 8: expanded_episode_contains_episode_specific_marker,
prefix_drilldown_contains_prefix_rows,
timeline_capture_contains_timeline_marker,
ordinary_workbench_does_not_contain_expanded_content,
drilldown_uses_no_raw_mrt_parse.

Part 11: case_study_does_not_render_blank_ticket_fields,
not_applicable_is_distinct_from_not_reviewed,
case_study_header_names_selected_pilot,
case_study_header_states_no_incident_wide_verdict.

## Required invariant tests (Part 1, verbatim names)

changed_episode_cannot_have_no_change_result,
temporary_absence_with_restoration_has_restored_end_state,
case_study_pilot_has_no_incident_wide_verdict,
expectation_assessment_uses_assessment_not_title,
pilot_window_is_distinct_from_incident_horizon, region_key_is_valid,
observed_peer_asn_is_never_rendered_as_unreviewed,
primary_ui_contains_no_raw_predicate_json,
primary_ui_contains_no_raw_internal_enum_labels.
