// A copy from embedded-devkit/xtask/src/shared. Edit it there, then run `cargo xtask sync` in the devkit.

//! Renumbers the requirement IDs, so each letter's IDs count up from 1 in the order `REQUIREMENTS.md` defines
//! them. Every definition and citation that the ID check reads changes in one pass.

use super::git;
use super::ids::{self, Citation};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Renumbers the IDs in the repo at `root` and prints the IDs that changed as a block for the commit message.
///
/// Fails without writing when the tree has changes or files that are not committed, so `git checkout .` undoes a
/// run. Fails without writing when `REQUIREMENTS.md` is missing or the ID check reports a problem. A failure after
/// writing names the undo.
pub fn run(root: &Path) -> Result<(), String> {
    const UNDO: &str = "Some files are renumbered. Run `git checkout .` to undo the renumber.";
    if !git(root, &["status", "--porcelain"])?.is_empty() {
        return Err(
            "The tree has changes or files that are not committed. Commit them first, so `git checkout .` undoes the renumber."
                .to_owned(),
        );
    }
    let requirements = fs::read_to_string(root.join("REQUIREMENTS.md"))
        .map_err(|error| format!("REQUIREMENTS.md: {error}"))?;
    let problems = ids::check(root);
    if !problems.is_empty() {
        return Err(problems.join("\n"));
    }
    let order = ids::defined_in_order(&requirements);
    let mapping = mapping(&order);
    let rewritten: Vec<(PathBuf, String)> = ids::citations_by_file(root, &ids::letters(&mapping))
        .into_iter()
        .filter_map(|(path, (text, cited))| {
            let new = rewrite(&text, &cited, &mapping);
            (new != text).then_some((path, new))
        })
        .collect();
    for (path, text) in &rewritten {
        fs::write(root.join(path), text)
            .map_err(|error| format!("{}: {error}\n{UNDO}", path.display()))?;
    }
    let problems = ids::check(root);
    if !problems.is_empty() {
        return Err(format!("{}\n{UNDO}", problems.join("\n")));
    }
    print!("{}", report(&order, &mapping, rewritten.len()));
    Ok(())
}

/// The new ID for each ID in `order`. Each letter's IDs count up from 1 in the order of `order`.
fn mapping(order: &[String]) -> BTreeMap<String, String> {
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    let mut mapping = BTreeMap::new();
    for id in order {
        if let Some((letter, _)) = id.split_once('-') {
            let count = counts.entry(letter).or_insert(0);
            *count = count.saturating_add(1);
            mapping.insert(id.clone(), format!("{letter}-{count}"));
        }
    }
    mapping
}

/// `text` with each ID in `cited` replaced by its new ID in `mapping`. The ranges in `cited` are in order and do
/// not overlap.
fn rewrite(text: &str, cited: &[Citation], mapping: &BTreeMap<String, String>) -> String {
    let mut new = String::with_capacity(text.len());
    let mut copied = 0;
    for (range, id) in cited {
        new.push_str(text.get(copied..range.start).unwrap_or(""));
        new.push_str(mapping.get(id).unwrap_or(id));
        copied = range.end;
    }
    new.push_str(text.get(copied..).unwrap_or(""));
    new
}

/// What `run` prints: the IDs that changed, in the order of `order`, in a code block for the commit message.
fn report(order: &[String], mapping: &BTreeMap<String, String>, files: usize) -> String {
    let changed: Vec<String> = order
        .iter()
        .filter_map(|old| {
            mapping
                .get(old)
                .filter(|new| *new != old)
                .map(|new| format!("{old} → {new}\n"))
        })
        .collect();
    if changed.is_empty() {
        return "Every ID is in order. Nothing changed.\n".to_owned();
    }
    format!(
        "Renumbered {} IDs in {files} files. Put this block in the commit message:\n\n```\n{}```\n",
        changed.len(),
        changed.concat()
    )
}

#[cfg(test)]
mod tests {
    use super::super::files::testing::TempRepo;
    use super::*;

