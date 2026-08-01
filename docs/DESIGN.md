# inim — Design

## Project vision

inim is a reproducible, event-conditioned BGP observation system. It tests
operator-declared expectations against route behavior visible at selected
public collectors.

The central analytical unit is an **observer-prefix stream lifecycle
conditioned on a reviewed event manifest**. The core contribution is not
that any single component decides outage meaning:

- SEQUITUR is a **descriptive aid** for repeated transition sequences —
  it never assigns semantic labels and never determines the assessment.
- The value comes from the composition of:
  - reviewed operational expectation
  - deterministic observer cohort
  - exact route-instance evidence
  - observer-stream lifecycle reconstruction
  - event-relative transit interpretation
  - evidence-scoped assessment
  - separation of impact from mechanism

Conceptually minimal: one event, one reviewed observation plan, one frozen
cohort, one reconstructed lifecycle per observer-prefix stream, one
evidence-scoped assessment. Implementation complexity exists to preserve
correctness, provenance, and reproducibility.

## Local catalog and web application

The intended primary analyst interface is a local web application; the CLI
remains the administration, automation, and debugging interface.

- **Catalog** (`src/catalog/`): source-neutral identities — catalog
  events, immutable source snapshots, reviewed manifest revisions, exact
  analysis plans, immutable analysis runs, artifact references with
  hashes, stream lifecycle summaries, semantic wave summaries, sync
  runs.
- **SQLite** (rusqlite, bundled): identities, revisions, status, searchable
  metadata, summaries, artifact paths. Raw MRT archives, derived caches,
  evidence appendices, and reports remain on the filesystem.
- **GRNOC Public Task Viewer** is the first `EventCatalogSource` adapter;
  sync populates the catalog only and never starts analysis.
- **Web server** (Axum + Askama): server-rendered, loopback-only,
  unauthenticated initially, read-only — HTTP requests never perform
  Broker discovery, downloads, MRT parsing, or analysis.
- **Association**: evidence belongs to an immutable `AnalysisRun` that
  references an exact snapshot, manifest revision, and plan. There is no
  `Observation.event_id` and no `RouteTransition.ticket_id`; the catalog
  never states that an observation was "caused by" an event.

See `docs/ADRs/LOCAL-CATALOG-AND-WEB.md` for the full decision record.

## Architecture overview

```
┌─────────────────────────────────────────────────────────┐
│                     CLI (main.rs)                        │
│                 clap derive, subcommands                  │
└─────────────────────┬───────────────────────────────────┘
                      │
┌─────────────────────▼───────────────────────────────────┐
│                   Orchestration                          │
│    (future: analyze, compare, list-waves commands)       │
└──┬────────┬────────┬────────┬────────┬────────┬─────────┘
   │        │        │        │        │        │
┌──▼──┐ ┌──▼──┐ ┌──▼──────┐ ┌──▼──┐ ┌──▼───┐ ┌──▼─────┐
│ BGP │ │Toknz│ │SEQUITUR │ │Waves│ │Assess│ │ Report │
│ MRT │ │     │ │grammar  │ │     │ │      │ │        │
│ RIB │ │     │ │inference│ │     │ │      │ │        │
└──▲──┘ └─────┘ └─────────┘ └─────┘ └──────┘ └────────┘
   │
┌──┴──────────────────────────────────────────────────────┐
│                    Domain types                           │
│  Event, Expectation, Entity, Route, Transition, Wave,    │
│  Assessment, Evidence, Verdict                            │
└──────────────────────────────────────────────────────────┘
                      ▲
┌─────────────────────┴───────────────────────────────────┐
│               Sources (adapters)                          │
│  internet2/ — ticket parsing, expectation derivation     │
│  (future: other-network/)                                 │
└──────────────────────────────────────────────────────────┘
```

## Data flow for `analyze` command

