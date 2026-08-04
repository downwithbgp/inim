# Internal evaluator walkthrough findings — 2026-08

Dated audit. Records the internal project-owner evaluator walkthrough
that motivated the evaluator-blocking workbench corrections.

**This is an internal walkthrough, not an external evaluation
session.** The pilot registry (`docs/evaluation/PILOT-REGISTRY.md`)
remains at **zero external sessions**.

## Reproduction

Fresh deterministic demo (`inim demo init` + read-only `inim serve`)
at commit `94f8aad`; pages inspected over HTTP:

- `/events/INC0301970` (Smithville event page)
- `/analyses/3` (Smithville run page)
- `/case-studies/manlan-2019` (MAN LAN case-study page)

## Defects found (severity)

| # | Surface | Defect | Severity |
|---|---|---|---|
| 1 | Smithville run page | `INC0301970/report.json` listed with SHA/size and simultaneously reported missing (`Missing artifact files: INC0301970/report.json`) | P0 (contradictory artifact availability) |
| 2 | Smithville run page | observation-scope and lifecycle values rendered as `null`; empty Stream lifecycle detail section | P1 (null internal values as operator facts) |
| 3 | Smithville run page | no collector/site/peer/prefix-visibility rendering; no TargetPresentRelationshipAbsent / RequiredSessionAbsent distinction; UPDATE skip unexplained | P1 (missing observation-coverage evidence) |
| 4 | Smithville event page | leads with "Ready to queue" while a completed run exists; open window rendered as `start →`; reviewed snapshot cutoff not prominent | P1 (evaluator-blocking workflow language) |
| 5 | Smithville event page | fixture path (`file://tests/fixtures/grnoc/INC0301970.json`) presented as the source identity | P2 (provenance presentation) |
| 6 | MAN LAN page | NORDUnet shown "Unresearched" while four reviewed linked runs analyze NORDUnet AS2603 | P0 (stale authority conflict) |
| 7 | MAN LAN page | "No historical archive plan yet" + "Historical pilot — Not planned" while completed linked runs exist | P1 (contradictory plan/pilot state) |
| 8 | MAN LAN page | legacy machine verdict labels (`LessImpactThanExpected`, `ExpectedLossOfReachability`) as primary results; run IDs without target/collector context | P1 (legacy labels as current results) |
| 9 | MAN LAN page | default page dumps the full prefix × observer-peer matrix; collector timestamps repeated per peer | P2 (boundedness + dedup) |
| 10 | MAN LAN page | "Location" label for collector site; related-ticket provenance wording stale ("not independently retrieved" for tickets that now have snapshots) | P2 |

## Root causes

1. **Artifact contradiction:** catalog artifact rows store paths relative
   to the import output root; the web run page resolved them only
   against the catalog root (`<root>/<rel>`), while the demo verifier
   used a hardcoded case-study candidate list. The two checks could
   disagree, and every case-study artifact failed web resolution.
2. **Null rendering:** the insufficient-visibility report has no
   lifecycle/semantic-wave sections; the changed-routes presentation
   rendered missing sections as `null`.
3. **Workflow precedence:** `workflow_status_for` only considered runs
   linked through completed jobs; the demo imports runs without job
   rows, so the completed run was invisible to the workflow logic.
4. **Target authority:** case-study target rows (AAR-derived,
   Unresearched) were shown without reference to the reviewed linked
   runs that analyze those targets.
5. **Plan/pilot wording:** plan and pilot records are incident-wide
   planning concepts; the presence of a completed single-target pilot
   was not reflected when the pilot record was absent.

## Corrections

- Shared artifact-path resolver (`src/catalog/artifact_path.rs`) used
  by the web run page, the demo verifier, and any artifact-serving
  path; artifact rows show per-row availability; hash/size verified
  against catalog metadata.
- Dedicated `InsufficientVisibilityView` presentation (reviewed
  relationship, target origin, lifecycle, cutoff, qualifying-cohort
  zeros vs "Not applicable", vantage-point coverage table with
  collector/site/family/baseline SHA, no-UPDATE explanation, reviewed
  manifest notes quoted with attribution). Zero and not-applicable are
  distinct; no `null` renders.
- Event workflow: a completed imported run leads the workflow state;
  open events render Start / Source lifecycle / Analysis cutoff;
  queueing is not presented as the next step in the read-only demo.
- Snapshot provenance: fixture imports are disclosed as import
  provenance; the original source identity (GRNOC Public Task Viewer,
  external ID, snapshot SHA) leads.
- MAN LAN: analyzed targets derived from linked runs' manifests
  (NORDUnet moved out of the unresearched list); "Other
  operator-reported participants" keeps the remainder; incident-wide
  plan None vs completed single-target pilot; operator-first route
  story derived from stream lifecycles (first-change range,
  restoration range, cooldown re-change, scope limitation); linked-run
  table with target/family/collector/current result labels; per
  (prefix, collector) grouped evidence with peer-level detail inside a
  `<details>` disclosure; per-collector summaries; cross-observer
  timing deduplicated per collector; ticket provenance statuses
  derived from catalog state; "Collector site" label with a
  non-geography note.

## Canonical evidence

No canonical evidence changed: no artifact file, no manifest, no plan
payload, no reviewed JSON was modified. The pilot registry remains at
zero external sessions.

## Follow-up

See `docs/audits/2026-08-documentation-spec-conformance.md` for the
preceding documentation conformance audit.
