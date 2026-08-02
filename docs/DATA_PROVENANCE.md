# inim — Data Provenance

## Principle

Every conclusion rendered by inim must be traceable to concrete source
records. This document defines the provenance model.

## Current state

### Event subjects are data, not code
No event name, ASN, or network name appears in any domain enum variant.
Manifests carry reviewed entity/predicate mappings; rendered messages
interpolate manifest values.

### Reviewed manifest (canonical, schema v2)
- `event_id`, `revision`, `schema_version`, windows, warmup/cooldown
- `target.origin_asns` — reviewed entity mapping (origin ASNs)
- `target.transit_predicate` — `TransitPredicateMapping` with
  `status` (Reviewed/Unresolved), `predicate`, and `provenance`
  (statement, reviewer, date)
- `collectors` + `collectors_provenance`, `analyst_notes`

Legacy single-ASN shortcut fields (`managed_network_asn`,
`internet2_asn`) are **rejected** on load with
`LegacyManifestRequiresMigration`; the offline `migrate-manifest` helper
converts them with analyst-confirmed provenance and never invents
unresolved ASNs.

### Observation provenance
Every `RouteObservation` carries:
- `source_url` / `archive_sha256` — canonical source URL and archive
  checksum
- `role` (RIB/updates), `parser_representation`
- `mrt_timestamp`, `element_seq` (deterministic within the archive),
  `archive_order` (deterministic across archives)
- `path_id` — ADD-PATH identity, preserved end-to-end

### Deterministic identity
Observation IDs are assigned **after** sorting by the documented order:
collector, timestamp, archive order, element sequence, peer IP, prefix,
path_id (`None < Some(id)`). Serial and parallel completion produce
identical IDs and identical artifacts. Evidence references carry the
observation id plus archive/URL provenance — there is no separate
evidence id namespace.

### Evidence chain
`StateChange` and `RouteTransition` carry evidenced baseline/before/after
states plus the triggering `EvidenceRef` (observation id, source URL,
archive SHA, collector, peer, prefix, timestamp, element seq, path_id).
Lifecycle transitions, restorations, ambiguity records, and semantic
waves retain evidence references.

### Derived caches (schema v2)
- RIB cache: source URL + SHA-256, collector, parser identity, reviewed
  entity ASNs, canonical TransitPredicate identity, frozen cohort
  identity (ObserverPrefixKey values), every baseline `RouteKey`
  including `path_id`, evidenced baseline observations, preflight
  counters, payload checksum.
- UPDATE cache: full `RouteObservation` records (path_id, complete
  attributes), source URL + SHA, archive order, element sequence,
  admission counters, cohort identity, parser identity, payload
  checksum. A zero-observation cache remains a valid hit.

### Artifacts
Report, evidence appendix, lifecycle, withdrawal audit, semantic waves,
comparison, and analysis-plan artifacts all carry schema versions. Old
artifacts are archived (e.g. `out/archive/pre-observer-prefix-schema/`),
never parsed as current schema.

## Audit trail

Artifacts written per event:
- `report.txt` / `report.json` — observed event signature, observable
  mechanism hints, limitations, verdict, evidence
- `archive_manifest.json` — every source archive (URL, local path,
  collector, type, size, SHA-256)
- `evidence_appendix.jsonl` — one line per transition with baseline/
  before/after states and triggering evidence
- `lifecycle.json`, `semantic_waves.json`, `withdrawal_audit.json`,
  `limitations.json`
- `analysis_plan.json` / `analysis_plan.txt` (plan command) — plan
  status, reason, broker calls (0), MRT files examined (0)

## Reproducibility

Reports are deterministic: no RNGs, stable sorts, explicit timestamp
ordering, deterministic wave clustering, deterministic IDs (serial and
parallel runs produce identical artifacts).

## Test fixture provenance

### MRT update-example fixture

- **File:** `tests/fixtures/mrt/update-example.gz`
- **Source:** `https://spaces.bgpkit.org/parser/update-example.gz`
- **Fetch date:** 2026-07-31
- **SHA256:** `9298763bbecbaef2a4378aa8bf58f0c8e911d9afd8e5d4cd1c15f0beb6922d66`
- **Size:** 68,469 bytes (compressed)
- **Content:** BGP4MP update records used by bgpkit-parser's own test suite.
- **Usage:** `ingest::tests::parses_actual_mrt_fixture_into_observations`.

## Local event catalog

- Catalog events are source-neutral identities; GRNOC Public Task Viewer
  is the first source adapter (`EventCatalogSource`).
