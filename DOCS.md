# Docs

These rules state where each kind of knowledge goes, and add to `WRITING.md` for an Architectural Decision Record (ADR).

This file is a copy from embedded-devkit. Edit it there, then run `cargo xtask sync` in the devkit.

## What goes where

Each kind of knowledge has one home:

- How to use a crate: its README and rustdoc. A family or board crate's README links the vendor documents for its chip, module, and board.
- Why the code is as it is: an ADR, or a comment at the code (`CODE.md`).
- What a crate must do: `REQUIREMENTS.md`.
- Open work and findings: the plan, outside the repo.

`docs/` holds the ADRs, in `docs/decisions/`, and `docs/README.md`, which indexes them.

A byte layout that a recorded sample or image tests is a requirement. Other implementation detail, such as a timing table or a buffer layout, lives in module-level rustdoc beside the code. An ADR links it.

A link between files in the repo is relative, never an absolute GitHub URL. The link check can then resolve it, and it survives a fork or a rename.

## ADRs

An ADR records a decision about the product, or about how it is built, tested, and shipped. The development process and the docs conventions live in the rule files and in `AGENTS.md` instead.

One ADR covers one mechanism, such as reconnect or factory reset, and every decision inside it. It is written when the step that builds the mechanism is done and reviewed. Until then the plan holds the step's decisions.

A choice belongs in an ADR when a competent newcomer would propose the option it rejected. A parameter, a name, an error variant, or a choice without a second option does not. A Summary bullet states the rule in one sentence, and what the rule makes happen in a second. It cites the requirement IDs it meets, and leaves rejected options to Alternatives. It does not restate a requirement, a number, a signature, or a default.

ADRs live at `docs/decisions/NNNN-slug.md`. Each opens with `# ADR-NNNN: <the mechanism>`, such as `# ADR-0004: Reconnect`, and a bold status line: `**Status: accepted**`, `**Status: proposed**`, or `**Status: superseded by ADR-NNNN**`. A `Covers:` line follows, and names what the ADR describes. A `## Summary` section follows, with one bullet per decision as it stands. The Context, Decision, Alternatives, and Consequences follow the Summary. A new ADR gets its row in the index in `docs/README.md` in the same commit.

- The Context states the problem in enough detail that a newcomer could judge the decision from it. Where a newcomer's first answer does not work here, it says why, without calling that answer "obvious".
- An ADR states a rule. It does not state a list of cases, because a list goes stale.
- Until the first release, an ADR is edited in place to state the current decision, without a History bullet or a new ADR. The commit message says what changed.
- After the first release, a change that keeps every Summary bullet is edited into the ADR. It adds a dated bullet to a `## History` section: `YYYY-MM-DD: Amended: <what changed>.`
- After the first release, a change that reverses a Summary bullet is a new ADR. The new ADR says what it supersedes, and the old ADR's status becomes `superseded by ADR-NNNN`.
- A wording fix or a renamed path does not need a History bullet.
