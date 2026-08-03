# MAN LAN ticket readiness review — 2026-08

Audit date: 2026-08-02 (corrected 2026-08-03). Source: the ten
immutable GRNOC snapshots now tracked under
`case-studies/manlan-2019/corpus/snapshots/`, the reviewed
interpretations in `pilot/ticket-reviews.json` (Session 34, with the
2026-08-02 applicability correction), and the historically-reviewed
target research in `target-research.json` (Session 31). No new tickets
were fetched; TASK0038206 and TASK0038211 remain unresolved references
(the viewer does not serve TASK records).

**2026-08-03 correction:** the two ESnet tickets are **optical
participant** relationships, not Internet2 IP participant or I2PX BGP
relationships (ticket titles 'I2 Optical Participant ESnet' / 'MANLAN
Optical Participant ESNet'; ticket text 'manually disabled this
interface' / 'internal testing by ESNet'). Both are reclassified
`NotDirectlyObservableInPublicBgp`; the AS293/AS11537 run for
INC0040293 is a preserved scope-mismatched supporting observation and
is NOT primary evidence for the optical relationship. INC0040258 (WIX
interconnect) and INC0040290 (Ixia) are reclassified
`NotOriginAttributable`.

## Readiness table

| Ticket | Reviewed role | Applicability | Target | Origin ASN | Origin evidence | Selection mode | Predicate | Timing | Preflight | Exact blockers |
|---|---|---|---|---|---|---|---|---|---|---|
| CHG0038258 | ChangeWindow | NotApplicableToPublicBgp | MAN LAN core node | — | — | — | — | window 04:38–13:00 | not run | change record, not a routed event |
| CHG0038386 | RollbackOrRecovery, ChangeWindow | NotApplicableToPublicBgp | MAN LAN core node | — | — | — | — | window 20:00–22:29 | not run | change record, not a routed event |
| INC0040257 | PrimaryIncident | PotentiallyVisibleInPublicBgp | MAN LAN participants (various) | — | none (no single target) | — | — | 13:00–19:59 | not run | target mapping not reviewed (no single entity) |
| INC0040258 | ParticipantImpact | NotOriginAttributable | MAN LAN–WIX interconnect | — | not origin-attributable (exchange fabric; Session 31 research) | — | — | 13:26–13:27 | not run | not origin-attributable |
| INC0040272 | ParticipantImpact | PotentiallyVisibleInPublicBgp | NORDUnet | 2603 | historically reviewed (2019 PeeringDB + registry) | OriginThroughTransit | Candidate ContainsAny[11537] | no source window in ticket | not run | EXCLUDED: NORDUnet pilot rerun |
| INC0040289 | ParticipantImpact | PotentiallyVisibleInPublicBgp | NORDUnet | 2603 | historically reviewed | OriginThroughTransit | Candidate ContainsAny[11537] | 16:16–21:33 | not run | EXCLUDED: NORDUnet pilot rerun |
| INC0040290 | ParticipantImpact | NotOriginAttributable | Ixia | — | none (test equipment; no ASN; Session 31 research) | — | — | 16:04–open | not run | no origin identity exists |
| INC0040291 | ParticipantImpact | **NotDirectlyObservableInPublicBgp** | ESnet (optical participant) | — | optical relationship; not an IP BGP target | — | — | 16:31–06:00+1d | not run | optical participant relationship (MANLAN Optical Participant ESNet; manual interface disable per ticket text); next action: review optical/telemetry evidence outside public-BGP scope |
| INC0040293 | ParticipantImpact | **NotDirectlyObservableInPublicBgp** | ESnet (optical participant) | — | optical relationship; not an IP BGP target | — | — | 16:36–20:25 | supporting run preserved (scope mismatch) | optical participant relationship (I2 Optical Participant ESnet; internal-testing per ticket text); the AS293/AS11537 run is a contemporaneous supporting observation only |
| INC0040318 | AlarmOrTelemetry | NotApplicableToPublicBgp | MAN LAN core node (CPU) | — | — | — | — | alarm 19:34 | not run | telemetry alarm, not a routed event |

## Notes

- Applicability from Session 34 (`ticket-reviews.json`) was audited
  against the immutable snapshots and retained as reviewed: the change
  records and the CPU alarm are not routed events; the WIX and Ixia
  tickets are not origin-attributable per the Session 31 research.
- NORDUnet tickets are excluded from execution because the accepted
  MAN LAN / NORDUnet pilot already covers that target.
- The ESnet tickets are **not** public-BGP candidates: they are
  optical participant relationships (corrected 2026-08-03).
- After the correction, none of the ten MAN LAN tickets is a Ready
  public-BGP candidate: NORDUnet is excluded (pilot rerun), WIX/Ixia
  are not origin-attributable, the ESnet tickets are optical, and the
  change/alarm records are not routed events. Fresh public-BGP events
  come from the bounded viewer discovery instead
  (`docs/audits/2026-08-fresh-event-discovery.md`).
