# inim — Domain Glossary

## Identity model (ADD-PATH-aware)

### RouteKey
Unique identity of a **route instance**: `collector`, `peer_ip`,
`prefix`, and `path_id` (`None` for ordinary unkeyed records).
The route-state store is `Map<RouteKey, RouteState>`.

### ObserverPrefixKey
Aggregate stream identity: `collector`, `peer_ip`, `prefix` — **no**
`path_id`. The principal lifecycle identity. The frozen cohort is a
`Set<ObserverPrefixKey>`.

Multiple route instances (multiple `path_id`s) may represent **one**
observer stream. Final instance loss is required for stream absence;
`path_id` churn alone is not routing impact.

### Deterministic observation identity
Observation IDs are assigned after sorting by the documented order:
collector, timestamp, archive order, element sequence, peer IP, prefix,
path_id (`None < Some(id)`). Serial and parallel completion produce
identical IDs; different route instances get different IDs. Evidence
references (`EvidenceRef`) carry the observation id plus archive/URL
provenance — there is no separate evidence id namespace.

## Core types

### EventId / EventWindow / OperationalEvent
Event identity and declared time window. `OperationalEvent` carries id,
title, window, source, and raw data for auditability. Event subjects are
**data, not code** — no event name appears in domain enum variants.

### ExpectationKind
`Redundant`, `NonRedundant`, `ParticipantRelationshipUnavailable`,
`PeerRelationshipUnavailable`, `Unknown`.

### ImpactExpectation
What kind of impact is expected, a human-readable description, and where
the expectation came from (convention provenance).

### TicketLifecycle
`Open` / `Closed`. **Orthogonal** to `ImpactExpectation`: an open ticket
can have any expectation; the lifecycle only describes whether a
published end time exists.

### TransitPredicate
Reviewed path predicate: `ContainsAny(asns)`, `ContainsAll(asns)`,
`Adjacent(left, right)`. Evaluated against AS paths.

### TransitPredicateMapping
Manifest-carried reviewed mapping: `status` (Reviewed/Unresolved),
`predicate`, `provenance` (statement, reviewer, date). Canonical manifests
use this — never single-ASN shortcut fields.

### Prefix / AsPath / RouteAttributes / RouteState
Route model types. `RouteState` carries prefix, attributes, timestamp,
observer, and `path_id`.

### TransitionKind
`Announcement`, `Withdrawal`, `Duplicate`, `PathReplacement`,
`AttributeChange`, `SessionReset`, `Restoration`, `ReturnToBaseline`.

### GenericTransitionEffects
Orthogonal facets computed for every transition: communities changed,
graceful-shutdown added/removed, prepend change, material path changed,
origin changed, next-hop changed, MED changed, local-pref changed.

### EventRelativeEffects
Separate, event-relative facets: transit retained, transit departed,
transit returned.

### RouteTransition
A classified transition with evidenced baseline/before/after states,
triggering evidence, an `AnalysisPhase` (`Warmup` | `Event` | `Cooldown`),
and a continuity marker (`Known` | `Unknown`, set when a session boundary
or missing observation interrupts the evidence chain).

## AnalysisPhase

- **Warmup** — updates before the event window are processed but emit no
  state change; they only establish pre-window context.
- **Event** — the event window proper. The **event baseline is frozen at
  the first event-period observation**; only event-phase transitions can
  be a stream's `first_change`.
- **Cooldown** — transitions after the event-window end are classified
  and stored separately (`cooldown_transitions`); open absence intervals
  are closed at the cooldown end. Cooldown observations never change the
  event-window verdict; they feed the analysis-final state.

## Stream lifecycle

### StreamCategory
Per-`ObserverPrefixKey`: `Unchanged`, `PrependOnly`,
`PathChangedStillViaTransit`, `DepartedTransitPath`, `Withdrawn`.
Rules:
- losing one instance while another remains is **not** Withdrawn;
- losing the final target instance **is** Withdrawn;
- an equivalent route reappearing under a new path_id is **not** a
  material path change;
- a visible stream with no active matching route is DepartedTransitPath;
- at least one active matching route remaining means still ViaTransit.

### Restoration kinds
- **ExactInstanceRestoration** — same path_id returns with a semantically
  equivalent route;
- **EquivalentRouteRestoration** — equivalent route returns under any
  path_id;
- **ObserverPrefixRestoration** — stream changes from absent to visible;
- **BaselineSetRestoration** — active route semantics again equal baseline
  route semantics.

