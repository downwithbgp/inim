//! inim — Internetwork Impact Monitor
//!
//! CLI entry point. Parses commands and orchestrates analysis.

use clap::{Parser, Subcommand};
use std::path::PathBuf;

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
        } => {
            println!("inim: analyze command parsed.");
            println!("  event:   {}", event.display());
            println!("  rib:     {}", rib.as_ref().map_or("none".into(), |p| p.display().to_string()));
            println!("  updates: {}", updates.as_ref().map_or("none".into(), |p| p.display().to_string()));
            println!("  output:  {}", output.as_ref().map_or("stdout".into(), |p| p.display().to_string()));

            // TODO: Implement the full analysis pipeline:
            // 1. Parse ticket fixture via Internet2 adapter
            // 2. Seed RIB state from MRT files
            // 3. Apply UPDATE records in timestamp order
            // 4. Tokenize route transitions
            // 5. Run SEQUITUR for motif discovery
            // 6. Detect impact waves
            // 7. Compare expectation vs observation
            // 8. Render terminal and JSON reports
        }
    }
}
