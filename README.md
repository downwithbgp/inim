# inim

**inim is a local, event-conditioned BGP analysis system and NOC incident
workbench.** It relates operator-declared network events to externally
observed route changes at selected RouteViews and RIPE RIS observer
sessions.

> **Status: early public alpha.** Analysis semantics and the interface
> are still under active development. Outputs are observer-scoped
> observations of BGP control-plane state — not traffic-impact or
> root-cause conclusions.

## What it does

An operator declares an event (a ticket with a reviewed time window) and
reviews an analysis plan: target origin ASN(s), a transit predicate
(named service plane), and selected observer sessions. inim acquires
public BGP archives for the window, reconstructs the route-state history
of each observer-prefix stream, and derives **routing findings**:

- who observed which prefixes doing what
- when
- over which peer relationship
- with which before/after route state
- and how the route ended

Every finding is scoped to the observer sessions that actually saw the
routes. Outputs distinguish observed signatures (withdrawn, temporarily
absent, path replaced, prepending changed, left the reviewed path) from
restoration classes (visibility, reviewed plane, equivalent route, exact
event baseline) and from coverage limits (no qualifying baseline,
required session absent, incomplete archive).

## What it cannot conclude

inim observes BGP control-plane state at public collectors. It does not
measure traffic, and it cannot prove physical failure, root cause,
global reachability, or causation. Temporal association with an
operator-declared event is association, not causation. A finding at one
observer session says nothing about other sessions or about the Internet
at large. "No route-state change" at the selected observers is
supporting evidence, never a proof of health. A named relationship with
no qualifying public-collector visibility is reported as unassessable,
not as unaffected.

## What evidence it uses

- **RouteViews and RIPE RIS MRT archives** — RIB baselines and UPDATE
  streams, acquired and cached locally, keyed with ADD-PATH-aware route
  identity. Observer sessions are the unit of observation; the two
  families are independent evidence and may legitimately disagree.
- **GRNOC Public Task Viewer ticket snapshots** — immutable, locally
  cached source declarations for the event catalog.
- **Reviewed metadata** — manifests, network profiles, ASN identities,
  collector locations, and case-study claims. Reviewed data never
  replaces protocol evidence.

Archives are immutable; derived artifacts carry schema versions and
reference their source runs. See `docs/DATA_PROVENANCE.md` and
`docs/sources/` for the source-specific contracts.

## What currently works

- The **plan/analyze pipeline**: manifest review, planning before
  acquisition, archive acquisition with bounded parallelism, RIB
  preflight, route reconstruction, lifecycle classification, findings,
  and reports.
- A **local event catalog** (SQLite + filesystem evidence store) with
  immutable source snapshots and reviewed revisions.
- A **server-rendered web workbench** — the primary human interface. It
  is read-only: HTTP GET requests never perform analysis or acquire
  data.
- A **CLI** for administration, import, planning, analysis, and audit.
- **Four reviewed case studies** under `case-studies/`:
  - `case-studies/manlan-2019` — single-target NORDUnet historical pilot across
    RouteViews and RIPE RIS observers (2019-08-21).
  - `case-studies/inc0299001` — UVA participant-unavailability event with
    partial routing impact.
  - `case-studies/inc0302574` — RIPE-via-NYIIX I2PX visibility audit with an
    unassessable named relationship.
  - `case-studies/manlan-esnet-2019` — narrow-scope ESnet participant event
    (INC0040293) with a stable reviewed-plane result.
- Text and JSON reports, run comparison, and a screenshot harness for
  visual review.

## Quick start

```sh
cargo build --release
```

The built binary is `./target/release/inim` (or install it on your PATH
with `cargo install --path .`).

### Deterministic offline demo (no network)

```sh
inim demo init --db ./inim-demo.sqlite --root .
inim serve --db ./inim-demo.sqlite --root .
# open http://127.0.0.1:8080  (events, workbenches, analysis jobs)
```

`inim demo init` builds a fresh catalog from tracked reviewed material
(no private databases, no downloads, no network); `inim demo verify`
checks events, workbenches, and artifact references.

### Local catalog and web workbench

```sh
inim catalog init --db data/inim.sqlite
inim catalog import --db data/inim.sqlite --root .
inim catalog case-study import --db data/inim.sqlite --path case-studies/manlan-2019
inim serve --db data/inim.sqlite --root .
# open http://127.0.0.1:8080
```