Path-id equality alone is never semantic equality. Route-semantic equality
compares: AS path, origin ASNs, origin type, next hop, MED, local
preference, atomic aggregate, and communities (as sets). Attributes not
preserved by the model (large/extended communities, non-ASN path
segments) are not compared.

### ADD-PATH continuity ambiguity
When both keyed (`path_id=Some`) and unkeyed (`None`) records appear for
one `ObserverPrefixKey`, the stream is flagged `add-path ambiguous` with
recorded evidence (first keyed record, first unkeyed record, archive
identities, affected time range). The ambiguity is **stream-scoped** and
suppresses strong stream-level assessment; unrelated streams remain fully
evaluable.

### GSHUT (RFC 8326)
The GRACEFUL_SHUTDOWN community is `65535:0`. The lifecycle tracks
baseline presence, addition, removal, first/last timestamps, presence
before withdrawal/path replacement, tag-to-consequence duration, and
removal during restoration. Its presence is an optional mechanism hint;
absence does not prove a mechanism was unused.

## Semantic waves

Temporal clusters of event-phase transitions, retaining contributing
RouteKey evidence: stable wave ID,
start/peak-interval/end, stream count vs route-instance count, prefixes,
peers, generic facet counts, event-relative counts, representative
before/after states, and evidence references. Labels
(`PrependReduction`, `PrependIncrease`, `StreamWithdrawal`,
`TransitDeparture`, `StreamRestoration`, `TransitReturn`,
`BaselinePolicyRestoration`, `MixedRouteChange`) require supporting
effects; ties resolve to `MixedRouteChange`. Wave counts are derived from
actual temporal clustering — never forced. SEQUITUR describes repeated
sequences; it never assigns semantic labels and never determines the
assessment.

## Incident case studies

The catalog distinguishes six kinds of objects; the distinction is
semantic, not a naming accident:

| Concept | What it is |
|---|---|
| `CatalogEvent` | one source ticket (one external record) |
| `IncidentCaseStudy` | reviewed grouping and interpretation of several sources and analyses |
| `EventSnapshot` | what a source ticket said at one point in time (immutable) |
| `ReferenceDocument` | immutable external supporting material (e.g. an after-action report) |
| `AnalysisRun` | what inim observed under one exact plan (immutable) |
| `ComparisonRow` | reviewed comparison of operator reports with BGP observation (`case_study_compare.rs`) — interpretation, not a causal foreign-key relationship |

- `CaseStudy` links to `CatalogEvent`s (relationship vocabulary:
  PrimaryChange, PrimaryIncident, RollbackChange, ParticipantIncident,
  Alarm, OperationalTask, Communication, Related), to `ReferenceDocument`s,
  and to `AnalysisRun`s. A link to a ticket that only exists in a source
  document has no catalog event — the external identifier is preserved as
  an unresolved document reference.
- `ReferenceDocument` content revisions are deduplicated by SHA-256; a
  changed document is a new revision. The local file, when available, is a
  catalog-relative path under `data/documents/`.
- `CaseStudyPhase` boundaries carry precision flags: `exact` (stated in
  the detailed timeline) vs `summarized` (broad boundary summarized by the
  source). Retrospective belief is never rendered as measured fact.
- `CaseStudyClaim` carries reviewed wording, qualification, source
  document + section, and an explicit observability classification
  (PotentiallyVisibleInPublicBgp / IndirectlyVisible / NotDirectlyVisible
  / Unknown) with rationale.
- `CaseStudyTarget` carries a research status; `Unresearched` and
  `NotApplicableToPublicBgp` are valid reviewed states, never guesses.
- `CaseStudyAnalysisPlan` (Draft until reviewed) records the horizon and
  expected archives; producing it downloads nothing.
- Phase-conditioned summaries derive from one continuous run: transitions
  are assigned to exactly one phase by time, stream visibility walks
  across phase boundaries without resetting baseline state, and counts are
  observer-stream counts.

## Assessment

### Verdict
`ExpectedRedundantImpact`, `ExpectedLossOfReachability`,
`ExpectedParticipantUnavailability`, `ExpectedAlternateRouting`,
`PartialImpact`, `UnexpectedContinuedInternet2Path`,
`PolicyChangeObserved`, `ProvisionalImpactObserved`,
`ProvisionalNoImpactSoFar`, `UnexpectedWithdrawals`,
`RedundancyFailureObserved`, `UnexpectedBlastRadius`,
`LessImpactThanExpected`, `NoObservableBgpImpact`,
`InsufficientVisibility`, `Indeterminate`.

