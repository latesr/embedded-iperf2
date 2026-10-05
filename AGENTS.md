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

The recorder needs the binary that `MCU_IPERF2` names, and refuses one that does not report the pinned version. Build it from the iperf2 repository at the pin that the [iperf2](docs/entities/iperf2.md) page names, with `./configure && make`, outside this repo. The binary is `src/iperf` in that clone. A package manager's iperf has another version.
