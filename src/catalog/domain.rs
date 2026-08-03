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
    /// First observed route-state change for this stream (UTC, from the
    /// immutable lifecycle evidence).
    pub first_change_utc: Option<String>,
    /// Stream restoration time (UTC, from the immutable lifecycle
    /// evidence), when the stream restored.
    pub restoration_time_utc: Option<String>,
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

/// Observed peer-session metadata.
///
/// The peer ASN is an OBSERVED protocol fact from baseline RIB
/// evidence, time-scoped by the RIB timestamp — distinct from reviewed
/// organization labels and roles. Multiple observations of the same
/// (collector, peer IP, address family) with DIFFERENT peer ASNs mean
/// the session's ASN is ambiguous and must be rendered as such.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObserverSessionMetadata {
    /// Row identity; defaults to 0 when parsed from a reviewed data
    /// file (never written back).
    #[serde(default)]
    pub id: i64,
    pub source_family: String,
    pub collector: String,
    pub peer_ip: String,
    pub address_family: String,
    pub peer_asn: u32,
    pub valid_from: String,
    pub valid_to: Option<String>,
    pub source_archive: String,
    pub source_sha256: String,
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

// ── Corpus discovery layer  ────────────────
//
// Discovery records how a ticket identifier entered the corpus; fetch
// records capture one HTTP fetch attempt per row. Event snapshots stay
// pure content-addressed immutability — fetch metadata never mutates a
// snapshot row.

pub const DISCOVERY_ANALYST_SEED: &str = "AnalystSeed";
pub const DISCOVERY_DOCUMENT_REFERENCE: &str = "DocumentReference";
pub const DISCOVERY_DESCRIPTION_REFERENCE: &str = "TicketDescriptionReference";
pub const DISCOVERY_PUBLIC_SEARCH_RESULT: &str = "PublicSearchResult";
pub const DISCOVERY_CASE_STUDY_REFERENCE: &str = "CaseStudyReference";
pub const DISCOVERY_OTHER_REVIEWED_SOURCE: &str = "OtherReviewedSource";

pub const DISCOVERY_STATUS_PENDING: &str = "Pending";
pub const DISCOVERY_STATUS_FETCHED: &str = "Fetched";
pub const DISCOVERY_STATUS_UNRESOLVED: &str = "Unresolved";
pub const DISCOVERY_STATUS_FAILED: &str = "Failed";

/// One discovery path of a ticket identifier into the corpus.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TicketDiscovery {
    pub id: i64,
    pub source_kind: String,
    pub external_id: String,
    /// One of the `DISCOVERY_*` provenance constants.
    pub provenance: String,
    /// Snapshot whose description text referenced the ticket
    /// (TicketDescriptionReference).
    pub source_snapshot_id: Option<i64>,
    /// Document whose text referenced the ticket (DocumentReference).
    pub source_document_id: Option<i64>,
    pub discovered_at: String,
    /// Pending | Fetched | Unresolved | Failed.
    pub status: String,
}

/// One HTTP fetch attempt against a public source.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotFetch {
    pub id: i64,
    pub event_id: i64,
    pub sync_run_id: i64,
    pub fetched_at: String,
    pub source_url: String,
    pub http_status: i64,
    pub content_type: Option<String>,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    /// e.g. "grnoc-viewer-api".
    pub acquisition_method: String,
    pub retry_count: i64,
    /// NULL when the fetch produced no new content (304/unchanged).
    pub snapshot_id: Option<i64>,
    pub conditional_requested: bool,
}

// ── Ticket relationship graph  ──────────────

