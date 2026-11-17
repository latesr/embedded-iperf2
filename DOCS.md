# Docs

These rules state where each kind of knowledge goes, and add to `WRITING.md` for an Architectural Decision Record (ADR).

This file is a copy from embedded-conventions. Edit it there, then run `cargo xtask sync` in embedded-conventions.

## What goes where

Each kind of knowledge has one home:

- How to use a crate: its README and rustdoc. A family or board crate's README links the vendor documents for its chip, module, and board.
- Why the code is as it is: an ADR, or a comment at the code (`CODE.md`).
- What a crate must do, and the reason when the reader would propose another behaviour: `REQUIREMENTS.md`.
- Open work and findings: the plan, outside the repo.

`docs/` holds the ADRs, in `docs/decisions/`, and `docs/README.md`, which indexes them.

A byte layout that a recorded sample or image tests is a requirement. Other implementation detail, such as a timing table or a buffer layout, lives in module-level rustdoc beside the code. An ADR links it.

A link between files in the repo is relative, never an absolute GitHub URL. The link check can then resolve it, and it survives a fork or a rename.

## ADRs

An ADR records a decision about the product, or about how it is built, tested, and shipped. The development process and the docs conventions live in the rule files and in `AGENTS.md` instead.

One ADR covers one mechanism, such as reconnect or factory reset, and every decision inside it. It is written when the step that builds the mechanism is done and reviewed. Until then the plan holds the step's decisions.

An ADR records a high-level decision that the reader would otherwise undo: the shape of a mechanism, not its parameters. A name, a number, a timeout, an error variant, or a choice without a second option does not go in an ADR. Detail goes in rustdoc, and a Decision bullet names the file that holds it. An ADR states a rule. It does not state a list of cases, because a list goes stale.

ADRs live at `docs/decisions/NNNN-slug.md`. Each opens with `# ADR-NNNN: <the mechanism>`, a bold status line (`**Status: accepted**`, `**Status: proposed**`, or `**Status: superseded by ADR-NNNN**`), and a `Covers:` line. These sections follow:

- `## Context`: the problem, in a few sentences.
- `## Decision`: one bullet for each decision that the reader would otherwise undo, with its reason. A bullet does not restate a requirement or give its reason. The requirement's `Rationale:` line holds that reason.
- `## Rejected`: one bullet for each option that the reader would propose, as `**The option.** The reason it does not work here.`
- `## Consequences`, when the decision costs a user something or obliges later work to do something: one bullet for each.

An ADR over 500 words holds detail that belongs in rustdoc, or a second mechanism. Move the detail, or split the ADR. Do not harm readability or the writing rules to make it fit. An ADR has at most 1,000 words. A requirement ID appears only where the requirement is the reason for a decision. A new ADR gets its row in the index in `docs/README.md` in the same commit.

- Until the first release, an ADR is edited in place to state the current decision, without a History bullet or a new ADR. The commit message says what changed.
- After the first release, a change that keeps every Decision bullet is edited into the ADR. It adds a dated bullet to a `## History` section: `YYYY-MM-DD: Amended: <what changed>.`
- After the first release, a change that reverses a Decision bullet is a new ADR. The new ADR says what it supersedes, and the old ADR's status becomes `superseded by ADR-NNNN`.
- A wording fix or a renamed path does not need a History bullet.

### Reviewing an ADR

A review checks each of these, and names the line of each finding:

1. Read the ADR once, as the reader described in `CONTRIBUTING.md`. Mark each bullet that needs a second read.
2. Check each sentence against each rule of `WRITING.md`, in turn.
3. Each Decision bullet holds one decision. Its reason is on the sentence that it explains.
4. Each Rejected option is one that the reader would propose. Its reason says why the option fails here.
5. Each fact matches the code, or the pinned source that it links.
6. The ADR does not hold detail that rustdoc states, and does not restate a requirement. A rewrite does not lose a decision of the earlier version.
7. A requirement ID appears only where the requirement is the reason for a decision.
