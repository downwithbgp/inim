# NOC alpha evaluation — glossary (evaluator subset)

Definitions are copied from the project glossary
(`docs/GLOSSARY.md`), which is normative. Read a term only when a task
uses it. Terms marked with an asterisk (*) are explained here because
they are used in the task booklet; other project terms do not appear in
the tasks.

- **Collector** — a route collector within a source family (for
  example `route-views2`, `rrc11`). The collector site is where the
  collector peers; it is not the target's location.
- **Collector site** — the site hosting the collector that observed
  the routes (for example "Eugene, Oregon, US"). Knowing where the
  collector is hosted does not tell you where the peer or the target
  is.
- **Observer peer** — the peer at the observer session through which
  the target's routes were observed, identified by ASN and IP.
- **Observer session** — one selected collector peer session through
  which the target's routes were observed. Identity:
  `<family>/<collector> peer <peer_ip>`.
- **Target origin** — the origin ASN (or ASNs) of the reviewed target
  whose routes the analysis tracks.
- **Named relationship** — the reviewed, named routing relationship
  declared by the source event (for example an R&E — Research and
  Education — plane or a paid-peering I2PX — Internet2 Peer Exchange —
  plane between two named networks). "Named" means reviewed and
  labeled, never invented by the tool.
- **Event baseline** — the route frozen at the event start (the first
  observed route), independent of any later change. Distinct from every
  later state.
- **Pre-finding state** — the route immediately before a finding's
  change (for a withdrawal, the pre-withdrawal route). Distinct from
  the event baseline when an earlier change happened in-window.
- **Event-window final state** — the last route state at the
  event-window end, derived only from lifecycle evidence.
- **Analysis final state** — the last route state at the analysis
  boundary (window end plus cooldown), derived only from lifecycle
  evidence. Independent of the event-window final state.
- **Exact event-baseline restoration** — the active route again equals
  the event-baseline route exactly. Never claimed from the pre-finding
  state alone.
- **Insufficient visibility** — the observation cannot support a
  route-state claim (no qualifying baseline, required session absent,
  predicate not matched, or incomplete archive). Distinct from
  observing "no change".
- **No route-state change observed** — an observed signature with
  complete coverage: the target stayed visible on the reviewed path.
  It is supporting evidence only; it is not an assessment of an
  unobservable relationship.
- **Provisional cutoff** — for an open source event, the explicit
  reviewed snapshot cutoff for the analysis; the result is provisional
  and states "observed through cutoff". A later source refresh creates
  a new snapshot, plan revision, job, and run; the provisional run is
  never mutated.
