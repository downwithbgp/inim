# inim — Internetwork Impact Monitor

inim is a reproducible, event-conditioned BGP observation system. It tests
operator-declared expectations against route behavior visible at selected
public collectors (RouteViews).

The central analytical unit is an **observer-prefix stream lifecycle
conditioned on a reviewed event manifest**: one event, one reviewed
observation plan, one frozen observer cohort, one reconstructed lifecycle
per observer-prefix stream, one evidence-scoped assessment. Implementation
complexity exists to preserve correctness, provenance, and
reproducibility.

It does not establish:

- global reachability
- traffic impact
- circuit state
- operator command usage
- causation from temporal association alone

## What inim produces

inim compares an operator-declared event expectation with externally
observable BGP route behavior at selected public collectors. Each
completed report answers: what the ticket implied, what the selected
observers showed, how the two compare, what the observation scope was,
and what the result does not prove.

### Case study: RIPE via NYIIX (INC0302574)

- ticket relationship: direct I2PX peer via NYIIX (the ticket does not
  name the R&E plane)
- event-date (2026-07-30) RIS baselines at RRC11/RRC14: the direct
  AS11164 sessions existed, but zero AS3333-origin routes were visible
  through them and no AS3333-origin path contained AS11164
- **I2PX relationship: insufficient public-collector visibility**
  (reviewed audit; the relationship cannot be assessed with
  public-collector evidence)
- supporting R&E-plane observation (origin AS3333 via the reviewed R&E
  predicate at RouteViews): 19 selected observer-prefix streams, no
  route-state change observed — classified as supporting, never primary
- observer-scoped limitation: the negative R&E finding does not prove
  physical redundancy

### Case study: UVA via Internet2 (INC0299001)

- participant-unavailability expectation
- 4 unique observer sessions (RouteViews peers, observed ASNs from the
  event-date RIB), 7 observer episodes, 48 selected observer-prefix streams,
  12 distinct prefixes (union across peers)
- 13 temporarily absent and later returned
- heterogeneous changes among the remainder (22 prepend-only, 11
  material changes retaining the reviewed transit, 2 departing it)
- **Partial routing impact observed** (PartialImpact)
- the report distinguishes 214 route-instance transitions from the
  13 observer-prefix streams that became absent — a demonstration of
  ADD-PATH-aware stream analysis

A peer event without a reviewed network-path predicate (e.g. INC0301970)
is blocked before archive discovery rather than assigned a speculative
impact verdict.

## Status

- Reviewed, canonical manifests drive every real analysis.
- Blocked planning is a **plan** status, never an `AnalysisOutcome`.
- ADD-PATH-aware identity: route state is keyed by
  `RouteKey` (collector, peer IP, prefix, path_id); stream lifecycles are
  keyed by `ObserverPrefixKey` (collector, peer IP, prefix).
- All persisted formats carry schema versions; old identity semantics are
  rejected, not silently reinterpreted.

## Build / test

```sh
cargo test
cargo fmt --check
cargo clippy --all-targets -- -D warnings
```

## CLI

```
inim plan    --event <ticket.json> --manifest <manifest.json> [--out <dir>]
inim analyze --event <ticket.json> --manifest <manifest.json> [--cache <dir>] [--out <dir>] [--jobs N] [--no-derived-cache|--rebuild-derived-cache]
inim migrate-manifest --input <legacy.json> --output <canonical.json> [--statement ... --reviewed-by ... --date ...]
inim compare --a <event-out-dir> --b <event-out-dir> [--blocked <plan-dir>] --out <comparison-dir>
inim catalog init --db data/inim.sqlite
inim catalog import --db data/inim.sqlite --root .
inim catalog sync grnoc --db data/inim.sqlite --source-dir <dir>
inim catalog document import --db data/inim.sqlite --file <aar.pdf> --source-url <url>
inim catalog case-study import --db data/inim.sqlite --path case-studies/<slug>
inim catalog case-study plan --db data/inim.sqlite --slug <slug>
inim serve --db data/inim.sqlite --root . --bind 127.0.0.1:8080
```

### Local event catalog and web UI

The **web UI is the intended primary analyst interface**. The CLI remains
the administration, automation, and debugging interface.

