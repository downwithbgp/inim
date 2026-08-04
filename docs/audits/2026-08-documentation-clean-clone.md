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
| Build (release) | `cargo build --release --locked` | pending |
| Doc tests | `cargo test --doc --locked` | pending |
| Doc build (warnings denied) | `RUSTDOCFLAGS="-D warnings" cargo doc --locked --no-deps --document-private-items` | pending |
| Documentation drift audit | `scripts/audit-docs.sh` | pending |
| Demo initialization | `./target/release/inim demo init --db ... --root . --force` | pending |
| Demo verification | `./target/release/inim demo verify --db ... --root .` | pending |
| Answer-key generation | `python3 scripts/build-evaluation-answer-key.py --db ... --root . --out ...` | pending |
| Evaluation pack | `sh scripts/build-evaluation-pack.sh --output ... --db ... --root .` | pending |
| Project-scope audit | `./target/release/inim project-scope audit --db ... --root .` | pending |

Results are filled from the recorded run. This is not an external
evaluation; it is maintainer verification.

## Ambiguities found and corrected

None identified at the time of writing; any ambiguity discovered during
the run is recorded in the final conformance audit
(`docs/audits/2026-08-documentation-spec-conformance.md`).