/// Neutral relationship kinds. A specific kind is used only where the
/// surrounding source wording supports it; otherwise `References`.
pub const RELATIONSHIP_REFERENCES: &str = "References";
pub const RELATIONSHIP_TRACKS_REMAINING_IMPACT: &str = "TracksRemainingImpactIn";
pub const RELATIONSHIP_SUPERSEDED_BY: &str = "SupersededBy";
pub const RELATIONSHIP_RELATED_CHANGE: &str = "RelatedChange";
pub const RELATIONSHIP_RELATED_INCIDENT: &str = "RelatedIncident";
pub const RELATIONSHIP_RELATED_TASK: &str = "RelatedTask";
pub const RELATIONSHIP_UNKNOWN_REFERENCE: &str = "UnknownReference";
/// Machine-derived overlap candidate — explicitly NOT a causal edge.
pub const RELATIONSHIP_TEMPORAL_OVERLAP: &str = "TemporalOverlap";
/// Machine-derived shared-reviewed-entity overlap — NOT a causal edge.
pub const RELATIONSHIP_ENTITY_OVERLAP: &str = "EntityOverlap";
/// Reviewed kinds (assigned by analyst review, never by
/// automatic wording classification).
pub const RELATIONSHIP_ROLLBACK_FOR: &str = "RollbackFor";
pub const RELATIONSHIP_PARTICIPANT_IMPACT_DURING: &str = "ParticipantImpactDuring";
pub const RELATIONSHIP_ALARM_DURING: &str = "AlarmDuring";
pub const RELATIONSHIP_OPERATIONAL_TASK_DURING: &str = "OperationalTaskDuring";

/// Evidence kinds: explicit provenance vs derived candidates.
pub const EVIDENCE_EXPLICIT_TICKET_TEXT: &str = "ExplicitTicketText";
pub const EVIDENCE_REFERENCE_DOCUMENT: &str = "ReferenceDocument";
pub const EVIDENCE_SHARED_CASE_STUDY: &str = "SharedCaseStudy";
pub const EVIDENCE_ANALYST_REVIEWED: &str = "AnalystReviewed";
pub const EVIDENCE_DERIVED_TEMPORAL_OVERLAP: &str = "DerivedTemporalOverlap";
pub const EVIDENCE_DERIVED_ENTITY_OVERLAP: &str = "DerivedEntityOverlap";

pub const REVIEW_UNREVIEWED: &str = "Unreviewed";
pub const REVIEW_ACCEPTED: &str = "Accepted";
pub const REVIEW_REJECTED: &str = "Rejected";

/// A source-neutral ticket-to-ticket relationship edge.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TicketRelationship {
    pub id: i64,
    /// The ticket whose source text/document asserts the edge.
    pub from_event_id: i64,
    /// Resolved target; NULL while the identifier is unresolved.
    pub to_event_id: Option<i64>,
    /// The target ticket identifier (always present).
    pub to_external_id: String,
    /// One of the `RELATIONSHIP_*` kinds.
    pub relationship_kind: String,
    /// One of the `EVIDENCE_*` kinds.
    pub evidence_kind: String,
    pub source_snapshot_id: Option<i64>,
    pub source_document_id: Option<i64>,
    pub reviewed_status: String,
    pub note: Option<String>,
    pub created_utc: String,
}

// ── Case-study layer ──────────────────────────────────
//
// A CaseStudy is a reviewed grouping and interpretation of several sources
// and analysis runs. It never owns observations: evidence continues to
// belong to immutable AnalysisRuns (CaseStudy → AnalysisRun → stream
// lifecycle → route-instance evidence).

/// Generic relationship vocabulary between a case study and a ticket.
pub const RELATIONSHIP_PRIMARY_CHANGE: &str = "PrimaryChange";
pub const RELATIONSHIP_PRIMARY_INCIDENT: &str = "PrimaryIncident";
pub const RELATIONSHIP_ROLLBACK_CHANGE: &str = "RollbackChange";
pub const RELATIONSHIP_PARTICIPANT_INCIDENT: &str = "ParticipantIncident";
pub const RELATIONSHIP_ALARM: &str = "Alarm";
pub const RELATIONSHIP_OPERATIONAL_TASK: &str = "OperationalTask";
pub const RELATIONSHIP_COMMUNICATION: &str = "Communication";
pub const RELATIONSHIP_RELATED: &str = "Related";

