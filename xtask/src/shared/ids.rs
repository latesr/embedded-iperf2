// A copy from embedded-devkit/xtask/src/shared. Edit it there, then run `cargo xtask sync` in the devkit.

//! Checks that every requirement ID the repo cites is defined in `REQUIREMENTS.md`, and defined once.
//!
//! A definition is a bold ID, such as `**C-6**`. A citation is the same ID anywhere else, in markdown or in a Rust
//! comment. Only the letters that `REQUIREMENTS.md` defines IDs with count as ID letters, so `AES-256` and
//! `ADR-0001` are never read as IDs. An ID in a markdown code block is not a citation, so a commit message can
//! list old IDs in one. A repo without `REQUIREMENTS.md` has nothing to check.
//!
//! The copies from the devkit, the rule files and `xtask/src/shared/`, are not read. Their example IDs cite no
//! repo's requirements, and a renumber must not edit a copy in place. The devkit holds the originals, so they must
//! not cite a devkit requirement either: the check would not see the citation, and a sync would copy it to every
//! repo.
//!
//! A renumber rewrites an ID in a heading, and not the anchors that link to the heading. The links check then
//! reports each broken anchor.

use super::files::{RULE_FILES, read_files};
use super::prose::{trailing_comment, whole_line_comment};
use pulldown_cmark::{Event, Parser, Tag};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::ops::Range;
use std::path::{Path, PathBuf};

/// One cited ID and its byte range in the text that cites it.
pub(super) type Citation = (Range<usize>, String);

/// One line per problem in the repo at `root`, as `path:line: what`.
pub fn check(root: &Path) -> Vec<String> {
    let Ok(requirements) = fs::read_to_string(root.join("REQUIREMENTS.md")) else {
        return Vec::new();
    };
    let defined = definitions(&requirements);
    let mut problems: Vec<String> = defined
        .iter()
        .filter(|(_, count)| **count > 1)
        .map(|(id, count)| format!("REQUIREMENTS.md: {id} is defined {count} times"))
        .collect();
    for (path, (text, cited)) in citations_by_file(root, &letters(&defined)) {
        problems.extend(undefined(&path, &text, cited, &defined));
    }
    problems
}

/// One line per ID that the commit message at `path` cites and `REQUIREMENTS.md` does not define. Git drops the
/// lines that start with `#`, so they are not read.
pub fn check_message(root: &Path, path: &Path) -> Result<Vec<String>, String> {
    let Ok(requirements) = fs::read_to_string(root.join("REQUIREMENTS.md")) else {
        return Ok(Vec::new());
    };
    let text = fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))?;
    let kept: Vec<&str> = text
        .lines()
        .map(|line| if line.starts_with('#') { "" } else { line })
        .collect();
    let kept = kept.join("\n");
    let defined = definitions(&requirements);
    let cited = markdown_citations(&kept, &letters(&defined));
    Ok(undefined(path, &kept, cited, &defined))
}

/// Each markdown and Rust file the check reads under `root`, keyed by its path relative to `root`, with its text
/// and the IDs it cites in order.
pub(super) fn citations_by_file(
    root: &Path,
    letters: &BTreeSet<String>,
) -> BTreeMap<PathBuf, (String, Vec<Citation>)> {
    let markdown = read_files(root, "md").into_iter().map(|(path, text)| {
        let cited = markdown_citations(&text, letters);
        (path, (text, cited))
    });
    let rust = read_files(root, "rs").into_iter().map(|(path, text)| {
        let cited = rust_citations(&text, letters);
        (path, (text, cited))
    });
    markdown
        .chain(rust)
        .filter(|(path, _)| !is_synced(path))
        .collect()
}

/// Whether the file at `path`, relative to the repo root, is a copy from the devkit.
fn is_synced(path: &Path) -> bool {
    RULE_FILES.iter().any(|file| path == Path::new(file)) || path.starts_with("xtask/src/shared")
}

/// Each bold ID in `requirements`, in the order the file defines them, with any repeat.
pub(super) fn defined_in_order(requirements: &str) -> Vec<String> {
    requirements
        .split("**")
        .skip(1)
        .step_by(2)
        .filter(|bold| {
            bold.split_once('-').is_some_and(|(letter, number)| {
                (1..=2).contains(&letter.len())
                    && letter.bytes().all(|byte| byte.is_ascii_uppercase())
                    && !number.is_empty()
                    && number.bytes().all(|byte| byte.is_ascii_digit())
            })
        })
        .map(str::to_owned)
        .collect()
}

