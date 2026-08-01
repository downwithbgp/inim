# Session 35 — Correct the Internet2 service-plane model and review performance

Starting HEAD: `6ebac15` (744 tests). Follows the user's 14-part brief; each
part's required tests are listed verbatim. All existing tests must stay green
(744 baseline).

## Design decisions

### D1 — Network profile as data (Parts 1, 2, 7)
New generic module `src/catalog/netprofile.rs`:
- `NamedServicePlane { id, display_label, asns: Vec<u32> }`
- `ReviewedAsnRole { asn, role: String }` — role vocabulary is DATA
  (`regional-re`, `national-re`, `international-nren`, `exchange-participant`,
  `internet2-re`, `internet2-i2px`); unknown → display "unclassified observed ASN".
- `NetworkProfile { service_planes, asn_roles, updated_utc, provenance }`,
  loadable from JSON.
- Session classification: `SessionRelationship { DirectPeerToNamedPlane{plane_id}, IndirectPathViaNamedPlane{plane_id}, OtherObservedPath, Ambiguous }` — generic enum, allowed.
- `ObserverSessionKey { source_family, collector, peer_ip, peer_asn, address_family }`.

Reviewed profile data file: `case-studies/manlan-2019/pilot/network-profile.json`
(AS11537 = Internet2 R&E; AS11164 = Internet2 Peer Exchange/I2PX; role
mappings for ASNs that actually appear in the 2019 paths — from the session
audit, not invention; others stay unclassified).