pub const CLAIM_TYPE_REPORTED_IMPACT: &str = "ReportedImpact";
pub const CLAIM_TYPE_REPORTED_MECHANISM: &str = "ReportedMechanism";
pub const CLAIM_TYPE_REPORTED_TIMELINE: &str = "ReportedTimeline";
pub const CLAIM_TYPE_REPORTED_RECOVERY: &str = "ReportedRecovery";
pub const CLAIM_TYPE_REPORTED_LIMITATION: &str = "ReportedLimitation";
pub const CLAIM_TYPE_PROCESS_FINDING: &str = "ProcessFinding";

pub const OBSERVABILITY_POTENTIALLY_VISIBLE: &str = "PotentiallyVisibleInPublicBgp";
pub const OBSERVABILITY_INDIRECTLY_VISIBLE: &str = "IndirectlyVisible";
pub const OBSERVABILITY_NOT_DIRECTLY_VISIBLE: &str = "NotDirectlyVisible";
pub const OBSERVABILITY_UNKNOWN: &str = "Unknown";

pub const TARGET_STATUS_UNRESEARCHED: &str = "Unresearched";
pub const TARGET_STATUS_CANDIDATE: &str = "Candidate";
pub const TARGET_STATUS_HISTORICALLY_REVIEWED: &str = "HistoricallyReviewed";
pub const TARGET_STATUS_UNRESOLVED: &str = "Unresolved";
pub const TARGET_STATUS_NOT_APPLICABLE: &str = "NotApplicableToPublicBgp";
pub const TARGET_STATUS_AMBIGUOUS_SERVICE_IDENTITY: &str = "AmbiguousServiceIdentity";

pub const PHASE_PRECISION_EXACT: &str = "exact";
pub const PHASE_PRECISION_SUMMARIZED: &str = "summarized";

pub const PLAN_STATUS_DRAFT: &str = "Draft";
pub const PLAN_STATUS_REVIEWED: &str = "Reviewed";

/// A reviewed multi-source incident case study.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaseStudy {
    pub id: i64,
    pub slug: String,
    pub title: String,
    pub summary: String,
    pub start_utc: Option<String>,
    pub end_utc: Option<String>,
    pub status: String,
    pub content_sha256: String,
    pub created_utc: String,
    pub updated_utc: String,
}

/// Link between a case study and a related ticket.
///
/// `catalog_event_id` is NULL when the ticket is only referenced by a source
/// document and no independent source snapshot exists in the catalog — the
/// external identifier is preserved as an unresolved document reference.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaseStudyEventLink {
    pub id: i64,
    pub case_study_id: i64,
    pub catalog_event_id: Option<i64>,
    pub external_identifier: String,
    pub relationship: String,
    pub reviewed_note: Option<String>,
    pub sort_order: i64,
    pub source_document_id: Option<i64>,
}

/// An immutable external reference document (e.g. an after-action report).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReferenceDocument {
    pub id: i64,
    pub title: String,
    pub source_url: Option<String>,
    pub doc_type: String,
    pub redistribution_status: String,
    pub publication_date: Option<String>,
    pub provenance: String,
    pub imported_utc: String,
}

/// One content revision of a reference document (deduplicated by SHA-256).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentRevision {
    pub id: i64,
    pub document_id: i64,
    pub revision: i64,
    pub sha256: String,
    pub media_type: String,
    pub page_count: Option<i64>,
    /// Catalog-relative local path; NULL when the file is not available locally.
    pub local_path: Option<String>,
    pub metadata_json: Option<String>,
    pub imported_utc: String,
}

/// Link between a case study and one of its source documents.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaseStudyDocumentLink {
    pub id: i64,
    pub case_study_id: i64,
    pub document_id: i64,
    pub relationship: String,
    pub reviewed_note: Option<String>,
}