### AnalysisOutcome
`Completed`, `InsufficientVisibility`, `Incomplete`. Blocked planning is
**not** an outcome — `AnalysisPlanStatus::Blocked` precedes acquisition.

### Evidence / EventAssessment
Evidence links conclusions to source records; an assessment carries
event id, expectation, verdict, evidence, waves, and generation time.

## Catalog

### CatalogEvent / EventSnapshot / ManifestRevision
Source-neutral event identity; immutable snapshots of what the operator
source said at one time (raw payload + normalized fields + source
SHA-256); reviewed manifest revisions referencing the exact snapshot they
were reviewed against.

### AnalysisPlanRecord / AnalysisRun
An immutable plan records exactly what inim intended to observe; an
AnalysisRun records one execution and its evidence (software version,
parser identity, schema versions, verdict, assessment, artifact
references). Route observations and stream lifecycles belong to the
AnalysisRun — never directly to a mutable event.

### AnalysisArtifact / StreamLifecycleSummary / SemanticWaveSummary
Artifact rows store kind, relative path, media type, schema version,
SHA-256, size. Lifecycle and wave summaries are denormalized for listing
without loading evidence artifacts.

### CatalogSyncRun / EventCatalogSource
Sync-run records; the `EventCatalogSource` trait with the GRNOC Public
Task Viewer adapter as the first implementation.

### Catalog status
Derived (never stored as the sole truth) with documented precedence:
Running > Failed > Stale > Blocked > Complete > Ready > NeedsReview >
Discovered. Stale means the current event state has not yet been analyzed
under the latest inputs — an old completed run remains historically
complete.

## Corpus domain

- **DiscoveryProvenance** — how a ticket identifier entered the corpus:
  AnalystSeed, DocumentReference, TicketDescriptionReference,
  PublicSearchResult, CaseStudyReference, OtherReviewedSource. Each
  discovery row keeps status (Pending/Fetched/Unresolved/Failed).
- **SnapshotFetch** — one HTTP fetch attempt: status, content-type,
  ETag, Last-Modified, acquisition method, retries, conditional flag,
  optional snapshot link (NULL on 304/unchanged).
- **TicketRelationship** — from_event → to_event (or unresolved
  external id), relationship_kind (References, TracksRemainingImpactIn,
  SupersededBy, RelatedChange/Incident/Task, UnknownReference,
  TemporalOverlap), evidence_kind (ExplicitTicketText,
  ReferenceDocument, SharedCaseStudy, AnalystReviewed,
  DerivedTemporalOverlap, DerivedEntityOverlap), snapshot/document
  provenance, review status. Derived edges are never causal.
- **Analyzability** — per-event BGP-analysis readiness derived from
  reviewed inputs: NotReviewed, NeedsEntityMapping,
  NeedsTransitPredicate, NeedsAnalysisWindow, NotApplicableToPublicBgp,
  ReadyForArchivePlanning, ArchivePlanReady,
  InsufficientBaselineVisibility, AnalysisComplete, AnalysisStale,
  AnalysisFailed, AnalysisRunning.
- **IncidentGroupCandidate** — member events, evidence signals,
  categorical confidence (ExplicitlyLinked, StrongCandidate,
  WeakCandidate, Rejected), evidence fingerprint for rejection
  persistence; groups never replace events.
- **CorrelationBatch** — per-event cohort plans + unique raw archives +
  consumer map; archives avoided through reuse; deterministic; evidence
  identity independent of batch membership.
- **SourceFamily** — RouteViews | RipeRis; part of collector identity
  in archive plans.

## Reviewed interpretation layer

- **TicketReview** — analyst-reviewed case-study interpretation for one
  catalog ticket, stored separately from its immutable snapshot:
  reviewed roles, entity/asset labels, linked maintenance/change
  identifiers, analysis applicability, relationship to the case study,
  per-field provenance, reviewer, reviewed-at.
- **Reviewed roles** (vocabulary): ChangeWindow, PrimaryIncident,
  ParticipantImpact, AlarmOrTelemetry, RollbackOrRecovery,
  OperationalTask, Other. Distinct from the source `task_type`
  (Incident / Change Request / Task).
