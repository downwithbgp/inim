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

