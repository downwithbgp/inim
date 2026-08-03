//! inim — local event-conditioned BGP analysis system and NOC incident workbench.
//!
//! CLI entry point. Parses commands and orchestrates analysis.
//!
//! Process exit status contract:
//!   EXIT_SUCCESS           plan produced / analysis completed (0)
//!   EXIT_INVALID_INPUT     malformed ticket or manifest (1)
//!   EXIT_ANALYSIS_INCOMPLETE  infrastructure failure during analysis (2)
//!   EXIT_ANALYSIS_BLOCKED  plan produced but blocked; no analysis ran (3)

use clap::{Parser, Subcommand};
use std::io::Write;
use std::path::PathBuf;

use inim::assess;
use inim::fixtures;
use inim::report;
use inim::routes;
use inim::sources::internet2::ticket as i2ticket;
use inim::tokenize;
use inim::waves;

pub const EXIT_SUCCESS: i32 = 0;
pub const EXIT_INVALID_INPUT: i32 = 1;
pub const EXIT_ANALYSIS_INCOMPLETE: i32 = 2;
pub const EXIT_ANALYSIS_BLOCKED: i32 = 3;
/// Queue conflict: an active job for the exact plan revision already
/// exists; the caller may follow the existing job.
pub const EXIT_QUEUE_CONFLICT: i32 = 4;
/// Worker failure: execution or publication failed.
pub const EXIT_WORKER_FAILURE: i32 = 5;

/// Local event-conditioned BGP analysis: relate operator-declared network events to
/// the globally visible routing system.
#[derive(Parser)]
#[command(name = "inim")]
#[command(version = "0.1.0")]
#[command(about = "Local event-conditioned BGP analysis and NOC incident workbench", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Produce an analysis plan from a reviewed manifest.
    ///
    /// Runs before any Broker query, archive download, cache lookup, or MRT
    /// parsing. Prints "Ready" or "Blocked" and the plan JSON.
    ///
    /// Exit codes: 0 when the manifest was parsed and a plan was produced
    /// (even if the plan is Blocked); 1 for malformed input.
    Plan {
        /// Path to the event fixture JSON file (required).
        #[arg(short, long, value_name = "PATH")]
        event: PathBuf,

        /// Path to the reviewed event manifest JSON file.
        #[arg(short = 'm', long, value_name = "PATH")]
        manifest: PathBuf,

        /// Output directory for plan artifacts (analysis_plan.json/.txt,
        /// limitations.json). When absent, only stdout is written.
        #[arg(short = 'o', long, value_name = "DIR")]
        out: Option<PathBuf>,
    },
    /// Migrate a legacy manifest (schema v1) to the canonical schema.
    ///
    /// Converts `managed_network_asn` / `internet2_asn` to a
    /// `TransitPredicateMapping`. A Reviewed predicate requires
    /// analyst-confirmed provenance. Never executes analysis.
    MigrateManifest {
        /// Path to the legacy manifest JSON file.
        #[arg(long, value_name = "PATH")]
        input: PathBuf,

        /// Output path for the migrated manifest.
        #[arg(long, value_name = "PATH")]
        output: PathBuf,

        /// Provenance statement confirming the transit predicate review.
        #[arg(long, value_name = "TEXT")]
        statement: Option<String>,

        /// Reviewer identity for the provenance record.
        #[arg(long, value_name = "NAME")]
        reviewed_by: Option<String>,

        /// Review date (ISO-8601) for the provenance record.
        #[arg(long, value_name = "DATE")]
        date: Option<String>,
    },
    /// Compare two completed event analyses.
    ///
    /// Reads each event's report.json (current schema) and writes
    /// comparison.json + comparison.txt. Observer-scoped, no severity score.
    Compare {
        /// First event output directory (contains report.json).
        #[arg(long, value_name = "DIR")]
        a: PathBuf,

        /// Second event output directory (contains report.json).
        #[arg(long, value_name = "DIR")]
        b: PathBuf,

        /// Optional directory containing a blocked analysis_plan.json to
        /// include as a planning-status entry (never as an observed event).
        #[arg(long, value_name = "DIR")]
        blocked: Option<PathBuf>,

        /// Output directory for the comparison artifacts.
        #[arg(short = 'o', long, value_name = "DIR")]
        out: PathBuf,
    },
    /// Catalog administration: initialize, import, and synchronize the
    /// local event catalog.
    #[command(subcommand)]
    Catalog(CatalogCommands),
    /// Serve the localhost catalog web UI (read-only by default).
    ///
    /// Mutates the catalog only when --enable-writes is set. Write mode
    /// is unauthenticated and intended for trusted local use; it never
    /// executes analysis (a separate `inim worker` process executes
    /// queued jobs).
    Serve {
        /// Catalog database path.
        #[arg(long, value_name = "PATH")]
        db: PathBuf,

        /// Catalog root directory (artifact paths are relative to it).
        #[arg(long, value_name = "DIR", default_value = ".")]
        root: PathBuf,

        /// Bind address. Default is loopback only.
        #[arg(long, value_name = "ADDR", default_value = "127.0.0.1:8080")]
        bind: String,

        /// Explicitly allow a non-loopback bind (no authentication).
        #[arg(long)]
        allow_non_loopback: bool,

        /// Enable local catalog mutations (queue/cancel/retry/plan
        /// edits) with CSRF protection. No analysis runs in this
        /// process.
        #[arg(long)]
        enable_writes: bool,

        /// Extra explicit acknowledgement required for write mode on a
        /// non-loopback bind (write mode is unauthenticated).
        #[arg(long)]
        allow_unauthenticated_writes: bool,
    },
    /// Analyze a single operational event against BGP observations.
    ///
    /// Without --manifest: runs a built-in synthetic demonstration.
    /// With --manifest: plans first, then executes real analysis using
    /// discovered RouteViews data when the plan is Ready. A Blocked plan
    /// performs no Broker or MRT work and exits with EXIT_ANALYSIS_BLOCKED.
    Analyze {
        /// Path to the event fixture JSON file (required).
        #[arg(short, long, value_name = "PATH")]
        event: PathBuf,

        /// Path to the reviewed event manifest JSON file.
        /// When present, triggers real-analysis path (broker discovery + cache).
        #[arg(short = 'm', long, value_name = "PATH")]
        manifest: Option<PathBuf>,

        /// Local cache directory for downloaded archive files.
        /// Default: ./cache
        #[arg(short = 'c', long, value_name = "DIR", default_value = "./cache")]
        cache: PathBuf,

        /// Output directory for reports and evidence.
        /// Default: ./out
        #[arg(short = 'o', long, value_name = "DIR", default_value = "./out")]
        out: PathBuf,

        /// Disable all derived caches (both RIB and UPDATE).
        #[arg(long, default_value_t = false)]
        no_derived_cache: bool,

        /// Stage A: stop after Broker discovery + RIB preflight (no UPDATE
        /// acquisition, no analysis). Prints preflight JSON to stdout.
        #[arg(long, default_value_t = false)]
        preflight_only: bool,

        /// Origin-only inventory: classify all origin-matching baseline
        /// routes at each collector against the manifest's named path
        /// classifiers (one/both/neither). No cohort verdict. Reuses the
        /// source extraction cache; the RIB is parsed once if needed.
        #[arg(long, default_value_t = false)]
        origin_inventory: bool,

        /// Reviewed network profile (network-profile.json) for inventory
        /// plane display labels and ASN roles.
        #[arg(long, value_name = "PATH")]
        profile: Option<PathBuf>,

        /// Explicit parser-worker count (0 = follow --jobs).
        #[arg(long, default_value_t = 0)]
        parse_jobs: usize,

        /// Network download concurrency (conservative; default 2).
        #[arg(long, default_value_t = 2)]
        download_jobs: usize,

        /// Print the effective execution plan (host topology + worker
        /// counts) and exit without acquiring anything.
        #[arg(long, default_value_t = false)]
        show_execution_plan: bool,

        /// Rebuild only UPDATE derived caches (keeps the RIB derived cache;
        /// useful for parser-scaling benchmarks).
        #[arg(long, default_value_t = false)]
        rebuild_update_caches: bool,

        /// Force rebuild of all derived caches (ignore and overwrite).
        #[arg(long, default_value_t = false)]
        rebuild_derived_cache: bool,

        /// Number of parallel parsing jobs (1=serial; default from the
        /// local raw-cache benchmark; 0 rejected — use --parse-jobs).
        #[arg(short = 'j', long, default_value_t = 8)]
        jobs: usize,
    },
    /// Inspect an analysis plan revision from the catalog.
    ///
    /// Read-only: never mutates the catalog and never accesses the
    /// network. Shows the exact plan revision, its reviewed/derived
    /// split, blocker reasons, and the canonical plan hash.
    AnalysisPlan {
        #[command(subcommand)]
        command: AnalysisPlanCommands,
    },
    /// Administer durable analysis jobs (queue/cancel/retry).
    ///
    /// Queueing mutates the local catalog but never executes analysis
    /// and never accesses the network; execution happens in `inim
    /// worker`.
    #[command(subcommand)]
    AnalysisJob(AnalysisJobCommands),
    /// Execute queued analyses from the durable job queue.
    ///
    /// Mutates the catalog (claim/progress/publication) and may access
    /// configured public archive sources (RouteViews, RIPE RIS) unless
    /// --offline is set. Run as a separate process from `inim serve`.
    Worker {
        /// Catalog database path.
        #[arg(long, value_name = "PATH")]
        db: PathBuf,
        /// Catalog root directory (job staging/published runs live
        /// under data/jobs and data/runs relative to it).
        #[arg(long, value_name = "DIR", default_value = ".")]
        root: PathBuf,
        /// Stable worker id; a random process-lifetime id is generated
        /// when absent. Never a bare hostname.
        #[arg(long, value_name = "ID")]
        worker_id: Option<String>,
        /// Poll interval when the queue is empty.
        #[arg(long, value_name = "DURATION", default_value = "2s")]
        poll_interval: String,
        /// Maximum concurrent jobs (default 1; >1 requires bounded
        /// parse budget).
        #[arg(long, default_value_t = 1)]
        max_jobs: usize,
        /// Network download concurrency.
        #[arg(long, default_value_t = 2)]
        download_jobs: usize,
        /// Parser-worker count.
        #[arg(long, default_value_t = 8)]
        parse_jobs: usize,
        /// Claim and execute at most one job, then exit.
        #[arg(long)]
        once: bool,
        /// Reject any job requiring uncached network acquisition.
        #[arg(long)]
        offline: bool,
        /// Print the effective execution plan and exit without running.
        #[arg(long)]
        show_execution_plan: bool,
        /// Keep the staging directory of failed/cancelled jobs.
        #[arg(long)]
        keep_failed_workdir: bool,
    },
    /// Build or verify a deterministic offline demo catalog.
    #[command(subcommand)]
    Demo(DemoCommands),
    /// Read-only project-scope administration.
    ///
    /// The tracked policy file (config/project-scope.toml) is the
    /// reviewed authority. These commands never modify the policy and
    /// never delete catalog records.
    #[command(subcommand)]
    ProjectScope(ProjectScopeCommands),
}

/// Project-scope administration subcommands (read-only).
#[derive(Subcommand)]
enum ProjectScopeCommands {
    /// Show the reviewed project-scope policy (read-only).
    Show {
        /// Repository root containing config/project-scope.toml.
        #[arg(long, value_name = "DIR", default_value = ".")]
        root: PathBuf,
    },
    /// Audit the catalog against the policy (read-only; never deletes).
    Audit {
        /// Catalog database path.
        #[arg(long, value_name = "PATH")]
        db: PathBuf,
        /// Repository root containing config/project-scope.toml.
        #[arg(long, value_name = "DIR", default_value = ".")]
        root: PathBuf,
    },
}

/// Analysis-plan inspection subcommands.
#[derive(Subcommand)]
enum AnalysisPlanCommands {
    /// Show the latest plan revision for an event (read-only).
    Show {
        /// Catalog database path.
        #[arg(long, value_name = "PATH")]
        db: PathBuf,
        /// Event id (e.g. INC0302574).
        #[arg(long, value_name = "ID")]
        event: String,
        /// Emit JSON instead of text.
        #[arg(long)]
        json: bool,
    },
}

/// Analysis-job administration subcommands.
#[derive(Subcommand)]
enum AnalysisJobCommands {
    /// Queue an exact plan revision (idempotent; no execution).
    Queue {
        /// Catalog database path.
        #[arg(long, value_name = "PATH")]
        db: PathBuf,
        /// Exact plan revision id (from `inim analysis-plan show`).
        #[arg(long, value_name = "ID")]
        plan: i64,
    },
    /// List jobs (execution state).
    List {
        /// Catalog database path.
        #[arg(long, value_name = "PATH")]
        db: PathBuf,
        /// Optional state filter (e.g. Queued, Failed, Completed).
        #[arg(long, value_name = "STATE")]
        state: Option<String>,
    },
    /// Show one job with its recent events.
    Show {
        /// Catalog database path.
        #[arg(long, value_name = "PATH")]
        db: PathBuf,
        /// Job id.
        #[arg(long, value_name = "ID")]
        job: String,
    },
    /// Request cancellation (cooperative; queued jobs cancel directly).
    Cancel {
        /// Catalog database path.
        #[arg(long, value_name = "PATH")]
        db: PathBuf,
        /// Job id.
        #[arg(long, value_name = "ID")]
        job: String,
    },
    /// Create a new attempt for a Failed or Cancelled job.
    Retry {
        /// Catalog database path.
        #[arg(long, value_name = "PATH")]
        db: PathBuf,
        /// Job id.
        #[arg(long, value_name = "ID")]
        job: String,
    },
    /// Report stale/expired leases and orphaned artifacts (read-only).
    Audit {
        /// Catalog database path.
        #[arg(long, value_name = "PATH")]
        db: PathBuf,
        /// Catalog root directory.
        #[arg(long, value_name = "DIR", default_value = ".")]
        root: PathBuf,
    },
    /// Clean terminal-job staging directories (DRY-RUN by default).
    ///
    /// Deletes only Failed/Cancelled/unreferenced job staging older
    /// than --older-than, with path containment and a terminal-state
    /// re-check. Never deletes runs, referenced artifacts, caches, or
    /// tracked evidence. Pass --apply to actually delete; the default
    /// only reports.
    Cleanup {
        /// Catalog database path.
        #[arg(long, value_name = "PATH")]
        db: PathBuf,
        /// Catalog root directory.
        #[arg(long, value_name = "DIR", default_value = ".")]
        root: PathBuf,
        /// Minimum age of eligible staging (e.g. 7d, 48h, 90m).
        #[arg(long, value_name = "AGE", default_value = "7d")]
        older_than: String,
        /// Actually delete eligible directories (default is dry-run).
        #[arg(long)]
        apply: bool,
    },
}

/// Demo catalog subcommands (offline, deterministic).
#[derive(Subcommand)]
enum DemoCommands {
    /// Build a demo catalog from tracked reviewed material (offline).
    ///
    /// Imports the reviewed case-study metadata and current artifacts
    /// into a fresh SQLite database. Never accesses the network, never
    /// modifies tracked files, and refuses to overwrite an existing
    /// database unless --force is given.
    Init {
        /// Output database path (must not exist unless --force).
        #[arg(long, value_name = "PATH")]
        db: PathBuf,
        /// Repository root containing manifests/ and case-studies/.
        #[arg(long, value_name = "DIR", default_value = ".")]
        root: PathBuf,
        /// Overwrite an existing database at --db.
        #[arg(long)]
        force: bool,
    },
    /// Verify a demo catalog: expected events, workbenches, artifact
    /// references, no source access, no absolute-path leaks.
    Verify {
        /// Demo database path.
        #[arg(long, value_name = "PATH")]
        db: PathBuf,
        /// Repository root containing manifests/ and case-studies/.
        #[arg(long, value_name = "DIR", default_value = ".")]
        root: PathBuf,
    },
}

