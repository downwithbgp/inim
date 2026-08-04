# inim — public status

Current public status page. This document describes the state of the
project at the time of writing; it is not a specification of behavior.
Normative descriptions live in the documents it links.

## Stage

**Public alpha.** The product is under active development and the
external evaluation period is open. Nothing here is production-ready,
externally validated, or NOC-approved.

## Evaluation state

- The **NOC alpha evaluation kit is prepared**: a deterministic,
  read-only, offline demo with five reviewed scenarios, a task
  booklet, and a facilitator answer key generated from canonical
  artifacts.
- **Zero external evaluation sessions have been completed.** The pilot
  registry records this truthfully
  (`docs/evaluation/PILOT-REGISTRY.md`). Internal procedural dry runs
  are not external sessions.
- The **alpha evaluation freeze is active**
  (`docs/evaluation/ALPHA-FREEZE.md`): product, semantic, and evidence
  changes are limited to demonstrated defects; documentation
  maintenance remains allowed.

## Product boundary

**Local, event-conditioned public-BGP analysis and workbench.** inim
relates operator-declared network events to route-state changes
observed at selected RouteViews and RIPE RIS collector sessions. It
does not measure traffic, does not observe private networks, does not
determine root cause, and does not assess global reachability.

## Interfaces

- **Primary human interface:** server-rendered local web application
  (workbench, event catalog, case studies, plan review, jobs, queue,
  corpus). See `docs/UX.md`.
- **Administrative interface:** CLI (`docs/reference/CLI.md`).
- **Execution boundary:** a separate `inim worker` process claims and
  executes queued analysis jobs; the web process never executes
  analysis.

## Web behavior

- **Default: read-only.** HTTP GET requests never analyze, acquire, or
  mutate.
- **Write behavior:** local mutations (queue, cancel, retry, plan
  edits) require explicit `--enable-writes`, are loopback-only by
  default, require a process-lifetime CSRF token, and are intended for
  trusted local use only. There is no authentication.

## Supported sources

- **Public BGP families:** RouteViews and RIPE RIS (MRT archives),
  as independent observer families.
- **Source-ticket support:** bounded current source adapters — the
  GRNOC Public Task Viewer (`docs/sources/GRNOC_PUBLIC_TASK_VIEWER.md`)
  for the event catalog. Source access is polite, budget-bounded, and
  explicit.

## Tracked reviewed examples

Five reviewed case studies under `case-studies/`:

- MAN LAN / NORDUnet historical pilot (2019-08-21)
- UVA participant-unavailability event (INC0299001)
- RIPE-via-NYIIX I2PX visibility audit (INC0302574)
- ESnet optical participant records (INC0040293, INC0040291)
- Indiana GigaPOP–Smithville peer relationship (INC0301970,
  provisional open-event analysis)

## Current major limitations

- Observer-scoped BGP control-plane evidence only; no traffic,
  physical-layer, optical-interface, or Layer-2 observability.
- Route absence at one observer is not traffic loss; no qualifying
  baseline is insufficient visibility, not "no change".
- Temporal association with an operator-declared event is not
  causation.
- The evaluation demo is a bounded offline catalog; it is not a live
  monitoring service.

## Deferred areas

- Incident-family workbench (deferred; not restarted during the
  freeze)
- Static HTML export of the demo (deferred pending evaluator need)
- Broader event acquisition (paused during the freeze)
- First tagged alpha release (a post-pilot decision option, not
  automatic; see `docs/evaluation/POST-PILOT-DECISION-GATE.md`)

## Next action

The next human action is to invite one real network engineer to
complete the prepared external alpha evaluation.

## Links

- Alpha freeze: `docs/evaluation/ALPHA-FREEZE.md`
- Architecture: `docs/DESIGN.md`
- Observability: `docs/OBSERVABILITY.md`
- Evaluation kit: `docs/evaluation/NOC-ALPHA-EVALUATION.md` and
  `docs/evaluation/` (evaluator and facilitator material)
- Post-pilot decision gate: `docs/evaluation/POST-PILOT-DECISION-GATE.md`
- Documentation map and authority model: `docs/README.md`
