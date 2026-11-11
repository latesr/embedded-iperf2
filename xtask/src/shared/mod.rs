// A copy from embedded-devkit/xtask/src/shared. Edit it there, then run `cargo xtask sync` in the devkit.

//! The commands every repo shares. A repo's `main.rs` flattens [`Command`] into its own commands.

mod adrs;
pub mod checks;
pub(crate) mod files;
pub(crate) mod hooks;
pub mod host;
mod ids;
pub mod links;
mod mutants;
pub mod prose;
mod renumber;
pub mod size;
pub mod sweep;
pub mod toolchain;

use clap::Subcommand;
use std::path::{Path, PathBuf};

/// The shared commands.
#[derive(Subcommand, Debug, PartialEq)]
pub enum Command {
    /// Check relative links, heading anchors, and requirement IDs, or the requirement IDs in one commit message file.
    Links {
        /// A commit message file to check the requirement IDs of, in place of the repo.
        #[arg(long)]
        message: Option<PathBuf>,
    },
    /// Check markdown, rustdoc, and code comments for banned phrases, or one commit message file. Warns on the
    /// paragraphs that differ from HEAD.
    Prose {
        /// A commit message file to check instead of the repo.
        #[arg(long)]
        message: Option<PathBuf>,
        /// Warn on the whole tree.
        #[arg(long)]
        warn_all: bool,
        /// The tree to check instead of this repo, such as a worktree of an upstream project.
        #[arg(long)]
        root: Option<PathBuf>,
    },
    /// Write the git hooks that run the checks.
    InstallHooks,
    /// Run cargo-mutants over the staged lines, or over the lines that differ from a base commit.
    Mutants {
        /// Test the staged changes against HEAD.
        #[arg(long, conflicts_with_all = ["base", "push"])]
        staged: bool,
        /// Test what the branch changed since its upstream, or else since origin/HEAD. The pre-push hook uses it.
        #[arg(long, conflicts_with = "base")]
        push: bool,
        /// Test the working tree against this commit.
        #[arg(long, default_value = "HEAD")]
        base: String,
    },
    /// Renumber the requirement IDs so each letter counts up from 1 in the order REQUIREMENTS.md defines them, in
    /// every file the ID check reads. Needs a tree without uncommitted changes.
    Renumber,
}

/// Runs one shared command in the repo at `root`.
pub fn run(root: &Path, command: Command) -> Result<(), String> {
    match command {
        Command::Links { message: None } => links::run(root),
        Command::Links {
            message: Some(path),
        } => {
            let problems = ids::check_message(root, &path)?;
            if problems.is_empty() {
                Ok(())
            } else {
                Err(problems.join("\n"))
            }
        }
        Command::Prose {
            message,
            warn_all,
            root: tree,
        } => prose::run_with(
            tree.as_deref().unwrap_or(root),
            message.as_deref(),
            warn_all,
        ),
        Command::InstallHooks => hooks::install(root),
        Command::Mutants { staged, push, base } => {
            let base = if push {
                mutants::push_base(root)?
            } else {
                base
            };
            mutants::run(root, staged, &base)
        }
        Command::Renumber => renumber::run(root),
    }
}

/// Fails with an install hint when the cargo subcommand is not installed.
pub fn require_tool(root: &Path, subcommand: &str, install: &str) -> Result<(), String> {
    let found = std::process::Command::new("cargo")
        .current_dir(root)
        .args([subcommand, "--version"])
        .output()
        .is_ok_and(|output| output.status.success());
    if found {
        Ok(())
    } else {
        Err(format!(
            "cargo {subcommand} is not installed. Run `{install}`."
        ))
    }
}

/// Runs git in `root` and returns its output. Fails with git's error output when it exits with a failure.
///
/// Git gives a hook `GIT_DIR`, `GIT_WORK_TREE`, and an index in `GIT_INDEX_FILE`, and a commit with a pathspec
/// makes the index a temporary one. They name the hook's repo, and `root` can be another, such as a scratch
/// repo a test creates. So git finds the repo from `root`, and the index reaches git only when it lies in the
/// git folder of the repo at `root`.
pub fn git(root: &Path, args: &[&str]) -> Result<Vec<u8>, String> {
    git_with_index(root, args, std::env::var_os("GIT_INDEX_FILE").as_deref())
}

