# Requirements

embedded-iperf2 is an iperf 2 client and server for microcontrollers, wire compatible with iperf 2.2.2. It is written in Rust. The core is `no_std`, does not allocate memory and is sans-I/O. It does not read from the network, write to it or keep time itself. The caller moves the bytes and supplies the time. Two drivers behind cargo features do the I/O: one over embassy-net for a board, and one over the standard library for a host.

Each requirement has an ID, such as W-3 or P-12. Tests, plans and commits cite the ID. An ID is never reused after its requirement is removed.

## Terms

Each term has one meaning. A term with a concept page is defined there.

| Term | Meaning |
|---|---|
| Stock iperf | A stock iperf is a binary built from the iperf2 repository at the commit the [iperf2](docs/entities/iperf2.md) page pins. |
| Client | The client starts a test. It opens the connection and sends the test header. |
| Server | The server listens for tests and accepts them. |
| Sender and receiver | The sender sends the data of a test, and the receiver receives it. The client is the sender, except in a reverse test. |
| Reverse test | A reverse test is one the client starts with `-R`. The server is the sender. |
| Time mode and byte mode | A test in time mode lasts for a duration, `-t`. A test in byte mode sends a count of bytes, `-n`. |
| Test header | See [Test header](docs/concepts/test-header.md). |
| Prefix | The prefix is the first 16 bytes of every UDP datagram. See W-1. |
| Packet ID | The packet ID is the signed 64-bit number in the prefix. See W-1. |
| End of a UDP test | See [End of a UDP test](docs/concepts/udp-end-of-test.md). |
| Server report | The server report is the datagram a UDP server sends at the end of a test. See W-11. |
| Header ack | The header ack is the 28 bytes a TCP server sends after the test header. See W-9. |
| Burst header | The burst header is the 120 bytes at the start of each burst of a reverse TCP test. See W-10. |
| Loss, out-of-order and jitter | See [Loss, out-of-order and jitter](docs/concepts/loss-and-jitter.md). |
| Interval | An interval is one period of `-i`, the report interval. |
| Caller | The caller is the code that owns a client or a server and its sockets, such as a driver. |

## Why another one?

crates.io had no `no_std` iperf crate on 2026-10-05. Every iperf crate there is a host tool, and most of them speak iperf3. This crate speaks iperf 2, so a maintainer can repeat a number from a board with a stock `iperf` on a host.

## Goals

- A board runs an iperf 2 client or server against a stock iperf 2.2.2 on a host, over TCP or UDP, in either direction.
- A host runs the same client and server through the `std` driver, so a test tool such as `mcu-run` can be the host end.
- The core runs on ESP32 and STM32 microcontrollers without a heap.
- Each wire format is checked against samples recorded from a stock iperf 2.2.2, not only against our reading of the source.

## Design principles

- Compatible with iperf 2.2.2 means that a stock client and server work against ours, in each v1 mode, and print the same report fields. It does not mean that our datagrams equal theirs byte for byte. The timestamps and the payload fill differ.
- The core takes time from the caller as a monotonic count of microseconds.

## Out of scope for v1

| Feature | What v1 does |
|---|---|
| Parallel streams, `-P` | The server refuses the test. See P-13. |
| Tradeoff and dual tests, `-r` and `-d` | The server refuses the test. See P-13. |
| Full duplex, `--full-duplex` | The server refuses the test. See P-13. |
| Isochronous tests, bursts, `--trip-times` and bounceback | The server refuses the test. See P-13. |
| L4S, multicast and the congestion control choice | The server refuses the test. See P-13. |
| Permit keys | The server refuses the test. See P-13. |
| IPv6 on a board | The `embassy-net` driver uses IPv4. See D-4. |

## Crates

| Crate | On crates.io | Contents |
|---|---|---|
| `embedded-iperf2` | Yes | The wire format, the client and server state machines, and the drivers behind the `embassy-net` and `std` features |
| `xtask` | No | The checks, the recorder and other repo tasks |

## Wire format

Every field is big-endian. Each offset counts from the start of the message. [The iperf 2.2.2 source](docs/summaries/iperf2-2.2.2.md) is the source of each layout, and the samples check it (T-1).