/// Catalog administration subcommands.
#[derive(Subcommand)]
enum CatalogCommands {
    /// Initialize a new catalog database (applies all migrations).
    Init {
        #[arg(long, value_name = "PATH")]
        db: PathBuf,
    },
    /// Import canonical manifests and analysis artifacts into the catalog.
    Import {
        #[arg(long, value_name = "PATH")]
        db: PathBuf,
        /// Repository root containing manifests/ and analysis artifacts
        /// (out/ for locally generated runs; case-studies/*/out for
        /// reviewed event evidence).
        #[arg(long, value_name = "DIR", default_value = ".")]
        root: PathBuf,
    },
    /// Render the incident workbench as a text report (same shared
    /// presentation model as the web workbench and the JSON API).
    Workbench {
        #[arg(long, value_name = "PATH")]
        db: PathBuf,
        /// Event id (e.g. INC0302574) or case-study slug (e.g. manlan-2019).
        #[arg(long, value_name = "ID")]
        subject: String,
    },
    /// Write the exact finding-audit record for a subject
    ///  The prose renderer uses these exact
    /// fields; output is written to --out or stdout.
    FindingAudit {
        #[arg(long, value_name = "PATH")]
        db: PathBuf,
        /// Event id (e.g. INC0302574) or case-study slug (e.g. manlan-2019).
        #[arg(long, value_name = "ID")]
        subject: String,
        #[arg(long, value_name = "PATH")]
        out: Option<PathBuf>,
    },
    /// Write the checked per-prefix chronology audit for an event
    ///  the exact ordered transition sequence
    /// with evidence ids and archive identities, read from the
    /// canonical lifecycle artifact. Output to --out or stdout.
    FindingChronologyAudit {
        /// Event id (e.g. INC0299001); the run directory is located
        /// under case-studies/<event>/out/<event>/.
        #[arg(long, value_name = "ID")]
        event: String,
        /// Observer session filter: collector (default route-views2).
        #[arg(long, default_value = "route-views2")]
        collector: String,
        /// Observer session filter: peer IP (default 163.253.3.14).
        #[arg(long, default_value = "163.253.3.14")]
        peer: String,
        #[arg(long, value_name = "PATH")]
        out: Option<PathBuf>,
        /// Run directory override (default case-studies/<event>/out/<event>).
        #[arg(long, value_name = "DIR")]
        run_dir: Option<PathBuf>,
    },
    /// Manage incident case studies.
    #[command(subcommand)]
    CaseStudy(CaseStudyCommands),
    /// Manage reference documents.
    #[command(subcommand)]
    Document(DocumentCommands),
    /// Synchronize a catalog source into the catalog.
    #[command(subcommand)]
    Sync(SyncSource),
    /// Rebuild the ticket relationship graph from fetched snapshots.
    #[command(subcommand)]
    Relationships(RelationshipsCommands),
    /// Show the corpus-level BGP-analysis readiness queue.
    AnalysisQueue {
        #[arg(long, value_name = "PATH")]
        db: PathBuf,
        /// Filter by readiness state (e.g. NotReviewed, AnalysisComplete).
        #[arg(long, value_name = "STATE")]
        state: Option<String>,
    },
    /// Plan shared raw-archive batches across event cohorts.
    #[command(subcommand)]
    ArchiveBatches(ArchiveBatchesCommands),
    /// Export corpus metadata only (no raw payloads) as JSON.
    CorpusExport {
        #[arg(long, value_name = "PATH")]
        db: PathBuf,
        /// Output path; defaults to stdout.
        #[arg(long, value_name = "PATH")]
        out: Option<PathBuf>,
    },
    /// Import a reviewed-interpretation file (ticket reviews + reviewed
    /// relationship edges) into the corpus. Reviewed data is stored
    /// separately from source snapshots; snapshots are never modified.
    CorpusReview {
        #[arg(long, value_name = "PATH")]
        db: PathBuf,
        /// Reviewed-interpretation file (ticket-reviews.json).
        #[arg(value_name = "FILE")]
        file: PathBuf,
        /// Source kind the reviews apply to.
        #[arg(long, value_name = "KIND", default_value = "grnoc-public-task-viewer")]
        source_kind: String,
    },
    /// Audit historical collector sessions from baseline RIB peer metadata.
    SessionAudit {
        #[arg(
            long,
            value_name = "DIR",
            default_value = "case-studies/manlan-2019/pilot"
        )]
        root: PathBuf,
        /// Reviewed network profile (network-profile.json).
        #[arg(long, value_name = "PATH")]
        profile: Option<PathBuf>,
        /// Reviewed collector locations (collector-locations.json).
        #[arg(long, value_name = "PATH")]
        locations: Option<PathBuf>,
        /// Cache directory + source family, repeatable: --cache DIR:family.
        #[arg(long, value_name = "DIR:FAMILY", required = true)]
        cache: Vec<String>,
        /// Filename date filter for baseline RIBs (e.g. 20190821).
        #[arg(long, value_name = "DATE", default_value = "20190821")]
        date: String,
        /// Target origin ASNs (comma-separated).
        #[arg(long, value_name = "ASNS", default_value = "2603")]
        origin_asns: String,
        /// Shared cache root for the extraction cache (default: first --cache dir).
        #[arg(long, value_name = "DIR")]
        extraction_cache: Option<PathBuf>,
        /// Parallel parse workers.
        #[arg(long, value_name = "N", default_value = "4")]
        jobs: usize,
        /// Full peer inventory: report EVERY session in the baseline RIBs
        /// (all peers, all route counts) instead of only target-origin
        /// sessions. Answer "was a direct session with peer ASN X present
        /// at all" even when that session carried no target-origin routes.
        #[arg(long)]
        full_inventory: bool,
        /// Output JSON path.
        #[arg(long, value_name = "PATH", default_value = "session-audit.json")]
        out: PathBuf,
    },
    /// Backfill OBSERVED peer-session metadata from cached baseline RIBs.
    ///
    /// Runs a full peer inventory over the given cache directories for
    /// the date and records each session's peer ASN (an observed
    /// protocol fact) into observer_session_metadata, time-scoped by the
    /// RIB timestamp. Idempotent: re-running produces identical rows.
    SessionMetadataBackfill {
        #[arg(long, value_name = "PATH")]
        db: PathBuf,
        /// Cache directory + source family, repeatable: --cache DIR:family.
        #[arg(long, value_name = "DIR:FAMILY", required = true)]
        cache: Vec<String>,
        /// Filename date filter for baseline RIBs (e.g. 20260714).
        #[arg(long, value_name = "DATE", required = true)]
        date: String,
    },
}

/// Ticket-relationship administration.
#[derive(Subcommand)]
enum RelationshipsCommands {
    /// Re-extract explicit relationships from all fetched snapshots,
    /// resolve unresolved targets, derive temporal-overlap candidates,
    /// and regenerate incident group candidates. Idempotent.
    Rebuild {
        #[arg(long, value_name = "PATH")]
        db: PathBuf,
    },
    /// Print the reviewed relationship-graph audit: source node,
    /// destination or unresolved reference, relationship, evidence kind,
    /// exact source, review status. Read-only.
    Audit {
        #[arg(long, value_name = "PATH")]
        db: PathBuf,
        /// Source kind to audit (default: grnoc-public-task-viewer).
        #[arg(long, value_name = "KIND", default_value = "grnoc-public-task-viewer")]
        source_kind: String,
    },
}

/// Shared archive-batch planning.
#[derive(Subcommand)]
enum ArchiveBatchesCommands {
    /// Build a deterministic correlation batch per case study from its
    /// stored archive plans. Pure computation; downloads nothing.
    Plan {
        #[arg(long, value_name = "PATH")]
        db: PathBuf,
    },
}

/// Incident case-study subcommands.
#[derive(Subcommand)]
enum CaseStudyCommands {
    /// Import a reviewed case-study data file (case-study.json).
    ///
    /// Transactional, idempotent (slug + content hash), schema-validated;
    /// links existing catalog events and preserves unresolved ticket
    /// references without fabricating source snapshots.
    Import {
        #[arg(long, value_name = "PATH")]
        db: PathBuf,
        /// Case-study directory containing case-study.json, or the file itself.
        #[arg(long, value_name = "PATH")]
        path: PathBuf,
    },
    /// Link an existing analysis run to a case study.
    ///
    /// Uses the existing case_study_analysis_links association; the run and
    /// its evidence stay owned by the run.
    LinkRun {
        #[arg(long, value_name = "PATH")]
        db: PathBuf,
        /// Case-study slug.
        #[arg(long, value_name = "SLUG")]
        slug: String,
        /// Analysis run id.
        #[arg(long, value_name = "ID")]
        run: i64,
        /// Role of the run for this case study (default PilotObservation).
        #[arg(long, value_name = "ROLE", default_value = "PilotObservation")]
        role: String,
        /// Reviewed note.
        #[arg(long, value_name = "TEXT")]
        note: Option<String>,
    },
    /// Apply a reviewed pilot-result record to the case study's plan.
    PilotResult {
        #[arg(long, value_name = "PATH")]
        db: PathBuf,
        /// Reviewed pilot-result record (pilot-result.json).
        #[arg(long, value_name = "PATH")]
        path: PathBuf,
    },
    /// Apply a reviewed target-research record to a case study.
    ///
    /// Updates ONLY the research fields of matching target rows (mapped
    /// ASNs, predicate status, notes, provenance, audit timestamp); the
    /// case-study content revision is never touched.
    ApplyResearch {
        #[arg(long, value_name = "PATH")]
        db: PathBuf,
        /// Reviewed target-research record (target-research.json).
        #[arg(long, value_name = "PATH")]
        path: PathBuf,
    },
    /// Build the historical-archive plan for a case study.
    ///
    /// Computes the reproducible horizon and expected archive files WITHOUT
    /// downloading anything. Targets with unresolved historical mappings are
    /// reported as blocked. The plan is stored as Draft until reviewed.
    Plan {
        #[arg(long, value_name = "PATH")]
        db: PathBuf,
        /// Case-study slug.
        #[arg(long, value_name = "SLUG")]
        slug: String,
        /// Warmup hours before the incident window (default 2).
        #[arg(long, value_name = "HOURS", default_value_t = 2)]
        warmup_hours: i64,
        /// Cooldown hours after the incident window (default 2).
        #[arg(long, value_name = "HOURS", default_value_t = 2)]
        cooldown_hours: i64,
        /// Source family to plan: RouteViews (default) or RipeRis.
        #[arg(long, value_name = "FAMILY", default_value = "RouteViews")]
        family: String,
    },
}

/// Reference-document subcommands.
#[derive(Subcommand)]
enum DocumentCommands {
    /// Import an external reference document into the catalog.
    ///
    /// The file is copied to `<root>/data/documents/<sha12>/<basename>` and
    /// the catalog records the catalog-relative path, SHA-256, media type,
    /// and best-effort PDF metadata.
    Import {
        #[arg(long, value_name = "PATH")]
        db: PathBuf,
        /// Document file to import (basename is used for storage).
        #[arg(long, value_name = "FILE")]
        file: PathBuf,
        /// Source URL of the document.
        #[arg(long, value_name = "URL")]
        source_url: String,
        /// Reviewed document title (defaults to the file name).
        #[arg(long, value_name = "TITLE")]
        title: Option<String>,
        /// Document type (e.g. AfterActionReport).
        #[arg(long, value_name = "TYPE")]
        doc_type: Option<String>,
        /// Provenance note for the document record.
        #[arg(long, value_name = "TEXT")]
        provenance: Option<String>,
        /// Catalog root (default: current directory).
        #[arg(long, value_name = "DIR", default_value = ".")]
        root: PathBuf,
    },
}

/// Catalog source adapters.
#[derive(Subcommand)]
enum SyncSource {
    /// GRNOC Public Task Viewer records.
    ///
    /// Offline mode: `--source-dir DIR` syncs local JSON record files
    /// (one `GnocRecord` per file).
    ///
    /// Live mode (no --source-dir): politely fetches exact ticket
    /// numbers from the public task viewer. Requires at least one
    /// discovery source (--seed, --case-study, or --expand-references);
    /// there is no "download everything" mode.
    Grnoc {
        #[arg(long, value_name = "PATH")]
        db: PathBuf,
        /// Offline mode: directory containing GRNOC JSON records.
        #[arg(long, value_name = "DIR")]
        source_dir: Option<PathBuf>,
        /// Live mode: exact ticket id to seed (repeatable).
        #[arg(long, value_name = "ID")]
        seed: Vec<String>,
        /// Live mode: seed tickets from a reviewed case study (repeatable).
        #[arg(long, value_name = "SLUG")]
        case_study: Vec<String>,
        /// Live mode: expand the frontier from fetched public descriptions.
        #[arg(long)]
        expand_references: bool,
        /// Live mode: bounded scoped search (repeatable). Searches the
        /// documented viewer search mechanism with a non-empty reviewed
        /// query; incident search requires --domain. Never issues
        /// empty or unscoped queries.
        #[arg(long, value_name = "QUERY")]
        search: Vec<String>,
        /// Domain id (sys_id) from `get_domains` used to scope incident
        /// searches (unscoped incident search returns 403).
        #[arg(long, value_name = "ID")]
        domain: Option<String>,
        /// Request budget for this sync (default 100; never unbounded).
        #[arg(long, value_name = "N")]
        max_requests: Option<usize>,
        /// Sustained request rate (reviewed default 5.0; values above
        /// 5.0 require --allow-higher-rate).
        #[arg(long, value_name = "RPS")]
        requests_per_second: Option<f64>,
        /// Explicitly allow a substantially higher request rate.
        #[arg(long)]
        allow_higher_rate: bool,
        /// Contact (email or URL) placed in the User-Agent. Never invented.
        #[arg(long, value_name = "CONTACT")]
        contact: Option<String>,
        /// Show the request frontier and budget; no network access.
        #[arg(long)]
        dry_run: bool,
        /// Print the self-imposed access policy and exit; no network.
        #[arg(long)]
        show_access_policy: bool,
        /// Fetch and print the viewer's public network/domain list
        /// (one polite request), then exit.
        #[arg(long)]
        show_domains: bool,
    },
}

fn main() {
    let cli = Cli::parse();
    let code = run(&cli);
    std::process::exit(code);
}

/// Dispatch a parsed CLI command and return the process exit code.
fn run(cli: &Cli) -> i32 {
    match &cli.command {
        Commands::Plan {
            event,
            manifest,
            out,
        } => cmd_plan(&mut std::io::stdout(), event, manifest, out.as_deref()),
        Commands::MigrateManifest {
            input,
            output,
            statement,
            reviewed_by,
            date,
        } => cmd_migrate_manifest(
            &mut std::io::stdout(),
            input,
            output,
            statement.as_deref(),
            reviewed_by.as_deref(),
            date.as_deref(),
        ),
        Commands::Compare { a, b, blocked, out } => {
            cmd_compare(&mut std::io::stdout(), a, b, blocked.as_deref(), out)
        }
        Commands::Catalog(command) => cmd_catalog(&mut std::io::stdout(), command),
        Commands::AnalysisPlan { command } => cmd_analysis_plan(&mut std::io::stdout(), command),
        Commands::AnalysisJob(command) => cmd_analysis_job(&mut std::io::stdout(), command),
        Commands::Worker {
            db,
            root,
            worker_id,
            poll_interval,
            max_jobs,
            download_jobs,
            parse_jobs,
            once,
            offline,
            show_execution_plan,
            keep_failed_workdir,
        } => cmd_worker(
            db,
            root,
            worker_id.as_deref(),
            poll_interval,
            *max_jobs,
            *download_jobs,
            *parse_jobs,
            *once,
            *offline,
            *show_execution_plan,
            *keep_failed_workdir,
        ),
        Commands::Demo(command) => cmd_demo(&mut std::io::stdout(), command),
        Commands::ProjectScope(command) => cmd_project_scope(&mut std::io::stdout(), command),
        Commands::Serve {
            db,
            root,
            bind,
            allow_non_loopback,
            enable_writes,
            allow_unauthenticated_writes,
        } => cmd_serve(
            &mut std::io::stdout(),
            db,
            root,
            bind,
            *allow_non_loopback,
            *enable_writes,
            *allow_unauthenticated_writes,
        ),
        Commands::Analyze {
            event,
            manifest,
            cache,
            out,
            no_derived_cache,
            rebuild_derived_cache,
            jobs,
            preflight_only,
            origin_inventory,
            profile,
            parse_jobs,
            download_jobs,
            show_execution_plan,
            rebuild_update_caches,
        } => {
            if let Err(e) = validate_jobs(*jobs) {
                let _ = writeln!(std::io::stderr(), "error: {e}");
                return EXIT_INVALID_INPUT;
            }
            if *origin_inventory {
                return cmd_origin_inventory(
                    &mut std::io::stdout(),
                    &mut std::io::stderr(),
                    manifest.as_ref().map(|p| p.as_path()),
                    profile.as_deref(),
                    cache,
                    out,
                    *jobs,
                );
            }
            if *show_execution_plan {
                let info = inim::perf::host_info(*jobs, *parse_jobs, *download_jobs);
                let effective_parse = if *parse_jobs > 0 { *parse_jobs } else { *jobs };
                let _ = writeln!(
                    std::io::stdout(),
                    "execution plan:\n  host logical CPUs: {}\n  available_parallelism: {}\n  --jobs: {}\n  effective parser workers: {}\n  download workers: {}\n  cgroup cpu.max: {}\n  affinity: {}",
                    info.logical_cpus,
                    info.available_parallelism,
                    info.jobs,
                    effective_parse,
                    info.download_jobs,
                    info.cgroup_cpu_max.as_deref().unwrap_or("unlimited"),
                    info.cpu_affinity.as_deref().unwrap_or("unrestricted"),
                );
                return EXIT_SUCCESS;
            }
            let discovery = inim::discover::LiveArchiveDiscovery;
            let cache_control = inim::orchestrate::CacheControl {
                no_derived_cache: *no_derived_cache,
                rebuild_derived_cache: *rebuild_derived_cache,
                rebuild_update_caches: *rebuild_update_caches,
                jobs: *jobs,
                parse_jobs: *parse_jobs,
                download_jobs: *download_jobs,
                offline: false,
            };
            cmd_analyze(
                &mut std::io::stdout(),
                &mut std::io::stderr(),
                event,
                manifest.as_ref().map(|p| p.as_path()),
                cache,
                out,
                &discovery,
                cache_control,
                *preflight_only,
            )
        }
    }
}

/// Validate the --jobs value: 0 was previously "auto" and is now rejected
/// (use --parse-jobs for explicit parse concurrency).
fn validate_jobs(jobs: usize) -> Result<(), String> {
    if jobs == 0 {
        Err("--jobs 0 is no longer accepted; use --parse-jobs N (or omit both for the measured default)".to_string())
    } else {
        Ok(())
    }
}

/// `inim plan`: parse ticket + manifest, produce the plan, print it.
///
/// Exits 0 whenever the manifest parsed and a plan was produced — even for
/// Blocked plans. Exits EXIT_INVALID_INPUT for malformed input.
fn cmd_plan(
    stdout: &mut dyn Write,
    event_path: &std::path::Path,
    manifest_path: &std::path::Path,
    out_dir: Option<&std::path::Path>,
) -> i32 {
    let (event_id, expectation) =
        match inim::sources::derive_expectation_from_fixture(event_path.to_string_lossy().as_ref())
        {
            Ok(pair) => pair,
            Err(e) => {
                let _ = writeln!(stdout, "error: failed to parse ticket fixture: {e}");
                return EXIT_INVALID_INPUT;
            }
        };
    let manifest = match inim::manifest::Manifest::load(manifest_path) {
        Ok(m) => m,
        Err(e) => {
            let _ = writeln!(stdout, "error: {e}");
            return EXIT_INVALID_INPUT;
        }
    };
    let plan = match inim::plan::plan_from_manifest(&event_id.0, expectation, &manifest) {
        Ok(p) => p,
        Err(e) => {
            let _ = writeln!(stdout, "error: {e}");
            return EXIT_INVALID_INPUT;
        }
    };

    let artifact = inim::plan::PlanArtifact::from_plan(&plan);
    let _ = writeln!(stdout, "{}", artifact.status_line());
    let json = serde_json::to_string_pretty(&artifact).unwrap_or_default();
    let _ = writeln!(stdout, "{json}");

    // Write plan artifacts when an output directory is provided.
    if let Some(dir) = out_dir {
        if let Err(e) = inim::plan::write_plan_artifacts(&artifact, dir) {
            let _ = writeln!(stdout, "error: failed to write plan artifacts: {e}");
            return EXIT_INVALID_INPUT;
        }
    }

    EXIT_SUCCESS
}

/// `inim migrate-manifest`: convert a legacy manifest to the canonical schema.
///
/// Never executes analysis. Requires provenance when the conversion would
/// produce a Reviewed predicate.
fn cmd_migrate_manifest(
    stdout: &mut dyn Write,
    input: &std::path::Path,
    output: &std::path::Path,
    statement: Option<&str>,
    reviewed_by: Option<&str>,
    date: Option<&str>,
) -> i32 {
    let legacy = match inim::manifest::Manifest::load_legacy(input) {
        Ok(m) => m,
        Err(e) => {
            let _ = writeln!(stdout, "error: {e}");
            return EXIT_INVALID_INPUT;
        }
    };

    let provenance = match (statement, reviewed_by, date) {
        (Some(s), Some(r), Some(d)) => Some(inim::plan::Provenance {
            statement: s.to_string(),
            reviewed_by: r.to_string(),
            date: d.to_string(),
        }),
        (None, None, None) => None,
        _ => {
            let _ = writeln!(
                stdout,
                "error: provenance must be provided fully (--statement, --reviewed-by, --date) or not at all"
            );
            return EXIT_INVALID_INPUT;
        }
    };

    match inim::manifest::migrate_manifest(&legacy, provenance) {
        Ok(migrated) => {
            let json = match serde_json::to_string_pretty(&migrated) {
                Ok(j) => j,
                Err(e) => {
                    let _ = writeln!(stdout, "error: serialization failed: {e}");
                    return EXIT_INVALID_INPUT;
                }
            };
            if let Err(e) = std::fs::write(output, json) {
                let _ = writeln!(stdout, "error: cannot write {}: {e}", output.display());
                return EXIT_INVALID_INPUT;
            }
            let _ = writeln!(
                stdout,
                "migrated {} to canonical schema v{} (revision {}) at {}",
                migrated.event_id,
                migrated.schema_version,
                migrated.revision,
                output.display()
            );
            EXIT_SUCCESS
        }
        Err(e) => {
            let _ = writeln!(stdout, "error: {e}");
            EXIT_INVALID_INPUT
        }
    }
}