/// A reviewed phase of the incident timeline.
///
/// `start_precision`/`end_precision` are `exact` (stated in the detailed
/// timeline) or `summarized` (broad boundary summarized by the source);
/// retrospective belief is never rendered as measured fact.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaseStudyPhase {
    pub id: i64,
    pub case_study_id: i64,
    pub label: String,
    pub start_utc: String,
    pub end_utc: String,
    pub start_precision: String,
    pub end_precision: String,
    pub description: String,
    pub source_document_id: i64,
    pub source_page_or_section: String,
    pub review_status: String,
    pub sort_order: i64,
}

/// Link between a case study and one event-conditioned analysis run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaseStudyAnalysisLink {
    pub id: i64,
    pub case_study_id: i64,
    pub run_id: i64,
    pub role: String,
    pub reviewed_note: Option<String>,
}

/// A reviewed operator-reported claim with its observability classification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaseStudyClaim {
    pub id: i64,
    pub case_study_id: i64,
    pub claim_type: String,
    pub claim_text: String,
    pub qualification: Option<String>,
    pub source_document_id: i64,
    pub source_page_or_section: String,
    pub review_status: String,
    pub time_or_phase: Option<String>,
    pub observability: String,
    pub observability_rationale: String,
    pub sort_order: i64,
}

/// A candidate BGP-analysis target derived from the source document.
///
/// Research status is reviewed data; `Unresearched` and
/// `NotApplicableToPublicBgp` are valid states, never guesses.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaseStudyTarget {
    pub id: i64,
    pub case_study_id: i64,
    pub source_label: String,
    pub role_in_report: String,
    pub candidate_org_identity: Option<String>,
    pub candidate_origin_asns_json: Option<String>,
    pub candidate_predicate: Option<String>,
    pub historical_validity_status: String,
    pub provenance: Option<String>,
    pub research_status: String,
    pub reviewed_note: Option<String>,
    pub sort_order: i64,
}

/// A recorded historical-analysis plan (Draft until reviewed).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaseStudyAnalysisPlan {
    pub id: i64,
    pub case_study_id: i64,
    pub horizon_json: String,
    pub plan_json: String,
    pub status: String,
    pub created_utc: String,
}

/// One route transition of a run, imported from the transitions artifact.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunTransitionRecord {
    pub id: i64,
    pub run_id: i64,
    pub seq: i64,
    pub kind: String,
    pub occurred_utc: String,
    pub run_phase: String,
    pub collector: String,
    pub peer_ip: String,
    pub prefix: String,
    pub path_id: Option<i64>,
    pub material_path_changed: bool,
    pub communities_changed: bool,
    pub announced: bool,
    pub withdrawn: bool,
    pub observation_id: Option<i64>,
    pub archive_sha256: Option<String>,
}

/// One evidence signal behind a candidate group.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct GroupEvidence {
    pub signal: String,
    pub detail: String,
}

/// A candidate group of tickets that may describe parts of one incident.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct IncidentGroupCandidate {
    pub id: i64,
    pub label: String,
    /// Sorted catalog event ids of the members.
    pub member_event_ids: Vec<i64>,
    pub evidence: Vec<GroupEvidence>,
    pub confidence: String,
    pub review_status: String,
    /// Deterministic hash of members + evidence; regeneration skips
    /// rejected fingerprints until the evidence changes.
    pub evidence_fingerprint: String,
    pub created_utc: String,
    pub updated_utc: String,
}

// ── Reviewed ticket interpretation ────────────

