# GRNOC bulk-access request — DRAFT (not sent)

**Status:** DRAFT — created 2026-08-01, refined 2026-08-01 (Session 34).
**Do not send without user approval.** Silence is never treated as
permission for aggressive crawling.

This document contains two versions of the request:

- **Version A — concise email** (what you would actually send).
- **Version B — detailed technical appendix** (attached to the email or
  offered on request; contains every question in machine-readable form).

## Recipient

GlobalNOC (Indiana University) — operator of the GlobalNOC Ticket
Viewer (`https://ticket-viewer.grnoc.iu.edu/`).

## Version A — concise email

> Subject: Request for guidance on public task-viewer data access for
> reproducible research
>
> Hello GlobalNOC,
>
> I am writing on behalf of **inim** (Internetwork Impact Monitor), an
> open-source, MIT-licensed research and analysis tool that correlates
> operator-declared network events with externally observable BGP route
> behavior from public collectors such as RouteViews and RIPE RIS.
>
> inim would like to preserve **public ticket metadata** from the GlobalNOC
> public task viewer (ticket numbers, titles, public descriptions,
> published windows, states) so that historical operational events can be
> reproducibly correlated with public BGP data. We would acquire this
> data politely: low rate, low concurrency, exact ticket-number lookups
> and scoped public searches only, honoring rate limits and stopping if
> the service appears stressed.
>
> We do **not** need private notes, contact data, or any authenticated
> content. We would retain public snapshots locally for reproducibility
> and would not redistribute raw payloads without separate permission.
>
> We would prefer an official mechanism if one exists. Could you please
> confirm or advise on:
>
> 1. Whether a public bulk endpoint or data export exists for public
>    task records (or whether the current viewer JSON endpoints are the
>    intended public interface).
> 2. An acceptable sustained request rate and maximum concurrency for
>    automated retrieval of historical records.
> 3. Your preferred User-Agent / contact format for automated clients.
> 4. Historical-retention expectations for locally stored public
>    snapshots.
> 5. Any attribution requirements.
> 6. Whether redistribution of **normalized metadata** (ticket number,
>    title, type, state, published windows — no raw payloads, no private
>    fields) would be permitted, and under what conditions.
>
> Thank you for your consideration.
>
> — {YOUR NAME}, {AFFILIATION — see "Awaiting user input" below}

## Version B — detailed technical appendix

> ### Appendix: technical details of the requested access
>
> **Who we are**
> - Project: inim (Internetwork Impact Monitor) — open-source,
>   MIT-licensed (repository: {REPOSITORY_URL}).
> - Contact: {CONTACT_EMAIL}.
> - Affiliation: {AFFILIATION_WORDING}.
>
> **What we retrieve**
> - Public task records served by the GlobalNOC Ticket Viewer:
>   `INC` (incidents) and `CHG` (change requests) records, with the
>   fields the viewer itself publishes: number, short description,
>   description, notification text, state, category, priority, opened,
>   work/planned windows, maintenance type. We do **not** retrieve or
>   store private notes, contact data, or authenticated content.
>
> **How we retrieve it today (current conservative behavior)**
> - One request at a time (no concurrent requests).
> - One request every four seconds (0.25 req/s) — well below any
>   documented or observed rate limit.
> - Conditional requests where the service supports them; no polling
>   loops that re-fetch unchanged records.
> - A fixed request budget per sync (default 100 requests); the client
>   stops when the budget is exhausted.
> - Exact ticket-number lookups and scoped searches only; **no numeric
>   enumeration** (we never walk sequential ticket IDs).
> - The client backs off and stops on errors, 429s, or signs of service
>   stress.
>
> **Questions for GlobalNOC**
> 1. **Official bulk/export mechanism** — does an official public bulk
>    endpoint or data export exist for public task records? If yes, what
>    is it and what are its access terms? If no, are the viewer JSON
>    endpoints the intended public interface?
> 2. **Acceptable sustained rate** — what sustained request rate is
>    acceptable for automated historical retrieval (requests/second)?
>    Is the current 0.25 req/s policy acceptable, or could it be raised?
> 3. **Maximum concurrency** — how many concurrent requests may an
>    automated client issue?
> 4. **Preferred User-Agent / contact format** — is there a preferred
>    User-Agent string or contact header so automated clients can be
>    identified and reached?
> 5. **Historical-retention expectations** — may public snapshots be
>    retained locally (content-addressed, with fetch provenance) for
>    reproducibility? Are there retention limits?
> 6. **Attribution requirements** — what attribution is required when
>    public ticket metadata is used in publications or tool output?
> 7. **Normalized-metadata redistribution** — would redistribution of
>    **normalized metadata only** (ticket number, title, task type,
>    state, published windows; no raw payloads, no description text, no
>    private fields) be permitted? Under what conditions (e.g. license,
>    attribution, notice)?

## Current conservative behavior (self-imposed, in effect regardless of this request)

- One request at a time.
- One request every four seconds (0.25 req/s).
- Conditional requests where supported.
- Fixed request budget per sync (default 100 requests; `--max-requests`).
- No numeric enumeration — exact identifiers and scoped searches only.
- Stops on errors/429s; never treats silence as permission to go faster.

## Awaiting user input (fill before sending)

These fields must be supplied by the user; the request must **not** be
sent until they are filled in:

- **Contact email:** (user to supply)
- **Repository URL:** (user to supply)
- **Affiliation wording:** e.g. "independent researcher", "researcher at
  {institution}", "maintainer of the inim open-source project" — (user
  to supply)
- **Sender name** for the email signature: (user to supply)

## Usage notes

- The project is not distributing any corpus in this session; the
  request is for guidance before any larger acquisition or publication
  is considered.
- Current acquisition remains bounded by the self-imposed conservative
  policy (1 concurrent request, 0.25 req/s, budget 100 per sync).
- See `GRNOC_PUBLIC_TASK_VIEWER.md` for the protocol audit that
  documents the current viewer behavior.