```
Ticket fixture (JSON)
    │
    ▼
Ticket/GRNOC adapter ──► EventId + ImpactExpectation
                            │
Reviewed manifest (JSON) ───┤
    │                        ▼
    ▼                 plan_from_manifest (BEFORE acquisition)
AnalysisPlan                 │
    │                         │ Ready?
    │                         ▼
    │              Broker discovery + archive caching
    │                         │
    │              RIB derived-cache lookup ── hit? skip MRT parse
    │                         │
    │              RIB preflight ──► FrozenCohort (ObserverPrefixKey set)
    │                         │
    │              UPDATE derived-cache lookup ── hit? skip MRT parse
    │                         │
    │              deterministic sort + observation ID assignment
    │                         │
    │              Route reconstruction (RouteStateStore, RouteKey)
    │                         │
    │              Tokenize ──► RouteTransition + effects
    │                         │
    │              Waves ──► ImpactWave list (+ SEQUITUR motifs)
    │                         │
    │              Lifecycles ──► StreamLifecycle per ObserverPrefixKey
    │                         │
    │              Semantic waves (from lifecycles + evidence)
    │                         │
    │              Assess ──► EventAssessment (verdict, evidence)
    │                         │
    │              Outputs ──► report/evidence/lifecycle/wave/audit JSON
    ▼
AnalysisOutcome (completed | insufficient_visibility | incomplete)
```

Blocked plans (`AnalysisPlanStatus::Blocked`) stop before the dashed
branch: zero Broker calls, zero MRT parses, exit `EXIT_ANALYSIS_BLOCKED`.

## Key design decisions

### Why Rust

- Explicit memory model, no GC pauses
- Strong type system for domain modeling
- Zero-cost abstractions for iterators and streaming parsers
- Single static binary for deployment
- Cargo ecosystem includes MRT parsing crates when needed

### Why planning precedes acquisition

Every analysis starts from a reviewed manifest. Planning validates the
reviewed entity mapping and transit predicate before any network activity,
so blocked plans are cheap, safe, and deterministic. Blocked planning is
a plan status, not an `AnalysisOutcome` — outcomes describe completed or
failed observation, never pre-acquisition refusal.

### Why the principal lifecycle identity is ObserverPrefixKey

BGP ADD-PATH means one observer may advertise multiple route instances
(`RouteKey` with distinct `path_id`) for one prefix. Routing impact is
experienced at the observer-prefix stream level, so lifecycles are keyed
by `ObserverPrefixKey` while every instance's history is retained. Stream
absence requires the loss of the **final** instance; path_id churn alone
is not routing impact.

### Why canonical manifests use TransitPredicateMapping

A single-ASN shortcut cannot express the reviewed predicates the event
model supports (`ContainsAny`, `ContainsAll`, `Adjacent`). Canonical
manifests carry status + predicate + provenance. Legacy shortcut fields
are rejected with `LegacyManifestRequiresMigration` and converted only by
the offline `migrate-manifest` helper, which requires analyst-confirmed
provenance and never invents unresolved ASNs.

### Why schema versions are explicit and old versions are rejected

Identity semantics changed with ADD-PATH awareness. Reusing an old schema
number would silently reinterpret pre-ADD-PATH caches. Every persisted
format (manifest, RIB cache, UPDATE cache, cohort identity, report,
evidence appendix, lifecycle, withdrawal audit, semantic wave,
comparison, analysis plan) carries an explicit version; mismatches are
rejected and rebuilt/archived rather than parsed as current.

### Why mechanism hints are separate from routing impact

Route-state changes are evidence of impact. RFC 8326 GSHUT communities
may propagate without causing route changes, and their absence proves
nothing. Mechanism hints (RFC 8326 observations, RFC 9003 / RFC 8327 /
Graceful Restart non-observability statements) are reported in a separate
section and can never change the impact assessment by themselves.

### Why async is deferred

The current scope (local CLI processing one ticket + bounded MRT files)
does not require network concurrency. Async adds complexity in error
handling, debugging, and trait ergonomics. It can be introduced later if
live scraping or streaming becomes necessary.

### Why crate boundaries as modules, not workspace crates

The initial codebase is small enough that multiple crates add build
overhead without meaningful isolation. Internal modules with `pub`
visibility provide sufficient boundaries. Separate crates can be
extracted later if independent versioning or compilation becomes
desirable.

### Why SEQUITUR as a standalone module

SEQUITUR must have no BGP-specific knowledge. It operates on abstract
symbol sequences. This makes it independently testable with property
tests (round-trip, digram uniqueness, rule reuse) and reusable for
non-BGP sequence analysis. SEQUITUR describes repeated sequences; it
never assigns semantic labels and never determines the assessment.

