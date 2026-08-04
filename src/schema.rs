//! Persisted schema versions — the canonical registry.
//!
//! Every persisted format carries an explicit schema version. Old versions
//! are never reused after identity semantics change; old artifacts are
//! archived (see `out/archive/`) rather than silently parsed as current.
//!
//! Migration policy:
//!   - manifest: v1 (single-ASN shortcuts) → rejected with
//!     `LegacyManifestRequiresMigration`; offline `migrate-manifest` helper.
//!   - derived caches: version mismatch → invalidated and rebuilt atomically.
//!   - report/evidence/lifecycle/wave/comparison/plan artifacts: current
//!     output directories contain only current schemas.

/// Reviewed manifest schema. v1 = legacy ASN shortcuts; v2 = canonical
/// TransitPredicateMapping.
pub const MANIFEST_SCHEMA_VERSION: u32 = 2;

/// Derived RIB preflight cache schema. v2 preserves every baseline RouteKey
/// including path_id, the frozen ObserverPrefixKey cohort identity, the
/// reviewed TransitPredicate identity, and a payload checksum.
pub const RIB_CACHE_SCHEMA_VERSION: u32 = 2;

/// Derived UPDATE observation cache schema. v2 preserves full RouteObservation
/// records (path_id, complete attributes, archive order, element sequence),
/// admission counters, cohort identity, and payload checksum.
pub const UPDATE_CACHE_SCHEMA_VERSION: u32 = 2;

/// RouteObservation schema. v2 reflects ADD-PATH-aware identity semantics.
pub const OBSERVATION_SCHEMA_VERSION: u32 = 2;

/// Frozen cohort identity schema (ObserverPrefixKey values).
pub const COHORT_IDENTITY_SCHEMA_VERSION: u32 = 1;

/// Report JSON schema. v1: signature/hints/limitations structure. v2:
/// adds machine-readable `result`, `assessment`, and `archive_coverage`
/// fields alongside the existing signature (text rendering changed
/// without weakening the JSON).
pub const REPORT_SCHEMA_VERSION: u32 = 3;

/// Evidence appendix schema.
pub const EVIDENCE_APPENDIX_SCHEMA_VERSION: u32 = 1;

/// Lifecycle artifact schema.
pub const ARCHIVE_MANIFEST_SCHEMA_VERSION: u32 = 1;
pub const LIFECYCLE_ARTIFACT_SCHEMA_VERSION: u32 = 1;
pub const TRANSITIONS_ARTIFACT_SCHEMA_VERSION: u32 = 1;

/// Withdrawal audit artifact schema.
pub const WITHDRAWAL_AUDIT_SCHEMA_VERSION: u32 = 1;

/// Semantic-wave artifact schema.
pub const SEMANTIC_WAVE_SCHEMA_VERSION: u32 = 1;

/// Comparison artifact schema.
pub const COMPARISON_SCHEMA_VERSION: u32 = 1;

/// Analysis-plan artifact schema.
pub const ANALYSIS_PLAN_SCHEMA_VERSION: u32 = 1;

/// Deterministic observation identity ordering (documented contract):
///
///   1. collector
///   2. timestamp
///   3. archive order
///   4. element sequence
///   5. peer IP
///   6. prefix
///   7. path_id
///
/// Path ID ordering: `None` sorts before `Some(id)`.
pub const OBSERVATION_IDENTITY_ORDER: &str =
    "collector, timestamp, archive order, element sequence, peer IP, prefix, path_id (None < Some(id))";
