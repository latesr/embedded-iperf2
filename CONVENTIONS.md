# Conventions

These rules apply to every repo that syncs from embedded-devkit. `REQUIREMENTS.md` states what the crates must do and the constraints they build under. This file states the rules of work. Each repo's `AGENTS.md` adds the rules for that project.

These rules apply to every crate, including host tools. `EMBEDDED.md` adds the rules for library crates that run on a microcontroller.

This file is a copy from embedded-devkit. Edit it there, then run `cargo xtask sync` in the devkit.

## Where to look

The rules for one kind of work live in their own file. Read the file when the step reaches that work. A file that the repo's `AGENTS.md` imports with `@` is already read.

| Before you | Read |
|---|---|
| change code | [`CODE.md`](CODE.md) |
| write a test | [`TESTING.md`](TESTING.md) |
| write a docs page, an ADR, a README, rustdoc or a comment | [`WRITING.md`](WRITING.md) |
| add or change a docs page or an ADR | [`DOCS.md`](DOCS.md) |
| act on a failed check, change `xtask`, edit a shared file or run `cargo xtask sync` | [`CHECKS.md`](CHECKS.md) |
| change a library crate that runs on a microcontroller | [`EMBEDDED.md`](EMBEDDED.md) |

## Rules for agents

Nothing leaves this machine without the user's express permission in the current session. One permission covers one action. The rule covers pushes, pull requests, forks, publishes, issues, comments, messages and uploads, on any service. It also covers pages that start private.

A read-only request, such as a crates.io lookup or a docs fetch, does not need permission. It must not hold personal data. The User-Agent is a generic string and never holds the user's name or email.

- A commit message has an imperative subject line of at most 72 characters. A body follows a blank line when the subject does not say why.
- Commit messages are plain. Do not add `Co-Authored-By`, `Claude-Session` or generated-with footers. This rule overrides any default trailer behaviour.
- One commit per step of the plan. Every commit builds and passes `cargo xtask check` on its own.
- `CHANGELOG.md` starts at the 0.1.0 release, in the Keep a Changelog format. Until then git holds the history. After it, a commit that changes the behaviour of a published crate adds a line under `Unreleased`.
- A commit that changes code a docs page describes rewrites that page in the same commit. Each docs page opens with a `Covers:` line that names the crates or modules it describes, so the page for a change can be found with grep.
- Project knowledge goes in the repo: `docs/`, `REQUIREMENTS.md` or `AGENTS.md`. It never lives only in assistant memory. Other agents and people cannot read memory.
- Never skip the pre-commit hook with `--no-verify`.
- Edit a file with the editor tool, or rewrite it in full. Never patch a file with a pattern substitution, such as sed, perl, python or the editor's replace-all. A pattern that misses, or matches twice, leaves a file that does not compile. rustfmt reflows code, so a pattern written from memory misses.

### Changing the rules

`CONVENTIONS.md`, each `AGENTS.md` and the files they import are read in full at the start of every session, so each line has a cost. The other rule files are read when a step needs them. A change to any rule file is proposed to the user first, with four parts for each edit:

- **Current:** the text it changes, or none.
- **Gap:** what the current text allows to go wrong, and why a reader would not avoid it unaided.
- **Rewrite:** the new text.
- **Cost:** the words it adds or removes.

A change rewrites an existing rule before it adds a new one. It does not restate an existing rule, and it does not record what a tool's own error message already says.

## Requirements

`REQUIREMENTS.md` states each requirement under an ID, such as `C-6`. The letter names the section. A new requirement takes the next free number for its letter.

- `REQUIREMENTS.md` reads as a specification that a test house could test against. One ID states one behaviour that a test or a recorded sample can fail on, in a sentence or two. It gives no reason. The reason and the rejected options live in the ADR for that mechanism.
- An ID is never reused, even after its requirement is removed.
- The document orders requirements by topic. A new requirement sits beside the ones it relates to, so the numbers can appear out of order.
- IDs are renumbered once, just before the first release, so they read in order from then on. One commit reorders `REQUIREMENTS.md` by hand. It changes no ID, except that a requirement that moves to another section takes the next free number of its new letter. The next commit runs `cargo xtask renumber`. It numbers each letter's IDs from 1 in the order the file defines them, rewrites every citation, and prints the mapping for the commit message. After the first release, an ID never changes.