- **Analysis applicability** (reviewed): PotentiallyVisibleInPublicBgp,
  NotApplicableToPublicBgp,
  **NotDirectlyObservableInPublicBgp** (the reviewed relationship
  exists but is not directly observable in public BGP — optical
  participant interface, Layer-2 circuit, exchange fabric, alarm or
  telemetry; a contemporaneous BGP run is a scope-mismatched
  supporting observation only),
  **NotOriginAttributable** (no reviewed origin attribution — test
  equipment, exchange fabric), ApplicableTargetNotYetMapped. The
  RELATIONSHIP named by the ticket decides applicability, not the
  mere existence of an ASN for the named organization.
- **ReviewProvenance** — per-field citation: `SnapshotField:<field>` or
  a reference document (AAR) with `source_document_id`. Missing source
  fields are never backfilled without a cited document.
- **Reviewed relationship kinds**: RollbackFor,
  ParticipantImpactDuring, AlarmDuring, OperationalTaskDuring — plus
  the corpus-reviewed kinds (TracksRemainingImpactIn, RelatedChange,
  RelatedIncident, References, SupersededBy, RelatedTask,
  UnknownReference) and derived overlap kinds (TemporalOverlap,
  EntityOverlap).

## Project scope

Project scope is a reviewed project-owner decision, stored in
`config/project-scope.toml` (schema v1) and loaded once per process by
the web app, CLI, worker, and demo. It is a universal first-party
execution policy: the standalone `inim analyze` path applies the same
reviewed exclusions as the catalog workflow, before any planning,
broker discovery, archive acquisition, or MRT parsing (a scope-blocked
analyze exits `EXIT_ANALYSIS_BLOCKED` and writes no outputs). It is
ORTHOGONAL to the analytical
applicability vocabulary: an exclusion never marks an event
not-observable, failed, or invalid. Matching keys, in precedence order:
exact external source ID, exact reviewed entity name, exact reviewed
ASN, exact alias — all exact and normalized. Excluded plans cannot be
queued or retried (`project_scope_excluded`); the worker rechecks scope
after claim and cancels pre-execution jobs (`excluded_by_project_scope`);
imports skip excluded manifests explicitly; demo verify fails when an
excluded event is present; immutable runtime records are hidden from
default views and reported by the read-only `project-scope audit`.
Project scope is policy, not authentication or sandboxing: it constrains
which subjects first-party commands analyze and publish, and does not
protect a hostile local caller.

## Candidate grouping

- **Confidence categories**: ExplicitlyLinked (source-asserted),
  StrongCandidate (reviewed case-study membership), WeakCandidate
  (temporal overlap plus a supporting signal: shared reviewed
  entity/asset label, shared maintenance/change identifier, explicit
  reference), TemporalCoincidence (temporal overlap alone — hidden from
  the default queue, still queryable), Rejected (analyst).
- **Evidence signals**: ExplicitTicketText, SharedCaseStudy,
  DerivedTemporalOverlap, SharedReviewedEntity,
  SharedMaintenanceChange. One candidate per pair, evidence = union of
  all signals.

## Multi-observer analysis

- **SourceFamily** — RouteViews | RipeRis; part of collector identity;
  drives broker project, archive URL conventions, RIB cadence, cache
  key identity, and report labeling.
- **ObserverComparisonRow** — per normalized prefix × collector: first
  observed change, temporary absence, path replacement, transit
  departure, restoration, baseline visibility, family.
- **PrefixStatement** — bounded cross-observer vocabulary (multiple
  independent collectors / one selected collector / similar change with
  different timing / no counterpart / insufficient baseline visibility);
  never global confirmation.
- **Next analyst action** (queue): Review entity mapping · Review
  transit predicate · Run RIB preflight · Review archive volume ·
  Analyze · No public-BGP target · Inspect stale run. Derived from
  readiness + reviewed applicability; never executed from a GET.

## Reviewed service-plane model

- **NamedServicePlane** — reviewed profile data (`id`, `display_label`,
  `asns`); one organization can have multiple planes (e.g. an R&E
  routing plane and a settlement-free peering plane). Production logic
  is plane-neutral; identities live in data files.
- **ReviewedAsnRole** — ASN → role string (data); unknown ASNs display
  as `unclassified observed ASN`, never as "commercial" by default.
- **ObserverSessionKey** — `{source_family, collector, peer_ip,
  peer_asn, address_family}`; source family never determines peer ASN.
- **SessionRelationship** — DirectPeerToNamedPlane (peer ASN equals a
  plane ASN), IndirectPathViaNamedPlane (path contains a plane ASN),
  OtherObservedPath, Ambiguous. Direct and indirect are different
  facts and never co-occur for one plane; roles are time-scoped to the
  RIB timestamp.
