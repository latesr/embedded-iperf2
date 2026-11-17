//! Runs the repository tasks. `cargo xtask <command>` runs one, and `cargo xtask help` lists them.

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
        /// Also run the slow checks, over what the branch changed since its upstream or origin/HEAD. The pre-push
        /// hook uses it.
        #[arg(long, conflicts_with = "full")]
        push: bool,
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
        /// The number of seconds that the fuzzer runs.
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
        Command::Check { push, full } => check::run(
            &root,
            match (push, full) {
                (_, true) => check::Scope::Full,
                (true, false) => check::Scope::Push,
                (false, false) => check::Scope::Commit,
            },
        ),
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

/// The workspace root. It is the parent folder of the `xtask` crate.
fn repo_root() -> PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| manifest_dir.to_path_buf())
}