- Event snapshots preserve the raw source payload, retrieval timestamp,
  source URL, content SHA-256, normalized fields, and parser version.
  Snapshots are immutable: a changed ticket creates a new snapshot; the
  latest event view is derived from the latest snapshot.
- Reviewed manifest revisions are immutable and reference the exact
  snapshot reviewed against. Analysis plans reference the exact manifest
  revision; analysis runs reference the exact plan.
- Observations and stream lifecycles are associated with an AnalysisRun,
  never with a mutable event. Artifact paths are relative to the catalog
  root; SQLite never stores raw MRT data or machine-specific absolute
  paths.
- The catalog database uses versioned migrations (`PRAGMA user_version`),
  foreign keys, and WAL mode; migrations and imports are transactional.

## Case-study provenance

- A case study's source documents are immutable reference records: source
  URL, SHA-256, media type, best-effort page count/metadata, import time,
  provenance note, redistribution status. Content deduplicates by SHA-256;
  changed content is a new revision.
- "Referenced by the AAR" and "independently retrieved source snapshot
  exists" are distinct states. Historical tickets that are only referenced
  keep their external identifiers as unresolved document references; the
  importer never fabricates a source snapshot.
- Phase boundaries and claims record their source document and
  page/section. `exact` vs `summarized` precision preserves the difference
  between a timestamp stated in the detailed timeline and a broad boundary
  summarized by the report.
- The archive plan records the reproducible horizon (warmup / incident /
  cooldown), expected files, and estimated sizes flagged as estimates;
  exact sizes are recorded at acquisition. Plans are `Draft` until
  reviewed.
- The MAN LAN AAR PDF is not redistributed: its record carries
  redistribution status `Unknown` and no local copy exists in this
  repository; local document storage is excluded from the crate package.

## Historical research and pilot provenance

- Reviewed target mappings carry: exact ASN set, validity date, sources
  (URLs + what each says), reviewed statement, confidence. A mapping is
  HistoricallyReviewed only with dated evidence; AS9264-style mistakes
  (candidate positively excluded) are recorded, not silently replaced.
- The path predicate is validated by contemporaneous RouteViews RIB
  observation during Stage A preflight (evidence hierarchy level 4) — the
  2019-08-21 02:00 RIB showed 33 AS2603 routes transiting AS11537.
- Pilot artifacts (report, transitions, withdrawal audit, evidence
  appendix) are immutable run artifacts under
  `case-studies/manlan-2019/pilot/out/MANLAN-2019-NORDUNET-PILOT/`;
  `run_transitions` remains a compact index rebuilt from `transitions.json`.
- The pilot result is one target, one collector, one window; it never
  becomes a complete-incident verdict in any artifact or UI text.

## Performance metadata vs substantive output

`performance.json` records stage wall-clock timings and per-archive parse
metrics. It is volatile by design: timings depend on hardware, load, and
cache state and are EXCLUDED from substantive artifact-equivalence checks.
Substantive outputs (report, transitions, lifecycle, waves, evidence,
withdrawal audit) never contain benchmark timing, and the routing verdict
never depends on performance measurements. Acquisition (download) time is
reported separately from parsing time; parser-scaling benchmarks run with
all raw archives already local.

## Corpus acquisition and redistribution policy

### Acquisition
- Public-source acquisition is polite, bounded, and incremental: default
  ceiling 5 requests/second (smooth limiter, burst 2, max 5 in-flight),
  budget 100 requests per sync by default; higher rates require explicit
  `--allow-higher-rate` flags. The GRNOC viewer's public JSON endpoints
  are undocumented; the reviewed ceiling is local operational guidance,
  not a public API guarantee. Rate-control responses (429 / Retry-After /
  403 / 503) always override the configured ceiling; conditional requests
  (ETag/If-None-Match, If-Modified-Since) are honored and a 304 never
  creates a new snapshot. Stop conditions: repeated 429/403, unexpected
  authentication, robots prohibition, schema incompatibility affecting
  most items. Permanent 404s are never retried.
- Discovery is explicit only: analyst seeds, document/case-study
  references, ticket-description references, and scoped public search.
  There is no blind numeric-ID enumeration and no "download everything"
  mode.
- Corpus completeness is never assumed; the corpus is labeled a locally
  acquired public-ticket corpus.

### Local retention
- The local database retains public source snapshots (raw payloads +
  normalized fields) for reproducibility, with per-fetch HTTP provenance
  and discovery provenance. No session cookies or secrets are stored.
- Snapshots are immutable; a changed payload creates a new snapshot and
  old snapshots remain linked to their historical runs.

