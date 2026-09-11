# Repository audit — CI gates and inventory claims (2026-09)

Audit date: 2026-09-11 · audit start HEAD: `198cbb7`. Historical
record, not normative. This audit reproduced the repository's offline
CI gates locally, re-verified the inventories and declared counts, and
scanned the tracked tree for stale markers. No source was contacted;
no canonical analysis was rerun; no external evaluation session
occurred (the pilot registry remains truthfully at zero sessions).

## Scope

- **Gate reproduction:** `cargo fmt --check`; `cargo clippy
  --all-targets --all-features -- -D warnings`; `cargo test`; `cargo
  test --doc`; `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps
  --document-private-items`; `scripts/audit-docs.sh`; `cargo deny
  check licenses` and `check bans`; `cargo package` with the
  packaged-content scan; the offline queued-analysis smoke and the
  evaluation-kit smoke (demo init/verify, project-scope audit,
  answer-key regeneration drift, pack build).
- **Inventory verification:** every tracked file classified in
  `repository-inventory.json`; the documentation-inventory checked
  lists match `git ls-files`; the repository-truth render re-renders
  byte-identically.
- **Claim verification:** invariant-register arithmetic (enforced by
  `tests/invariant_register_test.rs`), pilot-registry counts,
  schema-version matrix, route references, action pins, case-study
  indexes.
- **Fresh-eyes scan:** TODO/FIXME markers, secrets, `unsafe`,
  suppression attributes, untracked/ignored worktree state.

## Results

| Gate | Result |
|---|---|
| `cargo test` | 1517 passed, 0 failed, 1 ignored (network-required live research probe) |
| `cargo fmt --check` | clean |
| `cargo clippy --all-targets --all-features -- -D warnings` | clean |
| `cargo doc` (warnings denied) | clean |
| `scripts/audit-docs.sh` | ok |
| `cargo deny check licenses` / `check bans` | ok |
| `cargo package` + packaged-content scan | ok; no runtime material packaged |
| Queued-analysis smoke (5 checks) | ok |
| Evaluation smoke: demo init/verify, scope audit | ok (excluded events in catalog: 0) |
| Answer-key regeneration drift | no diff |
| Evaluation pack build | ok (SHA256SUMS present; no database packaged) |

Claims verified: the invariant register's declared arithmetic matches
its rows (56 rows = 53 enforced + 3 partially enforced); the pilot
registry states zero external sessions; the tracked-file inventories
equal the tracked set.

## Findings

### F-1 (fixed): the repository-truth render embedded the wall clock

`scripts/build-repo-audit.py` rendered `audit date: {date.today()}`
into `docs/audits/2026-08-repository-truth-audit.md`, while
`scripts/audit_docs.py` verifies that document by re-running the
render and requiring a byte-identical result. Consequently, every run
on a day other than the last regeneration reported nonexistent drift,
and the check could not distinguish real drift from its own timestamp.

Confirmed in hosted CI: the two 2026-09-01 dependency-update
pull-request runs (`33460775306`, `33460742365`) passed every job
except `docs`, which failed at the documentation drift audit step; the
last green run on `main` is the 2026-08-05 merge (`30970095524`), and
no later run had reached a green `docs` job.

Fix: the render date is pinned beside the audit start HEAD
(`AUDIT_DATE` in `scripts/build-repo-audit.py`), so regeneration is
byte-stable on any day and the committed render is unchanged.

### F-2 (recorded, not fixed): frozen derived-cache streams store a placeholder peer ASN

`src/orchestrate.rs` writes `peer_asn: 0` on each `CachedTargetStream`
(the frozen-stream cache write for the derived RIB preflight cache),
leaving a `TODO: capture from observations` marker. No current code
path consumes the stored value (it is serialized with the cache entry
but never read for behavior), so no behavior depends on it; under the
active alpha freeze (changes limited to demonstrated defects) it is
recorded rather than changed.
If the field gains a consumer, populate it from the baseline
observations first.

### Housekeeping

- `.reasonix/` (local agent/session scratch) is now git-ignored and
  never tracked.
- Untracked local run outputs remain under
  `case-studies/manlan-2019/pilot/out/` in the local worktree. They
  are local scratch, not repository content, and are not included by
  `cargo package` listings; they can be cleaned from the worktree
  when convenient.

## Verification inventory state

- Tracked files: 472 → 473 (this record); inventory entries equal the
  tracked set; the repository-truth render is regenerated with this
  registration.

## Boundaries

- No canonical analysis rerun; no source contacted; no archive
  acquired.
- All measurements above are local offline runs at the audit HEAD;
  hosted-CI facts are cited where they check a repository claim.
