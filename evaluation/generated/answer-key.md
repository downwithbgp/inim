# NOC alpha evaluation — facilitator answer key (generated)

Generated from reviewed tracked artifacts. This document is for the
facilitator only; it must not be distributed to evaluators.

- Generator: `scripts/build-evaluation-answer-key.py`
- Schema version: 1
- Source demo-manifest SHA-256: `e50d6a83685714f42a29221fc7f03a70c03a174832bfa4806786b4c71e48e960`

Every factual answer below carries a repository-relative artifact
reference. If a value contradicts the referenced artifact, the artifact
is authoritative and the contradiction is a P0 defect.

## Scenario: MAN LAN 2019-08-21 (multi-ticket operator incident)

- **Source event**: MAN LAN 2019-08-21 (multi-ticket operator incident)
- **Target**: NORDUnet (AS2603)
- **Reviewed relationship**: NORDUnet (AS2603) routes via the Internet2 R&E plane (AS11537)

### Incident context (Layer-2 fabric)

- **text**: MAN LAN is a Layer-2 exchange/fabric: it has no ASN for this case study, does not speak BGP, does not originate routes, and does not appear as an AS-path hop.
- **target**: NORDUnet AS2603 is the analyzed BGP target (one attached network); the completed pilot is NORDUnet-target-scoped, not MAN LAN BGP analysis.
- **path_evidence**: observed AS paths are public-collector evidence (route-views2 peer 64.57.28.241 and RIS observers); they show what the collector received, never switch-fabric state.
- **attachment_vs_adjacency**: Layer-2 attachment and AS-path adjacency are different evidence classes: attachment does not prove BGP adjacency, route export, a commercial relationship, traffic flow, or active state during the event.
- **entity_taxonomy**: Ixia is network test/measurement equipment, not a participating network and not a BGP peer. Not every source-mentioned entity is a reviewed fabric attachment: source mention is not proof of attachment, a reviewed ASN is not proof of attachment, and a familiar organization name is not proof of entity class.
- **reference**: `case-studies/manlan-2019/case-study.json`
- **Artifact**: `case-studies/manlan-2019/pilot/cross-observer-matrix.json`

### Route state answers

- **first_direct_absence_utc**: 2019-08-21T16:45:25+00:00
- **affected_prefix_count**: 11
- **absence_duration_seconds**: 2
- **returned_path**: 11537 22388 24489 24489 24489 24489 24490 20965 2603 (still traverses AS11537)
- **exact_baseline_restoration_range_utc**: 2019-08-21T16:59:26Z .. 2019-08-21T17:02:03Z
- **analysis_final_state**: exact event-baseline path present at analysis end (18:30:00 UTC)
- **rrc15_cooldown**:
  - count: 11
  - first_change_utc: 2019-08-21T17:52:16+00:00
  - note: path replacements in the cooldown window; no restoration observed before analysis end
  - reference: case-studies/manlan-2019/pilot/out/MANLAN-2019-NORDUNET-PILOT-RIS-RRC15/report.json
- **reference**: `case-studies/manlan-2019/pilot/pilot-result.json`

### Observed result

- **direct_observer**: 11 of 33 selected observer-prefix streams (all AS2603 via AS11537) became absent at 16:45:25Z and returned at 16:45:27Z (2 s); 30 path replacements followed (16:45:26-17:02:03Z), with 11 streams departing the reviewed transit and 11 retaining it with material path changes; all 33 streams returned to baseline paths by 17:02:19Z.
- **finding**: Public BGP exposed a temporally related routing consequence for one reviewed participant (NORDUnet, AS2603) at one selected public collector during the reported incident — transient route-state disruption temporally associated with the broader reported instability interval, not attributed to either specific interface action. Single-target, single-collector, single-window pilot; not a complete MAN LAN incident verdict.
- **reference**: `case-studies/manlan-2019/pilot/pilot-result.json`

### Non-conclusions

