# iperf2

Covers: the iperf 2 tool, at the version this repo pins.

iperf 2 measures the throughput of a network over TCP and UDP. It is a separate project from iperf3, and the two do not speak to each other. Its source and releases live at <https://sourceforge.net/projects/iperf2/>. This repo pins iperf 2.2.2, released on 2026-09-29, and `cargo xtask record` records what that version sends. An observed fact points to a sample. A read fact points to [the iperf 2.2.2 source](../summaries/iperf2-2.2.2.md).

## Where 2.2.2 comes from

Read in the iperf2 repository on 2026-10-05:

- The maintainer releases each version from its own branch, such as `2-2-1` and `2-2-2`. The repository has no tag for 2.2.2.
- The pin is `74d4b553`, the tip of `2-2-2`. The release notes for 2.2.2 are commit `6b281a0b` on that branch. The two commits after it change only the Android app.
- `master` holds the same changes as separate commits. Its commit `1779e3e3` matches `74d4b553` in `src/`, `include/`, `compat/` and `configure.ac`, and differs only in `doc/README.txt` and `doc/RELEASE_NOTES`.
- SourceForge publishes `iperf-2.2.2-SHA256SUMS.txt`, which names `iperf-2.2.2.tar.gz`. The tarball itself was not on SourceForge on 2026-10-05.
- The 2.2.1 tarball matches the MD5 that SourceForge publishes. Its 61 files in `src/` and `include/` match commit `5fe98129` on the branch `2-2-1` byte for byte.

A stock iperf here is built from the repository at the pin with `./configure && make`. Homebrew's `iperf` was 2.2.1 on 2026-10-05.

## 2.2.2

Observed in [the 2.2.2 samples](../../embedded-iperf2/tests/samples/2.2.2/README.md), recorded on macOS from a build of `1779e3e3`:

- `iperf -v` prints `iperf 2.2.2 (29 September 2026) pthreads (community)`. 2.2.1 printed `iperf version 2.2.1`.
- The first bytes from a TCP client are the 64-byte test header with `flags` 0x40010080, or 0x44010080 with `-R`. See `tcp.bin` and `tcp-reverse.bin`.
- The TCP server sends the 28-byte header ack in a forward test, and nothing else. It sends no ack in a reverse test. See `tcp.bin` and `tcp-reverse.bin`.
- The header counts toward `-n`. With `-n 64K -l 16K`, the client sent the 64-byte header and 65472 bytes of data. See `tcp.bin`.
- In a reverse TCP test, each burst header leaves `burst_period_us` unwritten, so it holds fill. On macOS, `reverse_retry` is 0xFFFFFFFF in the first burst and 0 in the last. See `tcp-reverse.bin`.
- The `mPort` field holds the port the client connected to, which is the relay's port in these samples.
- The UDP client sets `flags` 0x480100A0, and 0xEC0100A0 with `-R -l 512`. See `udp.bin` and `udp-reverse.bin`.
- In a reverse UDP test, the client sent one header datagram. The server's datagrams hold the prefix and then fill from offset 16. See `udp-reverse.bin`.
- The server report holds a jitter of 168 µs in `udp.bin`, and the server prints 0.169 ms. The stock client prints the jitter of the same report as 0.000 ms. See `udp.client.txt` and `udp.server.txt`.
- The server report in `udp.bin` reads, by the layout of W-11: `flags` 0x88000000, 66560 bytes, a duration of 2 s and 17751 µs, 0 lost, 0 out of order, 130 datagrams and a jitter of 168 µs. 66560 bytes is 130 datagrams of 512 bytes, so the end-of-test datagram counts toward the bytes.
- The client printed `Sent 131 datagrams` in `udp.client.txt`. The relay saw 130 datagrams from it: 129 of data and one end of test. The server counted 130.
