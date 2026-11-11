// A copy from embedded-devkit/xtask/src/shared. Edit it there, then run `cargo xtask sync` in the devkit.

//! Checks that every Architectural Decision Record (ADR) in `docs/decisions/` has the shape `DOCS.md` states: the
//! file name, the title, the status line, the sections in order, and the word limit of the short shape. It warns on
//! a short-shape ADR over the target number of words.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// The sections of the old shape, which stays valid until every repo has converted to the short shape.
const OLD_SECTIONS: [&str; 5] = [
    "Summary",
    "Context",
    "Decision",
    "Alternatives",
    "Consequences",
];

/// The sections of the short shape, before its optional `Consequences`.
const SHORT_SECTIONS: [&str; 3] = ["Context", "Decision", "Rejected"];

/// The number of words that an ADR in the short shape can hold.
const MAX_WORDS: usize = 1000;

/// The number of words in a short-shape ADR above which the check warns.
const TARGET_WORDS: usize = 500;

const SECTIONS_PROBLEM: &str = "the sections are not Context, Decision, Rejected and an optional Consequences, or the old Summary, Context, Decision, Alternatives and Consequences, with an optional History last";

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
        if is_short(text) {
            let words = text.split_whitespace().count();
            if words > MAX_WORDS {
                problems.push(format!(
                    "{}: the ADR has {words} words, and the limit is {MAX_WORDS}",
                    path.display()
                ));
            }
        } else if body_sections(text) != OLD_SECTIONS {
            problems.push(format!("{}: {SECTIONS_PROBLEM}", path.display()));
        }
    }
    problems
}

/// The `##` headings of `text`, without an optional `History` last.
fn body_sections(text: &str) -> Vec<&str> {
    let mut found = sections(text);
    if found.last() == Some(&"History") {
        found.pop();
    }
    found
}

/// Returns whether `text` has the sections of the short shape.
fn is_short(text: &str) -> bool {
    let found = body_sections(text);
    found.strip_suffix(&["Consequences"]).unwrap_or(&found) == SHORT_SECTIONS
}

/// One line per warning, as `path: warning: what`, for each short-shape ADR among `pages` in `changed` that has
/// more than the target number of words.
pub(super) fn warnings(
    pages: &BTreeMap<PathBuf, String>,
    changed: &BTreeSet<PathBuf>,
) -> Vec<String> {
    let mut warnings = Vec::new();
    for (path, text) in pages {
        if path.parent() != Some(Path::new("docs/decisions"))
            || !changed.contains(path)
            || !is_short(text)
        {
            continue;
        }
        let words = text.split_whitespace().count();
        if words > TARGET_WORDS {
            warnings.push(format!(
                "{}: warning: the ADR has {words} words, and the target is {TARGET_WORDS}",
                path.display()
            ));
        }
    }
    warnings
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

    const SHORT: &str = "# ADR-0001: Do it\n\n**Status: accepted**\n\nCovers: x.\n\n## Context\n\nC.\n\n## Decision\n\n- D.\n\n## Rejected\n\n- **An option.** A reason.\n";

    /// `SHORT` with words added until the file holds `words` words.
    fn short_with(words: usize) -> String {
        let padding = words
            .checked_sub(SHORT.split_whitespace().count())
            .expect("`words` covers SHORT's own words");
        format!("{SHORT}\n{}\n", "word ".repeat(padding))
    }

    #[test]
    fn the_short_shape_passes_with_or_without_consequences_and_history() {
        let consequences = format!("{SHORT}\n## Consequences\n\n- A cost.\n");
        let history = "\n## History\n\n- 2027-01-01: Amended: a word.\n";
        let found = check(&pages(&[
            ("docs/decisions/0001-short.md", SHORT),
            (
                "docs/decisions/0002-costs.md",
                &consequences.replace("0001", "0002"),
            ),
            (
                "docs/decisions/0003-amended.md",
                &format!("{consequences}{history}").replace("0001", "0003"),
            ),
            (
                "docs/decisions/0004-amended.md",
                &format!("{SHORT}{history}").replace("0001", "0004"),
            ),
            (
                "docs/decisions/0005-limit.md",
                &short_with(1000).replace("0001", "0005"),
            ),
        ]));
        assert_eq!(found, Vec::<String>::new());
    }

    #[test]
    fn only_the_short_shape_has_a_word_limit() {
        let long_old = format!("{GOOD}\n{}\n", "word ".repeat(1200));
        let found = check(&pages(&[
            ("docs/decisions/0001-old.md", &long_old),
            (
                "docs/decisions/0002-long.md",
                &short_with(1001).replace("0001", "0002"),
            ),
        ]));
        assert_eq!(
            found,
            vec![
                "docs/decisions/0002-long.md: the ADR has 1001 words, and the limit is 1000"
                    .to_owned()
            ]
        );
    }

    #[test]
    fn a_changed_short_adr_over_the_target_is_warned_on() {
        let long_old = format!("{GOOD}\n{}\n", "word ".repeat(600));
        let pages = pages(&[
            ("docs/decisions/0001-over.md", &short_with(501)),
            (
                "docs/decisions/0002-unchanged.md",
                &short_with(501).replace("0001", "0002"),
            ),
            (
                "docs/decisions/0003-at.md",
                &short_with(500).replace("0001", "0003"),
            ),
            (
                "docs/decisions/0004-old.md",
                &long_old.replace("0001", "0004"),
            ),
            ("docs/a.md", &short_with(501)),
        ]);
        let changed: BTreeSet<PathBuf> = [
            "docs/decisions/0001-over.md",
            "docs/decisions/0003-at.md",
            "docs/decisions/0004-old.md",
            "docs/a.md",
        ]
        .into_iter()
        .map(PathBuf::from)
        .collect();
        assert_eq!(
            warnings(&pages, &changed),
            vec![
                "docs/decisions/0001-over.md: warning: the ADR has 501 words, and the target is 500"
                    .to_owned()
            ]
        );
        assert!(check(&pages).is_empty());
    }

    #[test]
    fn a_mix_of_the_two_shapes_or_a_short_shape_out_of_order_is_reported() {
        let found = check(&pages(&[
            (
                "docs/decisions/0001-mixed.md",
                &SHORT.replace("## Context", "## Summary\n\n- A rule.\n\n## Context"),
            ),
            (
                "docs/decisions/0002-order.md",
                &SHORT
                    .replace("0001", "0002")
                    .replace("## Decision", "## Swap")
                    .replace("## Rejected", "## Decision")
                    .replace("## Swap", "## Rejected"),
            ),
            (
                "docs/decisions/0003-both.md",
                &SHORT
                    .replace("0001", "0003")
                    .replace("## Rejected", "## Alternatives"),
            ),
        ]));
        assert_eq!(
            found,
            vec![
                format!("docs/decisions/0001-mixed.md: {SECTIONS_PROBLEM}"),
                format!("docs/decisions/0002-order.md: {SECTIONS_PROBLEM}"),
                format!("docs/decisions/0003-both.md: {SECTIONS_PROBLEM}"),
            ]
        );
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
        let sections = SECTIONS_PROBLEM;
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