### UDP datagrams

**W-1** Every UDP datagram of a test starts with the prefix. The packet ID is `id` as the low 32 bits and `id2` as the high 32 bits, read as a signed 64-bit number.

| Offset | Size | Field | Contents |
|---|---|---|---|
| 0 | 4 | `id` | The low 32 bits of the packet ID |
| 4 | 4 | `tv_sec` | The sender's clock at the send, seconds |
| 8 | 4 | `tv_usec` | The sender's clock at the send, microseconds below one second |
| 12 | 4 | `id2` | The high 32 bits of the packet ID |

**W-2** A sender's first datagram has packet ID 1, and each later datagram of data has the ID one above the one before.

**W-3** A datagram with a negative packet ID ends the test. The first such datagram has ID `-N`, where `N` is one above the ID of the last datagram of data. Each repeat of it has an ID one lower than the one before.

### The test header

**W-4** A TCP client sends a 64-byte test header as the first bytes of the stream. A UDP client sends an 80-byte test header at the start of every datagram, as the prefix and then 64 bytes. Our client writes these values:

| TCP offset | UDP offset | Size | Field | Contents |
|---|---|---|---|---|
| 0 | 16 | 4 | `flags` | The flags of W-5 |
| 4 | 20 | 4 | `numThreads` | 1 |
| 8 | 24 | 4 | `mPort` | The server port |
| 12 | 28 | 4 | `mBufLen` | `-l`, or 0 when the caller did not set it |
| 16 | 32 | 4 | `mWinBand` | 0 |
| 20 | 36 | 4 | `mAmount` | The length of the test, as W-6 states |
| 24 | 40 | 8 | `type`, `length` | 0 |
| 32 | 48 | 2 | `upperflags` | 0x0400 in a reverse test, and 0 otherwise |
| 34 | 50 | 2 | `lowerflags` | 0 |
| 36 | 52 | 4 | `version_u` | 0x00020002 |
| 40 | 56 | 4 | `version_l` | 0x00020003 |
| 44 | 60 | 4 | `reserved`, `tos` | 0 |
| 48 | 64 | 4 | `lRate` | The low 32 bits of the rate in bits per second |
| 52 | 68 | 4 | `uRate` | The high 32 bits of the rate, shifted left by 8 |
| 56 | 72 | 8 | `TCPWritePrefetch`, `barrier_usecs` | 0 |

**W-5** Our client writes the same `flags` as a stock 2.2.2 client in each v1 mode:

| Test | `flags` |
|---|---|
| TCP | 0x40010080 |
| TCP, reverse | 0x44010080 |
| UDP | 0x480100A0 |
| UDP, reverse | 0x6C0100A0 |
| UDP, reverse, with `-l` | 0xEC0100A0 |

**W-6** In time mode, `mAmount` is the duration in units of 10 ms, negated, as a 32-bit two's complement number. In byte mode, `mAmount` is the byte count masked with 0x7FFFFFFF. A reader takes a set top bit as time mode.

**W-7** A reader takes the header length from `flags`. With none of 0x80000000, 0x40000000 and 0x04000000 set, the stream has no test header. With 0x00010000 set, the length is `(flags & 0xFFFE) >> 1`. Otherwise the length is 24 when 0x80000000 or 0x40000000 is set, plus 40 when 0x04000000 or 0x40000000 is set. A reader refuses a length above 256. When a header is present, a reader also refuses a length below 64 in a TCP header, or below 80 in a UDP header, whose length includes the prefix.

**W-8** The rate in a UDP header is the caller's rate, or 1048576 bit/s when the caller did not set one. The rate in a TCP header is the caller's rate, or 0 when the caller did not set one.

### TCP messages

**W-9** The header ack is 28 bytes:

| Offset | Size | Field | Contents |
|---|---|---|---|
| 0 | 4 | `type` | 2 |
| 4 | 4 | `length` | 28 |
| 8 | 4 | `flags` | 0 |
| 12 | 4 | `version_u` | 0x00020002 |
| 16 | 4 | `version_l` | 0x00020003 |
| 20 | 8 | `reserved1`, `reserved2` | 0 |

