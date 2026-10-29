# Code

These rules apply to every crate in a repo that syncs from embedded-devkit, host tools included. `EMBEDDED.md` adds the rules for a library crate that runs on a microcontroller.

This file is a copy from embedded-devkit. Edit it there, then run `cargo xtask sync` in the devkit.

## Rust only

Every program in the repo is Rust: the crates, the test tools, the firmware, the `xtask` crate and the fuzz targets. There are no shell scripts, Makefiles, justfiles or Python. A task that needs a script gets an `xtask` command. The exceptions are files that are not programs: Cargo, rustup, clippy, rustfmt and probe-rs configuration, the CI workflow, and the one-line git hooks that `cargo xtask install-hooks` writes.

## Four rules

1. **Implement what the task asks for.** Do not add an abstraction, a configuration option or an extension point that the task did not ask for. If a change adds two hundred lines and fifty would work, rewrite it before the commit.
2. **Every changed line traces to the task.** Do not improve nearby code, comments or formatting while doing other work. Do not refactor what is not broken. Match the file's style. A style change is its own commit. Dead code in a file the task did not otherwise touch is named in the retrospective, not deleted. An import, a local or a helper that the change leaves unused goes in the same commit.
3. **A test asserts a specific value**, never only that the code ran. `TESTING.md` holds the rest.
4. **A boundary is checked defensively.** Bytes from outside the program, such as the network or flash, are checked. Such a check is correctness, not over-engineering.

A smell in a function or struct that the task requires the change to touch is fixed in the same commit. Nearby code that the task does not change is left alone. The test is whether a reviewer would understand why the cleanup is in this commit.

## Comments say why, never what

`WRITING.md` holds the rules for rustdoc. The rules below apply to every other comment.

Write the code without comments. Then read it as a stranger would, and add a comment only where the stranger would stop and ask why. Before writing one, try a rename, an extracted helper or a new type. Comment last.

A comment is needed when it does one of these:

- names an invariant the reader must keep
- says why a plainer form of the code does not work
- says what the code does not do, and the fact that forbids it
- records a workaround for a named dependency or hardware quirk
- cites an ADR
- explains a rule that links distant lines of code, such as a call order

A comment states the rule the code keeps, with `must`: "The MPLL must have power before the rate is set." The reason is a fact about the hardware or the compiler, never what another project does. The constraint comes first, then the code's response: "The bounds check has a panic path in flash, so the code uses `get`." A citation is `See <link>.`, pinned to a commit, alone on the last line. A fact about one chip, board or run goes in the commit or the pull request. A comment on a configuration setting is one short line.

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