- **SessionAuditRow** — historical session facts from the MRT header
  (peer IP, peer ASN, address family, origin route counts, distinct
  prefixes, path-class membership). Current peer lists are supporting
  context only.
- **CollectorLocation** — reviewed collector metadata with temporal
  provenance; location describes where the collector is hosted, not the
  path of observed routes, and never defines a network role.
- **CohortSelector vs PathClassifierSet** — manifest `transit_predicate`
  selects the baseline cohort; `path_classifiers` classify origin-
  matching routes into named classes (one/both/neither). Never
  conflated: the inventory admits every origin route regardless of
  classifier match.
- **Source-extraction cache** — versioned, origin-scoped parse reuse
  keyed by (source sha, family, collector, sorted origin set); predicate-
  independent, so plane-specific runs parse each RIB once. Evidence
  identity is content-derived and never changes with cache path.

## ObserverEpisode and observed breadth

- **ObserverEpisode** — the primary human-facing unit: one observer
  session (collector + peer) × one presentation-level signature.
  Grouping is by (collector, peer IP, effect kind); the named service
  plane is a per-run reviewed constant, not part of the group key.
  Different peers at one collector stay separate; direct and indirect
  observations stay separate; generation is deterministic.
- **Effect kinds are presentation groupings** of existing lifecycle/
  transition evidence (withdrawn+restored → TemporaryStreamAbsence;
  withdrawn+unrestored → RouteWithdrawal; transit departed/returned →
  NamedPlaneDeparture/Return; prepend-only → PrependChange; path
  change retaining transit → PathReplacement; qualifying baseline with
  no observed change → NoRouteStateChange).
- **Observed breadth** (regional): changed / eligible observer sessions
  with the denominator always visible. NoRouteStateChange (an observed
  signature with `Complete` coverage), NoBaselineVisibility, and
  IncompleteCoverage are distinct states and never collapse into one
  zero; incomplete coverage is never counted as unchanged. This is
  breadth, not severity — no severity score exists.
- **Observer site vs peer identity** — a collector site has a reviewed
  location and region (AMER/EMEA/APAC/Unknown, data-driven); the peer's
  own location is never inferred from the collector's. Direct peer ASN
  membership and AS-in-path membership are separate facts. A multihop
  collector still has a site region but is visibly labeled multihop.
- **Timeline** — lanes are observer sessions; operator anchors and BGP
  evidence have distinct kinds; timestamps are exact observations, never
  interpolated; unresolved end states are explicit.

## Job model

An analysis job is durable execution state for one exact immutable plan
revision: queue → claim (with a renewable lease) → execution stages →
Completed / Cancelled / Failed. Terminal jobs are immutable; retry
creates a new attempt linked via `original_job_id`. The canonical plan
hash (execution-relevant fields only, no timestamps) is the identity
used for queue idempotency and run provenance. Staged artifact roots
are catalog-root-relative paths, never absolute. See
`docs/GLOSSARY.md` (Analysis job, Worker lease, Plan revision, Staging
artifact).

## Layer-2 interconnection context (reviewed, non-protocol)

A case study may carry **reviewed interconnection context** describing
the Layer-2 environment of an operator-reported incident (for example
an exchange fabric). The context names reviewed attachments and, where
the reviewed records establish them, their ASN labels with a validity
date. It is stored as reviewed interpretation in the case-study layer
(`interconnection_context`) and rendered as presentation.

Entities mentioned by sources are classified with a **bounded entity
taxonomy**: `AttachedNetwork` (reviewed evidence supports a Layer-2
attachment), `TestEquipment` (measurement/test hardware — never a
peer, an AS node, or an attached network), `InterconnectContext`
(interconnection or external-fabric reference), `OperationalService`
(service/facility context), and `UnresolvedMention` (role not
sufficiently established). Only reviewed attached networks are fabric
diagram participants; every other class is listed separately as
context. Source mention is not proof of attachment; a reviewed ASN is
not by itself proof of attachment; a Layer-2 attachment is not proof
of BGP adjacency; a familiar organization name is not proof of entity
class.

The context is deliberately **not protocol evidence**: production
analysis never uses fabric attachment metadata in route predicates,
cohort selection, or findings. Layer-2 attachment is not BGP
adjacency; an exchange fabric is not automatically an AS-path hop;
public BGP observes exported route consequences, not switch-fabric
state. Observed AS paths are rendered from canonical route evidence
only (stream lifecycles and transitions), never from fabric context.
