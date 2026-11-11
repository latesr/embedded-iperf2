// A copy from embedded-devkit/xtask/src/shared. Edit it there, then run `cargo xtask sync` in the devkit.

//! Runs cargo-mutants over the staged lines, or over the lines that differ from a base commit.

use super::git;
use std::path::Path;
use std::process::Command;

/// Writes the diff to `target/mutants.diff`, then runs cargo-mutants over it. With `staged`, the diff is the
/// staged changes against `HEAD`. Without it, the diff is the working tree against `base`.
pub fn run(root: &Path, staged: bool, base: &str) -> Result<(), String> {
    let diff = diff(root, staged, base)?;
    if diff.is_empty() {
        println!("no lines to test");
        return Ok(());
    }
    super::require_tool(root, "mutants", "cargo install cargo-mutants")?;
    let diff_path = root.join("target").join("mutants.diff");
    std::fs::write(&diff_path, diff)
        .map_err(|error| format!("{}: {error}", diff_path.display()))?;
    let jobs = jobs();
    println!("==> mutants, {jobs} at once");
    let status = command(root, &diff_path, jobs)
        .status()
        .map_err(|error| format!("could not start cargo mutants: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err("cargo mutants found surviving mutants or failed".to_owned())
    }
}

/// The commit that `--push` tests the changes since, from [`super::checks::push_base`]. Fails without one, as
/// before a repo's first push, because the run would then mutate every line in the repo.
pub fn push_base(root: &Path) -> Result<String, String> {
    super::checks::push_base(root).ok_or_else(|| {
        "the branch has no upstream and the repo has no origin/HEAD, so nothing marks what the push adds. Run \
         `cargo xtask check --push` and `cargo xtask mutants --base <commit>`, with the last commit that passed the \
         mutation check. Then the user pushes once with `--no-verify`"
            .to_owned()
    })
}

/// The cargo-mutants run over the diff at `diff_path`, with every feature on, because a test behind a feature
/// does not run in a default build and every mutant it covers would survive.
fn command(root: &Path, diff_path: &Path, jobs: usize) -> Command {
    let mut command = Command::new("cargo");
    command
        .current_dir(root)
        .arg("mutants")
        .arg("--in-diff")
        .arg(diff_path)
        .arg("--jobs")
        .arg(jobs.to_string())
        .arg("--all-features");
    command
}

/// The mutants to build and test at once: the cores divided by three, and at least one and at most three. The
/// cargo-mutants book says to "set the number of jobs very conservatively, starting at -j2 or -j3". The build and
/// test tools parallelise on their own, and a high count gives spurious timeouts, which count as surviving
/// mutants. A 4-vCPU CI runner gets one job and a 10-core laptop three.
fn jobs() -> usize {
    std::thread::available_parallelism()
        .map_or(1, |cores| cores.get() / 3)
        .clamp(1, 3)
}

/// The lines to test. New files are not in a plain diff of the working tree, so without `staged` the untracked ones
/// are marked intent-to-add first. That changes the index and nothing else. With `staged` the index is left alone,
/// because the pre-commit hook must commit exactly what the author staged.
fn diff(root: &Path, staged: bool, base: &str) -> Result<Vec<u8>, String> {
    if staged {
        return git(root, &["diff", "--cached"]);
    }
    git(root, &["add", "--intent-to-add", "--all"])?;
    git(root, &["diff", base])
}

#[cfg(test)]
mod tests {
    use super::super::files::testing::TempRepo;
    use super::*;

    /// A repo with one committed file, `a.rs`.
    fn repo(name: &str) -> TempRepo {
        let repo = TempRepo::new(name);
        for args in [
            &["init", "--quiet"][..],
            &["config", "user.email", "test@example.com"],
            &["config", "user.name", "test"],
        ] {
            git(&repo.0, args).unwrap();
        }
        repo.write("a.rs", "fn a() {}\n");
        git(&repo.0, &["add", "a.rs"]).unwrap();
        git(&repo.0, &["commit", "--quiet", "-m", "a"]).unwrap();
        repo
    }

    fn text(diff: Vec<u8>) -> String {
        String::from_utf8(diff).unwrap()
    }

    #[test]
    fn staged_holds_the_staged_lines_and_not_the_unstaged_ones() {
        let repo = repo("mutants-staged");
        repo.write("a.rs", "fn a() { staged(); }\n");
        git(&repo.0, &["add", "a.rs"]).unwrap();
        repo.write("a.rs", "fn a() { staged(); unstaged(); }\n");
        repo.write("b.rs", "fn b() {}\n");

        let diff = text(diff(&repo.0, true, "HEAD").unwrap());
        assert!(diff.contains("+fn a() { staged(); }"), "{diff}");
        assert!(!diff.contains("unstaged"), "{diff}");
        assert!(!diff.contains("b.rs"), "{diff}");
        assert_eq!(
            text(git(&repo.0, &["status", "--porcelain"]).unwrap()),
            "MM a.rs\n?? b.rs\n"
        );
    }

    #[test]
    fn a_base_diff_holds_the_working_tree_and_new_files() {
        let repo = repo("mutants-base");
        repo.write("a.rs", "fn a() { changed(); }\n");
        repo.write("b.rs", "fn b() {}\n");

        let diff = text(diff(&repo.0, false, "HEAD").unwrap());
        assert!(diff.contains("+fn a() { changed(); }"), "{diff}");
        assert!(diff.contains("+fn b() {}"), "{diff}");
    }

    #[test]
    fn a_push_without_an_upstream_or_origin_head_says_to_name_a_base() {
        let repo = repo("mutants-no-base");
        let error = push_base(&repo.0).unwrap_err();
        assert_eq!(
            error,
            "the branch has no upstream and the repo has no origin/HEAD, so nothing marks what the push adds. \
             Run `cargo xtask check --push` and `cargo xtask mutants --base <commit>`, with the last commit that \
             passed the mutation check. Then the user pushes once with `--no-verify`"
        );
        let head = String::from_utf8(git(&repo.0, &["rev-parse", "HEAD"]).unwrap())
            .unwrap()
            .trim()
            .to_owned();
        git(&repo.0, &["update-ref", "refs/remotes/origin/main", &head]).unwrap();
        git(
            &repo.0,
            &[
                "symbolic-ref",
                "refs/remotes/origin/HEAD",
                "refs/remotes/origin/main",
            ],
        )
        .unwrap();
        assert_eq!(push_base(&repo.0), Ok(head));
    }

    #[test]
    fn a_base_diff_covers_every_commit_since_the_base() {
        let repo = repo("mutants-range");
        repo.write("a.rs", "fn a() { committed(); }\n");
        git(&repo.0, &["commit", "--quiet", "-am", "b"]).unwrap();

        let since_first = text(diff(&repo.0, false, "HEAD~1").unwrap());
        assert!(
            since_first.contains("+fn a() { committed(); }"),
            "{since_first}"
        );
        assert_eq!(text(diff(&repo.0, false, "HEAD").unwrap()), "");
    }

    #[test]
    fn the_run_turns_every_feature_on() {
        let command = command(
            Path::new("/repo"),
            Path::new("/repo/target/mutants.diff"),
            2,
        );
        let args: Vec<_> = command.get_args().collect();
        assert_eq!(
            args,
            [
                "mutants",
                "--in-diff",
                "/repo/target/mutants.diff",
                "--jobs",
                "2",
                "--all-features"
            ]
        );
        assert_eq!(command.get_current_dir(), Some(Path::new("/repo")));
    }

    #[test]
    fn nothing_staged_succeeds_without_running_cargo_mutants() {
        // The temp repo does not have a Cargo.toml, so a cargo-mutants run there would fail.
        let repo = repo("mutants-empty");
        repo.write("a.rs", "fn a() { unstaged(); }\n");
        assert_eq!(run(&repo.0, true, "HEAD"), Ok(()));
    }
}