/// Each ID that `requirements` defines, with the number of times it is defined.
fn definitions(requirements: &str) -> BTreeMap<String, usize> {
    let mut found = BTreeMap::new();
    for id in defined_in_order(requirements) {
        let count = found.entry(id).or_insert(0usize);
        *count = count.saturating_add(1);
    }
    found
}

/// The letters that the IDs in `ids` use, such as `C` for `C-6`.
pub(super) fn letters<V>(ids: &BTreeMap<String, V>) -> BTreeSet<String> {
    ids.keys()
        .filter_map(|id| id.split_once('-'))
        .map(|(letter, _)| letter.to_owned())
        .collect()
}

/// The IDs in `markdown` with letters in `letters`, outside code blocks.
fn markdown_citations(markdown: &str, letters: &BTreeSet<String>) -> Vec<Citation> {
    let code_blocks: Vec<Range<usize>> = Parser::new(markdown)
        .into_offset_iter()
        .filter_map(|(event, range)| {
            matches!(event, Event::Start(Tag::CodeBlock(_))).then_some(range)
        })
        .collect();
    citations(markdown, letters)
        .into_iter()
        .filter(|(range, _)| !code_blocks.iter().any(|block| block.contains(&range.start)))
        .collect()
}

/// The IDs with letters in `letters` in the comments of a Rust file, with their ranges in `source`.
fn rust_citations(source: &str, letters: &BTreeSet<String>) -> Vec<Citation> {
    let mut found = Vec::new();
    let mut line_start = 0usize;
    for line in source.split_inclusive('\n') {
        // Both helpers return the end of `line`, so the comment starts where its length says.
        if let Some(comment) = whole_line_comment(line).or_else(|| trailing_comment(line)) {
            let start = line_start.saturating_add(line.len().saturating_sub(comment.len()));
            found.extend(citations(comment, letters).into_iter().map(|(range, id)| {
                (
                    start.saturating_add(range.start)..start.saturating_add(range.end),
                    id,
                )
            }));
        }
        line_start = line_start.saturating_add(line.len());
    }
    found
}

/// Each ID in `text` with letters in `letters`, in order.
fn citations(text: &str, letters: &BTreeSet<String>) -> Vec<Citation> {
    let bounded = |byte: Option<u8>| byte.is_none_or(|byte| !byte.is_ascii_alphanumeric());
    let mut found = Vec::new();
    for (dash, _) in text.match_indices('-') {
        let before = text.get(..dash).unwrap_or("");
        let after = text.get(dash.saturating_add(1)..).unwrap_or("");
        let letter_count = before
            .bytes()
            .rev()
            .take_while(u8::is_ascii_uppercase)
            .count();
        let start = dash.saturating_sub(letter_count);
        let letter = before.get(start..).unwrap_or("");
        let digit_count = after.bytes().take_while(u8::is_ascii_digit).count();
        let preceding = before
            .get(..start)
            .and_then(|text| text.bytes().next_back());
        let following = after.as_bytes().get(digit_count).copied();
        if digit_count > 0 && letters.contains(letter) && bounded(preceding) && bounded(following) {
            let end = dash.saturating_add(1).saturating_add(digit_count);
            if let Some(id) = text.get(start..end) {
                found.push((start..end, id.to_owned()));
            }
        }
    }
    found
}