**W-10** In a reverse TCP test, each burst of `burst_size` bytes starts with a 120-byte burst header. A reader needs these fields:

| Offset | Size | Field | Contents |
|---|---|---|---|
| 12 | 8 | `start_tv_sec`, `start_tv_usec` | The sender's clock at the start of the test |
| 20 | 8 | `write_tv_sec`, `write_tv_usec` | The sender's clock at the write of the burst |
| 44 | 4 | `burst_id` | 1 for the first burst, and one more for each later burst |
| 48 | 4 | `burst_size` | The length of the burst, header included |
| 52 | 8 | `seqno_lower`, `seqno_upper` | 1 plus the bytes of every burst so far, this one included, low and high word |

**W-13** The other fields of the burst header hold the sender's TCP statistics. Our server writes 0xFFFFFFFF in `reverse_retry` at offset 36 and in each 4-byte field from offset 76 to 107, which a stock reader takes as unknown. It writes 0 from offset 108 to 119, and fill in the other bytes.

### The server report

**W-11** The server report is 128 bytes. The prefix is zeros. `flags` at offset 16 is 0x88000000. Each count of 64 bits is split into a high and a low word.

| Offset | Size | Field | Contents |
|---|---|---|---|
| 16 | 4 | `flags` | 0x88000000 |
| 20 | 8 | `total_len1`, `total_len2` | The bytes received, high and low word |
| 28 | 8 | `stop_sec`, `stop_usec` | The duration of the test at the server |
| 36 | 4 | `error_cnt` | The lost datagrams, low word |
| 40 | 4 | `outorder_cnt` | The out-of-order datagrams, low word |
| 44 | 4 | `datagrams` | The highest packet ID received, low word |
| 48 | 8 | `jitter1`, `jitter2` | The jitter, seconds and microseconds |
| 56 | 60 | the transit statistics | The minimum, maximum, sum, mean, M2 and last deviation of transit time, each as seconds and microseconds, then the count of transit times and two IPG words |
| 116 | 4 | `error_cnt2` | The lost datagrams, high word |
| 120 | 4 | `outorder_cnt2` | The out-of-order datagrams, high word |
| 124 | 4 | `datagrams2` | The highest packet ID received, high word |

### Parsing

**W-12** Every parser takes a byte slice and reports whether it holds a complete message, too few bytes or bytes that cannot be the message. A parser never panics, and never reads past the end of the slice.

## Protocol

### The client

**P-1** A TCP client sends the test header, then data until the duration ends or the byte count is sent. The header bytes count toward the byte count. The client then shuts down its write side, and reads and discards what the server sends until the server closes. It waits for the close for at most the larger of the duration and 2 s.

**P-2** A UDP client sends datagrams of `-l` bytes at the rate of W-8. Over a test of at least 1 s, the bytes it sends per second are within 2 % of the rate. In byte mode, the last datagram is cut to the bytes left, and is never shorter than the 80-byte header.

**P-3** After its last datagram of data, a UDP client sends the end of the test of W-3. It waits up to 10 ms for a reply, and sends again after each silent wait, up to 200 sends. A reply of at least 128 bytes whose `flags` have 0x80000000 set is the server report. The client ignores any other reply.

**P-4** A UDP client that receives no server report ends the test without one, and its result says so.

**P-5** A client in a reverse TCP test sends the test header and then reads bursts of W-10 until the server closes. In time mode it also ends the test at the duration plus 2 s after the header is out. In byte mode it also ends the test when the byte count has arrived, or 10 s after the last bytes. It refuses a burst header whose `burst_size` is below 120.

**P-6** A client in a reverse UDP test sends the header datagram with packet ID 1, and sends it again each second until data arrives. It ends the test with an error when no data arrives within 10 s.

**P-22** A client in a reverse UDP test receives as a UDP server does (P-11), and ends the test at a negative packet ID. It also ends the test at the duration plus 2 s in time mode, and 10 s after the last datagram in byte mode.

**P-23** A client in a reverse UDP test sends no server report.

**P-24** A TCP client whose server closes the connection while the client sends ends the test with its final report. A close before a reverse test's header is out ends the test as refused.

