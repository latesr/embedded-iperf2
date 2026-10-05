@CONVENTIONS.md

# embedded-iperf2

Read `CONVENTIONS.md` first. It holds the rules shared by every repo that syncs from embedded-devkit. This file holds the rules for this repo only.

embedded-iperf2 is an iperf 2 client and server for microcontrollers, wire compatible with iperf 2.2.2.

## The wire format

`CONVENTIONS.md` states the rules for every format under Formats. These rules add to them. `REQUIREMENTS.md` states what the samples cover.

- A sample is recorded from a stock iperf at the pinned version. It is never written by hand to match what we assume.
- Committed samples change only when the recorder that made them was wrong. The commit names the fault and re-records every case. A new case may be added when every existing sample stays byte for byte the same.
- The timestamps and the payload fill in a sample differ on every run. A test compares the fields the format fixes, never a whole datagram.

## Recording the samples

`cargo xtask record` records every case into `embedded-iperf2/tests/samples/<version>/`, and `cargo xtask record <case>` records one. It runs a stock iperf server and client on 127.0.0.1, with a relay between them that writes both directions to `<case>.bin`. It saves what the client and the server printed beside it. The rustdoc of `xtask/src/record/sample.rs` states the file layout.

The recorder needs the binary that `MCU_IPERF2` names, and refuses one that does not report the pinned version. `cargo xtask install-iperf` builds it from the iperf2 repository at the pin into `~/.local/opt/iperf/2.2.2/iperf`. A package manager's iperf has another version.

## Fuzzing

`cargo xtask fuzz --seconds N` seeds the corpus with each record of each sample, and runs the `parse` target in `fuzz/` on nightly for that long. It needs `cargo install cargo-fuzz` and a nightly toolchain. `CONVENTIONS.md` states when to run it. The files that count as a parser are `embedded-iperf2/src/wire/`. The nightly CI job runs it for ten minutes. A crash input lands under `fuzz/artifacts/parse/`, and the fix commits it as a regression test.
