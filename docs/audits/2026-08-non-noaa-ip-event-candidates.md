# Non-excluded IP-event candidate review — 2026-08-03

Dated execution audit. The Session 48 fresh-event selection is outside
project scope (see `2026-08-project-scope-noaa-removal.md`); this audit
records the non-excluded candidate review for the Session 49 selection.
The filename uses "non-noaa" as a dated scope marker only; it is not
normative product vocabulary.

## Discovery (bounded, 2026-08-03)

- Mechanism: GRNOC Public Task Viewer incident search scoped to the
  Internet2 domain (`get_domains` + `get_incidents` with `domain_id`),
  through the polite adapter (`catalog sync grnoc --search --domain`).
- Requests: 3 searches + 1 exact fetch = 4 requests (budget 250);
  all HTTP 200; no throttle responses.
- The Internet2-domain surface returned 25 records (the Session 48
  surface was 32; 7 records dropped). Query text did not visibly filter.
- Excluded by project scope: none returned (the excluded record is no
  longer in the active surface).
- New record surfaced since Session 48: **INC0303502** "Outage - I2 PX
  Peer AMAZON via EQUINIX (SEAT)" — a fresh Amazon I2PX peer event.

## Shortlist (3; ≤5 per policy)

| Ticket | Title | Relationship | Target | Origin ASN status | Attachment scope | Route selection | Event-date observer | Blocker / readiness |
|---|---|---|---|---|---|---|---|---|
| INC0303502 | Outage - I2 PX Peer AMAZON via EQUINIX (SEAT) | I2PX peer (direct exchange-plane scope; "via EQUINIX" peer-mapping premise) | Amazon | AS16509 historically reviewed (ARIN RDAP, reg. 2000-05-04; Session 48) | peer relationship; no redundant-attachment qualifier applies | OriginThroughTransit(ContainsAny[11164]) | rrc11 bview.20260803.1600 + route-views2 rib.20260803.1800 | **blocked**: no DIRECT exchange-plane observer session. rrc11: 157,943 AS16509 routes, 0 via the reviewed plane. route-views2: 419 AS16509 paths containing the reviewed plane, all in-path via peer AS2152 (paths `[2152, 11164, 16509]`), 0 direct AS11164 sessions — an in-path observation is not a direct I2PX relationship |
| INC0303305 | Brief Outage - I2 Participant Front Range Gigapop (STAR) | IP participant (redundant-attachment convention) | Front Range Gigapop | **unresolved** — AS3856 is Packet Clearing House (PCH-AS, ARIN RDAP), not FRGP; no reviewed FRGP origin mapping exists | parenthesized (STAR) | (not reached) | — | target origin mapping unresolved; NOT guessed |
| INC0303264 | Availability - I2 Participant CLOUDFLARE | IP participant | Cloudflare | AS13335 reviewed (Session 48) | no qualifier | AS11537 and AS11164 scopes tested | route-views2 | **blocked, retained**: no qualifying baseline under either reviewed scope; unchanged premise (not re-preflighted) |

Known blocked (not promoted): INC0303260 (Session 48 Amazon I2PX — no
direct exchange-plane baseline in the rrc11 00:00 bview; unchanged
premise; INC0303502 supersedes the relationship question with a new
baseline and the Equinix premise). INC0303303/INC0299200 not bounded
(no end); INC0301481 excessive window; INC0303298 excluded by project
scope.

## Selection result (2026-08-03)

**No candidate reached Ready; no event was executed.**

- INC0303502 (Amazon I2PX via Equinix): blocked — no direct
  exchange-plane observer session with target-origin baseline at either
  checked observer (the 419 route-views2 matches are AS11164-in-path
  via peer AS2152, which is not a direct I2PX relationship).
- INC0303305 (Front Range Gigapop): blocked — origin ASN unresolved
  (AS3856 is Packet Clearing House, not FRGP; no reviewed mapping).
- INC0303264 (Cloudflare): blocked — no qualifying baseline under the
  reviewed scopes (unchanged premise; not re-preflighted).
- INC0303260 (Session 48 Amazon event): blocked — unchanged premise.

No identity, attachment state, route-selection mode, or predicate was
guessed. No excluded event was used. No full UPDATE acquisition occurred
(preflight-only; the pipeline's own preflight-mode stop is by design).

## Selection rule

Execute at most one event, only when: project scope Included; immutable
snapshot exists; identity historically reviewed; route selection
reviewed; at least one qualifying event-date observer; plan Ready. If
no candidate reaches Ready, execute none and retain exact blockers.