**P-7** Without the caller's choice, the client uses port 5001, a duration of 10 s, `-l` of 131072 bytes for TCP and 1470 bytes for UDP over IPv4.

### The server

**P-8** A TCP server reads the first 4 bytes of the stream as `flags`, then the rest of the test header of W-7. The header bytes count toward the bytes received. Without a test header, the 4 bytes do not count.

**P-9** A TCP server sends the header ack of W-9 after the test header when `flags` have 0x40000000 set and 0x04000000 clear, or have 0x02000000 set.

**P-10** A TCP server receives until the client closes, then closes. The test lasts from the first byte to the close. The bytes that arrive before the header ack is out count, and a close then ends the test with its report.

**P-11** A UDP server starts a test at a datagram of at least 40 bytes with a packet ID above 0. It reads 64-bit packet IDs when the first datagram's `flags` have 0x08000000 set, and 32-bit IDs otherwise. It reads no test header when `flags` are 0x36373839.

**P-12** A UDP server ends a test at a negative packet ID, and sends the server report of W-11. For each datagram that arrives within 250 ms of its last report, it sends the same report again, up to 9 reports. Once the report has gone out, a datagram from another address and port ends the test, and the repeats stop.

**P-13** A server refuses a test whose test header asks for a mode outside v1. Such a header has 0x80000000 set without 0x04000000, or has any of 0x10000000 and 0x00800000 set, or has an `upperflags` or `lowerflags` bit other than 0x0400 set, or has a `numThreads` other than 1. A TCP server closes the connection. A UDP server ignores the flow.

**P-14** A server runs one test at a time. It refuses a TCP connection, or ignores a UDP flow, that starts while a test runs. A UDP test that waits after its report, as P-12 states, is not running.

**P-15** A server that the caller gave a duration ends a test at that duration from its first byte, whether or not the end of the test arrived, in every state until its final report.

**P-16** A TCP server in a reverse test sends for the length of W-6 at the rate of W-8. It sends bursts of `mBufLen` bytes, or 131072 bytes when `mBufLen` is 0, and 120 bytes when that is smaller. Each burst starts with the burst header of W-10. In time mode the last burst is sent whole. The server then shuts down its write side. A close from the client while the server sends ends the test with the bytes sent.

**P-21** A UDP server in a reverse test sends for the length of W-6 at the rate of W-8. Its datagrams are `mBufLen` bytes, cut to 1470 bytes over IPv4 or 1450 over IPv6, and those defaults when `mBufLen` is 0. Each datagram holds the prefix and fill, without a header. The server then sends one datagram of negative packet ID, and does not wait for a reply.

### Counting

**P-17** A receiver counts loss, out-of-order and jitter as the [Loss, out-of-order and jitter](docs/concepts/loss-and-jitter.md) page states. For the same datagrams in the same order, its byte, datagram, lost and out-of-order counts equal those of a stock iperf 2.2.2 server.

**P-18** Jitter compares differences of transit time, so the sender's clock and the receiver's clock do not need to agree. A sender writes its own monotonic time to the prefix.

**P-19** When the caller sets an interval, the client and the server each report the bytes of every interval that ends while data moves, an empty one included. Bytes count toward the interval they arrived in. A test that ends inside an interval reports that last part before its final report. A UDP receiver adds the lost, out-of-order and datagram counts and the jitter of the interval.

**P-20** At the end of a test, the client and the server each report the bytes and the duration. A UDP sender's bytes include its first end-of-test datagram, as a stock sender's do. A UDP receiver adds the lost, out-of-order and datagram counts and the jitter. A UDP client adds the server report when one arrived.

**P-25** A test's duration runs from the first bytes that this end sent or received. A sender's duration ends when its socket takes the last bytes, or at the receiver's close. A TCP receiver's ends at the close, or when it stops waiting for one. A UDP receiver's ends at the end-of-test datagram, or at its last datagram when the end does not arrive.

## Drivers

**D-1** The core builds without any feature, for every target in the CI targets table, and does not allocate memory.

**D-2** The `std` feature adds a driver that runs a client or a server over the standard library's TCP and UDP sockets.