- The catalog is source-neutral: `CatalogEvent`, immutable
  `EventSnapshot` records, reviewed `ManifestRevision` records, exact
  `AnalysisPlan` records, and immutable `AnalysisRun` records with
  artifact paths and hashes. **GRNOC Public Task Viewer** is the first
  catalog source adapter.
- Source snapshots are immutable — a changed ticket creates a new
  snapshot; reviewed manifests are revisioned; analyses reference exact
  revisions. **Observations and stream lifecycles are associated with an
  AnalysisRun, never directly with a mutable ticket.**
- SQLite stores identities, revisions, status, summaries, and artifact
  paths. Raw MRT archives, derived caches, and detailed evidence remain
  on the filesystem.
- The web server is **localhost-only and unauthenticated** by default
  (loopback bind; non-loopback requires `--allow-non-loopback` and prints
  a warning). HTTP requests **never** perform Broker discovery, MRT
  parsing, or analysis — the web layer is read-only; analysis stays on
  the CLI.
- Sync only populates the catalog: it never starts planning or analysis
  and never infers reviewed ASN mappings from names.

### NOC incident workbench (Session 36)

The **incident workbench** is the dense, operator-oriented analysis view.
It is reachable from an event (`/events/{id}/workbench`) or a case study
(`/case-studies/{slug}/workbench`) and is built from ONE reusable
`IncidentWorkbenchViewModel` shared by the web workbench, the text report
(`inim catalog workbench --db ... --subject <id>`), and the JSON API
(`/api/v1/events/{id}/workbench`,
`/api/v1/analyses/{run_id}/observer-episodes`,
`/api/v1/analyses/{run_id}/regional-breadth`).

The Session 37 revision makes the workbench semantically correct before
visually dense: expectation assessments come from the run assessment
(not the target label), episode rows separate observed signature, end
state, and coverage status (changed episodes never show "NoChange"),
restoration derives from lifecycle evidence, region keys are canonical,
and peer ASNs are observed facts. The UI is a compact incident header
with generated observed-result text, a glanceable breadth matrix, an
observer-episode table with filters and native-`details` drill-downs, a
server-rendered lane timeline with operator anchors, grouped
investigation cues, and a collapsed analysis history — old-school NOC
styling at ~1440px content width with a dedicated mobile layout.
Deterministic query states (`?changed`, `?episode=`, `?prefixes=`,
`?view=timeline`) feed the validated screenshot harness
(`scripts/screenshot-review-session37.sh`).

- **ObserverEpisode** is the primary human-facing unit: streams at one
  observer session (collector + peer) sharing one presentation-level
  signature (`TemporaryStreamAbsence`, `RouteWithdrawal`,
  `PathReplacement`, `NamedPlaneDeparture`, `NamedPlaneReturn`,
  `PrependChange`, `MixedRouteChange`, `NoRouteStateChange`). These are
  groupings of existing lifecycle/transition evidence — no new
  route-transition semantics.
- Every episode renders a precise, data-supported sentence
  (effect-specific verb; collector site and peer identity are separate
  facts; never traffic loss, never causation).
- **Observed breadth by region** (AMER/EMEA/APAC/Unknown) always shows
  the denominator (`changed / eligible observer sessions`).
  `NoChange`, `NoBaselineVisibility`, and `IncompleteCoverage` are
  distinct states and never collapse into one zero. This is observed
  breadth — not outage severity; no severity score exists.
- Observer-site regions and multihop labels are reviewed data
  (`collector-locations.json`); a region classifies the OBSERVER SITE
  only — never the affected network, the route path, the peer
  organization, or the affected users.
- Timeline lanes (one per observer session) use exact timestamps only;
  operator-reported anchors are visibly distinct from BGP evidence; no
  interpolation of unobserved state; an unresolved episode gets no
  fabricated restoration.
- Suggested internal checks are **investigation cues** traceable to
  observed facts (session, interval, plane, prefixes) — never device
  commands, never root cause, never diagnosis.
- NOC HCI: rectangular panels, square corners, thin borders, strong
  headers, compact line height, monospaced timestamps, dense sortable
  tables with fixed headers, explicit text status, underlined links,
  visible focus. Server-rendered; small progressive JS for
  sort/expand/copy only. No SPA.
- Performance: workbench GETs perform **no analysis and no MRT parsing**;
  the main queries use existing indexes (verified with `EXPLAIN QUERY
  PLAN`); the demo catalog renders in ~16–23 ms median.

