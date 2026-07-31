# inim — Architectural Decision Records

## ADR-001: Use Rust (stable, edition 2021)

**Date:** 2025-06-15
**Status:** Accepted

**Context:** The project needs a systems language for high-performance
BGP data processing, deterministic output, and single-binary deployment.

**Decision:** Use stable Rust, edition 2021.

**Consequences:**
- Strong type system enforces domain invariants at compile time
- No GC pauses during streaming MRT processing
- Single static binary for operator deployment
- Cargo ecosystem includes MRT parsing and BGP libraries when needed

---

## ADR-002: No async code in initial version

**Date:** 2025-06-15
**Status:** Accepted

**Context:** The initial scope is a local CLI processing one ticket and
bounded MRT files. Network concurrency is not required.

**Decision:** Do not use async/await. Use synchronous iterators and
streaming parsers.

**Consequences:**
- Simpler error handling with standard `Result` types
- Easier debugging with synchronous call stacks
- Lower cognitive overhead for contributors
- Can introduce async later for live scraping or streaming if needed

---

## ADR-003: Module boundaries, not workspace crates

**Date:** 2025-06-15
**Status:** Accepted

**Context:** The codebase is small and will remain so for the MVP.

**Decision:** Use Rust modules with `pub` visibility, not separate
workspace crates.

**Consequences:**
- Single compilation unit, faster builds during development
- No crate-boundary overhead for internal types
- Can extract into crates later if independent versioning or
  compilation isolation becomes desirable

---

## ADR-004: SEQUITUR as a standalone module

**Date:** 2025-06-15
**Status:** Accepted

**Context:** SEQUITUR infers hierarchical structure from symbol
sequences. It must not contain BGP-specific knowledge to remain
reusable and independently testable.

**Decision:** Implement SEQUITUR in `src/sequitur/` with no dependencies
on domain types. It operates on abstract symbol sequences (e.g. strings
or integers).

**Consequences:**
- Property tests can use simple alphabets (a, b, c)
- Round-trip (expand grammar → original sequence) is independently
  verifiable
- The algorithm can be reused for non-BGP sequence analysis
- BGP-specific symbol assignment lives in the tokenize module

---

## ADR-005: Parenthesized convention is Internet2-specific

**Date:** 2025-06-15
**Status:** Accepted

**Context:** Internet2 tickets use parenthesized site codes (e.g.
`(NEWY32AOA)`) to indicate expected redundancy. Other networks do not
necessarily use this convention.

**Decision:** Implement the parenthesized-site-code convention in the
Internet2 adapter (`src/sources/internet2/`). Do not embed it in the
core domain model. Every derived expectation includes a provenance
string documenting the source convention.

**Consequences:**
- Core domain types remain network-agnostic
- Future adapters can implement different expectation derivation logic
- Provenance tracking ensures auditability
- Tests document the convention explicitly

---

## ADR-006: chrono for timestamps

**Date:** 2025-06-15
**Status:** Accepted

**Context:** The project needs DateTime types with serde support for
JSON serialization of route observations, event windows, and reports.

**Decision:** Use `chrono` 0.4 with `serde` feature.

**Consequences:**
- Mature, widely understood crate
- Built-in serde support via feature flag
- Timezone-aware types (Utc) for correct timestamp handling
- If dependency conflicts arise, `time` 0.3 is the fallback

---

## ADR-007: clap derive for CLI

**Date:** 2025-06-15
**Status:** Accepted

**Context:** The CLI needs subcommands (`analyze`, future `compare`,
`fetch-event`) with typed arguments.

**Decision:** Use `clap` 4.x with `derive` feature.

**Consequences:**
- Minimal boilerplate: struct + enum = CLI
- Auto-generated `--help` output
- Type-safe argument parsing
- Standard Rust CLI convention