### Why parenthesized convention is Internet2-specific

Other networks do not necessarily use this convention. The core domain
model must be source-neutral. The Internet2 adapter explicitly documents
this as an Internet2 naming convention with provenance tracking, so
future adapters for other networks can implement their own expectation
derivation without misunderstanding this as a universal rule.


## Case-study layer (Session 30)

- **Schema v2** adds the case-study tables (`case_studies`,
  `case_study_event_links`, `reference_documents`, `document_revisions`,
  `case_study_document_links`, `case_study_phases`, `case_study_analysis_links`,
  `case_study_claims`, `case_study_targets`, `case_study_analysis_plans`,
  `run_transitions`). All FKs are `ON DELETE RESTRICT`.
- **Transitions artifact**: every completed run writes `transitions.json`
  (compact per-transition records: kind, occurred UTC with evidence
  fallback for absent states, phase, key, effects, observation id). The
  catalog imports it into `run_transitions`; phase summaries are pure
  read-only derivations over `run_transitions` + lifecycle + wave
  summaries.
- **Case-study import** (`case-studies/<slug>/case-study.json`, schema v1):
  transactional, idempotent by (slug, content hash), conflicting immutable
  revision rejected; documents resolved by SHA-256 (metadata-only records
  when the file is absent); related tickets linked to existing catalog
  events or preserved as unresolved references.
- **Document import**: media-type allowlist, SHA-256, basename-only storage
  under `<root>/data/documents/<sha12>/`, catalog-relative paths, best-effort
  PDF page count + Info metadata from raw bytes (no OCR, no external
  dependencies). Same content idempotent; changed content → new revision;
  existing files never overwritten; a later import may attach a missing
  local file (availability update only).
- **Archive planner**: pure computation (no downloads). Reproducible
  horizon (2h warmup / incident / 2h cooldown), expected 2019 RouteViews
  RIB+update files with estimated sizes (flagged as estimates), blocked
  targets with reasons, `Draft` status until reviewed.
- **Phase summaries**: one run summarized by reviewed phases; transitions
  assigned to exactly one phase by time; stream visibility walked
  continuously across the run (no baseline reset); counts are
  observer-stream counts; outside-phases bucket reported explicitly.
- **Comparison matrix**: operator claims (first five categories) × BGP
  observation from linked runs; labels Before/During/After/Overlapping/
  NoObservedCounterpart/NotDirectlyObservable/Indeterminate; never
  ConfirmedCause; no linked runs → Indeterminate ("not yet executed").
- **Web/API**: `/case-studies`, `/case-studies/{slug}`, `/documents/{id}`,
  `/api/v1/case-studies`, `/case-studies/{slug}`,
  `/case-studies/{slug}/timeline`, `/case-studies/{slug}/comparison`.
  Document serving validates record existence, catalog-relative path,
  canonical containment under the catalog root, SHA-256 match, and media
  allowlist (inline only for approved types).

## Historical validation and pilots (Session 31)

- **Reconstruction contract**: one baseline RIB (latest at or before
  warmup_start) establishes initial state; the UPDATE sequence covers
  [warmup_start, cooldown_end] at the archive's cadence; one optional
  post-window validation RIB is a continuity checkpoint only and is never
  replayed as event input. Interval RIBs are not selected as repeated
  baselines.
- **Target research method**: dated evidence hierarchy (operator docs →
  contemporaneous RIR → archived PeeringDB → contemporaneous RouteViews
  observations → reputable dated docs); current metadata is a lead only.
  Origin identity and path predicate are separate questions; a predicate
  (e.g. ContainsAny[11537] for Internet2 transit presence) is a candidate
  until validated by contemporaneous RIB observation.
- **Reviewed research record** (`target-research.json`) is canonical;
  `inim catalog case-study apply-research` updates only research fields of
  matching target rows (documented immutability exception with audit
  timestamp; no new tables).
- **Staged execution**: Stage A = broker + one baseline RIB preflight
  (`--preflight-only`, no updates, JSON contract on stdout); Stage B =
  short-window pilot around one documented boundary; Stage C = full run
  only if Stage B is interpretable. No full download without reviewed
  approval.