### Historical RRC11 baseline audit (Session 36)

The RRC11/NYIIX relationship to the NORDUnet 2019-08-21 pilot was
established from the actual historical baseline bview
(`rrc11/bview.20190821.0000.gz`, sha
`37e0f94d60b4b8bd52a9d66c590994d6b2541ae74ec860bb0ee7f38a8fdcd791`),
NOT from the current peer list:

- 39 peer sessions in the baseline (24 IPv4, 15 IPv6); **zero with peer
  ASN 11164** — no direct peering-plane session existed at RRC11 in
  2019, despite the current peer list showing one at NYIIX.
- 106 AS2603-origin routes arrived via 18 other sessions, all with
  neither-plane paths; zero qualifying observer-prefix streams for the
  peering plane.
- Direct I2PX pilot decision: `blocked-no-direct-session` — the exact
  reason is recorded in
  `case-studies/manlan-2019/pilot/rrc11-pex-pilot-decision.json` (and
  `.md`); the pilot was not executed, the target was not broadened, and
  the run was never merged with the R&E-plane runs.
- The full peer-inventory mode (`inim catalog session-audit
  --full-inventory`) streams an entire RIB per session (memory bounded
  by session count, not route count) and reports every peer in the MRT
  peer table.

### Multi-ticket incident case studies

A `CaseStudy` is a reviewed grouping and interpretation of several sources
and analysis runs — **not** a synthetic ticket and **not** an owner of BGP
observations. The association path is
`CaseStudy → AnalysisRun → stream lifecycle → route-instance evidence`.
See `docs/ADRs/CASE-STUDY-LAYER.md`.

- `inim catalog case-study import` reads a reviewed `case-studies/<slug>/`
  data file (documents, phases with `exact`/`summarized` precision,
  related tickets, claims with explicit observability classifications,
  analysis targets). Import is transactional and idempotent; a conflicting
  immutable revision is rejected.
- Tickets that are only referenced by the source document stay unresolved
  document references — no source snapshot is ever fabricated.
- `inim catalog document import` stores an immutable reference document
  (SHA-256, media type, best-effort PDF metadata) under
  `data/documents/<sha12>/`; identical content deduplicates, changed
  content creates a new revision. Local document storage is excluded from
  the crate package.
- `inim catalog case-study plan` computes the reproducible horizon and
  expected 2019 archive files **without downloading anything**; targets
  with unresolved historical mappings are reported as blocked, and the
  plan stays `Draft` until reviewed.
- Web pages: `/case-studies`, `/case-studies/<slug>` (What happened /
  What public BGP showed / What BGP could not show, timeline, related
  tickets, document provenance, targets, plan, phase-conditioned BGP
  summaries, operator/BGP comparison matrix), `/documents/<id>` (validated
  serving). API: `/api/v1/case-studies[...]`.
- The MAN LAN 2019 case study (`case-studies/manlan-2019/`) demonstrates a
  detailed operator timeline combining Layer-2, physical, configuration,
  and routing effects: only part of the incident may be externally
  BGP-visible. Its target research is deliberately incomplete and its BGP
  analysis deliberately **not executed** — no conclusions are invented.

### Process exit status contract

| Code | Name                    | Meaning                                                        |
|------|-------------------------|----------------------------------------------------------------|
| 0    | `EXIT_SUCCESS`          | Plan produced (even Blocked) / analysis completed              |
| 1    | `EXIT_INVALID_INPUT`    | Malformed ticket or manifest; internal planning failure        |
| 2    | `EXIT_ANALYSIS_INCOMPLETE` | Infrastructure failure during analysis                      |
| 3    | `EXIT_ANALYSIS_BLOCKED` | Plan produced but Blocked; no Broker or MRT work was performed |

These are **process** exit codes, documented constants in `main.rs` — they
are never encoded in domain enums. `AnalysisPlanStatus::Blocked` lives in
the library/domain; `AnalysisOutcome` only ever carries completed,
insufficient-visibility, or incomplete results.

## Concurrency and performance

- Archive-level parallelism: a bounded download→parse pipeline
  (`--download-jobs`, `--parse-jobs`) processes compressed archives
  concurrently; each worker owns its parser; results merge in discovery
  order and route-state reconstruction runs sequentially, so artifacts are
  identical at any job count. `--jobs 0` is rejected (use `--parse-jobs`).
