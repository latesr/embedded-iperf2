//! `cargo xtask check [--full]`: runs these checks in order, and stops at the first failure:
//!
//! 1. the links and the prose
//! 2. the toolchain file
//! 3. the format
//! 4. the lints and the tests of the crates that the change reaches, or of every crate with `--full`, with every
//!    feature on, then off
//! 5. the docs
//! 6. with `--full`, each combination of up to two features
//! 7. the build of each published crate for each target in `rust-toolchain.toml`
//! 8. the workspace at its minimum supported Rust version (MSRV)
//!
//! The link and prose checks take under a second. They go first, so a writing fault fails before the builds. A
//! change that is only markdown skips every cargo check.

use crate::shared;
use crate::shared::checks::{changed_paths, feature_runs, only_markdown, reached_packages, step};
use std::path::Path;
use std::process::Command;

/// The crates that are published to crates.io and must build for every target in `rust-toolchain.toml`.
const PUBLISHED: &[&str] = &["embedded-iperf2"];

/// The features that only a board turns on, so only a cross build compiles them for a board's target.
const BOARD_FEATURES: &[&str] = &["embassy-net"];

/// Runs each check in turn and stops at the first failure. With `full`, the lints and tests cover every crate,
/// whatever changed.
pub fn run(root: &Path, full: bool) -> Result<(), String> {
    shared::sweep::daily(root, &[]);
    let targets = toolchain_targets(root)?;
    let msrv = workspace_msrv(root)?;
    let changed = if full {
        Vec::new()
    } else {
        changed_paths(root)
    };

    println!("==> links");
    shared::links::run(root)?;
    println!("==> prose");
    shared::prose::run(root, None)?;
    println!("==> toolchain");
    shared::toolchain::check(root)?;
    // The root workspace is the only group. No code reads a markdown file at run time, so the list of paths is empty.
    if only_markdown(&changed, &[]) {
        println!("==> every cargo check skipped, because the change is only markdown");
        return Ok(());
    }
    shared::require_tool(root, "hack", "cargo install cargo-hack")?;
    step("format", cargo(root).args(["fmt", "--all", "--check"]))?;
    let crates = match reached_packages(root, &changed)? {
        None => vec!["--workspace".to_owned()],
        Some(names) => names
            .into_iter()
            .flat_map(|name| ["--package".to_owned(), name])
            .collect(),
    };
    if crates.is_empty() {
        println!(
            "==> lints and tests skipped, because the change reaches no crate in the workspace"
        );
    } else {
        for (features, args) in feature_runs(full) {
            step(
                &format!("lints, {features}"),
                cargo(root)
                    .args(["hack", "clippy", "--all-targets"])
                    .args(&crates)
                    .args(args)
                    .args(["--", "-D", "warnings"]),
            )?;
        }
        for (features, args) in feature_runs(full) {
            step(
                &format!("tests, {features}"),
                cargo(root).args(["hack", "test"]).args(&crates).args(args),
            )?;
        }
    }
    step(
        "docs",
        cargo(root)
            .args(["doc", "--workspace", "--no-deps"])
            .env("RUSTDOCFLAGS", "-D warnings"),
    )?;
    if full {
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
    }
    for (target, args) in cross_builds(&targets, PUBLISHED, BOARD_FEATURES) {
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

/// The `cargo` arguments that build the published crates for each target, each with the name of its step. Each
/// target builds once without features, then once with each of `features`. The list is empty when `published` is
/// empty, because `cargo build` without `--package` builds `xtask` for a microcontroller.
fn cross_builds(
    targets: &[String],
    published: &[&str],
    features: &[&str],
) -> Vec<(String, Vec<String>)> {
    if published.is_empty() {
        return Vec::new();
    }
    let mut builds = Vec::new();
    for target in targets {
        let mut args = vec!["build".to_owned(), "--target".to_owned(), target.clone()];
        for crate_name in published {
            args.push("--package".to_owned());
            args.push((*crate_name).to_owned());
        }
        builds.push((target.clone(), args.clone()));
        for feature in features {
            let mut with = args.clone();
            with.extend(["--features".to_owned(), (*feature).to_owned()]);
            builds.push((format!("{target} with {feature}"), with));
        }
    }
    builds
}

/// A `cargo` command that runs in the workspace root.
fn cargo(root: &Path) -> Command {
    let mut command = Command::new("cargo");
    command.current_dir(root);
    command
}

/// The targets listed in `rust-toolchain.toml`. rustup installs the targets in that file, so the file owns the list.
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
        assert_eq!(cross_builds(&targets(), &[], &["f"]), Vec::new());
    }

    #[test]
    fn each_target_builds_every_published_crate_without_features_and_with_each_board_feature() {
        let args = |target: &str, feature: Option<&str>| {
            let mut args = [
                "build",
                "--target",
                target,
                "--package",
                "a",
                "--package",
                "b",
            ]
            .map(str::to_owned)
            .to_vec();
            if let Some(feature) = feature {
                args.extend(["--features".to_owned(), feature.to_owned()]);
            }
            args
        };
        let thumb = "thumbv6m-none-eabi";
        let riscv = "riscv32imac-unknown-none-elf";
        assert_eq!(
            cross_builds(&targets(), &["a", "b"], &["f", "g"]),
            vec![
                (thumb.to_owned(), args(thumb, None)),
                (format!("{thumb} with f"), args(thumb, Some("f"))),
                (format!("{thumb} with g"), args(thumb, Some("g"))),
                (riscv.to_owned(), args(riscv, None)),
                (format!("{riscv} with f"), args(riscv, Some("f"))),
                (format!("{riscv} with g"), args(riscv, Some("g"))),
            ]
        );
    }
}
