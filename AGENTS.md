@CONVENTIONS.md
@CONTRIBUTING.md
@CODE.md
@TESTING.md
@EMBEDDED.md

# embedded-iperf2

embedded-iperf2 is an iperf 2 client and server for microcontrollers, wire compatible with iperf 2.2.2.

## The wire format

- A sample is recorded from a stock iperf at the pinned version. It is never written by hand to match an assumption.
- Committed samples change only when the recorder that made them was wrong. The commit names the fault and re-records every case. A new case can be added when every existing sample stays byte for byte the same.

## Recording the samples

`cargo xtask record` records every case into `embedded-iperf2/tests/samples/<version>/`, and `cargo xtask record <case>` records one. The rustdoc of `xtask/src/record/` states how it records, and the file layout.

The recorder and the interoperability tests need the binary that `MCU_IPERF2` names, and refuse one that does not report the pinned version. `cargo xtask install-iperf` builds it from the iperf2 repository at the pin into `~/.local/opt/iperf/2.2.2/iperf`. A package manager's iperf has another version.

## Fuzzing

`cargo xtask fuzz --seconds N` seeds the corpus with each record of each sample, and runs the `parse` target in `fuzz/` for that long. The files that count as a parser are `embedded-iperf2/src/wire/`.