- `--show-execution-plan` prints the effective worker topology (logical
  CPUs, `available_parallelism`, cgroup/affinity limits) before acquisition.
- `performance.json` (per stage + per archive) is a separate, volatile
  artifact — never part of substantive equivalence checks, never in the
  verdict. Benchmark: `scripts/bench_parse_scaling.sh` (local raw caches,
  `--rebuild-update-caches`).
- Visual review: `scripts/screenshot-review.sh` captures loopback-only
  fixed-viewport screenshots of the deterministic demo catalog to
  `tmp/ui-review/` (gitignored, excluded from the package) for external
  computer-vision review.

## Workflow

1. **Manifest review** — a canonical manifest (schema v2) carries the
   reviewed `TransitPredicateMapping` (status, predicate, provenance).
   Legacy single-ASN shortcut fields (`managed_network_asn`,
   `internet2_asn`) are rejected with `LegacyManifestRequiresMigration`;
   use `inim migrate-manifest` offline to convert (never automatic, never
   invents unresolved ASNs).
2. **Planning precedes acquisition** — `inim plan` (and `analyze` before
   any work) parses ticket + manifest and produces an `AnalysisPlan`.
   Blocked plans (e.g. `MissingReviewedTransitPredicate`) perform **zero**
   Broker calls and **zero** MRT parses.
3. **Acquisition** — broker discovery, archive caching, derived-cache
   lookup, MRT parsing (skipped on valid cache hits).
4. **Analysis** — RIB preflight freezes the observer-prefix cohort; UPDATE
   admission; route reconstruction; tokenization; lifecycle classification
   by `ObserverPrefixKey`; semantic waves; assessment.
5. **Artifacts** — report.txt/json (observed event signature + observable
   mechanism hints + limitations), evidence appendix, archive manifest,
   lifecycle.json, semantic_waves.json, withdrawal_audit.json,
   limitations.json; optional comparison artifacts.

See `docs/` for the full design, domain model, decisions, data provenance,
and observability contracts.

## License

inim is licensed under the MIT License. See LICENSE.

SPDX-License-Identifier: MIT

## Local corpus workspace (Session 33)

inim includes a **locally acquired public-ticket corpus** workspace for
correlating historical operational events with public BGP data:

- **Polite acquisition** — the GRNOC Public Task Viewer is queried with
  1 concurrent request at 0.25 req/s (one every 4 s), burst 1, budget
  100 requests per sync; higher rates require explicit flags. Repeated
  429/403, unexpected auth, robots prohibition, and broad schema
  incompatibility stop the sync; permanent 404s are never retried. See
  `docs/sources/GRNOC_PUBLIC_TASK_VIEWER.md` for the protocol audit.
- **Explicit discovery only** — ticket identifiers enter through
  analyst seeds, document/case-study references, ticket-description
  references, or a scoped public search. There is no blind numeric-ID
  enumeration and no "download everything" mode.
- **Immutable snapshots with provenance** — every fetch records HTTP
  status, content-type, ETag, Last-Modified, acquisition method, retry
  count, and discovery provenance; a changed payload creates a new
  snapshot, a 304 never does, and old snapshots stay linked to their
  historical runs. Fetch metadata is whitelisted (no cookies/headers).
- **Relationship graph** — explicit ticket references (with exact
  source spans and wording-classified kinds such as
  `TracksRemainingImpactIn`) are stored distinctly from machine-derived
  candidates (`TemporalOverlap`, evidence
  `DerivedTemporalOverlap`); temporal overlap is never causal.
- **Analysis queue** — each event derives a BGP-analysis readiness
  state (NotReviewed … AnalysisComplete/Stale/Failed) from reviewed
  inputs only; nothing is auto-analyzed and no ASN mapping or predicate
  is inferred.
- **Candidate groups** — categorical, explainable confidence
  (ExplicitlyLinked / StrongCandidate / WeakCandidate / Rejected);
  groups never replace individual events, and rejected candidates stay
  suppressed until the evidence changes.
- **Shared archive planning** — `CorrelationBatch` groups per-event raw
  archive requirements (RouteViews and RIPE RIS families) so one
  archive is downloaded once across overlapping cohorts, without
  merging any event's evidence or verdict.
