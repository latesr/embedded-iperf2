# Embedded library crates

These rules add to `CODE.md` and `TESTING.md` for every library crate that runs on a microcontroller. Host tools, `xtask`, and crates that build only with `std` do not follow them.

This file is a copy from embedded-conventions. Edit it there, then run `cargo xtask sync` in embedded-conventions.

## Embedded rules

`REQUIREMENTS.md` states the constraints under Constraints. Lints enforce them. The workspace `Cargo.toml` sets these under `[workspace.lints]`, and every library crate inherits them.

| Rule | Lint |
|---|---|
| No `unsafe` | `unsafe_code = "forbid"` |
| Every public item has a doc comment | `missing_docs = "deny"` |
| No `unwrap` or `expect` | `clippy::unwrap_used`, `clippy::expect_used` |
| No `panic!`, `todo!`, or `unimplemented!` | `clippy::panic`, `clippy::todo`, `clippy::unimplemented` |
| No indexing that can go out of bounds | `clippy::indexing_slicing` |
| No arithmetic that can overflow or divide by zero | `clippy::arithmetic_side_effects` |
| No broken link in a doc comment | `rustdoc::broken_intra_doc_links = "deny"` |

- Arithmetic overflow panics in debug builds and on targets that enable overflow checks. Use a checked, saturating, or wrapping operation, and choose the one the value needs.
- A 64-bit division on a 32-bit target links about 900 bytes of `compiler_builtins`. Keep hot paths free of it.
- The `thumbv6m-none-eabi` build catches compare-and-swap atomics. A `--no-default-features` build of each core crate catches code that needs a feature.
- The caller supplies time, randomness, and key material. A library takes them through a trait in its API, or through a driver that the firmware registers at link time, such as embassy-crypto's `Rng`.
- `REQUIREMENTS.md` lists the dependency crates with traits that can appear in a public API. Any other dependency type in a public API needs an Architectural Decision Record (ADR).
- Board support code lives in the examples and the hardware test firmware. It never enters a library crate.

## Tests in a `no_std` crate

- A library crate declares `#![cfg_attr(not(test), no_std)]`, so its inline tests build with `std` on the host. The cross-target builds check that the crates compile.
- The panic lints exist to keep panics out of library code, and a test panics on purpose. `clippy.toml` allows them inside `#[cfg(test)]` modules. A file under `tests/` is not inside one, so it opens with `#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing, clippy::arithmetic_side_effects)]`. `clippy.toml` does not have a test option for `arithmetic_side_effects`, so an inline test module opens with `#[allow(clippy::arithmetic_side_effects)]`.

## Embedded checks

| Check | Command |
|---|---|
| Cross targets | `cargo build` of each published `no_std` crate for each target that `REQUIREMENTS.md` marks "Build on every CI push" |
| Code size | `cargo xtask size` builds each reference firmware and reports flash and static RAM. It compares them with the budget file that `REQUIREMENTS.md` names, once the budget is set. |

A repo runs the code size check once it has firmware.

## Firmware and upstream changes

- Each board's test firmware is its own workspace under `firmware/<board>/`. The library workspace excludes it.
- An upstream change that is not merged comes from a public fork of the upstream repo, at the commit that the dependency names by `rev`. When upstream releases the change, the `git` and `rev` keys go, and the version rises.