/// `inim compare`: build the comparison artifact from two event reports.
fn cmd_compare(
    stdout: &mut dyn Write,
    a_dir: &std::path::Path,
    b_dir: &std::path::Path,
    blocked_dir: Option<&std::path::Path>,
    out_dir: &std::path::Path,
) -> i32 {
    let a = match inim::compare::load_event_summary(&a_dir.join("report.json")) {
        Ok(s) => s,
        Err(e) => {
            let _ = writeln!(stdout, "error: {e}");
            return EXIT_INVALID_INPUT;
        }
    };
    let b = match inim::compare::load_event_summary(&b_dir.join("report.json")) {
        Ok(s) => s,
        Err(e) => {
            let _ = writeln!(stdout, "error: {e}");
            return EXIT_INVALID_INPUT;
        }
    };
    let mut artifact = inim::compare::ComparisonArtifact::new(a, b);
    if let Some(dir) = blocked_dir {
        match inim::compare::load_blocked_plan_summary(&dir.join("analysis_plan.json")) {
            Ok(blocked) => artifact = artifact.with_blocked(blocked),
            Err(e) => {
                let _ = writeln!(stdout, "error: {e}");
                return EXIT_INVALID_INPUT;
            }
        }
    }
    if let Err(e) = artifact.write(out_dir) {
        let _ = writeln!(stdout, "error: {e}");
        return EXIT_INVALID_INPUT;
    }
    let _ = writeln!(
        stdout,
        "comparison written to {} (comparison.json, comparison.txt)",
        out_dir.display()
    );
    EXIT_SUCCESS
}
///
/// A Blocked plan prints the plan and exits EXIT_ANALYSIS_BLOCKED without
/// any Broker query, archive download, or MRT parse. Incomplete analysis
/// exits EXIT_ANALYSIS_INCOMPLETE.
#[allow(clippy::too_many_arguments)]
fn cmd_origin_inventory(
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
    manifest_path: Option<&std::path::Path>,
    profile_path: Option<&std::path::Path>,
    cache: &std::path::Path,
    out: &std::path::Path,
    jobs: usize,
) -> i32 {
    use inim::catalog::netprofile::{CollectorLocationRegistry, ServicePlaneProfile};
    use inim::catalog::origin_inventory::build_inventory;
    use inim::manifest::Manifest;

    let Some(manifest_path) = manifest_path else {
        let _ = writeln!(stderr, "error: --origin-inventory requires --manifest");
        return EXIT_INVALID_INPUT;
    };
    let manifest = match Manifest::load(manifest_path) {
        Ok(m) => m,
        Err(e) => {
            let _ = writeln!(stderr, "error: {e}");
            return EXIT_INVALID_INPUT;
        }
    };
    let profile_path = profile_path.map(|p| p.to_path_buf()).unwrap_or_else(|| {
        std::path::PathBuf::from("case-studies/manlan-2019/pilot/network-profile.json")
    });
    let profile = match ServicePlaneProfile::load(&profile_path) {
        Ok(p) => p,
        Err(e) => {
            let _ = writeln!(stderr, "error: {e}");
            return EXIT_INVALID_INPUT;
        }
    };
    let locations_path = profile_path
        .parent()
        .map(|d| d.join("collector-locations.json"))
        .unwrap_or_else(|| std::path::PathBuf::from("collector-locations.json"));
    let registry = match CollectorLocationRegistry::load(&locations_path) {
        Ok(r) => r,
        Err(e) => {
            let _ = writeln!(stderr, "error: {e}");
            return EXIT_INVALID_INPUT;
        }
    };

    match build_inventory(
        &profile,
        &registry,
        cache,
        &manifest.source_family,
        &manifest.collectors,
        &manifest.target.origin_asns,
        &manifest.target.path_classifiers,
        cache,
        jobs,
    ) {
        Ok(inventories) => {
            let json = match serde_json::to_string_pretty(&inventories) {
                Ok(j) => j,
                Err(e) => {
                    let _ = writeln!(stderr, "error: cannot serialize inventory: {e}");
                    return EXIT_INVALID_INPUT;
                }
            };
            let _ = std::fs::create_dir_all(out);
            let path = out.join("origin-inventory.json");
            if let Err(e) = std::fs::write(&path, json) {
                let _ = writeln!(stderr, "error: cannot write {}: {e}", path.display());
                return EXIT_INVALID_INPUT;
            }
            let _ = writeln!(
                stdout,
                "origin inventory: {} collector(s) written to {}",
                inventories.len(),
                path.display()
            );
            EXIT_SUCCESS
        }
        Err(e) => {
            let _ = writeln!(stderr, "error: {e}");
            EXIT_INVALID_INPUT
        }
    }
}

/// Analyze a single event (see `--help`); EXIT_ANALYSIS_INCOMPLETE on
/// partial analysis.
#[allow(clippy::too_many_arguments)] // CLI passthrough; each maps to one flag
fn cmd_analyze(
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
    event_path: &std::path::Path,
    manifest_path: Option<&std::path::Path>,
    cache: &std::path::Path,
    out: &std::path::Path,
    discovery: &dyn inim::discover::ArchiveDiscovery,
    cache_control: inim::orchestrate::CacheControl,
    preflight_only: bool,
) -> i32 {
    let Some(manifest_path) = manifest_path else {
        return run_analyze_synthetic(stdout, event_path);
    };

    // ── Plan first: no broker/cache/MRT work before planning ──
    let (event_id, expectation) =
        match inim::sources::derive_expectation_from_fixture(event_path.to_string_lossy().as_ref())
        {
            Ok(pair) => pair,
            Err(e) => {
                let _ = writeln!(stderr, "error: failed to parse ticket fixture: {e}");
                return EXIT_INVALID_INPUT;
            }
        };
    let manifest = match inim::manifest::Manifest::load(manifest_path) {
        Ok(m) => m,
        Err(e) => {
            let _ = writeln!(stderr, "error: {e}");
            return EXIT_INVALID_INPUT;
        }
    };
    let plan = match inim::plan::plan_from_manifest(&event_id.0, expectation, &manifest) {
        Ok(p) => p,
        Err(e) => {
            let _ = writeln!(stderr, "error: {e}");
            return EXIT_INVALID_INPUT;
        }
    };

    if plan.is_blocked() {
        // Blocked plans are printed and never executed: zero Broker calls,
        // zero MRT parses, no AnalysisOutcome produced.
        let artifact = inim::plan::PlanArtifact::from_plan(&plan);
        let json = serde_json::to_string_pretty(&artifact).unwrap_or_default();
        let _ = writeln!(stdout, "{json}");
        return EXIT_ANALYSIS_BLOCKED;
    }

    let outcome = inim::orchestrate::run_real_analysis(
        event_path,
        manifest_path,
        cache,
        out,
        discovery,
        cache_control,
        preflight_only,
        &std::sync::atomic::AtomicBool::new(false),
        &inim::execution::TermSink,
    );

    if preflight_only {
        // Stage A: the preflight JSON was already printed by the runner
        // on success; do not emit an analysis outcome or write outputs.
        // A failed preflight must still be reported.
        if let inim::outcome::AnalysisOutcome::Incomplete { failure } = &outcome {
            let _ = writeln!(stderr, "error: preflight failed: {failure}");
            return EXIT_ANALYSIS_INCOMPLETE;
        }
        return EXIT_SUCCESS;
    }

    let json = serde_json::to_string_pretty(&outcome).unwrap_or_default();
    let _ = writeln!(stdout, "{json}");
    if matches!(outcome, inim::outcome::AnalysisOutcome::Incomplete { .. }) {
        EXIT_ANALYSIS_INCOMPLETE
    } else {
        EXIT_SUCCESS
    }
}

