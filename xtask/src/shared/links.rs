// A copy from embedded-devkit/xtask/src/shared. Edit it there, then run `cargo xtask sync` in the devkit.

//! Checks every relative link and heading anchor in the repo's markdown files.
//!
//! A link is relative when it does not have a scheme. The target file must exist, and a `#fragment` must match a heading in it.
//! Every page under `docs/` must be linked from `docs/README.md`. Every cited requirement ID must be defined, as
//! the `ids` module checks. Every ADR must have the shape the `adrs` module checks. The check warns on a long ADR
//! that differs from `HEAD`.

use pulldown_cmark::{Event, Parser, Tag, TagEnd};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Runs the check and prints every problem. Fails when there is at least one. The warnings go to stderr and do not
/// fail.
pub fn run(root: &Path) -> Result<(), String> {
    for warning in warnings(root) {
        eprintln!("{warning}");
    }
    let problems = check(root);
    if problems.is_empty() {
        return Ok(());
    }
    Err(problems.join("\n"))
}

/// Returns one line per problem found under `root`. An empty list means every link and requirement ID resolves.
pub fn check(root: &Path) -> Vec<String> {
    let pages = super::files::read_files(root, "md");
    let mut problems = Vec::new();
    for (path, markdown) in &pages {
        for target in link_targets(markdown) {
            if let Some(problem) = check_link(root, &pages, path, &target) {
                problems.push(problem);
            }
        }
    }
    problems.extend(unindexed_docs(&pages));
    problems.extend(super::adrs::check(&pages));
    problems.extend(super::ids::check(root));
    problems
}

/// One line per warning for the markdown files under `root` that differ from `HEAD`, or that git does not track and
/// does not ignore. Outside a git repo there are none.
fn warnings(root: &Path) -> Vec<String> {
    let mut changed = std::collections::BTreeSet::new();
    for args in [
        &["diff", "HEAD", "--name-only", "--no-ext-diff"][..],
        &["ls-files", "--others", "--exclude-standard"],
    ] {
        let Ok(paths) = super::git(root, args) else {
            return Vec::new();
        };
        changed.extend(String::from_utf8_lossy(&paths).lines().map(PathBuf::from));
    }
    super::adrs::warnings(&super::files::read_files(root, "md"), &changed)
}

/// Checks one link from `page`. Returns the problem, if any.
fn check_link(
    root: &Path,
    pages: &BTreeMap<PathBuf, String>,
    page: &Path,
    target: &str,
) -> Option<String> {
    if target.contains("://") || target.starts_with("mailto:") {
        return None;
    }
    let (file, fragment) = match target.split_once('#') {
        Some((file, fragment)) => (file, Some(fragment)),
        None => (target, None),
    };
    let target_path = if file.is_empty() {
        page.to_path_buf()
    } else {
        normalise(&page.parent().unwrap_or(Path::new("")).join(file))
    };
    if !root.join(&target_path).exists() {
        return Some(format!(
            "{}: link to {target} points at a file that does not exist",
            page.display()
        ));
    }
    let fragment = fragment?;
    let Some(markdown) = pages.get(&target_path) else {
        return Some(format!(
            "{}: link to {target} has an anchor but the target is not markdown",
            page.display()
        ));
    };
    if heading_anchors(markdown)
        .iter()
        .any(|anchor| anchor == fragment)
    {
        return None;
    }
    Some(format!(
        "{}: link to {target} names a heading that does not exist",
        page.display()
    ))
}

/// Resolves `.` and `..` components without touching the file system.
fn normalise(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::ParentDir => {
                out.pop();
            }
            std::path::Component::CurDir => {}
            other => out.push(other),
        }
    }
    out
}

