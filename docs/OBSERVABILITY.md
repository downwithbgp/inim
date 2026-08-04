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
- **Best-path/export-policy limitation**: public collectors export their
  own best path per peer; policies, route filtering, and local preference
  shape what is visible. A route hidden at a collector may still exist
  in the network — absence at the selected observers is an observation
  about those sessions, not about the network.

## Claim observability matrix

Every case-study claim carries an explicit reviewed classification:

- **PotentiallyVisibleInPublicBgp** — the condition itself may appear as a
  public-BGP route-state change (e.g. participant path withdrawal, path
  shift to alternate transit, rollback restoration).
- **IndirectlyVisible** — only exported consequences may appear (e.g.
  administratively disabled customer interfaces; the action itself is not
  visible).
- **NotDirectlyVisible** — not observable in public BGP (e.g. Layer-2
  broadcast/unknown-unicast/multicast replication, physical cross-connect
  swaps, OESS circuit migration as an operation, missing switch
  telemetry).
- **Unknown** — classification not yet reviewed.

These are reviewed data, never hard-coded claims about any specific
incident. A NotDirectlyVisible claim is classified as
`NotDirectlyObservable` in the comparison matrix — never reported as a
missed detection; `no BGP change does not refute a Layer-2 incident`, and
`observed BGP change does not prove the reported mechanism`.

## Historical predicate validation

A candidate path predicate for the reviewed Internet2 transit presence in
NORDUnet paths (ContainsAny[11537]) is **candidate** until validated by
contemporaneous observation: the 2019-08-21 RouteViews RIB (Stage A
preflight) confirmed ContainsAny[11537] for AS2603 (33 streams), so the
pilot predicate is reviewed-by-observation rather than assumed. The
predicate is a proxy for the reviewed R&E-plane transit presence in the
NORDUnet (AS2603) target's paths; it never represents MAN LAN itself
(MAN LAN is a Layer-2 exchange fabric with no ASN for the case study,
and does not appear as an AS-path hop). A
NotDirectlyVisible condition stays NotDirectlyObservable even when a pilot
run exists; a narrow pilot's absence of observations never refutes
non-BGP-visible conditions, and never extends beyond its own window.

## Observer-coverage summaries (2026-08)

Event pages may render a reviewed **observation-coverage summary**
(collector-by-collector: collector, collector site, source family,
target-origin visibility, reviewed-relationship visibility, and the
qualification reason). Coverage summaries are derived from canonical
preflight/run evidence (reviewed per-collector counts, the reviewed
archive manifest, and the reviewed manifest analyst notes) and are
reviewed interpretation, not new evidence. Target-origin visibility and
reviewed-relationship visibility are distinct facts and are always
presented separately: target routes may be visible at a collector while
the reviewed relationship is not exposed by its baselines. Collector
site describes where the collector's route reflector is hosted; it is
not the observer peer's location. A coverage summary never claims a
relationship did not exist, that routing was stable, or that no outage
occurred — it states what the selected public observers could and could
not see.

## Pilot timing interpretation

Temporal relations preserve event order: for point action anchors the
relation is Before/After based on the earliest observed route-state
activity relative to the point, and the comparison row exposes the exact
times and delta ("order is explicit, no causal attribution"). A BGP
observation that precedes the reported interface action is never rendered
as a consequence of that action; restoration before a reported re-enable
is exposed. Broad instability intervals may legitimately overlap BGP
activity. Public BGP absence at one selected collector is a temporary
observer-stream observation, never proof of traffic loss.

## Observer sources are not ground truth

RouteViews and RIPE RIS are **observer sources**, not ground truth.
The corpus planner treats both families on equal footing
(`SourceFamily::RouteViews | SourceFamily::RipeRis`); collector
identity includes the family, RIS archives are planned with their own
URLs and cadence (bview 8-hour grid, 5-minute updates), and the
analyst-facing report never labels a RIS observer as RouteViews.
Observability classifications and verdicts are conditioned on the
frozen observer cohort of each AnalysisRun — never on ticket text
alone.

## Multi-observer agreement is not global proof

- **RIS and RouteViews are peer observer families.** Each selected
  collector is an independent vantage with its own baseline cohort,
  peer set, and archive coverage; runs are never merged into a combined
  verdict.
- **Different observers may legitimately disagree** — a prefix may be
  withdrawn at one collector while another observes only a path
  replacement, or a change may occur at different times. The
  comparison layer preserves per-observer rows and timing differences.
- **Bounded cross-observer vocabulary** — permitted statements:
  "Observed at multiple independent public collectors", "Observed only
  at one selected collector", "Similar route-state change with
  different timing", "No counterpart at this observer", "Insufficient
  baseline visibility". Forbidden: "globally confirmed", "complete
  outage", "traffic loss confirmed", "operator action confirmed".
- **Absence of baseline visibility is not absence of impact.** A
  prefix with no baseline stream at an observer is reported as
  insufficient visibility — never as "no change".
- **Batch reuse does not change evidence.** Sharing raw archives or
  derived caches across runs never merges event assessments; evidence
  ids do not depend on batch membership.

## Plane-scoped observability

- A named-plane cohort is observed through the exact reviewed predicate
  that selected it; "qualifying visibility" statements name the
  predicate and explicitly deny blanket visibility claims.
- Direct peer sessions and AS-in-path membership are separate evidence
  classes and render separately; an indirect R&E observation is never
  relabeled as a direct peering-plane observation.
- A missing plane baseline (no AS11164-in-path route at any selected
  observer) is reported as a missing baseline — never as "no event
  change" on that plane.
- Different observers expose different routing-policy views; agreement
  across them is still not global confirmation, and disagreement is
  expected and analytically useful.

## Incident workbench observability

- Workbench pages and APIs present **observed breadth by region** with
  visible denominators (`changed / eligible observer sessions`),
  never a severity score, never "percentage of the Internet affected".
- `NoRouteStateChange` (an observed signature with `Complete`
  coverage), `NoBaselineVisibility`, and `IncompleteCoverage` are
  distinct rendered states; a session without a qualifying baseline is
  never shown as unchanged.
- **Coverage reasons** name why a session is not in the eligible
  denominator: `EligibleWithBaseline`, `SessionPresentNoTargetBaseline`
  (session exists, target not visible), `RequiredSessionAbsent` (no
  historical session matches the reviewed relationship),
  `PredicateNotMatched`, `ArchiveIncomplete`, `UnsupportedSource`.
  "Required session absent" and "target not visible" are distinct
  facts and are never collapsed.
- Sentences per episode use effect-specific verbs and never claim
  traffic loss or causation; "returned to visibility" is distinct from
  "restored its baseline path".
- Suggested internal checks are labeled **investigation cues**
  traceable to observed facts (session, interval, plane, prefixes);
  they never name unreviewed devices, never claim root cause, and never
  generate device commands.
- The RRC11 historical audit distinguishes "no direct session in the
  2019 baseline" from "no AS2603 visibility": both facts are reported
  separately (`rrc11-audit-2019.json`), and the direct pilot decision
  records the exact blocking reason.

## Job observability

The job page shows factual progress with explicit units (archives
parsed, streams frozen) and never invents percentages from an unknown
denominator. Worker presence is freshness-based; a worker that is
absent is not unhealthy. Cancellation is cooperative and checked at
stage and archive boundaries, never per BGP element. A completed
analysis with InsufficientVisibility is a valid completed job, not a
worker failure.
