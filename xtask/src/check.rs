//! `cargo xtask check [--push | --full]`: runs these checks in order, and stops at the first failure:
//!
//! 1. the links and the prose
//! 2. the toolchain file
//! 3. the format
//! 4. the lints and the tests of the crates that the change reaches, or of every crate with `--full`, with every
//!    feature on, then off. With `--push` or `--full`, the tests include the ignored ones, which are the
//!    interoperability tests.
//! 5. the docs
//! 6. with `--full`, each combination of up to two features
//! 7. with `--push` or `--full`, the build of each published crate for each target in `rust-toolchain.toml`
//! 8. with `--push` or `--full`, the workspace at its minimum supported Rust version (MSRV)
//!
//! Without a flag, the change is the one against `HEAD`, as the pre-commit hook needs. With `--push`, it is what
//! the branch changed since its base, as the pre-push hook needs.
//!
//! The link and prose checks take under a second. They go first, so a writing fault fails before the builds. A
//! change that is only markdown skips every cargo check.

use crate::shared;
use crate::shared::checks::{
    changed_paths, changed_since, feature_runs, only_markdown, push_base, reached_packages, step,
};
use std::path::{Path, PathBuf};
use std::process::Command;

/// The crates that are published to crates.io and must build for every target in `rust-toolchain.toml`.
const PUBLISHED: &[&str] = &["embedded-iperf2"];

/// The features that only a board turns on, so only a cross build compiles them for a board's target.
const BOARD_FEATURES: &[&str] = &["embassy-net"];

/// Which checks a run makes, and over which changes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Scope {
    /// The quick checks over the changes against `HEAD`, which the pre-commit hook runs.
    Commit,
    /// Every check over what the branch changed since its base, which the pre-push hook runs.
    Push,
    /// Every check on every crate, with each feature pair, whatever changed.
    Full,
}

impl Scope {
    /// Whether the slow checks run: the interoperability tests, the cross-target builds, and the MSRV check.
    fn slow(self) -> bool {
        self != Scope::Commit
    }
}

/// The paths that `scope` checks. An empty list reaches every crate: `Full`, and a push with no base, as before a
/// repo's first push.
fn changed(root: &Path, scope: Scope) -> Vec<PathBuf> {
    match scope {
        Scope::Commit => changed_paths(root),
        Scope::Push => push_base(root)
            .map(|base| changed_since(root, &base))
            .unwrap_or_default(),
        Scope::Full => Vec::new(),
    }
}

/// Runs each check in `scope` in turn and stops at the first failure.
pub fn run(root: &Path, scope: Scope) -> Result<(), String> {
    shared::sweep::daily(root, &[]);
    let full = scope == Scope::Full;
    let targets = toolchain_targets(root)?;
    let msrv = workspace_msrv(root)?;
    let changed = changed(root, scope);

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
        // The interoperability tests are ignored, so that a commit does not wait for a stock iperf.
        let ignored: &[&str] = if scope.slow() {
            &["--", "--include-ignored"]
        } else {
            &[]
        };
        for (features, args) in feature_runs(full) {
            step(
                &format!("tests, {features}"),
                cargo(root)
                    .args(["hack", "test"])
                    .args(&crates)
                    .args(args)
                    .args(ignored),
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
    if scope.slow() {
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
    } else {
        println!(
            "==> the interoperability tests, the cross-target builds and the MSRV check skipped, because the \
             pre-push hook runs them"
        );
    }
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
    use crate::shared::files::testing::TempRepo;

    #[test]
    fn a_commit_leaves_the_slow_checks_to_the_push_and_the_full_run() {
        assert!(!Scope::Commit.slow());
        assert!(Scope::Push.slow());
        assert!(Scope::Full.slow());
    }

    #[test]
    fn a_commit_checks_its_changes_a_push_the_branch_s_and_a_full_run_everything() {
        let repo = TempRepo::new("check-scope");
        let git = |args: &[&str]| shared::git(&repo.0, args).unwrap();
        git(&["init", "--quiet", "--initial-branch=main"]);
        git(&["config", "user.email", "test@example.com"]);
        git(&["config", "user.name", "test"]);
        repo.write("a.md", "a\n");
        git(&["add", "."]);
        git(&["commit", "--quiet", "-m", "first"]);
        git(&["update-ref", "refs/remotes/origin/main", "HEAD"]);
        repo.write("b.md", "b\n");
        git(&["add", "."]);
        git(&["commit", "--quiet", "-m", "second"]);
        repo.write("c.md", "c\n");
        git(&["add", "c.md"]);
        let names = |scope| {
            changed(&repo.0, scope)
                .into_iter()
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>()
        };
        assert_eq!(names(Scope::Commit), ["c.md"]);
        // Without origin/HEAD the push does not have a base, so it checks everything.
        assert_eq!(names(Scope::Push), Vec::<String>::new());
        git(&[
            "symbolic-ref",
            "refs/remotes/origin/HEAD",
            "refs/remotes/origin/main",
        ]);
        assert_eq!(names(Scope::Push), ["b.md", "c.md"]);
        assert_eq!(names(Scope::Full), Vec::<String>::new());
    }

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