    fn ids(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| (*name).to_owned()).collect()
    }

    fn commit_all(repo: &TempRepo) {
        git(&repo.0, &["init", "--quiet"]).unwrap();
        git(&repo.0, &["add", "--all"]).unwrap();
        git(
            &repo.0,
            &[
                "-c",
                "user.name=test",
                "-c",
                "user.email=test@example.invalid",
                "-c",
                "commit.gpgsign=false",
                "commit",
                "--quiet",
                "--no-verify",
                "--message",
                "start",
            ],
        )
        .unwrap();
    }

    fn read(repo: &TempRepo, path: &str) -> String {
        fs::read_to_string(repo.0.join(path)).unwrap()
    }

    #[test]
    fn each_letter_counts_up_from_one_in_document_order() {
        let mapping = mapping(&ids(&["B-3", "A-2", "B-1", "A-1", "B-7"]));
        let pairs: Vec<(&str, &str)> = mapping
            .iter()
            .map(|(old, new)| (old.as_str(), new.as_str()))
            .collect();
        assert_eq!(
            pairs,
            [
                ("A-1", "A-2"),
                ("A-2", "A-1"),
                ("B-1", "B-2"),
                ("B-3", "B-1"),
                ("B-7", "B-3")
            ]
        );
    }

    #[test]
    fn a_rewrite_swaps_ids_in_one_pass() {
        let mapping: BTreeMap<String, String> = [("B-1", "B-2"), ("B-2", "B-1")]
            .iter()
            .map(|(old, new)| ((*old).to_owned(), (*new).to_owned()))
            .collect();
        let cited = [(0..3, "B-2".to_owned()), (9..12, "B-1".to_owned())];
        assert_eq!(rewrite("B-2 then B-1.", &cited, &mapping), "B-1 then B-2.");
    }

    #[test]
    fn the_report_lists_the_changed_ids_in_document_order() {
        let order = ids(&["B-3", "B-1", "A-1"]);
        assert_eq!(
            report(&order, &mapping(&order), 4),
            "Renumbered 2 IDs in 4 files. Put this block in the commit message:\n\n```\nB-3 → B-1\nB-1 → B-2\n```\n"
        );
        let order = ids(&["B-1", "A-1"]);
        assert_eq!(
            report(&order, &mapping(&order), 0),
            "Every ID is in order. Nothing changed.\n"
        );
    }

    #[test]
    fn a_run_renumbers_every_file_the_check_reads() {
        let repo = TempRepo::new("renumber-run");
        fs::create_dir_all(repo.0.join("src")).unwrap();
        fs::create_dir_all(repo.0.join("xtask/src/shared")).unwrap();
        repo.write(
            "REQUIREMENTS.md",
            "- **B-4** Four, after B-2.\n- **B-2** Two.\n- **A-9** Nine.\n",
        );
        repo.write(
            "docs/page.md",
            "Covers: x\n\nB-2 and B-4, and A-9.\n\n```\nB-4\n```\n",
        );
        repo.write(
            "src/lib.rs",
            "// Meets B-4.\nconst S: &str = \"B-4\"; // B-2\n",
        );
        repo.write("CONVENTIONS.md", "Such as B-2.\n");
        repo.write("xtask/src/shared/ids.rs", "// Such as B-2.\n");
        commit_all(&repo);

        assert_eq!(run(&repo.0), Ok(()));
        assert_eq!(
            read(&repo, "REQUIREMENTS.md"),
            "- **B-1** Four, after B-2.\n- **B-2** Two.\n- **A-1** Nine.\n"
        );
        assert_eq!(
            read(&repo, "docs/page.md"),
            "Covers: x\n\nB-2 and B-1, and A-1.\n\n```\nB-4\n```\n"
        );
        assert_eq!(
            read(&repo, "src/lib.rs"),
            "// Meets B-1.\nconst S: &str = \"B-4\"; // B-2\n"
        );
        assert_eq!(read(&repo, "CONVENTIONS.md"), "Such as B-2.\n");
        assert_eq!(read(&repo, "xtask/src/shared/ids.rs"), "// Such as B-2.\n");
    }

    #[test]
    fn a_run_refuses_a_tree_with_changes() {
        let repo = TempRepo::new("renumber-dirty");
        repo.write("REQUIREMENTS.md", "**B-2** Two.\n");
        commit_all(&repo);
        repo.write("docs/page.md", "Covers: x\n\nB-2.\n");
        let error = run(&repo.0).unwrap_err();
        assert!(error.contains("not committed"), "{error}");
        assert_eq!(read(&repo, "REQUIREMENTS.md"), "**B-2** Two.\n");
    }

    #[test]
    fn a_run_refuses_while_the_check_fails() {
        let repo = TempRepo::new("renumber-failing");
        repo.write("REQUIREMENTS.md", "**B-2** Two.\n\n**B-2** Again.\n");
        repo.write("docs/page.md", "Covers: x\n\nB-2.\n");
        commit_all(&repo);
        let error = run(&repo.0).unwrap_err();
        assert!(error.contains("B-2 is defined 2 times"), "{error}");
        assert_eq!(read(&repo, "docs/page.md"), "Covers: x\n\nB-2.\n");
    }

    #[test]
    fn a_run_needs_requirements() {
        let repo = TempRepo::new("renumber-none");
        repo.write("docs/page.md", "Covers: x\n");
        commit_all(&repo);
        let error = run(&repo.0).unwrap_err();
        assert!(error.contains("REQUIREMENTS.md"), "{error}");
    }
}
