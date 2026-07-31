# inim — Design

## Architecture overview

```
┌─────────────────────────────────────────────────────────┐
│                     CLI (main.rs)                        │
│                 clap derive, subcommands                  │
└─────────────────────┬───────────────────────────────────┘
                      │
┌─────────────────────▼───────────────────────────────────┐
│                   Orchestration                          │
│    (future: analyze, compare, list-waves commands)       │
└──┬────────┬────────┬────────┬────────┬────────┬─────────┘
   │        │        │        │        │        │
┌──▼──┐ ┌──▼──┐ ┌──▼──────┐ ┌──▼──┐ ┌──▼───┐ ┌──▼─────┐
│ BGP │ │Toknz│ │SEQUITUR │ │Waves│ │Assess│ │ Report │
│ MRT │ │     │ │grammar  │ │     │ │      │ │        │
│ RIB │ │     │ │inference│ │     │ │      │ │        │
└──▲──┘ └─────┘ └─────────┘ └─────┘ └──────┘ └────────┘
   │
┌──┴──────────────────────────────────────────────────────┐
│                    Domain types                           │
│  Event, Expectation, Entity, Route, Transition, Wave,    │
│  Assessment, Evidence, Verdict                            │
└──────────────────────────────────────────────────────────┘
                      ▲
┌─────────────────────┴───────────────────────────────────┐
│               Sources (adapters)                          │
│  internet2/ — ticket parsing, expectation derivation     │
│  (future: other-network/)                                 │
└──────────────────────────────────────────────────────────┘
```

## Data flow for `analyze` command

```
Ticket fixture (JSON)
    │
    ▼
Internet2 adapter ──► OperationalEvent + ImpactExpectation
                            │
MRT RIB files ──► BGP module ──► seeded RouteState per observer
    │                               │
MRT UPDATE files ──────────────────┤
                                    ▼
                          RouteTransition stream
                                    │
                                    ▼
                          Tokenize ──► TransitionSymbol sequence
                                    │
                                    ▼
                          SEQUITUR ──► Grammar (motifs)
                                    │
                                    ▼
                          Waves ──► ImpactWave list
                                    │
                                    ▼
                          Assess ──► EventAssessment
                                    │
                                    ▼
                          Report ──► Terminal + JSON output
```

## Key design decisions

### Why Rust

- Explicit memory model, no GC pauses
- Strong type system for domain modeling
- Zero-cost abstractions for iterators and streaming parsers
- Single static binary for deployment
- Cargo ecosystem includes MRT parsing crates when needed

### Why no async

The initial scope (local CLI processing one ticket + bounded MRT files)
does not require network concurrency. Async adds complexity in error
handling, debugging, and trait ergonomics. It can be introduced later
if live scraping or streaming becomes necessary.

### Why crate boundaries as modules, not workspace crates

The initial codebase is small enough that multiple crates add build
overhead without meaningful isolation. Internal modules with `pub`
visibility provide sufficient boundaries. Separate crates can be
extracted later if independent versioning or compilation becomes
desirable.

### Why SEQUITUR as a standalone module

SEQUITUR must have no BGP-specific knowledge. It operates on abstract
symbol sequences. This makes it independently testable with property
tests (round-trip, digram uniqueness, rule reuse) and reusable for
non-BGP sequence analysis.

### Why parenthesized convention is Internet2-specific

Other networks do not necessarily use this convention. The core domain
model must be source-neutral. The Internet2 adapter explicitly documents
this as an Internet2 naming convention with provenance tracking, so
future adapters for other networks can implement their own expectation
derivation without misunderstanding this as a universal rule.