/// `inim catalog ...` administration commands.
fn cmd_catalog(stdout: &mut dyn Write, command: &CatalogCommands) -> i32 {
    match command {
        CatalogCommands::Init { db } => match inim::catalog::db::open_catalog(db) {
            Ok(conn) => {
                let version = inim::catalog::db::current_version(&conn).unwrap_or(0);
                drop(conn);
                let _ = writeln!(
                    stdout,
                    "catalog initialized at {} (schema v{version})",
                    db.display()
                );
                EXIT_SUCCESS
            }
            Err(e) => {
                let _ = writeln!(stdout, "error: {e}");
                EXIT_INVALID_INPUT
            }
        },
        CatalogCommands::CaseStudy(CaseStudyCommands::LinkRun {
            db,
            slug,
            run,
            role,
            note,
        }) => {
            let conn = match inim::catalog::db::open_catalog(db) {
                Ok(c) => c,
                Err(e) => {
                    let _ = writeln!(stdout, "error: {e}");
                    return EXIT_INVALID_INPUT;
                }
            };
            let Some(cs) = inim::catalog::archive_plan::find_case_study(&conn, slug) else {
                let _ = writeln!(stdout, "error: no case study with slug '{slug}'");
                return EXIT_INVALID_INPUT;
            };
            let run_exists: Option<i64> = conn
                .query_row("SELECT id FROM analysis_runs WHERE id = ?1", [*run], |r| {
                    r.get(0)
                })
                .ok();
            let Some(_) = run_exists else {
                let _ = writeln!(stdout, "error: no analysis run with id {run}");
                return EXIT_INVALID_INPUT;
            };
            let link = inim::catalog::domain::CaseStudyAnalysisLink {
                id: 0,
                case_study_id: cs.id,
                run_id: *run,
                role: role.clone(),
                reviewed_note: note.clone(),
            };
            match inim::catalog::store::insert_case_study_analysis_link(&conn, &link) {
                Ok(id) => {
                    let _ = writeln!(
                        stdout,
                        "run {run} linked to case study '{}' (role {role}, link id {id})",
                        cs.slug
                    );
                }
                Err(e) => {
                    let _ = writeln!(stdout, "error: {e}");
                    return EXIT_INVALID_INPUT;
                }
            }
            EXIT_SUCCESS
        }
        CatalogCommands::CaseStudy(CaseStudyCommands::PilotResult { db, path }) => {
            let conn = match inim::catalog::db::open_catalog(db) {
                Ok(c) => c,
                Err(e) => {
                    let _ = writeln!(stdout, "error: {e}");
                    return EXIT_INVALID_INPUT;
                }
            };
            match inim::catalog::archive_plan::apply_pilot_result(&conn, path) {
                Ok(slug) => {
                    let _ = writeln!(stdout, "pilot result recorded for case study '{slug}'");
                }
                Err(e) => {
                    let _ = writeln!(stdout, "error: {e}");
                    return EXIT_INVALID_INPUT;
                }
            }
            EXIT_SUCCESS
        }
        CatalogCommands::CaseStudy(CaseStudyCommands::ApplyResearch { db, path }) => {
            let conn = match inim::catalog::db::open_catalog(db) {
                Ok(c) => c,
                Err(e) => {
                    let _ = writeln!(stdout, "error: {e}");
                    return EXIT_INVALID_INPUT;
                }
            };
            match inim::catalog::target_research::apply_target_research(&conn, path) {
                Ok(s) => {
                    let _ = writeln!(
                        stdout,
                        "target research applied: case_study={} applied={} missing={}",
                        s.slug, s.targets_applied, s.targets_missing
                    );
                }
                Err(e) => {
                    let _ = writeln!(stdout, "error: {e}");
                    return EXIT_INVALID_INPUT;
                }
            }
            EXIT_SUCCESS
        }
        CatalogCommands::CaseStudy(CaseStudyCommands::Plan {
            db,
            slug,
            warmup_hours,
            cooldown_hours,
            family,
        }) => {
            let conn = match inim::catalog::db::open_catalog(db) {
                Ok(c) => c,
                Err(e) => {
                    let _ = writeln!(stdout, "error: {e}");
                    return EXIT_INVALID_INPUT;
                }
            };
            let Some(cs) = inim::catalog::archive_plan::find_case_study(&conn, slug) else {
                let _ = writeln!(stdout, "error: no case study with slug '{slug}'");
                return EXIT_INVALID_INPUT;
            };
            let targets = match inim::catalog::archive_plan::list_targets(&conn, cs.id) {
                Ok(t) => t,
                Err(e) => {
                    let _ = writeln!(stdout, "error: {e}");
                    return EXIT_INVALID_INPUT;
                }
            };
            let families = match inim::catalog::archive_plan::SourceFamily::parse_family(family) {
                Some(f) => vec![f],
                None => {
                    let _ = writeln!(
                        stdout,
                        "error: unknown source family '{family}' (expected RouteViews or RipeRis)"
                    );
                    return EXIT_INVALID_INPUT;
                }
            };
            match inim::catalog::archive_plan::build_plan_for_families(
                &cs,
                &targets,
                *warmup_hours,
                *cooldown_hours,
                &families,
            ) {
                Ok((horizon, plan)) => {
                    if let Err(e) =
                        inim::catalog::archive_plan::save_plan(&conn, cs.id, &horizon, &plan)
                    {
                        let _ = writeln!(stdout, "error: {e}");
                        return EXIT_INVALID_INPUT;
                    }
                    let _ = writeln!(
                        stdout,
                        "archive plan stored (Draft) for {}: horizon {} .. {} (warmup {}h, cooldown {}h)",
                        cs.slug, horizon.warmup_start_utc, horizon.cooldown_end_utc,
                        horizon.warmup_hours, horizon.cooldown_hours
                    );
                    for c in &plan.collectors {
                        let _ = writeln!(
                            stdout,
                            "  {}: baseline RIB {} + validation RIB {} + {} updates (availability: {})",
                            c.collector,
                            c.baseline_rib.url.rsplit('/').next().unwrap_or_default(),
                            c.validation_rib
                                .as_ref()
                                .map(|r| r.url.rsplit('/').next().unwrap_or_default().to_string())
                                .unwrap_or_else(|| "none".to_string()),
                            c.updates.len(),
                            c.availability
                        );
                    }
                    let _ = writeln!(
                        stdout,
                        "  estimated total download: ~{:.1} MiB compressed / ~{:.1} GiB uncompressed (estimates)",
                        plan.estimated_total_bytes as f64 / (1024.0 * 1024.0),
                        plan.estimated_total_uncompressed_bytes as f64 / (1024.0 * 1024.0 * 1024.0)
                    );
                    for b in &plan.blocked_targets {
                        let _ = writeln!(stdout, "  blocked: {} — {}", b.source_label, b.reason);
                    }
                    for n in &plan.notes {
                        let _ = writeln!(stdout, "  note: {n}");
                    }
                }
                Err(e) => {
                    let _ = writeln!(stdout, "error: {e}");
                    return EXIT_INVALID_INPUT;
                }
            }
            EXIT_SUCCESS
        }
        CatalogCommands::CaseStudy(CaseStudyCommands::Import { db, path }) => {
            let conn = match inim::catalog::db::open_catalog(db) {
                Ok(c) => c,
                Err(e) => {
                    let _ = writeln!(stdout, "error: {e}");
                    return EXIT_INVALID_INPUT;
                }
            };
            match inim::catalog::case_study_import::import_case_study(&conn, path) {
                Ok(summary) => {
                    let _ = writeln!(
                        stdout,
                        "case study imported: id={} slug={} created={} documents={} phases={} claims={} targets={} event_links={} (linked={}, unresolved={})",
                        summary.case_study_id,
                        summary.slug,
                        summary.created,
                        summary.documents,
                        summary.phases,
                        summary.claims,
                        summary.targets,
                        summary.event_links,
                        summary.linked_events,
                        summary.unresolved_references
                    );
                }
                Err(e) => {
                    let _ = writeln!(stdout, "error: {e}");
                    return EXIT_INVALID_INPUT;
                }
            }
            EXIT_SUCCESS
        }
        CatalogCommands::Document(DocumentCommands::Import {
            db,
            file,
            source_url,
            title,
            doc_type,
            provenance,
            root,
        }) => {
            let conn = match inim::catalog::db::open_catalog(db) {
                Ok(c) => c,
                Err(e) => {
                    let _ = writeln!(stdout, "error: {e}");
                    return EXIT_INVALID_INPUT;
                }
            };
            match inim::catalog::document::import_document(
                &conn,
                root,
                file,
                source_url,
                title.as_deref(),
                doc_type.as_deref(),
                provenance.as_deref(),
            ) {
                Ok(o) => {
                    let _ = writeln!(
                        stdout,
                        "document imported: id={} revision={} sha256={} path={} media={} pages={}",
                        o.document_id,
                        o.revision,
                        o.sha256,
                        o.relative_path,
                        o.media_type,
                        o.page_count
                            .map(|n| n.to_string())
                            .unwrap_or_else(|| "n/a".to_string())
                    );
                }
                Err(e) => {
                    let _ = writeln!(stdout, "error: {e}");
                    return EXIT_INVALID_INPUT;
                }
            }
            EXIT_SUCCESS
        }
        CatalogCommands::Import { db, root } => {
            let conn = match inim::catalog::db::open_catalog(db) {
                Ok(c) => c,
                Err(e) => {
                    let _ = writeln!(stdout, "error: {e}");
                    return EXIT_INVALID_INPUT;
                }
            };
            let version = env!("CARGO_PKG_VERSION");
            let git = git_revision();
            match inim::catalog::import::import_repository(&conn, root, version, git.as_deref()) {
                Ok(summary) => {
                    let _ = writeln!(
                        stdout,
                        "imported {} events, {} snapshots, {} manifests, {} plans, {} runs, {} artifacts, {} streams, {} waves",
                        summary.events,
                        summary.snapshots,
                        summary.manifests,
                        summary.plans,
                        summary.runs,
                        summary.artifacts,
                        summary.streams,
                        summary.waves
                    );
                    EXIT_SUCCESS
                }
                Err(e) => {
                    let _ = writeln!(stdout, "error: {e}");
                    EXIT_INVALID_INPUT
                }
            }
        }
        CatalogCommands::Workbench { db, subject } => {
            let conn = match inim::catalog::db::open_catalog(db) {
                Ok(c) => c,
                Err(e) => {
                    let _ = writeln!(stdout, "error: {e}");
                    return EXIT_INVALID_INPUT;
                }
            };
            // Events get the reviewed collector-site registry only (the
            // pilot-scoped session audit and peering-plane decision are case-study
            // context). Case studies get the full pilot context.
            let vm = match inim::catalog::web::view::load_event_workbench(
                &conn,
                subject,
                std::path::Path::new("."),
                &inim::catalog::web::handlers::WorkbenchQuery::default(),
            ) {
                Ok(Some(v)) => Some(v.vm),
                Ok(None) => {
                    match inim::catalog::web::view::load_case_study_workbench(
                        &conn,
                        subject,
                        std::path::Path::new("."),
                        &inim::catalog::web::handlers::WorkbenchQuery::default(),
                    ) {
                        Ok(v) => v.map(|v| v.vm),
                        Err(e) => {
                            let _ = writeln!(stdout, "error: {e}");
                            return EXIT_INVALID_INPUT;
                        }
                    }
                }
                Err(e) => {
                    let _ = writeln!(stdout, "error: {e}");
                    return EXIT_INVALID_INPUT;
                }
            };
            match vm {
                Some(v) => {
                    let _ = write!(stdout, "{}", v.render_text());
                    EXIT_SUCCESS
                }
                None => {
                    let _ = writeln!(stdout, "error: no event or case study matches {subject}");
                    EXIT_INVALID_INPUT
                }
            }
        }
        CatalogCommands::FindingChronologyAudit {
            event,
            collector,
            peer,
            out,
            run_dir: run_dir_override,
        } => {
            let run_dir = run_dir_override.clone().unwrap_or_else(|| {
                std::path::Path::new("case-studies")
                    .join(event.to_lowercase())
                    .join("out")
                    .join(event)
            });
            let audit = match inim::catalog::workbench::load_finding_chronology_audit(
                &run_dir, event, collector, peer,
            ) {
                Ok(a) => a,
                Err(e) => {
                    let _ = writeln!(stdout, "error: {e}");
                    return EXIT_INVALID_INPUT;
                }
            };
            let json = match serde_json::to_string_pretty(&audit) {
                Ok(j) => j,
                Err(e) => {
                    let _ = writeln!(stdout, "error: cannot serialize audit: {e}");
                    return EXIT_INVALID_INPUT;
                }
            };
            match out {
                Some(path) => {
                    if let Err(e) = std::fs::write(path, json) {
                        let _ = writeln!(stdout, "error: cannot write {}: {e}", path.display());
                        return EXIT_INVALID_INPUT;
                    }
                }
                None => {
                    let _ = writeln!(stdout, "{json}");
                }
            }
            EXIT_SUCCESS
        }
        CatalogCommands::FindingAudit { db, subject, out } => {
            let conn = match inim::catalog::db::open_catalog(db) {
                Ok(c) => c,
                Err(e) => {
                    let _ = writeln!(stdout, "error: {e}");
                    return EXIT_INVALID_INPUT;
                }
            };
            let vm = match inim::catalog::web::view::load_event_workbench(
                &conn,
                subject,
                std::path::Path::new("."),
                &inim::catalog::web::handlers::WorkbenchQuery::default(),
            ) {
                Ok(Some(v)) => Some(v.vm),
                Ok(None) => {
                    match inim::catalog::web::view::load_case_study_workbench(
                        &conn,
                        subject,
                        std::path::Path::new("."),
                        &inim::catalog::web::handlers::WorkbenchQuery::default(),
                    ) {
                        Ok(v) => v.map(|v| v.vm),
                        Err(e) => {
                            let _ = writeln!(stdout, "error: {e}");
                            return EXIT_INVALID_INPUT;
                        }
                    }
                }
                Err(e) => {
                    let _ = writeln!(stdout, "error: {e}");
                    return EXIT_INVALID_INPUT;
                }
            };
            let Some(vm) = vm else {
                let _ = writeln!(stdout, "error: no event or case study matches {subject}");
                return EXIT_INVALID_INPUT;
            };
            let audit = inim::catalog::workbench::FindingAudit::from_vm(&vm);
            let json = match serde_json::to_string_pretty(&audit) {
                Ok(j) => j,
                Err(e) => {
                    let _ = writeln!(stdout, "error: cannot serialize audit: {e}");
                    return EXIT_INVALID_INPUT;
                }
            };
            match out {
                Some(path) => {
                    if let Err(e) = std::fs::write(path, json) {
                        let _ = writeln!(stdout, "error: cannot write {}: {e}", path.display());
                        return EXIT_INVALID_INPUT;
                    }
                }
                None => {
                    let _ = writeln!(stdout, "{json}");
                }
            }
            EXIT_SUCCESS
        }
        CatalogCommands::Sync(source) => match source {
            SyncSource::Grnoc {
                db,
                source_dir,
                seed,
                case_study,
                expand_references,
                search,
                domain,
                max_requests,
                requests_per_second,
                allow_higher_rate,
                contact,
                dry_run,
                show_access_policy,
                show_domains,
            } => {
                use inim::catalog::access::AccessPolicy;
                use inim::catalog::grnoc_viewer::GrnocViewerClient;
                let policy = AccessPolicy::conservative();
                if *show_domains {
                    let mut client = match GrnocViewerClient::new(policy.clone()) {
                        Ok(c) => c,
                        Err(e) => {
                            let _ = writeln!(stdout, "error: {e}");
                            return EXIT_INVALID_INPUT;
                        }
                    };
                    match client.fetch_domains() {
                        Ok(domains) => {
                            for d in domains {
                                let name = d.get("name").and_then(|v| v.as_str()).unwrap_or("?");
                                let id = d.get("sys_id").and_then(|v| v.as_str()).unwrap_or("?");
                                let criteria =
                                    d.get("criteria").and_then(|v| v.as_str()).unwrap_or("");
                                let _ = writeln!(stdout, "{name}	{id}	{criteria}");
                            }
                        }
                        Err(e) => {
                            let _ = writeln!(stdout, "error: cannot fetch domains: {e}");
                            return EXIT_INVALID_INPUT;
                        }
                    }
                    return EXIT_SUCCESS;
                }
                if *show_access_policy {
                    use inim::catalog::access::AccessPolicy;
                    let policy = AccessPolicy::conservative();
                    let _ = writeln!(
                        stdout,
                        "self-imposed access policy (public corpus acquisition):"
                    );
                    let _ = writeln!(stdout, "  max concurrency:    {}", policy.max_concurrency);
                    let _ = writeln!(
                        stdout,
                        "  sustained rate:     {} requests/second (one every {:.0}s)",
                        policy.requests_per_second,
                        1.0 / policy.requests_per_second
                    );
                    let _ = writeln!(
                        stdout,
                        "  burst:              {} (then paced)",
                        policy.burst
                    );
                    let _ = writeln!(stdout, "  adaptive:           429/Retry-After halves the effective rate; repeated throttling stops; sustained success recovers up to the ceiling");
                    let _ = writeln!(
                        stdout,
                        "  request budget:     {} per sync",
                        policy.max_requests
                    );
                    let _ = writeln!(stdout, "  user agent:         {}", policy.user_agent());
                    let _ = writeln!(
                        stdout,
                        "  retries:            {} per transient failure",
                        policy.max_retries
                    );
                    let _ = writeln!(stdout, "  stop conditions:    repeated 429/403, unexpected auth, robots prohibition, schema incompatibility affecting most items");
                    let _ = writeln!(
                        stdout,
                        "  404 policy:         permanent 404s are never retried"
                    );
                    let _ = writeln!(
                        stdout,
                        "  enumeration:        no blind numeric-ID enumeration"
                    );
                    return EXIT_SUCCESS;
                }
                match source_dir {
                    Some(dir) => cmd_grnoc_sync_offline(stdout, db, dir),
                    None => cmd_grnoc_sync_live(
                        stdout,
                        db,
                        seed,
                        case_study,
                        *expand_references,
                        search,
                        domain.as_deref(),
                        *max_requests,
                        *requests_per_second,
                        *allow_higher_rate,
                        contact.as_deref(),
                        *dry_run,
                    ),
                }
            }
        },
        CatalogCommands::Relationships(RelationshipsCommands::Rebuild { db }) => {
            cmd_relationships_rebuild(stdout, db)
        }
        CatalogCommands::Relationships(RelationshipsCommands::Audit { db, source_kind }) => {
            cmd_relationships_audit(stdout, db, source_kind)
        }
        CatalogCommands::CorpusReview {
            db,
            file,
            source_kind,
        } => cmd_corpus_review(stdout, db, file, source_kind),
        CatalogCommands::SessionAudit {
            root,
            profile,
            locations,
            cache,
            date,
            origin_asns,
            extraction_cache,
            jobs,
            full_inventory,
            out,
        } => cmd_session_audit(
            stdout,
            root,
            profile.as_deref(),
            locations.as_deref(),
            cache,
            date,
            origin_asns,
            extraction_cache.as_deref(),
            *jobs,
            *full_inventory,
            out,
        ),
        CatalogCommands::SessionMetadataBackfill { db, cache, date } => {
            let conn = match inim::catalog::db::open_catalog(db) {
                Ok(c) => c,
                Err(e) => {
                    let _ = writeln!(stdout, "error: {e}");
                    return EXIT_INVALID_INPUT;
                }
            };
            let mut caches: Vec<(std::path::PathBuf, String)> = Vec::new();
            for entry in cache {
                let Some((dir, family)) = entry.split_once(':') else {
                    let _ = writeln!(stdout, "error: --cache expects DIR:FAMILY, got {entry:?}");
                    return EXIT_INVALID_INPUT;
                };
                caches.push((std::path::PathBuf::from(dir), family.to_string()));
            }
            match inim::catalog::session_audit::backfill_session_metadata(&conn, &caches, date) {
                Ok(n) => {
                    let _ = writeln!(
                        stdout,
                        "session metadata backfill: {n} observation(s) recorded"
                    );
                    EXIT_SUCCESS
                }
                Err(e) => {
                    let _ = writeln!(stdout, "error: {e}");
                    EXIT_INVALID_INPUT
                }
            }
        }
        CatalogCommands::AnalysisQueue { db, state } => {
            cmd_analysis_queue(stdout, db, state.as_deref())
        }
        CatalogCommands::ArchiveBatches(ArchiveBatchesCommands::Plan { db }) => {
            cmd_archive_batches_plan(stdout, db)
        }
        CatalogCommands::CorpusExport { db, out } => cmd_corpus_export(stdout, db, out.as_deref()),
    }
}

/// Current git revision for run provenance (best effort).
fn git_revision() -> Option<String> {
    let out = std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()?;
    if out.status.success() {
        Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
    } else {
        None
    }
}

/// `inim serve` — read-only localhost catalog web UI.
fn cmd_serve(
    stdout: &mut dyn Write,
    db: &std::path::Path,
    root: &std::path::Path,
    bind: &str,
    allow_non_loopback: bool,
    enable_writes: bool,
    allow_unauthenticated_writes: bool,
) -> i32 {
    if let Err(e) = inim::catalog::web::server::validate_bind(bind, allow_non_loopback) {
        let _ = writeln!(stdout, "error: {e}");
        return EXIT_INVALID_INPUT;
    }
    let runtime = match tokio::runtime::Runtime::new() {
        Ok(r) => r,
        Err(e) => {
            let _ = writeln!(stdout, "error: cannot start async runtime: {e}");
            return EXIT_INVALID_INPUT;
        }
    };
    let version = env!("CARGO_PKG_VERSION");
    match runtime.block_on(inim::catalog::web::server::serve(
        db,
        root,
        bind,
        allow_non_loopback,
        enable_writes,
        allow_unauthenticated_writes,
        version,
    )) {
        Ok(()) => EXIT_SUCCESS,
        Err(e) => {
            let _ = writeln!(stdout, "error: {e}");
            EXIT_INVALID_INPUT
        }
    }
}

fn run_analyze_synthetic(stdout: &mut dyn Write, event_path: &std::path::Path) -> i32 {
    // ── 1. Parse the Internet2 ticket fixture ────────────────────
    let ticket = match i2ticket::parse_ticket_fixture(event_path.to_string_lossy().as_ref()) {
        Ok(t) => t,
        Err(e) => {
            let _ = writeln!(stdout, "Error parsing ticket: {e}");
            return EXIT_INVALID_INPUT;
        }
    };

    let expectation = i2ticket::derive_expectation(&ticket);

    let _ = writeln!(stdout, "inim: analyze command parsed.");
    let _ = writeln!(stdout, "  event:   {}", event_path.display());
    let _ = writeln!(stdout, "  expectation: {:?}", expectation.kind);

    // ── 2. Ingest observations ──────────────────────────────────
    // TODO: when --rib and --updates are provided, use ingest::ObservationStream.
    // For now, use synthetic observations for the demo/vertical slice.

    // ── 3. Reconstruct route state ──────────────────────────────
    // For the demo: use the built-in redundant scenario

    let _ = writeln!(stdout, "  Using synthetic observations for demonstration.");
    let _ = writeln!(stdout);

    let (store, changes) = build_demo_scenario("route-views2");

    use inim::domain::route::Continuity;
    let any_unknown = changes
        .iter()
        .any(|sc| sc.continuity == Continuity::Unknown);

    // ── 4. Tokenize transitions ─────────────────────────────────
    // Build baseline map from frozen event baseline
    let baseline_map: std::collections::HashMap<_, _> = store
        .all_states()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    let transitions = tokenize::tokenize(changes, &baseline_map);

    // ── 5. Detect impact waves ──────────────────────────────────
    let mut detected_waves = waves::detect_waves(&transitions, chrono::Duration::seconds(30));
    waves::summarize_waves(&mut detected_waves);

    // ── 6. Assess expectation vs observation ────────────────────
    let assessment = assess::assess(
        ticket.id.clone(),
        expectation,
        &transitions,
        detected_waves,
        any_unknown,
        None, // synthetic path: no lifecycle data
    );

    // ── 7. Render reports ───────────────────────────────────────
    let data_note = "SYNTHETIC (no --manifest provided)";
    let terminal_report = report::render_terminal(&assessment, data_note);

    let _ = writeln!(stdout, "{terminal_report}");

    let json_report = report::render_json(&assessment, data_note);
    let json_str = serde_json::to_string_pretty(&json_report).unwrap_or_default();
    let _ = writeln!(stdout, "--- JSON ---");
    let _ = writeln!(stdout, "{json_str}");
    EXIT_SUCCESS
}

