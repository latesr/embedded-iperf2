// A copy from embedded-conventions/xtask/src/shared. Edit it there, then run `cargo xtask sync` there.

//! Checks markdown, rustdoc, code comments, and commit messages for the phrases that CHECKS.md lists.

use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use std::fs;
use std::path::Path;

/// The banned phrases by group. A phrase matches regardless of case, on word boundaries, and a hyphen in the
/// text counts as a space, so one spelling covers both.
const BANNED: &[(&str, &[&str])] = &[
    (
        "commentary",
        &[
            "worth noting",
            "note that",
            "the point is",
            "what matters is",
            "importantly",
            "notably",
            "crucially",
            "it should be noted",
            "keep in mind",
        ],
    ),
    (
        "hedging",
        &[
            "obviously",
            "arguably",
            "genuinely",
            "plainly",
            "frankly",
            "simply",
            "clearly",
            "basically",
            "essentially",
            "of course",
        ],
    ),
    (
        "restating",
        &[
            "which means",
            "which is to say",
            "in other words",
            "that is to say",
        ],
    ),
    (
        "idiom",
        &[
            "load bearing",
            "long pole",
            "surface area",
            "first class",
            "on the other hand",
            "under the hood",
            "low hanging",
            "boils down",
            "at the end of the day",
            "moving parts",
            "footgun",
        ],
    ),
    (
        "marketing",
        &[
            "leverage",
            "robust",
            "seamless",
            "powerful",
            "elegant",
            "comprehensive",
            "state of the art",
            "blazing",
        ],
    ),
];

/// The phrases that are often a fault and sometimes right, by group. A warning does not fail the check. A phrase
/// that starts or ends with punctuation matches without a word boundary on that side.
const WARNED: &[(&str, &[&str])] = &[
    (
        "restating",
        &[
            ", which is",
            ", which makes",
            ", which gives",
            ", which lets",
            ", that is,",
            "this means",
            "needs no",
            "does no",
            "keeps no",
            "has no",
        ],
    ),
    ("far reference", &["the former", "the latter", "one whose"]),
    ("unusual verb", &["gains", "gained", "and go"]),
    ("containment", &["carries", "carry", "carried"]),
    ("filler", &["in order to"]),
    ("hedging", &["actually", "obvious"]),
    ("citation", &[", see"]),
    ("diff reference", &["this code", "this port"]),
    ("unapproved word", &["whose", "unless", "itself"]),
];

/// The words that tie a sentence to the day it was written. A commit message describes its own change, so only
/// files are warned on.
const RELATIVE_TIME: &[&str] = &["today", "currently", "at the moment", "recently"];

/// The docs folders in which each page states each fact once: the ADRs. Other pages repeat a pattern on purpose, such as a
/// requirement or the rustdoc of sibling functions.
const ONE_STATEMENT_FOLDERS: &[&str] = &["decisions"];

/// Which of the warnings that depend on the kind of text apply.
#[derive(Clone, Copy, Debug)]
struct Scope {
    /// Compare each sentence with the others for repeated words.
    duplicates: bool,
    /// Warn on relative time.
    relative_time: bool,
}

/// Whether the page at `path`, from the repo root, states each fact once.
fn compares_duplicates(path: &Path) -> bool {
    let mut parts = path.iter();
    parts.next().is_some_and(|part| part == "docs")
        && parts
            .next()
            .is_some_and(|folder| ONE_STATEMENT_FOLDERS.iter().any(|name| folder == *name))
}

/// A finding: the line it is on and what was found.
type Finding = (usize, String);

/// Scans the repo, or one commit message file when `message` is given, and warns on the paragraphs that differ
/// from `HEAD`. Fails with one line per finding.
pub fn run(root: &Path, message: Option<&Path>) -> Result<(), String> {
    run_with(root, message, false)
}

/// As [`run`], and warns on every paragraph when `warn_all` is set. The warnings go to stderr and do not fail.
pub fn run_with(root: &Path, message: Option<&Path>, warn_all: bool) -> Result<(), String> {
    for warning in warnings(root, message, warn_all)? {
        eprintln!("{warning}");
    }
    let problems = match message {
        Some(path) => check_message(path)?,
        None => check(root),
    };
    if problems.is_empty() {
        return Ok(());
    }
    Err(problems.join("\n"))
}

/// One line per finding in the markdown and Rust files under `root`, as `path:line: what`.
pub fn check(root: &Path) -> Vec<String> {
    let mut problems = Vec::new();
    for (path, markdown) in super::files::read_files(root, "md") {
        problems.extend(located(&path, scan_markdown(&markdown, 1)));
    }
    for (path, source) in super::files::read_files(root, "rs") {
        problems.extend(located(&path, scan_rust(&source)));
    }
    problems
}

fn check_message(path: &Path) -> Result<Vec<String>, String> {
    let text = fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))?;
    Ok(located(path, scan_message(&text)))
}

/// The lines of each file that differ from `HEAD`, as inclusive ranges, by path from the repo root.
type Changed = std::collections::BTreeMap<std::path::PathBuf, Vec<(usize, usize)>>;