/// Reviewed case-study roles for a ticket. These are analyst-reviewed
/// case-study roles and NEVER replace the source task type.
pub mod reviewed_role {
    pub const CHANGE_WINDOW: &str = "ChangeWindow";
    pub const PRIMARY_INCIDENT: &str = "PrimaryIncident";
    pub const PARTICIPANT_IMPACT: &str = "ParticipantImpact";
    pub const ALARM_OR_TELEMETRY: &str = "AlarmOrTelemetry";
    pub const ROLLBACK_OR_RECOVERY: &str = "RollbackOrRecovery";
    pub const OPERATIONAL_TASK: &str = "OperationalTask";
    pub const OTHER: &str = "Other";

    pub const ALL: &[&str] = &[
        CHANGE_WINDOW,
        PRIMARY_INCIDENT,
        PARTICIPANT_IMPACT,
        ALARM_OR_TELEMETRY,
        ROLLBACK_OR_RECOVERY,
        OPERATIONAL_TASK,
        OTHER,
    ];
}

/// Analysis applicability of a ticket to public-BGP analysis (reviewed).
///
/// The RELATIONSHIP named by the ticket decides applicability, not the
/// mere existence of an ASN for the named organization: an optical
/// participant interface, Layer-2 circuit, exchange fabric, alarm, or
/// telemetry condition is not directly observable in public BGP even
/// when the organization owns an ASN.
pub mod applicability {
    pub const POTENTIALLY_VISIBLE: &str = "PotentiallyVisibleInPublicBgp";
    pub const NOT_APPLICABLE: &str = "NotApplicableToPublicBgp";
    /// The reviewed relationship exists but is not directly observable
    /// in public BGP (e.g. optical participant interface, Layer-2
    /// circuit, exchange fabric, alarm/telemetry). A contemporaneous
    /// BGP run may be retained as supporting context but never becomes
    /// primary evidence for the named relationship.
    pub const NOT_DIRECTLY_OBSERVABLE: &str = "NotDirectlyObservableInPublicBgp";
    /// No reviewed origin attribution exists (test equipment, exchange
    /// fabric, entities without a public origin ASN).
    pub const NOT_ORIGIN_ATTRIBUTABLE: &str = "NotOriginAttributable";
    pub const TARGET_NOT_YET_MAPPED: &str = "ApplicableTargetNotYetMapped";
}

/// One per-field provenance citation for a reviewed interpretation.
///
/// `source` is either `SnapshotField:<field>` (the value is directly
/// present in the source snapshot) or a citation to a reference document
/// (`source_document_id` required, e.g. the AAR). A review may never
/// backfill a missing source field without a document citation.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ReviewProvenance {
    /// Which interpretation field this entry supports
    /// (roles | entity_labels | linked_change_ids | applicability |
    /// relationship_to_case_study | window | task_type).
    pub field: String,
    /// `SnapshotField:<field>` or a document citation string.
    pub source: String,
    /// Exact wording or document section that supports the value.
    pub detail: String,
    /// Required when `source` is a reference-document citation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_document_id: Option<i64>,
}

/// Analyst-reviewed interpretation of one catalog ticket.
///
/// Stored separately from `event_snapshots`; importing or updating a
/// review never modifies source snapshot content.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TicketReview {
    pub id: i64,
    pub catalog_event_id: i64,
    pub external_id: String,
    /// Reviewed case-study roles (vocabulary `reviewed_role::ALL`).
    pub reviewed_roles: Vec<String>,
    /// Affected entity / asset labels (reviewed).
    pub entity_labels: Vec<String>,
    /// Maintenance/change identifiers the ticket is reviewed as linked to.
    pub linked_change_ids: Vec<String>,
    pub analysis_applicability: String,
    pub applicability_rationale: String,
    /// Relationship to the case study (case-study vocabulary).
    pub relationship_to_case_study: String,
    pub review_status: String,
    pub reviewer: String,
    pub reviewed_at: String,
    /// Per-field provenance; every non-empty field must be covered.
    pub provenance: Vec<ReviewProvenance>,
    /// Reference document cited by provenance entries (e.g. the AAR).
    pub source_document_id: Option<i64>,
}
