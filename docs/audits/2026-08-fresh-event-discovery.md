# Fresh-event discovery audit — 2026-08-03

Dated execution audit for the Session 48 bounded GRNOC discovery and the
corrected fresh-event candidate search (no session narrative in normative
docs).

## Search mechanism

The GRNOC Public Task Viewer search mechanism documented in
`docs/sources/GRNOC_PUBLIC_TASK_VIEWER.md` was used through the new bounded
`--search` capability of `inim catalog sync grnoc`:

- `POST /api/get_domains` — returned 19 domains; the **Internet2** domain
  (`sys_id 11b87c3ddb65e200d1b4fa5aaf96194a`) is the reviewed scope for
  I2 participant / I2PX peer discovery.
- `POST /api/get_incidents` with `{"query", "domain_id", "active": true,
  "limit": 20, "offset": 0}` — incident search scoped to the Internet2
  domain (unscoped incident search returns 403 per the source doc).

## Queries and requests

| # | Query | Endpoint | Status | Records |
|---|---|---|---|---|
| 1 | (domains) | get_domains | 200 | 19 domains |
| 2 | I2 IP Participant | get_incidents | 200 | 32 |
| 3 | I2PX Peer | get_incidents | 200 | 32 |
| 4 | I2 PX Peer | get_incidents | 200 | 32 |
| 5 | IP Participant | get_incidents | 200 | 32 |
| 6 | Participant unavailable | get_incidents | 200 | 32 |
| 7 | Outage | get_incidents | 200 | 32 |
| 8 | (recheck) I2 IP Participant | get_incidents | 200 | 32 |

Total requests: **8** (well under the 250 budget; the polite client's
`--max-requests` was set to 40 and enforced after the budget-wiring fix).
Rate control: no 429 responses, no Retry-After observed. Three additional
python-urllib probes were attempted; the server rejected them with 404
(server-side client discrimination), so all further discovery used the
polite adapter only.

**Behavior note:** for the Internet2 domain, every incident query returned
the same 32-record active set — the server did not visibly filter by
query text for this domain (a no-match query returned 404, so the query
is applied, but the domain's public incident surface appears to be a
fixed active set). `active: false` returned 404. Discovery is therefore
bounded to the 32 exposed records; no enumeration was performed.

## Events discovered

32 INC records (2026-era; the Internet2 domain's public surface). The
records expose `number`, `short_description`, `state`, `category`,
`work_start`, `work_end`; most descriptions are empty or Sub5
boilerplate. Full records were fetched through the exact-lookup frontier
(1 request each) and stored in the runtime discovery catalog
(`data/s48-discovery.sqlite`, untracked).

## Shortlist (≤5, ranked by analytical suitability)

| Ticket | Title | Relationship type | Window (UTC) | Origin ASN (ARIN RDAP) | Notes |
|---|---|---|---|---|---|
| INC0303264 | Availability - I2 Participant CLOUDFLARE | IP participant (no site qualifier → relationship-unavailable convention) | 2026-08-03 09:49:30–10:06:08 | AS13335 (CLOUDFLARENET, reg. 2010-07-14) | Priority 1 shape |
| INC0303260 | Brief Outage - I2 PX Peer Amazon (SEAT) | I2PX peer (direct AS11164 relationship) | 2026-08-03 09:08:05–09:11:46 | AS16509 (AMAZON-02, reg. 2000-05-04) | Priority 2 shape |
| INC0303298 | Brief Outage - I2 Participant NOAA (KANS-WASH) | IP participant (parenthesized site → redundant-attachment convention) | 2026-08-03 12:16:06–13:07:17 | AS270 (NASA-Z, reg. 1989-02-24) | Priority 1 alternative |

Excluded: optical/alarm/telemetry records (INC0303197, INC0302864,
INC0303274, INC0303022, INC0303174, INC0290567, INC0294264, INC0295650,
INC0297615, INC0301714, INC0216124, INC0303030), inquiries and decoms
(INC0252776, INC0284133, INC0286963, INC0295876, INC0302597, INC0302792,
INC0294713, INC0300261), non-bounded records (INC0303303, INC0299200),
and INC0301481 (MANLAN Participant SINET; real description but a 9-day
instability window — archive estimate excessive for this session).

## Next steps

Preflight (Stage A, RIB-only) for the shortlist; select at most one Ready
event; reviewed manifest revision; durable queue + worker execution.
