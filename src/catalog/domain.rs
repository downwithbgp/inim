//! Catalog domain — source-neutral identities and records.
//!
//! The catalog stores identities, revisions, status, searchable metadata,
//! summaries, and artifact paths with hashes. Large immutable data (raw
//! MRT archives, derived caches, evidence appendices, reports) remains on
//! the filesystem.
//!
//! Evidence is associated through an immutable `AnalysisRun`, never
//! directly with a mutable `CatalogEvent`.

use serde::{Deserialize, Serialize};

/// A source-neutral catalog event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogEvent {
    pub id: i64,
    pub source_kind: String,
    pub external_id: String,
    pub first_seen: String,
    pub last_seen: String,
}

/// An immutable snapshot of what the operator source said at one time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventSnapshot {
    pub id: i64,
    pub event_id: i64,
    pub fetched_at: String,
    pub source_url: String,
    pub content_sha256: String,
    /// Raw or minimally transformed source payload.
    pub raw_payload: String,
    /// Normalized event fields as JSON.
    pub normalized_json: String,
    pub parser_version: String,
}

/// An immutable reviewed manifest revision.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestRevision {
    pub id: i64,
    pub event_id: i64,
    pub snapshot_id: i64,
    pub manifest_schema: u32,
    pub payload: String,
    pub sha256: String,
    pub review_status: String,
    pub reviewed_at: Option<String>,
    pub reviewer: Option<String>,
}

/// An immutable analysis plan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisPlanRecord {
    pub id: i64,
    pub manifest_revision_id: i64,
    pub plan_schema: u32,
    pub payload: String,
    pub sha256: String,
    pub status: String,
    pub block_reason: Option<String>,
    pub created_at: String,
}

/// One execution of a plan and its evidence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisRun {
    pub id: i64,
    pub plan_id: i64,
    pub software_version: String,
    pub git_revision: Option<String>,
    pub parser_identity: String,
    pub cache_schema_version: u32,
    pub report_schema_version: u32,
    pub status: String,
    pub started_at: String,
    pub completed_at: Option<String>,
    pub runtime_secs: Option<f64>,
    pub verdict: Option<String>,
    pub assessment: Option<String>,
}

/// A cataloged artifact of an analysis run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisArtifact {
    pub id: i64,
    pub run_id: i64,
    pub kind: String,
    /// Path relative to the configured catalog root — never absolute.
    pub relative_path: String,
    pub media_type: String,
    pub schema_version: Option<u32>,
    pub sha256: String,
    pub size: i64,
    pub created_at: String,
}

/// A per-observer-prefix-stream lifecycle summary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamLifecycleSummary {
    pub id: i64,
    pub run_id: i64,
    pub collector: String,
    pub peer_ip: String,
    pub prefix: String,
    pub category: String,
    pub baseline_instances: i64,
    pub max_active_instances: i64,
    pub transition_count: i64,
    pub withdrawn: bool,
    pub restored: bool,
    pub transit_state: String,
    pub add_path_ambiguous: bool,
    /// Evidence artifact references (JSON array of relative paths).
    pub evidence_refs: String,
}

/// A semantic wave summary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticWaveSummary {
    pub id: i64,
    pub run_id: i64,
    pub wave_id: String,
    pub label: String,
    pub start: String,
    pub peak_start: String,
    pub peak_end: String,
    pub end: String,
    pub stream_count: i64,
    pub instance_count: i64,
}

/// A catalog synchronization run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogSyncRun {
    pub id: i64,
    pub source: String,
    pub started_at: String,
    pub completed_at: Option<String>,
    pub status: String,
    pub events_examined: i64,
    pub new_events: i64,
    pub changed_events: i64,
    pub unchanged_events: i64,
    pub failures: i64,
}

/// A generic source item produced by an `EventCatalogSource`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogSourceItem {
    pub source: String,
    pub external_id: String,
    pub fetched_at: String,
    pub source_url: String,
    /// Raw or minimally transformed source payload.
    pub raw_payload: String,
    /// Normalized event fields as JSON.
    pub normalized_json: String,
}
