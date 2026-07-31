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
    ///
    /// Without --manifest: runs a built-in synthetic demonstration.
    /// With --manifest: executes real analysis using discovered RouteViews data.
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

        /// Force rebuild of all derived caches (ignore and overwrite).
        #[arg(long, default_value_t = false)]
        rebuild_derived_cache: bool,

        /// Number of parallel parsing jobs (1=serial, 0=auto, default: 1).
        #[arg(short = 'j', long, default_value_t = 1)]
        jobs: usize,
    },
}

fn main() {
    let cli = Cli::parse();

    match &cli.command {
        Commands::Analyze {
            event,
            manifest,
            cache,
            out,
            no_derived_cache,
            rebuild_derived_cache,
            jobs,
        } => {
            if let Some(manifest_path) = manifest {
                let discovery = inim::discover::LiveArchiveDiscovery;
                let cache_control = inim::orchestrate::CacheControl {
                    no_derived_cache: *no_derived_cache,
                    rebuild_derived_cache: *rebuild_derived_cache,
                    jobs: *jobs,
                };
                let outcome = inim::orchestrate::run_real_analysis(
                    event,
                    manifest_path,
                    cache,
                    out,
                    &discovery,
                    cache_control,
                );

                let json = serde_json::to_string_pretty(&outcome).unwrap_or_default();
                println!("{json}");
                if matches!(outcome, inim::outcome::AnalysisOutcome::Incomplete { .. }) {
                    std::process::exit(2);
                }
            } else {
                run_analyze_synthetic(event, cache, out);
            }
        }
    }
}

fn run_analyze_synthetic(event_path: &std::path::Path, _cache: &PathBuf, _out: &PathBuf) {
    // ── 1. Parse the Internet2 ticket fixture ────────────────────
    let ticket = match i2ticket::parse_ticket_fixture(event_path.to_string_lossy().as_ref()) {
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
    );

    // ── 7. Render reports ───────────────────────────────────────
    let data_note = "SYNTHETIC (no --manifest provided)";
    let terminal_report = report::render_terminal(&assessment, data_note);

    println!("{terminal_report}");

    let json_report = report::render_json(&assessment, data_note);
    let json_str = serde_json::to_string_pretty(&json_report).unwrap_or_default();
    println!("--- JSON ---");
    println!("{json_str}");
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
        }
    }
}