`cargo xtask links` fails when markdown or a Rust comment cites an ID that `REQUIREMENTS.md` does not define, or when `REQUIREMENTS.md` defines an ID twice. A definition is the bold ID, such as `**C-6**`. Only the letters that `REQUIREMENTS.md` defines IDs with count, so `AES-256` and `ADR-0001` are not IDs. The commit-msg hook runs the same check over the message. The check cannot tell a right ID from a wrong one that exists. The check does not look for IDs in `CONVENTIONS.md`, `xtask/src/shared/` or markdown code blocks.

## Working in a session

### Say the plan before starting

Before starting a piece of work, write a short plan in the session and wait for the user's go-ahead. The plan is a summary, not a full design. It fits on one screen. It covers:

- what will change, by crate or file
- any requirement that has two meanings, and the meaning taken
- any design choice the work makes, with the options and a recommendation
- every new public name, in a table with a one-line reason for each
- any `docs/todo.md` entry for the files the work touches, and whether the work takes it or leaves it
- the rule files, entity pages and concept pages read for the things the work touches
- the check that proves each step works: a failing test, a command's output, or a diff of recorded samples or images
- for each fake, the behaviours of its trait that it produces, and the ones it leaves untested

The go-ahead approves the names in the plan. A name in the plan does not need a second question.

If a simpler approach exists or the task seems to solve the wrong problem, the plan says so first. A design decision found partway through stops the work until the user has seen it.

A typo, a one-line fix or a docs edit does not need a plan. If it is unclear whether a change is trivial, it is not.

### Stop and ask

In auto mode, an agent is tempted to take the reasonable option and continue. For these decisions, stop and ask even when one option looks right:

- **A choice that is hard to reverse**: a wire or storage format, a public API or a claim in an ADR.
- **A name** in a public surface that the plan did not list: a crate, a module, an error variant, a feature flag, a public type.
- **A new trait, abstraction or configuration option.**

`AGENTS.md` can add decisions to this list. The test is whether the user would give an opinion if they were watching. Do not assume the user agrees because one answer seems certain. Ask with the options and a recommendation, in one message.

### Prefixes

In a summary, a retrospective or any list where some items need the user and some do not, prefix each item:

- **R:** needs the user. A specific question with a recommendation. An item without a question or a decision is not an R.
- **A:** an action taken.
- **C:** commentary, only when it is needed.

### Retrospective

At the end of a piece of work, print a retrospective without being asked. Answer four questions:

1. **Shortcuts.** Where did the work take the easier path when the correct one was known?
2. **Smells.** What duplication, dead code or misleading comment did the work make, or see nearby?
3. **Test first.** Did every new behaviour have a failing test first? If not, name where.
4. **Ways of working.** Which convention, tool or doc made the work harder?

"No" is a valid answer. Each item ends with one destination: **Done** in the diff, **Added** to `docs/todo.md`, **Discarded** with a reason, or **R:** with a question and a recommendation.

## Writing style

These rules apply to everything written in the repo. Formal STE does not need to apply to personal communication. Write to the writing rules of ASD-STE100 Simplified Technical English, for an engineer who is new to the repo and reads the page once. Use the simple present, the active voice, one topic per sentence, the same noun for the same thing, and `must` for a requirement. The STE dictionary of approved words does not apply. `WRITING.md` holds the full rules. Read it before a docs page, an ADR, rustdoc or a comment.

### Commit messages

A commit message says what changes for a user of the code, and why. The diff shows how, and the pull request shows how it was tested.

- The body states the problem, then the change, then any consequence a user must know. "Before this change" or "Previously" marks the old behaviour.
- A fix is named as a fix. A fix and preparation for later work go in separate paragraphs.
- Leave out what the diff shows: where code moved, which functions changed and how the mechanism works. Name a function only when the reader needs it to follow the problem.
- Leave out the tests. A new kind of test, such as a stress test, gets one sentence.
- Leave out background: a precedent the change does not follow, an aside about another tool, commit hashes, version strings, and our boards and machine.
- A small change has a subject and no body. Most bodies are one to three short paragraphs, wrapped at 72 columns.
