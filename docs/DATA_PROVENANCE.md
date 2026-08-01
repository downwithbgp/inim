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
Observation IDs and evidence IDs are assigned **after** sorting by the
documented order: collector, timestamp, archive order, element sequence,
peer IP, prefix, path_id (`None < Some(id)`). Serial and parallel
completion produce identical IDs and identical artifacts.

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
