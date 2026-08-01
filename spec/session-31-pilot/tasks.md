# Session 31 — tasks

## T1. Spec + review — gate: /review pass.

## T2. Attach + verify the AAR PDF (Part 1)
- pdfinfo fallback for page count; import /tmp/MANLAN-20190821-Postmortem.pdf;
  verify sha/media/pages/path/serving.
- Gate: T, F, C. Commit.

## T3. Archive-plan audit + fix (Part 2)
- baseline RIB + validation RIB + update sequence semantics; per-stamp
  year/month; URL dedup; planning-table fields; 8 required tests; re-plan.
- Gate: T, F, C. Commit.

## T4. run_transitions storage audit (Part 3)
- import bound + docs; 5 required tests.
- Gate: T, F, C. Commit.

## T5. Target research (Parts 4-5)
- research subagents; target-research.json (reviewed record); apply-research
  CLI + migration V3 (research_updated_utc column) +
  AmbiguousServiceIdentity constant; no guessed mappings.
- Gate: T, F, C. Commit.

## T6. Pilot selection record (Part 6) — data file + rationale.

## T7. Staged execution (Parts 7, 9)
- `--preflight-only` (JSON stdout contract); run-linking CLI
  (`catalog case-study link-run`); Stage A run; pilot event+manifest;
  Stage B run; Stage C decision; pilot comparison data.
- Gate: verify artifacts; commit.

## T8. Phase-summary continuity tests (Part 8) — 4 tests.

## T9. Web updates (Part 10) — target research, plan audit, pilot block.

## T10. Docs (Part 12) + gates (Part 13) + completion report.
