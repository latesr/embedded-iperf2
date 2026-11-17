# Contributing

These rules apply to every change, to every crate in every repo that syncs from embedded-devkit, host tools included. `REQUIREMENTS.md` states what the crates must do. Each repo's `AGENTS.md` adds the rules for that repo.

This file is a copy from embedded-devkit. Edit it there, then run `cargo xtask sync` in the devkit.

## Where to look

Each kind of work has its own rule file. Read the file when the work starts. A file that `AGENTS.md` imports with `@` is already read.

| Before you | Read |
|---|---|
| change code | [`CODE.md`](CODE.md) |
| write a test | [`TESTING.md`](TESTING.md) |
| write an ADR, a README, rustdoc, or a comment | [`WRITING.md`](WRITING.md) |
| add or change an ADR | [`DOCS.md`](DOCS.md) |
| act on a failed check, change `xtask`, or renumber the requirements | [`CHECKS.md`](CHECKS.md) |
| change code that runs on a microcontroller | [`EMBEDDED.md`](EMBEDDED.md) |

## Every change

- Every commit passes `cargo xtask check` on its own. A branch passes `cargo xtask check --push` and the mutation check before it is pushed.
- A commit that changes code that an ADR or a README describes updates that text in the same commit.
- `CHANGELOG.md` starts at the 0.1.0 release, in the Keep a Changelog format. After that release, a commit that changes the behaviour of a published crate adds a line under `Unreleased`.

## Requirements

`REQUIREMENTS.md` states each requirement under an ID, such as `C-6`. The letter names the section. A new requirement takes the next free number for its letter. It sits beside the requirements it relates to, so the numbers can appear out of order. An ID is never reused.

`REQUIREMENTS.md` reads as a specification that a test house could test against. One ID states one behaviour that a test or a recorded sample can fail on, in a sentence or two. It states what the crate guarantees, not the mechanism, a workaround, or a pinned version. When the reader would propose another behaviour, a `Rationale:` line under the requirement gives the reason and names that behaviour, in one or two sentences. A test does not check a `Rationale:` line. Any other reason goes in a comment at the code. A rule about how the code is written goes in an ADR, not in a requirement.

`CHECKS.md` states how the IDs are checked, and how they are renumbered once before the first release.

## Writing style

Everything written in the repo follows the writing rules of ASD-STE100 Simplified Technical English, without its dictionary. The reader is an engineer who is new to the repo and reads the page once. Use the simple present, the active voice, one topic per sentence, the same noun for the same thing, and `must` for a requirement. `WRITING.md` holds the full rules.

### Commit messages

A commit message says what changes for a user of the code, and why. The diff shows how, and the pull request shows how it was tested.

- The subject line is imperative, and at most 72 characters. A body follows a blank line when the subject does not say why.
- The body states the problem, then the change, then any consequence a user must know. "Before this change" or "Previously" marks the old behaviour.
- A fix is named as a fix. A fix and preparation for later work go in separate paragraphs.
- Leave out what the diff shows: where code moved, which functions changed, and how the mechanism works. Name a function only when the reader needs it to follow the problem.
- Leave out testing details. A commit that changes how the code is tested, such as with a new stress test, says so in one sentence.
- Leave out background: a precedent the change does not follow, an aside about another tool, commit hashes except a sync's devkit commit, version strings, and the developer's boards and machine.
- A small change has a subject and no body. Most bodies are one to three short paragraphs, wrapped at 72 columns.

Before:

```
Fix parser bug — the parser was incorrectly handling tabs which meant that messages with tabs would fail. This commit addresses that by updating the whitespace handling logic to be more robust.
```

After:

```
Accept tabs between MSG fields

The spec allows one or more spaces or tabs between
fields. The parser accepted spaces only.
```
