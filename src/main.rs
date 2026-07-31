//! inim — Internetwork Impact Monitor
//!
//! CLI entry point. Parses commands and orchestrates analysis.

use clap::{Parser, Subcommand};
use std::path::PathBuf;

use inim::assess;
use inim::fixtures;
use inim::report;
use inim::routes;
use inim::sources::internet2::ticket as i2ticket;
use inim::tokenize;
use inim::waves;

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
    /// Analyze a single operational event against BGP observations.
    Analyze {
        /// Path to the event fixture JSON file.
        #[arg(short, long, value_name = "PATH")]
        event: PathBuf,

        /// Path to a RIB MRT file for baseline state.
        #[arg(short, long, value_name = "PATH")]
        rib: Option<PathBuf>,

        /// Directory containing UPDATE MRT files.
        #[arg(short, long, value_name = "DIR")]
        updates: Option<PathBuf>,

        /// Path to write the JSON report (default: stdout).
        #[arg(short, long, value_name = "PATH")]
        output: Option<PathBuf>,

        /// Collector identifier (e.g. "route-views2").
        #[arg(long, default_value = "route-views2")]
        collector: String,
    },
}

fn main() {
    let cli = Cli::parse();

    match &cli.command {
        Commands::Analyze {
            event,
            rib,
            updates,
            output,
            collector,
        } => {
            run_analyze(event, rib.as_ref(), updates.as_ref(), output.as_ref(), collector);
        }
    }
}

fn run_analyze(
    event_path: &std::path::Path,
    _rib_path: Option<&std::path::PathBuf>,
    _updates_dir: Option<&std::path::PathBuf>,
    output_path: Option<&std::path::PathBuf>,
    collector: &str,
) {
    // ── 1. Parse the Internet2 ticket fixture ────────────────────
    let ticket = match i2ticket::parse_ticket_fixture(
        event_path.to_string_lossy().as_ref(),
    ) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("Error parsing ticket: {e}");
            std::process::exit(1);
        }
    };

    let expectation = i2ticket::derive_expectation(&ticket);

    println!("inim: analyze command parsed.");
    println!("  event:   {}", event_path.display());
    println!("  expectation: {:?}", expectation.kind);

    // ── 2. Ingest observations ──────────────────────────────────
    // TODO: when --rib and --updates are provided, use ingest::ObservationStream.
    // For now, use synthetic observations for the demo/vertical slice.

    // ── 3. Reconstruct route state ──────────────────────────────
    // For the demo: use the built-in redundant scenario

    println!("  Using synthetic observations for demonstration.");
    println!();

    let (store, changes) = build_demo_scenario(collector);

    use inim::domain::route::Continuity;
    let any_unknown = changes.iter().any(|sc| sc.continuity == Continuity::Unknown);

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
    );

    // ── 7. Render reports ───────────────────────────────────────
    let data_note = if _rib_path.is_some() || _updates_dir.is_some() {
        "not yet wired to ingest; using synthetic data"
    } else {
        "SYNTHETIC (no --rib/--updates provided)"
    };
    let terminal_report = report::render_terminal(&assessment, data_note);

    println!("{terminal_report}");

    let json_report = report::render_json(&assessment, data_note);

    if let Some(out_path) = output_path {
        let json_str = serde_json::to_string_pretty(&json_report).unwrap_or_default();
        if let Err(e) = std::fs::write(out_path, &json_str) {
            eprintln!("Error writing JSON report: {e}");
            std::process::exit(1);
        }
        println!("JSON report written to: {}", out_path.display());
    }

    // Also print JSON to stdout if no output file specified (or always, for demo)
    if output_path.is_none() {
        println!();
        println!("--- JSON ---");
        println!(
            "{}",
            serde_json::to_string_pretty(&json_report).unwrap_or_default()
        );
    }
}

/// Build a demonstration scenario for the vertical slice:
/// redundant maintenance — baseline → alternate → stable → restore.
fn build_demo_scenario(
    collector: &str,
) -> (inim::routes::RouteStateStore, Vec<inim::domain::route::StateChange>) {
    use chrono::{TimeZone, Utc};

    let event_start = Utc.with_ymd_and_hms(2025, 6, 15, 1, 0, 0).unwrap();
    let event_end = Utc.with_ymd_and_hms(2025, 6, 15, 6, 0, 0).unwrap();

    let obs = vec![
        // Two observer perspectives (rv2:AS6447 and rv6:AS6447)
        fixtures::make_synthetic_rib(
            "192.0.2.0/24", collector, "185.1.8.65", 6447,
            vec![6447, 11537, 1101], // baseline: via AS11537
            Utc.with_ymd_and_hms(2025, 6, 15, 0, 50, 0).unwrap(), 0,
        ),
        fixtures::make_synthetic_rib(
            "192.0.2.0/24", collector, "2001:7f8:4::1", 6447,
            vec![6447, 11537, 1101],
            Utc.with_ymd_and_hms(2025, 6, 15, 0, 50, 0).unwrap(), 1,
        ),
        // Warm-up: pre-event alternate announcement (should not emit transition)
        fixtures::make_synthetic_announcement(
            "192.0.2.0/24", collector, "185.1.8.65", 6447,
            vec![6447, 11537, 1101], // same as baseline, warm-up
            Utc.with_ymd_and_hms(2025, 6, 15, 0, 55, 0).unwrap(), 2,
        ),
        // Event: failover to alternate path (path change)
        fixtures::make_synthetic_announcement(
            "192.0.2.0/24", collector, "185.1.8.65", 6447,
            vec![6447, 237, 1101], // alternate: via AS237
            Utc.with_ymd_and_hms(2025, 6, 15, 1, 2, 14).unwrap(), 3,
        ),
        fixtures::make_synthetic_announcement(
            "192.0.2.0/24", collector, "2001:7f8:4::1", 6447,
            vec![6447, 237, 1101], // second peer sees same alternate
            Utc.with_ymd_and_hms(2025, 6, 15, 1, 2, 18).unwrap(), 4,
        ),
        // Restoration: back to baseline path
        fixtures::make_synthetic_announcement(
            "192.0.2.0/24", collector, "185.1.8.65", 6447,
            vec![6447, 11537, 1101], // restore to baseline
            Utc.with_ymd_and_hms(2025, 6, 15, 5, 51, 44).unwrap(), 5,
        ),
        fixtures::make_synthetic_announcement(
            "192.0.2.0/24", collector, "2001:7f8:4::1", 6447,
            vec![6447, 11537, 1101], // second peer restores
            Utc.with_ymd_and_hms(2025, 6, 15, 5, 53, 11).unwrap(), 6,
        ),
    ];

    routes::reconstruct_routes(obs, event_start, event_end)
}
