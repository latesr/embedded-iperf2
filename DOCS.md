# Docs

These rules state what goes in `docs/` and in an Architectural Decision Record (ADR). `WRITING.md` states how a page is written.

This file is a copy from embedded-devkit. Edit it there, then run `cargo xtask sync` in the devkit.

## Page types

`docs/` has five directories. Every page opens with a `Covers:` line that names what it describes.

- `entities/` holds the chips, boards, libraries and upstream projects the crates build against: what each is, what it does and what it does not do. A versioned entity has a section per version that was pinned or tested.
- `concepts/` explains how a mechanism works, independent of any one entity. A term used in a requirement, an ADR or a doc comment links its concept page, and that page is the only place that defines it.
- `comparisons/` compares entities against stated criteria. The criteria come first. A number comes from a named xtask command or test.
- `summaries/` holds the claims this repo takes from one read source, each with the source section and the date read. A summary holds only claims that another page cites.
- `decisions/` holds the ADRs.

A byte layout that a recorded sample or image tests is a requirement. Other implementation detail, such as a timing table or a buffer layout, lives in module-level rustdoc beside the code. An ADR or a page links it.

- A page other than an ADR exists only when a requirement, an ADR or another page links it. The index in `docs/README.md` does not count. An ADR records a decision, so the index is enough for it.
- A page lists observed facts and read facts under separate headings. Every fact comes from a public source or from an observation made in this project on the boards in `REQUIREMENTS.md`. An observed fact points to the test, the recorded sample or the image. A read fact points to the summary page of the document it comes from. A fact read from source code links the file at the commit it was read from, and does not need a summary page.
- `docs/README.md` indexes every page, and a new page gets its row in the same commit. A link between files in the repo is relative, never an absolute GitHub URL, so the link check can resolve it and it survives a fork or a rename.

## ADRs

An ADR records a decision about the product, or about how it is built, tested and shipped. The development process and the docs conventions live in the rule files and in `AGENTS.md` instead.

One ADR covers one mechanism, such as reconnect or factory reset, and every decision inside it. It is written when the step that builds the mechanism is done and reviewed. Until then the plan holds the step's decisions.

A choice belongs in an ADR when a competent newcomer would propose the option it rejected. A parameter, a name, an error variant or a choice without a second option does not. A Summary bullet states the rule in one sentence, and what the rule makes happen in a second. It cites the requirement IDs it meets, and leaves rejected options to Alternatives. It does not restate a requirement, a number, a signature or a default.

ADRs live at `docs/decisions/NNNN-slug.md`. Each opens with `# ADR-NNNN: <the mechanism>`, such as `# ADR-0004: Reconnect`, and a bold status line: `**Status: accepted**`, `**Status: proposed**` or `**Status: superseded by ADR-NNNN**`. A `## Summary` section follows, with one bullet per decision as it stands. The Context, Decision, Alternatives and Consequences follow the Summary. A new ADR gets its row in the index in `docs/README.md` in the same commit.

- The Context states the problem in enough detail that a newcomer could judge the decision from it. Where a newcomer's first answer does not work here, it says why, without calling that answer "obvious".
- An ADR states a rule. It does not state a list of cases, because a list goes stale.
- Implementation detail lives in rustdoc, and a format lives in `REQUIREMENTS.md`. The ADR links or cites it.
- Until the first release, an ADR is edited in place to state the current decision, without a History bullet or a new ADR. The commit message says what changed.
- After the first release, a change that keeps every Summary bullet is edited into the ADR. It adds a dated bullet to a `## History` section: `YYYY-MM-DD: Amended: <what changed>.`
- After the first release, a change that reverses a Summary bullet is a new ADR. The new ADR says what it supersedes, and the old ADR's status becomes `superseded by ADR-NNNN`.
- A wording fix or a renamed path does not need a History bullet.

## `docs/todo.md`

`docs/todo.md` holds findings that a piece of work saw and did not fix. Each entry is one line: the date, the file or item, the finding, and the short hash of the commit that found it. A plan reads the entries for the files it touches. The commit that fixes a finding removes its line.