/// One line per warning, as `path:line: warning: what`. Without `warn_all` the warnings cover the paragraphs that
/// differ from `HEAD`, and none outside a git repo. A file that git does not track yet, and does not ignore, is
/// warned on whole. A commit message is warned on whole.
fn warnings(root: &Path, message: Option<&Path>, warn_all: bool) -> Result<Vec<String>, String> {
    if let Some(path) = message {
        let text =
            fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))?;
        let blocks = [(1, message_text(&text))];
        let scope = Scope {
            duplicates: false,
            relative_time: false,
        };
        return Ok(warned(path, warn_blocks(&blocks, &|_, _| true, scope)));
    }
    let changed = (!warn_all).then(|| {
        super::git(
            root,
            &["diff", "HEAD", "--unified=0", "--no-color", "--no-ext-diff"],
        )
        .map(|diff| {
            let mut changed = changed_ranges(&String::from_utf8_lossy(&diff));
            let untracked = super::git(root, &["ls-files", "--others", "--exclude-standard"])
                .unwrap_or_default();
            for path in String::from_utf8_lossy(&untracked).lines() {
                changed.insert(std::path::PathBuf::from(path), vec![(1, usize::MAX)]);
            }
            changed
        })
        .unwrap_or_default()
    });
    let mut lines = Vec::new();
    for extension in ["md", "rs"] {
        for (path, text) in super::files::read_files(root, extension) {
            let ranges = match &changed {
                None => None,
                Some(changed) => match changed.get(&path) {
                    Some(ranges) => Some(ranges),
                    None => continue,
                },
            };
            let blocks = if extension == "md" {
                vec![(1, text)]
            } else {
                rust_blocks(&text)
            };
            let scope = Scope {
                duplicates: extension == "md" && compares_duplicates(&path),
                relative_time: true,
            };
            let in_change = |first: usize, last: usize| {
                ranges.is_none_or(|ranges| {
                    ranges
                        .iter()
                        .any(|(from, to)| *from <= last && first <= *to)
                })
            };
            lines.extend(warned(&path, warn_blocks(&blocks, &in_change, scope)));
        }
    }
    Ok(lines)
}

fn warned(path: &Path, findings: Vec<Finding>) -> Vec<String> {
    findings
        .into_iter()
        .map(|(line, what)| format!("{}:{line}: warning: {what}", path.display()))
        .collect()
}

/// The changed line ranges in a `git diff --unified=0`. A hunk that only deletes lines marks the line it
/// follows, so the paragraph it cut from still counts as changed.
fn changed_ranges(diff: &str) -> Changed {
    let mut changed = Changed::new();
    let mut path: Option<std::path::PathBuf> = None;
    let mut in_header = false;
    for line in diff.lines() {
        if line.starts_with("diff --git ") {
            in_header = true;
            path = None;
        } else if in_header && let Some(name) = line.strip_prefix("+++ ") {
            path = name.strip_prefix("b/").map(std::path::PathBuf::from);
        } else if let Some(hunk) = line.strip_prefix("@@ ") {
            in_header = false;
            let new_side = hunk.split(' ').find_map(|part| part.strip_prefix('+'));
            if let (Some(path), Some(new_side)) = (&path, new_side) {
                let (start, count) = new_side.split_once(',').unwrap_or((new_side, "1"));
                if let (Ok(start), Ok(count)) = (start.parse::<usize>(), count.parse::<usize>()) {
                    let end = start.saturating_add(count.saturating_sub(1));
                    changed.entry(path.clone()).or_default().push((start, end));
                }
            }
        }
    }
    changed
}

/// The warnings for one file, with its prose in `blocks` as `(first line, markdown)`. `changed` says whether a
/// paragraph, given by its first and last line, is one to warn on. When `scope` asks for duplicates, a sentence is
/// compared with every other sentence of the file.
fn warn_blocks(
    blocks: &[(usize, String)],
    changed: &dyn Fn(usize, usize) -> bool,
    scope: Scope,
) -> Vec<Finding> {
    let mut findings: Vec<(usize, usize, String)> = Vec::new();
    let mut sentences: Vec<Sentence> = Vec::new();
    let mut in_summary = false;
    let relative_time: &[(&str, &[&str])] = if scope.relative_time {
        &[("relative time", RELATIVE_TIME)]
    } else {
        &[]
    };
    for (first_line, markdown) in blocks {
        let line_of = |offset: usize| {
            first_line.saturating_add(
                markdown
                    .get(..offset)
                    .map_or(0, |before| before.matches('\n').count()),
            )
        };
        let masked = masked_prose(markdown);
        for (start, end) in paragraphs(markdown) {
            let is_changed = changed(line_of(start), line_of(end));
            let (Some(text), Some(raw)) = (masked.get(start..end), markdown.get(start..end)) else {
                continue;
            };
            let raw = raw.trim_start();
            if raw.starts_with('#') {
                in_summary = raw
                    .trim_start_matches('#')
                    .trim()
                    .eq_ignore_ascii_case("summary");
            }
            // A Summary repeats its Decision, a heading names what follows it, and table rows repeat their
            // columns' words, by design.
            let compared =
                scope.duplicates && !in_summary && !raw.starts_with('|') && !raw.starts_with('#');
            if is_changed {
                let (folded, origin) = fold(text, start);
                for (group, phrases) in WARNED.iter().chain(relative_time) {
                    for phrase in *phrases {
                        for at in find_phrase(&folded, phrase) {
                            let offset = origin.get(at).copied().unwrap_or(start);
                            findings.push((
                                line_of(offset),
                                offset,
                                format!("\"{phrase}\" ({group})"),
                            ));
                        }
                    }
                }
            }
            for (from, to) in sentence_ranges(text) {
                let offset = start.saturating_add(from);
                if let Some(sentence) = text.get(from..to) {
                    sentences.push(Sentence::new(
                        sentence,
                        line_of(offset),
                        offset,
                        is_changed,
                        compared,
                    ));
                }
            }
        }
    }
    for (index, sentence) in sentences.iter().enumerate() {
        if !sentence.changed {
            continue;
        }
        let mut report = |what: String| findings.push((sentence.line, sentence.offset, what));
        if matches!(sentence.first.as_str(), "this" | "that")
            && OPENER_VERBS.contains(&sentence.second.as_str())
        {
            report(format!(
                "\"{} {}\" opens a sentence (restating)",
                sentence.first, sentence.second
            ));
        }
        if sentence.causes >= 2 {
            report("two causes in one sentence (restating)".to_owned());
        }
        if sentence.brackets >= 2 {
            report("two brackets in one sentence (length)".to_owned());
        }
        if sentence.length > LONG_SENTENCE {
            report(format!("a sentence of {} words (length)", sentence.length));
        }
        let previous = index
            .checked_sub(1)
            .and_then(|before| sentences.get(before));
        if sentence.first == "so" && previous.is_some_and(|previous| previous.first == "so") {
            report("two sentences in a row open with \"so\" (restating)".to_owned());
        }
    }
    for (later, sentence) in sentences.iter().enumerate() {
        for other in sentences.iter().take(later) {
            if !(sentence.changed || other.changed)
                || !(sentence.compared && other.compared)
                || !sentence.repeats(other)
            {
                continue;
            }
            let (at, with) = if sentence.changed {
                (sentence, other)
            } else {
                (other, sentence)
            };
            let other_sentence = if with.line == at.line {
                "an earlier sentence on the same line".to_owned()
            } else {
                format!("line {}", with.line)
            };
            findings.push((
                at.line,
                at.offset,
                format!("shares most of its words with {other_sentence} (restating)"),
            ));
        }
    }
    findings.sort_by_key(|(line, offset, _)| (*line, *offset));
    let mut lines: Vec<Finding> = findings
        .into_iter()
        .map(|(line, _, what)| (line, what))
        .collect();
    lines.dedup();
    lines
}

