//! Runs every check that CI runs. The link and prose checks go first, because they take under a second and a
//! writing fault should fail before the builds.

use crate::shared;
use std::path::Path;
use std::process::Command;

/// The crates that are published to crates.io and must build for every target in `rust-toolchain.toml`.
const PUBLISHED: &[&str] = &["embedded-iperf2"];

/// Runs each check in turn and stops at the first failure.
pub fn run(root: &Path) -> Result<(), String> {
    let targets = toolchain_targets(root)?;
    let msrv = workspace_msrv(root)?;

    println!("==> links");
    shared::links::run(root)?;
    println!("==> prose");
    shared::prose::run(root, None)?;
    shared::require_tool(root, "hack", "cargo install cargo-hack")?;
    step("format", cargo(root).args(["fmt", "--all", "--check"]))?;
    step(
        "lints",
        cargo(root).args([
            "hack",
            "clippy",
            "--workspace",
            "--all-targets",
            "--feature-powerset",
            "--depth",
            "2",
            "--",
            "-D",
            "warnings",
        ]),
    )?;
    step(
        "tests",
        cargo(root).args([
            "hack",
            "test",
            "--workspace",
            "--feature-powerset",
            "--depth",
            "2",
        ]),
    )?;
    step(
        "docs",
        cargo(root)
            .args(["doc", "--workspace", "--no-deps"])
            .env("RUSTDOCFLAGS", "-D warnings"),
    )?;
    step(
        "feature combinations",
        cargo(root).args([
            "hack",
            "check",
            "--workspace",
            "--feature-powerset",
            "--depth",
            "2",
            "--no-dev-deps",
        ]),
    )?;
    for (target, args) in cross_builds(&targets, PUBLISHED) {
        step(&format!("build for {target}"), cargo(root).args(args))?;
    }
    step(
        &format!("MSRV {msrv}"),
        cargo(root).args([
            &format!("+{msrv}"),
            "check",
            "--workspace",
            "--all-features",
        ]),
    )?;
    println!("==> every check passed");
    Ok(())
}

/// The `cargo` arguments that build the published crates for each target. With no published crates there is
/// nothing to build, and `cargo build` without `--package` would build `xtask` for a microcontroller.
fn cross_builds(targets: &[String], published: &[&str]) -> Vec<(String, Vec<String>)> {
    if published.is_empty() {
        return Vec::new();
    }
    targets
        .iter()
        .map(|target| {
            let mut args = vec!["build".to_owned(), "--target".to_owned(), target.clone()];
            for crate_name in published {
                args.push("--package".to_owned());
                args.push((*crate_name).to_owned());
            }
            (target.clone(), args)
        })
        .collect()
}

/// A `cargo` command that runs in the workspace root.
fn cargo(root: &Path) -> Command {
    let mut command = Command::new("cargo");
    command.current_dir(root);
    command
}

/// Prints the step's name, runs it and fails when its exit status is not success.
fn step(name: &str, command: &mut Command) -> Result<(), String> {
    println!("==> {name}");
    let status = command
        .status()
        .map_err(|error| format!("{name}: could not start cargo: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{name}: failed"))
    }
}

/// The targets listed in `rust-toolchain.toml`. That file is what rustup installs, so it owns the list.
fn toolchain_targets(root: &Path) -> Result<Vec<String>, String> {
    let table = read_toml(&root.join("rust-toolchain.toml"))?;
    let targets = table
        .get("toolchain")
        .and_then(|toolchain| toolchain.get("targets"))
        .and_then(toml::Value::as_array)
        .ok_or("rust-toolchain.toml has no [toolchain] targets list")?;
    Ok(targets
        .iter()
        .filter_map(toml::Value::as_str)
        .map(str::to_owned)
        .collect())
}

/// The `rust-version` in the workspace `Cargo.toml`.
fn workspace_msrv(root: &Path) -> Result<String, String> {
    let table = read_toml(&root.join("Cargo.toml"))?;
    table
        .get("workspace")
        .and_then(|workspace| workspace.get("package"))
        .and_then(|package| package.get("rust-version"))
        .and_then(toml::Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| "Cargo.toml has no workspace.package.rust-version".to_owned())
}

fn read_toml(path: &Path) -> Result<toml::Table, String> {
    let text =
        std::fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))?;
    text.parse()
        .map_err(|error| format!("{}: {error}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn targets() -> Vec<String> {
        vec![
            "thumbv6m-none-eabi".to_owned(),
            "riscv32imac-unknown-none-elf".to_owned(),
        ]
    }

    #[test]
    fn nothing_is_cross_built_without_a_published_crate() {
        assert_eq!(cross_builds(&targets(), &[]), Vec::new());
    }

    #[test]
    fn each_target_builds_every_published_crate() {
        let args = |target: &str| {
            [
                "build",
                "--target",
                target,
                "--package",
                "a",
                "--package",
                "b",
            ]
            .map(str::to_owned)
            .to_vec()
        };
        assert_eq!(
            cross_builds(&targets(), &["a", "b"]),
            vec![
                ("thumbv6m-none-eabi".to_owned(), args("thumbv6m-none-eabi")),
                (
                    "riscv32imac-unknown-none-elf".to_owned(),
                    args("riscv32imac-unknown-none-elf")
                ),
            ]
        );
    }
}