`inim serve` binds loopback only and is unauthenticated; a non-loopback
bind requires `--allow-non-loopback`. The server is read-only by
default; local mutations (queue, cancel, retry, plan edits) require
`--enable-writes` and are intended for trusted local use only — never
expose write mode to untrusted networks. No analysis runs in the web
process; a separate worker executes queued jobs. The workbench for an
event or case study is also available as a text report without the web
server:

```sh
inim catalog workbench --db data/inim.sqlite --subject manlan-2019
```

### Security model

The web server is trusted-local only: no authentication exists, write
mode is disabled by default, the default bind is loopback-only, and a
non-loopback bind with writes requires the explicit
`--allow-unauthenticated-writes` acknowledgement. Every mutation POST
requires a process-lifetime CSRF token (from the OS random source),
bodies are size-bounded (64 KiB), GET never mutates, and mutation
endpoints do not exist when writes are disabled. Do not expose write
mode to untrusted networks; there is no TLS termination and no password
storage by design.

### Queued analysis (reviewed plan → durable job → worker)

```sh
inim analysis-plan show --db data/inim.sqlite --event <event-id>   # read-only plan review
inim analysis-job queue --db data/inim.sqlite --plan <plan-revision-id>   # queue (no execution)
inim worker --db data/inim.sqlite --root .   # separate process; claims and executes jobs
# terminal 1: inim serve --db data/inim.sqlite --root . --enable-writes
# terminal 2: inim worker --db data/inim.sqlite --root .
```

Queueing is idempotent (one active job per exact plan revision and
canonical plan hash), performs no network access, and never executes
analysis. The worker claims jobs transactionally, stages and validates
artifacts, and publishes completed runs atomically. Cancellation is
cooperative; retry creates a new immutable attempt. See
`docs/OPERATIONS.md` and `docs/ADRs/DURABLE-ANALYSIS-JOBS.md`.

### One-off analysis

```sh
inim plan --event <ticket.json> --manifest <manifest.json>
inim analyze --event <ticket.json> --manifest <manifest.json> --cache cache --out out
```

Planning precedes acquisition: a blocked plan (for example a missing
reviewed transit predicate) performs no downloads and no MRT parsing.
See `RELEASING.md` and `CONTRIBUTING.md` for repository policy, and
`docs/README.md` for the full documentation map.

## CLI overview

| Area | Commands |
|---|---|
| Read-only inspection | `inim catalog workbench`, `finding-audit`, `finding-chronology-audit`, `analysis-queue`, `archive-batches`, `relationships audit` |
| Local catalog mutation | `inim catalog init`, `import`, `document import`, `case-study import`, `corpus-review`, `session-metadata-backfill` |
| Public-source synchronization | `inim catalog sync grnoc` (acquires ticket snapshots; never starts analysis) |
| Archive acquisition | `inim analyze` (downloads MRT archives into `--cache`) |
| BGP analysis | `inim plan`, `inim analyze`, `inim compare` |
| Web serving | `inim serve` (read-only by default; `--enable-writes` for local mutations) |
| Queued analysis | `inim analysis-plan show`, `analysis-job queue/list/show/cancel/retry/audit`, `inim worker` |
| Offline demo | `inim demo init`, `inim demo verify` (deterministic, no network) |
| Audit and validation | `inim catalog session-audit`, `relationships`, `corpus-export`, `analysis-job audit` |
| Migration | `inim migrate-manifest` (offline, schema v1 → canonical) |

Commands that acquire data or mutate the catalog are explicit about it;
there is no hidden network access. `inim plan` and `inim analyze` exit
with code 0 for a produced plan (even `Blocked`) or completed analysis,
1 for malformed input, 2 for incomplete analysis, and 3 for a blocked
plan.

## Testing

```sh
cargo test
cargo test --doc
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo deny check licenses && cargo deny check bans
```

## Documentation

- `docs/README.md` — documentation map and authority model
- `docs/GLOSSARY.md` — normative terminology
- `docs/DESIGN.md`, `docs/DOMAIN.md` — architecture and domain model
- `docs/OBSERVABILITY.md` — what the evidence can and cannot show
- `docs/DATA_PROVENANCE.md` — provenance and immutability policy
- `docs/UX.md` — workbench design and operator task model
- `docs/ADRs/` — historical decision records with current status
- `docs/audits/` — repository truth audit (2026-08)

## License

MIT. See `LICENSE`. Third-party material is covered by
`THIRD_PARTY_NOTICES.md`.
