# Session 31 — MAN LAN plan validation + one pilot (design)

## Facts established at start

- The AAR PDF is **not on disk** but the public source URL serves the exact
  expected bytes: SHA-256 `d29df26a…d7114` matches (verified). 15 pages,
  extracted text available (pdftotext). Appendix A gives per-participant
  times: CANARIE dropped 12:34 / up 21:33; NORDUnet flapping 15:33, interface
  disabled **16:50**, re-enabled 20:48; ESnet shut ~16:39, disabled 17:30,
  re-enabled 20:44; TWAREN first issue 13:45; OMAN rides CANARIE's interface;
  GÉANT up late; WIX brief 13:26–13:27; Ixia outage 16:54; NEAAR up 13:32.
- Network works (routeviews + docs.globalnoc reachable).
- No preflight-only analysis mode exists; no run-linking CLI; target
  research fields exist on `case_study_targets` but research updates have no
  flow (case-study import is immutable by slug+content sha).

## Part 1 — Attach the PDF

`/tmp/MANLAN-20190821-Postmortem.pdf` (sha verified) imported via
`inim catalog document import` into `data/inim.sqlite` (attaches `local_path`
to the metadata-only revision). Improve `pdf_page_count` with a best-effort
`pdfinfo` fallback when the raw-byte scan cannot count pages (real PDFs use
compressed object streams); the fallback resolves `pdfinfo` via PATH
(`Command::new("pdfinfo")`, standard lookup), degrades to None when absent;
tests gate the fallback path on pdfinfo availability.
Verify: sha, page count 15, media type, catalog-relative path,
`GET /documents/:id` serves through the guarded route. The PDF never enters
the package (data/ excluded; release tests assert it).

## Part 2 — Archive-plan audit + fix

Findings (to verify in tests): the planner currently selects **every 2-hour
RIB** in the horizon (12/collector) and counts updates at 5-minute cadence
(272 for 02:00→00:38 next day — correct IF the cadence is 5 min; prove with
exact first/last records). Reconstruction contract: **one baseline RIB** at
or before warmup_start; **updates** covering [warmup_start, cooldown_end];
**one optional validation RIB** (post-window checkpoint, never replayed as
event input). Also fix: URL year/month taken from incident_start (wrong
across month rollover) → per-timestamp year/month. Add URL dedup. Extend
`ArchivePlan` JSON with per-collector: baseline_rib, validation_rib,
first/last update stamps, uncovered_intervals, duplicate_urls,
estimated compressed+uncompressed bytes. Re-plan MAN LAN (Draft remains).
8 required tests.

## Part 3 — run_transitions storage audit

Canonical representation = immutable artifacts (`transitions.json`,
`evidence_appendix.jsonl`); `run_transitions` is a compact searchable index
(no full route states — already true). Add: sanity bound on import size
(const, reject above), per-row inserts inside the import transaction
(bounded, not streamed — documented). Document rebuild procedure (delete +
re-import or idempotent re-import). 5 required tests.

## Part 4/5 — Target research (method + record)

Research record file `case-studies/manlan-2019/target-research.json`
(schema: per target — entity type, historical ASN set, validity date,
sources with URLs, reviewed statement, path-predicate status, applicability,
status, confidence, provenance). Statuses: HistoricallyReviewed / Unresolved
/ NotApplicableToPublicBgp / AmbiguousServiceIdentity. Evidence hierarchy:
dated operator docs → contemporaneous RIR → PeeringDB archives → 2019
RouteViews observations → reputable dated docs. Research via subagents
(web_fetch: RIPEStat/PeeringDB/Wikipedia/archived docs); **contemporaneous
validation**: the 2019-08-21 RIB itself (Stage A) confirms origin + path
predicate empirically. Path predicate researched SEPARATELY from origin
mapping (Part 5): candidate = Internet2 transit presence (AS11537) — only
reviewed if dated evidence (2019 RIB paths) supports it; never assumed.

Apply flow: `inim catalog case-study apply-research --db --path
<target-research.json>` updates only the research fields of matching
`case_study_targets` rows (documented exception to row immutability —
research state, not incident content; idempotent; provenance recorded).
Migration V3 adds `research_updated_utc` to `case_study_targets` (one
column — not a new table/entity) so each mutation carries an audit
timestamp. Also add the missing `AmbiguousServiceIdentity` status constant
to domain.rs + validation.

## Part 6 — Pilot selection

Candidates with AAR-documented actions: NORDUnet (flap 15:33, disable 16:50,
re-enable 20:48 — matches the Part 9 example), ESnet (16:39/17:30/20:44),
CANARIE (12:34/21:33), WIX (13:26 brief). Expected selection: **NORDUnet
(AS2603)** — distinct documented action, moderate prefix volume, stable
well-documented ASN. Selection record with candidates/rejections/rationale.
Collector: route-views2. Baseline RIB rib.20190821.0200.

## Part 7 — Staged execution

- Stage A: add `--preflight-only` to `inim analyze` (stops after Phase A RIB
  preflight; one collector, one RIB, no updates). Output contract: JSON on
  stdout (`{"status":"preflight-only","per_collector":[{collector,
  origin_matching_routes, transit_matching_routes, frozen_streams,
  distinct_prefixes, distinct_peers}], "qualifying": <count>, "stopped":
  "no updates acquired"}`); human progress stays on stderr.
  Also validates AS2603 origin presence + AS11537-in-path empirically.
- Stage B: short-window pilot around 16:30–17:30 UTC (boundary: 16:50
  interface disable; sufficient warmup from 02:00 RIB + updates 02:00→18:30
  ≈ 200 files ≈ 0.6 GiB). Pilot event+manifest under
  `case-studies/manlan-2019/pilot/` (manifests/ + out/ layout so
  `inim catalog import --root` works); pilot event is a local-repository
  analysis event, not a ticket.
- Stage C: only if Stage B is interpretable; likely "not justified yet"
  (1/10 targets reviewed, predicate single-observation, ~2 GiB/collector
  for the full window).
- Run linking: `inim catalog case-study link-run --db --slug <slug> --run
  <id> [--role PilotObservation] [--note ...]` (uses existing
  `case_study_analysis_links` + store insert; no new tables).

## Part 8 — Phase-summary continuity tests

4 tests asserting existing continuous semantics (no baseline reset,
inherited impairment not counted as new transitions, restoration closes
prior lifecycles, one lifecycle spans phases).

## Part 9 — Pilot comparison output

Pilot result rendered as reviewed data in the plan record JSON:
`pilot: {status, target, window, collector, operator_evidence,
bgp_observation, temporal_relationship, interpretation, limitation,
finding}`. Labels/narrow wording enforced (no disproved/confirmed).

## Part 10/11 — Web + narrow conclusion

MAN LAN page: target research columns (mapped ASNs, predicate status, BGP
applicability), archive-plan audit block (previous vs corrected counts,
baseline RIB, update range, volume, review status), pilot block labeled
"Historical pilot — NORDUnet"; main case study keeps "no complete
incident-wide public-BGP conclusion". PlanView + TargetView extended (data
already stored).

## Part 12/13 — Docs + gates

README/DESIGN/DATA_PROVENANCE/OBSERVABILITY/ADR-003 updates; full gate chain
(fmt, test, test --release, clippy, deny, package); confirm PDF excluded,
no guessed mappings in reviewed data, no full download without review,
HTTP read-only, conclusion narrow. No publish/tag/push.
