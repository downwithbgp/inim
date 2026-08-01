# Session 30 — MAN LAN multi-ticket incident case study (requirements)

Source: the pasted Session 30 brief (19 parts). This file condenses scope and
definition of done; `design.md` records the technical decisions; `tasks.md`
splits the work into verifiable tasks.

## Scope

Add a multi-event `IncidentCaseStudy` layer to the local event catalog that
compares a reviewed operator-reported incident timeline with BGP behavior
observed in one or more event-conditioned `AnalysisRun`s.

Hard constraints:

- Do **not** collapse the MAN LAN incident into one synthetic ticket.
- Do **not** attach BGP observations to the PDF or to a mutable ticket.
  Evidence continues to belong to immutable `AnalysisRun`s.
  Association path: CaseStudy → AnalysisRun → stream lifecycle → route-instance
  evidence. No `RouteObservation.case_study_id`, no
  `RouteTransition.case_study_id`, no `EvidenceRef.case_study_id`.
- Relationship enum is generic (`PrimaryChange`, `PrimaryIncident`,
  `RollbackChange`, `ParticipantIncident`, `Alarm`, `OperationalTask`,
  `Communication`, `Related`) — no MAN-LAN-named variants.
- No production type/branch may contain `MANLAN`, `CANARIE`, `NORDUnet`,
  `ESnet`, `EVPN loop`, `CHG0038258`. Those are case-study data.
- The AAR PDF must not enter the crate package; local document storage is
  excluded from packaging; may be stored as local catalog data with source
  URL, relative local path, SHA-256, media type, import timestamp, metadata.
- Do not guess historical target mappings; `Unresearched` /
  `NotApplicableToPublicBgp` are valid honest states.
- No HTTP request may trigger historical BGP analysis or archive downloads.
- No public-BGP conclusion until historical target mappings AND an archive
  plan have been reviewed.
- Do not copy the PDF's complete text into fixtures; tests use a small
  synthetic PDF.

## Deliverables (Part 18)

Required: generic case-study schema + migrations, reference-document import,
case-study import, MAN LAN reviewed metadata, case-study web list/detail,
case-study API, secure document serving, phase + observability presentation,
initial historical-analysis plan. Historical BGP execution may remain
unexecuted.

## Definition of done

1. The catalog can represent a reviewed multi-ticket incident case study
   without collapsing it into one mutable event.
2. The operator AAR is preserved as an immutable reference document with
   exact provenance.
3. MAN LAN timeline, related ticket references, reported effects, and
   observability limits are visible in the web UI.
4. No public-BGP conclusion is produced until target mappings and an archive
   plan are reviewed.
5. Future analysis runs can be linked and compared phase by phase with the
   operator timeline without implying causation.