/// Every page under `docs/` that `docs/README.md` does not link.
fn unindexed_docs(pages: &BTreeMap<PathBuf, String>) -> Vec<String> {
    let index_path = Path::new("docs").join("README.md");
    let Some(index) = pages.get(&index_path) else {
        return Vec::new();
    };
    let indexed: Vec<PathBuf> = link_targets(index)
        .iter()
        .map(|target| target.split('#').next().unwrap_or(""))
        .map(|file| normalise(&Path::new("docs").join(file)))
        .collect();
    pages
        .keys()
        .filter(|path| path.starts_with("docs") && **path != index_path && !indexed.contains(path))
        .map(|path| {
            format!(
                "{}: page is not linked from {}",
                path.display(),
                index_path.display()
            )
        })
        .collect()
}

/// The GitHub anchor for a heading's text.
fn slug(heading: &str) -> String {
    heading
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == ' ' || *c == '-' || *c == '_')
        .map(|c| {
            if c == ' ' {
                '-'
            } else {
                c.to_ascii_lowercase()
            }
        })
        .collect()
}

/// The anchors of every heading in `markdown`, in order, with GitHub's `-1`, `-2` suffixes for repeats.
fn heading_anchors(markdown: &str) -> Vec<String> {
    let mut anchors: Vec<String> = Vec::new();
    let mut heading: Option<String> = None;
    for event in Parser::new(markdown) {
        match event {
            Event::Start(Tag::Heading { .. }) => heading = Some(String::new()),
            Event::Text(text) | Event::Code(text) => {
                if let Some(heading) = heading.as_mut() {
                    heading.push_str(&text);
                }
            }
            Event::End(TagEnd::Heading(_)) => {
                if let Some(text) = heading.take() {
                    let base = slug(&text);
                    let repeats = anchors
                        .iter()
                        .filter(|a| {
                            **a == base || a.strip_prefix(&base).is_some_and(is_repeat_suffix)
                        })
                        .count();
                    anchors.push(if repeats == 0 {
                        base
                    } else {
                        format!("{base}-{repeats}")
                    });
                }
            }
            _ => {}
        }
    }
    anchors
}

/// True for a `-N` suffix, which GitHub adds to the Nth repeat of a heading.
fn is_repeat_suffix(suffix: &str) -> bool {
    suffix
        .strip_prefix('-')
        .is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
}