/// The problems for the citations in `cited` that `defined` does not hold. `text` is the text the ranges index.
fn undefined(
    path: &Path,
    text: &str,
    cited: Vec<Citation>,
    defined: &BTreeMap<String, usize>,
) -> Vec<String> {
    cited
        .into_iter()
        .filter(|(_, id)| !defined.contains_key(id))
        .map(|(range, id)| {
            let line = text
                .get(..range.start)
                .map_or(0, |before| before.matches('\n').count())
                .saturating_add(1);
            format!(
                "{}:{line}: cites {id}, which REQUIREMENTS.md does not define",
                path.display()
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::super::files::testing::TempRepo;
    use super::*;

    fn letters(names: &[&str]) -> BTreeSet<String> {
        names.iter().map(|name| (*name).to_owned()).collect()
    }

    fn citation(range: Range<usize>, id: &str) -> Citation {
        (range, id.to_owned())
    }

    #[test]
    fn an_id_is_found_only_with_a_defined_letter_and_on_word_boundaries() {
        let text = "See C-6 and AES-256, ADR-0001, TR-02102-1.\nThen S-3, xS-4 and C-7b, but not C- or S-.\n";
        assert_eq!(
            citations(text, &letters(&["C", "S"])),
            vec![citation(4..7, "C-6"), citation(48..51, "S-3")]
        );
    }

    #[test]
    fn an_id_in_a_markdown_code_block_is_not_a_citation() {
        let text = "C-1 and `C-2`.\n\n```\nC-3\n```\n\n    C-4 indented\n\nC-5\n";
        assert_eq!(
            markdown_citations(text, &letters(&["C"])),
            vec![
                citation(0..3, "C-1"),
                citation(9..12, "C-2"),
                citation(47..50, "C-5")
            ]
        );
    }

    #[test]
    fn rust_citations_are_in_comments_and_index_the_source() {
        let source =
            "fn a() {} // C-1\n    /// C-2 and C-3\nlet s = \"C-4\"; // C-5\nlet t = \"// C-6\";\n";
        let found = rust_citations(source, &letters(&["C"]));
        let ids: Vec<&str> = found.iter().map(|(_, id)| id.as_str()).collect();
        assert_eq!(ids, ["C-1", "C-2", "C-3", "C-5"]);
        for (range, id) in &found {
            assert_eq!(source.get(range.clone()), Some(id.as_str()));
        }
    }

    #[test]
    fn definitions_are_the_bold_ids_counted() {
        let requirements = "**C-1** One.\n\n- **C-2** Two, as C-1 says.\n- **C-1** Again.\n\n**c-3**, **C-x**, **C-** and **ABC-4** are bold but not IDs.\n";
        assert_eq!(defined_in_order(requirements), ["C-1", "C-2", "C-1"]);
        let found = definitions(requirements);
        assert_eq!(found.get("C-1"), Some(&2));
        assert_eq!(found.get("C-2"), Some(&1));
        assert_eq!(found.len(), 2);
    }

    #[test]
    fn undefined_and_twice_defined_ids_are_reported() {
        let repo = TempRepo::new("ids-check");
        fs::create_dir_all(repo.0.join("src")).unwrap();
        fs::create_dir_all(repo.0.join("xtask/src/shared")).unwrap();
        repo.write(
            "REQUIREMENTS.md",
            "**C-1** One.\n\n**C-1** Again.\n\n- **C-2** Two, unlike C-3.\n",
        );
        repo.write(
            "docs/page.md",
            "Covers: x\n\nC-2 holds, and C-9 does not.\n",
        );
        repo.write(
            "src/lib.rs",
            "const A: u8 = 1;\n// Follows C-2 and C-8.\nconst NOT_A_CITATION: &str = \"C-7\";\n",
        );
        repo.write("CONVENTIONS.md", "An ID such as C-6.\n");
        repo.write("WRITING.md", "An ID such as C-6.\n");
        repo.write("xtask/src/shared/ids.rs", "// Such as C-6.\n");
        assert_eq!(
            check(&repo.0),
            vec![
                "REQUIREMENTS.md: C-1 is defined 2 times".to_owned(),
                "REQUIREMENTS.md:5: cites C-3, which REQUIREMENTS.md does not define".to_owned(),
                "docs/page.md:3: cites C-9, which REQUIREMENTS.md does not define".to_owned(),
                "src/lib.rs:2: cites C-8, which REQUIREMENTS.md does not define".to_owned(),
            ]
        );
    }

    #[test]
    fn a_repo_without_requirements_has_nothing_to_check() {
        let repo = TempRepo::new("ids-none");
        repo.write("docs/page.md", "Covers: x\n\nC-9.\n");
        assert_eq!(check(&repo.0), Vec::<String>::new());
    }

    #[test]
    fn a_message_is_checked_without_the_lines_git_drops_or_its_code_blocks() {
        let repo = TempRepo::new("ids-message");
        repo.write("REQUIREMENTS.md", "**C-2** Two.\n");
        repo.write(
            "message.txt",
            "Follow C-2\n\nAnd C-7.\n# C-8 in a comment line\n\n```\nC-9 → C-2\n```\n",
        );
        let message = repo.0.join("message.txt");
        assert_eq!(
            check_message(&repo.0, &message),
            Ok(vec![format!(
                "{}:3: cites C-7, which REQUIREMENTS.md does not define",
                message.display()
            )])
        );
    }
}
