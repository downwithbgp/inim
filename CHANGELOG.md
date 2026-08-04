## [unreleased]

### Session 55 — fabric entity taxonomy + Smithville coverage summary (2026-08-05)

- MAN LAN entity taxonomy correction: Ixia is test/measurement equipment
  (not a fabric attachment, peer, or AS node); WIX interconnect is
  interconnect context; NEAAR is a service reference; TWAREN and OMAN are
  unresolved source mentions. Fabric diagram and attachment count
  (10 -> 5) contain only reviewed attached networks.
- Reviewed interconnection context gains a bounded entity taxonomy
  (AttachedNetwork / TestEquipment / InterconnectContext /
  OperationalService / UnresolvedMention) with a dated classification
  review record.
- Smithville event page: observation-coverage summary (collector, site,
  family, target visibility, relationship visibility, qualification)
  derived from canonical preflight evidence; why Insufficient qualifying
  visibility; why no UPDATE archives were acquired.
- Source snapshot time, source lifecycle at snapshot, analysis cutoff,
  and cutoff provenance are distinct fields; the demo import prefers the
  latest reviewed immutable snapshot over the older fixture.
- Assessment leads with the operator-readable explanation; Analysis
  status is distinct from Source lifecycle.
- Evaluation kit: facilitator truths and regenerated answer key.
# Changelog

All notable changes to inim are documented here. This project follows
[Semantic Versioning](https://semver.org/). Pre-1.0: minor versions may
introduce breaking changes.

## [Unreleased] — initial public alpha (no release or tag yet)

- GRNOC event ingestion: Internet2 and Indiana GigaPOP ticket parsing with
  the parenthesized site-code convention and expectation derivation.
- Reviewed manifest and TransitPredicate planning: canonical
  `TransitPredicateMapping` (status, predicate, provenance); planning
  precedes all acquisition; blocked plans perform zero Broker/MRT work and
  exit with a documented process status.
- RouteViews/BGPKIT acquisition: broker discovery, archive caching with
  SHA-256 sidecar integrity, and RIB/UPDATE derived caches with explicit
  schema versions.
- ADD-PATH-aware observer-stream reconstruction: `RouteKey` identity
  (collector, peer, prefix, path_id), `ObserverPrefixKey` stream
  lifecycles, final-instance-loss semantics, stream-scoped continuity
  ambiguity, and four restoration kinds.
- Lifecycle and semantic-wave analysis: per-stream categories, GSHUT
  timing, evidence-bearing waves with facet counts.
- Mechanism-neutral RFC 8326 hints: GRACEFUL_SHUTDOWN observations are
  reported separately from routing impact and never change the assessment.
- Evidence-bearing reports: observed event signature, observable mechanism
  hints, limitations, evidence appendix, withdrawal audit, archive
  manifest.
- Layer-2 fabric semantics + evidence-grounded AS-path diagrams
  (2026-08): reviewed interconnection context on case studies
  (fabric attachments with reviewed ASN labels; attachment is not BGP
  adjacency and never enters analysis), server-rendered observed
  AS-path diagrams (canonical lifecycle evidence, before/after state
  comparison, absence blocks, prepend compaction, reviewed-unobserved
  relationships dashed), MAN LAN presented as Layer-2 exchange fabric
  with NORDUnet AS2603 as the analyzed target, and structured
  interconnection/path facts in the case-study API.
- Evaluator workbench corrections (2026-08): a shared artifact-path
  resolver (web run page and demo verifier agree on artifact
  availability; SHA-256/size verified against catalog metadata),
  dedicated insufficient-visibility presentation (vantage-point
  coverage table, reviewed relationship, provisional cutoff, no-UPDATE
  explanation; zeros vs not-applicable distinct; no null operator
  values), event workflow led by completed imported runs with
  fixture-import provenance disclosure, and case-study pages that
  derive analyzed targets from linked runs, lead with an operator-first
  route story, group cross-observer evidence per collector with
  peer-level detail, and present current result labels.
- Completed case studies: INC0302574 (RIPE via NYIIX — named I2PX
  relationship not assessable from the selected public observers,
  insufficient visibility) and INC0299001 (UVA via Internet2 — partial
  impact), both regenerated from canonical manifests under current
  schemas.
- Offline blocked planning: unresolved mappings produce a generic blocked
  plan artifact without an observational outcome.
