// A copy from embedded-devkit/xtask/src/shared. Edit it there, then run `cargo xtask sync` in the devkit.

//! Writes the git hooks that run the checks.

use std::fs;
use std::path::Path;

const PRE_COMMIT: &str = "#!/bin/sh\ncargo xtask check && exec cargo xtask prose\n";
const PRE_PUSH: &str = "#!/bin/sh\ncargo xtask check --push && exec cargo xtask mutants --push\n";
/// The pre-push hook of a repo with a `cargo xtask check` that does not take `--push`, and runs every check.
const PRE_PUSH_WHOLE_CHECK: &str =
    "#!/bin/sh\ncargo xtask check && exec cargo xtask mutants --push\n";
const COMMIT_MSG: &str =
    "#!/bin/sh\ncargo xtask prose --message \"$1\" && exec cargo xtask links --message \"$1\"\n";

/// Writes the git hooks into `.git/hooks`.
///
/// The pre-commit hook runs `cargo xtask check`, then the prose check again so that its warnings are the last lines
/// printed. The pre-push hook runs `cargo xtask check --push`, then cargo-mutants over what the branch changed. When
/// the repo's `cargo xtask check` does not take `--push`, it runs every check, so the pre-push hook runs it in place
/// of `--push`. The commit-msg hook runs the prose check and the requirement ID check
/// over the message.
pub fn install(root: &Path) -> Result<(), String> {
    write_hooks(root, takes_push(root))
}

/// Whether the repo's `cargo xtask check --help` lists `--push`.
fn takes_push(root: &Path) -> bool {
    std::process::Command::new("cargo")
        .current_dir(root)
        .args(["xtask", "check", "--help"])
        .output()
        .is_ok_and(|output| String::from_utf8_lossy(&output.stdout).contains("--push"))
}

/// Writes the hooks. `push` says whether the repo's `cargo xtask check` takes `--push`.
fn write_hooks(root: &Path, push: bool) -> Result<(), String> {
    let hooks = root.join(".git").join("hooks");
    if !hooks.is_dir() {
        return Err(format!(
            "{} does not exist. Is this a git repository?",
            hooks.display()
        ));
    }
    let pre_push = if push { PRE_PUSH } else { PRE_PUSH_WHOLE_CHECK };
    let scripts = [
        ("pre-commit", PRE_COMMIT),
        ("pre-push", pre_push),
        ("commit-msg", COMMIT_MSG),
    ];
    for (name, script) in scripts {
        let path = hooks.join(name);
        fs::write(&path, script).map_err(|error| format!("{}: {error}", path.display()))?;
        make_executable(&path)?;
        println!("wrote {}", path.display());
    }
    Ok(())
}

#[cfg(unix)]
fn make_executable(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o755))
        .map_err(|error| format!("{}: {error}", path.display()))
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) -> Result<(), String> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::files::testing::TempRepo;
    use super::*;

    fn read(repo: &TempRepo, hook: &str) -> Option<String> {
        fs::read_to_string(repo.0.join(".git/hooks").join(hook)).ok()
    }

    #[test]
    fn the_hooks_run_the_quick_checks_per_commit_and_the_slow_ones_per_push() {
        let repo = TempRepo::new("hooks-push");
        fs::create_dir_all(repo.0.join(".git/hooks")).unwrap();
        write_hooks(&repo.0, true).unwrap();
        assert_eq!(read(&repo, "pre-commit").as_deref(), Some(PRE_COMMIT));
        assert_eq!(read(&repo, "pre-push").as_deref(), Some(PRE_PUSH));
        assert_eq!(read(&repo, "commit-msg").as_deref(), Some(COMMIT_MSG));
        assert!(PRE_COMMIT.contains("cargo xtask check &&") && !PRE_COMMIT.contains("mutants"));
        assert!(PRE_PUSH.contains("check --push") && PRE_PUSH.contains("mutants --push"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(repo.0.join(".git/hooks/pre-push"))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o755);
        }
    }

    #[test]
    fn a_repo_without_check_push_runs_its_whole_check_and_the_mutants_per_push() {
        let repo = TempRepo::new("hooks-no-push");
        fs::create_dir_all(repo.0.join(".git/hooks")).unwrap();
        write_hooks(&repo.0, false).unwrap();
        assert_eq!(
            read(&repo, "pre-push").as_deref(),
            Some("#!/bin/sh\ncargo xtask check && exec cargo xtask mutants --push\n")
        );
        assert_eq!(read(&repo, "pre-commit").as_deref(), Some(PRE_COMMIT));
    }

    #[test]
    fn a_folder_without_git_hooks_is_refused() {
        let repo = TempRepo::new("hooks-no-git");
        let error = write_hooks(&repo.0, true).unwrap_err();
        assert!(
            error.ends_with("does not exist. Is this a git repository?"),
            "{error}"
        );
    }
}
