# inim — Tasks

## Completed

- [x] Project scaffold — Cargo.toml, directory structure, stub modules
- [x] Core domain types — Event, Expectation, Entity, Route, Transition, Wave, Assessment
- [x] Internet2 ticket parser — fixture parsing, expectation derivation, redundancy indicator detection
- [x] CLI entry point — clap derive, `analyze` subcommand with `--event`, `--rib`, `--updates`, `--output`
- [x] Documentation scaffold — REQUIREMENTS, DESIGN, TASKS, DOMAIN, DECISIONS, DATA_PROVENANCE
- [x] bgpkit-parser 0.19.0 pinned — `parser`+`local` features, ADR in DECISIONS.md
- [x] Observation model — `src/domain/observation.rs` with Asn, CollectorId, Communities, ObservationSource, IngestRole, ObservationKind, RouteObservation, ObservationAttributes, ObservationProvenance
- [x] Ingest boundary — `src/ingest.rs`: IngestContext (role+collector, never inferred from BgpElem), ObservationStream (streaming, no Vec<BgpElem>), InimError (7 spec variants), bgp_elem_to_observation mapping, no unwraps
- [x] Route reconstruction — `src/routes.rs`: RouteStateStore, seed_from_rib (no transitions), apply_update (StateChange with before/after, no classification), phased orchestration (warm-up/event/cool-down), freeze_event_baseline, continuity tracking, session boundary support
- [x] Tokenization — `src/tokenize.rs`: single classification point (diff_states), no STABLE_ALTERNATE symbol, TransitionSymbol alphabet, batch tokenize via HashMap baseline
- [x] Wave detection — `src/waves.rs`: temporal gap-threshold clustering, start/peak/end per wave, dominant-kind provisional motif, summarize_waves with labels
- [x] Assessment — `src/assess.rs`: verdict derivation (ExpectedRedundantImpact, RedundancyFailureObserved, ExpectedLossOfReachability, etc.), evidence collection, continuity gate (Unknown → InsufficientVisibility)
- [x] Report rendering — `src/report.rs`: terminal report, JSON report (serde)
- [x] Fixture helpers — `src/fixtures.rs`: make_synthetic_rib/announcement/withdrawal for synthetic observation construction
- [x] Vertical slice integration test — full pipeline asserting ExpectedRedundantImpact
- [x] Demo run — `cargo run -- analyze --event tests/fixtures/internet2/CHG0107955.json` produces terminal+JSON with EXPECTED REDUNDANT IMPACT

## Up next (Session 3)

### SEQUITUR module

- [ ] Core algorithm: digram uniqueness enforcement, rule creation, rule substitution
- [ ] Property tests: expansion round-trip, no duplicate digrams, rule reuse ≥2
- [ ] Sequence boundary markers (session resets, archive gaps)
- [ ] Integrate motifs into ImpactWave descriptions (SEQUITUR must not independently decide verdict)
- [ ] Vertical slice with SEQUITUR-derived motifs

### BGPKIT Broker + remote files

- [ ] Record-level session-boundary extraction (MrtRecord/BGP4MP STATE_CHANGE)
- [ ] Remote URL input support
- [ ] bgpkit-broker for RouteViews file discovery

### Production hardening

- [ ] `fetch-event` command — live GRNOC scraping
- [ ] `compare` command — compare two events
- [ ] `list-waves` command — detailed wave breakdown
- [ ] SQLite persistence for historical data
- [ ] Additional network adapters

## Test count: 115 (was 61 in Session 1)