**Plane-branch gate** (`tests/release_test.rs`,
`production_source_contains_no_internet2_specific_plane_branch`):
- `i2px` and `11164`: **zero occurrences anywhere in src/** (currently true —
  the I2PX plane identity is data-only; all tests use generic ASNs).
- `11537` and `internet2` (case-insensitive): allowed only in the files that
  already contained them at session start. The gate embeds the frozen
  pre-session hit sets (21 files for 11537, 24 for internet2 — all
  pre-existing: doc comments naming the operator, the GRNOC ticket-title
  source (`sources/internet2`, `profiles/`), legacy single-plane verdicts
  (`assess.rs`), manifest legacy-field migration fixtures (`manifest.rs`),
  test fixtures) and asserts the live hit set EQUALS the frozen set. Any
  session-35 file containing the tokens fails the gate. Reviewed plane
  values live only in `case-studies/*.json` data files, asserted in
  `tests/release_test.rs`.

### D2 — Session classification (Part 2)
Per-route classification: `peer_asn ∈ plane.asns` → Direct; else path contains
plane asn → Indirect; else Other. A route may match multiple planes. Per-session
role is derived from the route set at a given RIB timestamp — time-scoped
(roles carry the evidence RIB timestamp). Source family NEVER determines
plane: classification takes the profile + RIB peer metadata only.

### D3 — Collector metadata with temporal provenance (Part 3)
New data file `case-studies/manlan-2019/pilot/collector-locations.json`:
`{ family, collector, location, note, source, as_of }` for all 18 RIS
candidates + route-views2/6. RRC06 = **Otemachi, Japan** (not United States).
All displayed locations audited against RIPE RIS / RouteViews public pages
(web_fetch) and recorded with provenance. Location ≠ route geography; the
web/report text says so where relevant.

### D4 — Session audit from MRT evidence (Part 3)
New CLI `inim catalog session-audit --root case-studies/manlan-2019/pilot
--cache cache/... --db data/corpus.sqlite`: for each baseline RIB in the
local cache (18 RIS bviews + route-views2 rib.20190821.0200.bz2), parse once
and emit per-peer rows: source_family, collector, location (from metadata),
RIB ts + sha, peer_ip, peer_asn (MRT header — source of truth), address_family,
AS2603-origin route count, distinct prefixes, path-class counts
(11537-only / 11164-only / both / neither). Output:
`pilot/session-audit-2019.json` + rendered table in `pilot/session-audit-2019.md`.

### D5 — Source-level origin extraction cache (Part 10.4, enables Part 3/5/6)
Versioned, origin-scoped extraction cache: after a RIB (and UPDATE) parse,
persist the origin-matching observations (pre-predicate) as
`cache/extracted/<source_sha16>/<family>-<collector>-<ts>.json.gz` keyed by
(source sha, family, collector, SORTED origin set, parser/schema version) —
NOT by predicate (origin-set canonicalization: sorted ASNs). Extraction rows
are full RouteObservations (peer_ip, peer_asn, address_family, prefix,
complete AS path, timestamp, path_id) — the session audit's path-class counts
and the inventory's plane classification are computed AFTER load, so no
information needed by any consumer is dropped. On a later parse request for
the same source+origin, load the extraction instead of decompress+parse;
predicate filtering/admission runs identically in memory. Evidence IDs
derive from observation content → unchanged. Standalone vs reused outputs
must be byte-identical. Extraction is origin-scoped (small: hundreds of
routes), never a full-table warehouse. Both the session audit and the
two-plane runs consume it.

### D6 — Cohort selector vs path classifiers (Part 5)
Manifest keeps `transit_predicate` (cohort selector) and gains optional
`path_classifiers: Vec<NamedPathClassifier{ id, display_label, predicate }>`
(manifest data, serde default empty). New plane-specific manifests
(independent AnalysisRuns, same engine, same target/window):
- `MANLAN-2019-NORDUNET-PILOT-RE.json` — selector ContainsAny[11537],
  classifiers RE + I2PX
- `MANLAN-2019-NORDUNET-PILOT-I2PX.json` — selector ContainsAny[11164],
  classifiers RE + I2PX
Origin-only inventory: `inim analyze --origin-inventory-only` (or a
report-only builder over the extraction): all AS2603 baseline routes per
observer classified by named plane (one/both/neither), NO verdict.

### D7 — Corrected comparison matrix + web (Parts 6, 11, 12)
New artifact `pilot/cross-observer-matrix.{json,md}`: per run/inventory row —
collector, location, peer ASN, direct/indirect, baseline streams/prefixes,
named plane, temp absences, path replacements, R&E departures/returns,
I2PX departures/returns, other-path transitions, restoration, evidence
interval. Built from: session audit + S34 runs + new plane runs + inventory.
S34 runs are NOT relabeled: rrc00/rrc06/rrc15 AS11537-in-path results are
reported as **indirect R&E observations** (unless the audit shows direct
11164 peers); "direct I2PX" appears only where a peer_asn=11164 session
exists. Missing I2PX baseline → "no direct I2PX baseline available", never
"no I2PX event change".
Web comparison rows gain: location, peer ASN, direct/indirect, named plane,
cohort predicate (loaded from audit + profile data). Case-study first screen
gains the planes-explainer. Conclusion rewritten per the brief's structure
from actual evidence.

### D8 — GRNOC policy (Part 8)
`src/catalog/access.rs`: `DEFAULT_REQUESTS_PER_SECOND = 5.0`,
`DEFAULT_MAX_CONCURRENCY = 5`, `DEFAULT_BURST = 2`. New smooth token-bucket
limiter: capacity = burst (2), refill = 5/s → at most 2 immediate requests,
then paced. In-flight counter capped at max_concurrency (5). Adaptive:
429 → honor Retry-After AND halve effective rate immediately (floor 0.25);
two consecutive 429s → Stop(RepeatedRateLimited) (existing);
sustained success (e.g. 20 requests without throttle) → bounded recovery
(doubling) up to configured ceiling. `--allow-higher-rate` required for
>5.0; CLI error text and `--show-access-policy` updated. Conditional
requests, budgets, immutable snapshots, no enumeration all retained.
Bulk-access draft updated with the new reviewed local policy; NOT sent.

### D9 — Bounded live sync (Part 9)
`inim catalog sync grnoc --db data/corpus.sqlite --case-study manlan-2019
--max-requests 75 --requests-per-second 5` — the case-study references are
the known catalog IDs (ten MAN LAN tickets); `--expand-references` is NOT
passed (expansion is opt-in; a non-expanding sync over known IDs + conditional
refreshes satisfies "no enumeration"). The sync already stores conditional
ETag/Last-Modified state for 304 refreshes. Extend the sync report to record:
configured/observed rate, max in-flight, latency distribution (min/p50/max),
HTTP status counts, control messages, Retry-After responses, effective-rate
reductions, bytes, changed/unchanged snapshots. Stop immediately on source
feedback.

### D10 — Performance review (Part 10)
- 10.1 CPU topology: measured — VM exposes 12 vCPUs (1 socket × 6 cores × 2
  threads, E5-2630L v2); nproc/nproc --all/cpuinfo/cpuset/affinity all = 12;
  no cgroup limit; the 24-core figure describes the physical host.
  `perf::host_info` already reports host vs process visibility; add a
  `cpu_topology_report` (lscpu summary + cgroup + affinity) with graceful
  degradation + test.
- 10.2 Stage metrics: capture per-stage timings (acquisition, raw-cache
  validation, RIB parse, UPDATE parse, admission, derived-cache write,
  merge, reconstruction, lifecycle) for the benchmark runs; reconstruct S34
  stage data from existing artifacts where recorded, re-measure
  representative runs locally otherwise. → `docs/BENCHMARK.md`.
- 10.3 Jobs benchmark (local cache, no network): jobs 1/4/8/12/16/24 on
  (a) rrc00 bview preflight, (b) rrc00 UPDATE pilot, (c) repeated two-plane
  RIB analysis. /usr/bin/time -v for wall/user/sys/RSS.
- 10.4 Repeated work: extraction cache (D5) — same RIB parsed once for
  both planes; update path shares the mechanism where safe.
- 10.5 Acceptance: ≥2× on repeated two-plane local-cache preflight (parse
  once vs twice) or documented unavoidable cost; outputs identical across
  worker counts and cache paths.

## Task breakdown (implementation order = dependency order)

1. netprofile module + profile data file + gate test extension
   (Parts 1, 2, 7 tests: 15 required).
2. Collector metadata file (D3) + location audit (web_fetch RIPE/RouteViews).
3. Extraction cache (D5) in RIB path + tests (Part 10.4: 5 required).
4. Session-audit CLI (D4) + run over local caches + audit artifacts
   (Part 3 tests: 5 required).
5. Wording corrections (Part 4: 4 required) + web display separation.
6. Path classifiers + plane manifests + origin-inventory (Part 5: 5 required);
   execute new plane runs (R&E + I2PX) + inventory.
7. Matrix artifact (D7) + web columns + first-screen + conclusion
   (Parts 6, 11, 12: 5 required).
8. CPU topology report + benchmarks (Part 10.1–10.3, 10.5) + BENCHMARK.md.
9. GRNOC policy (Part 8: 9 required) + bulk-access draft update. NOTE: this
   deliberately updates two existing tests to the reviewed values —
   `default_rate_is_conservative` (asserts 0.25 → becomes
   `default_grnoc_rate_is_five_per_second` at 5.0) and
   `higher_rate_requires_explicit_flag` (threshold 1.0 → 5.0; rps=2.0 no
   longer errors, rps=6.0 does) — the brief mandates the new policy.
10. Bounded live sync + metrics report (Part 9).
11. Documentation (Part 13).
12. Quality gates + completion report (Part 14).

## Verification gates
- After each part: `cargo test` + `cargo clippy --all-targets --all-features -- -D warnings` + `cargo fmt --check`; required-test names grep (0 missing).
- Gate test additions must pass: `cargo test --test release_test`.
- Final: full gate chain (fmt, test, test --release, clippy, deny licenses,
  deny bans, cargo package) + confirmations list from Part 14.
- No publish/tag/push; bulk-access request not sent.
