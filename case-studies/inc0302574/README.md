# INC0302574 — RIPE via NYIIX I2PX visibility audit (2026-07-30)

Reviewed case-study evidence for the RIPE-via-NYIIX event. This
directory contains the canonical run artifacts (`out/INC0302574/`),
including the reviewed relationship audit
(`out/INC0302574/relationship-audit.json`). This README records the
reviewed interpretation; every claim below traces to those artifacts.

## The ticket

The ticket names a **direct I2PX peer relationship** ("Brief Outage - I2
PX Peer RIPE via NYIIX (NEWA)"). The named relationship is an I2PX
peering-plane claim; it is not an R&E-plane claim.

## The relationship audit (event-date baselines)

- The event-date (2026-07-30) RIS baseline bviews at **RRC11 and RRC14
  had direct AS11164 sessions** (the collectors with direct AS11164
  peers per the current RIS peer lists; IPv4 and IPv6 each).
- **Zero AS3333-origin routes were visible through those sessions**, and
  **no AS3333-origin path contained AS11164** in the event-date
  baselines.
- The event-date bview peer table is authoritative; current peer lists
  are supporting context only (bview SHAs are recorded in
  `relationship-audit.json`).

## Verdict

- The named I2PX relationship is **not assessable through the selected
  public collectors**: decision `insufficient-visibility`.
- The existing AS11537 run (19 selected observer-prefix streams at
  route-views2 and route-views6, no route-state change observed) is
  **supporting R&E-plane evidence only** — it is classified
  `supporting-re-plane`.
- **No-change supporting evidence is not an assessment of the named
  I2PX relationship.** The absence of observed route-state changes does
  not prove that the attachment, circuit, or network was physically
  redundant, and it says nothing about a relationship that the selected
  observers could not see.

## Evidence

- `out/INC0302574/relationship-audit.json` — reviewed audit (bview
  SHAs, direct sessions, visibility counts, decision, supporting-run
  classification).
- `out/INC0302574/report.json` / `report.txt` — the generated report
  (schema v2).
- `out/INC0302574/lifecycle.json` — the R&E-plane run's canonical
  lifecycle evidence.