/// Build a demonstration scenario for the vertical slice:
/// redundant maintenance — baseline → alternate → stable → restore.
fn build_demo_scenario(
    collector: &str,
) -> (
    inim::routes::RouteStateStore,
    Vec<inim::domain::route::StateChange>,
) {
    use chrono::{TimeZone, Utc};

    let event_start = Utc.with_ymd_and_hms(2025, 6, 15, 1, 0, 0).unwrap();
    let event_end = Utc.with_ymd_and_hms(2025, 6, 15, 6, 0, 0).unwrap();

    let obs = vec![
        // Two observer perspectives (rv2:AS6447 and rv6:AS6447)
        fixtures::make_synthetic_rib(
            "192.0.2.0/24",
            collector,
            "185.1.8.65",
            6447,
            vec![6447, 11537, 1101], // baseline: via AS11537
            Utc.with_ymd_and_hms(2025, 6, 15, 0, 50, 0).unwrap(),
            0,
        ),
        fixtures::make_synthetic_rib(
            "192.0.2.0/24",
            collector,
            "2001:7f8:4::1",
            6447,
            vec![6447, 11537, 1101],
            Utc.with_ymd_and_hms(2025, 6, 15, 0, 50, 0).unwrap(),
            1,
        ),
        // Warm-up: pre-event alternate announcement (should not emit transition)
        fixtures::make_synthetic_announcement(
            "192.0.2.0/24",
            collector,
            "185.1.8.65",
            6447,
            vec![6447, 11537, 1101], // same as baseline, warm-up
            Utc.with_ymd_and_hms(2025, 6, 15, 0, 55, 0).unwrap(),
            2,
        ),
        // Event: failover to alternate path (path change)
        fixtures::make_synthetic_announcement(
            "192.0.2.0/24",
            collector,
            "185.1.8.65",
            6447,
            vec![6447, 237, 1101], // alternate: via AS237
            Utc.with_ymd_and_hms(2025, 6, 15, 1, 2, 14).unwrap(),
            3,
        ),
        fixtures::make_synthetic_announcement(
            "192.0.2.0/24",
            collector,
            "2001:7f8:4::1",
            6447,
            vec![6447, 237, 1101], // second peer sees same alternate
            Utc.with_ymd_and_hms(2025, 6, 15, 1, 2, 18).unwrap(),
            4,
        ),
        // Restoration: back to baseline path
        fixtures::make_synthetic_announcement(
            "192.0.2.0/24",
            collector,
            "185.1.8.65",
            6447,
            vec![6447, 11537, 1101], // restore to baseline
            Utc.with_ymd_and_hms(2025, 6, 15, 5, 51, 44).unwrap(),
            5,
        ),
        fixtures::make_synthetic_announcement(
            "192.0.2.0/24",
            collector,
            "2001:7f8:4::1",
            6447,
            vec![6447, 11537, 1101], // second peer restores
            Utc.with_ymd_and_hms(2025, 6, 15, 5, 53, 11).unwrap(),
            6,
        ),
    ];

    let cooldown_end = event_end + chrono::Duration::hours(1);
    routes::reconstruct_routes(obs, event_start, event_end, cooldown_end)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_parse_analyze_with_event_only() {
        let args = vec!["inim", "analyze", "--event", "event.json"];
        let cli = Cli::try_parse_from(args).unwrap();
        match cli.command {
            Commands::Analyze {
                event,
                manifest,
                cache,
                out,
                ..
            } => {
                assert_eq!(event.to_string_lossy(), "event.json");
                assert!(manifest.is_none());
                assert_eq!(cache.to_string_lossy(), "./cache");
                assert_eq!(out.to_string_lossy(), "./out");
            }
            Commands::Plan { .. } => unreachable!("plan not expected"),
            Commands::MigrateManifest { .. } => unreachable!("migrate not expected"),
            Commands::Compare { .. } => unreachable!("compare not expected"),
            Commands::Catalog(_) => unreachable!("catalog not expected"),
            Commands::Serve { .. } => unreachable!("serve not expected"),
            Commands::AnalysisPlan { .. } => unreachable!("analysis-plan not expected"),
            Commands::AnalysisJob(_) => unreachable!("analysis-job not expected"),
            Commands::Worker { .. } => unreachable!("worker not expected"),
            Commands::Demo(_) => unreachable!("demo not expected"),
            Commands::ProjectScope(_) => unreachable!("project-scope not expected"),
        }
    }

    #[test]
    fn cli_parse_analyze_with_manifest() {
        let args = vec![
            "inim",
            "analyze",
            "--event",
            "event.json",
            "--manifest",
            "manifest.json",
        ];
        let cli = Cli::try_parse_from(args).unwrap();
        match cli.command {
            Commands::Analyze { manifest, .. } => {
                assert!(manifest.is_some());
            }
            Commands::Plan { .. } => unreachable!("plan not expected"),
            Commands::MigrateManifest { .. } => unreachable!("migrate not expected"),
            Commands::Compare { .. } => unreachable!("compare not expected"),
            Commands::Catalog(_) => unreachable!("catalog not expected"),
            Commands::Serve { .. } => unreachable!("serve not expected"),
            Commands::AnalysisPlan { .. } => unreachable!("analysis-plan not expected"),
            Commands::AnalysisJob(_) => unreachable!("analysis-job not expected"),
            Commands::Worker { .. } => unreachable!("worker not expected"),
            Commands::Demo(_) => unreachable!("demo not expected"),
            Commands::ProjectScope(_) => unreachable!("project-scope not expected"),
        }
    }

    #[test]
    fn cli_parse_analyze_with_cache_and_out() {
        let args = vec![
            "inim", "analyze", "--event", "ev.json", "--cache", "/tmp/c", "--out", "/tmp/o",
        ];
        let cli = Cli::try_parse_from(args).unwrap();
        match cli.command {
            Commands::Analyze { cache, out, .. } => {
                assert_eq!(cache.to_string_lossy(), "/tmp/c");
                assert_eq!(out.to_string_lossy(), "/tmp/o");
            }
            Commands::Plan { .. } => unreachable!("plan not expected"),
            Commands::MigrateManifest { .. } => unreachable!("migrate not expected"),
            Commands::Compare { .. } => unreachable!("compare not expected"),
            Commands::Catalog(_) => unreachable!("catalog not expected"),
            Commands::Serve { .. } => unreachable!("serve not expected"),
            Commands::AnalysisPlan { .. } => unreachable!("analysis-plan not expected"),
            Commands::AnalysisJob(_) => unreachable!("analysis-job not expected"),
            Commands::Worker { .. } => unreachable!("worker not expected"),
            Commands::Demo(_) => unreachable!("demo not expected"),
            Commands::ProjectScope(_) => unreachable!("project-scope not expected"),
        }
    }

    #[test]
    fn cli_rejects_missing_event() {
        let args = vec!["inim", "analyze"];
        assert!(Cli::try_parse_from(args).is_err());
    }

    #[test]
    fn cli_short_flags_work() {
        let args = vec![
            "inim", "analyze", "-e", "e.json", "-m", "m.json", "-c", "c", "-o", "o",
        ];
        let cli = Cli::try_parse_from(args).unwrap();
        match cli.command {
            Commands::Analyze {
                event, manifest, ..
            } => {
                assert_eq!(event.to_string_lossy(), "e.json");
                assert!(manifest.is_some());
            }
            Commands::Plan { .. } => unreachable!("plan not expected"),
            Commands::MigrateManifest { .. } => unreachable!("migrate not expected"),
            Commands::Compare { .. } => unreachable!("compare not expected"),
            Commands::Catalog(_) => unreachable!("catalog not expected"),
            Commands::Serve { .. } => unreachable!("serve not expected"),
            Commands::AnalysisPlan { .. } => unreachable!("analysis-plan not expected"),
            Commands::AnalysisJob(_) => unreachable!("analysis-job not expected"),
            Commands::Worker { .. } => unreachable!("worker not expected"),
            Commands::Demo(_) => unreachable!("demo not expected"),
            Commands::ProjectScope(_) => unreachable!("project-scope not expected"),
        }
    }

    // ── Part 1: planning + exit-status semantics ──────────────────

    use inim::discover::{ArchiveDiscovery, ArchiveItem, InimArchiveError};
    use std::io::Cursor;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Discovery that counts calls and fails if ever invoked — used to
    /// prove blocked plans perform zero Broker work.
    struct CountingDiscovery {
        calls: AtomicUsize,
    }

    impl CountingDiscovery {
        fn new() -> Self {
            CountingDiscovery {
                calls: AtomicUsize::new(0),
            }
        }
        fn call_count(&self) -> usize {
            self.calls.load(Ordering::SeqCst)
        }
    }

    impl ArchiveDiscovery for CountingDiscovery {
        fn query(
            &self,
            _project: &str,
            _collectors: &[&str],
            _ts_start: chrono::DateTime<chrono::Utc>,
            _ts_end: chrono::DateTime<chrono::Utc>,
            _data_type: &str,
        ) -> Result<Vec<ArchiveItem>, InimArchiveError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Err(InimArchiveError::BrokerQueryError {
                reason: "discovery must not run for blocked plans".into(),
            })
        }
    }

    const TICKET: &str = "tests/fixtures/internet2/INC0302574.json";

    /// A blocked manifest: unresolved transit predicate, open ticket.
    const BLOCKED_MANIFEST: &str = r#"{
        "event_id": "INC0301970",
        "revision": 1,
        "schema_version": 2,
        "event_window_utc": {"start": "2026-07-28T04:35:00Z", "end": ""},
        "open": true,
        "analysis_end_utc": "2026-07-29T04:35:00Z",
        "ticket_window_local": {"start": "2026-07-28 00:35:00", "end": "", "timezone": "EDT"},
        "warmup_minutes": 60,
        "cooldown_minutes": 60,
        "target": {
            "label": "Smithville via Indiana GigaPOP",
            "origin_asns": [11550],
            "transit_predicate": {"status": "Unresolved"},
            "prefix_selection": "origin AS11550 AND baseline AS path contains <PENDING REVIEW>"
        },
        "collectors": ["route-views2"]
    }"#;

    /// A ready manifest: reviewed ContainsAny predicate.
    const READY_MANIFEST: &str = r#"{
        "event_id": "INC0302574",
        "revision": 2,
        "schema_version": 2,
        "event_window_utc": {"start": "2026-07-30T09:25:00Z", "end": "2026-07-30T09:47:00Z"},
        "ticket_window_local": {"start": "2026-07-30 05:25:00", "end": "2026-07-30 05:47:00", "timezone": "EDT"},
        "warmup_minutes": 60,
        "cooldown_minutes": 60,
        "target": {
            "label": "RIPE via NYIIX",
            "origin_asns": [3333],
            "transit_predicate": {
                "status": "Reviewed",
                "predicate": {"ContainsAny": [11537]},
                "provenance": {"statement": "AS11537 = Internet2", "reviewed_by": "analyst", "date": "2026-08-01"}
            },
            "prefix_selection": "origin AS3333 AND baseline path contains AS11537"
        },
        "collectors": ["route-views2"]
    }"#;

    fn write_manifest(dir: &std::path::Path, json: &str) -> std::path::PathBuf {
        let p = dir.join("manifest.json");
        std::fs::write(&p, json).unwrap();
        p
    }

    #[test]
    fn plan_command_reports_blocked_and_exits_successfully() {
        let dir = tempfile::tempdir().unwrap();
        let manifest = write_manifest(dir.path(), BLOCKED_MANIFEST);
        let mut out = Cursor::new(Vec::new());
        let code = cmd_plan(&mut out, std::path::Path::new(TICKET), &manifest, None);
        assert_eq!(code, EXIT_SUCCESS);
        let text = String::from_utf8(out.into_inner()).unwrap();
        assert!(text.contains("Blocked"), "{text}");
        assert!(text.contains("MissingReviewedTransitPredicate"), "{text}");
    }

    #[test]
    fn analyze_command_reports_blocked_and_exits_nonzero() {
        let dir = tempfile::tempdir().unwrap();
        let manifest = write_manifest(dir.path(), BLOCKED_MANIFEST);
        let out_dir = dir.path().join("out");
        let discovery = CountingDiscovery::new();
        let mut out = Cursor::new(Vec::new());
        let mut err = Cursor::new(Vec::new());
        let code = cmd_analyze(
            &mut out,
            &mut err,
            std::path::Path::new(TICKET),
            Some(&manifest),
            &dir.path().join("cache"),
            &out_dir,
            &discovery,
            inim::orchestrate::CacheControl::default(),
            false,
        );
        assert_eq!(code, EXIT_ANALYSIS_BLOCKED);
        let text = String::from_utf8(out.into_inner()).unwrap();
        assert!(text.contains("MissingReviewedTransitPredicate"), "{text}");
    }

    #[test]
    fn blocked_analyze_has_no_analysis_outcome() {
        let dir = tempfile::tempdir().unwrap();
        let manifest = write_manifest(dir.path(), BLOCKED_MANIFEST);
        let out_dir = dir.path().join("out");
        let discovery = CountingDiscovery::new();
        let mut out = Cursor::new(Vec::new());
        let mut err = Cursor::new(Vec::new());
        cmd_analyze(
            &mut out,
            &mut err,
            std::path::Path::new(TICKET),
            Some(&manifest),
            &dir.path().join("cache"),
            &out_dir,
            &discovery,
            inim::orchestrate::CacheControl::default(),
            false,
        );
        let text = String::from_utf8(out.into_inner()).unwrap();
        // A blocked plan produces a plan artifact, never an AnalysisOutcome.
        assert!(text.contains("\"plan\""), "{text}");
        assert!(!text.contains("completed"), "{text}");
        assert!(!text.contains("insufficient_visibility"), "{text}");
        assert!(!text.contains("incomplete"), "{text}");
    }

    #[test]
    fn blocked_analyze_performs_zero_broker_calls() {
        let dir = tempfile::tempdir().unwrap();
        let manifest = write_manifest(dir.path(), BLOCKED_MANIFEST);
        let out_dir = dir.path().join("out");
        let discovery = CountingDiscovery::new();
        let mut out = Cursor::new(Vec::new());
        let mut err = Cursor::new(Vec::new());
        let code = cmd_analyze(
            &mut out,
            &mut err,
            std::path::Path::new(TICKET),
            Some(&manifest),
            &dir.path().join("cache"),
            &out_dir,
            &discovery,
            inim::orchestrate::CacheControl::default(),
            false,
        );
        assert_eq!(code, EXIT_ANALYSIS_BLOCKED);
        assert_eq!(discovery.call_count(), 0);
    }

    #[test]
    fn blocked_analyze_performs_zero_mrt_parses() {
        let dir = tempfile::tempdir().unwrap();
        let manifest = write_manifest(dir.path(), BLOCKED_MANIFEST);
        let out_dir = dir.path().join("out");
        let discovery = CountingDiscovery::new();
        let mut out = Cursor::new(Vec::new());
        let mut err = Cursor::new(Vec::new());
        let code = cmd_analyze(
            &mut out,
            &mut err,
            std::path::Path::new(TICKET),
            Some(&manifest),
            &dir.path().join("cache"),
            &out_dir,
            &discovery,
            inim::orchestrate::CacheControl::default(),
            false,
        );
        assert_eq!(code, EXIT_ANALYSIS_BLOCKED);
        // No MRT parsing implies no analysis artifacts are written.
        assert!(!out_dir.exists() || std::fs::read_dir(&out_dir).unwrap().count() == 0);
        // Zero broker calls also means no archive could have been parsed.
        assert_eq!(discovery.call_count(), 0);
    }

    #[test]
    fn ready_plan_does_not_start_analysis() {
        let dir = tempfile::tempdir().unwrap();
        let manifest = write_manifest(dir.path(), READY_MANIFEST);
        let mut out = Cursor::new(Vec::new());
        let code = cmd_plan(&mut out, std::path::Path::new(TICKET), &manifest, None);
        assert_eq!(code, EXIT_SUCCESS);
        let text = String::from_utf8(out.into_inner()).unwrap();
        assert!(text.contains("Ready"), "{text}");
        // Planning never performs acquisition: no artifacts, no outcome.
        assert!(!text.contains("NoObservableBgpImpact"), "{text}");
    }

    #[test]
    fn malformed_manifest_is_distinct_from_blocked_plan() {
        let dir = tempfile::tempdir().unwrap();
        let malformed = write_manifest(dir.path(), "not json at all");
        let mut out = Cursor::new(Vec::new());
        let code = cmd_plan(&mut out, std::path::Path::new(TICKET), &malformed, None);
        assert_eq!(code, EXIT_INVALID_INPUT);

        // The blocked plan uses a different, documented exit code.
        let blocked = write_manifest(dir.path(), BLOCKED_MANIFEST);
        let mut out = Cursor::new(Vec::new());
        let code = cmd_plan(&mut out, std::path::Path::new(TICKET), &blocked, None);
        assert_eq!(code, EXIT_SUCCESS); // plan command: blocked still exits 0
        assert_ne!(EXIT_INVALID_INPUT, EXIT_ANALYSIS_BLOCKED);
        assert_ne!(EXIT_INVALID_INPUT, EXIT_SUCCESS);
    }

    #[test]
    fn exit_codes_are_documented_constants() {
        // The exit codes are named constants, never magic integers.
        assert_eq!(EXIT_SUCCESS, 0);
        assert_eq!(EXIT_INVALID_INPUT, 1);
        assert_eq!(EXIT_ANALYSIS_INCOMPLETE, 2);
        assert_eq!(EXIT_ANALYSIS_BLOCKED, 3);
    }
}

#[cfg(test)]
mod session32_cli_tests {
    use super::*;

    #[test]
    fn zero_jobs_is_rejected() {
        assert!(validate_jobs(0).is_err());
        assert!(validate_jobs(1).is_ok());
        assert!(validate_jobs(24).is_ok());
    }

    #[test]
    fn jobs_error_message_names_the_replacement() {
        let err = validate_jobs(0).unwrap_err();
        assert!(err.contains("--parse-jobs"), "{err}");
    }
}

// ── corpus CLI commands ────────────────────────────────

/// Offline fixture sync (existing behavior).
fn cmd_grnoc_sync_offline(
    stdout: &mut dyn Write,
    db: &std::path::Path,
    source_dir: &std::path::Path,
) -> i32 {
    let conn = match inim::catalog::db::open_catalog(db) {
        Ok(c) => c,
        Err(e) => {
            let _ = writeln!(stdout, "error: {e}");
            return EXIT_INVALID_INPUT;
        }
    };
    let fetched = chrono::Utc::now().to_rfc3339();
    let src =
        inim::catalog::grnoc::GrnocCatalogSource::new(source_dir.to_path_buf(), fetched.clone());
    match inim::catalog::sync::sync_catalog(&conn, &src, &fetched) {
        Ok(summary) => {
            let _ = writeln!(
                stdout,
                "grnoc sync complete: {} examined, {} new, {} changed, {} unchanged, {} failures",
                summary.events_examined,
                summary.new_events,
                summary.changed_events,
                summary.unchanged_events,
                summary.failures
            );
            EXIT_SUCCESS
        }
        Err(e) => {
            let _ = writeln!(stdout, "error: {e}");
            EXIT_INVALID_INPUT
        }
    }
}