/// The most words a sentence has before it warns. ASD-STE100 keeps a sentence under 25 words.
const LONG_SENTENCE: usize = 35;

/// The second words that make a sentence opening with "this" or "that" comment on the sentence before it.
const OPENER_VERBS: &[&str] = &[
    "is", "was", "lets", "means", "makes", "gives", "keeps", "avoids", "ensures", "allows", "helps",
];

/// Words without content of their own, left out when two sentences are compared.
const STOP_WORDS: &[&str] = &[
    "the", "and", "for", "with", "from", "that", "this", "these", "those", "its", "are", "was",
    "were", "has", "have", "had", "not", "but", "all", "any", "each", "every", "one", "two", "can",
    "may", "must", "will", "into", "after", "before", "when", "then", "than", "also", "only",
    "other", "here", "there", "which", "what", "who", "whose", "where", "does", "did", "our",
    "you", "your", "they", "them", "their", "out", "own", "same", "such", "more", "most", "some",
    "both", "about", "over", "under", "again", "once",
];

/// One sentence of prose, reduced to what the warnings compare.
struct Sentence {
    line: usize,
    offset: usize,
    changed: bool,
    /// Whether the duplicate check compares this sentence with the others.
    compared: bool,
    first: String,
    second: String,
    causes: usize,
    /// The words of the sentence, separated by white space.
    length: usize,
    /// The opening brackets in the sentence's prose. A bracket inside code is masked.
    brackets: usize,
    words: std::collections::BTreeSet<String>,
}

impl Sentence {
    fn new(text: &str, line: usize, offset: usize, changed: bool, compared: bool) -> Self {
        let lower = text.to_ascii_lowercase();
        let tokens: Vec<&str> = lower
            .split(|ch: char| !ch.is_alphanumeric())
            .filter(|token| !token.is_empty())
            .collect();
        let folded = lower.split_whitespace().collect::<Vec<_>>().join(" ");
        let causes = folded.matches(", so ").count().saturating_add(
            tokens
                .iter()
                .filter(|token| matches!(**token, "because" | "therefore"))
                .count(),
        );
        Self {
            line,
            offset,
            changed,
            compared,
            first: tokens
                .first()
                .map_or_else(String::new, |token| (*token).to_owned()),
            second: tokens
                .get(1)
                .map_or_else(String::new, |token| (*token).to_owned()),
            causes,
            length: text.split_whitespace().count(),
            brackets: text.matches('(').count(),
            words: tokens
                .iter()
                .filter(|token| token.len() >= 3 && !STOP_WORDS.contains(token))
                .map(|token| stem(token))
                .collect(),
        }
    }

    /// Whether this sentence and `other` each hold at least six content words, share at least four, and share at
    /// least half of the words the two hold between them.
    fn repeats(&self, other: &Self) -> bool {
        let shared = self.words.intersection(&other.words).count();
        let all = self.words.union(&other.words).count();
        self.words.len().min(other.words.len()) >= 6
            && shared >= 4
            && shared.saturating_mul(10) >= all.saturating_mul(5)
    }
}

/// `word` without an `ing`, `ed`, or plural `s` ending, so that "flushes" and "flushed" compare equal to the stem
/// they share.
fn stem(word: &str) -> String {
    let stemmed = if word.len() > 5 {
        word.strip_suffix("ing")
    } else {
        None
    }
    .or_else(|| (word.len() > 4).then(|| word.strip_suffix("ed")).flatten())
    .or_else(|| {
        (word.len() > 3 && !word.ends_with("ss"))
            .then(|| word.strip_suffix('s'))
            .flatten()
    })
    .unwrap_or(word);
    stemmed.to_owned()
}

/// `markdown` with every byte that is not prose replaced by a space. Line breaks stay, so offsets and lines hold.
fn masked_prose(markdown: &str) -> String {
    let bytes: Vec<u8> = prose_mask(markdown)
        .iter()
        .zip(markdown.bytes())
        .map(|(is_prose, byte)| {
            if *is_prose || byte == b'\n' {
                byte
            } else {
                b' '
            }
        })
        .collect();
    String::from_utf8_lossy(&bytes).into_owned()
}

/// The byte ranges of the paragraphs of `markdown`. A blank line ends a paragraph. A line that starts a list
/// item, a table row, or a heading starts a new one, because the docs put one item on one line.
fn paragraphs(markdown: &str) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    let mut current: Option<(usize, usize)> = None;
    let mut offset = 0usize;
    for line in markdown.split('\n') {
        let end = offset.saturating_add(line.len());
        let trimmed = line.trim_start();
        let digits = trimmed.trim_start_matches(|ch: char| ch.is_ascii_digit());
        let starts_item = ["- ", "* ", "| ", "#"]
            .iter()
            .any(|marker| trimmed.starts_with(marker))
            || (digits.len() < trimmed.len() && digits.starts_with(". "));
        if trimmed.is_empty() || starts_item {
            ranges.extend(current.take());
        }
        if !trimmed.is_empty() {
            current = Some(current.map_or((offset, end), |(start, _)| (start, end)));
        }
        offset = end.saturating_add(1);
    }
    ranges.extend(current);
    ranges
}

