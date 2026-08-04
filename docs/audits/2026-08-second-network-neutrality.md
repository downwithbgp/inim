# Second-network source-neutrality audit — 2026-08-04

Dated execution audit. The Indiana GigaPOP / Smithville review is the
first analysis of a managed network other than Internet2; this audit
records the production-source neutrality checks.

## Assumptions checked

- **AS11537 / AS11164 defaults**: no generic code path defaults a
  non-Internet2 event to the Internet2 R&E or I2PX plane. The Indiana
  GigaPOP profile carries its own reviewed routing ASN (AS19782). The
  release gate `production_source_contains_no_internet2_specific_plane_branch`
  stays green.
- **Internet2 title prefixes**: the GRNOC title convention
  (`src/conventions/grnoc.rs`) is SHARED across managed networks
  (Internet2, Indiana GigaPOP, etc.); no "I2" prefix is required. The
  I2 ticket parser (`src/sources/internet2/ticket.rs`) remains the
  Internet2-specific adapter, used only when the source family is
  Internet2.
- **Event-role defaults**: no default Internet2 event role applies to
  other networks; the profile dispatch (`src/profiles/mod.rs`) selects
  the reviewed profile by source/network mapping.
- **RouteViews collector assumptions**: no case-study collector set is
  hard-coded in generic code; each reviewed profile supplies its own
  default collector set, and the plan's collector list is reviewed
  per event.
- **Service-plane names in generic output**: report/workbench labels
  come from the reviewed manifest/profile data, never from embedded
  generic strings.

## Defects found

- **None in production logic.** One naming smell: the Indiana GigaPOP
  profile function lives in `src/profiles/internet2.rs` (historical
  layout). It is data/config code with no entity branch and is
  dispatched generically; renaming the module is deferred (would churn
  the reviewed profile file list without behavior change).
- The `smithville_rib_probe` test initially produced invalid results
  because the temp file lost its `.gz`/`.bz2` extension (the parser
  sniffs extensions to select decompression); the probe now preserves
  the original extension and prints a parse sanity counter. This was a
  research-tool defect, not a production defect.

## Entity-token scan

`git grep` over `src/` for `Indiana GigaPOP`, `Smithville`,
`INC0301970`, `AS11550`, `19782`: no production branch contains these
tokens. The tokens appear only in reviewed profile data
(`src/profiles/internet2.rs` — the Indiana GigaPOP profile comment),
test fixtures, and dated audits. The release neutrality gates pass.

## Remaining source-specific adapter boundaries

- `src/sources/internet2/ticket.rs` — Internet2 ticket parsing (the
  Internet2 source family).
- `src/profiles/internet2.rs` — reviewed Internet2 + Indiana GigaPOP
  profile data (the profile is the reviewed-config boundary).
- `src/sources/grnoc.rs` + `src/conventions/grnoc.rs` — shared GRNOC
  normalization and the shared title convention (network-neutral).