/// Live viewer sync: discover frontier → polite fetch → link case-study
/// tickets → optional reference expansion.
#[allow(clippy::too_many_arguments)]
fn cmd_grnoc_sync_live(
    stdout: &mut dyn Write,
    db: &std::path::Path,
    seeds: &[String],
    case_studies: &[String],
    expand_references: bool,
    searches: &[String],
    domain: Option<&str>,
    max_requests: Option<usize>,
    requests_per_second: Option<f64>,
    allow_higher_rate: bool,
    contact: Option<&str>,
    dry_run: bool,
) -> i32 {
    use inim::catalog::access::{AccessPolicy, DEFAULT_MAX_REQUESTS};
    use inim::catalog::grnoc_viewer::{sync_frontier, GrnocViewerClient};

    let conn = match inim::catalog::db::open_catalog(db) {
        Ok(c) => c,
        Err(e) => {
            let _ = writeln!(stdout, "error: {e}");
            return EXIT_INVALID_INPUT;
        }
    };

    // ── Access policy ──────────────────────────────────────────────
    let mut policy = AccessPolicy::conservative();
    if let Some(rps) = requests_per_second {
        if rps > 5.0 && !allow_higher_rate {
            let _ = writeln!(
                stdout,
                "error: {rps} requests/second exceeds the reviewed ceiling of 5.0; pass --allow-higher-rate to confirm"
            );
            return EXIT_INVALID_INPUT;
        }
        policy.requests_per_second = rps;
    }
    policy.max_requests = max_requests.unwrap_or(DEFAULT_MAX_REQUESTS);
    if let Some(c) = contact {
        if !c.is_empty() {
            policy.contact = Some(c.to_string());
        }
    }
    if let Err(e) = policy.validate() {
        let _ = writeln!(stdout, "error: {e}");
        return EXIT_INVALID_INPUT;
    }

    // ── Bounded scoped search (Part E) ────────────────────────────
    // The viewer's search mechanism is used only with non-empty
    // reviewed queries; incident search requires a domain (unscoped
    // incident search returns 403). Every query is recorded below as
    // a discovery-source line, and results are recorded as frontier
    // events with exact provenance. ONE polite client is created and
    // shared with the frontier phase below so the reviewed request
    // budget is never doubled across phases.
    let source_kind = "grnoc-public-task-viewer";
    let now = chrono::Utc::now().to_rfc3339();
    let mut client = match GrnocViewerClient::new(policy.clone()) {
        Ok(c) => c,
        Err(e) => {
            let _ = writeln!(stdout, "error: {e}");
            return EXIT_INVALID_INPUT;
        }
    };
    if !searches.is_empty() {
        if dry_run {
            for q in searches {
                let _ = writeln!(stdout, "dry-run: would search {q:?} (domain {domain:?})");
            }
        } else {
            for q in searches {
                if client.budget_remaining() == 0 {
                    let _ = writeln!(stdout, "stop: request budget exhausted");
                    break;
                }
                let endpoint = if domain.is_some() {
                    "/api/get_incidents"
                } else {
                    "/api/get_change_requests"
                };
                let _ = writeln!(
                    stdout,
                    "search {q:?} via {endpoint} (domain {domain:?}, remaining budget {})",
                    client.budget_remaining()
                );
                match client.search(endpoint, q, domain, 20) {
                    Ok(records) => {
                        let _ = writeln!(stdout, "  -> {} records", records.len());
                        for rec in &records {
                            let number = rec.number.clone();
                            let title = rec.short_description.clone();
                            let _ = writeln!(stdout, "     {number}  {title}");
                            if let Err(e) = inim::catalog::discovery::record_analyst_seed(
                                &conn,
                                source_kind,
                                &number,
                                &now,
                            ) {
                                let _ = writeln!(stdout, "error: {e}");
                                return EXIT_INVALID_INPUT;
                            }
                        }
                    }
                    Err(e) => {
                        let _ = writeln!(stdout, "  search failed: {e}");
                        let _ = writeln!(
                            stdout,
                            "note: search stopped (403/429/budget); continuing with the frontier"
                        );
                        break;
                    }
                }
            }
        }
    }

    // ── Discovery frontier ─────────────────────────────────────────

    for seed in seeds {
        if let Err(e) =
            inim::catalog::discovery::record_analyst_seed(&conn, source_kind, seed, &now)
        {
            let _ = writeln!(stdout, "error: {e}");
            return EXIT_INVALID_INPUT;
        }
    }
    for slug in case_studies {
        let Some(cs) = inim::catalog::archive_plan::find_case_study(&conn, slug) else {
            let _ = writeln!(stdout, "error: no case study with slug '{slug}'");
            return EXIT_INVALID_INPUT;
        };
        match inim::catalog::discovery::record_case_study_references(
            &conn,
            source_kind,
            cs.id,
            &now,
        ) {
            Ok(n) => {
                let _ = writeln!(
                    stdout,
                    "case study '{slug}': {n} ticket references recorded"
                );
            }
            Err(e) => {
                let _ = writeln!(stdout, "error: {e}");
                return EXIT_INVALID_INPUT;
            }
        }
    }
    if expand_references {
        match inim::catalog::discovery::expand_from_snapshots(&conn, source_kind, &now) {
            Ok(n) => {
                let _ = writeln!(stdout, "reference expansion: {n} new discoveries");
            }
            Err(e) => {
                let _ = writeln!(stdout, "error: {e}");
                return EXIT_INVALID_INPUT;
            }
        }
    }
    let frontier = match inim::catalog::store::pending_frontier(&conn, source_kind) {
        Ok(f) => f,
        Err(e) => {
            let _ = writeln!(stdout, "error: {e}");
            return EXIT_INVALID_INPUT;
        }
    };
    if frontier.is_empty() {
        let _ = writeln!(
            stdout,
            "error: no pending tickets in the frontier; supply --seed, --case-study, or --expand-references"
        );
        return EXIT_INVALID_INPUT;
    }
    let _ = writeln!(
        stdout,
        "frontier: {} ticket(s) pending (budget {})",
        frontier.len(),
        policy.max_requests
    );

    // ── Dry run / policy display ───────────────────────────────────
    if dry_run {
        let _ = writeln!(stdout, "dry-run: no network access performed");
        for id in &frontier {
            let _ = writeln!(stdout, "  would fetch {id}");
        }
        let _ = writeln!(
            stdout,
            "  budget: {} requests; rate: {} req/s; concurrency: 1",
            policy.max_requests, policy.requests_per_second
        );
        return EXIT_SUCCESS;
    }

    let started_at = chrono::Utc::now().to_rfc3339();
    let wall_start = std::time::Instant::now();
    match sync_frontier(&conn, &mut client, source_kind, &frontier, &started_at) {
        Ok(summary) => {
            let elapsed = wall_start.elapsed();
            let rate = if elapsed.as_secs_f64() > 0.0 {
                summary.requests_made as f64 / elapsed.as_secs_f64()
            } else {
                0.0
            };
            let _ = writeln!(stdout, "corpus sync summary:");
            let _ = writeln!(stdout, "  examined:        {}", summary.examined);
            let _ = writeln!(stdout, "  new snapshots:   {}", summary.new_snapshots);
            let _ = writeln!(stdout, "  unchanged:       {}", summary.unchanged);
            let _ = writeln!(stdout, "  not modified:    {}", summary.not_modified);
            let _ = writeln!(stdout, "  not found:       {}", summary.not_found);
            let _ = writeln!(stdout, "  unsupported:     {}", summary.unsupported);
            let _ = writeln!(stdout, "  failures:        {}", summary.failures);
            let _ = writeln!(
                stdout,
                "  stopped:         {}",
                summary
                    .stopped
                    .as_ref()
                    .map(|s| format!("{s:?}"))
                    .unwrap_or_else(|| "no".to_string())
            );
            let _ = writeln!(stdout, "  requests:        {}", summary.requests_made);
            let _ = writeln!(stdout, "  bytes:           {}", client.bytes_transferred());
            let _ = writeln!(stdout, "  elapsed:         {:.1}s", elapsed.as_secs_f64());
            let _ = writeln!(stdout, "  avg rate:        {rate:.3} req/s");
            let _ = writeln!(
                stdout,
                "  configured rate: {:.1} req/s (ceiling)",
                client.policy().requests_per_second
            );
            let _ = writeln!(
                stdout,
                "  final effective rate: {:.2} req/s",
                client.metrics().final_effective_rps
            );
            let _ = writeln!(
                stdout,
                "  max in-flight:   {}",
                client.metrics().max_observed_inflight
            );
            let _ = writeln!(
                stdout,
                "  rate reductions: {} (adaptive)",
                client.metrics().rate_reductions
            );
            let _ = writeln!(
                stdout,
                "  rate recoveries: {} (bounded)",
                client.metrics().rate_recoveries
            );
            let _ = writeln!(
                stdout,
                "  retry-after:     {} response(s)",
                client.metrics().retry_after_responses
            );
            if !client.metrics().latencies_secs.is_empty() {
                let mut lat = client.metrics().latencies_secs.clone();
                lat.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                let p50 = lat[lat.len() / 2];
                let min = lat[0];
                let max = lat[lat.len() - 1];
                let _ = writeln!(
                    stdout,
                    "  latency:         min {:.0}ms / p50 {:.0}ms / max {:.0}ms ({:.1}s total)",
                    min * 1000.0,
                    p50 * 1000.0,
                    max * 1000.0,
                    lat.iter().sum::<f64>()
                );
            }
            for (status, n) in &client.metrics().status_counts {
                let _ = writeln!(stdout, "  http {status}:        {n}");
            }
            for msg in &client.metrics().control_messages {
                let _ = writeln!(stdout, "  control:         {msg}");
            }
            // Link retrieved tickets to their case-study references.
            for slug in case_studies {
                if let Some(cs) = inim::catalog::archive_plan::find_case_study(&conn, slug) {
                    match inim::catalog::grnoc_viewer::link_case_study_tickets(
                        &conn,
                        cs.id,
                        source_kind,
                    ) {
                        Ok(n) => {
                            let _ = writeln!(
                                stdout,
                                "case study '{slug}': {n} ticket link(s) resolved"
                            );
                        }
                        Err(e) => {
                            let _ = writeln!(stdout, "error: {e}");
                            return EXIT_INVALID_INPUT;
                        }
                    }
                }
            }
            if expand_references {
                // Second pass: references from the freshly fetched
                // descriptions enter the frontier for a later run.
                match inim::catalog::discovery::expand_from_snapshots(
                    &conn,
                    source_kind,
                    &chrono::Utc::now().to_rfc3339(),
                ) {
                    Ok(n) => {
                        let _ = writeln!(stdout, "reference expansion: {n} new discoveries");
                    }
                    Err(e) => {
                        let _ = writeln!(stdout, "error: {e}");
                        return EXIT_INVALID_INPUT;
                    }
                }
            }
            if summary.stopped.is_some() {
                EXIT_ANALYSIS_INCOMPLETE
            } else {
                EXIT_SUCCESS
            }
        }
        Err(e) => {
            let _ = writeln!(stdout, "error: {e}");
            EXIT_INVALID_INPUT
        }
    }
}

/// `inim catalog relationships rebuild` — re-extract explicit
/// relationships, resolve targets, derive overlap candidates, and
/// regenerate incident group candidates. Idempotent.
fn cmd_relationships_rebuild(stdout: &mut dyn Write, db: &std::path::Path) -> i32 {
    use inim::catalog::relationships;
    let conn = match inim::catalog::db::open_catalog(db) {
        Ok(c) => c,
        Err(e) => {
            let _ = writeln!(stdout, "error: {e}");
            return EXIT_INVALID_INPUT;
        }
    };
    let now = chrono::Utc::now().to_rfc3339();
    let source_kind = "grnoc-public-task-viewer";
    let extracted =
        match relationships::extract_relationships_from_snapshots(&conn, source_kind, &now) {
            Ok(n) => n,
            Err(e) => {
                let _ = writeln!(stdout, "error: {e}");
                return EXIT_INVALID_INPUT;
            }
        };
    let resolved = match relationships::resolve_unresolved_edges(&conn, source_kind) {
        Ok(n) => n,
        Err(e) => {
            let _ = writeln!(stdout, "error: {e}");
            return EXIT_INVALID_INPUT;
        }
    };
    let overlaps = match relationships::derive_temporal_overlaps(&conn, source_kind, &now) {
        Ok(n) => n,
        Err(e) => {
            let _ = writeln!(stdout, "error: {e}");
            return EXIT_INVALID_INPUT;
        }
    };
    let groups = match inim::catalog::grouping::generate_candidates(&conn, &now) {
        Ok(n) => n,
        Err(e) => {
            let _ = writeln!(stdout, "error: {e}");
            return EXIT_INVALID_INPUT;
        }
    };
    let _ = writeln!(
        stdout,
        "relationships rebuilt: {extracted} explicit edge(s) extracted, {resolved} target(s) resolved, {overlaps} overlap candidate(s), {groups} group candidate(s)"
    );
    EXIT_SUCCESS
}

/// `inim catalog relationships audit` — print the reviewed graph audit.
fn cmd_relationships_audit(stdout: &mut dyn Write, db: &std::path::Path, source_kind: &str) -> i32 {
    let conn = match inim::catalog::db::open_catalog(db) {
        Ok(c) => c,
        Err(e) => {
            let _ = writeln!(stdout, "error: {e}");
            return EXIT_INVALID_INPUT;
        }
    };
    let rows = match inim::catalog::review::graph_audit(&conn, source_kind) {
        Ok(r) => r,
        Err(e) => {
            let _ = writeln!(stdout, "error: {e}");
            return EXIT_INVALID_INPUT;
        }
    };
    let unresolved = rows.iter().filter(|r| !r.to_resolved).count();
    for r in &rows {
        let target = if r.to_resolved {
            r.to_external.clone()
        } else {
            format!("{} (unresolved reference)", r.to_external)
        };
        let _ = writeln!(
            stdout,
            "{} -> {} | {} | {} | source: {} | {}",
            r.from_external,
            target,
            r.relationship_kind,
            r.evidence_kind,
            r.exact_source,
            r.review_status
        );
    }
    let _ = writeln!(
        stdout,
        "graph audit: {} edge(s), {unresolved} unresolved reference(s)",
        rows.len()
    );
    EXIT_SUCCESS
}

/// `inim catalog corpus review <file>` — import reviewed interpretations
/// and reviewed relationship edges. Never modifies source snapshots.
fn cmd_corpus_review(
    stdout: &mut dyn Write,
    db: &std::path::Path,
    file: &std::path::Path,
    source_kind: &str,
) -> i32 {
    let conn = match inim::catalog::db::open_catalog(db) {
        Ok(c) => c,
        Err(e) => {
            let _ = writeln!(stdout, "error: {e}");
            return EXIT_INVALID_INPUT;
        }
    };
    let raw = match std::fs::read_to_string(file) {
        Ok(r) => r,
        Err(e) => {
            let _ = writeln!(stdout, "error: cannot read {}: {e}", file.display());
            return EXIT_INVALID_INPUT;
        }
    };
    let parsed: serde_json::Value = match serde_json::from_str(&raw) {
        Ok(v) => v,
        Err(e) => {
            let _ = writeln!(stdout, "error: invalid review file {}: {e}", file.display());
            return EXIT_INVALID_INPUT;
        }
    };
    let slug = parsed
        .get("case_study")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let reviewer = parsed
        .get("reviewer")
        .and_then(|v| v.as_str())
        .unwrap_or("analyst");
    let reviewed_at = parsed
        .get("reviewed_at")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if reviewed_at.is_empty() {
        let _ = writeln!(stdout, "error: review file needs a reviewed_at timestamp");
        return EXIT_INVALID_INPUT;
    }

    // Resolve the case study's reference document (AAR) for citations.
    let aar_document_id = if slug.is_empty() {
        None
    } else {
        match inim::catalog::archive_plan::find_case_study(&conn, slug) {
            Some(cs) => {
                match inim::catalog::review::case_study_document(&conn, cs.id, "AfterActionReport")
                {
                    Ok(d) => d,
                    Err(e) => {
                        let _ = writeln!(stdout, "error: {e}");
                        return EXIT_INVALID_INPUT;
                    }
                }
            }
            None => {
                let _ = writeln!(stdout, "error: no case study with slug '{slug}'");
                return EXIT_INVALID_INPUT;
            }
        }
    };

    use inim::catalog::review::{import_reviewed_edge, validate_review};
    use inim::catalog::store;

    let mut reviews = 0usize;
    let mut edges = 0usize;
    if let Some(list) = parsed.get("reviews").and_then(|v| v.as_array()) {
        for item in list {
            let entry: inim::catalog::review::ReviewFileEntry =
                match serde_json::from_value(item.clone()) {
                    Ok(r) => r,
                    Err(e) => {
                        let _ = writeln!(stdout, "error: invalid review entry: {e}");
                        return EXIT_INVALID_INPUT;
                    }
                };
            let review = entry.into_review(reviewer, reviewed_at);
            let review = match validate_review(&conn, source_kind, review, aar_document_id) {
                Ok(r) => r,
                Err(e) => {
                    let _ = writeln!(stdout, "error: {e}");
                    return EXIT_INVALID_INPUT;
                }
            };
            match store::upsert_ticket_review(&conn, &review) {
                Ok(_) => reviews += 1,
                Err(e) => {
                    let _ = writeln!(stdout, "error: {e}");
                    return EXIT_INVALID_INPUT;
                }
            }
        }
    }
    if let Some(list) = parsed.get("relationships").and_then(|v| v.as_array()) {
        for item in list {
            let from = item.get("from").and_then(|v| v.as_str()).unwrap_or("");
            let to = item.get("to").and_then(|v| v.as_str()).unwrap_or("");
            let kind = item.get("kind").and_then(|v| v.as_str()).unwrap_or("");
            let evidence = item.get("evidence").and_then(|v| v.as_str()).unwrap_or("");
            let note = item.get("note").and_then(|v| v.as_str()).map(String::from);
            let document_cited = item
                .get("document_cited")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            if from.is_empty() || to.is_empty() || kind.is_empty() || evidence.is_empty() {
                let _ = writeln!(
                    stdout,
                    "error: relationship entry needs from/to/kind/evidence"
                );
                return EXIT_INVALID_INPUT;
            }
            match import_reviewed_edge(
                &conn,
                source_kind,
                &inim::catalog::review::ReviewedEdgeInput {
                    from: from.to_string(),
                    to: to.to_string(),
                    kind: kind.to_string(),
                    evidence: evidence.to_string(),
                    note,
                    document_cited,
                },
                aar_document_id,
                reviewed_at,
            ) {
                Ok(true) => edges += 1,
                Ok(false) => {}
                Err(e) => {
                    let _ = writeln!(stdout, "error: {e}");
                    return EXIT_INVALID_INPUT;
                }
            }
        }
    }
    // Derive entity-overlap candidate edges from reviewed entity labels.
    let entity_overlaps = match inim::catalog::review::derive_entity_overlaps(&conn, reviewed_at) {
        Ok(n) => n,
        Err(e) => {
            let _ = writeln!(stdout, "error: {e}");
            return EXIT_INVALID_INPUT;
        }
    };
    let _ = writeln!(
        stdout,
        "review imported by {reviewer}: {reviews} ticket review(s), {edges} reviewed edge(s), {entity_overlaps} derived entity-overlap edge(s) (source snapshots unchanged)"
    );
    EXIT_SUCCESS
}

/// `inim catalog analysis-queue` — print the derived readiness queue.
#[allow(clippy::too_many_arguments)] // CLI arg passthrough; each maps to one flag
fn cmd_session_audit(
    stdout: &mut dyn Write,
    root: &std::path::Path,
    profile: Option<&std::path::Path>,
    locations: Option<&std::path::Path>,
    cache: &[String],
    date: &str,
    origin_asns: &str,
    extraction_cache: Option<&std::path::Path>,
    jobs: usize,
    full_inventory: bool,
    out: &std::path::Path,
) -> i32 {
    use inim::catalog::netprofile::{CollectorLocationRegistry, ServicePlaneProfile};
    use inim::catalog::session_audit::{
        run_peer_inventory, run_session_audit, SessionAuditOptions,
    };

    let profile_path = profile
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| root.join("network-profile.json"));
    let locations_path = locations
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| root.join("collector-locations.json"));
    let profile = match ServicePlaneProfile::load(&profile_path) {
        Ok(p) => p,
        Err(e) => {
            let _ = writeln!(stdout, "error: {e}");
            return EXIT_INVALID_INPUT;
        }
    };
    let registry = match CollectorLocationRegistry::load(&locations_path) {
        Ok(r) => r,
        Err(e) => {
            let _ = writeln!(stdout, "error: {e}");
            return EXIT_INVALID_INPUT;
        }
    };

    let mut caches: Vec<(std::path::PathBuf, String)> = Vec::new();
    for entry in cache {
        let Some((dir, family)) = entry.split_once(':') else {
            let _ = writeln!(stdout, "error: --cache expects DIR:FAMILY, got {entry:?}");
            return EXIT_INVALID_INPUT;
        };
        caches.push((std::path::PathBuf::from(dir), family.to_string()));
    }
    let origins: Vec<u32> = match origin_asns
        .split(',')
        .map(|s| s.trim().parse::<u32>())
        .collect::<Result<Vec<_>, _>>()
    {
        Ok(v) if !v.is_empty() => v,
        _ => {
            let _ = writeln!(stdout, "error: --origin-asns expects comma-separated ASNs");
            return EXIT_INVALID_INPUT;
        }
    };
    let extraction_cache = extraction_cache
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| caches[0].0.clone());

    let opts = SessionAuditOptions {
        profile,
        registry,
        caches,
        date: date.to_string(),
        origin_asns: origins,
        jobs,
        extraction_cache,
    };
    let json: String = if full_inventory {
        match run_peer_inventory(&opts) {
            Ok(rows) => match serde_json::to_string_pretty(&rows) {
                Ok(j) => j,
                Err(e) => {
                    let _ = writeln!(stdout, "error: cannot serialize peer inventory: {e}");
                    return EXIT_INVALID_INPUT;
                }
            },
            Err(e) => {
                let _ = writeln!(stdout, "error: {e}");
                return EXIT_INVALID_INPUT;
            }
        }
    } else {
        match run_session_audit(&opts) {
            Ok(rows) => {
                let j = match serde_json::to_string_pretty(&rows) {
                    Ok(j) => j,
                    Err(e) => {
                        let _ = writeln!(stdout, "error: cannot serialize audit: {e}");
                        return EXIT_INVALID_INPUT;
                    }
                };
                let _ = writeln!(
                    stdout,
                    "session audit: {} row(s) written to {}",
                    rows.len(),
                    out.display()
                );
                j
            }
            Err(e) => {
                let _ = writeln!(stdout, "error: {e}");
                return EXIT_INVALID_INPUT;
            }
        }
    };
    if let Some(parent) = out.parent() {
        if !parent.as_os_str().is_empty() {
            let _ = std::fs::create_dir_all(parent);
        }
    }
    if let Err(e) = std::fs::write(out, json) {
        let _ = writeln!(stdout, "error: cannot write {}: {e}", out.display());
        return EXIT_INVALID_INPUT;
    }
    EXIT_SUCCESS
}

