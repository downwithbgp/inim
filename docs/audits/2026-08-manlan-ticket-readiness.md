# MAN LAN ticket readiness review — 2026-08

Audit date: 2026-08-02. Source: the ten immutable GRNOC snapshots now
tracked under `case-studies/manlan-2019/corpus/snapshots/`, the
reviewed interpretations in `pilot/ticket-reviews.json` (Session 34),
and the historically-reviewed target research in `target-research.json`
(Session 31). No new tickets were fetched; TASK0038206 and TASK0038211
remain unresolved references (the viewer does not serve TASK records).

## Readiness table

| Ticket | Reviewed role | Applicability | Target | Origin ASN | Origin evidence | Selection mode | Predicate | Timing | Preflight | Exact blockers |
|---|---|---|---|---|---|---|---|---|---|---|
| CHG0038258 | ChangeWindow | NotApplicableToPublicBgp | MAN LAN core node | — | — | — | — | window 04:38–13:00 | not run | change record, not a routed event |
| CHG0038386 | RollbackOrRecovery, ChangeWindow | NotApplicableToPublicBgp | MAN LAN core node | — | — | — | — | window 20:00–22:29 | not run | change record, not a routed event |
| INC0040257 | PrimaryIncident | PotentiallyVisibleInPublicBgp | MAN LAN participants (various) | — | none (no single target) | — | — | 13:00–19:59 | not run | target mapping not reviewed (no single entity) |
| INC0040258 | ParticipantImpact | ApplicableTargetNotYetMapped | MAN LAN–WIX interconnect | — | not origin-attributable (exchange fabric; Session 31 research) | — | — | 13:26–13:27 | not run | not origin-attributable |
| INC0040272 | ParticipantImpact | PotentiallyVisibleInPublicBgp | NORDUnet | 2603 | historically reviewed (2019 PeeringDB + registry) | OriginThroughTransit | Candidate ContainsAny[11537] | no source window in ticket | not run | EXCLUDED: NORDUnet pilot rerun |
| INC0040289 | ParticipantImpact | PotentiallyVisibleInPublicBgp | NORDUnet | 2603 | historically reviewed | OriginThroughTransit | Candidate ContainsAny[11537] | 16:16–21:33 | not run | EXCLUDED: NORDUnet pilot rerun |
| INC0040290 | ParticipantImpact | ApplicableTargetNotYetMapped | Ixia | — | none (test equipment; no ASN; Session 31 research) | — | — | 16:04–open | not run | no origin identity exists |
| INC0040291 | ParticipantImpact | PotentiallyVisibleInPublicBgp | ESnet | 293 | historically reviewed (2019 PeeringDB + ARIN autnum, 1997) | OriginThroughTransit | Candidate ContainsAny[11537] | 16:31–06:00+1d | **pending (event-date RIB)** | none identified yet |
| INC0040293 | ParticipantImpact | PotentiallyVisibleInPublicBgp | ESnet | 293 | historically reviewed | OriginThroughTransit | Candidate ContainsAny[11537] | 16:36–20:25 | **pending (event-date RIB)** | none identified yet |
| INC0040318 | AlarmOrTelemetry | NotApplicableToPublicBgp | MAN LAN core node (CPU) | — | — | — | — | alarm 19:34 | not run | telemetry alarm, not a routed event |

## Notes

- Applicability from Session 34 (`ticket-reviews.json`) was audited
  against the immutable snapshots and retained as reviewed: the change
  records and the CPU alarm are not routed events; the WIX and Ixia
  tickets are not origin-attributable per the Session 31 research.
- NORDUnet tickets are excluded from execution because the accepted
  MAN LAN / NORDUnet pilot already covers that target.
- The ESnet tickets are the only candidates with a reviewed target,
  reviewed origin ASN (AS293, high confidence, dated evidence), a
  candidate reviewed plane (Internet2 AS11537 — to be validated by
  event-date preflight), and closed windows.
- Selection mode for all routed candidates is
  OriginThroughTransit(ContainsAny[11537]); no origin-only mode is
  required by any of the ten tickets (see the route-selection review).
