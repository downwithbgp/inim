# GRNOC Public Task Viewer — protocol audit

**Observation date:** 2026-08-01 (UTC; server `Date` header
`Sat, 01 Aug 2026 13:27:28 GMT`).

**Methods used:** direct HTTP probes with a descriptive User-Agent
(`inim/0.1.0 (research tool; polite single-request probe)`), one request
at a time with spacing between probes; static inspection of the
application JavaScript bundle; JSON API probes of the endpoints the
bundle calls. No browser automation was needed and no private or
authenticated endpoint was used.

## Identity and access

- **Viewer:** `https://ticket-viewer.grnoc.iu.edu/` — the "GlobalNOC
  Ticket Viewer", a single-page application (React, served by nginx).
- The viewer is shared across GlobalNOC-supported networks; the network
  list comes from the `get_domains` endpoint (19 domains observed:
  AMPATH, ARE-ON, Big Ten Academic Alliance, CAAREN, CEN, Indiana
  GigaPoP, I-Light, Internet2, Idaho Regional Optical Network, and
  others).
- A second public entry point exists: `https://sn-tools.grnoc.iu.edu/
  public-task-viewer/?network_name=...` (not audited further; the
  ticket-viewer host is the primary one referenced by public ticket
  URLs).
- **Authentication:** the API is anonymous. `GET /api/get_auth_status`
  returns `{"signedIn":false}` without credentials. The SPA references a
  Shibboleth SSO login for restricted content; public record lookups
  work without it. An unscoped incident search (see below) returned
  `403 {"detail":"User does not have access to the specified network"}`
  — network scoping is enforced server-side for some queries.

## Page rendering

- The viewer page is **not server-rendered**: every route under
  `/tickets/...` returns the identical 438-byte HTML shell
  (`index-623f9256.js` + `index-115bf753.css`), and the browser fetches
  ticket data via JSON endpoints.
- We therefore use the **stable public JSON request** (below), which is
  clearly the source of the same information the rendered DOM displays.

## Public request that retrieves ticket details

Undocumented JSON endpoints consumed by the SPA (API base `/api`):

| Endpoint | Method | Purpose |
|---|---|---|
| `/api/get_incidents` | POST | Incident records (`INC...` numbers) |
| `/api/get_change_requests` | POST | Change-request records (`CHG...` numbers) |
| `/api/get_domains` | POST | Network/domain list (name, `sys_id`, criteria) |
| `/api/get_auth_status` | GET | Anonymous/signed-in status |

Request bodies observed for record lookup:

```json
{"number": "INC0227937"}
```

The SPA also passes `active` (boolean) and, for scoped searches,
`domain_id` (from `get_domains`) plus `criteria` (domain-specific).
`{"number": "...", "active": true}` returned zero results for an old
closed ticket while the number-only lookup returned it, indicating
`active:true` restricts to currently active records.

**Search/list:** an official public search exists through the same
endpoints — the SPA passes `{"query": "...", "domain_id": "...",
"active": true, "limit": n, "offset": n}`. A probe of
`POST /api/get_change_requests` with `{"query":"MAN LAN","active":true,
"limit":5}` returned `200` with `{"total":341,...}`; the same query
against `get_incidents` without a domain returned 403. **The adapter
must never issue empty or unscoped broad queries** (enumeration risk);
search is only used with an explicit reviewed domain and query string.

**2026-08-03 discovery observations (bounded search, 8 requests):**
incident search scoped to the Internet2 domain (`sys_id
11b87c3ddb65e200d1b4fa5aaf96194a`) returns the domain's public active
set (32 records) and did not visibly filter by query text for that
domain; a no-match query returned 404; `active: false` returned 404;
the python-urllib client was rejected with 404 while the polite
reqwest adapter with the reviewed User-Agent succeeded. Records
returned by search carry titles/windows but empty descriptions for
recent entries. See `docs/audits/2026-08-fresh-event-discovery.md`.

**Pagination:** `limit`/`offset` fields in the request body (not query
parameters).

## Response schema

`Content-Type: application/json` (Python `uvicorn` server). Envelope:

```json
{"total": 1, "result": [ { ...record... } ]}
```

Record fields (raw source values; timestamps are **unix-epoch seconds as
strings**, `""` when unset; state/priority are **code strings**):

| Field | Meaning |
|---|---|
| `number` | Task number, e.g. `INC0227937`, `CHG0038258` |
| `short_description` | Title |
| `description` | Public description (may be empty) |
| `u_outgoing_notification_text` | Published notification text |
| `state` | State code (see maps below) |
| `category` | e.g. `Circuit`, `Undetermined`; may be absent |
| `work_start`, `work_end` | Actual start/end (unix string; `work_end` `""` = open) |
| `opened_at` | Opened timestamp (unix string) |
| `priority` | Priority code (see map below) |
| `start_date`, `end_date` | CHG-only planned window (unix strings) |
| `u_maintenance_type` | CHG-only, e.g. `Hardware`, `Power` |

Unknown fields are ignored by the parser (serde defaults); a future
schema addition does not corrupt normalization.

**State code maps** (labels exactly as the viewer renders them; these
are lossless label translations, never computed severity scores):

- Incidents: `1` New, `2` In Progress, `3` On Hold, `-1` Review Needed,
  `-170` Custodian Review, `6` Resolved, `7` Closed, `8` Canceled.
