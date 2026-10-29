# Testing

These rules apply to every crate in a repo that syncs from embedded-devkit. `EMBEDDED.md` adds the rules for tests in a `no_std` crate.

This file is a copy from embedded-devkit. Edit it there, then run `cargo xtask sync` in the devkit.

## Where a test goes

- Unit and integration tests run on the host. Hardware tests run on the boards in `REQUIREMENTS.md`.
- Pure logic without collaborators is tested inline in `src/`, in a `#[cfg(test)]` module.
- A crate's public API is tested in `tests/`. An integration test reaches only `pub` items, so it tests the interface and not the internals.
- In the root workspace, a crate's own tests must cover its code. cargo-mutants runs only the tests of the crate it mutates, so a test in another crate, such as an integration-test crate, does not count as coverage. Board crates and test firmware outside the root workspace rely on their on-target tests instead.
- Behaviour across components is tested in `tests/` with the real collaborators wired together, over in-memory fakes at the edge, such as a buffer for the network or a mock flash.
- An internal helper that the integration tests exercise does not get its own test. A test tied to internal structure makes refactoring harder and does not add coverage.
- `REQUIREMENTS.md` states the property tests every parser needs. Property tests elsewhere are encouraged and not required.

## Test first

New, changed and bug-fixing code is written test first. A change to docs, comments or anything that does not run does not need a failing test.

1. Write a failing test.
2. Run it and confirm it fails when run, on an assertion or on the stub's panic. A test that does not compile has not failed.
3. Implement until it passes.

A test for a new item needs the item to exist. Stub it with `todo!()`. The stub is replaced before the commit, and the `clippy::todo` lint rejects one that is left behind.

A change to a public signature, a type, a trait impl, a field's visibility or a module boundary is a change. A rename inside a crate is not.

## What a test must be able to catch

The compiler already checks that a variant exists, that a match is exhaustive, that a signature fits and that ownership holds. A test that passes whenever the build passes does not check anything. Before committing a test, ask whether it would fail under a wrong implementation that still compiles. If it would not, delete it.

These three kinds of test do not check anything:

- an exhaustive `match` mapped to placeholder values, with one `assert_eq!` against a placeholder
- a body of `let _ = Enum::Variant(payload)`
- a test added to keep an import in use

The compiler cannot tell the right value from the wrong one in these cases, so each needs a test:

- the variant a field is set to, when two variants have the same type
- a parser or reader reporting incomplete, corrupt or future-version input
- a validator refusing an integer out of range
- a length or offset field in a frame or header

## Fakes

A fake that stands in for a trait models the trait's documented contract, not the behaviour of one implementation. It can produce every behaviour the contract allows that the code under test must handle. For an I/O trait these include short reads and writes, a write that only buffers until `flush`, the errors the docs say can be retried, end of stream, and an async call that stays pending. For a flash trait they include the write and erase granularity, the erased value, and a write that stops part way.

A fake that is also checked against one real implementation is still held to the trait's contract. The real implementation shows one of the behaviours the contract allows.

## Formats

A round trip still passes when the encoder and the parser change together. A wire or storage format needs other checks too. `REQUIREMENTS.md` states what the recorded samples or images cover, and `AGENTS.md` states how the repo records them.

- Every crash the fuzzer finds becomes a regression test in the same commit as the fix.
- A repo with fuzz targets runs them for a minute before the last code commit of a piece of work, and before any commit that changes a parser. `AGENTS.md` names the command and the files that count. A crash that an earlier commit of the work introduced is then found at its end, and the nightly job runs the fuzzer for longer.
- A property test failure becomes a named test using the shrunk value. The `.proptest-regressions` files are then not required and are not committed.

## Mutation check

Every commit runs cargo-mutants over the lines it changes. The pre-commit hook runs `cargo xtask mutants --staged`, and that command tests the staged diff against `HEAD`. `cargo xtask mutants --base <rev>` tests the working tree against a commit, and covers a range of commits. A surviving mutant means that the tests do not check that mechanism. Tighten the test, or skip the mutant with a comment that says why it does not change behaviour.

A mutant that cargo-mutants reports as a timeout is a surviving mutant. Its code loops for ever, or for far longer than the test allows, without the mutated line, and the test hangs instead of failing. Change the test first. A fake that a loop waits on gives up after a bounded number of calls, with an error the code does not expect. The mutated loop then fails on an assertion within seconds. Exclude the mutant in `.cargo/mutants.toml` only when the mutated line is the loop's only exit, so no test can end the loop without it. A `run` that loops until `step` fails, with `step` mutated to always succeed, is such a case. A loop that waits on time or on a collaborator is not, even when the fake does not exist yet. The exclusion names the one mutant and its comment says why.

The hook mutates only the lines a commit changes. A change can make an unchanged line dead, for example by moving a decision it made elsewhere. When a commit changes what other lines in a file mean, run `cargo mutants -f <file> --all-features` on that file before committing, with the `--jobs` that `cargo xtask mutants` uses. A commit that only adds or edits lines with a meaning that stays in place needs the hook's run and nothing more.

`REQUIREMENTS.md` states what CI runs.
