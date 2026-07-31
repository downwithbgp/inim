# inim — Data Provenance

## Principle

Every conclusion rendered by inim must be traceable to concrete source
records. This document defines the provenance model and will be updated
as the system gains the ability to track data lineage through the
analysis pipeline.

## Current state

The MVP foundation establishes the data structures for provenance
tracking:

### OperationalEvent
- `source: String` — identifies the origin system (e.g. `"internet2-grnoc"`)
- `raw: Value` — preserves the original fixture data verbatim

### ImpactExpectation
- `provenance: String` — documents where the expectation came from
  (e.g. `"Internet2 title convention: parenthesized site code indicates
  expected redundancy"`)

### Evidence
- `description: String` — human-readable explanation
- `source_records: Vec<String>` — references to specific source records
  that support the conclusion

### RouteState
- `observer: String` — identifies which collector:peer reported this state
- `timestamp: DateTime<Utc>` — when the observation was made

### EventAssessment
- `generated_at: DateTime<Utc>` — when the assessment was produced

## Future provenance model

Each analysis stage will add provenance metadata:

1. **Ticket ingestion**: fixture file path, parse timestamp
2. **BGP reconstruction**: MRT file paths, RIB sequence numbers,
   UPDATE message offsets
3. **Tokenization**: which RouteStates were compared
4. **SEQUITUR**: which sequences were fed, grammar derivation steps
5. **Wave detection**: clustering parameters, transition membership
6. **Assessment**: verdict rules triggered, evidence sources
7. **Report**: output file path, rendering timestamp

## Audit trail

The JSON report format will include a `provenance` section enumerating:
- Input files (paths, hashes)
- Processing parameters
- Per-conclusion source record references
- Reproducibility metadata (version, timestamp, random seed if any)

## Reproducibility

Reports must be deterministic. Given the same inputs (ticket fixture,
MRT files, configuration), inim must produce identical output. This
requires:
- No random number generators (use deterministic algorithms)
- Stable sorting of collections
- Explicit timestamp ordering
- No floating-point comparisons in decision logic