/// Runs git in `root` without the variables that name a repo, and with `inherited` as its `GIT_INDEX_FILE`
/// when that index lies in the git folder of the repo at `root`.
fn git_with_index(
    root: &Path,
    args: &[&str],
    inherited: Option<&std::ffi::OsStr>,
) -> Result<Vec<u8>, String> {
    let mut command = std::process::Command::new("git");
    command.current_dir(root).args(args);
    clear_repo_env(&mut command)?;
    let cwd = std::env::current_dir().map_err(|error| format!("no current directory: {error}"))?;
    // A folder that is not yet a repo does not have a git folder, so no inherited index is its own.
    if let Some(index) = inherited
        && let Ok(git_dir) = git_with_index(root, &["rev-parse", "--absolute-git-dir"], None)
        && index_belongs_to(
            Path::new(String::from_utf8_lossy(&git_dir).trim()),
            Path::new(index),
            &cwd,
        )
    {
        command.env("GIT_INDEX_FILE", index);
    }
    let output = command
        .output()
        .map_err(|error| format!("could not run git: {error}"))?;
    if output.status.success() {
        Ok(output.stdout)
    } else {
        Err(format!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}

/// Removes from `command` every variable that names a repo, as `git rev-parse --local-env-vars` lists them.
/// Git clears the same list when it runs in a submodule.
fn clear_repo_env(command: &mut std::process::Command) -> Result<(), String> {
    static NAMES: std::sync::OnceLock<Result<Vec<String>, String>> = std::sync::OnceLock::new();
    let names = NAMES.get_or_init(|| {
        let output = std::process::Command::new("git")
            .args(["rev-parse", "--local-env-vars"])
            .output()
            .map_err(|error| format!("could not run git: {error}"))?;
        if !output.status.success() {
            return Err(format!(
                "git rev-parse --local-env-vars failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        Ok(String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(str::to_owned)
            .collect())
    });
    for name in names.as_ref().map_err(Clone::clone)? {
        command.env_remove(name);
    }
    Ok(())
}

/// Whether `index`, resolved against `cwd` when relative, is a file under `git_dir`. The index's folder is
/// canonicalised rather than the index, because a temporary index may be gone by the time this runs.
fn index_belongs_to(git_dir: &Path, index: &Path, cwd: &Path) -> bool {
    let index = cwd.join(index);
    let (Ok(git_dir), Some(folder)) = (git_dir.canonicalize(), index.parent()) else {
        return false;
    };
    folder
        .canonicalize()
        .is_ok_and(|folder| folder.starts_with(git_dir))
}

#[cfg(test)]
mod tests {
    use super::files::testing::TempRepo;
    use super::*;
    use clap::Parser;

    #[derive(Parser)]
    struct Cli {
        #[command(subcommand)]
        command: Command,
    }

    fn parse(args: &[&str]) -> Result<Command, clap::Error> {
        Cli::try_parse_from(std::iter::once("xtask").chain(args.iter().copied()))
            .map(|cli| cli.command)
    }

    #[test]
    fn the_hooks_command_lines_parse() {
        assert_eq!(
            parse(&["mutants", "--staged"]).unwrap(),
            Command::Mutants {
                staged: true,
                push: false,
                base: "HEAD".to_owned()
            }
        );
        assert_eq!(
            parse(&["mutants", "--push"]).unwrap(),
            Command::Mutants {
                staged: false,
                push: true,
                base: "HEAD".to_owned()
            }
        );
        assert!(parse(&["mutants", "--push", "--staged"]).is_err());
        assert!(parse(&["mutants", "--push", "--base", "main"]).is_err());
        assert_eq!(
            parse(&["prose", "--message", "MSG"]).unwrap(),
            Command::Prose {
                message: Some(PathBuf::from("MSG")),
                warn_all: false,
                root: None,
            }
        );
        assert_eq!(
            parse(&["prose", "--warn-all"]).unwrap(),
            Command::Prose {
                message: None,
                warn_all: true,
                root: None,
            }
        );
        assert_eq!(
            parse(&["prose", "--root", "DIR"]).unwrap(),
            Command::Prose {
                message: None,
                warn_all: false,
                root: Some(PathBuf::from("DIR")),
            }
        );
        assert_eq!(
            parse(&["links", "--message", "MSG"]).unwrap(),
            Command::Links {
                message: Some(PathBuf::from("MSG"))
            }
        );
        assert_eq!(parse(&["install-hooks"]).unwrap(), Command::InstallHooks);
        assert_eq!(parse(&["renumber"]).unwrap(), Command::Renumber);
        assert!(parse(&["mutants", "--staged", "--base", "main"]).is_err());
    }

    #[test]
    fn run_sends_each_command_to_its_check() {
        let repo = TempRepo::new("shared-run");
        let links = || Command::Links { message: None };
        let prose = |warn_all| Command::Prose {
            message: None,
            warn_all,
            root: None,
        };
        repo.write("README.md", "[gone](missing.md)\n");
        let error = run(&repo.0, links()).unwrap_err();
        assert!(error.contains("missing.md"), "{error}");
        assert_eq!(run(&repo.0, prose(false)), Ok(()));

        let other = TempRepo::new("shared-run-other");
        other.write("README.md", "A robust page.\n");
        let elsewhere = Command::Prose {
            message: None,
            warn_all: false,
            root: Some(other.0.clone()),
        };
        let error = run(&repo.0, elsewhere).unwrap_err();
        assert!(error.contains("robust"), "{error}");

        repo.write("README.md", "A robust page.\n");
        assert_eq!(run(&repo.0, links()), Ok(()));
        let error = run(&repo.0, prose(true)).unwrap_err();
        assert!(error.contains("robust"), "{error}");

        repo.write("REQUIREMENTS.md", "**C-1** One.\n");
        repo.write("README.md", "As C-2 says.\n");
        let error = run(&repo.0, links()).unwrap_err();
        assert!(error.contains("cites C-2"), "{error}");

        repo.write("README.md", "As C-1 says.\n");
        repo.write("message.txt", "Follow C-3\n");
        let message = Command::Links {
            message: Some(repo.0.join("message.txt")),
        };
        let error = run(&repo.0, message).unwrap_err();
        assert!(error.contains("cites C-3"), "{error}");
    }

    #[test]
    fn an_index_belongs_to_the_repo_whose_git_folder_holds_it() {
        let repo = TempRepo::new("shared-index");
        let other = TempRepo::new("shared-index-other");
        std::fs::create_dir(repo.0.join(".git")).unwrap();
        std::fs::create_dir(other.0.join(".git")).unwrap();
        let index = repo.0.join(".git/next-index-1.lock");
        assert!(index_belongs_to(&repo.0.join(".git"), &index, &other.0));
        assert!(!index_belongs_to(&other.0.join(".git"), &index, &repo.0));
        assert!(!index_belongs_to(
            Path::new("/nonexistent"),
            &index,
            &repo.0
        ));
    }

    /// The index git reports is the inherited one only when it lies under the repo. A relative index resolves
    /// against the current directory, and the current directory is not the scratch repo.
    #[test]
    fn a_scratch_repo_keeps_only_an_index_of_its_own() {
        let repo = TempRepo::new("shared-git-index");
        git(&repo.0, &["init", "--quiet"]).unwrap();
        let own = repo.0.join(".git").join("next-index-1.lock");
        let reported = |inherited: Option<&Path>| {
            let output = git_with_index(
                &repo.0,
                &["rev-parse", "--git-path", "index"],
                inherited.map(Path::as_os_str),
            )
            .unwrap();
            String::from_utf8(output).unwrap().trim().to_owned()
        };
        assert_eq!(reported(Some(&own)), own.to_str().unwrap());
        assert_eq!(
            reported(Some(Path::new("/nonexistent/index"))),
            ".git/index"
        );
        assert_eq!(reported(Some(Path::new("next-index-2.lock"))), ".git/index");
        assert_eq!(reported(None), ".git/index");
    }

    /// A hook runs with `GIT_DIR` and `GIT_WORK_TREE` naming the hook's repo. In a worktree they are absolute,
    /// so without the clear a scratch repo's git would write to the hook's repo.
    #[test]
    fn a_scratch_repo_ignores_variables_that_name_another_repo() {
        let scratch = TempRepo::new("shared-env-scratch");
        let other = TempRepo::new("shared-env-other");
        git(&scratch.0, &["init", "--quiet"]).unwrap();
        git(&other.0, &["init", "--quiet"]).unwrap();
        let mut command = std::process::Command::new("git");
        command
            .current_dir(&scratch.0)
            .args(["config", "user.name", "scratch"])
            .env("GIT_DIR", other.0.join(".git"))
            .env("GIT_WORK_TREE", &other.0);
        clear_repo_env(&mut command).unwrap();
        assert!(command.status().unwrap().success());
        let name = |repo: &TempRepo| {
            let output = git(
                &repo.0,
                &["config", "--local", "--default", "", "user.name"],
            )
            .unwrap();
            String::from_utf8(output).unwrap().trim().to_owned()
        };
        assert_eq!(name(&scratch), "scratch");
        assert_eq!(name(&other), "");
    }

    /// A hook's index is inherited by every git the tests run, including the one that makes a scratch repo.
    #[test]
    fn a_repo_can_be_made_while_an_index_is_inherited() {
        let folder = TempRepo::new("shared-env-init");
        let inherited = folder.0.with_file_name("elsewhere/.git/index");
        assert!(
            git_with_index(&folder.0, &["init", "--quiet"], Some(inherited.as_os_str())).is_ok()
        );
        assert!(folder.0.join(".git/HEAD").is_file());
    }

    /// A worktree's index lies under the main repo's `.git/worktrees`, outside the worktree's folder.
    #[test]
    fn a_worktree_keeps_the_index_in_its_git_folder() {
        let repo = TempRepo::new("shared-worktree");
        let worktree = repo
            .0
            .with_file_name(format!("xtask-shared-worktree-wt-{}", std::process::id()));
        git(&repo.0, &["init", "--quiet"]).unwrap();
        git(
            &repo.0,
            &[
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@t",
                "commit",
                "--quiet",
                "--allow-empty",
                "-m",
                "start",
            ],
        )
        .unwrap();
        git(
            &repo.0,
            &[
                "worktree",
                "add",
                "--quiet",
                "--detach",
                worktree.to_str().unwrap(),
            ],
        )
        .unwrap();
        let own = repo
            .0
            .join(".git/worktrees")
            .join(worktree.file_name().unwrap())
            .join("index");
        let output = git_with_index(
            &worktree,
            &["rev-parse", "--git-path", "index"],
            Some(own.as_os_str()),
        );
        let _ = std::fs::remove_dir_all(&worktree);
        assert_eq!(
            String::from_utf8(output.unwrap()).unwrap().trim(),
            own.to_str().unwrap()
        );
    }

    #[test]
    fn a_relative_index_is_resolved_against_the_current_directory() {
        let repo = TempRepo::new("shared-index-relative");
        let other = TempRepo::new("shared-index-relative-other");
        std::fs::create_dir(repo.0.join(".git")).unwrap();
        std::fs::create_dir(other.0.join(".git")).unwrap();
        let index = Path::new(".git/index");
        assert!(index_belongs_to(&repo.0.join(".git"), index, &repo.0));
        assert!(!index_belongs_to(&repo.0.join(".git"), index, &other.0));
    }

    #[test]
    fn require_tool_names_the_install_command_for_a_missing_tool() {
        let repo = TempRepo::new("shared-tool");
        assert_eq!(require_tool(&repo.0, "fmt", "unused"), Ok(()));
        assert_eq!(
            require_tool(
                &repo.0,
                "no-such-tool-xyz",
                "cargo install no-such-tool-xyz"
            ),
            Err(
                "cargo no-such-tool-xyz is not installed. Run `cargo install no-such-tool-xyz`."
                    .to_owned()
            )
        );
    }
}
