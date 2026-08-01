# ADR-002: Local event catalog and first web interface

**Status:** Accepted
**Date:** 2026-07-31
**Session:** 29

## Context

inim's analysis engine and reports are mature, but the product is still a
CLI-only tool. The intended primary analyst experience is a local web
application; the CLI remains an administration, automation, and debugging
interface.

The missing product layer is a **persistent local event catalog**: a
source-neutral store of events, immutable source snapshots, reviewed
manifest revisions, plans, analysis runs, and artifact references — with a
read-only localhost web UI and a small stable JSON API.

## Decisions

### SQLite is the initial catalog database

**rusqlite 0.40** (with the `bundled` feature, so no developer-local
SQLite install is required) is the catalog database library.

Rationale versus sqlx:

- The catalog is local, single-process, and small. sqlx's async
  connection pools and compile-time query checks add complexity without
  benefit here.
- rusqlite is synchronous and simple; in the async web handlers the
  catalog connection is held behind a `std::sync::Mutex` with a busy
  timeout. Queries are metadata-sized.
- The project deliberately avoids ORMs; explicit SQL keeps migrations and
  invariants (uniqueness, immutability) visible.

No PostgreSQL, Redis, external job queue, or SPA framework is introduced.

### Large immutable data remains in the filesystem

Raw MRT archives, derived caches, detailed evidence appendices, and
generated reports stay on disk. SQLite stores **identities, revisions,
status, searchable metadata, summaries, and artifact paths with hashes**.
Raw MRT blobs and individual BGP observations are never stored as rows.

### The catalog core is source-neutral

`CatalogEvent` / `EventSnapshot` / `ManifestRevision` / `AnalysisPlan` /
`AnalysisRun` / `AnalysisArtifact` / `StreamLifecycleSummary` /
`SemanticWaveSummary` / `CatalogSyncRun` carry no network-specific
knowledge. GRNOC title conventions live in the source adapter, not the
generic catalog layer.

### GRNOC Public Task Viewer is the first EventCatalogSource adapter

The existing GRNOC ingestion code (`sources::grnoc`) is wrapped by the
first `EventCatalogSource` implementation. Synchronization only populates
and updates the catalog; it never starts planning or analysis and never
infers reviewed ASN mappings from names.

### The web UI is server-rendered and local-first

Axum serves server-rendered HTML with Askama templates and a small
project-owned CSS file embedded in the binary (no CDN, no external fonts,
no analytics). The default bind is loopback-only (`127.0.0.1:8080`);
non-loopback binds require an explicit flag and print a warning that the
initial application has no authentication. The first web version is
**read-only**: HTTP requests never perform Broker discovery, downloads,
MRT parsing, or analysis.

### The CLI remains available for administration and automation

`catalog init`, `catalog import`, `catalog sync grnoc`, and `serve`
remain CLI commands; analysis stays on the CLI (and, in the future, a
worker process).

### Observations are associated through AnalysisRun, not directly with an Event

Direct observation-to-ticket association is unsafe because:

- source tickets may be **edited** (an old observation could be
  retroactively attached to a changed ticket);
- one event may have **several reviewed manifests** (review evolves);
- one event may be **analyzed several times** (re-runs must not mutate
  earlier evidence);
- **overlapping events** may observe the same BGP transition (a global
  route-state table cannot be partitioned by ticket);
- **temporal proximity does not establish causation** — attaching
  observations directly to a ticket implies a causal link the data cannot
  support.

Therefore evidence belongs to an immutable `AnalysisRun` that references
an exact `EventSnapshot`, an exact `ManifestRevision`, and an exact
`AnalysisPlan`. The catalog may show "observed during this
event-conditioned analysis"; it never states "caused by this event".
Future cross-event correlations must be modeled as explicit hypotheses
with provenance and status, not as direct foreign keys.

## Future design: analysis jobs

The web process will never execute analysis inside an HTTP request.
Future web-triggered analysis uses a persistent job table and a separate
worker:

- `AnalysisJob`: Queued → Running → Complete / Failed / Cancelled
- the web process creates jobs;
- a separate worker process claims and executes jobs.

The worker queue is not implemented in this session; catalog import does
not require it.

### Update (Session 30)

The catalog schema moved to **v2** with the case-study layer (see
`docs/ADRs/CASE-STUDY-LAYER.md`): case-study tables plus `run_transitions`
(imported from the new `transitions.json` artifact) and the
`reference_documents`/`document_revisions` pair. The web layer gained
read-only case-study pages/API and validated document serving; the
"no analysis on any request path" property is unchanged.
