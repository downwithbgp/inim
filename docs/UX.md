# UX: the operator task model

This document defines what the incident workbench is FOR and therefore
what its first screen must show. It is the accepted alpha baseline for
the workbench design.

## The primary workbench task

> **During this event, what externally visible routing changes occurred,
> where were they observed, which prefixes were involved, and how did
> they end?**

The primary workbench is **not**:

- a database coverage report
- a schema browser
- a count dashboard
- an analysis-engine status page

## The questions the workbench answers, in order

1. What was the earliest meaningful externally observed change?
2. Which observer and peer saw it?
3. Which prefixes were affected?
4. What exactly changed?
5. What path existed before?
6. What path or absence followed?
7. When did visibility or the event-baseline route return?
8. Which other observers saw a related or different signature?
9. Which observers had no relevant visibility?
10. What should the operator inspect internally?

Abstract coverage counts ("8 of 10 eligible observer sessions",
"58 of 80 baseline streams", "12 distinct prefixes") are **secondary
coverage facts**. They answer none of the questions above on their own;
they belong in an **Observation coverage** section after the findings.

## What the first screen must contain (changed event)

- event title
- displayed analysis scope
- scope limitation
- changed findings in **operational priority order** (not
  chronological; the dense findings table below sorts by time), each
  showing:
  - exact observation time
  - named observer site
  - peer ASN (and reviewed peer name where available)
  - exact prefix group (one action away from the full list)
  - before route (the pre-finding state; the event baseline is a
    distinct earlier state and is never relabeled "baseline")
  - after route or explicit absence
  - restoration / outcome (visibility returned, exact event-baseline
    restoration, still changed at window end)

Then, in order: **Observer comparison by region** (concrete per-site
statements), the dense **Routing findings** table (time-ordered rows),
**Timeline** (secondary), **Suggested internal checks** (evidence-linked
cues), and **Observation coverage** (breadth ratios, no-baseline
sessions, archive coverage). A collapsed **Context** block (event
context, linked tickets) sits between the findings and the region
comparison; a collapsed **Analysis history** and **Observer episodes**
table follow the coverage section.

## The unit of analysis: RoutingFinding

A `RoutingFinding` is one coherent routing story at one observer
session: which prefixes, what changed, what the paths were, and how the
episode ended. It is a **presentation model derived from existing
canonical evidence** (observer episodes + exact lifecycle paths +
reviewed ASN identities) — it introduces no new transition or lifecycle
semantics.

Findings group observations only when they share:

- the same observer session
- the same effect
- the same semantic before-state
- the same semantic after-state
- a coherent temporal cluster

Unrelated path changes at one collector are never combined; one coherent
prefix group is never split merely because the evidence has several
internal facets. A stream with both a withdrawal and an earlier visible
path change is split into two findings (visible change + absence) that
are never merged; the absence finding links to the separate prepend
finding only when the canonical transition is a real prepend delta.

## Statement vocabulary

Exact verbs only:

- `stopped seeing`
- `became absent`
- `withdrew`
- `changed AS path`
- `reduced prepending`
- `left the reviewed path plane`
- `returned to visibility`
- `restored the event-baseline path`
- `remained changed`

Forbidden unless separately reviewed operational evidence supports them:
`protected`, `failed over`, `backup path`, `rerouted around the
outage`, `traffic restored`.

## No-change and no-visibility events

A no-change event leads with the visibility statement (e.g. the reviewed
I2PX relationship audit), not with a session ratio. A supporting
R&E-plane observation is shown as supporting only — never as proof about
the named I2PX relationship. No-baseline conditions are coverage
limitations, never "no change". The coverage reason vocabulary
(`RequiredSessionAbsent`, `SessionPresentNoTargetBaseline`,
`PredicateNotMatched`, `ArchiveIncomplete`, `UnsupportedSource`)
distinguishes "no session existed" from "session existed, target not
visible".

## Rendering rules

- Exact ASN sequences are authoritative; reviewed names are enrichment.
- Compact summary paths may collapse repeated ASNs (`AS24489×4`); the
  exact uncollapsed path is retained in the drill-down and JSON.
- Every changed finding exposes directly: exact prefix list, pre-finding
  AS path, changed AS path or explicit absence, final observed AS path,
  first-change timestamp, restoration timestamp, evidence reference.
- Copy actions (prefixes, before paths, after paths) are progressive
  enhancement only — the exact data is visible without JavaScript.
- No primary table consists mainly of abstract counters; internal terms
  (schema versions, run IDs, transition counts) stay in Analysis
  details / Provenance / JSON API.
- Filter state is a deterministic query: `?changed=1`,
  `?kind=absent|path|plane|unchanged|withdrawn|prepend|mixed`,
  `?region=AMER|EMEA|APAC`, `?rel=direct|indirect`, `?expand=1`,
  `?episode=`, `?prefixes=`, `?view=timeline`.

## Old-school NOC design

Rectangular panels, square corners, thin rules, strong headers,
restrained grey section headers, compact line height, monospaced
technical values, conventional underlined blue links, visible focus.
No animation, no external fonts, no CDN dependencies, no SPA; color is
additional signal, never the only signal. Server-rendered HTML with
small progressive JavaScript for sort/expand/copy only.

## Mobile behavior

Below 640 px the result and scope stay on top; the context block and
filters collapse (progressive enhancement); fact rows stack; episode
tables reflow into labeled definition lists; wide tables remain
scrollable; timestamps, ASNs, prefixes, and collector IDs never wrap or
shrink below readable size.

## Alpha baseline freeze

The compact workbench design is accepted as the alpha baseline:

- overall information hierarchy
- compact finding cards
- route-sequence expansion
- prefix drill-down
- old-school visual design
- mobile layout

Two post-freeze semantic corrections are part of the baseline because
they make the rendered story agree with the canonical route evidence:
the event-baseline vs pre-finding-state distinction (including exact
event-baseline restoration phrasing) and the earlier-change link between
an absence finding and a genuine separate prepend finding.

Future changes to the workbench should be driven by one of:

- a newly analyzed incident
- contradictory evidence
- measured NOC-user feedback
- accessibility defects

Do not continue iterative visual polishing based solely on the existing
three case studies. Semantic corrections that make the rendered story
agree with the canonical route evidence remain in scope; visual
redesigns do not.
