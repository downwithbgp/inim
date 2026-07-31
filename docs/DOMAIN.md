# inim — Domain Glossary

## Core types

### EventId
A unique identifier for an operational event (e.g. ticket number).
Newtype wrapper around `String`. Examples: `"CHG0107955"`, `"INC0302574"`.

### EventWindow
The declared time window for an event: `start` and `end` as
`DateTime<Utc>`.

### OperationalEvent
A parsed event from any source. Contains the id, title, declared time
window, source identifier, and the original raw data for auditability.

### RedundancyIndicator
Whether a parenthesized site code was found in an Internet2 ticket title,
and the extracted code if present. This is Internet2-specific evidence.

### ExpectationKind
Enum: `Redundant`, `NonRedundant`, `Unknown`. Describes the expected
impact type derived from the ticket.

### ImpactExpectation
A parsed operational expectation with provenance: what kind of impact is
expected, a human-readable description, and where the expectation came
from.

### EntityType
Enum: `Participant`, `Peer`, `Exchange`, `RouterSite`, `Unknown`.
Classifies a network entity referenced in an event.

### NetworkEntity
A named network entity with its type and optional site code.

### Prefix
A BGP prefix string (e.g. `"192.0.2.0/24"`). Newtype wrapper.

### AsPath
A sequence of AS numbers (e.g. `[11537, 237, 1101]`). Newtype wrapper.

### RouteAttributes
The attributes of a route: AS path, origin AS, MED, local preference,
and communities.

### RouteState
The state of a route as observed by a specific observer at a specific
time. Contains prefix, attributes, timestamp, and observer identifier.

### TransitionKind
Enum describing the kind of state change:
- `Announcement` — previously absent route appears
- `Withdrawal` — route disappears
- `ExactDuplicate` — no change
- `PathChange` — AS path changed (old, new)
- `AttributeChange` — non-path attributes changed
- `SessionReset` — observer session discontinuity
- `Restoration` — previously withdrawn route returns with original path

### RouteTransition
A transition from one RouteState to another, tagged with the kind of
transition.

### ImpactWave
A temporally concentrated set of related route transitions:
label, start, peak, end, affected prefixes, affected peer observers,
and optional SEQUITUR-derived motif.

### Verdict
Enum with 9 variants describing the assessment outcome:
`ExpectedRedundantImpact`, `ExpectedLossOfReachability`,
`UnexpectedWithdrawals`, `RedundancyFailureObserved`,
`UnexpectedBlastRadius`, `LessImpactThanExpected`,
`NoObservableBgpImpact`, `InsufficientVisibility`, `Indeterminate`.

### Evidence
A piece of evidence linking a conclusion to specific source records.

### EventAssessment
A complete assessment: event id, expectation, verdict, evidence list,
waves, and generation timestamp.

## Internet2-specific types

### Internet2Ticket
A parsed Internet2 GRNOC ticket: id, title, declared time window, and
raw fixture data.

### TicketFixture
Internal deserialization format for JSON fixture files on disk.

## Sequence types (future)

### TransitionSymbol
A canonical symbol representing a route transition (e.g. "ANNOUNCEMENT",
"PRIMARY_TO_ALTERNATE"). Used as input to SEQUITUR.

### Grammar
A SEQUITUR grammar: rules mapping non-terminal symbols to sequences of
terminals and non-terminals. Compresses and reveals structure in
transition sequences.
