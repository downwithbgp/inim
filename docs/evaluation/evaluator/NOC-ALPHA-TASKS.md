# NOC alpha evaluation — task booklet

Work through the tasks in order. Record each answer on the response
sheet (`NOC-ALPHA-RESPONSE-SHEET.md`). Do not search the repository
source code or the project documentation beyond the linked glossary —
everything you need is on the pages of the running demo.

Setup (already done for you by the facilitator, or via
`scripts/evaluator-bootstrap.sh`):

- Server: `http://127.0.0.1:8080/` (read-only demo, loopback only)
- Scenario 1 (NORDUnet): `http://127.0.0.1:8080/case-studies/manlan-2019/workbench`
- Scenario 2 (UVA): `http://127.0.0.1:8080/events/INC0299001/workbench`
- Scenario 3 (I2PX): `http://127.0.0.1:8080/events/INC0302574/workbench`
- Scenario 4 (Smithville): `http://127.0.0.1:8080/events/INC0301970/workbench`
- Scenario 5 (ESnet optical, optional): `http://127.0.0.1:8080/events/INC0040293/workbench`

Terms used in the tasks are defined in the glossary:
`docs/evaluation/evaluator/TERMS.md` (also linked from each page where
needed). Read a term only when a task uses it; you do not need to read
the glossary first.

Notes:

- All timestamps are UTC.
- "Observer" means a public BGP collector session (RouteViews or RIPE
  RIS) selected for the case.
- The demo is read-only: you cannot change anything, and nothing you do
  is recorded.
- If you are unsure, record what you think and your confidence. Hesitation
  is useful feedback, not a failure.

---

## Section A — Basic orientation (all scenarios)

Use the four scenario starting URLs above (NORDUnet, UVA, I2PX,
Smithville).

**Task A1.** Open the first provided event workbench (NORDUnet). State:

- the source event
- the target network
- the reviewed routing relationship

**Task A2.** For the same workbench, identify:

- source family
- collector
- collector site
- peer ASN
- peer IP

**Task A3.** Explain whether the peer's location is known from the
collector site.

---

## Section B — NORDUnet route change

Use the NORDUnet workbench (`/case-studies/manlan-2019/workbench`).

**Task B1 (core).** Identify the first externally observed route-state
change in the direct RouteViews observation.

**Task B2 (core).** Record:

- first-change timestamp
- affected prefix count
- two example prefixes
- observer peer

**Task B3 (core).** Compare the route before the absence with the first
route after visibility returned.

**Task B4 (core).** Determine:

- whether the returned route still traversed the reviewed plane
- whether the exact event-baseline route later returned

**Task B5 (core).** Determine the route state at analysis end.

**Task B6 (optional).** Find one observer whose result differed from
the direct RouteViews observer, and describe the difference.

---

## Section C — UVA chronology

Use the UVA workbench (`/events/INC0299001/workbench`).

**Task C1 (core).** Identify the route state at event baseline.

**Task C2 (core).** Identify the route immediately before withdrawal.

**Task C3 (core).** State the prepend-count change that occurred while
routes remained visible.

**Task C4 (core).** State:

- withdrawal timestamp
- return timestamp
- observer-route absence duration

**Task C5 (core).** Identify the first returned route.

**Task C6 (core).** Determine whether the final route matched:

- event baseline
- pre-withdrawal route
- neither

**Task C7 (optional).** Find the prefix whose lifecycle differed from
the principal 11-prefix group.

---

## Section D — I2PX not-assessable case

Use the I2PX workbench (`/events/INC0302574/workbench`).

**Task D1 (core).** State the relationship named by the source event.

**Task D2 (core).** Identify which direct public observer sessions were
reviewed.

**Task D3 (core).** State why those sessions did not qualify.

**Task D4 (core).** Determine whether the supporting R&E observation
assesses the named I2PX relationship.

**Task D5 (core).** State the strongest conclusion inim supports for
this case.

---

## Section E — Smithville second-network case

Use the Smithville workbench (`/events/INC0301970/workbench`).

**Task E1 (core).** Identify:

- managed network
- peer network
- reviewed AS relationship

**Task E2 (core).** State whether the source event was open or closed
at analysis time.

**Task E3 (core).** Identify the analysis cutoff.

**Task E4 (core).** Determine:

- whether AS11550-origin routes were visible at selected collectors
- whether any selected route traversed AS19782
- whether a direct AS19782 observer session existed

**Task E5 (core).** Explain why the result is "insufficient visibility"
rather than "no route-state change observed".

**Task E6 (core).** State one conclusion inim cannot make about
Smithville's total connectivity.

---

## Section F — Evidence navigation (optional deep dive)

**Task F1.** Find one exact evidence reference for a route transition.
Record: archive identity, collector, peer, prefix, timestamp.

**Task F2.** Find the route sequence for one prefix.

**Task F3.** Find the source snapshot or reviewed-plan provenance.

---

## Section G — Operational follow-up

These tasks have no single right answer; they tell us whether the
workbench supports the next investigative step.

**Task G1.** For one changed event (NORDUnet or UVA), state what
internal network data you would inspect next on your own equipment.

**Task G2.** For the Smithville case, state what additional external or
internal evidence would make the reviewed relationship assessable.

---

## Section H — Optical scope (optional, classification only)

Use the ESnet optical workbench (`/events/INC0040293/workbench`).

**Task H1 (optional).** State whether this event's named relationship
can be assessed from public BGP, and why.

---

## Timing

- Core tasks (A1–A3, B1–B5, C1–C6, D1–D5, E1–E6, G1–G2): about
  15–20 minutes.
- Optional tasks (B6, C7, F1–F3, H1): about 5 minutes.
- Total: about 20–30 minutes.

There is no strict time pressure; durations are recorded as
observations, not scored.
