//! inim — Internetwork Impact Monitor
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

/// Internetwork Impact Monitor — determine how network events affect
/// the globally visible routing system.
#[derive(Parser)]
#[command(name = "inim")]
#[command(version = "0.1.0")]
#[command(about = "Internetwork Impact Monitor", long_about = None)]
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
    /// Serve the read-only localhost catalog web UI.
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

        /// Force rebuild of all derived caches (ignore and overwrite).
        #[arg(long, default_value_t = false)]
        rebuild_derived_cache: bool,

        /// Number of parallel parsing jobs (1=serial, 0=auto, default: 1).
        #[arg(short = 'j', long, default_value_t = 1)]
        jobs: usize,
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
        /// Repository root containing manifests/ and out/.
        #[arg(long, value_name = "DIR", default_value = ".")]
        root: PathBuf,
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
    /// GRNOC Public Task Viewer records (one JSON file per ticket).
    Grnoc {
        #[arg(long, value_name = "PATH")]
        db: PathBuf,
        /// Directory containing GRNOC JSON records.
        #[arg(long, value_name = "DIR")]
        source_dir: PathBuf,
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
        Commands::Serve {
            db,
            root,
            bind,
            allow_non_loopback,
        } => cmd_serve(&mut std::io::stdout(), db, root, bind, *allow_non_loopback),
        Commands::Analyze {
            event,
            manifest,
            cache,
            out,
            no_derived_cache,
            rebuild_derived_cache,
            jobs,
            preflight_only,
        } => {
            let discovery = inim::discover::LiveArchiveDiscovery;
            let cache_control = inim::orchestrate::CacheControl {
                no_derived_cache: *no_derived_cache,
                rebuild_derived_cache: *rebuild_derived_cache,
                jobs: *jobs,
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
    );

    if preflight_only {
        // Stage A: the preflight JSON was already printed by the runner;
        // do not emit an analysis outcome or write outputs.
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
            match inim::catalog::archive_plan::build_plan(
                &cs,
                &targets,
                *warmup_hours,
                *cooldown_hours,
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
        CatalogCommands::Sync(source) => match source {
            SyncSource::Grnoc { db, source_dir } => {
                let conn = match inim::catalog::db::open_catalog(db) {
                    Ok(c) => c,
                    Err(e) => {
                        let _ = writeln!(stdout, "error: {e}");
                        return EXIT_INVALID_INPUT;
                    }
                };
                let fetched = chrono::Utc::now().to_rfc3339();
                let src = inim::catalog::grnoc::GrnocCatalogSource::new(
                    source_dir.clone(),
                    fetched.clone(),
                );
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
        },
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
