//! Repository tasks. `cargo xtask <command>` runs one.

mod check;
mod fuzz;
mod iperf;
mod record;
mod shared;

use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(about = "Repository tasks", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run the checks with every feature on and then off.
    Check {
        /// Run every check, over each feature alone and each pair, as CI does.
        #[arg(long)]
        full: bool,
    },
    /// Record the samples from the iperf 2.2.2 binary that `MCU_IPERF2` names.
    Record {
        /// Record only these cases. Every case is recorded when none is named.
        only: Vec<String>,
    },
    /// Build iperf 2.2.2 at the pinned commit into ~/.local/opt/iperf/2.2.2/iperf, for MCU_IPERF2.
    InstallIperf,
    /// Run the parser fuzz target on nightly, seeded with the samples.
    Fuzz {
        /// How long to fuzz for.
        #[arg(long, default_value_t = 60)]
        seconds: u64,
    },
    #[command(flatten)]
    Shared(shared::Command),
}

fn main() {
    let cli = Cli::parse();
    let root = repo_root();
    let result = match cli.command {
        Command::Check { full } => check::run(&root, full),
        Command::Record { only } => record::run(&root, &only),
        Command::InstallIperf => iperf::install(),
        Command::Fuzz { seconds } => fuzz::run(&root, seconds),
        Command::Shared(command) => shared::run(&root, command),
    };
    if let Err(message) = result {
        eprintln!("{message}");
        std::process::exit(1);
    }
}

/// The workspace root, which is the parent of the `xtask` crate.
fn repo_root() -> PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| manifest_dir.to_path_buf())
}
