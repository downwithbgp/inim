# GRNOC bulk-access request — DRAFT (not sent)

**Status:** DRAFT — created 2026-08-01. **Do not send without user
approval.** Silence is never treated as permission for aggressive
crawling.

## Recipient

GlobalNOC (Indiana University) — operator of the GlobalNOC Ticket
Viewer (`https://ticket-viewer.grnoc.iu.edu/`).

## Draft text

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
> We would prefer an official mechanism if one exists. Specifically, we
> would like to know:
>
> 1. Whether a public bulk endpoint or data export exists for public
>    task records (or whether the current viewer JSON endpoints are the
>    intended public interface).
> 2. Whether historical access is acceptable.
> 3. Your recommended request rate and concurrency for automated
>    retrieval.
> 4. Any attribution requirements.
> 5. Any retention requirements.
> 6. Whether redistribution of **normalized metadata** (ticket number,
>    title, type, state, published windows — no raw payloads, no private
>    fields) would be permitted, and under what conditions.
>
> Thank you for your consideration.

## Usage notes

- The project is not distributing any corpus in this session; the
  request is for guidance before any larger acquisition or publication
  is considered.
- Current acquisition remains bounded by the self-imposed conservative
  policy (1 concurrent request, 0.25 req/s, budget 100 per sync).
- See `GRNOC_PUBLIC_TASK_VIEWER.md` for the protocol audit that
  documents the current viewer behavior.
