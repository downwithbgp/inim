# Release checklist (inim)

Prepared for an initial public release of version **0.1.0** (MIT).

No git tag and no crates.io publication happen unless explicitly approved.

## Gate commands

```sh
cargo fmt --check
cargo test
cargo test --release
cargo test --doc
cargo clippy --all-targets --all-features -- -D warnings
cargo doc --no-deps --document-private-items   # warnings denied
scripts/audit-docs.sh                          # documentation drift audit
cargo deny check licenses
cargo deny check bans
cargo package --list
cargo package
```

## License and dependency audit

```sh
# Dependency-license audit (development tool; not linked into inim).
# If cargo-deny is not installed: `cargo install cargo-deny`.
cargo deny check licenses
```

Record the tool version (`cargo deny --version`) and the audit result.

## Catalog and web verification

```sh
inim catalog init --db data/inim.sqlite
inim catalog import --db data/inim.sqlite --root .
inim catalog sync grnoc --db data/inim.sqlite --source-dir <dir>
inim serve --db data/inim.sqlite --root . --bind 127.0.0.1:8080
```

- `serve` binds loopback only by default; non-loopback requires
  `--allow-non-loopback` and prints the no-authentication warning.
- HTTP requests never perform BGP analysis; the web layer is read-only.
- `data/`, `*.sqlite*`, and catalog runtime state are excluded from the
  crate package; migrations and templates are embedded in the binary.

## Packaging

```sh
cargo package --list      # inspect contents (see below)
cargo package             # build the .crate (--allow-dirty only if needed)
```

Inspect the packaged contents:

- `Cargo.toml` declares `license = "MIT"` and `readme = "README.md"`.
- `LICENSE` and `README.md` are present in the package.
- `cache/`, `out/`, raw MRT archives, validation outputs, archived
  generated reports, editor/temp files are **excluded**.
- `tests/fixtures/` (small, licensed, provenance-documented fixtures) is
  included; live analysis data is not.
- No credentials, private keys, or machine-specific paths are present.

## Build from the packaged source

```sh
cargo package
# locate target/package/inim-0.1.0.crate
mkdir -p tmp/inim-pkg && cd tmp/inim-pkg
tar xzf <path-to>.crate
cd inim-0.1.0
cargo test
cargo test --release
cargo clippy --all-targets --all-features -- -D warnings
```

## Before publishing (future, with approval)

1. Confirm the canonical repository URL (`repository` is already set in
   `Cargo.toml` to the public GitHub repository).
2. Decide the `publish` policy explicitly (currently unset — crates.io
   publication is enabled by default; no crates.io publication is
   planned without approval).
3. Create and push the git tag `v0.1.0` only when explicitly requested.

## Screenshot review workflow

- Run `scripts/screenshot-review.sh` (requires an installed Playwright
  chromium; loopback only; deterministic demo catalog at
  `data/inim.sqlite`). Output: `tmp/ui-review/*.png` — gitignored and
  excluded from `cargo package`; do not commit screenshots.
- Screenshots are for EXTERNAL computer-vision review; do not self-certify
  visual quality in this repository.
- Release checklist: verify `tmp/` stays out of the package
  (`cargo package --list`), and that no browser dependency enters the Rust
  runtime graph.

## Corpus packaging

- The downloaded corpus (public ticket snapshots acquired by
  `inim catalog sync grnoc` into the local database) is **excluded
  from the crate package**: `data/` and `*.sqlite*` are in the
  `exclude` list, and no corpus dump is ever committed.
- Test fixtures under `tests/fixtures/grnoc/viewer/` are small,
  provenance-documented public responses and stay in the package
  (they are required by the parser tests).
- Corpus export is metadata-only by default; do not publish raw
  payloads without a separate redistribution review
  (`docs/DATA_PROVENANCE.md`).
- The GRNOC bulk-access request draft
  (`docs/sources/GRNOC_BULK_ACCESS_REQUEST.md`) must NOT be sent
  without user approval.
