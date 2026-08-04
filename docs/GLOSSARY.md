# inim — Terminology Glossary

This glossary is normative for the current implementation. Every current
document, comment, API description, report example, and template uses
these definitions. Where a term has a historical meaning, that meaning is
recorded in the relevant ADR and noted here under "Prohibited stale
terms" — the historical meaning is never used in current output.

## Core objects

- **Event** — an operator-declared network incident (ticket) with a
  reviewed time window, imported into the local event catalog. Events
  are the analysis unit: plans, runs, and findings are conditioned on one
  event.
- **Source ticket** — the operator ticket (INC/CHG) that declares an
  event. The ticket snapshot is immutable evidence; the event is the
  reviewed interpretation of that snapshot.
- **Case study** — a reviewed collection of one or more events, targets,
  plans, runs, and findings with provenance, published under
  `case-studies/`. A case study is interpretation, not protocol
  evidence.
- **Analysis plan** — a reviewed plan (manifest) defining the target
  origin, transit predicate, named service plane, observer sessions, and
  event window for one analysis.
- **Analysis run** — one execution of a plan against acquired archives.
  A run produces immutable derived artifacts (transitions, lifecycle,
  findings) and a report.

## Routing identities and units

- **Target origin** — the origin ASN (or ASNs) of the reviewed target
  whose routes the analysis tracks.
- **Transit predicate** — the reviewed AS-path condition (a named
  service plane) that qualifies a route as "via the reviewed path".
- **Named service plane** — a reviewed, named AS-path condition used to
  classify routes (for example an R&E — Research and Education — plane
  or a paid-peering I2PX — Internet2 Peer Exchange — plane). "Named"
  means reviewed and labeled, never invented by the tool.
- **Source family** — the public BGP data source family (RouteViews,
  RIPE RIS, GRNOC). Families are distinct evidence sources and may
  legitimately disagree.
- **Collector** — a route collector within a source family (for example
  `route-views2`, `rrc11`). The collector site is where the collector
  peers; it is not the target's location.
- **Observer site** — the site hosting the collector that observed the
  routes (for example "Eugene, Oregon, US").
- **Observer session** — one selected collector peer session through
  which the target's routes were observed. Identity:
  `<family>/<collector> peer <peer_ip>`.
- **Peer ASN** — the ASN of the observer session's peer. Reviewed
  session context wins; otherwise the observed peer ASNs from source RIB
  evidence are used, and multiple distinct observed ASNs are ambiguous.
- **Direct observation** — the target was observed through a direct
  (customer/transit) peer relationship at the observer session.
- **Indirect observation** — the target was observed through a
  non-direct relationship (for example a route-server peer) at the
  observer session.
- **Route instance** — one active route for a prefix at an observer
  session, identified by its (prefix, path_id) with ADD-PATH awareness.
- **Observer-prefix stream** — the ordered route-state history of one
  prefix at one observer session (the principal lifecycle identity).
- **Distinct prefix** — one unique prefix counted once regardless of how
  many route instances or observer sessions carried it.
- **Transition** — one ordered route-state change on an
  observer-prefix stream with exact before/after path evidence and a
  timestamp.
- **Lifecycle** — the full ordered transition history of an
  observer-prefix stream across the event window and analysis cooldown.
- **Routing finding** — the operator-facing unit of output: which
  observer session saw which prefixes do what, when, over which peer
  relationship, with which before/after route state, and how the route
  ended.

## Time-scoped states

- **Event baseline** — the route frozen at the event start (the first
  observed route), independent of any later change. Distinct from every
  later state.
- **Pre-finding state** — the route immediately before a finding's
  change (for a withdrawal, the pre-withdrawal route). Distinct from the
  event baseline when an earlier change happened in-window.
- **First changed state** — the route immediately after the first
  observed change of the finding.
- **Event-window final state** — the last route state at the
  event-window end, derived only from lifecycle evidence.
- **Analysis final state** — the last route state at the analysis
  boundary (window end plus cooldown), derived only from lifecycle
  evidence. Independent of the event-window final state.
- **Final route** — the last route at a defined analysis boundary; the
  boundary must always be named (event-window end or analysis end).

## Restoration and coverage

- **Visibility restoration** — an absent observer-prefix stream returns
  to visibility (absent → visible).
- **Reviewed-plane restoration** — the route returns to the reviewed
  named service plane.
- **Equivalent-route restoration** — a semantically equivalent route
  returns under any path_id.
- **Exact event-baseline restoration** — the active route again equals
  the event-baseline route exactly. Never claimed from the pre-finding
  state alone.
- **Observation coverage** — whether the observation could be made at a
  session: `Complete`, `NoBaselineVisibility` (target not visible),
  `IncompleteCoverage` (run/archive limitation). "No change" is not a
  coverage state.
- **Insufficient visibility** — the observation cannot support a
  route-state claim (no qualifying baseline, required session absent,
  predicate not matched, or incomplete archive). Distinct from observing
  "no change".
