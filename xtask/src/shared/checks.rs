// A copy from embedded-devkit/xtask/src/shared. Edit it there, then run `cargo xtask sync` in the devkit.

//! Runs and times the steps of `cargo xtask check`, and decides which groups of steps a commit's changes reach.
//!
//! Each repo's `check.rs` names its groups and the paths each one builds from. This module names the paths that
//! reach every group: a file in the repo root that is not markdown, and anything under `.cargo/` or `xtask/`. A
//! tree without changes, as in CI, reaches every group.

// A repo uses the functions its groups need. A repo without a separate workspace never calls `workspace_inputs`.
#![allow(dead_code)]

use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

/// Prints the step's name, runs it, prints how long it took and fails when its exit status is not success.
pub fn step(name: &str, command: &mut Command) -> Result<(), String> {
    println!("==> {name}");
    let start = Instant::now();
    let status = command
        .status()
        .map_err(|error| format!("{name}: could not start cargo: {error}"))?;
    println!("==> {name} took {}", duration(start.elapsed()));
    if status.success() {
        Ok(())
    } else {
        Err(format!("{name}: failed"))
    }
}

/// A step's duration in seconds, to a tenth.
fn duration(elapsed: Duration) -> String {
    format!("{:.1} s", elapsed.as_secs_f64())
}

/// The paths that differ from `HEAD`, staged or not. Outside a git repo the list is empty, so every check runs.
pub fn changed_paths(root: &Path) -> Vec<PathBuf> {
    super::git(root, &["diff", "HEAD", "--name-only", "--no-ext-diff"])
        .map(|names| {
            String::from_utf8_lossy(&names)
                .lines()
                .map(PathBuf::from)
                .collect()
        })
        .unwrap_or_default()
}

/// The paths, from the repo root, that the workspace in `workspace` builds from: the workspace folder and each path
/// dependency inside the repo. The path dependencies come from the workspace's manifest, from each member's, and
/// from each in-repo crate those reach, in turn. A path outside the repo, such as a clone of an upstream project,
/// is left out.
pub fn workspace_inputs(root: &Path, workspace: &str) -> Result<Vec<PathBuf>, String> {
    fn paths<'a>(value: &'a toml::Value, found: &mut Vec<&'a str>) {
        if let toml::Value::Table(table) = value {
            for (key, value) in table {
                match (key.as_str(), value.as_str()) {
                    ("path", Some(path)) => found.push(path),
                    _ => paths(value, found),
                }
            }
        }
    }
    let mut inputs = vec![PathBuf::from(workspace)];
    let mut read = Vec::new();
    let mut folders = vec![PathBuf::from(workspace)];
    while let Some(folder) = folders.pop() {
        if read.contains(&folder) {
            continue;
        }
        let manifest = toml::Value::Table(read_toml(&root.join(&folder).join("Cargo.toml"))?);
        read.push(folder.clone());
        let mut found = Vec::new();
        paths(&manifest, &mut found);
        let members = manifest
            .get("workspace")
            .and_then(|workspace| workspace.get("members"))
            .and_then(toml::Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(toml::Value::as_str);
        found.extend(members);
        for path in found {
            let Some(resolved) = inside_repo(&folder.join(path)) else {
                continue;
            };
            // A `path` key also names a target's source file, which has no manifest of its own.
            if root.join(&resolved).join("Cargo.toml").is_file() {
                folders.push(resolved.clone());
            }
            if !resolved.starts_with(workspace) {
                inputs.push(resolved);
            }
        }
    }
    inputs.sort();
    inputs.dedup();
    Ok(inputs)
}

/// `path`, from the repo root, with its `..` parts resolved, or `None` when it leaves the repo.
fn inside_repo(path: &Path) -> Option<PathBuf> {
    let mut resolved = PathBuf::new();
    for part in path.components() {
        match part {
            Component::ParentDir => {
                if !resolved.pop() {
                    return None;
                }
            }
            Component::Normal(name) => resolved.push(name),
            _ => {}
        }
    }
    Some(resolved)
}

/// Whether a group that builds from `inputs` runs, given the paths that differ from `HEAD`. A tree without changes
/// reaches every group.
pub fn reaches(changed: &[PathBuf], inputs: &[PathBuf]) -> bool {
    changed.is_empty()
        || changed.iter().any(|path| {
            reaches_every_group(path) || inputs.iter().any(|input| path.starts_with(input))
        })
}

/// Whether `path` is a file in the repo root that is not markdown, or lies under `.cargo/` or `xtask/`. These hold
/// the build's shared settings, such as the toolchain, the lock file and the checks themselves.
fn reaches_every_group(path: &Path) -> bool {
    let mut parts = path.components();
    match (parts.next(), parts.next()) {
        (Some(Component::Normal(_)), None) => !is_markdown(path),
        (Some(Component::Normal(first)), Some(_)) => first == ".cargo" || first == "xtask",
        _ => false,
    }
}

/// Whether the change is made only of markdown files, none of them under a path in `named`. An empty change is
/// not, so a tree without changes runs every check.
pub fn only_markdown(changed: &[PathBuf], named: &[PathBuf]) -> bool {
    !changed.is_empty()
        && changed
            .iter()
            .all(|path| is_markdown(path) && !named.iter().any(|name| path.starts_with(name)))
}

