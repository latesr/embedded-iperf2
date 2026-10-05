//! The stock iperf that the recorder and the interoperability tests run, and `cargo xtask install-iperf`, which
//! builds it.

use std::path::{Path, PathBuf};
use std::process::Command;

/// The version the samples come from. `MCU_IPERF2` must name a binary that reports it.
pub const VERSION: &str = "2.2.2";
const REPOSITORY: &str = "https://git.code.sf.net/p/iperf2/code";
/// The tip of the branch `2-2-2`. The repository has no tag for 2.2.2, and the iperf2 entity page states why this
/// commit is the release.
const PIN: &str = "74d4b55321b9e7827c6567f3e48ae024ba3417c8";

/// Builds iperf from the repository at the pin into `~/.local/opt/iperf/<version>/iperf`, unless a binary of the
/// pinned version is already there. Prints the path for `MCU_IPERF2`.
pub fn install() -> Result<(), String> {
    let home = std::env::var("HOME").map_err(|_| "HOME is not set".to_owned())?;
    let folder = PathBuf::from(home).join(".local/opt/iperf").join(VERSION);
    let binary = folder.join("iperf");
    if binary.exists() && check_version(&binary).is_ok() {
        println!("==> iperf {VERSION} is already at {}", binary.display());
        return Ok(());
    }
    let source = folder.join("source");
    if source.exists() {
        std::fs::remove_dir_all(&source)
            .map_err(|error| format!("{}: {error}", source.display()))?;
    }
    std::fs::create_dir_all(&folder).map_err(|error| format!("{}: {error}", folder.display()))?;

    println!("==> clone {REPOSITORY}");
    run(Command::new("git")
        .args(["clone", "--quiet", "--no-checkout", REPOSITORY])
        .arg(&source))?;
    run(Command::new("git")
        .arg("-C")
        .arg(&source)
        .args(["checkout", "--quiet", PIN]))?;
    println!("==> configure");
    run(Command::new("./configure")
        .arg("--quiet")
        .current_dir(&source))?;
    println!("==> make");
    let jobs = std::thread::available_parallelism().map_or(1, usize::from);
    run(Command::new("make")
        .args(["--silent", &format!("-j{jobs}")])
        .current_dir(&source))?;

    std::fs::copy(source.join("src/iperf"), &binary)
        .map_err(|error| format!("{}: {error}", binary.display()))?;
    std::fs::remove_dir_all(&source).map_err(|error| format!("{}: {error}", source.display()))?;
    check_version(&binary)?;
    println!("==> installed {}", binary.display());
    println!("==> set MCU_IPERF2 to that path");
    Ok(())
}

/// Fails unless the first line that `binary -v` prints names the pinned version. iperf 2 prints it on standard
/// error. Returns that line.
pub fn check_version(binary: &Path) -> Result<String, String> {
    let output = Command::new(binary)
        .arg("-v")
        .output()
        .map_err(|error| format!("{}: {error}", binary.display()))?;
    let text = [output.stdout, output.stderr].concat();
    let line = String::from_utf8_lossy(&text)
        .lines()
        .next()
        .unwrap_or_default()
        .to_owned();
    if line.starts_with(&format!("iperf {VERSION} ")) {
        Ok(line)
    } else {
        Err(format!(
            "{} reports `{line}`, not {VERSION}",
            binary.display()
        ))
    }
}

fn run(command: &mut Command) -> Result<(), String> {
    let status = command
        .status()
        .map_err(|error| format!("{command:?}: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{command:?} failed: {status}"))
    }
}