/// The byte ranges of the sentences in `text`. A sentence ends at a full stop, a question mark, or an exclamation
/// mark that white space or the end of the text follows.
fn sentence_ranges(text: &str) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    let mut start: Option<usize> = None;
    let mut chars = text.char_indices().peekable();
    while let Some((index, ch)) = chars.next() {
        if start.is_none() && !ch.is_whitespace() {
            start = Some(index);
        }
        let next_is_space = chars.peek().is_none_or(|(_, next)| next.is_whitespace());
        if matches!(ch, '.' | '?' | '!')
            && next_is_space
            && let Some(from) = start.take()
        {
            ranges.push((from, index.saturating_add(1)));
        }
    }
    ranges.extend(start.map(|from| (from, text.len())));
    ranges
}

fn located(path: &Path, findings: Vec<Finding>) -> Vec<String> {
    findings
        .into_iter()
        .map(|(line, what)| format!("{}:{line}: {what}", path.display()))
        .collect()
}

/// Scans a commit message. Git drops the lines that start with `#`, so they are not prose. They are blanked
/// rather than removed, so the line numbers hold.
fn scan_message(text: &str) -> Vec<Finding> {
    scan_markdown(&message_text(text), 1)
}

/// A commit message without the lines that git drops.
fn message_text(text: &str) -> String {
    let kept: Vec<&str> = text
        .lines()
        .map(|line| if line.starts_with('#') { "" } else { line })
        .collect();
    kept.join("\n")
}

/// Scans the comments of a Rust source file. Strings are code, not prose.
fn scan_rust(source: &str) -> Vec<Finding> {
    rust_blocks(source)
        .iter()
        .flat_map(|(first_line, markdown)| scan_markdown(markdown, *first_line))
        .collect()
}

/// The comments of a Rust source file as markdown blocks, each with the line it starts on. Consecutive
/// whole-line comments form one block, and a comment after code on the same line is a block of its own.
fn rust_blocks(source: &str) -> Vec<(usize, String)> {
    let mut blocks = Vec::new();
    let mut block: Vec<&str> = Vec::new();
    let mut block_start = 1;
    for (index, line) in source.lines().enumerate() {
        let number = index.saturating_add(1);
        if let Some(text) = whole_line_comment(line) {
            if block.is_empty() {
                block_start = number;
            }
            block.push(text);
            continue;
        }
        flush(&mut block, block_start, &mut blocks);
        if let Some(text) = trailing_comment(line) {
            blocks.push((number, text.to_owned()));
        }
    }
    flush(&mut block, block_start, &mut blocks);
    blocks
}

/// Moves the comment lines gathered so far into `blocks` as one block.
fn flush(block: &mut Vec<&str>, first_line: usize, blocks: &mut Vec<(usize, String)>) {
    if !block.is_empty() {
        blocks.push((first_line, block.join("\n")));
        block.clear();
    }
}

/// The text of a line that holds only a comment, after its `//!`, `///`, or `//` and one space. Rustdoc treats
/// the space after the marker as part of the marker, and the spaces after it as indentation.
pub(super) fn whole_line_comment(line: &str) -> Option<&str> {
    let trimmed = line.trim_start();
    let text = trimmed
        .strip_prefix("//!")
        .or_else(|| trimmed.strip_prefix("///"))
        .or_else(|| trimmed.strip_prefix("//"))?;
    Some(text.strip_prefix(' ').unwrap_or(text))
}

/// The text of a comment after code on the same line. A `//` inside a string literal is not a comment, and an
/// odd count of quotes before it says it is inside one. A quote after a backslash is inside the string, so it does
/// not count.
pub(super) fn trailing_comment(line: &str) -> Option<&str> {
    let unescaped_quotes = |before: &str| {
        before
            .match_indices('"')
            .filter(|(at, _)| !before.get(..*at).is_some_and(|text| text.ends_with('\\')))
            .count()
    };
    line.match_indices("//")
        .map(|(at, _)| at)
        .find(|at| {
            line.get(..*at)
                .is_some_and(|before| unescaped_quotes(before) & 1 == 0)
        })
        .and_then(|at| line.get(at.saturating_add(2)..))
        .map(str::trim_start)
}

/// Scans markdown that starts at line `first_line` of its file. Code blocks, code spans, and text in double quotes
/// are not prose.
fn scan_markdown(markdown: &str, first_line: usize) -> Vec<Finding> {
    let mask = prose_mask(markdown);
    let mut findings: Vec<(usize, String)> = Vec::new();
    let mut run_start: Option<usize> = None;
    for (index, is_prose) in mask
        .iter()
        .copied()
        .chain(std::iter::once(false))
        .enumerate()
    {
        match (run_start, is_prose) {
            (None, true) => run_start = Some(index),
            (Some(start), false) => {
                if let Some(run) = markdown.get(start..index) {
                    scan_run(run, start, &mut findings);
                }
                run_start = None;
            }
            _ => {}
        }
    }
    findings.sort_by_key(|(offset, _)| *offset);
    findings
        .into_iter()
        .map(|(offset, what)| {
            let newlines = markdown
                .get(..offset)
                .map_or(0, |before| before.matches('\n').count());
            (first_line.saturating_add(newlines), what)
        })
        .collect()
}

