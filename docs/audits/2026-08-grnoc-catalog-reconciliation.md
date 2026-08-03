# GRNOC catalog reconciliation — 2026-08

Audit date: 2026-08-02. This audit explains what Session 46's
completion report meant by "no GRNOC corpus events exist in the catalog
yet", and where the reviewed MAN LAN GRNOC material actually lives.

## Question

Session 46 reported zero GRNOC corpus events in the catalog, despite
Sessions 33–34 having acquired and cataloged ten MAN LAN INC/CHG
tickets. Which catalog was meant, and where did the ten tickets go?

## Answer

Session 46's statement was **accurate for the main catalog**
(`data/inim.sqlite`) and every demo catalog. The ten MAN LAN tickets
were acquired into a **separate runtime corpus database**
(`data/corpus.sqlite`, catalog schema v7, created in Session 33) that
was never merged into the main catalog and never imported by the demo
bootstrap. The GRNOC events were therefore present, but only in an
untracked SQLite database; they were never "lost" and never tracked as
immutable snapshots.

## Catalog inventory (relative paths, no absolute local paths)

| Catalog | Category | Schema | Events | GRNOC events | Snapshots | Manifests | Plans | Runs | Jobs | Relationships |
|---|---|---|---|---|---|---|---|---|---|---|
| `data/inim.sqlite` | runtime catalog | v10 | 17 | 0 | 17 | 17 | 17 | 10 | 0 | 0 |
| `data/corpus.sqlite` | runtime corpus (Session 33–34) | v7 | 27 | **10** | 27 | 17 | 17 | 10 | n/a (v7) | **36** |
| offline demo (generated) | generated demo | v10 | 3 | 0 | 3 | 3 | 3 | 2 | 0 | 0 |
| packaged demo (generated) | packaged demo | v10 | 3 | 0 | 3 | 3 | 3 | 2 | 0 | 0 |

## Where the ten tickets live

| Location | Contains | Tracked? |
|---|---|---|
| `data/corpus.sqlite` (runtime) | 10 GRNOC catalog events + immutable snapshots (raw public JSON, ~6 KB total) + 36 relationships + case-study links | No (runtime) |
| `case-studies/manlan-2019/pilot/ticket-reviews.json` | Reviewed interpretation: roles, entity labels, applicability, per-field provenance | Yes |
| `case-studies/manlan-2019/pilot/corpus-acquisition.json` | Acquisition policy + exact ticket IDs + source timestamps | Yes |
| `case-studies/manlan-2019/pilot/corpus-validation.md` | Session 34 corpus validation narrative | Yes |
| `tests/fixtures/grnoc/viewer/*.json` | Four parser fixtures (CHG0099999, INC0227937, INC0301970, malformed) — NOT the MAN LAN tickets | Yes |
| `case-studies/manlan-2019/case-study.json` + `case_study_event_links` | 12 case-study references: 10 resolve to corpus events, 2 (TASK0038206, TASK0038211) are unresolved references with `catalog_event_id = NULL` | Yes |

## Classification

- **Tracked source material:** ticket-reviews, acquisition record,
  validation narrative, parser fixtures, case-study references. The ten
  immutable ticket snapshots themselves are **not tracked**.
- **Imported runtime catalog state:** `data/inim.sqlite` (17 events,
  no GRNOC) and `data/corpus.sqlite` (27 events incl. 10 GRNOC).
- **Deterministic demo state:** 3 events, 0 GRNOC — by design; the demo
  imports only `manifests/` + `case-studies/` (per the demo contract).
- **Historical session report claims:** "no GRNOC corpus events exist
  in the catalog yet" referred to the main catalog + demo. The corpus
  DB was not the catalog.

## Consequences for the demo

The ten snapshots are public records (redistribution documented in
`docs/sources/GRNOC_PUBLIC_TASK_VIEWER.md` and `tests/fixtures/
README.md`). Session 47 tracks them as immutable case-study source
material under `case-studies/manlan-2019/corpus/` and imports the
bounded reviewed corpus into the demo deterministically (see the
session's demo-corpus work). No Ready plan and no job is created
automatically; the tickets enter the catalog as discovered events with
snapshots, reviewed roles, and reviewed relationships only.

## Required audit checks

- `demo_grnoc_event_count_is_explicit` — the demo reports its GRNOC
  event count explicitly instead of implying a global corpus.
- `case_study_ticket_reference_is_not_counted_as_catalog_event` — a
  case-study link without a catalog event (TASK0038206/TASK0038211) is
  a reference, never an event.
- `catalog_event_requires_source_snapshot` — no event row without an
  immutable snapshot may be imported.
- `migration_preserves_existing_catalog_events` — schema v9→v10
  migration retains every existing event/snapshot/relationship/plan/run
  and initializes the job tables empty.
