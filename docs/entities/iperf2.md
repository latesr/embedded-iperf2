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

A stock iperf here is built from the repository at the pin with `./configure && make`. `cargo xtask install-iperf` does the build, into `~/.local/opt/iperf/2.2.2/iperf`. Homebrew's `iperf` was 2.2.1 on 2026-10-05.

## 2.2.2

Observed in [the 2.2.2 samples](../../embedded-iperf2/tests/samples/2.2.2/README.md), recorded on macOS from a build of `1779e3e3`:

- `iperf -v` prints `iperf 2.2.2 (29 September 2026) pthreads (community)`. 2.2.1 printed `iperf version 2.2.1`.
- The first bytes from a TCP client are the 64-byte test header with `flags` 0x40010080, or 0x44010080 with `-R`. See `tcp.bin` and `tcp-reverse.bin`.
- The TCP server sends the 28-byte header ack in a forward test, and nothing else. It sends no ack in a reverse test. See `tcp.bin` and `tcp-reverse.bin`.
- The header counts toward `-n`. With `-n 64K -l 16K`, the client sent the 64-byte header and 65472 bytes of data. See `tcp.bin`.
- In a reverse TCP test, each burst header leaves `burst_period_us` unwritten, so it holds fill. On macOS, the sender's TCP statistics are 0xFFFFFFFF in the first burst. In each later burst they hold values, such as `reverse_retry` 0 and `reverse_rtt` 1000. See `tcp-reverse.bin`.
- The `mPort` field holds the port the client connected to, which is the relay's port in these samples.
- The UDP client sets `flags` 0x480100A0, and 0xEC0100A0 with `-R -l 512`. See `udp.bin` and `udp-reverse.bin`.
- In a reverse UDP test, the client sent one header datagram. The server's datagrams hold the prefix and then fill from offset 16. See `udp-reverse.bin`.
- The server report holds a jitter of 168 µs in `udp.bin`, and the server prints 0.169 ms. The stock client prints the jitter of the same report as 0.000 ms. See `udp.client.txt` and `udp.server.txt`.
- The server report in `udp.bin` reads, by the layout of W-11: `flags` 0x88000000, 66560 bytes, a duration of 2 s and 17751 µs, 0 lost, 0 out of order, 130 datagrams and a jitter of 168 µs. 66560 bytes is 130 datagrams of 512 bytes, so the end-of-test datagram counts toward the bytes.
- The client printed `Sent 131 datagrams` in `udp.client.txt`. The relay saw 130 datagrams from it: 129 of data and one end of test. The server counted 130.

Observed by the interoperability tests in [`interop.rs`](../../embedded-iperf2/tests/interop.rs), on macOS from a build of `74d4b553`:

- A UDP sender counts the end-of-test datagram in the bytes of its own report, as the receiver counts it. A stock client that sends 51200 bytes in datagrams of 512 bytes reports 51712 bytes, and our server's count equals it. See `stock_client_against_our_server`.
- With `-y C`, a TCP report line has 9 fields, and the eighth holds the bytes. A UDP report line has 14 fields. It adds the jitter, the lost datagrams, the total, the percent lost and the out-of-order datagrams.

Observed by `cargo xtask hil`, [`xtask/src/hil/mod.rs`](../../xtask/src/hil/mod.rs), on the ESP32-S31 over Ethernet from a build of `74d4b553`, on 2026-10-18:

- In a reverse TCP test in byte mode, the server sends whole bursts past the count. With `-n 20000000` and the default burst of 131072 bytes, it sent 153 bursts, 20054016 bytes. Our client stopped once 20000980 bytes had arrived.

Read in the source at `74d4b553`:

- A client in a reverse TCP test reads until the server closes the connection, or until `-t` plus 2 s has passed ([`src/Server.cpp:184`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Server.cpp#l184) and [`src/Server.cpp:1148`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Server.cpp#l1148)). Its duration runs from its first read to that point.
- The server stops writing at `-t` and then closes. The bytes still in its send buffer cross the link before the close does. On a slow link, a reverse test therefore reports a duration longer than `-t`, while its rate stays the rate the link delivered.