/// One flag per byte of `markdown`: true where the byte is prose. Text in code blocks and code spans is not
/// prose, and neither is text in double quotes. A quote that is not closed ends with its paragraph.
fn prose_mask(markdown: &str) -> Vec<bool> {
    let mut mask = vec![false; markdown.len()];
    let mut block_ends = Vec::new();
    let mut code_depth = 0usize;
    let parser = Parser::new_ext(markdown, Options::ENABLE_TABLES).into_offset_iter();
    for (event, range) in parser {
        match event {
            Event::Start(Tag::CodeBlock(_)) => code_depth = code_depth.saturating_add(1),
            Event::End(TagEnd::CodeBlock) => code_depth = code_depth.saturating_sub(1),
            Event::Text(_) | Event::SoftBreak | Event::HardBreak if code_depth == 0 => {
                if let Some(bytes) = mask.get_mut(range) {
                    bytes.fill(true);
                }
            }
            Event::End(
                TagEnd::Paragraph | TagEnd::Heading(_) | TagEnd::Item | TagEnd::TableCell,
            ) => block_ends.push(range.end),
            _ => {}
        }
    }
    block_ends.sort_unstable();
    let mut quoted = false;
    for (index, (is_prose, byte)) in mask.iter_mut().zip(markdown.bytes()).enumerate() {
        if block_ends.binary_search(&index).is_ok() {
            quoted = false;
        }
        if *is_prose && byte == b'"' {
            quoted = !quoted;
            *is_prose = false;
        } else if quoted {
            *is_prose = false;
        }
    }
    mask
}

/// Searches one run of prose. `base` is where the run starts in the source, and each finding holds the offset
/// of what it found.
fn scan_run(text: &str, base: usize, findings: &mut Vec<(usize, String)>) {
    let (folded, origin) = fold(text, base);
    let at = |folded_index: usize| origin.get(folded_index).copied().unwrap_or(base);

    for (group, phrases) in BANNED {
        for phrase in *phrases {
            for start in find_phrase(&folded, phrase) {
                findings.push((at(start), format!("\"{phrase}\" ({group})")));
            }
        }
    }

    let chars: Vec<(usize, char)> = folded.char_indices().collect();
    for (position, (index, ch)) in chars.iter().enumerate() {
        let what = match ch {
            '\u{2014}' => "an em dash",
            ';' => "a semicolon",
            '\u{2013}' => {
                let neighbour = |offset: usize| chars.get(offset).map(|(_, ch)| *ch);
                let before = position.checked_sub(1).and_then(neighbour);
                let after = neighbour(position.saturating_add(1));
                if before == Some(' ') && after == Some(' ') {
                    "an en dash used as a separator"
                } else {
                    continue;
                }
            }
            _ => continue,
        };
        findings.push((at(*index), format!("{what} (punctuation)")));
    }
}

/// `text` folded for matching, with the offset in the source of each folded byte. `base` is where `text` starts
/// in the source. Folding makes ASCII lower case, a hyphen a space, and a stretch of white space one space, so a
/// phrase matches across a line break and in either spelling.
fn fold(text: &str, base: usize) -> (String, Vec<usize>) {
    let mut folded = String::new();
    let mut origin: Vec<usize> = Vec::new();
    let mut after_space = false;
    for (index, ch) in text.char_indices() {
        let fold = if ch == '-' || ch.is_whitespace() {
            ' '
        } else {
            ch.to_ascii_lowercase()
        };
        if fold == ' ' && after_space {
            continue;
        }
        after_space = fold == ' ';
        folded.push(fold);
        origin.extend(std::iter::repeat_n(
            base.saturating_add(index),
            fold.len_utf8(),
        ));
    }
    (folded, origin)
}

/// Where `phrase` starts in `folded`. An end of the phrase that is a letter or a digit must sit on a word
/// boundary. An end that is punctuation needs none.
fn find_phrase(folded: &str, phrase: &str) -> Vec<usize> {
    let needs_boundary = |ch: Option<char>| ch.is_some_and(char::is_alphanumeric);
    let (check_before, check_after) = (
        needs_boundary(phrase.chars().next()),
        needs_boundary(phrase.chars().next_back()),
    );
    let mut starts = Vec::new();
    let mut from = 0;
    while let Some(found) = folded.get(from..).and_then(|rest| rest.find(phrase)) {
        let start = from.saturating_add(found);
        let end = start.saturating_add(phrase.len());
        let before = folded
            .get(..start)
            .and_then(|text| text.chars().next_back());
        let after = folded.get(end..).and_then(|text| text.chars().next());
        if (!check_before || before.is_none_or(|ch| !ch.is_alphanumeric()))
            && (!check_after || after.is_none_or(|ch| !ch.is_alphanumeric()))
        {
            starts.push(start);
        }
        from = end;
    }
    starts
}

#[cfg(test)]
mod tests {
    use super::super::files::testing::TempRepo;
    use super::*;
    use std::fs;

    fn found(line: usize, what: &str) -> Finding {
        (line, what.to_owned())
    }

    #[test]
    fn every_group_is_reported_with_its_line() {
        let markdown = "Fine.\n\nIt is worth noting that this is simply the case, which means we leverage the long pole.\n";
        assert_eq!(
            scan_markdown(markdown, 1),
            vec![
                found(3, "\"worth noting\" (commentary)"),
                found(3, "\"simply\" (hedging)"),
                found(3, "\"which means\" (restating)"),
                found(3, "\"leverage\" (marketing)"),
                found(3, "\"long pole\" (idiom)"),
            ]
        );
        assert_eq!(
            scan_markdown(markdown, 10)[0],
            found(12, "\"worth noting\" (commentary)")
        );
    }

    #[test]
    fn code_blocks_code_spans_and_quotes_are_skipped() {
        let markdown = "Say `worth noting` or \"note that\" here.\n\n```\nobviously; robust\n```\n\n    long pole indented\n\n| a | b |\n|---|---|\n| `powerful` | \"elegant\" |\n\nAfter the block, simply.\n";
        assert_eq!(
            scan_markdown(markdown, 1),
            vec![found(13, "\"simply\" (hedging)")]
        );
    }

