# inim Observability Model

## Core principle

inim never suggests that RFC 9003 text or RFC 8327 intent should appear at a remote public collector. Community absence is never treated as evidence of mechanism non-use.

## Observability matrix

| Signal / Mechanism | Protocol Scope | Represented in RIB/UPDATE data | Potentially visible remotely | Reliability | Permitted conclusion |
|---|---|---|---|---|---|
| RFC 9003 Administrative Shutdown Communication | BGP NOTIFICATION with Administrative Shutdown subcode | No — NOTIFICATION messages are not stored in MRT TABLE_DUMP_V2 | No | N/A | inim cannot assert that RFC 9003 shutdown was or was not used |
| RFC 8326 GRACEFUL_SHUTDOWN community 65535:0 | BGP path attribute — well-known discretionary COMMUNITY | Yes — stored in MRT when present; preserved through inim pipeline | Yes | Moderate — may be stripped or omitted | When present: "GRACEFUL_SHUTDOWN community observed on N streams." When absent: "No GRACEFUL_SHUTDOWN community reached the selected observers. This does not establish that the mechanism was not used." |
| RFC 8327 Session Culling / BGP Cease | Local router action — session termination | Only exported route consequences (withdrawals) visible | No (mechanism invisible) | N/A | inim may report withdrawals as "consistent with session culling" but must not assert culling occurred |
| BGP Graceful Restart | BGP capability negotiation (OPEN) + session state | GR capability not in MRT; route stability during restart may be observable | No (capability exchange invisible) | N/A | inim may note stable forwarding patterns but must not claim Graceful Restart |

## Classification labels

inim's user-facing reports separate mechanism hints from routing impact:

- **Observed mechanism hints** — RFC 8326 community seen on N streams; RFC 9003 not observable from these collectors
- **Observed routing impact** — withdrawals, path changes, prepend changes, restorations

## Evidence strength

| Level | Meaning |
|---|---|
| `ObservedDirectly` | The mechanism itself was observed (e.g. RFC 8326 community seen) |
| `ConsistentWith` | Observed behavior is consistent with the mechanism, but the mechanism itself was not directly visible |
| `NotObservable` | The mechanism is not observable from this dataset |