fn cmd_analysis_queue(
    stdout: &mut dyn Write,
    db: &std::path::Path,
    state_filter: Option<&str>,
) -> i32 {
    let conn = match inim::catalog::db::open_catalog(db) {
        Ok(c) => c,
        Err(e) => {
            let _ = writeln!(stdout, "error: {e}");
            return EXIT_INVALID_INPUT;
        }
    };
    match inim::catalog::analyzability::derive_all_analyzability(&conn) {
        Ok(rows) => {
            let _ = writeln!(stdout, "{:<14} {:<28} REASON", "EVENT", "READINESS");
            for r in rows {
                if let Some(f) = state_filter {
                    if r.readiness != f {
                        continue;
                    }
                }
                let _ = writeln!(
                    stdout,
                    "{:<14} {:<28} {}",
                    r.external_id, r.readiness, r.reason
                );
            }
            EXIT_SUCCESS
        }
        Err(e) => {
            let _ = writeln!(stdout, "error: {e}");
            EXIT_INVALID_INPUT
        }
    }
}

/// `inim catalog archive-batches plan` — deterministic correlation
/// batches from stored case-study archive plans. Pure computation.
fn cmd_archive_batches_plan(stdout: &mut dyn Write, db: &std::path::Path) -> i32 {
    use inim::catalog::archive_plan::{AnalysisHorizon, ArchivePlan};
    use inim::catalog::batch::{plan_batch, EventPlanInput};
    let conn = match inim::catalog::db::open_catalog(db) {
        Ok(c) => c,
        Err(e) => {
            let _ = writeln!(stdout, "error: {e}");
            return EXIT_INVALID_INPUT;
        }
    };
    let mut stmt = match conn.prepare(
        "SELECT p.id, p.case_study_id, p.horizon_json, p.plan_json, c.slug, c.title
         FROM case_study_analysis_plans p JOIN case_studies c ON c.id = p.case_study_id
         ORDER BY c.slug",
    ) {
        Ok(s) => s,
        Err(e) => {
            let _ = writeln!(stdout, "error: {e}");
            return EXIT_INVALID_INPUT;
        }
    };
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(4)?,
                r.get::<_, String>(5)?,
            ))
        })
        .map_err(|e| format!("catalog read failed: {e}"));
    let rows = match rows {
        Ok(r) => r,
        Err(e) => {
            let _ = writeln!(stdout, "error: {e}");
            return EXIT_INVALID_INPUT;
        }
    };
    let mut batches = 0usize;
    for row in rows {
        let row = match row {
            Ok(r) => r,
            Err(e) => {
                let _ = writeln!(stdout, "error: {e}");
                return EXIT_INVALID_INPUT;
            }
        };
        let (plan_id, plan_json, horizon_json, slug, title) = row;
        let Ok(plan) = serde_json::from_str::<ArchivePlan>(&plan_json) else {
            let _ = writeln!(stdout, "case study '{slug}': stored plan unreadable");
            continue;
        };
        let Ok(horizon) = serde_json::from_str::<AnalysisHorizon>(&horizon_json) else {
            continue;
        };
        // Members: the case study's linked catalog events.
        let mut stmt2 = match conn.prepare(
            "SELECT external_identifier FROM case_study_event_links
             WHERE case_study_id = ?1 AND catalog_event_id IS NOT NULL ORDER BY sort_order",
        ) {
            Ok(s) => s,
            Err(e) => {
                let _ = writeln!(stdout, "error: {e}");
                return EXIT_INVALID_INPUT;
            }
        };
        let rows2 = stmt2
            .query_map([plan_id], |r| r.get::<_, String>(0))
            .map_err(|e| format!("catalog read failed: {e}"));
        let rows2 = match rows2 {
            Ok(r) => r,
            Err(e) => {
                let _ = writeln!(stdout, "error: {e}");
                return EXIT_INVALID_INPUT;
            }
        };
        let mut ids = Vec::new();
        for r in rows2 {
            match r {
                Ok(id) => ids.push(id),
                Err(e) => {
                    let _ = writeln!(stdout, "error: {e}");
                    return EXIT_INVALID_INPUT;
                }
            }
        }
        if ids.is_empty() {
            let _ = writeln!(stdout, "case study '{slug}': no linked events; skipped");
            continue;
        }
        let inputs: Vec<EventPlanInput> = ids
            .iter()
            .map(|id| EventPlanInput {
                event_id: id.clone(),
                horizon: horizon.clone(),
                plan: plan.clone(),
            })
            .collect();
        let batch = plan_batch(&inputs);
        let _ = writeln!(stdout, "batch for case study '{slug}' ({title}):");
        let _ = writeln!(stdout, "  batch id:        {}", batch.batch_id);
        let _ = writeln!(stdout, "  events:          {}", batch.events.len());
        let _ = writeln!(stdout, "  unique archives: {}", batch.unique_archives.len());
        let _ = writeln!(
            stdout,
            "  archives avoided through reuse: {}",
            batch.archives_avoided_through_reuse
        );
        let _ = writeln!(
            stdout,
            "  estimated bytes: {}",
            batch.estimated_compressed_bytes
        );
        let _ = writeln!(
            stdout,
            "  expected parse operations: {}",
            batch.expected_parse_operations
        );
        let _ = writeln!(
            stdout,
            "  source families: {}",
            batch.source_families.join(", ")
        );
        batches += 1;
    }
    let _ = writeln!(
        stdout,
        "{batches} batch plan(s) produced (deterministic; nothing downloaded)"
    );
    EXIT_SUCCESS
}

/// `inim catalog corpus export` — metadata-only export: no raw payloads
/// by default, only hashes, source URLs, and normalized fields.
fn cmd_corpus_export(
    stdout: &mut dyn Write,
    db: &std::path::Path,
    out: Option<&std::path::Path>,
) -> i32 {
    use inim::catalog::db as cdb;
    use inim::catalog::store;
    let conn = match inim::catalog::db::open_catalog(db) {
        Ok(c) => c,
        Err(e) => {
            let _ = writeln!(stdout, "error: {e}");
            return EXIT_INVALID_INPUT;
        }
    };
    let events = match cdb::list_events(&conn) {
        Ok(e) => e,
        Err(e) => {
            let _ = writeln!(stdout, "error: {e}");
            return EXIT_INVALID_INPUT;
        }
    };
    let mut export = serde_json::json!({
        "policy": "metadata-only export; raw payloads are excluded by default (redistribution review required before any payload export)",
        "events": [],
        "snapshots": [],
        "relationships": [],
        "discoveries": [],
    });
    let mut snapshots = Vec::new();
    let mut discoveries = Vec::new();
    for e in &events {
        export["events"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!({
                "external_id": e.external_id,
                "source_kind": e.source_kind,
                "first_seen": e.first_seen,
                "last_seen": e.last_seen,
            }));
        for s in cdb::list_snapshots(&conn, e.id).unwrap_or_default() {
            snapshots.push(serde_json::json!({
                "event_id": e.external_id,
                "fetched_at": s.fetched_at,
                "source_url": s.source_url,
                "content_sha256": s.content_sha256,
                "parser_version": s.parser_version,
            }));
        }
    }
    export["snapshots"] = serde_json::Value::Array(snapshots);
    export["relationships"] =
        serde_json::to_value(store::list_relationships(&conn, None).unwrap_or_default())
            .unwrap_or(serde_json::Value::Null);
    for d in store::list_discoveries(&conn, "grnoc-public-task-viewer", None).unwrap_or_default() {
        discoveries.push(serde_json::json!({
            "external_id": d.external_id,
            "provenance": d.provenance,
            "status": d.status,
            "discovered_at": d.discovered_at,
        }));
    }
    export["discoveries"] = serde_json::Value::Array(discoveries);
    let text = serde_json::to_string_pretty(&export).unwrap_or_default();
    match out {
        Some(path) => {
            if let Err(e) = std::fs::write(path, &text) {
                let _ = writeln!(stdout, "error: {e}");
                return EXIT_INVALID_INPUT;
            }
            let _ = writeln!(stdout, "metadata export written to {}", path.display());
        }
        None => {
            let _ = writeln!(stdout, "{text}");
        }
    }
    EXIT_SUCCESS
}

#[cfg(test)]
mod session33_cli_tests {
    use super::*;
    use std::io::Cursor;

    fn temp_db() -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("catalog.sqlite");
        inim::catalog::db::open_catalog(&path).unwrap();
        (dir, path)
    }

    #[test]
    fn sync_grnoc_parses_live_flags() {
        let args = vec![
            "inim",
            "catalog",
            "sync",
            "grnoc",
            "--db",
            "c.sqlite",
            "--seed",
            "CHG0099999",
            "--seed",
            "INC0040257",
            "--case-study",
            "manlan-2019",
            "--expand-references",
            "--max-requests",
            "10",
            "--requests-per-second",
            "0.5",
            "--allow-higher-rate",
            "--contact",
            "ops@example.invalid",
            "--dry-run",
        ];
        let cli = Cli::try_parse_from(args).unwrap();
        match cli.command {
            Commands::Catalog(CatalogCommands::Sync(SyncSource::Grnoc {
                db,
                seed,
                case_study,
                expand_references,
                search: _,
                domain: _,
                max_requests,
                requests_per_second,
                allow_higher_rate,
                contact,
                dry_run,
                source_dir,
                show_access_policy,
                show_domains: _,
            })) => {
                assert_eq!(db.to_string_lossy(), "c.sqlite");
                assert_eq!(seed, vec!["CHG0099999", "INC0040257"]);
                assert_eq!(case_study, vec!["manlan-2019"]);
                assert!(expand_references);
                assert_eq!(max_requests, Some(10));
                assert_eq!(requests_per_second, Some(0.5));
                assert!(allow_higher_rate);
                assert_eq!(contact.as_deref(), Some("ops@example.invalid"));
                assert!(dry_run);
                assert!(!show_access_policy);
                assert!(source_dir.is_none());
            }
            _ => panic!("unexpected command"),
        }
    }

    #[test]
    fn rate_above_five_requires_explicit_override() {
        let (_dir, db) = temp_db();
        // 6.0 requests/second requires the flag.
        let mut out = Cursor::new(Vec::new());
        let code = cmd_grnoc_sync_live(
            &mut out,
            &db,
            &["INC0040257".to_string()],
            &[],
            false,
            &[],
            None,
            None,
            Some(6.0),
            false,
            None,
            true,
        );
        assert_eq!(code, EXIT_INVALID_INPUT);
        let text = String::from_utf8(out.into_inner()).unwrap();
        assert!(text.contains("--allow-higher-rate"), "{text}");
        // With the flag the same rate is accepted (and the sync proceeds
        // to the frontier check — still no network because it is dry-run).
        let mut out2 = Cursor::new(Vec::new());
        let code2 = cmd_grnoc_sync_live(
            &mut out2,
            &db,
            &["INC0040257".to_string()],
            &[],
            false,
            &[],
            None,
            None,
            Some(6.0),
            true,
            None,
            true,
        );
        assert_ne!(code2, EXIT_INVALID_INPUT, "flag must permit the rate");
    }

    #[test]
    fn dry_run_shows_frontier_without_network() {
        let (_dir, db) = temp_db();
        let conn = inim::catalog::db::open_catalog(&db).unwrap();
        inim::catalog::discovery::record_analyst_seed(
            &conn,
            "grnoc-public-task-viewer",
            "INC0040257",
            "2026-08-01T00:00:00Z",
        )
        .unwrap();
        drop(conn);
        let mut out = Cursor::new(Vec::new());
        let code = cmd_grnoc_sync_live(
            &mut out,
            &db,
            &[],
            &[],
            false,
            &[],
            None,
            None,
            None,
            false,
            None,
            true,
        );
        assert_eq!(code, EXIT_SUCCESS);
        let text = String::from_utf8(out.into_inner()).unwrap();
        assert!(
            text.contains("dry-run: no network access performed"),
            "{text}"
        );
        assert!(text.contains("would fetch INC0040257"), "{text}");
    }

    #[test]
    fn live_mode_without_seed_sources_is_rejected() {
        let (_dir, db) = temp_db();
        let mut out = Cursor::new(Vec::new());
        let code = cmd_grnoc_sync_live(
            &mut out,
            &db,
            &[],
            &[],
            false,
            &[],
            None,
            None,
            None,
            false,
            None,
            true,
        );
        assert_eq!(code, EXIT_INVALID_INPUT);
        let text = String::from_utf8(out.into_inner()).unwrap();
        assert!(text.contains("no pending tickets"), "{text}");
        assert!(text.contains("--seed"), "{text}");
    }

    #[test]
    fn analysis_queue_renders_readiness() {
        let (_dir, db) = temp_db();
        let conn = inim::catalog::db::open_catalog(&db).unwrap();
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("e.json"),
            r#"{"number":"INC0099901","short_description":"t","start":"2026-07-28T04:35:00Z","end":"2026-07-28T05:00:00Z"}"#,
        )
        .unwrap();
        let src = inim::catalog::grnoc::GrnocCatalogSource::new(
            dir.path().to_path_buf(),
            "2026-08-01T00:00:00Z".into(),
        );
        inim::catalog::sync::sync_catalog(&conn, &src, "2026-08-01T00:00:00Z").unwrap();
        drop(conn);
        let mut out = Cursor::new(Vec::new());
        let code = cmd_analysis_queue(&mut out, &db, None);
        assert_eq!(code, EXIT_SUCCESS);
        let text = String::from_utf8(out.into_inner()).unwrap();
        assert!(text.contains("INC0099901"), "{text}");
        assert!(text.contains("NotReviewed"), "{text}");
        // State filter narrows the output.
        let mut out = Cursor::new(Vec::new());
        let _ = cmd_analysis_queue(&mut out, &db, Some("AnalysisComplete"));
        let text = String::from_utf8(out.into_inner()).unwrap();
        assert!(!text.contains("INC0099901"), "{text}");
    }

    #[test]
    fn corpus_export_is_metadata_only() {
        let (_dir, db) = temp_db();
        let conn = inim::catalog::db::open_catalog(&db).unwrap();
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("e.json"),
            r#"{"number":"INC0099902","short_description":"t","description":"secret payload","start":"2026-07-28T04:35:00Z","source_url":"https://ticket-viewer.grnoc.iu.edu/tickets/INC0099902/"}"#,
        )
        .unwrap();
        let src = inim::catalog::grnoc::GrnocCatalogSource::new(
            dir.path().to_path_buf(),
            "2026-08-01T00:00:00Z".into(),
        );
        inim::catalog::sync::sync_catalog(&conn, &src, "2026-08-01T00:00:00Z").unwrap();
        drop(conn);
        let export_dir = tempfile::tempdir().unwrap();
        let out_path = export_dir.path().join("export.json");
        let mut out = Cursor::new(Vec::new());
        let code = cmd_corpus_export(&mut out, &db, Some(&out_path));
        assert_eq!(code, EXIT_SUCCESS);
        let export: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&out_path).unwrap()).unwrap();
        assert_eq!(export["events"][0]["external_id"], "INC0099902");
        assert_eq!(
            export["snapshots"][0]["content_sha256"]
                .as_str()
                .unwrap()
                .len(),
            64
        );
        assert_eq!(
            export["snapshots"][0]["source_url"],
            "https://ticket-viewer.grnoc.iu.edu/tickets/INC0099902/"
        );
        // Raw payloads never appear in the metadata export.
        let text = serde_json::to_string(&export).unwrap();
        assert!(!text.contains("secret payload"), "{text}");
        assert!(!text.contains("raw_payload"), "{text}");
    }
}

// ── Analysis plan / job / worker / demo commands ───────────────────

fn cmd_analysis_plan(stdout: &mut dyn Write, command: &AnalysisPlanCommands) -> i32 {
    match command {
        AnalysisPlanCommands::Show { db, event, json } => {
            let conn = match inim::catalog::db::open_catalog(db) {
                Ok(c) => c,
                Err(e) => {
                    let _ = writeln!(stdout, "error: {e}");
                    return EXIT_INVALID_INPUT;
                }
            };
            let view = match inim::catalog::web::jobs_view::load_plan_review(&conn, event, false) {
                Ok(Some(v)) => v,
                Ok(None) => {
                    let _ = writeln!(stdout, "error: event not found: {event}");
                    return EXIT_INVALID_INPUT;
                }
                Err(e) => {
                    let _ = writeln!(stdout, "error: {e}");
                    return EXIT_INVALID_INPUT;
                }
            };
            if *json {
                let payload = serde_json::json!({
                    "event_id": view.event_id,
                    "plan_status": view.plan_status,
                    "block_reason": view.block_reason,
                    "plan_revision_id": view.plan_revision_id,
                    "plan_hash": view.plan_hash,
                    "ready_to_queue": view.ready_to_queue,
                    "reviewed": view.reviewed.iter().map(|r| (r.label.clone(), r.value.clone())).collect::<Vec<_>>(),
                    "derived": view.derived.iter().map(|r| (r.label.clone(), r.value.clone())).collect::<Vec<_>>(),
                    "unresolved": view.unresolved,
                    "schema_version": 1,
                });
                let _ = writeln!(
                    stdout,
                    "{}",
                    serde_json::to_string_pretty(&payload).unwrap_or_default()
                );
            } else {
                let _ = writeln!(stdout, "Analysis plan — {}", view.event_id);
                let _ = writeln!(
                    stdout,
                    "Plan status: {} {}",
                    view.plan_status,
                    if view.block_reason.is_empty() {
                        String::new()
                    } else {
                        format!("({})", view.block_reason)
                    }
                );
                if let Some(h) = &view.plan_hash {
                    let _ = writeln!(stdout, "Plan hash: {h}");
                }
                let _ = writeln!(stdout, "\nReviewed input:");
                for r in &view.reviewed {
                    let _ = writeln!(stdout, "  {}: {}", r.label, r.value);
                }
                let _ = writeln!(stdout, "\nDerived execution plan:");
                for r in &view.derived {
                    let _ = writeln!(stdout, "  {}: {}", r.label, r.value);
                }
                if !view.unresolved.is_empty() {
                    let _ = writeln!(stdout, "\nUnresolved requirements:");
                    for u in &view.unresolved {
                        let _ = writeln!(stdout, "  - {u}");
                    }
                }
                let _ = writeln!(stdout, "\nRead-only; no network access performed.");
            }
            EXIT_SUCCESS
        }
    }
}

