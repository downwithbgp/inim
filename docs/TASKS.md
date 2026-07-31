# inim — Tasks

## Completed

- [x] Project scaffold — Cargo.toml, directory structure, stub modules
- [x] Core domain types — Event, Expectation, Entity, Route, Transition, Wave, Assessment
- [x] Internet2 ticket parser — fixture parsing, expectation derivation, redundancy indicator detection
- [x] CLI entry point — clap derive, `analyze` subcommand with `--event`, `--rib`, `--updates`, `--output`
- [x] Documentation scaffold — REQUIREMENTS, DESIGN, TASKS, DOMAIN, DECISIONS, DATA_PROVENANCE
- [x] bgpkit-parser 0.19.0 pinned — `parser`+`local` features, ADR-008 in DECISIONS.md
- [x] Observation model — `src/domain/observation.rs`
- [x] Ingest boundary — `src/ingest.rs`: IngestContext, ObservationStream, InimError
- [x] Route reconstruction — `src/routes.rs`: RouteStateStore, phased orchestration, continuity
- [x] Tokenization — `src/tokenize.rs`: single classification point, TransitionSymbol alphabet
- [x] Wave detection — `src/waves.rs`: temporal gap-threshold clustering
- [x] Assessment — `src/assess.rs`: verdict derivation, evidence, continuity gate
- [x] Report rendering — `src/report.rs`: terminal + JSON
- [x] Fixture helpers — `src/fixtures.rs`: synthetic observation builders
- [x] Vertical slice integration test — ExpectedRedundantImpact
- [x] ADR-008 (bgpkit-parser), ADR-009 (BGPKIT Broker), ADR-010 (SEQUITUR design)
- [x] SEQUITUR core — `src/sequitur/`: grammar.rs (Grammar, Symbol, RuleId, expand, render_root), builder.rs (streaming append, digram index, depth-limited recursion, overlapping-digram handling), invariants.rs (check_invariants, exhaustive + LCG property tests, 23 tests)
- [x] Wave-motif integration — SEQUITUR-derived motifs replace provisional dominant-kind labels; describe_motif() human labels; assess.rs confirmed SEQUITUR-free

## Up next

### Production hardening

- [ ] Record-level session-boundary extraction (MrtRecord/BGP4MP STATE_CHANGE)
- [ ] Remote URL input support
- [ ] bgpkit-broker for RouteViews file discovery
- [ ] `fetch-event` command — live GRNOC scraping
- [ ] `compare` command — compare two events
- [ ] `list-waves` command — detailed wave breakdown
- [ ] SQLite persistence for historical data
- [ ] Additional network adapters

## Test count: 138 (was 115 in Session 2, 61 in Session 1)
