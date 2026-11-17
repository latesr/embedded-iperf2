// A copy from embedded-conventions/xtask/src/shared. Edit it there, then run `cargo xtask sync` there.

//! Runs cargo-mutants over the staged lines, over the lines that differ from a base commit, or over every line.

use super::git;
use std::path::Path;
use std::process::Command;

/// Writes the diff to `target/mutants.diff`, then runs cargo-mutants over it in each package that it touches. With
/// `staged`, the diff is the staged changes against `HEAD`. Without it, the diff is the working tree against `base`.
// It only starts cargo-mutants and returns its result. packages, touched and command hold the decisions, and are
// tested.
#[mutants::skip]
pub fn run(root: &Path, staged: bool, base: &str) -> Result<(), String> {
    let diff = diff(root, staged, base)?;
    let packages = touched(&packages(root)?, &String::from_utf8_lossy(&diff));
    if packages.is_empty() {
        println!("no lines to test");
        return Ok(());
    }
    super::require_tool(root, "mutants", "cargo install cargo-mutants")?;
    let diff_path = root.join("target").join("mutants.diff");
    std::fs::write(&diff_path, diff)
        .map_err(|error| format!("{}: {error}", diff_path.display()))?;
    run_each(root, &packages, Some(&diff_path))
}

/// Runs cargo-mutants over every line, in each package of the workspace.
// It only starts cargo-mutants and returns its result.
#[mutants::skip]
pub fn run_all(root: &Path) -> Result<(), String> {
    let packages: Vec<_> = packages(root)?
        .into_iter()
        .map(|package| package.name)
        .collect();
    super::require_tool(root, "mutants", "cargo install cargo-mutants")?;
    run_each(root, &packages, None)
}

// Cargo unifies the features of the packages that one command builds, dev-dependencies included. Two packages
// whose tests need features of one dependency that conflict, such as two embassy-time drivers, do not build
// together, so each package gets its own run.
//
// It only starts cargo-mutants and returns its result.
#[mutants::skip]
fn run_each(root: &Path, packages: &[String], diff_path: Option<&Path>) -> Result<(), String> {
    let jobs = jobs();
    let mut failed = Vec::new();
    for package in packages {
        println!("==> mutants in {package}, {jobs} at once");
        let status = command(root, package, diff_path, jobs)
            .status()
            .map_err(|error| format!("could not start cargo mutants: {error}"))?;
        if !status.success() {
            failed.push(package.as_str());
        }
    }
    if failed.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "cargo mutants found surviving mutants or failed in {}",
            failed.join(", ")
        ))
    }
}

/// A package of the workspace.
#[derive(Debug, PartialEq)]
struct Package {
    /// The name in the package's `Cargo.toml`.
    name: String,
    /// The folder the root `Cargo.toml` lists it under, relative to the root.
    folder: String,
}