- **No route-state change** — an observed signature with complete
  coverage: the target stayed visible on the reviewed path. It is
  supporting evidence only; it is not an assessment of an unobservable
  relationship.
- **Observed breadth** — the set of observer sessions, streams, and
  distinct prefixes actually observed; counts of observer sessions,
  findings, streams, and distinct prefixes are distinct and never
  conflated.
- **Operator-reported anchor** — a time or fact taken from the
  operator's ticket/AAR, used to anchor analysis, not treated as BGP
  evidence.
- **Evidence reference** — a stable reference to one piece of
  evidence (observation or source record) that supports a claim.

## Explicit distinctions

| Term | Meaning |
|---|---|
| **Event baseline** | route frozen at event start |
| **Pre-finding state** | route immediately before a finding |
| **Final route** | last route at a defined analysis boundary |
| **Observer-session absence** | absence at one selected collector peer |
| **Traffic interruption** | NOT measured by inim; BGP evidence does not measure traffic |

## Analysis jobs and the worker

| Term | Meaning |
|---|---|
| **Analysis job** | durable execution state for one exact immutable plan revision: Queued → Claimed → execution stages → Completed/Cancelled/Failed. Job state is execution state, never ticket lifecycle, plan status, or analysis outcome (a completed analysis may be InsufficientVisibility without the job failing). |
| **Plan revision** | one immutable `analysis_plans` row: reviewed manifest + derived plan payload + status. Editing creates a new revision; a queued or completed revision is never mutated. |
| **Worker lease** | the time-bounded claim a worker holds on a job. A lease expires after a fixed interval unless renewed; an expired lease marks the job Failed (`worker_lease_expired`), preserves staging, and requires explicit retry — never silent auto-resume. |
| **Staging artifact** | an output written under `data/jobs/<job-id>/staging` before validation and atomic publication into `data/runs/<job-id>/`. An incomplete job never appears as a completed workbench. |
| **Canonical plan hash** | SHA-256 over the canonical serialized plan (execution-relevant fields only, no generated timestamps). Used for queue idempotency and run provenance. |

## Prohibited stale terms

These terms are not used in current output or current documentation
unless quoted from historical material or source documents:

| Stale term | Reason / current replacement |
|---|---|
| Internet impact | inim measures observer-scoped BGP visibility, not Internet-wide effects |
| Global impact | same; no global reachability claim |
| Outage severity | severity is not derivable from BGP evidence |
| Affected Internet percentage | no such measurement exists |
| Departed-I2 | Internet2-specific internal label; use "left the reviewed path" |
| Backup path | implies a mechanism; use the observed before/after path |
| Protected route | mechanism claim; not evidenced |
| Failover confirmed | mechanism claim; not evidenced |
| Traffic restored | inim does not measure traffic; use visibility/path restoration |
| Exact baseline (alone) | always "event baseline" or the named state |
| Stream (alone) | name the unit: session, prefix, or route instance |

The one automated check on these terms (documentation drift guard)
applies to current normative documents and generated outputs, never to
quoted source text or historical ADRs.

## Project scope

**Project scope** — the reviewed, tracked decision of which entities and
source records are Included in the active project corpus
(`config/project-scope.toml`). A **Project-scope exclusion** means inim
intentionally does not include the entity or source event; it is NOT an
analytical conclusion (the event is not marked invalid, unobservable, or
failed). Project scope is distinct from **analytical applicability**
(whether public BGP can observe the named relationship): an analytically
valid IP-layer event may still be excluded by project policy, and an
optical event may remain in the source corpus for incident context.
Matching is exact and normalized (external source ID, reviewed entity
name, reviewed ASN, exact alias); no fuzzy matching. Excluded items are
omitted from default web, API, candidate, demo, and case-study views;
the worker rechecks scope before any source access; immutable runtime
records are never deleted automatically.

## Managed network and named relationship

**Managed network** — an operator network whose public task records are
published through a supported source (e.g. Internet2, Indiana GigaPOP
via the GRNOC Public Task Viewer). A **peer relationship** is the
routing relationship between the managed network and a named
counterparty. The **named managed relationship** is the complete
relationship named by the operator record; "complete" refers to the
record's scope, NOT the counterparty's total external connectivity.
**Global single-homing** (the counterparty has no other upstreams) is
never inferred from a title alone.

**Attachment qualifier** — a trailing parenthesized code in the shared
title convention; presence indicates expected redundancy, absence
indicates the complete named relationship may be unavailable.

**Direct relationship observation** — evidence from a qualifying direct
observer session under the reviewed plan (the collector peers with the
reviewed managed-network ASN and the target-origin routes are visible
through that session).

**Indirect relationship observation** — a selected AS path matching a
reviewed relationship predicate without a direct observer session.
Indirect evidence is never called direct peering evidence.

**Provisional analysis / snapshot cutoff** — for an open source event,
the analysis end is an explicit reviewed snapshot cutoff (grounded in
the source retrieval timestamp or another reviewed time); the result
is Provisional and states "observed through cutoff". A later source
refresh creates a new snapshot, plan revision, job, and run; the
provisional run is never mutated.
