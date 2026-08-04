# Clean-clone documentation verification — 2026-08

Dated audit (2026-08-04, documentation conformance session). Verifies
that the public documented paths work from a clean clone of the
published branch. This is an execution record, not normative
installation documentation; the README remains the quick-start
authority.

## Environment

- Clone: `https://github.com/downwithbgp/inim.git`, branch
  `session-52-documentation-spec-conformance`, fresh empty directory
- Rust: `rustc 1.97.1 (8bab26f4f 2026-07-14)`; Cargo: `cargo 1.97.1`
- No shared `target/`, no local databases, no prior Cargo cache reuse
  (cold build; a second timing run may reuse the cache)
- No Git metadata required at runtime; all commands below are
  repository-relative

## Commands (all from the README / docs map / reference docs)

| Step | Command | Result |
|---|---|---|
| Build (release) | `cargo build --release --locked` | ok (3m41s cold, fresh registry) |
| Doc tests | `cargo test --doc --locked` | ok |
| Doc build (warnings denied) | `RUSTDOCFLAGS="-D warnings" cargo doc --locked --no-deps --document-private-items` | ok |
| Documentation drift audit | `scripts/audit-docs.sh` | ok |
| Demo initialization | `./target/release/inim demo init --db ... --root . --force` | ok (no source access) |
| Demo verification | `./target/release/inim demo verify --db ... --root .` | ok |
| Answer-key generation | `python3 scripts/build-evaluation-answer-key.py --db ... --root . --out ...` | ok; byte-identical to the tracked answer key |
| Evaluation pack | `sh scripts/build-evaluation-pack.sh --output ... --db ... --root .` | ok (19 files, SHA256SUMS) |
| Project-scope audit | `./target/release/inim project-scope audit --db ... --root .` | ok (0 excluded) |

Results are filled from the recorded run. This is not an external
evaluation; it is maintainer verification.

## Result

All documented public paths work from the clean clone at branch commit
`e2f38ae`. The answer key regenerates byte-identically from the
clean-clone demo, confirming deterministic generation. No ambiguity
required correction. This is not an external evaluation; the pilot
registry remains at zero external sessions.