/// The packages that the root `Cargo.toml` lists in `workspace.members`, in that order. Fails when a manifest
/// cannot be read or lacks a name, as for a member given as a glob.
fn packages(root: &Path) -> Result<Vec<Package>, String> {
    let manifest = |folder: &Path| -> Result<toml::Table, String> {
        let path = folder.join("Cargo.toml");
        std::fs::read_to_string(&path)
            .map_err(|error| format!("{}: {error}", path.display()))?
            .parse()
            .map_err(|error| format!("{}: {error}", path.display()))
    };
    let workspace = manifest(root)?;
    let members = workspace
        .get("workspace")
        .and_then(|workspace| workspace.get("members"))
        .and_then(toml::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(toml::Value::as_str);
    members
        .map(|folder| {
            let name = manifest(&root.join(folder))?
                .get("package")
                .and_then(|package| package.get("name"))
                .and_then(toml::Value::as_str)
                .map(str::to_owned)
                .ok_or_else(|| format!("{folder}/Cargo.toml has no package name"))?;
            Ok(Package {
                name,
                folder: folder.to_owned(),
            })
        })
        .collect()
}

/// The names of the packages that hold a file the diff adds or changes, in the order of `packages`.
fn touched(packages: &[Package], diff: &str) -> Vec<String> {
    let paths: Vec<_> = diff
        .lines()
        .filter_map(|line| line.strip_prefix("+++ b/"))
        .collect();
    packages
        .iter()
        .filter(|package| {
            paths.iter().any(|path| {
                path.strip_prefix(package.folder.as_str())
                    .is_some_and(|rest| rest.starts_with('/'))
            })
        })
        .map(|package| package.name.clone())
        .collect()
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

/// The cargo-mutants run in `package`, over the diff at `diff_path` or else over every line, with every feature
/// on.
fn command(root: &Path, package: &str, diff_path: Option<&Path>, jobs: usize) -> Command {
    let mut command = Command::new("cargo");
    command
        .current_dir(root)
        .args(["mutants", "--package", package]);
    if let Some(diff_path) = diff_path {
        command.arg("--in-diff").arg(diff_path);
    }
    command
        .arg("--jobs")
        .arg(jobs.to_string())
        .arg("--all-features");
    command
}

/// The mutants to build and test at once: the cores divided by three, and at least one and at most three. The
/// cargo-mutants book says to "set the number of jobs very conservatively, starting at -j2 or -j3". The build and
/// test tools parallelise on their own, and a high count gives spurious timeouts, which count as surviving
/// mutants. A 4-vCPU CI runner gets one job and a 10-core laptop three.
// The count depends on the host's cores, so no test can name the right value.
#[mutants::skip]
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
    fn a_run_tests_one_package_with_every_feature_on() {
        let over_diff = command(
            Path::new("/repo"),
            "alpha",
            Some(Path::new("/repo/target/mutants.diff")),
            2,
        );
        assert_eq!(
            command_args(&over_diff),
            [
                "mutants",
                "--package",
                "alpha",
                "--in-diff",
                "/repo/target/mutants.diff",
                "--jobs",
                "2",
                "--all-features"
            ]
        );
        assert_eq!(over_diff.get_current_dir(), Some(Path::new("/repo")));
        assert_eq!(
            command_args(&command(Path::new("/repo"), "alpha", None, 1)),
            [
                "mutants",
                "--package",
                "alpha",
                "--jobs",
                "1",
                "--all-features"
            ]
        );
    }

    fn command_args(command: &Command) -> Vec<String> {
        command
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect()
    }

    /// A workspace with the members `a` and `tools/b`. Their packages are `alpha` and `beta`.
    fn workspace(name: &str) -> TempRepo {
        let repo = repo(name);
        repo.write(
            "Cargo.toml",
            "[workspace]\nresolver = \"3\"\nmembers = [\"a\", \"tools/b\"]\n",
        );
        std::fs::create_dir_all(repo.0.join("a")).unwrap();
        std::fs::create_dir_all(repo.0.join("tools/b")).unwrap();
        repo.write("a/Cargo.toml", "[package]\nname = \"alpha\"\n");
        repo.write("tools/b/Cargo.toml", "[package]\nname = \"beta\"\n");
        repo
    }

    fn package(name: &str, folder: &str) -> Package {
        Package {
            name: name.to_owned(),
            folder: folder.to_owned(),
        }
    }

    #[test]
    fn the_packages_are_the_members_by_their_names() {
        let repo = workspace("mutants-packages");
        assert_eq!(
            packages(&repo.0),
            Ok(vec![package("alpha", "a"), package("beta", "tools/b")])
        );
        repo.write("Cargo.toml", "[workspace]\nmembers = [\"c\"]\n");
        let error = packages(&repo.0).unwrap_err();
        assert!(error.contains("c/Cargo.toml"), "{error}");
    }

    #[test]
    fn a_diff_touches_the_packages_of_the_files_it_adds_or_changes() {
        let packages = [package("alpha", "a"), package("beta", "tools/b")];
        let diff = |paths: &[&str]| {
            paths
                .iter()
                .map(|path| format!("--- a/{path}\n+++ {path}\n@@ -1 +1 @@\n-x\n+y\n"))
                .collect::<String>()
        };
        assert_eq!(
            touched(
                &packages,
                &diff(&["b/tools/b/src/lib.rs", "b/a/src/lib.rs", "b/a/x.rs"])
            ),
            ["alpha", "beta"]
        );
        assert_eq!(
            touched(
                &packages,
                &diff(&["b/README.md", "b/ab/src/lib.rs", "b/tools/bb.rs"])
            ),
            Vec::<String>::new()
        );
        assert_eq!(
            touched(
                &packages,
                "--- a/a/src/lib.rs\n+++ /dev/null\n@@ -1 +0,0 @@\n-x\n"
            ),
            Vec::<String>::new()
        );
    }

    #[test]
    fn nothing_staged_succeeds_without_running_cargo_mutants() {
        // A cargo-mutants run in the temp repo would fail, because the repo holds no package.
        let repo = workspace("mutants-empty");
        repo.write("a.rs", "fn a() { unstaged(); }\n");
        assert_eq!(run(&repo.0, true, "HEAD"), Ok(()));
    }
}
