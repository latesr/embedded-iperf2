# Checks

`cargo xtask check` runs the quick checks over what a change reaches, and the pre-commit hook runs it. `cargo xtask check --push` runs the quick and the slow checks over what the branch changed, and the pre-push hook runs it. `--full` runs every check on every crate, and CI runs it on every push and pull request. To save time while working, an agent runs the tests of the crate it changes with `cargo test -p <crate>`. CI also runs the mutation check over what each push or pull request changes, and over every line each night that `main` has changed. Every line is also checked before each release.

This file is a copy from embedded-conventions. Edit it there, then run `cargo xtask sync` in embedded-conventions.

| Check | Runs in | Command |
|---|---|---|
| Format | `check` | `cargo fmt --all --check` |
| Lints | `check` | `cargo clippy --all-targets -- -D warnings` over the crates the change reaches, with every feature and then with none. `--full` adds each feature alone and each pair |
| Tests | `check` | `cargo test` over the crates the change reaches, with every feature and then with none, so a test behind `cfg(not(feature))` runs too. `--full` adds each feature alone and each pair |
| Docs | `check` | `cargo doc --workspace --no-deps` with `RUSTDOCFLAGS="-D warnings"` |
| Links | `check` | `cargo xtask links` checks relative links, heading anchors, the `docs/README.md` index, [requirement IDs](#requirement-ids), and ADR structure |
| Prose | `check` | `cargo xtask prose` scans markdown, rustdoc, and code comments for banned phrases, as [the prose check](#the-prose-check) states |
| MSRV | `--push` | `cargo +<rust-version> check --all-features`, at each crate's `rust-version` |
| Cross targets | `--push` | The builds of the root workspace's crates for a microcontroller target |
| Other workspaces | `check` and `--push` | The format, lints, and builds of each workspace that `AGENTS.md` lists, when the change reaches it. Its docs and MSRV check run in `--push` |
| Feature combinations | `--full` | `cargo hack check --feature-powerset --depth 2`. The nightly CI job runs the full powerset |
| Mutation | pre-push hook | `cargo xtask mutants --push`, as [the mutation check](TESTING.md#mutation-check) states |

`AGENTS.md` lists the checks and commands that one repo adds.

- `cargo xtask` runs from the repo root. A firmware folder sets a microcontroller target, so Cargo there would try to build `xtask` for the microcontroller.
- `cargo xtask check` skips a group of checks that the change does not reach. It lints and tests only the crates the change reaches and the crates that depend on them. Each repo's `check.rs` names its groups and the paths each one builds from. `--push` takes the changes since the branch's merge base with its upstream, or else with `origin/HEAD`. A tree without changes, a branch without either base, or `--full` checks every crate.
- `xtask/src/shared/toolchain.rs` names the stable toolchain and the nightly. `cargo xtask check` fails when `rust-toolchain.toml` differs from it. Both move together, each to its latest, when a new stable is released.
- `cargo xtask mutants` runs cargo-mutants over a diff, once for each package the diff touches, with every feature on. `--all` runs it over every line of every package. `--push` takes the branch's base as `check --push` does, and fails without one. Before a repo's first push, run `cargo xtask check --push` and `cargo xtask mutants --base <commit>` by hand, with the last commit that passed the mutation check. Then the user pushes once with `--no-verify`, because the hook cannot find a base.
- `.cargo/mutants.toml` lists the code that cargo-mutants excludes for this repo, with a comment on each entry that says why. To exclude a function in `xtask/src/shared/`, which the repos share, mark it in the source with `#[mutants::skip]` and a comment that says why. `cargo xtask sync` then copies the skip to each repo with the function.
- `cargo xtask install-hooks` writes three hooks. The pre-commit hook runs `cargo xtask check`, then `cargo xtask prose` again so that its warnings print last. The pre-push hook runs `cargo xtask check --push`, then `cargo xtask mutants --push`. When a repo's `cargo xtask check` does not take `--push`, it runs every check, and the pre-push hook runs it in place of `--push`. The pre-push hook checks the current branch, not the refs that the push names. The commit-msg hook runs the prose and links checks over the message. `cargo xtask sync` installs the same hooks in each repo it syncs.
- The hooks build the working tree, not the index. Keep files that build free of unstaged changes until the commit or the push finishes, or the hook checks code that the commit or the push does not hold.
- Once a day, `cargo xtask check` runs `cargo sweep --time 7`, which removes build files that no build used in 7 days.

## The prose check

`cargo xtask prose` fails when it finds a banned phrase. It skips code blocks. The full list is in `xtask/src/shared/prose.rs`. The table states what each group catches.

| Group | Examples |
|---|---|
| Commentary | "worth noting", "note that", "the point is", "what matters is", "importantly" |
| Hedging adverbs | `obviously`, `arguably`, `genuinely`, `plainly`, `frankly`, `simply` |
| Restating clauses | "which means", "which is to say", "in other words" |
| Figurative idiom | "load bearing", "long pole", "surface area", "first-class", "on the other hand", "under the hood" |
| Marketing words | `leverage`, `robust`, `seamless`, `powerful`, `elegant`, `comprehensive` |
| Punctuation | an em dash, an en dash used as a separator, a semicolon in prose |

A banned phrase that a sentence must quote, such as this table, goes in quotes or a code span. The check skips those.

The check also warns on the paragraphs a commit changes and on the commit's message. A warning marks a phrase that is often a fault, such as ", which is", "carries" for "has", a citation inside a sentence, or "this code". It marks three words, "whose", "unless", and "itself", for which STE writes "of which", "if not", and the noun. It also marks a word tied to the day of writing, and a sentence over 35 words or with two brackets.

On an ADR, it also marks a sentence that shares most of its words with another on the page. A warning is a place to look, not a rule. `cargo xtask prose --warn-all` warns on the whole tree, and `cargo xtask prose --root <path>` checks another tree, such as a worktree of an upstream project.

## Requirement IDs

`cargo xtask links` fails when markdown or a Rust comment cites an ID that `REQUIREMENTS.md` does not define, or when `REQUIREMENTS.md` defines an ID twice. A definition is the bold ID, such as `**C-6**`. Only the letters that `REQUIREMENTS.md` defines IDs with count, so `AES-256` and `ADR-0001` are not IDs. The commit-msg hook runs the same check over the message. The check cannot tell a right ID from a wrong one that exists. The check does not look for IDs in the rule files, `xtask/src/shared/`, or markdown code blocks. `cargo xtask links` also lists each ID that no `@verify` comment names, and `cargo xtask check` prints their count. Once a repo's list is empty, its `check.rs` fails the check when an ID joins the list.

IDs are renumbered once, just before the first release, so they read in order from then on. One commit reorders `REQUIREMENTS.md` by hand. It changes no ID, except that a requirement that moves to another section takes the next free number of its new letter. The next commit runs `cargo xtask renumber`. It numbers each letter's IDs from 1 in the order the file defines them, rewrites every citation, and prints the mapping for the commit message. After the first release, an ID never changes.
