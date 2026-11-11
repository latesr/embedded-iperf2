//! Builds the stock iperf at the pinned version, checks its version, and runs it as a server. The recorder,
//! `cargo xtask hil`, and the interoperability tests run that binary. `cargo xtask install-iperf` builds it.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::time::Duration;

/// The version the samples come from. `MCU_IPERF2` must name a binary that reports it.
pub const VERSION: &str = "2.2.2";
const REPOSITORY: &str = "https://git.code.sf.net/p/iperf2/code";
/// The tip of the branch `2-2-2`. The repository does not have a tag for 2.2.2. The last change to the source on
/// the branch is the 2.2.2 release notes. The two commits after it change only the Android app. The samples were
/// recorded from a build of `1779e3e3` on `master`. Its `src/`, `include/`, and `compat/` equal those of the pin.
const PIN: &str = "74d4b55321b9e7827c6567f3e48ae024ba3417c8";

/// Builds iperf from the repository at the pin into `~/.local/opt/iperf/<version>/iperf`. Skips the build when a
/// binary of the pinned version is already there. Prints the path for `MCU_IPERF2`.
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
    // A build of the wrong version keeps its source tree for inspection.
    check_version(&binary)?;
    std::fs::remove_dir_all(&source).map_err(|error| format!("{}: {error}", source.display()))?;
    println!("==> installed {}", binary.display());
    println!("==> set MCU_IPERF2 to that path");
    Ok(())
}

/// Returns the first line that `binary -v` prints. Fails when that line does not name the pinned version. iperf 2
/// prints the line on standard error.
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

/// The binary that `MCU_IPERF2` names, and the version line it prints. Fails when the variable is not set, or when
/// the binary does not report the pinned version.
pub fn binary() -> Result<(String, String), String> {
    let binary = std::env::var("MCU_IPERF2").map_err(|_| {
        format!("MCU_IPERF2 must name an iperf {VERSION} binary, which `cargo xtask install-iperf` builds")
    })?;
    let version = check_version(Path::new(&binary))?;
    Ok((binary, version))
}

/// A stock server, killed when it is dropped, so a run that fails part way does not leave it running.
pub struct StockServer(Option<Child>);

impl StockServer {
    /// Starts `binary` with `args`, and waits `start` for it to listen.
    pub fn start(binary: &str, args: &[String], start: Duration) -> Result<Self, String> {
        let child = Command::new(binary)
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| format!("{binary}: {error}"))?;
        std::thread::sleep(start);
        Ok(Self(Some(child)))
    }

    /// Stops the server with SIGINT, and returns what the server printed. SIGINT lets the server flush its report.
    pub fn interrupt(mut self) -> Result<Output, String> {
        let child = self.0.take().ok_or("the server is gone")?;
        let status = Command::new("kill")
            .args(["-INT", &child.id().to_string()])
            .status()
            .map_err(|error| format!("kill: {error}"))?;
        if !status.success() {
            return Err("kill -INT failed".to_owned());
        }
        child
            .wait_with_output()
            .map_err(|error| format!("the server: {error}"))
    }
}

impl Drop for StockServer {
    fn drop(&mut self) {
        if let Some(child) = &mut self.0 {
            let _ = child.kill();
            let _ = child.wait();
        }
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
