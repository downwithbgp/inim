# Incident-family workbench deferral — 2026-08-03

Dated decision record. The Session 48 correction paused the
incident-family UI work until a genuinely IP-layer fresh event exists.

## State at the pause

- **No incident-family code was built.** The generic assessment-vocabulary
  separation, the applicability model, the bounded discovery tooling, and
  the reviewed corrections that were already implemented are source-neutral
  and retained.
- The MAN LAN family currently contains:
  - a valid NORDUnet public-BGP pilot (route changes at selected
    RouteViews and RIPE RIS observers), and
  - an ESnet **optical** relationship not directly observable through
    public BGP (the AS293/AS11537 run is a scope-mismatched supporting
    observation).
- That contrast is useful but is not a strong two-target BGP comparison.
  The family page would have presented an optical ticket as a target
  analysis, which the correction forbids.

## Decision

Defer the incident-family workbench (original Session 48 Parts 9-30) until
the fresh IP-layer event decision is complete. After the fresh event
(INC0303298, NOAA — executed 2026-08-03), reassess whether to:

1. add the fresh event to an incident family (the NOAA event is not part
   of the MAN LAN family; it would need its own reviewed grouping), or
2. broaden to a different operator/network, or
3. begin external NOC evaluation.

The NOC alpha evaluation kit extension (original Part 31) is deferred with
the family workbench: its incident-family task set depends on the family
page.
