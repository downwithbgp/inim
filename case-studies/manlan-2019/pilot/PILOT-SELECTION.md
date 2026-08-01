# Pilot selection — NORDUnet (AS2603)

Session 31, 2026-08-01. Evidence-based selection for the single narrow
historical pilot of the MAN LAN 2019 case study.

## Candidates reviewed

| Target | AAR-documented action (Appendix A/B) | Origin mapping | Predicate | Verdict |
|---|---|---|---|---|
| NORDUnet (AS2603) | Flapping reported 15:33; customer disables interface **16:50**; availability 16:50–21:33 (INC0040289); re-enabled 20:48 | HistoricallyReviewed (2019-dated PeeringDB capture 2019-08-26) | Candidate ContainsAny[11537] — empirically confirmed in Stage A (33 matching streams in the 2019-08-21 RIB) | **SELECTED** |
| ESnet (AS293) | Interface shut ~16:39; disabled 17:30; availability 16:55–22:24; re-enabled 20:44 | HistoricallyReviewed (2019-08-23 capture) | Candidate | Rejected — larger prefix set and independent trans-oceanic transit make attribution less clean; also the AAR documents ESnet's own interface actions overlapping NORDUnet's window, which would confound a second pilot |
| CANARIE (AS6509) | Dropped 12:34; optic swap at 12:03–12:34; services up 21:33; performance issues from ~14:14 | HistoricallyReviewed (2019-08-25 capture; MAN LAN presence documented) | Candidate | Rejected — the documented events (optic swap, dropped interface) occur during the maintenance window before the traffic-replication phase; the pilot spec prefers the 14:14/16:50-era replication boundary |
| WIX interconnect | Brief outage 13:26–13:27, resolved 14:40 | NotApplicableToPublicBgp (exchange fabric) | — | Rejected — not origin-attributable |
| Ixia | Outage 16:54 | NotApplicableToPublicBgp (test vendor) | — | Rejected — no public prefixes |
| NEAAR | VLAN no packets 13:04; up 13:32 | AmbiguousServiceIdentity | — | Rejected — no origin |
| OMAN | Rides CANARIE interface (down) | Unresolved | — | Rejected — no mapping |
| TWAREN (AS7539) | Interface not receiving light 13:45 | HistoricallyReviewed | Candidate | Rejected — 2019 US presence via PacificWave (West Coast); MAN LAN attachment less certain; small prefix set |
| SINET (AS2907) | Swapped with NORDUnet 15:48; BGP not re-establishing | HistoricallyReviewed | Candidate | Rejected — the documented event is a physical swap, harder to tie to a clean BGP signature |
| GÉANT (AS21320) | Still not up 13:13; peering via CANARIE pingable 13:46 | HistoricallyReviewed | Candidate | Rejected — events cluster in the troubleshooting phase, not the replication phase |

## Selected target

**NORDUnet (AS2603)** — rationale:

1. **Distinct documented operational action with an exact time**: the AAR
   records NORDUnet interface flapping at 15:33 and the customer disabling
   the interface at **16:50 UTC** to regain stability (INC0040272), with an
   availability window 16:50–21:33 (INC0040289). This is the incident's
   clearest participant-specific action.
2. **Historically reviewed origin mapping**: AS2603 = NORDUnet, confirmed
   by a 2019-08-26 PeeringDB capture (4 days after the incident) plus the
   registry record (high confidence).
3. **Predicate empirically confirmed in Stage A**: the 2019-08-21 02:00 RIB
   at route-views2 shows 526 AS2603 routes, 33 of them with AS11537 in path
   (Internet2 transit presence) — the candidate MAN LAN attachment
   predicate ContainsAny[11537] is validated by contemporaneous
   observation, not assumed.
4. **Manageable volume**: 33 baseline streams / 11 prefixes; the short
   window (16:00–17:30 UTC, warmup from 02:00) required ~199 UPDATE files
   (~0.6 GiB) — tractable.
5. **Operational relevance**: NORDUnet is an I2 PX participant per the AAR
   (INC0040289), directly relevant to the reported incident.

## Boundary choice

The 16:50 UTC interface disable is the anchor: the pilot window
16:00–17:30 UTC brackets it, with warmup from the 02:00 baseline RIB to
reconstruct continuous state (no state reset at any boundary).

## Pilot definition

- Target: NORDUnet, origin AS2603, predicate ContainsAny[11537]
- Collector: route-views2 (single peer 64.57.28.241 observed)
- Baseline qualifying streams: 33 (11 prefixes)
- Window: 2019-08-21 16:00–17:30 UTC; warmup 14 h; cooldown 1 h
- Expected archives: rib.20190821.0200.bz2 + ~199 update files ≈ 0.6 GiB
- Expected operator phases: traffic-replication incident (14:14–18:01),
  into rollback initiation (18:01)