**D-3** The `embassy-net` feature adds an async driver that runs a client or a server over an embassy-net stack. The caller supplies the stack and every buffer.

**D-4** The `embassy-net` driver uses IPv4. The core does not depend on the address family, and the `std` driver takes IPv4 and IPv6.

**D-5** Each driver supplies the core's time from a monotonic clock in microseconds.

**D-6** Until a UDP server starts or refuses a test, a driver passes it the datagrams of every address and port. From then until the test ends, it passes only those of the address and port that started or refused it. A datagram from another address and port that ends a test, as P-12 states, is the first datagram the driver passes to the next server. After a server refuses a flow, the driver drops that flow's later datagrams, rather than start a new server for each.

## Security

- The server reads bytes from the network. Each length and offset is checked before use (W-12). A header the server cannot read refuses the test, and never panics.
- A server answers any client that reaches it. Permit keys are out of scope, so a server runs only on a network the caller trusts.

## Platforms

### CI targets

| Target | Checks |
|---|---|
| x86_64 and aarch64 hosts | All tests, on every CI push |
| `riscv32imac-unknown-none-elf` | Build on every CI push |
| `thumbv7em-none-eabihf` | Build on every CI push |
| `thumbv6m-none-eabi` | Build on every CI push |

### Reference boards

The boards come from embedded-devkit, and the firmware builds on its board crates.

| Board | Chip | Order |
|---|---|---|
| ESP32-S31 Function CoreBoard-1 | ESP32-S31 | First |
| ESP32-C5-DevKitC-1 | ESP32-C5 | Second |

## Constraints

- The crates build on stable Rust. The minimum supported Rust version (MSRV) is 1.91, with edition 2024.
- The core is `no_std` and does not allocate memory.
- A public API names types from `embassy-net` only behind the `embassy-net` feature, and from the standard library only behind the `std` feature.

`CONVENTIONS.md` names the lints that enforce the code constraints.

## Testing

Development is test first. `CONVENTIONS.md` states how a developer writes and checks tests. This section states what the test suite must contain and what CI runs.

### Samples

**T-1** `cargo xtask record` records the samples from a stock iperf 2.2.2 on loopback into `embedded-iperf2/tests/samples/2.2.2/`. The cases cover TCP and UDP, each forward and reverse, and each transport in time mode and in byte mode. The samples' `README.md` lists them. A test parses every message in every sample, and checks each field that the wire format fixes.

**T-2** A test feeds the client datagrams of each forward UDP sample to our server, in their recorded order. The byte, datagram, lost and out-of-order counts in our report equal those in the recorded server report.

### Unit and property tests

**T-3** Every parser has property tests. Any bytes never make it panic. A message the encoder writes parses back to the same values. Every prefix of a complete message reports too few bytes.

### Fuzzing

**T-4** Each parser has a fuzz target. Its corpus starts with the messages in the samples.

### Interoperability tests

**T-5** On loopback, the `std` driver runs a client against a stock iperf 2.2.2 server, and a server against a stock iperf 2.2.2 client. Each runs TCP and UDP, forward and reverse, in time mode and byte mode. The stock binary's report and ours agree on the bytes, and on the lost and out-of-order counts.

**T-6** The `embassy-net` driver runs a client against its own server over two embassy-net stacks in one process, in each mode of T-5.

### Hardware tests

**T-7** Firmware under `firmware/<board>/`, built on the embedded-devkit board crate of each reference board, runs a client and a server against the `std` driver on the host, in each mode of T-5. `cargo xtask hil` runs them by hand, never in CI.

### Continuous integration

- CI runs `cargo xtask check --full` on every push. `CONVENTIONS.md` and `AGENTS.md` list what it runs.
- CI runs cargo-mutants over the changed lines on every push.
- A full cargo-mutants run covers every crate nightly and before each release.
- The nightly job checks every feature combination of every published crate with `cargo hack`.

## Licence

Users may choose either the MIT licence or the Apache-2.0 licence. The Cargo `license` field is `MIT OR Apache-2.0`. Contributions are licensed on the same terms.

## Open questions

None.
