# Clean-clone acceptance baseline — 2026-08

Audit date: 2026-08-02. This report is dated evidence of how the public
repository behaves when cloned into an empty directory and used without
any private runtime state. It is not normative installation
documentation; the README remains the quick-start authority.

## Environment

- Clone URL (HTTPS): `https://github.com/downwithbgp/inim.git`
- Rust: `rustc 1.97.1 (8bab26f4f 2026-07-14)`
- Cargo: `cargo 1.97.1 (c980f4866 2026-06-30)`
- OS: Linux 6.8.0-136-generic x86_64
- Logical CPUs visible to the process: 20
- Fresh cargo registry (cold build), no shared `target/`, no local
  caches, no local databases.

## Quality gates from the clean clone

All gates used `--locked` (the committed `Cargo.lock`).

| Gate | Command | Result | Wall time |
|---|---|---|---|
| Build | `cargo build --locked` | ok | 1m48s |
| Tests | `cargo test --locked` | 1117 passed, 0 failed | 2m55s |
| Doc tests | `cargo test --doc --locked` | ok | 4s |
| Clippy | `cargo clippy --locked --all-targets --all-features -- -D warnings` | clean | 1m29s |
| Doc build | `cargo doc --locked --no-deps --document-private-items` | ok | 25s |
| Package | `cargo package --locked` | ok (verifies tarball) | 1m01s |

Gates ran at HEAD `45bb524` (public main before the dependency-update
merge); the quick-start below ran at HEAD `34d7c9c` (public main after
it). Timings are single-host observations, not performance guarantees.

## README quick-start, exactly as written

Steps executed verbatim from the README at `34d7c9c`:

1. `cargo build --release` — ok (3m30s cold).
2. `inim catalog init --db data/inim.sqlite` — **failed as written**:
   the binary is not on PATH in a clean clone. Used
   `./target/release/inim` instead (documented ambiguity, see below).
   Result: `catalog initialized at data/inim.sqlite (schema v9)`.
3. `inim catalog import --db data/inim.sqlite --root .` — ok:
   `imported 3 events, 3 snapshots, 3 manifests, 3 plans, 2 runs,
   21 artifacts, 67 streams, 2 waves`.
4. `inim catalog case-study import --db data/inim.sqlite --path
   case-studies/manlan-2019` — ok (case study + documents + phases +
   claims + targets + event links).
5. `inim serve --db data/inim.sqlite --root .` — ok; `GET /` → 200,
   `GET /events/INC0302574/workbench` → 200. Server stopped cleanly.

No live network access was required for the demo catalog; all imports
came from tracked manifests and case-study evidence.

## Checklist results

| Question | Result |
|---|---|
| Does the README identify required system packages? | Yes (Rust stable toolchain; `cargo install cargo-deny` documented in `RELEASING.md`) |
| Does bundled SQLite avoid a system SQLite dependency? | Yes (`rusqlite` with `bundled`) |
| Is cargo-deny installation documented? | Yes (`RELEASING.md`) |
| Does the server start with a documented command? | Yes (`inim serve --db ... --root .`) |
| Can a demo catalog be created without private runtime files? | Yes (steps 2–4; `data/` is created by the tool) |
| Can the existing case studies be imported deterministically? | Yes (step 4) |
| Can the web workbench be opened without live network access? | Yes (step 5) |
| Are the required root paths understandable? | Yes (`--root .` relative to the clone) |
| Are errors actionable? | Yes (clear messages observed) |

## Failures and ambiguities found

1. **`inim` is not on PATH after `cargo build --release`.** The
   quick-start invoked a bare `inim`; a stranger must use
   `./target/release/inim` or `cargo install --path .`. Corrected in
   the README (the built-binary note). No other command substitution
   was needed.

## Changes made because of this audit

- `README.md` quick-start now states that the built binary is
  `./target/release/inim` and mentions `cargo install --path .`.

## Final verified path

```
git clone https://github.com/downwithbgp/inim.git
cd inim
cargo build --release
./target/release/inim catalog init --db data/inim.sqlite
./target/release/inim catalog import --db data/inim.sqlite --root .
./target/release/inim catalog case-study import --db data/inim.sqlite --path case-studies/manlan-2019
./target/release/inim serve --db data/inim.sqlite --root .
# open http://127.0.0.1:8080
```

All steps verified end to end in a fresh temporary clone.