- collector site (Eugene, Oregon, US) does not establish peer location or target location
- observer-route absence does not prove traffic loss
- a 2-second absence at one observer may reflect session behavior rather than the participant's own action
- this is a single-target, single-collector-direct pilot — not a complete MAN LAN incident verdict
- temporal association with the reported instability interval is not attribution to a specific interface action

### Likely confusion (facilitator markers)

- exact baseline returned (17:02:03Z) versus final route state
- one observer's result (route-views2) versus all observers
- collector site versus peer location

### Evidence needed

- the direct observer session (route-views2 peer 64.57.28.241) for the absence/return pair
- the exact-baseline restoration timestamps per prefix (lifecycle.json)
- the RRC15 cooldown transitions (report.json transitions.cooldown = 11)

### Unsupported stronger conclusion

- attributing the absence to the reported 16:50 interface-disable action
- claiming traffic interruption from the 2-second BGP absence
- an incident-wide MAN LAN assessment from this single-target pilot

## Scenario: INC0299001

- **Source event**: INC0299001
- **Target**: UVA via Internet2
- **Reviewed relationship**: UVA (AS225) via Internet2 (AS11537)
- **Artifact**: `manifests/INC0299001.json`

### Route state answers

- **event_baseline_path**: 11537 40220 225 225 225 225 225 225 225
- **pre_withdrawal_path**: 11537 40220 225
- **prepend_count_change**: AS225 prepend reduced from 7 to 1 while routes remained visible
- **withdrawal_timestamp**: 2026-07-14T07:33:59.462019920Z
- **return_timestamp**: 2026-07-14T07:33:59.516258955Z
- **absence_duration_secs**: 0.054
- **first_returned_path**: 11537 40220 225 225 225 225 225 225 225
- **final_path**: 11537 40220 225
- **final_matches**: pre-withdrawal route (AS225×1), not the event baseline (AS225×7)
- **principal_prefix_count**: 11
- **example_prefixes**: ['128.143.0.0/16', '137.54.0.0/16', '192.131.232.0/24']
- **outlier_prefix**:
  - prefix: 137.54.122.0/23
  - absence_duration_secs: 120.993
  - baseline_path: 11537 40220 225
  - final_path: 11537 40220 225
  - note: baseline already the reduced path; much longer absence than the 11-prefix group
- **reference**: `case-studies/inc0299001/finding-chronology-audit.json`

### Observed result

- **verdict**: Partial routing impact observed
- **finding**: The event produced partial and heterogeneous external routing impact. 13 of 48 selected observer-prefix streams became absent and later returned. Among the remaining 35 streams, 22 showed prepend-only changes, 11 had other material path changes while retaining the reviewed transit, and 2 remained visible after departing that transit.
- **reference**: `case-studies/inc0299001/out/INC0299001/report.json`

### Non-conclusions

- observer-route absence does not prove traffic loss
- the 54 ms absence at one observer session is not a measured outage duration
- exact baseline restoration at return does not mean the final state matched the baseline

### Likely confusion (facilitator markers)

- event baseline (AS225×7) versus pre-withdrawal route (AS225×1)
- prepend change (07:24:47Z) versus withdrawal (07:33:59Z)
- 11-prefix group versus the 12th prefix (137.54.122.0/23)

### Evidence needed

- the finding-chronology audit for prefix-level baseline/withdrawal/return/final paths
- the report.json finding for the 13-of-48 stream signature

### Unsupported stronger conclusion

- claiming the final state restored the event baseline
- claiming traffic impact from BGP absence

## Scenario: INC0302574

- **Source event**: INC0302574
- **Target**: RIPE (AS3333)
- **Reviewed relationship**: direct I2PX peer relationship (ticket: 'Brief Outage - I2 PX Peer RIPE via NYIIX (NEWA)')
- **Artifact**: `case-studies/inc0302574/out/INC0302574/relationship-audit.json`

### Route state answers


### Direct sessions reviewed

