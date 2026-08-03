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
- **Source adapters**: the GRNOC Public Task Viewer is the first
  `EventCatalogSource` adapter (`src/catalog/grnoc.rs`, viewer client
  `src/catalog/grnoc_viewer.rs`); ticket parsing adapters live under
  `src/sources/` with reviewed network profiles (`src/profiles/`,
  `src/conventions/`). Sync populates the catalog only and never starts
  analysis.
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
│          plan / analyze / compare / catalog / serve      │
│                  migrate-manifest                        │
└─────────────────────┬───────────────────────────────────┘
                      │
┌─────────────────────▼───────────────────────────────────┐
│                   Orchestration                          │
│     plan_from_manifest → discovery → pipeline →          │
│     reconstruction → lifecycle → waves → assess → out    │
└──┬────────┬────────┬────────┬────────┬────────┬─────────┘
   │        │        │        │        │        │
┌──▼──┐ ┌──▼──┐ ┌──▼──────┐ ┌──▼──┐ ┌──▼───┐ ┌──▼─────┐
│ BGP │ │toknz│ │SEQUITUR │ │Waves│ │Assess│ │ Report │
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
│  internet2/ (ticket), grnoc (record), profiles/,          │
│  conventions/ — source-neutral core, reviewed profiles    │
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
    │              Semantic waves (from event-phase transitions)
    │                         │
    │              Assess ──► EventAssessment (verdict, evidence)
    │                         │
    │              Outputs ──► report/evidence/lifecycle/wave/audit JSON
    ▼