### Redistribution
- Before distributing a corpus dump, separately review: source terms,
  attribution, personal names or contact details, redistribution
  expectations, document copyright, and ticket-description content.
- The crate package must not contain the downloaded corpus.
- Source-controlled fixtures remain minimal and provenance-documented
  (see `tests/fixtures/README.md`).
- inim does not claim MIT ownership over public ticket content.
- Corpus export is **metadata-only by default** (`inim catalog
  corpus-export`): external ids, hashes, source URLs, and optional
  normalized fields; no raw payload export. Raw-payload export requires
  a separate redistribution review before any future implementation.
- Public corpus publication is not implemented.

## Reviewed ticket interpretation

- **Corpus acquisition and analysis review are separate stages.** The
  acquisition stage produces immutable source snapshots with fetch
  provenance; the review stage adds analyst interpretation on top.
- **Source tickets remain immutable snapshots.** Reviewed
  interpretations live in the `ticket_reviews` table, keyed to catalog
  events, and never modify `event_snapshots` (raw or normalized).
- **Reviewed roles do not overwrite source task types.** The source
  `task_type` comes from the snapshot; the reviewed case-study role is a
  separate vocabulary (ChangeWindow / PrimaryIncident /
  ParticipantImpact / AlarmOrTelemetry / RollbackOrRecovery /
  OperationalTask / Other).
- **Per-field provenance is required.** Every interpretation field
  cites either a snapshot field (`SnapshotField:<field>`) or a
  reference document (the AAR, with `source_document_id`). A missing
  source field is never backfilled without a cited document; AAR
  enrichment without AAR provenance is rejected by the importer.
- **Reviewed relationship edges** use specific kinds (RollbackFor,
  ParticipantImpactDuring, AlarmDuring, OperationalTaskDuring,
  RelatedChange, RelatedIncident, TracksRemainingImpactIn, References)
  with evidence kinds (ExplicitTicketText, ReferenceDocument,
  AnalystReviewed) and may carry ticket-text AND document support on
  one edge. Derived edges (temporal overlap, shared reviewed entity)
  remain visibly distinct (`Derived*` evidence, Unreviewed).
- **Unavailable TASK records stay unresolved document references.** No
  snapshot is manufactured; the graph audit lists them as unresolved.
- **Candidate grouping is per pair and explainable.** One candidate per
  ticket pair whose evidence lists every supporting signal; temporal
  overlap alone is `TemporalCoincidence` (hidden from the default queue
  but queryable). Rejected candidates stay suppressed until the
  evidence fingerprint changes.

## Multi-observer analysis

- **RIS and RouteViews are peer observer families.** Each family has
  its own archive base, filename conventions, RIB cadence, and
  collector identity; a collector id is only meaningful with its family
  (`(family, collector)` is the identity, and derived caches are keyed
  on it).
- **Different observers may legitimately disagree.** Each selected
  collector produces its own independent AnalysisRun with its own
  evidence; runs are never merged into a combined verdict.
- **Multiple observer agreement is still not global proof.** The
  cross-observer comparison vocabulary is bounded ("Observed at
  multiple independent public collectors", "Observed only at one
  selected collector", "Similar route-state change with different
  timing", "No counterpart at this observer", "Insufficient baseline
  visibility") and never writes "globally confirmed", "complete
  outage", "traffic loss confirmed", or "operator action confirmed".
- **Absence of baseline visibility is not absence of impact.** A prefix
  may have different observer availability; the comparison states
  "Insufficient baseline visibility" instead of "no change".
- **Batch reuse does not merge event assessments.** Raw archives are
  downloaded once per unique URL; derived caches are reused only when
  cohort/cache identity permits; each AnalysisRun keeps its own
  evidence and verdict regardless of batch membership.
- **Weak temporal candidates are hidden by default.** Temporal-only
  coincidences remain stored and queryable, but do not dominate the
  analyst queue.

## Reviewed plane model provenance

- The Internet2 R&E / peer-exchange plane identities (AS11537 / AS11164)
  are reviewed profile data (`case-studies/manlan-2019/pilot/
  network-profile.json`); they never appear as control flow in
  production source (release gate
  `production_source_contains_no_internet2_specific_plane_branch`).
- Historical session relationships come from the MRT peer metadata of
  the 2019-08-21 baseline RIBs (`session-audit-2019.json`); current
  peer lists never override them.
- Collector locations are recorded with temporal provenance
  (`collector-locations.json`, as-of 2019-09-05, Internet Archive
  snapshot 20190905014936 of the RIS peer list); RRC06 is Otemachi,
  Tokyo, Japan — not a US collector.
- The source-extraction cache (`cache/extracted/`) is versioned
  (schema + parser) and keyed by content (source sha); it never changes
  evidence ids, which are assigned deterministically from observation
  content after sorting.
- The GRNOC sync ceiling of 5 requests/second is reviewed local
  operational guidance, not a public API guarantee; rate-control
  responses always override the configured ceiling, and the sync
  records its metrics for the record.

## Workbench and RRC11 audit provenance

- **Historical peer identity comes from the bview, not the peer list.**
  The RRC11 2019-08-21 baseline's peer table (peer IP, peer ASN,
  address family, route counts) is the evidence for the direct
  peering-plane session question; the current RIPE peer list is
  supporting context only and never overrides audit rows. RIB source
  sha and bview timestamp are recorded per row.
