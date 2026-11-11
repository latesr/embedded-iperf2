# Code

These rules apply to every crate in a repo that syncs from embedded-devkit, host tools included. `EMBEDDED.md` adds the rules for a library crate that runs on a microcontroller.

This file is a copy from embedded-devkit. Edit it there, then run `cargo xtask sync` in the devkit.

## Rust only

Every program in the repo is Rust: the crates, the test tools, the firmware, the `xtask` crate, and the fuzz targets. There are no shell scripts, Makefiles, justfiles, or Python. A task that needs a script gets an `xtask` command. The exceptions are files that are not programs: the Cargo, rustup, clippy, rustfmt, and probe-rs configuration, the CI workflow, and the one-line git hooks that `cargo xtask install-hooks` writes.

## Four rules

1. **Implement what the task asks for.** Do not add an abstraction, a configuration option, or an extension point that the task did not ask for. If a change adds 200 lines and 50 would work, rewrite it before the commit.
2. **Every changed line traces to the task.** Do not improve nearby code, comments, or formatting while doing other work. Do not refactor what is not broken. Match the file's style. A style change is its own commit. Dead code in a file the task did not otherwise touch is named in the retrospective, not deleted. An import, a local, or a helper that the change leaves unused goes in the same commit.
3. **A test asserts a specific value**, never only that the code ran. `TESTING.md` holds the rest.
4. **A boundary is checked defensively.** Bytes from outside the program, such as the network or flash, are checked. Such a check is correctness, not over-engineering.

A smell in a function or struct that the task requires the change to touch is fixed in the same commit. The test is whether a reviewer would understand why the cleanup is in this commit.

## Rustdoc

These rules add to `WRITING.md`. Every public item has a doc comment, in this order:

1. One sentence in the third person present that says what the item does: "Sets the baud rate." Not "Set the baud rate", and not "This function sets the baud rate." A constructor starts with "Creates a new". A predicate reads "Returns whether sampling is complete." A function that only returns a value can open with what it returns: "The paths that differ from `HEAD`." A constant or static opens with its quantity: "The number of PSRAM words that one seed covers."
2. The contract: what the caller must do before the call, what the item does not do, and what a returned handle is for. The contract uses `must`. It does not state what an attribute on the item states.
3. `# Examples`, `# Panics`, `# Errors`, `# Cancellation Safety`, and `# Safety` sections, in that order, each only when it has a body. A function with one or two failure conditions states them in the contract: "Fails when the tree has changes." More go in `# Errors`, which names the error type and the condition: "`ConfigError` when the baud rate is not supported." It does not repeat the summary.
4. `See <link>.` alone on the last line, for a source the code ports or a document it follows. The link is pinned to a commit or a revision. A second link follows as `and <link>.` on the next line.

A doc comment keeps one point of view. An enum variant's doc is a noun phrase or a short statement, not "This error occurs when". A field's doc is a sentence in the same voice as a summary. Link an item with an intra-doc link, ``[`Type`]``, not a URL.

A program's module doc says what the program checks, as a numbered list when it checks more than two things, and how to run it. The mechanism goes in a `//` comment beside the code it explains.

Before:

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

## Comments say why, never what

These rules add to `WRITING.md`, and apply to every comment that is not rustdoc.

Write the code without comments. Then read it as a stranger would, and add a comment only where the stranger would stop and ask why. Before writing one, try a rename, an extracted helper, or a new type. Comment last.

A comment is needed when it does one of these:

- names an invariant the reader must keep
- says why a plainer form of the code does not work
- says what the code does not do, and the fact that forbids it
- records a workaround for a named dependency or hardware quirk
- cites an ADR
- explains a rule that links distant lines of code, such as a call order

A comment states the rule the code keeps, with `must`: "The MPLL must have power before the rate is set." The reason is a fact about the hardware or the compiler, never what another project does. The constraint comes first, then the code's response: "The bounds check has a panic path in flash, so the code uses `get`." A citation follows Rustdoc rule 4. A fact about one board or one run goes in the pull request. A comment on a configuration setting is one short line.

Delete a comment that restates the next line or an attribute, paraphrases the signature, or names the function it ports. Delete one that names the task or diff that added it, or divides a file into sections without a reason. A comment does not name an issue. `// see #12` explains nothing once the issue closes. Name the invariant and, where one exists, the ADR.

Before:

```rust
// Like esp-idf's `pmu_sleep_finish`, this code powers the MPLL up, then sets its rate.
clocks::set_mpll_power(true);
clocks::set_mpll_frequency(mpll_mhz);
```

After:

```rust
// The MPLL must have power before the rate is set.
clocks::set_mpll_power(true);
clocks::set_mpll_frequency(mpll_mhz);
```