AnalysisOutcome (completed | insufficient_visibility | incomplete)
```

Blocked plans (`AnalysisPlanStatus::Blocked`) stop before acquisition:
zero Broker calls, zero MRT parses, exit `EXIT_ANALYSIS_BLOCKED` (code 3).

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

### Why the CLI analysis path is synchronous while the web layer is async

The analysis path is a bounded, deterministic pipeline over local
archives; it runs on worker threads with explicit job counts so artifacts
are identical at any concurrency level. The web layer is a separate
async server (Axum over Tokio) that never runs analysis — it only reads
the catalog and renders view models. The two runtimes never mix; there is
no async analysis path.

### Why crate boundaries as modules, not workspace crates

The codebase is small enough that multiple crates add build overhead
without meaningful isolation. Internal modules with `pub` visibility
provide sufficient boundaries. Separate crates can be extracted later if
independent versioning or compilation becomes desirable.

### Why SEQUITUR as a standalone module

SEQUITUR must have no BGP-specific knowledge. It operates on abstract
symbol sequences. This makes it independently testable with property
tests (round-trip, digram uniqueness, rule reuse) and reusable for
non-BGP sequence analysis. SEQUITUR describes repeated sequences; it
never assigns semantic labels and never determines the assessment.

### Why parenthesized convention is Internet2-specific

Other networks do not necessarily use this convention. The core domain
model must be source-neutral. The Internet2 adapter explicitly documents
this as an Internet2 naming convention with provenance tracking. Other
adapters (GRNOC records, the Indiana GigaPOP profile) implement their own
expectation derivation without misunderstanding this as a universal rule.

## Evidence storage layout

- **Raw caches**: MRT archives under `<cache>/raw/`, keyed by URL, with
  a SHA-256 sidecar written at download time and verified on cache reuse;
  writes are atomic (temp + rename).
- **Derived caches**: RIB parsing results under `<cache>/rib/<key>.json`
  and UPDATE parsing results under `<cache>/derived/updates/<key>.json`,
  versioned and keyed on (family, collector, archive identity).
- **Source-extraction cache**: `cache/extracted/<key>.json.gz`, keyed by
  (source sha, family, collector, sorted origin set) — predicate-
  independent, so plane-specific runs and audits parse each RIB once.
- **Evidence store**: every completed run writes report.txt, report.json,
  archive_manifest.json (URL + SHA per archive), evidence_appendix.jsonl,
  semantic_waves.json, withdrawal_audit.json, lifecycle.json,
  transitions.json, limitations.json, and performance.json. The catalog
  records each artifact's relative path and hash.

## Case-study layer

- **Schema v2** adds the case-study tables (`case_studies`,
  `case_study_event_links`, `reference_documents`, `document_revisions`,
  `case_study_document_links`, `case_study_phases`, `case_study_analysis_links`,
  `case_study_claims`, `case_study_targets`, `case_study_analysis_plans`,
  `run_transitions`). Foreign keys use SQLite's default NO ACTION —
  integrity is enforced by application-level import transactions.
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
  horizon (2h warmup / incident / 2h cooldown), expected RouteViews and
  RIPE RIS RIB+update files per the family's cadence (RIS `bview` on the
  8-hour grid, `updates` every 5 minutes) with estimated sizes (flagged as
  estimates), blocked targets with reasons, `Draft` status until reviewed.
- **Phase summaries**: one run summarized by reviewed phases; transitions
  assigned to exactly one phase by time; stream visibility walked
  continuously across the run (no baseline reset); counts are
  observer-stream counts; outside-phases bucket reported explicitly.
- **Comparison matrix**: operator claims (first five categories) × BGP
  observation from linked runs; labels Before/During/After/Overlapping/
  NoObservedCounterpart/NotDirectlyObservable/Indeterminate; never
  ConfirmedCause; no linked runs → Indeterminate ("not yet executed").
- **Web/API**: `/case-studies`, `/case-studies/{slug}`,
  `/case-studies/{slug}/workbench`, `/case-studies/{slug}/timeline`,
  `/case-studies/{slug}/comparison`, `/documents/{id}`, plus the
  matching `/api/v1/case-studies[...]` read-only endpoints. Document
  serving validates record existence, catalog-relative path, canonical
  containment under the catalog root, SHA-256 match, and media allowlist
  (inline only for approved types).

## Historical validation and pilots

- **Reconstruction contract**: one baseline RIB (latest at or before
  warmup_start) establishes initial state; the UPDATE sequence covers
  [warmup_start, cooldown_end] at the archive's cadence; one optional
  post-window validation RIB is a continuity checkpoint only and is never
  replayed as event input. Interval RIBs are not selected as repeated
  baselines. The event baseline is frozen at the first event-period
  observation; warmup-phase updates are silent (no state change is
  emitted); cooldown-phase transitions are classified separately and
  never change the event-window verdict.
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

## Concurrency and performance

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
- **Worker selection**: the default parse concurrency is a fixed 8
  (`--parse-jobs` default), informed by the local raw-cache benchmark;
  `--parse-jobs` and `--download-jobs` override; `--jobs 0` is rejected.
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

## Corpus workspace

The corpus layer sits beside the case-study layer in the catalog and
never changes lifecycle semantics:

- **Acquisition** (`catalog/access.rs`, `catalog/grnoc_viewer.rs`): a
  paced, budget-bounded HTTP client (default ceiling 5 requests/second,
  smooth limiter, burst 2, max 5 in-flight; adaptive response to
  429/Retry-After/403/503) drives exact ticket lookups against the public
  viewer. Discovery provenance (`catalog/discovery.rs`) records how each
  identifier entered (`AnalystSeed`, `DocumentReference`,
  `TicketDescriptionReference`, `PublicSearchResult`, `CaseStudyReference`).
  There is no blind numeric-ID enumeration and no "download everything"
  mode. Conditional requests (ETag/If-None-Match, If-Modified-Since) are
  honored; a 304 never creates a new snapshot.
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
  RIS execution (download + parse) is supported end-to-end through the
  same evidence-bearing engine (see `docs/ADRs/RIPE-RIS-SUPPORT.md`).

Design invariants: HTTP GET never crawls or analyzes; corpus
completeness is never assumed; source snapshots are immutable; explicit
and inferred relationships stay distinct; reviewed mappings remain
human-controlled; RouteViews/RIS are observer sources, not ground
truth; temporal correlation is not causal attribution.

## Reviewed multi-observer analysis

- **Reviewed interpretation layer** (`ticket_reviews`, V7): analyst
  review is a separate stage over immutable snapshots. Roles, entity
  labels, linked changes, applicability, and per-field provenance
  (snapshot field or cited AAR). Import: `inim catalog corpus-review`.
  Graph audit: `inim catalog relationships audit` + `/corpus/relationships`.
- **Candidate noise reduction**: per-pair candidates with union
  evidence; temporal overlap alone is `TemporalCoincidence`, hidden from
  the default queue but queryable; rejected fingerprints stay
  suppressed; superseded Unreviewed rows are merged (provenance
  preserved in the merged row).
- **RIPE RIS execution**: manifests carry `source_family`
  (default RouteViews); the orchestrator discovers through the family's
  broker project; derived caches are keyed on (family, collector);
  reports name the family; mixed-source archive ordering is a total
  order (ts_start, url). One real 2019 RIS fixture exercises the shared
  ingestion path.
- **RIS collector selection**: metadata probe + RIB preflight per
  candidate (`bview` on the 8-hour grid); selection requires qualifying
  visibility, peer/geographic diversity, manageable volume, complete
  coverage; rejected collectors are recorded with reasons.
- **Independent per-collector pilots**: each selected collector runs its
  own AnalysisRun with its own evidence; no merged verdict.
- **Cross-observer comparison** (`observer_compare`): per-prefix ×
  collector rows over linked runs; bounded statement vocabulary;
  timing differences preserved; no global-confirmation phrasing.
- **Analyst queue**: rows show reviewed role, archive-plan status, run
  count, and a derived next action; nothing executes from an HTTP GET.

## Plane-aware analysis

- **Profile data, generic logic**: named service planes and ASN roles
  live in `case-studies/<slug>/pilot/network-profile.json`
  (`NamedServicePlane`, `ReviewedAsnRole`); `netprofile.rs` classifies
  sessions generically (direct peer vs AS-in-path vs other vs
  ambiguous). Production code contains no operator-specific branch.
- **Session audit**: `inim catalog session-audit` parses baseline RIBs
  once (via the source-extraction cache) and emits per-peer historical
  facts from the MRT header — the source of truth for session
  relationships; current peer lists are supporting context only.
- **Cohort vs classification**: manifests carry `transit_predicate`
  (cohort selector) and optional `path_classifiers` (named predicates
  for classification only); `inim analyze --origin-inventory` classifies
  every origin-matching baseline route one/both/neither without a
  verdict.
- **Cross-observer matrix**: per-observer rows with location, peer ASN,
  direct/indirect relationship, plane, cohort predicate, and
  departures/returns; evidence stays per-run; a missing plane baseline
  is reported as missing, never as "no change".

## NOC incident workbench

### One reusable presentation model

`IncidentWorkbenchViewModel` (src/catalog/workbench.rs) is the single
derived presentation model shared by the web workbench, the text report
(`inim catalog workbench`), and the JSON API. Templates never
recalculate counts. It is not tied to any ticket identity — it can be
reached from an event, a case study, or eventually a network.

- **ObserverEpisode** — streams at one observer session (collector +
  peer) sharing a meaningful, temporally coherent signature. Effect
  kinds (`TemporaryStreamAbsence`, `RouteWithdrawal`,
  `PathReplacement`, `NamedPlaneDeparture`, `NamedPlaneReturn`,
  `PrependChange`, `MixedRouteChange`, `NoRouteStateChange`) are
  presentation-level groupings derived from existing lifecycle/
  transition evidence; they introduce no new route-transition
  semantics.
- **Coverage states** — `Complete` (a qualifying baseline existed and
  the session was observed), `NoBaselineVisibility` (target not
  visible), `IncompleteCoverage` (observation could not be completed).
  Coverage describes whether the observation could be made; "no
  route-state change" is an OBSERVED SIGNATURE
  (`EffectKind::NoRouteStateChange` with `Complete` coverage), never a
  coverage state. Never collapsed into one zero.
- **Shared output surface** — HTML workbench (NOC HCI), plain-text
  report, JSON API: same model, same counts.

### Presentation semantics

- **Expectation assessment** comes from the first completed run's
  `assessment` field (e.g. "Consistent with the redundant-attachment
  expectation."); the manifest `target.label` is a title and is never
  rendered as an assessment. Case studies have no incident-wide
  expectation: the model states this explicitly.
- **Observed result** on the first screen is generated from model
  counts (`render_observed_result`): changed/eligible observer sessions
  and changed/baseline streams, with no-baseline sessions reported
  separately. Case studies additionally show a scope limit ("single
  target historical pilot, not a complete incident assessment") and
  state that no incident-wide BGP verdict has been performed.
- **Episode rows split three concepts**: observed signature
  (EffectKind), end state (`EndState`, derived ONLY from lifecycle
  evidence — withdrawal/restoration flags and exact restoration
  timestamps; changed episodes can never end in "no change"), and
  coverage status (Complete / NoBaselineVisibility / IncompleteCoverage).
- **Restoration** is derived from the immutable `restoration_time_utc`
  for every changed stream (visibility restoration for withdrawals,
  exact event-baseline restoration for path changes) — never from an
  optional presentation field, never extrapolated.
- **Regions** are canonical keys (AMER/EMEA/APAC/Unknown); coverage-only
  sessions carry their region resolved at load time, so a collector id
  can never render as a region.
- **Peer identity**: an observed peer ASN renders as `AS<n>`; a missing
  ASN renders "peer ASN not in reviewed evidence". Organization and role
  labels are reviewed/unclassified concepts — the ASN itself is never
  "unreviewed".
- **Human labels** (`EffectKind::human_label`, `EndState::human_label`,
  `CoverageStatus::human_label`) replace raw enums and predicate JSON in
  the primary UI; raw labels and exact timestamps remain in the JSON API
  and expanded details.

### RoutingFinding derivation

The primary workbench unit is a **RoutingFinding** (src/catalog/
workbench.rs): one coherent routing story per observer session. Findings
are derived from existing lifecycle/transition evidence — they introduce
no new route semantics.

- **Grouping**: streams at one (collector, peer, EffectKind) whose
  chronologies are materially the same (same baseline path and first
  changed path, multiplicity-aware) form one finding; streams with
  different chronologies never share a finding.
- **Split episodes**: a stream with both a withdrawal and an earlier
  visible path change yields TWO findings — a visible prepend/path
  change and a separate absence finding — linked by an `EarlierChange`
  reference. The earlier-change link is rendered only when the canonical
  pre-withdrawal transition is a real prepend delta on the target
  origin; a related route transition alone is not a prepend change.
- **Exact paths**: baseline path, first changed path, final observed
  path per prefix, with the event baseline kept distinct from the
  pre-finding state. Summary rows collapse repeated ASNs (`AS24489×4`);
  the exact uncollapsed sequence is retained in drill-down and the JSON
  API.
- **Chronology**: `route_chronology` renders ordered steps (event
  baseline, pre-finding route, absent, first route after return, later
  changes, event-window end, analysis end) with exact timestamps; the
  pre-finding route is never labeled "baseline" when the event baseline
  differs from it.
- **Audit commands**: `inim catalog finding-audit` and
  `inim catalog finding-chronology-audit` write the exact record the
  prose renderer uses, read from the canonical lifecycle artifact.

### NOC HCI

Dense operations-console styling: rectangular panels, square corners,
thin borders, strong section headers, compact line height, monospaced
timestamps/AS paths, sortable tables with fixed headers, explicit text
status, restrained colors, underlined links, visible focus states,
keyboard-accessible controls. Server-rendered; small progressive
enhancements (sort, expand, copy) only; no SPA framework. The principal
result is fully understandable without JavaScript. Mobile (≤640px)
keeps result+scope on top, uses horizontally scrollable tables and
definition-list episode rows, and never breaks
timestamps/ASNs/prefixes.

### Timeline

One lane per observer session on a shared UTC axis, plus an operator
context strip whose axis spans the anchors' exact extent; the BGP focus
timeline holds the observer lanes and only in-window anchors. Markers
carry exact timestamps: analysis-window boundaries, operator-reported
anchors (visibly distinct kind), first route change, absence interval,
path-change interval, restoration interval, unresolved end state. No
interpolation between discrete BGP observations; unresolved episodes get
no fabricated restoration; lane baselines are strictly horizontal.

### Prefix drill-down

Episode expansion groups member streams with prefix, category,
withdrawn/restored, baseline instances, transition count, ADD-PATH
ambiguity, and evidence references. It reads catalog summaries and
immutable evidence artifacts only — raw MRT files are never loaded.

### Counting units

- **ObserverSessionKey** = (source family, collector, peer IP, address
  family derived from the peer IP literal). Regional breadth counts
  UNIQUE session keys for eligible/changed/unchanged — episodes never
  inflate the session denominator (UVA: 7 episodes → 4 sessions; MAN
  LAN: 10 sessions).
- **Streams** keep the peer dimension ((collector, peer, prefix));
  **distinct prefixes** are set unions per region and across regions
  (MAN LAN: 58 changed streams → 12 distinct prefixes; UVA: 48 streams
  → 12). Regional unions are never summed into a global count.
- **Route instances** include ADD-PATH instances
  (max_active_instances); **transitions** come from the run transition
  index (0 when the artifact is absent — never guessed).
- The VM exposes a machine-readable `units` block
  (session/episode/stream/prefix/route-instance/transition counts) in
  the page, the JSON API (`/api/v1/events/{id}/workbench`,
  `/api/v1/case-studies/{slug}/workbench`), and the text report.

### Coverage reasons

Excluded sessions carry a `CoverageReason`:
EligibleWithBaseline / SessionPresentNoTargetBaseline /
RequiredSessionAbsent / PredicateNotMatched / ArchiveIncomplete /
UnsupportedSource, plus the exact preflight evidence detail. RRC11's
I2PX pilot exclusion is `RequiredSessionAbsent` ("no direct session in
the historical baseline"), distinct from "session present, no target
baseline"; excluded sessions never enter the eligible denominator.

### Observed peer-session metadata

`observer_session_metadata` (V9): observed peer ASN per (collector,
peer IP, address family), time-scoped by the RIB timestamp, with the
source archive and SHA. `inim catalog session-metadata-backfill
--cache DIR:FAMILY --date YYYYMMDD` records observations from cached
baseline RIBs (idempotent). The workbench renders the observed ASN as a
protocol fact ("ASxxxx · organization unclassified · role unclassified")
or explicit ambiguity; it is distinct from reviewed organization labels
and never part of RouteKey identity.

### Window-end vs cooldown

Episodes separate `state_at_event_window_end` (EndState) from
`cooldown_outcome` (analysis_end = window end + cooldown minutes):
RestoredAt / StillChangingBeforeAnalysisEnd /
NoRestorationBeforeAnalysisEnd. The regional column is named **LAST
IN-WINDOW RESTORATION**.

### INC0302574 I2PX relationship audit

Event-date RIS baselines (bview.20260730.0000.gz, RRC11 + RRC14 — the
collectors with direct AS11164 peers per current peer lists) show the
direct sessions existed but carried zero AS3333-origin routes, and no
AS3333-origin path contained AS11164. Reviewed audit artifact
`case-studies/inc0302574/out/INC0302574/relationship-audit.json` records
the bview SHAs, the four direct sessions, and the visibility counts; decision
`insufficient-visibility`. The workbench assessment uses only
relationship-relevant evidence; the existing AS11537 run is classified
`supporting-re-plane`; page and API agree.

### Performance

Workbench GETs perform no analysis and no MRT parsing. Queries use the
existing run indexes (`idx_streams_run`, `idx_run_transitions_run`,
`idx_waves_run` — verified with EXPLAIN QUERY PLAN); no new indexes were
needed. Per-request SQL count and timings are captured; demo catalog
renders in ~16–23 ms median (target: <100 ms median, <250 ms worst).

## Durable analysis jobs and the worker boundary (ADR-004)

The web server is read-only by default and never executes analysis. A
reviewed immutable plan revision can be queued (web or CLI) into
`analysis_jobs`; a separate `inim worker` process claims jobs
transactionally, executes them through the shared execution service
(`src/execution.rs`), stages artifacts under `data/jobs/<job-id>/`,
validates them, and publishes completed runs atomically under
`data/runs/<job-id>/`. Job state is execution state and is distinct
from plan status and analysis outcome. See `docs/OPERATIONS.md` and
`docs/ADRs/DURABLE-ANALYSIS-JOBS.md`.

## Reviewed corpus import

The offline demo and any catalog can import a bounded reviewed corpus
(`case-studies/manlan-2019/corpus/`) deterministically: events with
immutable snapshots, the reviewed relationship graph, and per-ticket
reviewed roles — never Ready plans and never jobs. A corpus ticket
that already has a reviewed analysis plan is represented once by its
reviewed analysis event (its corpus row is skipped and relationship
edges resolve cross-source). Source snapshots are public records with
documented redistribution; raw MRT never enters the corpus.