/// Every link destination in `markdown`, in order.
fn link_targets(markdown: &str) -> Vec<String> {
    Parser::new(markdown)
        .filter_map(|event| match event {
            Event::Start(Tag::Link { dest_url, .. }) => Some(dest_url.into_string()),
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::super::files::testing::TempRepo;
    use super::*;
    use std::fs;

    #[test]
    fn slug_lowercases_and_hyphenates_spaces() {
        assert_eq!(slug("Why another client?"), "why-another-client");
        assert_eq!(slug("Later: JetStream"), "later-jetstream");
        assert_eq!(slug("The alloc feature"), "the-alloc-feature");
        assert_eq!(slug("docs/todo.md"), "docstodomd");
        assert_eq!(slug("ADR-0001: Send interface"), "adr-0001-send-interface");
    }

    #[test]
    fn heading_anchors_strip_code_spans_and_number_repeats() {
        let markdown = "# Title\n\n## The `alloc` feature\n\n## Notes\n\ntext\n\n## Notes\n";
        assert_eq!(
            heading_anchors(markdown),
            vec!["title", "the-alloc-feature", "notes", "notes-1"]
        );
    }

    #[test]
    fn a_longer_heading_is_not_a_repeat_and_repeats_count_up() {
        let markdown = "## Notes extra\n\n## Notes\n\n## Notes\n\n## Notes\n";
        assert_eq!(
            heading_anchors(markdown),
            vec!["notes-extra", "notes", "notes-1", "notes-2"]
        );
    }

    #[test]
    fn link_targets_include_fragments_and_skip_code_blocks() {
        let markdown = "See [a](docs/a.md#top) and [b](#local).\n\n```\n[not a link](x.md)\n```\n\n<https://example.com>\n";
        assert_eq!(
            link_targets(markdown),
            vec!["docs/a.md#top", "#local", "https://example.com"]
        );
    }

    #[test]
    fn resolving_links_pass() {
        let repo = TempRepo::new("pass");
        repo.write(
            "README.md",
            "[reqs](REQUIREMENTS.md#goals) [page](docs/page.md) [self](#intro)\n\n## Intro\n",
        );
        repo.write("REQUIREMENTS.md", "## Goals\n");
        repo.write("docs/README.md", "- [Page](page.md)\n");
        repo.write("docs/page.md", "Covers: x\n\n[up](../README.md)\n");
        assert_eq!(check(&repo.0), Vec::<String>::new());
    }

    #[test]
    fn missing_file_missing_anchor_and_unindexed_page_are_reported() {
        let repo = TempRepo::new("fail");
        repo.write(
            "README.md",
            "[gone](missing.md) [anchor](REQUIREMENTS.md#nope) <https://example.com/x>\n",
        );
        repo.write("REQUIREMENTS.md", "## Goals\n");
        repo.write("docs/README.md", "index\n");
        repo.write("docs/page.md", "text\n");
        let problems = check(&repo.0);
        assert_eq!(problems.len(), 3, "{problems:?}");
        assert!(
            problems[0].contains("README.md") && problems[0].contains("missing.md"),
            "{problems:?}"
        );
        assert!(problems[1].contains("REQUIREMENTS.md#nope"), "{problems:?}");
        assert!(
            problems[2].contains("docs/page.md") && problems[2].contains("docs/README.md"),
            "{problems:?}"
        );
    }

    #[test]
    fn target_and_hidden_directories_are_skipped() {
        let repo = TempRepo::new("skip");
        fs::create_dir_all(repo.0.join("target/doc")).unwrap();
        fs::create_dir_all(repo.0.join(".git")).unwrap();
        repo.write("target/doc/x.md", "[bad](nowhere.md)\n");
        repo.write(".git/x.md", "[bad](nowhere.md)\n");
        assert_eq!(check(&repo.0), Vec::<String>::new());
    }

    /// A short-shape ADR numbered `number` with more than 500 words.
    fn long_adr(number: &str) -> String {
        format!(
            "# ADR-{number}: X\n\n**Status: accepted**\n\n## Context\n\nC.\n\n## Decision\n\n- D.\n\n## Rejected\n\n- **O.** R.\n\n{}\n",
            "word ".repeat(500)
        )
    }

    #[test]
    fn a_long_adr_is_warned_on_only_where_it_differs_from_head() {
        let repo = TempRepo::new("adr-warn");
        fs::create_dir_all(repo.0.join("docs/decisions")).unwrap();
        for args in [
            &["init", "--quiet"][..],
            &["config", "user.email", "test@example.com"],
            &["config", "user.name", "test"],
        ] {
            super::super::git(&repo.0, args).unwrap();
        }
        repo.write("docs/decisions/0001-kept.md", &long_adr("0001"));
        repo.write("docs/decisions/0002-edited.md", &long_adr("0002"));
        super::super::git(&repo.0, &["add", "."]).unwrap();
        super::super::git(&repo.0, &["commit", "--quiet", "-m", "a"]).unwrap();
        assert_eq!(warnings(&repo.0), Vec::<String>::new());
        repo.write(
            "docs/decisions/0002-edited.md",
            &format!("{}more\n", long_adr("0002")),
        );
        repo.write("docs/decisions/0003-new.md", &long_adr("0003"));
        assert_eq!(
            warnings(&repo.0),
            vec![
                "docs/decisions/0002-edited.md: warning: the ADR has 518 words, and the target is 500"
                    .to_owned(),
                "docs/decisions/0003-new.md: warning: the ADR has 517 words, and the target is 500"
                    .to_owned(),
            ]
        );
    }

    #[test]
    fn outside_a_git_repo_nothing_is_warned_on() {
        let repo = TempRepo::new("adr-warn-no-git");
        fs::create_dir_all(repo.0.join("docs/decisions")).unwrap();
        repo.write("docs/decisions/0001-x.md", &long_adr("0001"));
        assert_eq!(warnings(&repo.0), Vec::<String>::new());
    }
}
