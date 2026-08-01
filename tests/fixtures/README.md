# inim — test fixture provenance

This document records the origin, license basis, and packaging status of
every committed test fixture. inim is MIT-licensed; MIT covers inim's
original code and documentation but does not relicense upstream data
copied into these fixtures. None of these fixtures claim MIT ownership
over upstream raw data.

## Fixture families

### 1. Synthetic fixtures (none committed)

All synthetic observation data used in unit tests is generated in code
(`fixtures::make_synthetic_rib` etc.) at test time. Nothing to package.

### 2. Upstream parser fixture — `mrt/update-example.gz`

- **Purpose:** exercises the bgpkit-parser → `RouteObservation` conversion
  boundary (`ingest::tests::parses_actual_mrt_fixture_into_observations`).
- **Source:** bgpkit-parser upstream test suite.
- **Original project:** BGPKIT (`bgpkit/bgpkit-parser`).
- **Source URL / identifier:**
  `https://spaces.bgpkit.org/parser/update-example.gz` (referenced in
  `docs/DATA_PROVENANCE.md`).
- **Retrieval date:** 2026-07-31.
- **Exact or minimized:** exact copy of the upstream file (68,469 bytes).
- **SHA-256:** `9298763bbecbaef2a4378aa8bf58f0c8e911d9afd8e5d4cd1c15f0beb6922d66`.
- **Content:** BGP4MP UPDATE records (real, anonymized RouteViews data).
- **Upstream license:** MIT (BGPKIT is MIT-licensed).
- **Modified:** no.
- **Why redistribution is permitted:** MIT license permits copying and
  redistribution with license preservation; the file is a small test
  artifact from a public test suite.
- **Attribution:** attribution preserved in `docs/DATA_PROVENANCE.md` and
  here.
- **Packaging:** **include** — required for the ingest test to run from a
  packaged crate.

### 3. Public ticket fixtures — `internet2/*.json`, `grnoc/INC0301970.json`

- **Purpose:** drive ticket parsing, expectation derivation, planning, and
  CLI tests (`CHG0107955`, `INC0302574`, `INC0299001`; the GRNOC fixture
  `INC0301970` drives the offline blocked-plan path).
- **Source:** publicly published operational tickets.
  - Internet2 tickets: public GRNOC task records (title/window/type only,
    reformatted as minimal JSON).
  - `INC0301970`: public GRNOC Public Task Viewer record
    (`https://grnoc.iu.edu/tasks/INC0301970`), represented as the generic
    `GnocRecord` shape.
- **Original author/project:** Internet2 / Indiana University GlobalNOC
  (operational announcements; no code involved).
- **Exact or minimized:** minimized — only the published fields used by
  the pipeline (id, title, start/end, timezone, description) are retained.
- **Upstream license:** not licensed as code; public operational
  announcements. Use is limited to testing parsing/planning behavior on
  public data.
- **Modified:** reformatted into the minimal fixture shape.
- **Why redistribution is permitted:** public operational announcements
  about network events; the fixtures contain no private or authenticated
  GRNOC data (see the secrets audit).
- **Packaging:** **include** — required for CLI/planning tests.

### 3b. Public Task Viewer response fixtures — `grnoc/viewer/*.json`

- **Purpose:** drive viewer-response parsing tests (Session 33,
  Part 1): envelope parsing, lossless raw-value preservation, unknown
  field tolerance, per-item failure isolation.
- **Source:** the GlobalNOC Public Task Viewer JSON responses captured
  during the protocol audit on 2026-08-01
  (`https://ticket-viewer.grnoc.iu.edu/api/get_incidents` and
  `/api/get_change_requests`; see
  `docs/sources/GRNOC_PUBLIC_TASK_VIEWER.md`).
  - `INC0227937.json` — public incident record (AMPATH circuit, 2025).
  - `INC0301970.json` — public incident record (Indiana GigaPOP peer,
    2026), matching the existing generic `grnoc/INC0301970.json`.
  - `CHG0038258.json` — public change-request record (MAN LAN core node
    maintenance, 2019) with planned/actual windows and maintenance type.
  - `malformed.json` — deliberately truncated envelope exercising the
    per-item failure path (not a real response).
- **Original author/project:** Indiana University GlobalNOC (public
  operational announcements).
- **Exact or minimized:** exact copies of the JSON responses as
  received (field order normalized by JSON serialization).
- **Upstream license:** public operational announcements; no code
  involved.
- **Modified:** no content changes; only serialization formatting.
- **Why redistribution is permitted:** the records are public tickets
  from a public viewer, retrievable by anyone without authentication;
  they contain no private notes, contact data, or authenticated content.
- **Packaging:** **include** — required for the viewer-response parsing
  tests to run from a packaged crate.

### 3c. RIS format-compatibility note

No separate RIS MRT fixture is committed: RIPE RIS publishes the same
BGP4MP MRT format as RouteViews, so `mrt/update-example.gz` (section 2)
is upstream-format-compatible with RIS `updates.*.gz` archives. RIS
archive-SELECTION behavior (URLs, cadence, bview naming, gzip
compression) is exercised in `src/catalog/archive_plan.rs` against the
planner's family-aware URL builder; see
`docs/ADRs/RIPE-RIS-SUPPORT.md`.

### 4. Generated expected-output fixtures (none committed)

Expected outputs are asserted in code (report wording, exit codes,
schema versions), not stored as golden files. Nothing to package.

## Packaging summary

All committed fixture families are small, public, and required by the test
suite, so the whole `tests/fixtures/` tree ships in the crate package.
Raw MRT archives and derived event outputs are never committed under
`tests/fixtures/`; live analysis data lives only in `cache/` (gitignored)
and `out/` (excluded from the package).

## Claim of authorship

The fixture JSON shapes and the test harness around them are original inim
code (MIT). The upstream MRT bytes and the public ticket facts are not
inim's own work and retain their own provenance above.
