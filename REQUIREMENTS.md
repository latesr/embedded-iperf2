# Requirements

embedded-iperf2 is an iperf 2 client and server for microcontrollers, wire compatible with iperf 2.2.2.

## Terms

## Why another one?

## Goals

## Design principles

## Out of scope for v1

## Crates

## Security

## Platforms

### CI targets

| Target | Checks |
|---|---|
| x86_64 and aarch64 hosts | All tests, on every CI push |
| `riscv32imac-unknown-none-elf` | Build on every CI push |
| `thumbv7em-none-eabihf` | Build on every CI push |
| `thumbv6m-none-eabi` | Build on every CI push |

### Reference boards

## Constraints

- The crates build on stable Rust. The minimum supported Rust version (MSRV) is 1.91, with edition 2024.

`CONVENTIONS.md` names the lints that enforce the code constraints.

## Testing

Development is test first. `CONVENTIONS.md` states how a developer writes and checks tests. This section states what the test suite must contain and what CI runs.

### Unit and property tests

### Fuzzing

### Hardware tests

### Continuous integration

- CI runs `cargo xtask check` on every push. `CONVENTIONS.md` and `AGENTS.md` list what it runs.
- CI runs cargo-mutants over the changed lines on every push.
- A full cargo-mutants run covers every crate nightly and before each release.
- The nightly job checks every feature combination of every published crate with `cargo hack`. `cargo xtask check` covers pairs of features.

## Licence

Users may choose either the MIT licence or the Apache-2.0 licence. The Cargo `license` field is `MIT OR Apache-2.0`. Contributions are licensed on the same terms.

## Open questions
