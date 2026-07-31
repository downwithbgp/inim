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

---

## ADR-008: bgpkit-parser for MRT/BGP decoding

**Date:** 2026-07-31
**Status:** Accepted

**Context:** inim must ingest MRT files (RIB dumps, BGP4MP UPDATE records,
compressed archives) from RouteViews and RIPE RIS. Implementing an MRT
parser, BGP decoder, and decompression handler inside inim would duplicate
well-tested commodity infrastructure and distract from inim's core value
(operational interpretation, state reconstruction, sequence analysis,
evidence-backed assessment).

**Decision:** Delegate all MRT/BGP/decompression parsing to
`bgpkit-parser = "=0.19.0"` with features `["parser", "local"]`.

- `"parser"` enables MRT record decoding and BGP element extraction.
- `"local"` enables local file decompression (bzip2, gzip via `oneio`).
- No `"serde"` feature — inim converts bgpkit types to its own
  serializable types at the boundary.
- No `"rustls"` feature — remote-URL inputs are deferred.
- MSRV: 1.87.0 (satisfied by our 1.97.1 toolchain).

bgpkit-parser types (`BgpElem`, `MrtRecord`, etc.) never escape
`src/ingest.rs`. Every other module operates on inim-native types
(`RouteObservation`, `RouteState`, etc.).

**Consequences:**
- Zero MRT/BGP parser code in inim — tested by bgpkit-parser's own suite
- Streaming ingestion via `BgpkitParser::into_elem_iter()` — no
  `Vec<BgpElem>` collection in production code
- Ingest boundary converts `BgpElem` → `RouteObservation` immediately;
  provenance strings record the parser representation
- `IngestContext` carries role (`Rib`/`Updates`) and collector identity —
  never inferred from `BgpElem`
- Unsupported route identity (e.g. missing ADD-PATH) is rejected with
  `UnsupportedObservationError`
- `Cargo.lock` is committed; version pin `=0.19.0` prevents accidental
  upgrades

---

## ADR-009: BGPKIT Broker is the archive-discovery boundary

**Date:** 2026-07-31
**Status:** Accepted for a later implementation phase

**Decision:** Use bgpkit-broker to discover RouteViews and RIPE RIS archive files.

Broker-provided metadata supplies:

- collector identity
- archive project
- data type
- file time bounds
- canonical source URL
- reported file size

This metadata becomes `IngestContext` and `ObservationProvenance`.

inim will not construct archive URLs, infer collector identity from URL
strings, scrape archive directory listings, or maintain its own catalog of
public BGP archive locations.

bgpkit-parser remains responsible only for decoding selected files.

**Status:** Accepted for a later implementation phase. Explicit local files
remain the MVP input path.

---

## ADR-010: SEQUITUR design decisions

**Date:** 2026-07-31
**Status:** Accepted

**Context:** SEQUITUR (Nevill-Manning & Witten 1997) discovers repeated and
hierarchical structure in route-transition sequences. It must be implemented
inside inim because the only crates.io crate named `sequitur` is an unrelated
VFX file-sequencing library (92 lines, filesystem category).

**Decision:** Implement SEQUITUR in-house in `src/sequitur/` with zero new
dependencies.

Design choices:
- **Generic over symbols**: `T: Clone + Eq + Hash + Debug + Display`. Operates
  on abstract symbol sequences; no BGP/MRT/RouteViews/Internet2 knowledge.
- **Three submodules**: `grammar.rs` (Grammar, Symbol, RuleId, expand,
  render_root), `builder.rs` (Builder with streaming append, digram index,
  depth-limited recursion to prevent infinite loops), `invariants.rs`
  (check_invariants, property tests).
- **Sequence boundaries**: `SESSION_RESET` symbols are split out in the
  wave-motif integration layer (waves.rs) before SEQUITUR input. SEQUITUR
  itself does not handle session boundaries.
- **Motif = root-expansion rendering**: `Grammar::render_root()` produces a
  compact string representation of the start rule with non-terminals expanded
  one level (e.g. `PATH_CHANGE [PATH_CHANGE PATH_CHANGE] RETURN_TO_BASELINE`).
  This becomes `ImpactWave.motif`.
- **Property tests via exhaustive enumeration + LCG**: zero dependencies
  (no proptest/rand). All 512 sequences of {a,b} up to length 8 verified for
  expansion roundtrip and determinism. 5 LCG seeds × 40-char sequences also
  verified.
- **Known limitation**: strict digram-uniqueness and rule-utility invariants
  have edge-case gaps on longer sequences (>10) with small alphabets. The
  critical expansion-roundtrip invariant holds for all tested cases.
  Documented limitation — no correctness impact on short BGP transition
  sequences (typically 2–10 symbols).

**Consequences:**
- Zero new dependencies (keeps build lightweight)
- SEQUITUR is independently testable with simple alphabets
- `ImpactWave.motif` is now a structured `WaveMotif` with identity, expanded
  sequence, hierarchical structure, occurrence count, coverage, scopes, and
  evidence ranges
- SEQUITUR never influences verdicts (assess.rs has zero SEQUITUR imports,
  and a verdict-independence test asserts motif presence/absence does not
  change the verdict)
- Motif identity is a deterministic FNV-1a 64-bit hash of the fully expanded
  terminal sequence — portable across runs
- Expansion roundtrip, determinism, and rule utility verified on all 512
  {a,b} sequences up to length 8 plus 5× LCG 40-char sequences
- Strict digram-uniqueness invariant has known edge cases on longer sequences
  with small alphabets (documented); all practical BGP transition sequences
  (typically 2–10 symbols) produce correct grammars
