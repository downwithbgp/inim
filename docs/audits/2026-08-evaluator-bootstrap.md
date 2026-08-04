# Clean evaluator bootstrap audit — 2026-08-04

Dated execution audit. Verified from a FRESH clone (no session state):

1. `cargo build --release --locked` — succeeds (release, 3m48s cold).
2. `inim demo init --db <new> --root . --force` — 4 events, 3 runs,
   no excluded record.
3. `inim demo verify` — ok (no source access, no absolute paths).
4. `inim serve` (read-only, loopback) — `/`, `/events`,
   `/events/INC0302574/workbench`, `/case-studies/manlan-2019/workbench`,
   `/analysis-queue`, `/case-studies` all HTTP 200.
5. `inim project-scope show` — policy schema v1 loads; excluded
   entities reported.

The evaluator needs no runtime catalog, no raw MRT, no web writes, no
worker, and no network access. The second-network case is NOT in the
demo (the Smithville relationship is not assessable through the
selected public collectors; no tracked case exists), so second-network
evaluation tasks remain deferred in
`docs/evaluation/SECOND-NETWORK-ALPHA-HANDOFF.md`.
