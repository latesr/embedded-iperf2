// A copy from embedded-devkit/xtask/src/shared. Edit it there, then run `cargo xtask sync` in the devkit.

//! The toolchains every repo builds with, and the check that a repo installs them.

use super::files::read_files;
use std::fs;
use std::path::Path;
use std::process::Command;

/// The stable toolchain that every `rust-toolchain.toml` names. It moves with [`NIGHTLY`] when a new stable is
/// released.
pub const STABLE: &str = "1.99.0";

/// The nightly that the fuzzer and `-Zbuild-std` builds use, as rustup names it. Pass it as
/// `format!("+{NIGHTLY}")`. It moves with [`STABLE`].
pub const NIGHTLY: &str = "nightly-2026-10-11";

/// Fails when a `rust-toolchain.toml` in the repo names a channel other than [`STABLE`], or when a CI workflow
/// installs or runs a nightly other than [`NIGHTLY`].
pub fn check(root: &Path) -> Result<(), String> {
    let mut problems = Vec::new();
    for (path, text) in read_files(root, "toml") {
        if path
            .file_name()
            .is_none_or(|name| name != "rust-toolchain.toml")
        {
            continue;
        }
        let channel = text
            .lines()
            .find_map(|line| line.trim().strip_prefix("channel"))
            .and_then(|rest| rest.split('"').nth(1));
        if channel != Some(STABLE) {
            problems.push(format!(
                "{} names the channel {}, not {STABLE}",
                path.display(),
                channel.unwrap_or("(none)")
            ));
        }
    }
    let workflows = Path::new(".github").join("workflows");
    let mut entries: Vec<_> = fs::read_dir(root.join(&workflows))
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.file_name())
        .collect();
    entries.sort();
    for name in entries {
        let path = workflows.join(&name);
        let Ok(text) = fs::read_to_string(root.join(&path)) else {
            continue;
        };
        for (line, number) in text.lines().zip(1..) {
            for toolchain in nightlies(line) {
                if toolchain != NIGHTLY {
                    problems.push(format!(
                        "{}:{number} uses {toolchain}, not {NIGHTLY}",
                        path.display()
                    ));
                }
            }
        }
    }
    if problems.is_empty() {
        Ok(())
    } else {
        Err(problems.join("\n"))
    }
}

/// The nightlies a workflow line installs or runs: the toolchains after `rustup toolchain install`, and each
/// `+` argument, that start with `nightly`.
fn nightlies(line: &str) -> Vec<&str> {
    let mut found = Vec::new();
    if let Some((_, rest)) = line.split_once("rustup toolchain install") {
        found.extend(
            rest.split_whitespace()
                .take_while(|word| !word.starts_with('-')),
        );
    }
    found.extend(
        line.split_whitespace()
            .filter_map(|word| word.strip_prefix('+')),
    );
    found.retain(|toolchain| toolchain.starts_with("nightly"));
    found
}

/// The target triple that `toolchain` builds for by default, as `rustc -vV` prints it on its `host:` line.
///
/// A prebuilt tool can have a default target other than the host. A cargo-fuzz built for musl builds for musl,
/// which the address sanitizer does not support. A fuzz command passes the triple to cargo-fuzz as `--target`.
/// Fails when `rustc` for `toolchain` does not start or exits with an error, or prints no `host:` line.
// A repo with a fuzzer calls it from its own fuzz command, and the devkit has none.
#[allow(dead_code)]
pub fn host_triple(toolchain: &str) -> Result<String, String> {
    let output = Command::new("rustc")
        .args([&format!("+{toolchain}"), "-vV"])
        .output()
        .map_err(|error| format!("could not start rustc +{toolchain}: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "rustc +{toolchain} -vV failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    host_of(&String::from_utf8_lossy(&output.stdout))
        .map(str::to_owned)
        .ok_or_else(|| format!("rustc +{toolchain} -vV printed no host line"))
}

/// The value of the `host:` line in the output of `rustc -vV`.
fn host_of(version: &str) -> Option<&str> {
    version
        .lines()
        .find_map(|line| line.strip_prefix("host:"))
        .map(str::trim)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared::files::testing::TempRepo;

    fn toolchain_file(channel: &str) -> String {
        format!("[toolchain]\nchannel = \"{channel}\"\ncomponents = [\"rustfmt\"]\n")
    }

    /// A repo with a root `rust-toolchain.toml` naming `root`, a board's naming `board`, and a workflow with
    /// `workflow` as its steps.
    fn repo(name: &str, root: &str, board: &str, workflow: &str) -> TempRepo {
        let repo = TempRepo::new(name);
        repo.write("rust-toolchain.toml", &toolchain_file(root));
        fs::create_dir_all(repo.0.join("esp/board")).unwrap();
        repo.write("esp/board/rust-toolchain.toml", &toolchain_file(board));
        fs::create_dir_all(repo.0.join(".github/workflows")).unwrap();
        let ci = format!("jobs:\n  nightly:\n    steps:\n{workflow}");
        repo.write(".github/workflows/ci.yml", &ci);
        repo
    }

    fn install(toolchain: &str) -> String {
        format!("      - run: rustup toolchain install {toolchain} --profile minimal\n")
    }

    #[test]
    fn a_repo_on_the_pinned_toolchains_passes() {
        let steps = format!(
            "{}      - run: cargo +{NIGHTLY} fuzz run frame\n",
            install(NIGHTLY)
        );
        let repo = repo("toolchain-pinned", STABLE, STABLE, &steps);
        assert_eq!(check(&repo.0), Ok(()));
    }

    #[test]
    fn a_toolchain_file_on_another_stable_fails_and_is_named() {
        let repo = repo("toolchain-stable", STABLE, "1.98.1", &install(NIGHTLY));
        let error = check(&repo.0).unwrap_err();
        assert!(error.contains("esp/board/rust-toolchain.toml"), "{error}");
        assert!(error.contains("1.98.1"), "{error}");
        assert!(!error.contains("\nrust-toolchain.toml"), "{error}");
    }

    #[test]
    fn a_workflow_that_installs_or_runs_another_nightly_fails() {
        for steps in [
            install("nightly"),
            install("nightly-2026-08-26"),
            "      - run: cargo +nightly fuzz run frame\n".to_owned(),
        ] {
            let repo = repo("toolchain-nightly", STABLE, STABLE, &steps);
            let error = check(&repo.0).unwrap_err();
            assert!(error.contains(".github/workflows/ci.yml"), "{error}");
            assert!(error.contains(NIGHTLY), "{error}");
        }
    }

    #[test]
    fn the_host_is_read_from_its_own_line() {
        let version = "rustc 1.101.0-nightly (6f5da406b 2026-10-10)\nbinary: rustc\n\
                       host: x86_64-unknown-linux-gnu\nrelease: 1.101.0-nightly\n";
        assert_eq!(host_of(version), Some("x86_64-unknown-linux-gnu"));
        assert_eq!(host_of("rustc 1.99.0\nrelease: 1.99.0\n"), None);
    }

    #[test]
    fn the_host_of_the_stable_toolchain_is_the_machine_the_test_runs_on() {
        let host = host_triple(STABLE).unwrap();
        assert!(
            host.starts_with(&format!("{}-", std::env::consts::ARCH)),
            "{host}"
        );
    }

    #[test]
    fn a_job_named_nightly_is_not_a_toolchain() {
        let repo = repo(
            "toolchain-job",
            STABLE,
            STABLE,
            "      - run: echo nightly\n",
        );
        assert_eq!(check(&repo.0), Ok(()));
    }
}
