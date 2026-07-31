# inim — Tasks

## Completed

- [x] Project scaffold — Cargo.toml, directory structure, stub modules
- [x] Core domain types — Event, Expectation, Entity, Route, Transition, Wave, Assessment
- [x] Internet2 ticket parser — fixture parsing, expectation derivation, redundancy indicator detection
- [x] Stub modules — bgp, tokenize, sequitur, waves, assess, report
- [x] CLI entry point — clap derive, `analyze` subcommand with `--event`, `--rib`, `--updates`, `--output`
- [x] Documentation scaffold — REQUIREMENTS, DESIGN, TASKS, DOMAIN, DECISIONS, DATA_PROVENANCE

## In progress

*(none)*

## Up next

### BGP module

- [ ] MRT file format parsing (RIPE NCC MRT library or custom parser)
- [ ] RIB seeding: load baseline route state per observer per prefix
- [ ] UPDATE application: apply BGP UPDATE messages in timestamp order
- [ ] Route-state reconstruction with correct state-machine semantics
- [ ] Differentiate announcement, withdrawal, exact duplicate, path change, attribute change, session reset, restoration
- [ ] Fixture tests with hand-written MRT-like event sequences

### Tokenize module

- [ ] RouteState diffing: compare two RouteStates and emit a TransitionKind
- [ ] TransitionSymbol encoding: map TransitionKind to canonical symbol strings
- [ ] Batch tokenization of a Transition stream

### SEQUITUR module

- [ ] Core algorithm: digram uniqueness enforcement, rule creation, rule substitution
- [ ] Property tests: expansion round-trip, no duplicate digrams, rule reuse ≥2
- [ ] Sequence boundary markers (session resets, archive gaps)

### Waves module

- [ ] Temporal clustering of related transitions
- [ ] Start/peak/end detection per wave
- [ ] Motif assignment from SEQUITUR grammar

### Assess module

- [ ] Expectation-versus-observation comparison logic
- [ ] Verdict derivation with evidence collection
- [ ] Rules for each verdict variant

### Report module

- [ ] Terminal report rendering (human-readable)
- [ ] JSON report rendering (machine-readable)
- [ ] Evidence traceability: every conclusion links to source records

### Integration

- [ ] End-to-end vertical slice: ticket fixture → BGP → tokenize → SEQUITUR → waves → assess → report
- [ ] Golden tests for known scenarios
- [ ] First real Internet2 event analysis

### Future

- [ ] `fetch-event` command — live GRNOC scraping
- [ ] `compare` command — compare two events
- [ ] `list-waves` command — detailed wave breakdown
- [ ] SQLite persistence for historical data
- [ ] Additional network adapters