fn is_markdown(path: &Path) -> bool {
    path.extension().is_some_and(|extension| extension == "md")
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

    fn paths(names: &[&str]) -> Vec<PathBuf> {
        names.iter().map(PathBuf::from).collect()
    }

    #[test]
    fn a_duration_is_written_in_seconds_to_a_tenth() {
        assert_eq!(duration(Duration::from_millis(1_234)), "1.2 s");
        assert_eq!(duration(Duration::from_millis(95_060)), "95.1 s");
        assert_eq!(duration(Duration::ZERO), "0.0 s");
    }

    #[test]
    fn a_step_fails_when_its_command_fails_or_cannot_start() {
        assert_eq!(step("true", &mut Command::new("true")), Ok(()));
        assert_eq!(
            step("false", &mut Command::new("false")),
            Err("false: failed".to_owned())
        );
        let error = step("missing", &mut Command::new("no-such-program-xyz")).unwrap_err();
        assert!(error.starts_with("missing: could not start"), "{error}");
    }

    #[test]
    fn the_changed_paths_are_the_staged_and_unstaged_ones_and_none_outside_git() {
        let repo = TempRepo::new("checks-changed");
        assert_eq!(changed_paths(&repo.0), Vec::<PathBuf>::new());
        for args in [
            &["init", "--quiet"][..],
            &["config", "user.email", "test@example.com"],
            &["config", "user.name", "test"],
        ] {
            super::super::git(&repo.0, args).unwrap();
        }
        repo.write("a.md", "a\n");
        repo.write("b.md", "b\n");
        super::super::git(&repo.0, &["add", "."]).unwrap();
        super::super::git(&repo.0, &["commit", "--quiet", "-m", "a"]).unwrap();
        repo.write("a.md", "changed\n");
        repo.write("docs/c.md", "new\n");
        super::super::git(&repo.0, &["add", "docs/c.md"]).unwrap();
        assert_eq!(changed_paths(&repo.0), paths(&["a.md", "docs/c.md"]));
    }

    #[test]
    fn a_workspace_builds_from_its_folder_and_its_path_dependencies_in_the_repo() {
        let repo = TempRepo::new("checks-inputs");
        std::fs::create_dir_all(repo.0.join("esp")).unwrap();
        repo.write(
            "esp/Cargo.toml",
            "[workspace.dependencies]\nfamily = { path = \"family\" }\nmcu-net = { path = \"../mcu-net\" }\nmcu-console = { path = \"../mcu-console\", features = [\"x\"] }\n\n[patch.'https://github.com/esp-rs/esp-hal']\nesp-hal = { path = \"../../esp-hal/esp-hal\" }\n",
        );
        assert_eq!(
            workspace_inputs(&repo.0, "esp"),
            Ok(paths(&["esp", "mcu-console", "mcu-net"]))
        );
        assert!(workspace_inputs(&repo.0, "missing").is_err());
    }

    #[test]
    fn a_workspace_builds_from_the_path_dependencies_of_its_members_and_of_theirs() {
        let repo = TempRepo::new("checks-inputs-members");
        for folder in [
            "firmware/reference/client",
            "firmware/reference/stub",
            "nats",
            "proto",
        ] {
            std::fs::create_dir_all(repo.0.join(folder)).unwrap();
        }
        repo.write(
            "firmware/reference/Cargo.toml",
            "[workspace]\nmembers = [\"client\", \"stub\"]\n\n[patch.crates-io]\ntls = { path = \"../../../tls\" }\n",
        );
        repo.write(
            "firmware/reference/client/Cargo.toml",
            "[package]\nname = \"client\"\n\n[dependencies]\nnats = { path = \"../../../nats\" }\nstub = { path = \"../stub\" }\n",
        );
        repo.write(
            "firmware/reference/stub/Cargo.toml",
            "[package]\nname = \"stub\"\n",
        );
        repo.write(
            "nats/Cargo.toml",
            "[package]\nname = \"nats\"\n\n[dependencies]\nproto = { path = \"../proto\" }\n",
        );
        repo.write("proto/Cargo.toml", "[package]\nname = \"proto\"\n");
        assert_eq!(
            workspace_inputs(&repo.0, "firmware/reference"),
            Ok(paths(&["firmware/reference", "nats", "proto"]))
        );
    }

    #[test]
    fn a_group_runs_when_the_change_reaches_its_inputs_or_nothing_changed() {
        let inputs = paths(&["esp", "mcu-console"]);
        let runs = |changed: &[&str]| reaches(&paths(changed), &inputs);
        assert!(runs(&[]));
        assert!(runs(&["docs/a.md", "esp/x/src/lib.rs"]));
        assert!(runs(&["mcu-console/src/lib.rs"]));
        assert!(!runs(&["docs/a.md", "docs/b.rs"]));
        assert!(!runs(&["espresso/a.md", "mcu-console-extra/a.rs"]));
    }

    #[test]
    fn a_root_file_that_is_not_markdown_and_anything_in_cargo_or_xtask_reach_every_group() {
        let runs = |changed: &[&str]| reaches(&paths(changed), &[]);
        assert!(runs(&["Cargo.toml"]));
        assert!(runs(&["Cargo.lock"]));
        assert!(runs(&["rust-toolchain.toml"]));
        assert!(runs(&[".cargo/config.toml"]));
        assert!(runs(&["xtask/src/shared/prose.rs"]));
        assert!(runs(&["xtask/Cargo.toml"]));
        assert!(!runs(&["README.md"]));
        assert!(!runs(&["docs/Cargo.toml"]));
        assert!(!runs(&["xtasks/a.rs"]));
    }

    #[test]
    fn only_markdown_needs_at_least_one_path_all_markdown_and_none_named() {
        let named = paths(&["nats-it/samples"]);
        let only = |changed: &[&str]| only_markdown(&paths(changed), &named);
        assert!(only(&["README.md", "docs/a.md"]));
        assert!(!only(&[]));
        assert!(!only(&["docs/a.md", "src/lib.rs"]));
        assert!(!only(&["docs/a.md", "nats-it/samples/one/README.md"]));
        assert!(!only(&["docs/a.markdown"]));
    }
}
