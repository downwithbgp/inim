# ADR-003 — Multi-ticket incident case-study layer

- Status: accepted
- Date: 2026-08-01 (Session 30)
- Supersedes: none (extends ADR-002)

## Context

A single NOC ticket is often one record inside a larger incident. The
MAN LAN Core Node Hardware Upgrade after-action report (AAR) describes one
incident spanning 12 related ticket references (changes, incidents, tasks),
multiple participants, Layer-2/physical/config/routing effects, and a
detailed operator timeline. Analyzing such an incident requires:

- grouping several tickets and documents into one reviewed case study,
- preserving the operator timeline as reviewed data with provenance,
- comparing it with BGP behavior observed in one or more event-conditioned
  AnalysisRuns, phase by phase,
- staying honest about what public BGP cannot show.

We must not collapse the incident into one synthetic ticket, must not attach
observations to the PDF or to mutable tickets, and must not guess historical
target mappings to produce a result.

## Decision

Add a generic `IncidentCaseStudy` layer to the catalog (schema v2, tables
`case_studies`, `case_study_event_links`, `reference_documents`,
`document_revisions`, `case_study_document_links`, `case_study_phases`,
`case_study_analysis_links`, `case_study_claims`, `case_study_targets`,
`case_study_analysis_plans`, `run_transitions`).

Key decisions:

1. **Evidence stays run-owned.** The association path is
   CaseStudy → AnalysisRun → stream lifecycle → route-instance evidence.
   No `RouteObservation.case_study_id`, no `RouteTransition.case_study_id`,
   no `EvidenceRef.case_study_id`. A case study never owns observations.
2. **Generic vocabulary.** Relationship types (`PrimaryChange`,
   `PrimaryIncident`, `RollbackChange`, `ParticipantIncident`, `Alarm`,
   `OperationalTask`, `Communication`, `Related`), claim categories, and
   observability classifications are neutral enums; MAN LAN specifics live
   only in case-study data files. No production type or branch may contain
   incident-specific names.
3. **Reviewed data files.** `case-studies/<slug>/case-study.json` is the
   canonical reviewed representation (schema v1, deny-unknown-fields,
   transactional import, idempotent by slug + content hash, conflicting
   immutable revision rejected). Phases carry `exact`/`summarized` boundary
   precision so retrospective belief is never rendered as measured fact.
4. **Unresolved references are first-class.** AAR-referenced historical
   tickets without independently retrieved snapshots stay document-referenced
   rows (`catalog_event_id NULL`); the importer never fabricates a snapshot.
5. **Documents are immutable references.** Reference documents are stored as
   local catalog data under `data/documents/<sha12>/` with SHA-256, media
   type, best-effort metadata, and a catalog-relative path. Identical content
   deduplicates; changed content is a new revision. The MAN LAN PDF is not
   redistributed (status Unknown) and never enters the crate package.
6. **Observability is reviewed data.** Every claim carries an explicit
   classification (PotentiallyVisibleInPublicBgp / IndirectlyVisible /
   NotDirectlyVisible / Unknown) with rationale. Non-observable conditions
   are classified — never reported as missed detections.
7. **No invented analysis.** The archive planner computes expected files and
   blocked targets without downloading anything; plans stay `Draft` until
   reviewed. No public-BGP conclusion exists until historical target
   mappings and the plan are reviewed; `Unresearched` and
   `NotApplicableToPublicBgp` are valid reviewed states.
8. **Comparison is interpretation, not causation.** Phase-conditioned
   summaries derive from one continuous run (no baseline reset at phase
   boundaries). The operator/BGP comparison uses explicit relationship
   labels (Before/During/After/Overlapping/NoObservedCounterpart/
   NotDirectlyObservable/Indeterminate) and never `ConfirmedCause`;
   temporal consistency is explicitly not causal proof.

## Consequences

- The catalog can represent one incident with many tickets, one event with
  one document, conflicting source documents, and incidents whose mechanism
  is not BGP-visible.
- Web pages and the JSON API expose the case study, timeline, related
  tickets, document provenance, targets, plans, phase summaries, and the
  comparison matrix; document serving is validated (relative path,
  containment, SHA-256, media allowlist) and read-only.
- Future analysis runs can be linked and compared phase by phase with the
  operator timeline without implying causation.

### Update (Session 31)