- Change requests: `0` Review, `3` Closed, `4` Canceled, `7` Impact
  Assessment, `-1` Implement, `-2` Scheduled, `-3` Authorized, `-4`
  Assess, `-5` New, `-7` Impact Assessment.

**Priority map:** `1` Critical, `2` High, `3` Moderate, `4` Low.

**Task types:** the viewer labels records by number prefix —
`INC` → Incident, `CHG` → Change Request. `TASK`-prefixed records are
**not served** by either endpoint: `{"number":"TASK0038206"}` returns
`{"total":0,"result":[]}`. Task references must remain unresolved in the
catalog (no fabricated records).

## HTTP behavior

- **Cache headers:** none on API responses (uvicorn; no `ETag`,
  `Last-Modified`, or `Cache-Control` observed). Static assets carry
  `ETag` + `Last-Modified` (nginx). **Consequence:** conditional
  requests cannot produce 304s against the live API today; the sync
  client still sends `If-None-Match`/`If-Modified-Since` when validators
  are known, and otherwise degrades to full fetch + content-SHA-256
  deduplication (which yields the same no-new-snapshot outcome).
- **ETag/Last-Modified support:** server emits them for static assets
  only; client support is generic and harmless against the API.
- **Retry-After:** not observed on any probe. The client honors
  `Retry-After` when present and otherwise uses its configured bounded
  backoff.
- **Cookies:** nginx sets a session cookie (`_f32bd=...`) on all
  responses; it is required only for normal browsing, not for the API.
  The sync client does not store or replay cookies and never stores
  session secrets.
- **robots.txt:** **none exists** — `GET /robots.txt` returns the SPA
  shell (200, `text/html`), so there are no published robots
  exclusions. There is also no documented acceptable-use statement
  linked from the viewer or the SPA bundle.
- **Terms:** the Internet2 site links a general
  [Terms of Use](https://internet2.edu/community/about-us/policies/terms-of-use/)
  and
  [Privacy Statement](https://internet2.edu/community/about-us/policies/privacy/)
  containing no crawling or automation prohibition (standard export
  control and copyright language only). The GlobalNOC homepage
  (`globalnoc.iu.edu`) contains no task-viewer terms.

## Official API status

**No official API or data-export mechanism is documented.** The JSON
endpoints above are the SPA's internal interface, undocumented and
without a compatibility guarantee. They are used here as the stable
public structured response that backs the rendered DOM — recorded as
undocumented public requests, not an official API. A bulk-access
request draft is maintained at `docs/sources/GRNOC_BULK_ACCESS_REQUEST.md`
(created, not sent).

## Client policy consequences

The sync client implemented for this protocol (see `src/catalog/access.rs`
and `src/catalog/grnoc_viewer.rs`) applies:

- Reviewed local operational guidance: the
  unauthenticated Public Task Viewer endpoints can be accessed at up to
  **5 requests/second** without operational concern. This is reviewed
  local guidance, NOT a publicly documented API service-level guarantee.
  Defaults: 5 requests/second sustained (smooth token bucket; at most 2
  immediate requests, then paced); maximum 5 in-flight; default budget
  100 requests per sync. Values above 5 req/s require an explicit
  `--allow-higher-rate` flag; a lower value may be selected freely.
- Fully responsive to source feedback: the first 429 or explicit
  throttle halves the effective rate immediately while honoring
  `Retry-After`; repeated throttling stops the sync cleanly; sustained
  success recovers in bounded steps up to the configured ceiling, which
  is never exceeded.
- A descriptive User-Agent naming `inim`, its version, and its research
  purpose (no invented contact address; a project URL is used only once
  one exists).
- Honor `Retry-After`, exponential backoff with bounded jitter, stop
  conditions on repeated 429/403, and no retry of permanent 404s.
- Exact ticket-number lookups and scoped public search only — no
  numeric-ID enumeration.

## Fixtures

The committed fixtures under `tests/fixtures/grnoc/viewer/` are exact
public responses captured during this audit (see
`tests/fixtures/README.md` for provenance and redistribution notes).
All tests use fixtures; the live service is never called from the test
suite.

## Reviewed corpus state

- The ten acquired MAN LAN tickets are tracked as immutable snapshots
  under `case-studies/manlan-2019/corpus/snapshots/` (with a manifest,
  the reviewed relationship graph, and per-ticket reviews) and are
  imported deterministically into the offline demo. An earlier
  report's "no GRNOC corpus events in the catalog" referred to the main catalog
  and demo, which never imported the corpus; see
  `docs/audits/2026-08-grnoc-catalog-reconciliation.md`.
- The ten acquired MAN LAN tickets are now **reviewed operational data**
  (reviewed case-study roles, entity labels, linked maintenance
  identifiers, analysis applicability, per-field provenance citing
  snapshot fields or the AAR) — stored separately from the immutable
  snapshots; see `case-studies/manlan-2019/pilot/ticket-reviews.json`
  and `inim catalog corpus-review`.
- The reviewed relationship graph (explicit, document-cited, analyst-
  reviewed, and derived edges) is auditable via
  `inim catalog relationships audit` and `/corpus/relationships`.
- The two TASK identifiers remain **unresolved document references**
  (the viewer does not serve TASK records; no snapshot is
  manufactured).
- The bulk-access request draft
  (`docs/sources/GRNOC_BULK_ACCESS_REQUEST.md`) now has a concise email
  version, a technical appendix, and a user-fill section (contact email,
  repository URL, affiliation). It has **not** been sent.