- **Full peer inventory** (`--full-inventory`) streams a RIB and
  aggregates per session (total routes, origin routes, distinct origin
  prefixes, path classes); it is deliberately not written to the
  origin-scoped extraction cache, and memory is bounded by session
  count.
- **Observer-site regions and multihop** are reviewed metadata in
  `collector-locations.json`, time-scoped by `as_of`; unknown locations
  map to Unknown and never guess.
- **Episode timestamps** come from immutable evidence: the transition
  index and the lifecycle.json per-stream `first_change` /
  `restoration_time` (schema v8); restoration intervals are never
  extrapolated.
- **Workbench reads** catalog tables (indexed), reviewed data files,
  and immutable report artifacts only; no analysis, no MRT parse, no
  cache writes happen on the request path.

## Operator anchors and workbench data provenance

- **`pilot/operator-anchors.json`** (new, reviewed): structured
  operator-reported timeline anchors derived from the existing
  `pilot/pilot-result.json` `operator_evidence` string (AAR Appendix A
  via the ticket reviews): NORDUnet interface flapping reported 15:33
  (INC0040272), interface disabled 16:50 (INC0040289), interface
  re-enabled 20:48. Each anchor records its source. Rendered on the
  workbench timeline with visibly distinct operator styling.
- **Plane labels in the workbench** are built from runtime data only:
  manifest `path_classifiers` display labels, then reviewed
  `network-profile.json` ASN→plane display-label mappings, then the
  predicate ASN list ("path via AS{asn}"). Raw predicate JSON is never
  rendered in the primary UI.
- **Expectation assessments** on the event workbench come from the
  immutable run `assessment` (report.json `assessment.statement`);
  manifest target labels are titles, not assessments.
- **Restoration and end states** derive from the immutable
  lifecycle.json per-stream `restoration_time` evidence; no
  presentation field fabricates or extrapolates a restoration.

- **Raw-archive SHA sidecar**: every downloaded MRT archive gets a
  `.sha256` sidecar written at download time; cache reuse verifies the
  checksum before parsing. `archive_manifest.json` records URL, local
  path, size, and SHA-256 for every archive used by a run.

## Finding and chronology audits

- `inim catalog finding-audit` writes the exact finding record the prose
  renderer uses (fields, signatures, restoration classes, evidence
  references). `inim catalog finding-chronology-audit` writes the
  checked per-prefix chronology audit — the exact ordered transition
  sequence with evidence ids and archive identities, read from the
  canonical lifecycle artifact. Both are read-only derivations; they
  never alter canonical evidence. Reviewed chronology audits are
  committed under `case-studies/<slug>/` (e.g.
  `case-studies/inc0299001/finding-chronology-audit.json`).

## Relationship audit and session metadata

- **`case-studies/inc0302574/out/INC0302574/relationship-audit.json`**
  (reviewed):
  event-date (2026-07-30) RIS baseline inventories at RRC11 and RRC14
  (bview SHAs recorded): the direct AS11164 sessions existed at both
  collectors (IPv4 + IPv6 each), but zero AS3333-origin routes were
  visible through them and no AS3333-origin path contained AS11164.
  Decision `insufficient-visibility`; the existing AS11537 run is
  classified `supporting-re-plane`. The current RIS peer lists are
  supporting context only — the event-date bview peer table is
  authoritative.
- **`observer_session_metadata`** (V9 migration): observed peer ASNs
  from baseline RIB evidence, time-scoped, with source archive + SHA;
  populated by `inim catalog session-metadata-backfill` (UVA's four
  peers from the cached 2026-07-14 RouteViews RIB: AS2152, AS11537,
  AS293, AS7660).