- **Run-window coverage in comparisons**: a narrow pilot run can only
  contribute observations (or no-counterpart conclusions) for claim
  windows its own event window intersects; otherwise the comparison says
  Indeterminate ("no linked analysis run covers this window") rather than
  fabricating a negative.
- **Pilot results** are reviewed data (`pilot-result.json`) recorded on the
  plan record and rendered as "Historical pilot — <target>", explicitly
  narrower than the whole incident.

## Concurrency, performance, and review (Session 32)

- **Concurrency boundary**: one compressed archive → parse → normalize →
  ObserverPrefixKey admission → deterministic per-archive chunk →
  per-archive derived cache may run concurrently. Route-state
  reconstruction, tokenization, lifecycle classification, and wave
  derivation run SEQUENTIALLY afterwards on the merged observation stream.
- **Pipeline**: `run_bounded_pipeline` — `download_jobs` workers (default 2,
  conservative) feed a bounded parse channel (capacity = `parse_jobs`);
  downloads and parses overlap. `archive_order` is pre-assigned from
  discovery order before any download; results merge in that order and
  observation ids are assigned after the global deterministic sort, so
  worker completion order never changes artifacts. Each worker owns its
  parser/decompression state. Memory is bounded by
  `(download_jobs + parse_jobs)` in-flight archives plus the merged
  observation vector; no unversioned temporary evidence is ever written.
- **Worker selection**: the default parse count is chosen from the local
  raw-cache benchmark (best safe throughput), not from the host CPU count.
  `--parse-jobs` and `--download-jobs` override; `--jobs 0` is rejected
  (previously "auto").
- **Performance metadata**: `performance.json` (schema v1) carries stage
  timings and per-archive metrics (identity URL+SHA, compressed bytes,
  parse time, elements, admitted observations, cache-write time, cache
  hit). It is a SEPARATE artifact: volatile timings never participate in
  substantive artifact-equivalence checks and never influence the verdict.
- **Screenshot review**: `scripts/screenshot-review.sh` builds the
  deterministic demo catalog state, serves inim on loopback, captures
  fixed-viewport full-page screenshots via an already-installed Playwright
  chromium, shuts the server down (trap on failure), and writes
  `tmp/ui-review/*.png` (gitignored, excluded from the package). Visual
  quality is reviewed externally, never self-certified.

## Corpus workspace (Session 33)

The corpus layer sits beside the case-study layer in the catalog and
never changes lifecycle semantics:

- **Acquisition** (`catalog/access.rs`, `catalog/grnoc_viewer.rs`): a
  strictly serial, paced, budget-bounded HTTP client drives exact
  ticket lookups against the public viewer. Discovery provenance
  (`catalog/discovery.rs`) records how each identifier entered
  (`AnalystSeed`, `DocumentReference`, `TicketDescriptionReference`,
  `PublicSearchResult`, `CaseStudyReference`).
- **Snapshots + per-fetch records**: `event_snapshots` stays pure
  content-addressed immutability; `snapshot_fetches` holds one row per
  HTTP attempt with whitelisted metadata.
- **Relationships** (`catalog/relationships.rs`): explicit edges with
  wording-classified kinds and exact source spans; derived
  temporal-overlap candidates with distinct evidence kinds; bounded
  adjacency traversal; reviewed edges never overwritten.
- **Readiness** (`catalog/analyzability.rs`): derived per event from
  reviewed manifests/plans/runs — separate from lifecycle, sync status,
  and verdict.
- **Grouping** (`catalog/grouping.rs`): candidate incident groups with
  categorical confidence and rejection persistence via evidence
  fingerprints.
- **Batching** (`catalog/batch.rs`): deterministic raw-archive sharing
  across event cohorts; evidence identity never depends on batch
  membership.
- **Observer families**: `SourceFamily` (RouteViews | RipeRis) is part
  of collector identity; RIS planning uses RIPE RIS URLs and cadence;
  RIS execution remains documented-Unsupported (see
  `docs/ADRs/RIPE-RIS-SUPPORT.md`).

Design invariants: HTTP GET never crawls or analyzes; corpus
completeness is never assumed; source snapshots are immutable; explicit
and inferred relationships stay distinct; reviewed mappings remain
human-controlled; RouteViews/RIS are observer sources, not ground
truth; temporal correlation is not causal attribution.