    #[test]
    fn matches_on_word_boundaries_regardless_of_case() {
        let markdown = "Simplify it. NOTE THAT we denote that clear things are clearest.\n";
        assert_eq!(
            scan_markdown(markdown, 1),
            vec![found(1, "\"note that\" (commentary)")]
        );
    }

    #[test]
    fn a_hyphen_and_a_space_are_the_same() {
        let markdown = "A load-bearing wall and a first class seat.\n";
        assert_eq!(
            scan_markdown(markdown, 1),
            vec![
                found(1, "\"load bearing\" (idiom)"),
                found(1, "\"first class\" (idiom)"),
            ]
        );
    }

    #[test]
    fn a_phrase_split_over_a_line_break_is_found_on_its_first_line() {
        let markdown = "Text.\nNote\nthat.\n";
        assert_eq!(
            scan_markdown(markdown, 1),
            vec![found(2, "\"note that\" (commentary)")]
        );
    }

    #[test]
    fn punctuation_is_reported() {
        let markdown = "One — two.\n\nA – b, but 2010–2012 is fine.\n\nAnd 5 –6 is fine.\n\nThis; that.\n\n`a; b` and \"x; y\" pass.\n";
        assert_eq!(
            scan_markdown(markdown, 1),
            vec![
                found(1, "an em dash (punctuation)"),
                found(3, "an en dash used as a separator (punctuation)"),
                found(7, "a semicolon (punctuation)"),
            ]
        );
    }

    #[test]
    fn an_unclosed_quote_ends_with_its_paragraph() {
        let markdown = "An unclosed \"quote here.\n\nBut simply here is seen.\n";
        assert_eq!(
            scan_markdown(markdown, 1),
            vec![found(3, "\"simply\" (hedging)")]
        );
    }

    #[test]
    fn rust_comments_are_scanned_and_strings_are_not() {
        let source = [
            "//! Crate docs.",
            "//!",
            "//! It is simply a crate.",
            "",
            "/// Doc.",
            "///",
            "/// ```",
            "/// let x = \"obviously\"; // robust",
            "/// ```",
            "fn f() {",
            "    let s = \"worth noting\";",
            "    let t = \"http://x\"; // seamless",
            "    // note",
            "    // that",
            "}",
        ]
        .join("\n");
        assert_eq!(
            scan_rust(&source),
            vec![
                found(3, "\"simply\" (hedging)"),
                found(12, "\"seamless\" (marketing)"),
                found(13, "\"note that\" (commentary)"),
            ]
        );
    }