fn cmd_analysis_job(stdout: &mut dyn Write, command: &AnalysisJobCommands) -> i32 {
    use inim::catalog::jobs::service as jobs;
    use inim::catalog::jobs::{JobState, RequestSource};
    match command {
        AnalysisJobCommands::Queue { db, plan } => {
            let conn = match inim::catalog::db::open_catalog(db) {
                Ok(c) => c,
                Err(e) => {
                    let _ = writeln!(stdout, "error: {e}");
                    return EXIT_INVALID_INPUT;
                }
            };
            let scope = match inim::catalog::scope::ProjectScope::load(
                std::path::Path::new("."),
            ) {
                Ok(s) => s,
                Err(e) => {
                    let _ = writeln!(stdout, "error: {e}");
                    return EXIT_INVALID_INPUT;
                }
            };
            let plan_hash =
                match inim::catalog::jobs::plan::validate_plan_for_queue(&conn, *plan, &scope) {
                    Ok(h) => h,
                    Err(e) => {
                        let _ = writeln!(stdout, "queue rejected: {e}");
                        if e.starts_with("invalid_plan") {
                            return EXIT_ANALYSIS_BLOCKED;
                        }
                        return EXIT_INVALID_INPUT;
                    }
                };
            match jobs::queue(&conn, *plan, RequestSource::Cli, &plan_hash, &scope) {
                Ok(jobs::QueueOutcome::Created(job_id)) => {
                    let _ = writeln!(stdout, "queued job {job_id} (plan revision {plan})");
                    let _ = writeln!(
                        stdout,
                        "mutates the catalog only; execution requires `inim worker`"
                    );
                    EXIT_SUCCESS
                }
                Ok(jobs::QueueOutcome::Duplicate(job_id)) => {
                    let _ = writeln!(stdout, "job already queued: {job_id}");
                    EXIT_QUEUE_CONFLICT
                }
                Err(e) => {
                    let _ = writeln!(stdout, "queue failed: {e}");
                    EXIT_INVALID_INPUT
                }
            }
        }
        AnalysisJobCommands::List { db, state } => {
            let conn = match inim::catalog::db::open_catalog(db) {
                Ok(c) => c,
                Err(e) => {
                    let _ = writeln!(stdout, "error: {e}");
                    return EXIT_INVALID_INPUT;
                }
            };
            let state_filter = match state {
                Some(s) => match JobState::parse_state(s) {
                    Ok(st) => Some(st),
                    Err(e) => {
                        let _ = writeln!(stdout, "error: {e}");
                        return EXIT_INVALID_INPUT;
                    }
                },
                None => None,
            };
            let jobs = match jobs::list(
                &conn,
                &jobs::JobFilter {
                    state: state_filter,
                    plan_revision_id: None,
                },
            ) {
                Ok(j) => j,
                Err(e) => {
                    let _ = writeln!(stdout, "error: {e}");
                    return EXIT_INVALID_INPUT;
                }
            };
            if jobs.is_empty() {
                let _ = writeln!(stdout, "no jobs");
                return EXIT_SUCCESS;
            }
            let _ = writeln!(
                stdout,
                "{:<20} {:<14} {:<10} REQUESTED-TIME",
                "JOB", "STATE", "STAGE"
            );
            for j in jobs {
                let _ = writeln!(
                    stdout,
                    "{:<20} {:<14} {:<10} {}",
                    j.id,
                    j.state.as_str(),
                    j.stage.clone().unwrap_or_default(),
                    j.requested_at
                );
            }
            EXIT_SUCCESS
        }
        AnalysisJobCommands::Show { db, job } => {
            let conn = match inim::catalog::db::open_catalog(db) {
                Ok(c) => c,
                Err(e) => {
                    let _ = writeln!(stdout, "error: {e}");
                    return EXIT_INVALID_INPUT;
                }
            };
            let j = match jobs::get(&conn, job) {
                Ok(j) => j,
                Err(e) => {
                    let _ = writeln!(stdout, "error: {e}");
                    return EXIT_INVALID_INPUT;
                }
            };
            let _ = writeln!(stdout, "job:        {}", j.id);
            let _ = writeln!(stdout, "state:      {}", j.state.as_str());
            let _ = writeln!(
                stdout,
                "stage:      {}",
                j.stage.clone().unwrap_or_default()
            );
            let _ = writeln!(
                stdout,
                "plan:       {} (hash {})",
                j.plan_revision_id, j.plan_hash
            );
            let _ = writeln!(
                stdout,
                "requested:  {} ({})",
                j.requested_at, j.requested_by
            );
            let _ = writeln!(stdout, "attempt:    {}", j.attempt);
            if let Some(o) = &j.original_job_id {
                let _ = writeln!(stdout, "retry of:   {o}");
            }
            if let Some(w) = &j.worker_id {
                let _ = writeln!(stdout, "worker:     {w}");
            }
            if let Some(r) = j.completed_run_id {
                let _ = writeln!(stdout, "run:        {r}");
            }
            if let Some(code) = &j.error_code {
                let _ = writeln!(
                    stdout,
                    "error:      {code} — {}",
                    j.error_summary.clone().unwrap_or_default()
                );
            }
            let _ = writeln!(stdout, "\nrecent events:");
            for ev in jobs::events(&conn, job, 10).unwrap_or_default() {
                let _ = writeln!(
                    stdout,
                    "  #{} {} {} — {}",
                    ev.sequence,
                    ev.occurred_at,
                    ev.state.as_str(),
                    ev.human_message
                );
            }
            EXIT_SUCCESS
        }
        AnalysisJobCommands::Cancel { db, job } => {
            let conn = match inim::catalog::db::open_catalog(db) {
                Ok(c) => c,
                Err(e) => {
                    let _ = writeln!(stdout, "error: {e}");
                    return EXIT_INVALID_INPUT;
                }
            };
            match jobs::request_cancel(&conn, job) {
                Ok(jobs::CancelOutcome::Cancelled(_)) => {
                    let _ = writeln!(stdout, "job {job} cancelled (was queued)");
                    EXIT_SUCCESS
                }
                Ok(jobs::CancelOutcome::Requested(_)) => {
                    let _ = writeln!(
                        stdout,
                        "cancellation requested for {job}; the worker stops at the next checkpoint"
                    );
                    EXIT_SUCCESS
                }
                Err(e) => {
                    let _ = writeln!(stdout, "cancel failed: {e}");
                    EXIT_INVALID_INPUT
                }
            }
        }
        AnalysisJobCommands::Retry { db, job } => {
            let conn = match inim::catalog::db::open_catalog(db) {
                Ok(c) => c,
                Err(e) => {
                    let _ = writeln!(stdout, "error: {e}");
                    return EXIT_INVALID_INPUT;
                }
            };
            let scope = match inim::catalog::scope::ProjectScope::load(
                std::path::Path::new("."),
            ) {
                Ok(s) => s,
                Err(e) => {
                    let _ = writeln!(stdout, "error: {e}");
                    return EXIT_INVALID_INPUT;
                }
            };
            let plan_hash = match jobs::get(&conn, job) {
                Ok(j) => j.plan_hash,
                Err(e) => {
                    let _ = writeln!(stdout, "error: {e}");
                    return EXIT_INVALID_INPUT;
                }
            };
            match jobs::retry(&conn, job, RequestSource::Cli, &plan_hash, &scope) {
                Ok(new_id) => {
                    let _ = writeln!(stdout, "retry created: {new_id} (attempt of {job})");
                    EXIT_SUCCESS
                }
                Err(e) => {
                    let _ = writeln!(stdout, "retry failed: {e}");
                    EXIT_INVALID_INPUT
                }
            }
        }
        AnalysisJobCommands::Cleanup {
            db,
            root,
            older_than,
            apply,
        } => {
            let conn = match inim::catalog::db::open_catalog(db) {
                Ok(c) => c,
                Err(e) => {
                    let _ = writeln!(stdout, "error: {e}");
                    return EXIT_INVALID_INPUT;
                }
            };
            let threshold = match inim::catalog::jobs::cleanup::parse_older_than(older_than) {
                Ok(d) => d,
                Err(e) => {
                    let _ = writeln!(stdout, "error: {e}");
                    return EXIT_INVALID_INPUT;
                }
            };
            match inim::catalog::jobs::cleanup::cleanup(&conn, root, threshold, *apply) {
                Ok(report) => {
                    let _ = write!(stdout, "{}", inim::catalog::jobs::cleanup::render(&report));
                    if *apply {
                        let _ = writeln!(
                            stdout,
                            "cleanup applied: {} deleted, {} refused",
                            report.deleted.len(),
                            report.refused.len()
                        );
                    } else {
                        let _ = writeln!(
                            stdout,
                            "dry-run: {} eligible; pass --apply to delete",
                            report.proposals.len()
                        );
                    }
                    EXIT_SUCCESS
                }
                Err(e) => {
                    let _ = writeln!(stdout, "cleanup failed: {e}");
                    EXIT_INVALID_INPUT
                }
            }
        }
        AnalysisJobCommands::Audit { db, root } => {
            let conn = match inim::catalog::db::open_catalog(db) {
                Ok(c) => c,
                Err(e) => {
                    let _ = writeln!(stdout, "error: {e}");
                    return EXIT_INVALID_INPUT;
                }
            };
            let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
            let stale = jobs::mark_stale_leases(&conn, &now).unwrap_or_default();
            for id in &stale {
                let _ = writeln!(
                    stdout,
                    "stale lease expired: {id} (Failed/worker_lease_expired; staging preserved)"
                );
            }
            match inim::catalog::jobs::publish::reconcile_orphans(&conn, root) {
                Ok(rep) => {
                    for d in &rep.orphan_directories {
                        let _ = writeln!(
                            stdout,
                            "orphan final directory: {} (unreferenced; not deleted)",
                            d.display()
                        );
                    }
                    for a in &rep.missing_run_artifacts {
                        let _ = writeln!(stdout, "missing run artifact: {a}");
                    }
                    if stale.is_empty()
                        && rep.orphan_directories.is_empty()
                        && rep.missing_run_artifacts.is_empty()
                    {
                        let _ = writeln!(
                            stdout,
                            "no stale leases, no orphan directories, no missing run artifacts"
                        );
                    }
                    EXIT_SUCCESS
                }
                Err(e) => {
                    let _ = writeln!(stdout, "orphan scan failed: {e}");
                    EXIT_INVALID_INPUT
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)] // one worker launch; each flag maps to one config field
fn cmd_worker(
    db: &std::path::Path,
    root: &std::path::Path,
    worker_id: Option<&str>,
    poll_interval: &str,
    max_jobs: usize,
    download_jobs: usize,
    parse_jobs: usize,
    once: bool,
    offline: bool,
    show_execution_plan: bool,
    keep_failed_workdir: bool,
) -> i32 {
    let poll = parse_duration_secs(poll_interval).unwrap_or(2);
    let config = inim::worker::WorkerConfig {
        db_path: db.to_path_buf(),
        root: root.to_path_buf(),
        worker_id: worker_id.map(|s| s.to_string()),
        poll_interval: std::time::Duration::from_secs(poll),
        max_jobs,
        download_jobs,
        parse_jobs,
        once,
        offline,
        lease_secs: inim::catalog::jobs::service::DEFAULT_LEASE_SECS,
        heartbeat_secs: inim::catalog::jobs::service::DEFAULT_HEARTBEAT_SECS,
        keep_failed_workdir,
        show_execution_plan,
    };
    let code = inim::worker::run_worker(&config);
    if code != 0 && code != 2 {
        return EXIT_WORKER_FAILURE;
    }
    code
}

fn parse_duration_secs(s: &str) -> Option<u64> {
    let t = s.trim();
    if let Some(secs) = t.strip_suffix('s') {
        return secs.trim().parse().ok();
    }
    if let Some(ms) = t.strip_suffix("ms") {
        return ms
            .trim()
            .parse::<u64>()
            .ok()
            .map(|v| v / 1000)
            .filter(|v| *v > 0);
    }
    t.parse().ok()
}

fn cmd_demo(stdout: &mut dyn Write, command: &DemoCommands) -> i32 {
    match command {
        DemoCommands::Init { db, root, force } => {
            match inim::catalog::demo::demo_init(db, root, *force) {
                Ok(report) => {
                    let _ = write!(stdout, "{}", inim::catalog::demo::render_report(&report));
                    if report.is_ok() {
                        let _ = writeln!(stdout, "demo catalog ready at {}", db.display());
                        EXIT_SUCCESS
                    } else {
                        EXIT_INVALID_INPUT
                    }
                }
                Err(e) => {
                    let _ = writeln!(stdout, "demo init failed: {e}");
                    EXIT_INVALID_INPUT
                }
            }
        }
        DemoCommands::Verify { db, root } => match inim::catalog::demo::demo_verify(db, root) {
            Ok(report) => {
                let _ = write!(stdout, "{}", inim::catalog::demo::render_report(&report));
                if report.is_ok() {
                    EXIT_SUCCESS
                } else {
                    EXIT_INVALID_INPUT
                }
            }
            Err(e) => {
                let _ = writeln!(stdout, "demo verify failed: {e}");
                EXIT_INVALID_INPUT
            }
        },
    }
}

// ── Project-scope CLI (read-only) ──────────────────────────────────

fn cmd_project_scope(stdout: &mut dyn Write, command: &ProjectScopeCommands) -> i32 {
    use inim::catalog::scope::ProjectScope;
    match command {
        ProjectScopeCommands::Show { root } => {
            let scope = match ProjectScope::load(root) {
                Ok(s) => s,
                Err(e) => {
                    let _ = writeln!(stdout, "error: {e}");
                    return EXIT_INVALID_INPUT;
                }
            };
            let _ = writeln!(stdout, "project-scope policy (schema v{})", scope.schema_version());
            let _ = writeln!(stdout, "  excluded entities: {}", scope.entities().len());
            for e in scope.entities() {
                let _ = writeln!(
                    stdout,
                    "    {} ({}; asns {:?}; reason {})",
                    e.reviewed_name, e.stable_key, e.reviewed_asns, e.reason_code
                );
            }
            let _ = writeln!(
                stdout,
                "  excluded source records: {}",
                scope.source_records().len()
            );
            for r in scope.source_records() {
                let _ = writeln!(
                    stdout,
                    "    {} / {} (reason {})",
                    r.source_family, r.external_id, r.reason_code
                );
            }
            let _ = writeln!(
                stdout,
                "read-only: the tracked file is the reviewed authority; no catalog access"
            );
            EXIT_SUCCESS
        }
        ProjectScopeCommands::Audit { db, root } => {
            let scope = match ProjectScope::load(root) {
                Ok(s) => s,
                Err(e) => {
                    let _ = writeln!(stdout, "error: {e}");
                    return EXIT_INVALID_INPUT;
                }
            };
            let conn = match inim::catalog::db::open_catalog(db) {
                Ok(c) => c,
                Err(e) => {
                    let _ = writeln!(stdout, "error: {e}");
                    return EXIT_INVALID_INPUT;
                }
            };
            // Read-only counts derived GENERICALLY from the policy
            // entries. Nothing is deleted; completed jobs/runs stay
            // immutable. Excluded events are counted per exact source
            // record; plans/jobs/runs/artifacts are counted through the
            // event join.
            let mut excluded_events = 0i64;
            for rec in scope.source_records() {
                let n: i64 = conn
                    .query_row(
                        "SELECT COUNT(*) FROM catalog_events WHERE external_id = ?1",
                        rusqlite::params![rec.external_id],
                        |r| r.get(0),
                    )
                    .unwrap_or(0);
                excluded_events += n;
            }
            let mut excluded_plans = 0i64;
            let mut excluded_jobs = 0i64;
            let mut excluded_runs = 0i64;
            let mut excluded_artifacts = 0i64;
            for rec in scope.source_records() {
                let scope_plans: i64 = conn
                    .query_row(
                        "SELECT COUNT(*) FROM analysis_plans p
                         JOIN manifest_revisions m ON m.id = p.manifest_revision_id
                         JOIN catalog_events e ON e.id = m.event_id
                         WHERE e.external_id = ?1",
                        rusqlite::params![rec.external_id],
                        |r| r.get(0),
                    )
                    .unwrap_or(0);
                let scope_jobs: i64 = conn
                    .query_row(
                        "SELECT COUNT(*) FROM analysis_jobs j
                         JOIN analysis_plans p ON p.id = j.plan_revision_id
                         JOIN manifest_revisions m ON m.id = p.manifest_revision_id
                         JOIN catalog_events e ON e.id = m.event_id
                         WHERE e.external_id = ?1",
                        rusqlite::params![rec.external_id],
                        |r| r.get(0),
                    )
                    .unwrap_or(0);
                let scope_runs: i64 = conn
                    .query_row(
                        "SELECT COUNT(*) FROM analysis_runs r
                         JOIN analysis_plans p ON p.id = r.plan_id
                         JOIN manifest_revisions m ON m.id = p.manifest_revision_id
                         JOIN catalog_events e ON e.id = m.event_id
                         WHERE e.external_id = ?1",
                        rusqlite::params![rec.external_id],
                        |r| r.get(0),
                    )
                    .unwrap_or(0);
                let scope_artifacts: i64 = conn
                    .query_row(
                        "SELECT COUNT(*) FROM analysis_artifacts a
                         JOIN analysis_runs r ON r.id = a.run_id
                         JOIN analysis_plans p ON p.id = r.plan_id
                         JOIN manifest_revisions m ON m.id = p.manifest_revision_id
                         JOIN catalog_events e ON e.id = m.event_id
                         WHERE e.external_id = ?1",
                        rusqlite::params![rec.external_id],
                        |r| r.get(0),
                    )
                    .unwrap_or(0);
                excluded_plans += scope_plans;
                excluded_jobs += scope_jobs;
                excluded_runs += scope_runs;
                excluded_artifacts += scope_artifacts;
            }
            let _ = writeln!(stdout, "project-scope audit (read-only; nothing deleted)");
            let _ = writeln!(stdout, "  policy schema:              v{}", scope.schema_version());
            let _ = writeln!(
                stdout,
                "  excluded source records:    {}",
                scope.source_records().len()
            );
            let _ = writeln!(
                stdout,
                "  excluded entities:          {}",
                scope.entities().len()
            );
            let _ = writeln!(stdout, "  excluded events in catalog: {excluded_events}");
            let _ = writeln!(stdout, "  excluded plans in catalog:  {excluded_plans}");
            let _ = writeln!(stdout, "  excluded jobs in catalog:   {excluded_jobs}");
            let _ = writeln!(stdout, "  excluded runs in catalog:   {excluded_runs}");
            let _ = writeln!(stdout, "  excluded artifacts:         {excluded_artifacts}");
            let _ = writeln!(
                stdout,
                "  behavior: excluded items are hidden from default web/API/candidate views; queued jobs whose event is excluded are cancelled by the worker before source access"
            );
            EXIT_SUCCESS
        }
    }
}
