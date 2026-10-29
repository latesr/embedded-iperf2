// A copy from embedded-devkit/xtask/src/shared. Edit it there, then run `cargo xtask sync` in the devkit.

//! Checks that every Architectural Decision Record (ADR) in `docs/decisions/` has the shape `DOCS.md` states: the
//! file name, the title, the status line and the sections in order.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

const SECTIONS: [&str; 5] = [
    "Summary",
    "Context",
    "Decision",
    "Alternatives",
    "Consequences",
];

/// One line per problem in the ADRs among `pages`, which are keyed by their path relative to the repo root.
pub(super) fn check(pages: &BTreeMap<PathBuf, String>) -> Vec<String> {
    let adrs: Vec<(&PathBuf, &String)> = pages
        .iter()
        .filter(|(path, _)| path.parent() == Some(Path::new("docs/decisions")))
        .collect();
    let numbers: BTreeSet<&str> = adrs.iter().filter_map(|(path, _)| number(path)).collect();
    let mut problems = Vec::new();
    for (path, text) in adrs {
        let Some(number) = number(path) else {
            problems.push(format!(
                "{}: the file name is not NNNN-slug.md",
                path.display()
            ));
            continue;
        };
        let title = format!("# ADR-{number}: ");
        let titled = text
            .lines()
            .next()
            .and_then(|line| line.strip_prefix(&title))
            .is_some_and(|rest| !rest.trim().is_empty());
        if !titled {
            problems.push(format!(
                "{}: the first line is not \"{title}\" and a title",
                path.display()
            ));
        }
        let status = text
            .lines()
            .skip(1)
            .find(|line| !line.trim().is_empty())
            .unwrap_or("");
        let superseded_by = status
            .strip_prefix("**Status: superseded by ADR-")
            .and_then(|rest| rest.strip_suffix("**"));
        if status != "**Status: accepted**"
            && status != "**Status: proposed**"
            && !superseded_by.is_some_and(|by| numbers.contains(by))
        {
            problems.push(format!(
                "{}: the status line is not accepted, proposed or superseded by an ADR that exists",
                path.display()
            ));
        }
        let found = sections(text);
        if found.strip_suffix(&["History"]).unwrap_or(&found) != SECTIONS {
            problems.push(format!(
                "{}: the sections are not Summary, Context, Decision, Alternatives and Consequences, with an optional History last",
                path.display()
            ));
        }
    }
    problems
}

/// The text of each `##` heading in `markdown`, outside fenced code blocks.
fn sections(markdown: &str) -> Vec<&str> {
    let mut fenced = false;
    let mut found = Vec::new();
    for line in markdown.lines() {
        if line.trim_start().starts_with("```") {
            fenced = !fenced;
        } else if let Some(heading) = line.strip_prefix("## ")
            && !fenced
        {
            found.push(heading.trim());
        }
    }
    found
}

/// The number in an ADR's file name, such as `0001` for `0001-copy-and-sync.md`, or `None` when the name does not
/// have that shape.
fn number(path: &Path) -> Option<&str> {
    let name = path.file_name()?.to_str()?;
    let (number, rest) = name.split_at_checked(4)?;
    let slug = rest.strip_prefix('-')?.strip_suffix(".md")?;
    (number.bytes().all(|byte| byte.is_ascii_digit()) && !slug.is_empty()).then_some(number)
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD: &str = "# ADR-0001: Do it\n\n**Status: accepted**\n\n## Summary\n\n- A rule.\n\n## Context\n\nC.\n\n## Decision\n\nD.\n\n```\n## not a heading\n```\n\n## Alternatives\n\nA.\n\n## Consequences\n\nC.\n";

    fn pages(files: &[(&str, &str)]) -> BTreeMap<PathBuf, String> {
        files
            .iter()
            .map(|(path, text)| (PathBuf::from(path), (*text).to_owned()))
            .collect()
    }

    #[test]
    fn a_well_formed_adr_and_other_pages_pass() {
        let history = format!("{GOOD}\n## History\n\n- 2027-01-01: Amended: a word.\n");
        let superseded = GOOD
            .replace("0001", "0002")
            .replace("accepted", "superseded by ADR-0003");
        let found = check(&pages(&[
            ("docs/decisions/0001-do-it.md", GOOD),
            ("docs/decisions/0002-old.md", &superseded),
            (
                "docs/decisions/0003-new.md",
                &history.replace("0001", "0003"),
            ),
            ("docs/concepts/x.md", "Covers: x\n"),
        ]));
        assert_eq!(found, Vec::<String>::new());
    }

    #[test]
    fn each_broken_rule_is_reported() {
        let found = check(&pages(&[
            ("docs/decisions/notes.md", GOOD),
            ("docs/decisions/0002-title.md", GOOD),
            (
                "docs/decisions/0003-status.md",
                &GOOD.replace("0001", "0003").replace("accepted", "Accepted"),
            ),
            (
                "docs/decisions/0004-gone.md",
                &GOOD
                    .replace("0001", "0004")
                    .replace("accepted", "superseded by ADR-0009"),
            ),
            (
                "docs/decisions/0005-order.md",
                &GOOD
                    .replace("0001", "0005")
                    .replace("## Context", "## Background"),
            ),
            (
                "docs/decisions/0006-missing.md",
                &GOOD
                    .replace("0001", "0006")
                    .replace("## Consequences\n\nC.\n", ""),
            ),
            (
                "docs/decisions/0007-late-status.md",
                &GOOD
                    .replace("0001", "0007")
                    .replace("**Status: accepted**", "Text.\n\n**Status: accepted**"),
            ),
        ]));
        let sections = "the sections are not Summary, Context, Decision, Alternatives and Consequences, with an optional History last";
        assert_eq!(
            found,
            vec![
                "docs/decisions/0002-title.md: the first line is not \"# ADR-0002: \" and a title".to_owned(),
                "docs/decisions/0003-status.md: the status line is not accepted, proposed or superseded by an ADR that exists".to_owned(),
                "docs/decisions/0004-gone.md: the status line is not accepted, proposed or superseded by an ADR that exists".to_owned(),
                format!("docs/decisions/0005-order.md: {sections}"),
                format!("docs/decisions/0006-missing.md: {sections}"),
                "docs/decisions/0007-late-status.md: the status line is not accepted, proposed or superseded by an ADR that exists".to_owned(),
                "docs/decisions/notes.md: the file name is not NNNN-slug.md".to_owned(),
            ]
        );
    }

    #[test]
    fn the_number_comes_from_a_four_digit_prefix() {
        assert_eq!(number(Path::new("docs/decisions/0012-x.md")), Some("0012"));
        assert_eq!(number(Path::new("docs/decisions/012-x.md")), None);
        assert_eq!(number(Path::new("docs/decisions/0012.md")), None);
        assert_eq!(number(Path::new("docs/decisions/00a2-x.md")), None);
    }
}
