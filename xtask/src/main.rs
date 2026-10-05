//! Repository tasks. `cargo xtask <command>` runs one.

mod check;
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
    /// Run every check that CI runs.
    Check,
    /// Record the samples from the iperf 2.2.2 binary that `MCU_IPERF2` names.
    Record {
        /// Record only these cases. Every case is recorded when none is named.
        only: Vec<String>,
    },
    #[command(flatten)]
    Shared(shared::Command),
}

fn main() {
    let cli = Cli::parse();
    let root = repo_root();
    let result = match cli.command {
        Command::Check => check::run(&root),
        Command::Record { only } => record::run(&root, &only),
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
