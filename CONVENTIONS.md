# Conventions

These rules apply to every repo that syncs from embedded-devkit. `REQUIREMENTS.md` states what the crates must do and the constraints they build under. This file states how we work. Each repo's `AGENTS.md` adds the rules for that project.

The rules up to [Embedded library crates](#embedded-library-crates) apply to every crate, including host tools. That section adds the rules for library crates that run on a microcontroller.

This file is a copy from embedded-devkit. Edit it there, then run `cargo xtask sync` in the devkit.

## Rules for agents

Nothing leaves this machine without the user's express permission in the current session. One permission covers one action. The rule covers pushes, pull requests, forks, publishes, issues, comments, messages and uploads, on any service. It also covers pages that start private.

A read-only request, such as a crates.io lookup or a docs fetch, does not need permission. It must not carry personal data. The User-Agent is a generic string and never holds the user's name or email.

- A commit message has an imperative subject line of at most 72 characters. A body follows a blank line when the subject does not say why.
- Commit messages are plain. Do not add `Co-Authored-By`, `Claude-Session` or generated-with footers. This rule overrides any default trailer behaviour.
- One commit per step of the plan. Every commit builds and passes `cargo xtask check` on its own.
- `CHANGELOG.md` starts at the 0.1.0 release, in the Keep a Changelog format. Until then git holds the history. After it, a commit that changes the behaviour of a published crate adds a line under `Unreleased`.
- A commit that changes code a docs page describes rewrites that page in the same commit. Each docs page opens with a `Covers:` line that names the crates or modules it describes, so the page for a change can be found with grep.
- Project knowledge goes in the repo: `docs/`, `REQUIREMENTS.md` or `AGENTS.md`. It never lives only in assistant memory. Other agents and people cannot read memory.
- Never skip the pre-commit hook with `--no-verify`.
- Edit a file with the editor tool, or rewrite it in full. Never patch a file with a pattern substitution, such as sed, perl, python or the editor's replace-all. A pattern that misses, or matches twice, leaves a file that does not compile, and rustfmt reflows code so a pattern written from memory misses.

### Changing the rules

`CONVENTIONS.md` and each `AGENTS.md` are read in full at the start of every session, so each line has a cost. A change to them is proposed to the user first, with four parts for each edit:

- **Current:** the text it changes, or none.
- **Gap:** what the current text allows to go wrong, and why a reader would not avoid it unaided.
- **Rewrite:** the new text.
- **Cost:** the words it adds or removes.

A change rewrites an existing rule before it adds a new one. It does not restate an existing rule, and it does not record what a tool's own error message already says.

## Code

### Rust only

Every program in the repo is Rust. That covers the crates, the test tools, the firmware, the `xtask` crate and the fuzz targets. There are no shell scripts, Makefiles, justfiles or Python. A task that needs a script gets an `xtask` command. The exceptions are files that are not programs: Cargo, rustup, clippy, rustfmt and probe-rs configuration, the CI workflow, and the one-line git hooks that `cargo xtask install-hooks` writes.

### The minimum that does the job

- Implement what the task asks for. Do not add an abstraction, configuration option or extension point that the task did not ask for.
- A test asserts a specific value, never only that the code ran.
- If a change adds two hundred lines and fifty would work, rewrite it before committing.
- A boundary is the exception. Bytes from outside the program, such as the network or flash, are checked defensively. That is correctness, not over-engineering.

### Every changed line traces to the task

- Do not improve nearby code, comments or formatting while doing other work.
- Do not refactor what is not broken.
- Match the file's style, even where you would write it differently. A style change is its own commit.
- Dead code in a file you did not otherwise touch is named in the retrospective, not deleted.
- If your change leaves an import, a local variable or a helper unused, remove it in the same commit.

A smell in a function or struct that the task requires you to change is fixed in the same commit. Nearby code that the task does not change is left alone. The test is whether a reviewer would understand why the cleanup is in this commit.

### Comments say why, never what

Every public item has a doc comment. It states what the item does, its contract and its errors. The rules below apply to every other comment.

Write the code without comments. Then read it as a stranger would, and add a comment only where the stranger would stop and ask why. Before writing one, try a rename, an extracted helper or a new type. Comment last.

A comment is needed when it does one of these:

- names an invariant the reader must keep
- says why the obvious approach was rejected
- records a workaround for a named dependency or hardware quirk
- cites an ADR
- explains a rule that links distant lines of code, such as a call order

Delete a comment that restates the next line, paraphrases the signature, names the task or diff that added it, or divides a file into sections without a reason.

A code comment does not name an issue. `// see #12` does not explain anything once the issue closes. Name the invariant and, where one exists, the ADR.

## Requirements

`REQUIREMENTS.md` states each requirement under an ID, such as `C-6`. The letter names the section. A new requirement takes the next free number for its letter.

- `REQUIREMENTS.md` reads as a specification that a test house could test against. One ID states one behaviour that a test or a recorded sample can fail on, in a sentence or two. It gives no reason. The reason and the rejected options live in the ADR for that mechanism.
- An ID is never reused, even after its requirement is removed.
- The document orders requirements by topic. A new requirement sits beside the ones it relates to, so the numbers can appear out of order.
- IDs are renumbered once, just before the first release, so they read in order from then on. One commit reorders `REQUIREMENTS.md` by hand. It changes no ID, except that a requirement that moves to another section takes the next free number of its new letter. The next commit runs `cargo xtask renumber`. It numbers each letter's IDs from 1 in the order the file defines them, rewrites every citation, and prints the mapping for the commit message. After the first release, an ID never changes.

`cargo xtask links` fails when markdown or a Rust comment cites an ID that `REQUIREMENTS.md` does not define, or when `REQUIREMENTS.md` defines an ID twice. A definition is the bold ID, such as `**C-6**`. Only the letters that `REQUIREMENTS.md` defines IDs with count, so `AES-256` and `ADR-0001` are not IDs. The commit-msg hook runs the same check over the message. The check cannot tell a right ID from a wrong one that exists. The check does not look for IDs in `CONVENTIONS.md`, `xtask/src/shared/` or markdown code blocks.

## Testing

### Where a test goes

- Unit and integration tests run on the host. Hardware tests run on the boards in `REQUIREMENTS.md`.
- Pure logic without collaborators is tested inline in `src/`, in a `#[cfg(test)]` module.
- A crate's public API is tested in `tests/`. An integration test reaches only `pub` items, so it tests the interface and not the internals.
- In the root workspace, a crate's own tests must cover its code. cargo-mutants runs only the tests of the crate it mutates, so a test in another crate, such as an integration-test crate, does not count as coverage. Board crates and test firmware outside the root workspace rely on their on-target tests instead.
- Behaviour across components is tested in `tests/` with the real collaborators wired together, over in-memory fakes at the edge, such as a buffer for the network or a mock flash.
- An internal helper that the integration tests exercise does not get its own test. A test tied to internal structure makes refactoring harder and does not add coverage.
- `REQUIREMENTS.md` states which property tests every parser needs. Property tests elsewhere are encouraged and not required.

### Test first

New, changed and bug-fixing code is written test first. A change to docs, comments or anything that does not run does not need a failing test.

1. Write a failing test.
2. Run it and confirm it fails when run, on an assertion or on the stub's panic. A test that does not compile has not failed.
3. Implement until it passes.

A test for a new item needs the item to exist. Stub it with `todo!()`. The stub is replaced before the commit, and the `clippy::todo` lint rejects one that is left behind.

A change to a public signature, a type, a trait impl, a field's visibility or a module boundary is a change. A rename inside a crate is not.

### What a test must be able to catch

The compiler already checks that a variant exists, that a match is exhaustive, that a signature fits and that ownership holds. A test that passes whenever the build passes does not check anything. Before committing a test, ask whether it would fail under a wrong implementation that still compiles. If it would not, delete it.

These three kinds of test do not check anything:

- an exhaustive `match` mapped to placeholder values, with one `assert_eq!` against a placeholder
- a body of `let _ = Enum::Variant(payload)`
- a test added to keep an import in use

The compiler cannot tell the right value from the wrong one in these cases, so each needs a test:

- which of two same-typed variants a field is set to
- a parser or reader reporting incomplete, corrupt or future-version input
- a validator refusing an integer out of range
- a length or offset field in a frame or header

### Fakes

A fake that stands in for a trait models the trait's documented contract, not the behaviour of one implementation. It can produce every behaviour the contract allows that the code under test must handle. For an I/O trait these include short reads and writes, a write that only buffers until `flush`, the errors the docs say can be retried, end of stream, and an async call that stays pending. For a flash trait they include the write and erase granularity, the erased value, and a write that stops part way.

A fake that is also checked against one real implementation is still held to the trait's contract. The real implementation shows one of the behaviours the contract allows.

### Formats

A round trip still passes when the encoder and the parser change together. A wire or storage format needs other checks too. `REQUIREMENTS.md` states what the recorded samples or images cover, and `AGENTS.md` states how the repo records them.

- Every crash the fuzzer finds becomes a regression test in the same commit as the fix.
- A repo with fuzz targets runs them for a minute before the last code commit of a piece of work, and before any commit that changes a parser. `AGENTS.md` names the command and the files that count. A crash that an earlier commit of the work introduced is then found at its end, and the nightly job runs the fuzzer for longer.
- A property test failure becomes a named test using the shrunk value. The `.proptest-regressions` files are then not required and are not committed.

### Mutation check

Every commit runs cargo-mutants over the lines it changes. The pre-commit hook runs `cargo xtask mutants --staged`, which tests the staged diff against `HEAD`. `cargo xtask mutants --base <rev>` tests the working tree against a commit, and covers a range of commits. A surviving mutant means that the tests do not check that mechanism. Tighten the test, or skip the mutant with a comment that says why it does not change behaviour.

A mutant that cargo-mutants reports as a timeout is a surviving mutant. Its code loops for ever, or for far longer than the test allows, without the mutated line, and the test hangs instead of failing. Change the test first. A fake that a loop waits on gives up after a bounded number of calls, with an error the code does not expect. The mutated loop then fails on an assertion within seconds. Exclude the mutant in `.cargo/mutants.toml` only when the mutated line is the loop's only exit, so no test can end the loop without it. A `run` that loops until `step` fails, with `step` mutated to always succeed, is such a case. A loop that waits on time or on a collaborator is not, even when the fake does not exist yet. The exclusion names the one mutant and its comment says why.

The hook mutates only the lines a commit changes. A change can make an unchanged line dead, for example by moving a decision it made elsewhere. When a commit changes what other lines in a file mean, run `cargo mutants -f <file> --all-features` on that file before committing, with the `--jobs` that `cargo xtask mutants` uses. A commit that only adds or edits lines whose meaning stays in place needs the hook's run and nothing more.

`REQUIREMENTS.md` states what CI runs.

## Checks

One command runs every check. `cargo xtask check` is the entry point. The `xtask` crate in the workspace holds it, and it is not published. The pre-commit hook and CI run the same command, and CI adds `--full`, so the three cannot drift. The hook runs it on every commit, so an agent does not run it by hand before one. While working, an agent runs the tests of the crate it changes with `cargo test -p <crate>`, which takes seconds where the check takes minutes after a change.

| Check | Command |
|---|---|
| Format | `cargo fmt --all --check` |
| Lints | `cargo clippy --workspace --all-targets -- -D warnings`, with `--all-features` and then with `--no-default-features` |
| Tests | `cargo test --workspace`, with `--all-features` and then with `--no-default-features`, so a test behind `cfg(not(feature))` runs too |
| Docs | `cargo doc --workspace --no-deps` with `RUSTDOCFLAGS="-D warnings"` |
| Feature combinations | `cargo hack check --workspace --feature-powerset --depth 2 --no-dev-deps`, with `--full` only |
| MSRV | `cargo +<rust-version> check --workspace --all-features`, with the `rust-version` from the workspace `Cargo.toml` |
| Links | `cargo xtask links` checks relative links, heading anchors, the `docs/README.md` index, [requirement IDs](#requirements) and ADR structure |
| Prose | `cargo xtask prose` scans markdown, rustdoc and code comments for banned phrases |

`AGENTS.md` lists the checks and commands that one repo adds.

- `xtask/src/shared/toolchain.rs` names the stable toolchain and the nightly that the fuzzer and `-Zbuild-std` builds use. `rust-toolchain.toml` and every CI job install those two, and `cargo xtask check` fails when they differ. Both move together, each to its latest, when a new stable is released.
- `cargo xtask check --full` runs the lints and tests over the feature powerset at `--depth 2` instead, which covers each feature alone and each pair. CI runs it on every push. The nightly CI job runs the full powerset.
- `cargo xtask` runs from the repo root. A firmware folder sets a microcontroller target, so Cargo there would try to build `xtask` for the microcontroller.
- `cargo xtask mutants` runs cargo-mutants over a diff, with every feature on, so a test behind a feature runs. `--staged` takes the staged changes against `HEAD`. Without it, the diff is the working tree against `--base`, which defaults to `HEAD`. It builds and tests a few mutants at once, the cores divided by three and at most three, as the cargo-mutants book advises, and prints the count. It is not part of `cargo xtask check`, because it is slow. The hook runs it after the check. The full run over every crate stays a manual command and a nightly job. `.cargo/mutants.toml` excludes the files that only start other programs, and the mutants that can only time out, and says why.
- `cargo xtask install-hooks` writes two hooks. The pre-commit hook runs `cargo xtask check`, then `cargo xtask mutants --staged`, then `cargo xtask prose` again so its warnings come last. The commit-msg hook runs `cargo xtask prose --message` and `cargo xtask links --message` over the message.
- Both hook checks build the working tree, not the index. Keep files that build free of unstaged changes until the commit finishes, or the hook checks code the commit does not hold.
- `cargo xtask check` skips a group of checks that the commit's changes do not reach. Each repo's `check.rs` names its groups and the paths each one builds from. The shared `checks` module names the paths that reach every group. Within the root workspace, it lints and tests only the crates the change reaches and the crates that depend on them. A change made only of markdown runs the link and prose checks, and a group that names one of its paths. A tree without changes, or `--full`, runs every check.
- `cargo xtask check` runs `cargo sweep --time 7` over the repo's target folders at most once a day. It removes build files that no build has used in 7 days. Without cargo-sweep installed, it prints how to install it and goes on.
- `CONVENTIONS.md` and `xtask/src/shared/` are copies from embedded-devkit. The shared folder holds the `links`, `prose`, `install-hooks`, `mutants` and `renumber` commands. Edit them in the devkit and commit there. Then run `cargo xtask sync <repo>` in the devkit, and commit the copy in the repo with the devkit commit in the subject.
- `cargo xtask sync` also installs the git hooks in each repo it syncs, so a change to a hook reaches every repo with the copy. A folder that is not a git repository yet gets the files and not the hooks.

## Working in a session

### Say the plan before starting

Before starting a piece of work, write a short plan in the session and wait for the user's go-ahead. The plan is a summary, not a full design. It fits on one screen. It covers:

- what will change, by crate or file
- any requirement that has two meanings, and the meaning taken
- any design choice the work makes, with the options and a recommendation
- every new public name, in a table with a one-line reason for each
- any `docs/todo.md` entry for the files the work touches, and whether the work takes it or leaves it
- the entity and concept pages read for the things the work touches
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

`AGENTS.md` can add decisions to this list. The test is whether the user would give an opinion if they were watching. Do not assume the user agrees because the answer seems obvious. Ask with the options and a recommendation, in one message.

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

## Docs

### Page types

`docs/` has five directories. Every page opens with a `Covers:` line that names what it describes.

- `entities/` holds the chips, boards, libraries and upstream projects the crates build against: what each is, what it does and what it does not do. A versioned entity has a section per version we pinned or tested.
- `concepts/` explains how a mechanism works, independent of any one entity. A term used in a requirement, an ADR or a doc comment links its concept page, and that page is the only place that defines it.
- `comparisons/` compares entities against stated criteria. The criteria come first. A number comes from a named xtask command or test.
- `summaries/` holds the claims this repo takes from one read source, each with the source section and the date read. A summary holds only claims that another page cites.
- `decisions/` holds the ADRs.

A byte layout that a recorded sample or image tests is a requirement. Other implementation detail, such as a timing table or a buffer layout, lives in module-level rustdoc beside the code. An ADR or a page links it.

- A page other than an ADR exists only when a requirement, an ADR or another page links it. The index in `docs/README.md` does not count. An ADR records a decision, so the index is enough for it.
- A page lists observed facts and read facts under separate headings. Every fact comes from a public source or from our own observation on our own boards. An observed fact points to the test, the recorded sample or the image. A read fact points to the summary page of the document it comes from. A fact read from source code links the file at the commit it was read from, and does not need a summary page.
- `docs/README.md` indexes every page, and a new page gets its row in the same commit. A link between files in the repo is relative, never an absolute GitHub URL, so the link check can resolve it and it survives a fork or a rename.

### ADRs

An ADR records a decision about the product, or about how it is built, tested and shipped. The development process and the docs conventions live in this file and in `AGENTS.md` instead.

One ADR covers one mechanism, such as reconnect or factory reset, and every decision inside it. It is written when the step that builds the mechanism is done and reviewed. Until then the plan holds the step's decisions.

A choice belongs in an ADR when a competent newcomer would propose the option it rejected. A parameter, a name, an error variant or a choice without a second option does not. A Summary bullet states the rule in one sentence, and what the rule makes happen in a second. It cites the requirement IDs it meets, and leaves rejected options to Alternatives. It does not restate a requirement, a number, a signature or a default.

ADRs live at `docs/decisions/NNNN-slug.md`. Each opens with `# ADR-NNNN: <the mechanism>`, such as `# ADR-0004: Reconnect`, and a bold status line: `**Status: accepted**`, `**Status: proposed**` or `**Status: superseded by ADR-NNNN**`. A `## Summary` section follows, with one bullet per decision as it stands. The Context, Decision, Alternatives and Consequences follow the Summary. A new ADR gets its row in the index in `docs/README.md` in the same commit.

- The Context states the problem in enough detail that a newcomer could judge the decision from it. Where a newcomer's first answer does not work here, it says why, without calling that answer obvious.
- An ADR states a rule. It doesn't state a list of cases, as they will go stale.
- Implementation detail lives in rustdoc, and a format lives in `REQUIREMENTS.md`. The ADR links or cites it.
- Until the first release, an ADR is edited in place to state the current decision, without a History bullet or a new ADR. The commit message says what changed.
- After the first release, a change that keeps every Summary bullet is edited into the ADR. It adds a dated bullet to a `## History` section: `YYYY-MM-DD: Amended: <what changed>.`
- After the first release, a change that reverses a Summary bullet is a new ADR. The new ADR says what it supersedes, and the old ADR's status becomes `superseded by ADR-NNNN`.
- A wording fix or a renamed path does not need a History bullet.

### `docs/todo.md`

`docs/todo.md` holds findings that a piece of work saw and did not fix. Each entry is one line: the date, the file or item, the finding, and the short hash of the commit that found it. A plan reads the entries for the files it touches. The commit that fixes a finding removes its line.

## Embedded library crates

These rules add to the ones above for every library crate that runs on a microcontroller. Host tools, `xtask` and crates that build only with `std` do not follow them.

### Embedded rules

`REQUIREMENTS.md` states the constraints under Constraints. Lints enforce them. The workspace `Cargo.toml` sets these under `[workspace.lints]`, and every library crate inherits them.

| Rule | Lint |
|---|---|
| No `unsafe` | `unsafe_code = "forbid"` |
| Every public item has a doc comment | `missing_docs = "deny"` |
| No `unwrap` or `expect` | `clippy::unwrap_used`, `clippy::expect_used` |
| No `panic!`, `todo!` or `unimplemented!` | `clippy::panic`, `clippy::todo`, `clippy::unimplemented` |
| No indexing that can go out of bounds | `clippy::indexing_slicing` |
| No arithmetic that can overflow or divide by zero | `clippy::arithmetic_side_effects` |
| No broken link in a doc comment | `rustdoc::broken_intra_doc_links = "deny"` |

- Arithmetic overflow panics in debug builds and on targets that enable overflow checks. Use a checked, saturating or wrapping operation, and choose the one the value needs.
- A 64-bit division on a 32-bit target links about 900 bytes of `compiler_builtins`. Keep hot paths free of it.
- The `thumbv6m-none-eabi` build catches compare-and-swap atomics. A `--no-default-features` build of each core crate catches code that needs a feature.
- The caller supplies time, randomness and key material through the traits in `REQUIREMENTS.md`.
- `REQUIREMENTS.md` lists the dependency crates whose traits may appear in a public API. Any other dependency type in a public API needs an Architectural Decision Record (ADR).
- Board support code lives in the examples and the hardware test firmware. It never enters a library crate.

### Tests in a `no_std` crate

- A library crate declares `#![cfg_attr(not(test), no_std)]`, so its inline tests build with `std` on the host. The cross-target builds check that the crates compile.
- The panic lints exist to keep panics out of library code, and a test panics on purpose. `clippy.toml` allows them inside `#[cfg(test)]` modules. A file under `tests/` is not inside one, so it opens with `#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing, clippy::arithmetic_side_effects)]`. `clippy.toml` has no test option for `arithmetic_side_effects`, so an inline test module opens with `#[allow(clippy::arithmetic_side_effects)]`.

### Embedded checks

| Check | Command |
|---|---|
| Cross targets | `cargo build` of each published `no_std` crate for each target that `REQUIREMENTS.md` marks "Build on every CI push" |
| Code size | `cargo xtask size` builds each reference firmware and reports flash and static RAM. It compares them with the budget file that `REQUIREMENTS.md` names, once the budget is set. |

A repo runs the code size check once it has firmware.

### Firmware and upstream patches

- Each board's test firmware is its own workspace under `firmware/<board>/`. The library workspace excludes it.
- A firmware redirects an upstream crate with a `[patch]` section in its own `Cargo.toml`, because Cargo applies `[patch]` only from the root of the workspace it builds.
- A patch for an upstream change that is not merged is pinned to a commit. The upstream crate's entity page names the pull request that the patch waits for. When upstream releases the change, the patch goes and the version rises.

## Writing style

This applies to everything written in the repo and to replies in the session. That includes docs pages, READMEs, rustdoc, code comments, issue bodies and commit messages.

Write plain English for an engineer who is new to the repo and reads the page once. Some readers speak English as a second language.

Before writing a docs page, an ADR or a commit message, re-read this section.

### Cut the commentary

The most common fault is a fact followed by a sentence that comments on it. Cut the second sentence. Write only what a reader needs to know. Do not comment on the writing, the document or the reasoning.

- Do not write about the document itself, its purpose or how to read it.
- Do not assess your own writing. Do not call anything honest, clear, "worth noting" or alarming.
- Do not add a verdict to a fact that already implies it.
- Do not use hedging adverbs such as `obviously`, `arguably`, `genuinely`, `plainly` or `frankly`.
- Do not add a "which" clause that restates the clause before it, or that adds a fact the sentence does not need.
- Do not write "worth noting", "note that", "the point is" or "what matters is".
- State the fact, then stop.
- Do not use figurative idiom such as "load bearing", "long pole", "surface area" or "first-class". Technical terms of art are fine.
- Do not restate the question before answering it.
- Do not end a reply with a summary of what it said.
- Do not offer next steps unless asked.
- Do not bold a whole sentence. Bold the first few words of a bullet at most.

When someone says "cut the commentary", remove commentary from the whole draft, not only the quoted phrases.

### Sentences

- Write one fact per sentence. Keep most sentences under 25 words.
- Every sentence has a subject and a verb, in bullets and table cells too. Name who did it: "We measured", "The spec states", "heapless 0.9 fixes". A list may instead continue the sentence that introduces it. Each item then completes that sentence and ends with a full stop.
- Three or more parallel items go in a list, not a sentence.
- Name the thing instead of writing "one", "it" or "that" when its noun is more than a clause back.
- If a sentence needs a semicolon, a dash or more than one "which", split it.
- At most one bracket per sentence, for a date, an ID or a link. A second fact gets its own sentence.
- Use the ordinary word. Write "uses", not "rests on". Write "checked", not "established". Write "fixes", not "addresses".
- Write "has" or "holds" for what a record, message, event or struct contains. Keep "carries" for something that moves: a port carries signals, a count carries across a reconnect.
- Do not use an idiom or a phrasal verb where a plain word exists. Write "continue", not "carry on". Write a new sentence, not "on the other hand".
- Introduce an abbreviation in full on first use, with the short form in brackets: Architectural Decision Record (ADR). Abbreviations every engineer knows, such as API, do not need this.
- Negate the verb, not the noun. Write "does not need" and "doesn't do I/O", not "needs no" and "does no I/O".
- A table cell holds one fact or a short list. Anything longer goes in prose under a heading, and the cell links to it.
- Say what an ID refers to on first mention: "PR #6192, the retry logic".
- Write dates as YYYY-MM-DD.
- A page does not describe its own edit history. Git holds it. An ADR's History section is the one exception.
- Never hard-wrap markdown. Put one line per paragraph, bullet or table row, and soft wrap in the editor.

### The prose check

`cargo xtask prose` scans markdown, rustdoc, code comments and the commit message for banned phrases, and fails when it finds one. It skips code blocks. The full list lives in `xtask/src/shared/prose.rs`. The table states what each group catches.

| Group | Examples |
|---|---|
| Commentary | "worth noting", "note that", "the point is", "what matters is", "importantly" |
| Hedging adverbs | `obviously`, `arguably`, `genuinely`, `plainly`, `frankly`, `simply` |
| Restating clauses | "which means", "which is to say", "in other words" |
| Figurative idiom | "load bearing", "long pole", "surface area", "first-class", "on the other hand", "under the hood" |
| Marketing words | `leverage`, `robust`, `seamless`, `powerful`, `elegant`, `comprehensive` |
| Punctuation | an em dash, an en dash used as a separator, a semicolon in prose |

A banned phrase that a sentence must quote, such as this table, goes in quotes or a code span. The check skips those.

The check also warns on the paragraphs a commit changes and on the commit's message. A warning marks a phrase that is often a fault, such as ", which is" or "carries" for "has". It also marks a word tied to the day of writing, and a sentence over 35 words or with two brackets. On an entity, summary, concept or ADR page, it also marks a sentence that shares most of its words with another on the page. A warning is a place to look, not a rule. `cargo xtask prose --warn-all` warns on the whole tree.

### Examples

A paragraph, before:

```
The cache layer serves reads with no database round trip in the hot path; the framework already ships it; and it costs one config flag and a TTL against the schema migration, query rewrite, deploy-order change needing platform sign-off and backfill that denormalising requires.
```

After:

```
The cache is cheaper to build. It serves reads without a database round trip. It needs one config flag and a TTL. Denormalising needs a schema migration, rewritten queries, deploy order instructions and a backfill.
```

A doc comment, before:

```rust
/// This function is used to parse a MSG operation from the given buffer. It's worth noting that
/// it handles partial input gracefully — returning `Incomplete` if there aren't enough bytes yet.
```

After:

```rust
/// Parses one `MSG` operation from the start of `input`.
///
/// Returns `Incomplete` when `input` ends before the operation does, and consumes nothing.
/// Returns `Corrupt` when the bytes cannot be a `MSG`.
```

A commit message, before:

```
Fix parser bug — the parser was incorrectly handling tabs which meant that messages with tabs would fail. This commit addresses that by updating the whitespace handling logic to be more robust.
```

After:

```
Accept tabs between MSG fields

The spec allows one or more spaces or tabs between fields. The parser accepted spaces only.
```
