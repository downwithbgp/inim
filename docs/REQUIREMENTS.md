# inim — Requirements

## What inim does

inim (Internetwork Impact Monitor) determines how planned or unplanned
network events affect the globally visible routing system. It compares
operator-declared expectations (from maintenance/incident tickets) against
observable BGP route-state changes from public route collectors like
RouteViews.

## Core research question

> Did the externally observable BGP impact of an event match the
> operational expectation expressed by its ticket, and what temporal
> routing-impact pattern occurred?

## Supported sources

The first supported network is Internet2 because its public GRNOC tickets
provide unusually useful operator-declared expectations: precise time
windows, participant names, exchange names, router/site codes, and
descriptions. The core domain model is network-agnostic; Internet2
specifics live in an adapter.

## Internet2 naming convention

Internet2 ticket titles use a parenthesized site/attachment code (e.g.
`(NEWY32AOA)`) that indicates expected redundancy: one attachment may be
unavailable, but the participant/peer should remain reachable through
another path. Titles without this convention indicate loss of reachability
may be expected.

This is Internet2-specific. Do not silently treat it as a universal
naming convention.

## Verdict vocabulary

| Verdict | Meaning |
|---|---|
| `EXPECTED_REDUNDANT_IMPACT` | Impact matched the redundant-failover expectation |
| `EXPECTED_LOSS_OF_REACHABILITY` | Impact matched the loss-of-reachability expectation |
| `UNEXPECTED_WITHDRAWALS` | Unexpected withdrawals contrary to declared redundancy |
| `REDUNDANCY_FAILURE_OBSERVED` | Redundancy failed — reachability was lost |
| `UNEXPECTED_BLAST_RADIUS` | Impact extended beyond the declared participant set |
| `LESS_IMPACT_THAN_EXPECTED` | Less impact than declared |
| `NO_OBSERVABLE_BGP_IMPACT` | No BGP impact visible from available observers |
| `INSUFFICIENT_VISIBILITY` | Not enough observer coverage to assess |
| `INDETERMINATE` | Unable to determine a verdict |

## First vertical slice

The MVP demonstrates one complete analysis:

**Input:**
- One Internet2 ticket fixture (JSON)
- One preceding route-state fixture (simulated RIB)
- One short ordered update fixture (simulated UPDATEs)
- Two or more observer perspectives

**Scenario:**
- Parenthesized Internet2 event declares redundant availability
- Baseline route changes to an alternate path
- No complete loss of reachability occurs
- Alternate remains stable
- Baseline later returns

**Output:**
- Parsed redundant expectation
- Reconstructed before/during/after states
- One or more detected waves
- SEQUITUR-derived failover/restoration structure
- `EXPECTED_REDUNDANT_IMPACT` verdict
- Evidence linking verdict to source records

## Non-goals for MVP

- Analyze all RouteViews history
- Automatically resolve participants to prefixes
- Operate continuously
- Predict failures
- Infer physical topology
- Web dashboard
- Classify every possible maintenance type
- Universal anomaly score

## Design principles

1. **Minimalism**: Small number of explicit domain concepts, one sequence-analysis mechanism (SEQUITUR). No framework-heavy architecture.
2. **Correctness before scale**: Correct BGP state reconstruction is more important than archive throughput.
3. **Auditability**: Every conclusion is traceable to concrete ticket fields and individual MRT observations.
4. **Network neutrality**: Core domain is source-agnostic. Adapters supply network-specific logic.
5. **Honest semantics**: The program observes control-plane effects, not physical causes.