| Collector | Peer IP | Family | Peer ASN | AS3333-origin routes |
|---|---|---|---|---|
| rrc11 | 198.32.160.221 | ipv4 | 11164 | 0 |
| rrc11 | 2001:504:1::a501:1164:1 | ipv6 | 11164 | 0 |
| rrc14 | 198.32.176.128 | ipv4 | 11164 | 0 |
| rrc14 | 2001:504:d::1:1164:2 | ipv6 | 11164 | 0 |

- **Non-qualification reason**: all four direct AS11164 sessions existed on 2026-07-30 but carried zero AS3333-origin routes; no AS3333-origin path contained AS11164; no qualifying I2PX baseline exists

- **Assessment**: Insufficient public-collector visibility for the named I2PX relationship: the direct AS11164 sessions existed at RRC11 and RRC14 on 2026-07-30, but no AS3333-origin route was visible through them, and no AS3333-origin path contained AS11164. No qualifying I2PX baseline exists; the relationship cannot be assessed with public-collector evidence.

- **Strongest supported conclusion**: the named I2PX relationship cannot be assessed from public-collector evidence at the event date; the direct sessions existed but carried no target-origin baseline, so no route-state claim about the relationship is supported

### Supporting observation

- **text**: Across 19 selected observer-prefix streams at route-views2, route-views6, inim observed no announcements, withdrawals, path changes, or community changes during the event analysis window.
- **verdict**: No route-state change observed
- **baseline_observer_prefix_streams**: 19
- **collectors**: ['route-views2', 'route-views6']
- **note**: supporting AS11537 observation only; does not assess the named I2PX relationship
- **reference**: `case-studies/inc0302574/out/INC0302574/report.json`

### Non-conclusions

- the supporting no-change observation (route-views2/route-views6, AS11537) does not assess the named I2PX relationship
- a direct AS11164 session existing does not make the relationship observable without a target-origin baseline
- no route-state change on a supporting plane is not 'the I2PX relationship was stable'

### Likely confusion (facilitator markers)

- direct AS11164 session existed (true) versus a qualifying baseline existed (false)
- AS3333-origin routes visible via other peers versus visible through the I2PX sessions
- supporting AS11537 evidence treated as named-relationship evidence

### Evidence needed

- the relationship-audit direct-session rows with zero AS3333-origin counts
- the supporting report.json 19-stream no-change observation

### Unsupported stronger conclusion

- claiming the I2PX relationship was stable
- claiming the event had no routing impact from a supporting plane

## Scenario: INC0301970

- **Source event**: INC0301970
- **Target**: Smithville via Indiana GigaPOP
- **Reviewed relationship**: Indiana GigaPOP (AS19782) peer Smithville (AS11550)
- **Artifact**: `manifests/INC0301970.json`

### Route state answers


### Visibility facts

- **as11550_routes_visible**: True
- **as11550_prefix_count**: 13
- **as11550_transit**: Cogent/Telia/BroadbandONE transit only (no AS19782)
- **routes_traversing_as19782**: 0
- **direct_as19782_sessions**: 0
- **as11550_at_route_views6**: 0
- **note**: facts from the reviewed manifest analyst notes (event-date baseline preflight)
- **reference**: `manifests/INC0301970.json`

- **Provisional cutoff**: 2026-08-04T00:01:37Z
- **Provisional language**: source event remains open; result is provisional through the reviewed snapshot cutoff; a later source refresh creates a new snapshot and run
- **Why insufficient visibility**: no selected observer had an event-baseline route exposing the reviewed AS19782-AS11550 adjacency, so no qualifying baseline cohort exists; the run records InsufficientVisibility with no UPDATE acquisition. This is distinct from observing no route-state change: there was no qualifying observation at all. Target-origin visibility (AS11550 routes were visible) and reviewed-relationship visibility (none exposed the adjacency) are separate.

### Non-conclusions

- no qualifying relationship evidence was observed through the reviewed cutoff
- not claimed: no relationship existed
- not claimed: no routing change occurred
- not claimed: Smithville was unaffected
- the named peer relationship is not all Smithville connectivity
- the result is provisional; the source event remained open at the cutoff

### Likely confusion (facilitator markers)

