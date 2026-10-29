# Checks

One command runs every check. `cargo xtask check` is the entry point. The `xtask` crate in the workspace holds it, and it is not published. The pre-commit hook and CI run the same command, and CI adds `--full`, so the three cannot drift. The hook runs it on every commit, so an agent does not run it by hand before one. While working, an agent runs the tests of the crate it changes with `cargo test -p <crate>`. That takes seconds where the check takes minutes after a change.

This file is a copy from embedded-devkit. Edit it there, then run `cargo xtask sync` in the devkit.

| Check | Command |
|---|---|
| Format | `cargo fmt --all --check` |
| Lints | `cargo clippy --workspace --all-targets -- -D warnings`, with `--all-features` and then with `--no-default-features` |
| Tests | `cargo test --workspace`, with `--all-features` and then with `--no-default-features`, so a test behind `cfg(not(feature))` runs too |
| Docs | `cargo doc --workspace --no-deps` with `RUSTDOCFLAGS="-D warnings"` |
| Feature combinations | `cargo hack check --workspace --feature-powerset --depth 2 --no-dev-deps`, with `--full` only |
| MSRV | `cargo +<rust-version> check --workspace --all-features`, with the `rust-version` from the workspace `Cargo.toml` |
| Links | `cargo xtask links` checks relative links, heading anchors, the `docs/README.md` index, [requirement IDs](CONVENTIONS.md#requirements) and ADR structure |
| Prose | `cargo xtask prose` scans markdown, rustdoc and code comments for banned phrases. `WRITING.md` states what it catches |

`AGENTS.md` lists the checks and commands that one repo adds.

- `xtask/src/shared/toolchain.rs` names the stable toolchain and the nightly that the fuzzer and `-Zbuild-std` builds use. `rust-toolchain.toml` and every CI job install those two, and `cargo xtask check` fails when they differ. Both move together, each to its latest, when a new stable is released.
- `cargo xtask check --full` runs the lints and tests over the feature powerset at `--depth 2` instead. That covers each feature alone and each pair. CI runs it on every push. The nightly CI job runs the full powerset.
- `cargo xtask` runs from the repo root. A firmware folder sets a microcontroller target, so Cargo there would try to build `xtask` for the microcontroller.
- `cargo xtask mutants` runs cargo-mutants over a diff, with every feature on, so a test behind a feature runs. `--staged` takes the staged changes against `HEAD`. Without it, the diff is the working tree against `--base`, and `--base` defaults to `HEAD`. It builds and tests a few mutants at once, the cores divided by three and at most three, as the cargo-mutants book advises, and prints the count. It is not part of `cargo xtask check`, because it is slow. The hook runs it after the check. The full run over every crate stays a manual command and a nightly job. `.cargo/mutants.toml` excludes the files that only start other programs, and the mutants that can only time out, and says why.
- `cargo xtask install-hooks` writes two hooks. The pre-commit hook runs `cargo xtask check`, then `cargo xtask mutants --staged`, then `cargo xtask prose` again so its warnings come last. The commit-msg hook runs `cargo xtask prose --message` and `cargo xtask links --message` over the message.
- Both hook checks build the working tree, not the index. Keep files that build free of unstaged changes until the commit finishes, or the hook checks code the commit does not hold.
- `cargo xtask check` skips a group of checks that the commit's changes do not reach. Each repo's `check.rs` names its groups and the paths each one builds from. The shared `checks` module names the paths that reach every group. Within the root workspace, it lints and tests only the crates the change reaches and the crates that depend on them. A change made only of markdown runs the link and prose checks, and a group that names one of its paths. A tree without changes, or `--full`, runs every check.
- `cargo xtask check` runs `cargo sweep --time 7` over the repo's target folders at most once a day. It removes build files that no build has used in 7 days. Without cargo-sweep installed, it prints how to install it and goes on.
- The rule files and `xtask/src/shared/` are copies from embedded-devkit. The shared folder holds the `links`, `prose`, `install-hooks`, `mutants` and `renumber` commands. Edit them in the devkit and commit there. Then run `cargo xtask sync <repo>` in the devkit, and commit the copy in the repo with the devkit commit in the subject.
- `cargo xtask sync` also installs the git hooks in each repo it syncs, so a change to a hook reaches every repo with the copy. A folder that is not a git repository yet gets the files and not the hooks.
