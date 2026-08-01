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
Observation IDs and evidence IDs are assigned after sorting by the
documented order: collector, timestamp, archive order, element sequence,
peer IP, prefix, path_id (`None < Some(id)`). Serial and parallel
completion produce identical IDs; different route instances get different
IDs.

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
A classified transition with evidenced baseline/before/after states and
triggering evidence.

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

Temporal clusters of event-window transitions derived primarily from
lifecycles, retaining contributing RouteKey evidence: stable wave ID,
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

## Catalog (Session 29)

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