- Case studies begin with **narrow pilots**: one target, one collector,
  one bounded window around a documented boundary; Stage A (RIB preflight,
  `--preflight-only`) precedes any UPDATE acquisition, and a full run
  requires an interpretable pilot.
- **Historical entity mappings require dated review**; the reviewed
  research record is applied to target rows via `apply-research` (the only
  documented mutation of research fields, with an audit timestamp), never
  guessed from current metadata.
- Comparisons respect **run-window coverage**: an out-of-scope run cannot
  fabricate a negative for a claim window it never covered.
- Pilot findings are labeled "Historical pilot — <target>" and never
  broadened into a complete-incident conclusion.

### Update (Session 32)

- Pilot comparisons preserve event order: point anchors yield
  Before/After with explicit deltas; observations preceding a reported
  action are never attributed to it; the two-second absence audit
  confirmed temporary observer-stream absence at one collector (native
  precision, single peer, no ordering artifact), which is not proof of
  traffic loss.
- Historical analysis parallelism is archive-level with a bounded
  download→parse pipeline; reconstruction stays sequential and
  deterministic; performance.json is separate from substantive output.

## Session 33 addendum — corpus linkage

Case-study event links resolve to catalog events **by identifier only**
(never by title matching) once the ticket is acquired; unresolved
references (e.g. TASK numbers the viewer does not serve) stay
unresolved without fabricated events. Source ticket timings and AAR
timings remain distinct values — nothing is silently reconciled.
Ticket-to-ticket relationships extracted from public descriptions
retain snapshot provenance and are stored in the source-neutral
`ticket_relationships` graph, distinct from machine-derived overlap
candidates. The NORDUnet analysis remains associated with its exact
AnalysisRun and the reviewed case study — never causally attached to
every related ticket.

### Session 34 addendum: reviewed interpretations, multi-observer runs

- **Corpus acquisition and analysis review are separate stages.** Source
  snapshots stay immutable; reviewed interpretations (`ticket_reviews`)
  are analyst-reviewed context with per-field provenance, and reviewed
  roles never replace source task types.
- **Reviewed relationships** use specific kinds (RollbackFor,
  ParticipantImpactDuring, AlarmDuring, OperationalTaskDuring) with
  evidence kinds (AnalystReviewed / ReferenceDocument) and may carry
  ticket-text AND document support on one edge; unavailable TASK records
  remain unresolved document references.
- **Candidate grouping is explainable**: one candidate per ticket pair
  with every supporting signal; temporal overlap alone is
  TemporalCoincidence, hidden from the default queue but queryable.
- **RouteViews and RIPE RIS are peer observer families.** The reviewed
  NORDUnet pilot runs independently at route-views2, rrc00, rrc06, and
  rrc15; each run keeps its own evidence and verdict. The comparison
  layer never merges evidence and never claims global confirmation;
  absence of baseline visibility is not absence of impact.
- **Batch reuse does not merge event assessments**: raw archives are
  downloaded once per unique URL and derived caches reused only under
  matching cohort identity; evidence IDs do not depend on batch
  membership.


## Session 35 addendum — multi-plane comparisons

- A case study may expose multiple named service planes for one
  organization (profile data: `NamedServicePlane` with distinct ASN
  sets). Production logic is plane-neutral; the reviewed identities live
  in `case-studies/<slug>/pilot/network-profile.json`.
- Cohort selection (`transit_predicate`) and path classification
  (`path_classifiers`) are separate manifest concerns: a plane-specific
  run selects its cohort through one reviewed predicate, while
  origin-only inventories classify every baseline route one/both/neither
  without a verdict.
- Each plane-specific run is an independent AnalysisRun; the
  cross-observer matrix keeps per-observer evidence separate and labels
  direct (peer ASN equals plane ASN) and indirect (path contains plane
  ASN) relationships distinctly. A missing plane baseline (e.g. no
  AS11164-in-path route anywhere) is reported as a missing baseline —
  never as "no event change" on that plane.
- Collector locations are reviewed metadata with temporal provenance
  (`collector-locations.json`, as-of 2019-09-05): location describes
  where the collector is hosted, not the path taken by observed routes,
  and it never defines a network's role.

## Current status (2026-08-02)

Accepted and still applicable. The case-study layer has grown the
reviewed-interpretation tables (`ticket_reviews`, V7) and the
relationship-graph layer on top of the original schema v2 decision;
the association path `CaseStudy → AnalysisRun → stream lifecycle →
route-instance evidence` is unchanged.

## Follow-up

None outstanding.
