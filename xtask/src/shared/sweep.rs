// A copy from embedded-conventions/xtask/src/shared. Edit it there, then run `cargo xtask sync` there.

//! Removes old build files once a day, through cargo-sweep. Cargo never removes a build, so `target/` fills with
//! one for each feature set and each version of each crate, and rustc reads the metadata of every one for each
//! extern crate it links.

// A repo calls `daily` once its check.rs is updated. Until then the sync commit must pass.
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime};

/// How long a sweep lasts before the next one is due.
const DAY: Duration = Duration::from_secs(86_400);

/// The file under the repo root with a modification time that records the last sweep.
const STAMP: &str = "target/.sweep-stamp";

/// Runs `cargo sweep --time 7` over the repo when the last sweep was a day ago or more, and records the sweep. It
/// sweeps every Cargo project under `root`, and each of `folders`. A folder in `folders` has its own `target/`,
/// which a repo's check builds into and a recursive sweep does not find. Without cargo-sweep, or when a sweep
/// fails, it says so and returns, so the check continues.
// It only starts cargo-sweep and records the sweep. due and sweeps hold the decisions, and are tested.
#[mutants::skip]
pub fn daily(root: &Path, folders: &[&str]) {
    let stamp = root.join(STAMP);
    let last = std::fs::metadata(&stamp)
        .and_then(|metadata| metadata.modified())
        .ok();
    if !due(last, SystemTime::now()) {
        return;
    }
    if super::require_tool(root, "sweep", "cargo install cargo-sweep --locked").is_err() {
        println!(
            "==> cargo-sweep is not installed, so old builds stay. Run `cargo install cargo-sweep --locked`."
        );
        return;
    }
    for sweep in sweeps(root, folders) {
        let mut command = Command::new("cargo");
        command.current_dir(root).args(&sweep.args);
        if let Some(target) = &sweep.target {
            command.env("CARGO_TARGET_DIR", target);
        }
        if let Err(error) = super::checks::step(&sweep.name, &mut command) {
            println!("==> {error}, so old builds stay");
            return;
        }
    }
    // The file's modification time is the record, and writing the file sets it.
    let _ = std::fs::create_dir_all(root.join("target"));
    let _ = std::fs::write(&stamp, "");
}

/// One `cargo` run of cargo-sweep: its step's name, its arguments, and the build folder it sweeps when that is not
/// the project's usual one.
#[derive(Debug, PartialEq, Eq)]
struct Sweep {
    name: String,
    args: Vec<String>,
    target: Option<PathBuf>,
}

/// The sweeps of every Cargo project under `root`, and of each of `folders` in its own `target/`, of build files
/// that no build has used in 7 days.
fn sweeps(root: &Path, folders: &[&str]) -> Vec<Sweep> {
    let args = |rest: &[&str], path: &Path| {
        ["sweep", "--time", "7"]
            .iter()
            .chain(rest)
            .map(|arg| (*arg).to_owned())
            .chain([path.display().to_string()])
            .collect()
    };
    let mut sweeps = vec![Sweep {
        name: "sweep".to_owned(),
        args: args(&["--recursive"], root),
        target: None,
    }];
    for folder in folders {
        let path = root.join(folder);
        sweeps.push(Sweep {
            name: format!("sweep {folder}"),
            args: args(&[], &path),
            target: Some(path.join("target")),
        });
    }
    sweeps
}

/// Whether a sweep is due, given when the last one ran, or `None` when none has.
fn due(last: Option<SystemTime>, now: SystemTime) -> bool {
    last.and_then(|last| now.duration_since(last).ok())
        .is_none_or(|age| age >= DAY)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared::files::testing::TempRepo;

    #[test]
    fn a_sweep_runs_only_when_due_and_then_records_itself() {
        let repo = TempRepo::new("sweep");
        std::fs::create_dir_all(repo.0.join("target")).unwrap();
        let stamp = repo.0.join(STAMP);
        let set_age = |age: Duration| {
            std::fs::write(&stamp, "").unwrap();
            std::fs::File::options()
                .write(true)
                .open(&stamp)
                .unwrap()
                .set_modified(SystemTime::now() - age)
                .unwrap();
        };
        let age = || {
            let modified = std::fs::metadata(&stamp).unwrap().modified().unwrap();
            SystemTime::now().duration_since(modified).unwrap()
        };
        set_age(Duration::from_secs(60 * 60));
        daily(&repo.0, &[]);
        assert!(age() >= Duration::from_secs(60 * 60));
        set_age(2 * DAY);
        daily(&repo.0, &[]);
        let installed = crate::shared::require_tool(&repo.0, "sweep", "").is_ok();
        assert_eq!(age() < DAY, installed);
    }

    #[test]
    fn a_sweep_is_due_with_no_stamp_or_one_a_day_old_and_not_sooner() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(10 * 24 * 60 * 60);
        assert!(due(None, now));
        assert!(due(Some(now - DAY), now));
        assert!(!due(Some(now - DAY + Duration::from_secs(1)), now));
        // A stamp from the future, as after a clock change, is not a reason to wait.
        assert!(due(Some(now + DAY), now));
    }

    #[test]
    fn the_sweeps_cover_every_project_under_the_root_and_each_folder_s_own_target() {
        let args = |args: &[&str]| args.iter().map(|arg| (*arg).to_owned()).collect::<Vec<_>>();
        assert_eq!(
            sweeps(Path::new("/repo"), &["esp/board"]),
            [
                Sweep {
                    name: "sweep".to_owned(),
                    args: args(&["sweep", "--time", "7", "--recursive", "/repo"]),
                    target: None,
                },
                Sweep {
                    name: "sweep esp/board".to_owned(),
                    args: args(&["sweep", "--time", "7", "/repo/esp/board"]),
                    target: Some(PathBuf::from("/repo/esp/board/target")),
                },
            ]
        );
    }
}