    #[test]
    fn an_escaped_quote_does_not_end_a_string() {
        assert_eq!(
            trailing_comment("    let u = \"{\\\"a\\\":[\\\"nats://x\\\"]}\";"),
            None
        );
        assert_eq!(
            trailing_comment(r#"    let u = "\"a\""; // simply"#),
            Some("simply")
        );
    }

    #[test]
    fn a_commit_message_skips_the_lines_git_drops() {
        let message = "Add the thing\n\nIt is basically done.\n# Please enter the message, which means nothing\n";
        assert_eq!(
            scan_message(message),
            vec![found(3, "\"basically\" (hedging)")]
        );
    }

    #[test]
    fn files_are_reported_by_path_and_line_and_target_is_skipped() {
        let repo = TempRepo::new("prose");
        fs::create_dir_all(repo.0.join("target")).unwrap();
        fs::create_dir_all(repo.0.join("src")).unwrap();
        repo.write("docs/page.md", "Covers: x\n\nA robust page.\n");
        repo.write("src/lib.rs", "//! Fine.\n\nfn f() {} // under the hood\n");
        repo.write("target/x.md", "obviously\n");
        assert_eq!(
            check(&repo.0),
            vec![
                "docs/page.md:3: \"robust\" (marketing)".to_owned(),
                "src/lib.rs:3: \"under the hood\" (idiom)".to_owned(),
            ]
        );
    }

    #[test]
    fn run_fails_with_the_findings_and_passes_without() {
        let repo = TempRepo::new("prose-run");
        repo.write("docs/page.md", "Covers: x\n\nA robust page.\n");
        assert_eq!(
            run(&repo.0, None),
            Err("docs/page.md:3: \"robust\" (marketing)".to_owned())
        );
        repo.write("docs/page.md", "Covers: x\n\nA plain page.\n");
        assert_eq!(run(&repo.0, None), Ok(()));
    }

    #[test]
    fn run_checks_one_message_file_instead_of_the_repo() {
        let repo = TempRepo::new("prose-message");
        repo.write("docs/page.md", "A robust page.\n");
        repo.write(
            "message.txt",
            "Add a thing\n\nIt is simply done.\n# comment\n",
        );
        let message = repo.0.join("message.txt");
        assert_eq!(
            run(&repo.0, Some(&message)),
            Err(format!("{}:3: \"simply\" (hedging)", message.display()))
        );
        repo.write("message.txt", "Add a thing\n");
        assert_eq!(run(&repo.0, Some(&message)), Ok(()));
        assert!(
            run(&repo.0, Some(&repo.0.join("missing.txt")))
                .unwrap_err()
                .contains("missing.txt")
        );
    }

    fn every(_: usize, _: usize) -> bool {
        true
    }

    const EVERYTHING: Scope = Scope {
        duplicates: true,
        relative_time: true,
    };

    fn warn(markdown: &str, changed: &dyn Fn(usize, usize) -> bool) -> Vec<Finding> {
        warn_blocks(&[(1, markdown.to_owned())], changed, EVERYTHING)
    }

    /// A sentence with six content words, a second that repeats them in another order, and a third that repeats
    /// them again.
    const FIRST: &str = "The driver flushes the socket buffer after its writes complete.";
    const SECOND: &str = "After its writes complete, the driver flushes the socket buffer.";
    const THIRD: &str = "After its writes complete the socket buffer flushes the driver.";

    #[test]
    fn each_warned_group_is_reported_with_its_line() {
        let markdown = "It fails, which is rare.\n\nThe frame carries a length.\n\nWe did it in order to test. It is actually fine.\n\nThe latter wins.\n\nThe carryover and `x, which is y` stay.\n\nThe obvious answer fails.\n\nThe pins hold, see <https://example.com/a>.\n\nThis code wraps.\n\nThe core whose turn it is waits.\n\nThe chip resets unless the dog is fed. Core 1 sleeps itself.\n";
        assert_eq!(
            warn(markdown, &every),
            vec![
                found(1, "\", which is\" (restating)"),
                found(3, "\"carries\" (containment)"),
                found(5, "\"in order to\" (filler)"),
                found(5, "\"actually\" (hedging)"),
                found(7, "\"the latter\" (far reference)"),
                found(11, "\"obvious\" (hedging)"),
                found(13, "\", see\" (citation)"),
                found(15, "\"this code\" (diff reference)"),
                found(17, "\"whose\" (unapproved word)"),
                found(19, "\"unless\" (unapproved word)"),
                found(19, "\"itself\" (unapproved word)"),
            ]
        );
    }

    #[test]
    fn an_unusual_verb_and_a_negated_noun_after_has_are_reported() {
        let markdown = "The proxy gains a handle.\n\nThe methods are unused and go.\n\nThe clock has no time.\n";
        assert_eq!(
            warn(markdown, &every),
            vec![
                found(1, "\"gains\" (unusual verb)"),
                found(3, "\"and go\" (unusual verb)"),
                found(5, "\"has no\" (restating)"),
            ]
        );
    }

    #[test]
    fn a_negated_noun_after_do_or_keep_is_reported_and_other_verbs_are_not() {
        let markdown = "The parser does no I/O.\n\nThe agent keeps no memory.\n\nThe board sends no RST. It does not need one, and keeps nothing.\n";
        assert_eq!(
            warn(markdown, &every),
            vec![
                found(1, "\"does no\" (restating)"),
                found(3, "\"keeps no\" (restating)"),
            ]
        );
    }

    #[test]
    fn a_sentence_that_shares_most_of_its_words_with_another_is_reported() {
        let markdown = format!("{FIRST}\n\nOther text here.\n\n{SECOND}\n");
        assert_eq!(
            warn(&markdown, &every),
            vec![found(5, "shares most of its words with line 1 (restating)")]
        );
        let twice = format!("{FIRST}\n\n{SECOND} {THIRD}\n");
        assert_eq!(
            warn(&twice, &every),
            vec![
                found(3, "shares most of its words with line 1 (restating)"),
                found(
                    3,
                    "shares most of its words with an earlier sentence on the same line (restating)",
                ),
            ]
        );
        let elsewhere = warn_blocks(
            &[(1, markdown)],
            &every,
            Scope {
                duplicates: false,
                ..EVERYTHING
            },
        );
        assert_eq!(elsewhere, vec![]);
    }

    #[test]
    fn a_heading_a_summary_a_table_row_and_a_short_sentence_are_not_compared() {
        let markdown = format!(
            "# {FIRST}\n\n## Summary\n\n- {FIRST}\n\n## Decision\n\n{SECOND}\n\n| {FIRST} |\n\nThe driver flushes the socket after its writes. After its writes, the driver flushes the socket.\n"
        );
        assert_eq!(warn(&markdown, &every), vec![]);
    }

    #[test]
    fn relative_time_a_long_sentence_and_two_brackets_are_reported() {
        let long = format!("{}end.", "word ".repeat(35));
        let limit = format!("{}end.", "word ".repeat(34));
        let markdown =
            format!("It runs today.\n\nIt stops (K-1) and waits (K-2).\n\n{long}\n\n{limit}\n");
        assert_eq!(
            warn(&markdown, &every),
            vec![
                found(1, "\"today\" (relative time)"),
                found(3, "two brackets in one sentence (length)"),
                found(5, "a sentence of 36 words (length)"),
            ]
        );
        let message = warn_blocks(
            &[(1, markdown)],
            &every,
            Scope {
                relative_time: false,
                ..EVERYTHING
            },
        );
        assert_eq!(
            message,
            vec![
                found(3, "two brackets in one sentence (length)"),
                found(5, "a sentence of 36 words (length)"),
            ]
        );
    }

    #[test]
    fn the_one_statement_folders_are_compared_and_other_files_are_not() {
        let compared: Vec<bool> = [
            "docs/entities/a.md",
            "docs/summaries/a.md",
            "docs/decisions/0001-a.md",
            "docs/concepts/a.md",
            "REQUIREMENTS.md",
            "docs/README.md",
            "docs/todo.md",
            "entities/a.md",
        ]
        .iter()
        .map(|path| compares_duplicates(Path::new(path)))
        .collect();
        assert_eq!(
            compared,
            [false, false, true, false, false, false, false, false]
        );
    }

    #[test]
    fn only_the_changed_paragraphs_are_warned_on() {
        let markdown = format!("{FIRST}\n\n{SECOND}\n\nIt fails, which is rare.\n");
        let markdown = markdown.as_str();
        assert_eq!(
            warn(markdown, &|first, _| first == 1),
            vec![found(1, "shares most of its words with line 3 (restating)")]
        );
        assert_eq!(
            warn(markdown, &|first, last| first <= 5 && 5 <= last),
            vec![found(5, "\", which is\" (restating)")]
        );
    }

    #[test]
    fn a_this_opener_two_causes_and_two_so_openers_are_reported() {
        let markdown = "This lets the test run.\n\nIt stops because the bus resets, so the run ends.\n\nSo it waits.\n\nSo it ends.\n\nIt runs.\n\nSo it stops.\n";
        assert_eq!(
            warn(markdown, &every),
            vec![
                found(1, "\"this lets\" opens a sentence (restating)"),
                found(3, "two causes in one sentence (restating)"),
                found(7, "two sentences in a row open with \"so\" (restating)"),
            ]
        );
    }

    #[test]
    fn a_stem_drops_ing_ed_and_a_plural_s_from_a_long_enough_word() {
        let stems: Vec<String> = [
            "flushing", "being", "flushed", "used", "uses", "bus", "class",
        ]
        .iter()
        .map(|word| stem(word))
        .collect();
        assert_eq!(
            stems,
            ["flush", "being", "flush", "used", "use", "bus", "class"]
        );
    }

    #[test]
    fn a_list_item_a_numbered_item_and_a_blank_line_start_a_paragraph() {
        let markdown = "1. One.\n2. Two.\n. more\n3 items\n\n- a\n- b";
        assert_eq!(
            paragraphs(markdown),
            vec![(0, 7), (8, 30), (32, 35), (36, 39)]
        );
    }

    #[test]
    fn a_hard_wrapped_comment_is_one_paragraph() {
        let source = "/// The run stops,\n/// which is rare.\nfn f() {}\n";
        assert_eq!(
            rust_blocks(source),
            vec![(1, "The run stops,\nwhich is rare.".to_owned())]
        );
        assert_eq!(
            warn_blocks(&rust_blocks(source), &every, EVERYTHING),
            vec![found(1, "\", which is\" (restating)")]
        );
    }

    #[test]
    fn the_changed_ranges_come_from_the_new_side_of_each_hunk() {
        let diff = "diff --git a/docs/a.md b/docs/a.md\n--- a/docs/a.md\n+++ b/docs/a.md\n@@ -3 +3 @@\n-old\n+new\n@@ -5,2 +4,0 @@\n-p\n-q\n@@ -10,0 +11,2 @@\n+x\n+y\ndiff --git a/gone.md b/gone.md\n--- a/gone.md\n+++ /dev/null\n@@ -1 +0,0 @@\n-z\n";
        let mut expected = Changed::new();
        expected.insert("docs/a.md".into(), vec![(3, 3), (4, 4), (11, 12)]);
        assert_eq!(changed_ranges(diff), expected);
    }

    /// A git repo with `docs/a.md` committed.
    fn committed(name: &str) -> TempRepo {
        let repo = TempRepo::new(name);
        for args in [
            &["init", "--quiet"][..],
            &["config", "user.email", "test@example.com"],
            &["config", "user.name", "test"],
        ] {
            super::super::git(&repo.0, args).unwrap();
        }
        repo.write(
            "docs/a.md",
            "Covers: x\n\nIt fails, which is rare.\n\nIt stops, which is fine.\n",
        );
        super::super::git(&repo.0, &["add", "."]).unwrap();
        super::super::git(&repo.0, &["commit", "--quiet", "-m", "a"]).unwrap();
        repo
    }

    #[test]
    fn a_repo_is_warned_on_where_it_differs_from_head_or_everywhere() {
        let repo = committed("prose-warn");
        repo.write(
            "docs/a.md",
            "Covers: x\n\nIt fails, which is rare.\n\nIt halts, which is fine.\n",
        );
        let warning =
            |line: usize| format!("docs/a.md:{line}: warning: \", which is\" (restating)");
        assert_eq!(warnings(&repo.0, None, false), Ok(vec![warning(5)]));
        assert_eq!(
            warnings(&repo.0, None, true),
            Ok(vec![warning(3), warning(5)])
        );
        assert_eq!(run(&repo.0, None), Ok(()));
    }

    #[test]
    fn a_file_git_does_not_track_is_warned_on_whole_unless_ignored() {
        let repo = committed("prose-warn-untracked");
        repo.write(".gitignore", "ignored.md\n");
        repo.write("docs/new.md", "Covers: x\n\nIt fails, which is rare.\n");
        repo.write("ignored.md", "It fails, which is rare.\n");
        assert_eq!(
            warnings(&repo.0, None, false),
            Ok(vec![
                "docs/new.md:3: warning: \", which is\" (restating)".to_owned()
            ])
        );
    }

    #[test]
    fn a_repo_compares_sentences_only_in_the_one_statement_folders() {
        let repo = committed("prose-warn-folders");
        let repeated = format!("Covers: x\n\n{FIRST}\n\n{SECOND}\n");
        fs::create_dir_all(repo.0.join("docs/decisions")).unwrap();
        fs::create_dir_all(repo.0.join("docs/entities")).unwrap();
        fs::create_dir_all(repo.0.join("src")).unwrap();
        repo.write("docs/decisions/0001-e.md", &repeated);
        repo.write("docs/entities/e.md", &repeated);
        repo.write("REQUIREMENTS.md", &repeated);
        repo.write("src/lib.rs", &format!("//! {FIRST}\n//!\n//! {SECOND}\n"));
        assert_eq!(
            warnings(&repo.0, None, true),
            Ok(vec![
                "docs/a.md:3: warning: \", which is\" (restating)".to_owned(),
                "docs/a.md:5: warning: \", which is\" (restating)".to_owned(),
                "docs/decisions/0001-e.md:5: warning: shares most of its words with line 3 (restating)"
                    .to_owned(),
            ])
        );
    }

    #[test]
    fn a_message_is_warned_on_whole_and_a_tree_outside_git_not_at_all() {
        let repo = TempRepo::new("prose-warn-message");
        repo.write("docs/a.md", "It fails, which is rare.\n");
        repo.write(
            "message.txt",
            "Add a thing\n\nIt fails, which is rare. It runs now.\n# x, which is y\n",
        );
        let message = repo.0.join("message.txt");
        assert_eq!(
            warnings(&repo.0, Some(&message), false),
            Ok(vec![format!(
                "{}:3: warning: \", which is\" (restating)",
                message.display()
            )])
        );
        assert_eq!(warnings(&repo.0, None, false), Ok(vec![]));
    }
}
