# Release checklist (inim)

Prepared for an initial public release of version **0.1.0** (MIT).

No git tag and no crates.io publication happen unless explicitly approved.

## Gate commands

```sh
cargo fmt --check
cargo test
cargo test --release
cargo clippy --all-targets --all-features -- -D warnings
```

## License and dependency audit

```sh
# Dependency-license audit (development tool; not linked into inim).
# If cargo-deny is not installed: `cargo install cargo-deny`.
cargo deny check licenses
```

Record the tool version (`cargo deny --version`) and the audit result.

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
mkdir -p /tmp/inim-pkg && cd /tmp/inim-pkg
tar xzf <path-to>.crate
cd inim-0.1.0
cargo test
cargo test --release
cargo clippy --all-targets --all-features -- -D warnings
```

## Before publishing (future, with approval)

1. Confirm the canonical repository URL and set `repository` in
   `Cargo.toml` before the first crates.io publish (crates.io requires it
   for new versions).
2. Decide the `publish` policy explicitly (currently unset — crates.io
   publication is enabled by default).
3. Create and push the git tag `v0.1.0` only when explicitly requested.
