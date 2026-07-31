# inim Observability Model

## Core principle

inim never suggests that RFC 9003 text or RFC 8327 intent should appear
at a remote public collector. Community absence is never treated as
evidence of mechanism non-use. Mechanism hints are reported separately
from routing impact and can never change the impact assessment by
themselves.

## Observability matrix

| Signal / Mechanism | Protocol Scope | Represented in RIB/UPDATE data | Potentially visible remotely | Reliability | Permitted conclusion |
|---|---|---|---|---|---|
| RFC 9003 Administrative Shutdown Communication | BGP NOTIFICATION with Administrative Shutdown subcode | No — NOTIFICATION messages are not stored in MRT TABLE_DUMP_V2 | No | N/A | "RFC 9003: administrative-shutdown message not observable from these remote collector sessions." |
| RFC 8326 GRACEFUL_SHUTDOWN community 65535:0 | BGP path attribute — well-known discretionary COMMUNITY | Yes — stored in MRT when present; preserved through inim pipeline | Yes | Moderate — may be stripped or omitted | When present: "RFC 8326 GRACEFUL_SHUTDOWN community was observed on N selected observer-prefix streams." When absent: "No RFC 8326 GRACEFUL_SHUTDOWN community reached the selected observers. Its absence does not establish that graceful shutdown was not used." |
| RFC 8327 Session Culling / BGP Cease | Local router action — session termination | Only exported route consequences (withdrawals) visible | No (mechanism invisible) | N/A | "RFC 8327: operational intent not directly observable." Withdrawals may be consistent with session culling but culling must not be asserted |
| BGP Graceful Restart | BGP capability negotiation (OPEN) + session state | GR capability not in MRT; route stability during restart may be observable | No (capability exchange invisible) | N/A | "Graceful Restart: negotiated session capability/state not directly observable from this dataset." |

## Classification labels

inim's reports separate mechanism hints from routing impact:

- **Observed event signature** — expectation, lifecycle, observer scope,
  stream/instance counts, lifecycle categories, semantic waves, final
  impact assessment
- **Observable mechanism hints** — RFC 8326 GSHUT observations (with
  per-stream timing), community-only change counts, and explicit
  non-observability statements for RFC 9003 / RFC 8327 / Graceful
  Restart

## Evidence strength

| Level | Meaning |
|---|---|
| `ObservedDirectly` | The mechanism itself was observed (e.g. RFC 8326 community seen) |
| `ConsistentWith` | Observed behavior is consistent with the mechanism, but the mechanism itself was not directly visible |
| `NotObservable` | The mechanism is not observable from this dataset |

## Lifecycle tracking of RFC 8326

Per observer-prefix stream, the lifecycle records:
- GSHUT present at baseline
- GSHUT newly added / removed (with first addition and last presence
  timestamps)
- GSHUT present before a stream withdrawal or path replacement
- tag-to-consequence duration (first addition → first withdrawal or
  replacement while tagged)
- GSHUT removed during a restoration

## Limitations contract (reported in every completed event)

- Selected collectors do not provide global visibility
- BGP route state is not traffic measurement
- Local session state is not observed
- Physical-link state is not observed
- Absent communities do not prove a mechanism was unused
- Event declarations and BGP changes establish temporal association, not
  automatic causation