- **Web UI / API** — `/corpus`, `/analysis-queue`,
  `/incident-candidates`, `/archive-batches`,
  `/events/{id}/relationships` and the matching read-only
  `/api/v1/corpus/*` endpoints. No HTTP GET starts crawling or
  analysis.

The corpus is labeled a **locally acquired public-ticket corpus** —
completeness is never assumed. Acquisition and redistribution policy:
see `docs/DATA_PROVENANCE.md` (metadata-only export default; raw
payloads excluded from the crate and from exports until separately
reviewed).

## Reviewed multi-observer analysis (Session 34)

The acquired corpus is now **reviewed operational data**, and the
NORDUnet pilot runs against **two independent public observer families**
(RouteViews and RIPE RIS) that are compared without merging evidence:

- **Reviewed interpretations** — every acquired ticket has a reviewed
  case-study role (ChangeWindow / PrimaryIncident / ParticipantImpact /
  AlarmOrTelemetry / RollbackOrRecovery / OperationalTask / Other),
  entity labels, linked maintenance/change identifiers, analysis
  applicability, and per-field provenance (snapshot field or the cited
  AAR). Source snapshots stay immutable; reviewed roles never replace
  source task types. Import: `inim catalog corpus-review`; graph audit:
  `inim catalog relationships audit` and `/corpus/relationships`.
- **Explainable candidate grouping** — one candidate per ticket pair
  with every supporting signal listed; temporal overlap alone is
  `TemporalCoincidence` — stored and queryable
  (`/incident-candidates?include=temporal`) but hidden from the default
  analyst queue. Rejected candidates stay suppressed until the evidence
  changes.
- **RIPE RIS execution** — RIS archives pass through the same
  evidence-bearing engine as RouteViews (manifest `source_family`;
  family-scoped cache identity; family-labeled reports). RIS and
  RouteViews are peer observer families; collector identity is
  `(family, collector)`.
- **RIS collector selection** — metadata + RIB preflight per candidate;
  only collectors with qualifying visibility (AS2603-origin routes with
  AS11537 in path at the pre-window baseline) are selected, with
  rejected collectors and reasons recorded
  (`case-studies/manlan-2019/pilot/ris-collector-selection.md`).
- **Independent per-collector runs** — the NORDUnet pilot has one
  RouteViews run (route-views2) and one run per selected RIS collector
  (rrc00, rrc06, rrc15), each with its own evidence and verdict. No
  merged verdict exists.
- **Reviewed service-plane model** (Session 35) — the R&E and
  peering-plane identities are profile data, not control flow; session
  relationships come from historical MRT peer metadata (route-views2
  peers directly with AS11537; the RIS collectors observe AS11537-in-
  path routes indirectly; no AS11164 baseline exists at any selected
  observer); collector locations are reviewed metadata with temporal
  provenance (RRC06 = Otemachi, Tokyo, Japan); plane-specific runs,
  origin-only inventories, and the cross-observer matrix keep every
  observer's evidence independent.
- **Source-extraction reuse** (Session 35) — a versioned, origin-scoped
  parse cache lets plane-specific runs and audits parse each RIB once;
  outputs are identical standalone vs reused, evidence ids never change.
- **GRNOC sync policy** (Session 35) — reviewed local guidance raises
  the default ceiling to 5 requests/second (smooth limiter, burst 2,
  max 5 in-flight) with full adaptive response to 429/Retry-After;
  rates above 5 req/s need `--allow-higher-rate`.
- **Cross-observer comparison** — per-prefix × collector rows and a
  bounded statement vocabulary ("Observed at multiple independent
  public collectors", "Observed only at one selected collector",
  "Similar route-state change with different timing", "No counterpart
  at this observer", "Insufficient baseline visibility"). Multiple
  observer agreement is never "global confirmation"; absence of
  baseline visibility is never counted as absence of impact.
- **Analyst queue** — rows show the reviewed role, archive-plan status,
  existing runs, and a derived next analyst action (Review entity
  mapping / Review transit predicate / Run RIB preflight / Review
  archive volume / Analyze / No public-BGP target / Inspect stale run);
  nothing executes from an HTTP GET.

The case-study page (`/case-studies/manlan-2019`) shows related public
tickets with their reviewed roles and the RouteViews/RIS observer
comparison. The bulk-access request to GlobalNOC remains a **draft**
(`docs/sources/GRNOC_BULK_ACCESS_REQUEST.md`) — reviewed but not sent.