- target AS11550 routes were visible (true) versus the reviewed AS19782–AS11550 relationship was visible (false)
- insufficient visibility versus no change
- named peer relationship versus all Smithville connectivity

### Evidence needed

- the manifest analyst notes (event-date baseline preflight counts)
- the report.json InsufficientVisibility assessment

### Unsupported stronger conclusion

- claiming Smithville had no routing change
- claiming the peer relationship was stable
- claiming Smithville was unaffected by the outage

## Scenario: INC0040293

- **Source event**: INC0040293
- **Target**: ESnet (Energy Sciences Network)
- **Reviewed relationship**: I2 Optical Participant ESnet (optical participant relationship)
- **Artifact**: `case-studies/manlan-esnet-2019/out/INC0040293/report.json`

### Route state answers


- **Scope statement**: public BGP does not directly observe the named optical participant interface

### Supporting observation

- **text**: Across 3 selected observer-prefix streams at route-views2, inim observed no announcements, withdrawals, path changes, or community changes during the event analysis window.
- **verdict**: Unexpected continued reviewed-transit path
- **baseline_observer_prefix_streams**: 3
- **collectors**: ['route-views2']
- **note**: contemporaneous supporting observation with scope mismatch; retained separately
- **reference**: `case-studies/manlan-esnet-2019/out/INC0040293/report.json`

### Non-conclusions

- public BGP cannot assess the optical interface state
- stable contemporaneous BGP routes do not assess an optical interface
- not claimed: less impact than expected
- not claimed: no optical impact
- not claimed: optical service stayed available

### Likely confusion (facilitator markers)

- contemporaneous stable BGP routes treated as an optical-interface assessment

### Evidence needed

- the report.json scope-mismatch supporting observation
- the event detail page's 'not directly assessable with public BGP' statement

### Unsupported stronger conclusion

- claiming the optical service stayed available
- comparing the supporting observation as an IP-participant result

## Scenario summary table

| Scenario | Named relationship | Target | Source lifecycle | Observer eligibility | Observed result | Expectation assessment | Final state | Primary limitation |
|---|---|---|---|---|---|---|---|---|
| nordunet-route-changes | NORDUnet (AS2603) routes via the Internet2 R&E plane (AS11537) | NORDUnet (AS2603) | Closed | 1 direct (route-views2) + 3 indirect RIS observers | direct observer: 11 streams absent 2 s, returned, baseline restored | not incident-wide; pilot only | exact event-baseline path present at analysis end (18:30:00 UTC) | single direct observer; 2 s absence may be session behavior |
| uva-prepend-withdrawal | UVA (AS225) via Internet2 (AS11537) | UVA via Internet2 | Closed | direct AS11537 session at route-views2; 48 baseline streams | Partial routing impact observed | ParticipantRelationshipUnavailable (no parenthesized site code in the title) | 11537 40220 225 (pre-withdrawal route (AS225×1), not the event baseline (AS225×7)) | observer-scoped BGP only; no traffic measurement |
| i2px-not-assessable | direct I2PX peer relationship (ticket: 'Brief Outage - I2 PX Peer RIPE via NYIIX (NEWA)') | RIPE (AS3333) | Closed | 4 direct AS11164 sessions existed; 0 qualifying baselines | named relationship not assessable (insufficient visibility) | no expectation assessment for the named relationship | not assessable | no target-origin baseline through direct sessions |
| smithville-insufficient-visibility | Indiana GigaPOP (AS19782) peer Smithville (AS11550) | Smithville via Indiana GigaPOP | True | target visible; reviewed relationship absent; 0 direct sessions | InsufficientVisibility | insufficient visibility; no expectation assessment possible | no qualifying observation through cutoff (provisional) | open event; provisional cutoff; relationship not visible |
| esnet-optical-scope | I2 Optical Participant ESnet (optical participant relationship) | ESnet (Energy Sciences Network) | Closed | 1 supporting observer (scope mismatch) | supporting observation only | not stated | not assessable (optical scope) | optical interface not observable in public BGP |

No severity, success/failure, incident verdict, or ranking is derived.
