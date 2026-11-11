# Requirements

embedded-iperf2 is an iperf 2 client and server for microcontrollers, wire compatible with iperf 2.2.2. The core is `no_std`, does not allocate memory, and is sans-I/O. The core does not read from the network, write to it, or keep time. The caller moves the bytes and supplies the time as a monotonic count of microseconds. Two drivers behind cargo features do the I/O: one over embassy-net for a board, and one over the standard library for a host.

Each requirement has an ID, such as W-3. The letter names its section:

- W, the wire format
- P, the protocol of the client and the server
- D, the drivers
- T, the tests

## Terms

| Term | Meaning |
|---|---|
| Stock iperf | A stock iperf is a binary built from the iperf2 repository at commit `74d4b553`, the tip of the branch `2-2-2`. `cargo xtask install-iperf` builds it. |
| Compatible with iperf 2.2.2 | A stock client and a stock server work against this crate in each v1 mode, and print the same report fields. The datagrams do not need to be equal byte for byte. |
| Client | The client starts a test. It opens the connection and sends the test header. |
| Server | The server listens for tests and accepts them. |
| Sender and receiver | The sender sends the data of a test, and the receiver receives it. The client is the sender, except in a reverse test. |
| Reverse test | A reverse test is one that the client starts with `-R`. The server is the sender. |
| Time mode and byte mode | A test in time mode lasts for a duration, `-t`. A test in byte mode sends a count of bytes, `-n`. |
| Test header | The test header is what a client sends at the start of a test. It states the direction, the length of the test, the datagram length, and the rate. See W-4 to W-8. |
| Prefix | The prefix is the first 16 bytes of every UDP datagram. See W-1. |
| Packet ID | The packet ID is the signed 64-bit number in the prefix. See W-1. |
| End of a UDP test | The end of a UDP test is the datagram with a negative packet ID that the sender sends after its data. See W-3 and P-3. |
| Server report | The server report is the datagram that a UDP server sends at the end of a test. See W-11. |
| Header ack | The header ack is the 28 bytes that a TCP server sends after the test header. See W-9. |
| Burst header | The burst header is the 120 bytes at the start of each burst of a reverse TCP test. See W-10. |
| Loss, out-of-order and jitter | Loss, out-of-order and jitter are the counts that a UDP receiver keeps from the packet ID and send time of each datagram. See P-17 and P-18. |
| Interval | An interval is one period of `-i`, the report interval. |
| Caller | The caller is the code that owns a client or a server and its sockets, such as a driver. |

## Why another one?

On 2026-10-05, crates.io did not have an iperf crate that runs on a microcontroller. This crate implements the iperf 2 protocol, so a stock iperf 2.2.2 can run on the host.

## Goals

- A board runs an iperf 2 client or server against a stock iperf 2.2.2 on a host, over TCP or UDP, in either direction.
- A host runs the same client and server through the `std` driver, so a test tool such as `mcu-run` can be the host end.

## Out of scope for v1

The server refuses a test that asks for one of these features (P-13):

- parallel streams, `-P`
- tradeoff and dual tests, `-r` and `-d`
- full duplex, `--full-duplex`
- isochronous tests, bursts, `--trip-times`, and bounceback
- L4S, multicast, and the choice of congestion control
- permit keys

The `embassy-net` driver uses IPv4 only (D-4).

## Crates

| Crate | On crates.io | Contents |
|---|---|---|
| `embedded-iperf2` | Yes | The wire format, the client and server state machines, and the drivers behind the `embassy-net` and `std` features |
| `xtask` | No | The checks, the recorder, and the other repo tasks |

## Wire format

Every field is big-endian. Each offset counts from the start of the message. The samples check each layout (T-1).

### UDP datagrams

**W-1** Every UDP datagram of a test starts with the prefix. The packet ID is `id` as the low 32 bits and `id2` as the high 32 bits, read as a signed 64-bit number.

| Offset | Size | Field | Contents |
|---|---|---|---|
| 0 | 4 | `id` | The low 32 bits of the packet ID |
| 4 | 4 | `tv_sec` | The sender's clock at the send, seconds |
| 8 | 4 | `tv_usec` | The sender's clock at the send, microseconds below one second |
| 12 | 4 | `id2` | The high 32 bits of the packet ID |

**W-2** A sender's first datagram has packet ID 1. Each later datagram of data has the ID one above the ID before it.

**W-3** A datagram with a negative packet ID ends the test. The first such datagram has ID `-N`, where `N` is one above the ID of the last datagram of data. Each repeat of it has an ID one lower than the one before.

### The test header

**W-4** A TCP client sends a 64-byte test header as the first bytes of the stream. A UDP client sends an 80-byte test header at the start of every datagram, as the prefix and then 64 bytes. The client writes these values:

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

**W-5** The client writes the same `flags` as a stock 2.2.2 client in each v1 mode:

| Test | `flags` |
|---|---|
| TCP | 0x40010080 |
| TCP, reverse | 0x44010080 |
| UDP | 0x480100A0 |
| UDP, reverse | 0x6C0100A0 |
| UDP, reverse, with `-l` | 0xEC0100A0 |

**W-6** In time mode, `mAmount` is the duration in units of 10 ms, negated, as a 32-bit two's complement number. In byte mode, `mAmount` is the byte count masked with 0x7FFFFFFF. A reader takes a set top bit as time mode.

**W-7** A reader takes the header length from `flags`:

- With none of 0x80000000, 0x40000000, and 0x04000000 set, the stream does not have a test header.
- With 0x00010000 set, the length is `(flags & 0xFFFE) >> 1`.
- Otherwise, the length is 24 when 0x80000000 or 0x40000000 is set, plus 40 when 0x04000000 or 0x40000000 is set.

A reader refuses a length above 256. When a header is present, a reader also refuses a TCP header length below 64. It refuses a UDP header length below 80, because that length includes the prefix.

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

**W-13** The other fields of the burst header hold the sender's TCP statistics. The server writes 0xFFFFFFFF in `reverse_retry` at offset 36, and in each 4-byte field from offset 76 to 107. A stock reader takes that value as unknown. The server writes 0 from offset 108 to 119, and fill in the other bytes.

### The server report

**W-11** The server report is 128 bytes. The prefix is zeros. `flags` at offset 16 is 0x88000000. Each 64-bit count is split into a high word and a low word.

| Offset | Size | Field | Contents |
|---|---|---|---|
| 16 | 4 | `flags` | 0x88000000 |
| 20 | 8 | `total_len1`, `total_len2` | The bytes received, high and low word |
| 28 | 8 | `stop_sec`, `stop_usec` | The duration of the test at the server |
| 36 | 4 | `error_cnt` | The lost datagrams, low word |
| 40 | 4 | `outorder_cnt` | The out-of-order datagrams, low word |
| 44 | 4 | `datagrams` | The highest packet ID received, low word |
| 48 | 8 | `jitter1`, `jitter2` | The jitter, seconds and microseconds |
| 56 | 60 | The transit statistics | The [transit statistics](#the-transit-statistics) |
| 116 | 4 | `error_cnt2` | The lost datagrams, high word |
| 120 | 4 | `outorder_cnt2` | The out-of-order datagrams, high word |
| 124 | 4 | `datagrams2` | The highest packet ID received, high word |

#### The transit statistics

The transit statistics are the minimum, the maximum, the sum, the mean, M2, and the last deviation of the transit time. Each is a pair of seconds and microseconds. The count of transit times and two IPG words follow them.

### Parsing

**W-12** Every parser takes a byte slice. It reports that the slice holds a complete message, too few bytes, or bytes that cannot be the message. A parser never panics, and never reads past the end of the slice.

## Protocol

### The client

**P-1** A TCP client sends the test header, then data until the duration ends or the byte count is sent. The header bytes count toward the byte count. The client then shuts down its write side. It reads and discards what the server sends until the server closes. It waits for the close for at most the larger of the duration and 2 s.

**P-2** A UDP client sends datagrams of `-l` bytes at the rate of W-8. Over a test of at least 1 s, the bytes that it sends per second are within 2 % of the rate. In byte mode, the last datagram is cut to the bytes left, and is never shorter than the 80-byte header.

**P-3** After its last datagram of data, a UDP client sends the end of the test of W-3. It waits up to 10 ms for a reply, and sends again after each silent wait, up to 200 sends. The server report is a reply of at least 128 bytes with 0x80000000 set in `flags`. The client ignores any other reply.

**P-4** A UDP client that does not receive a server report ends the test without one, and its result says so.

**P-5** A client in a reverse TCP test sends the test header, then reads bursts of W-10 until the server closes. In time mode, it also ends the test at the duration plus 2 s after the header is out. In byte mode, it also ends the test when the byte count has arrived, or 10 s after the last bytes. It refuses a burst header with a `burst_size` below 120.

**P-6** A client in a reverse UDP test sends the header datagram with packet ID 1. It sends the datagram again each second until data arrives. It ends the test with an error when no data arrives within 10 s.

**P-22** A client in a reverse UDP test receives as a UDP server does (P-11), and ends the test at a negative packet ID. It also ends the test at the duration plus 2 s in time mode, and 10 s after the last datagram in byte mode.

**P-23** A client in a reverse UDP test does not send a server report.

**P-24** When the server closes the connection while a TCP client sends, the client ends the test with its final report. A close before a reverse test's header is out ends the test as refused.

**P-7** When the caller does not choose them, the client uses port 5001 and a duration of 10 s. It uses an `-l` of 131072 bytes for TCP, and of 1470 bytes for UDP over IPv4.

### The server

**P-8** A TCP server reads the first 4 bytes of the stream as `flags`, then the rest of the test header of W-7. The header bytes count toward the bytes received. When the stream does not have a test header, the 4 bytes do not count.

**P-9** A TCP server sends the header ack of W-9 after the test header in two cases:

- `flags` have 0x40000000 set and 0x04000000 clear.
- `flags` have 0x02000000 set.

**P-10** A TCP server receives until the client closes, then closes. The test lasts from the first byte to the close. The bytes that arrive before the header ack is out count. A close before the header ack is out ends the test with its report.

**P-11** A UDP server starts a test at a datagram of at least 40 bytes with a packet ID above 0. It reads 64-bit packet IDs when the first datagram's `flags` have 0x08000000 set, and 32-bit IDs otherwise. It does not read a test header when `flags` are 0x36373839.

**P-12** A UDP server ends a test at a negative packet ID, and sends the server report of W-11. For each datagram that arrives within 250 ms of its last report, it sends the same report again, up to 9 reports. Once the report has gone out, a datagram from another address and port ends the test, and the repeats stop.

**P-13** A server refuses a test when the test header asks for a mode outside v1. Such a header has one of these:

- 0x80000000 set without 0x04000000
- 0x10000000 or 0x00800000 set
- a bit of `upperflags` or `lowerflags` other than 0x0400 set
- a `numThreads` other than 1

A TCP server closes the connection. A UDP server ignores the flow.

**P-14** A server runs one test at a time. It refuses a TCP connection, or ignores a UDP flow, that starts while a test runs. A UDP test that waits after its report, as P-12 states, is not running.

**P-15** When the caller gave a server a duration, the server ends a test at that duration from its first byte. It does so in every state until its final report, whether or not the end of the test arrived.

**P-16** A TCP server in a reverse test sends for the length of W-6 at the rate of W-8. Each burst is `mBufLen` bytes, or 131072 bytes when `mBufLen` is 0, and at least 120 bytes. Each burst starts with the burst header of W-10. In time mode, the server sends the last burst whole. The server then shuts down its write side. A close from the client while the server sends ends the test with the bytes sent.

**P-21** A UDP server in a reverse test sends for the length of W-6 at the rate of W-8. Its datagrams are `mBufLen` bytes, cut to 1470 bytes over IPv4 or 1450 bytes over IPv6. When `mBufLen` is 0, the datagrams have those default lengths. Each datagram holds the prefix and fill, without a header. The server then sends one datagram with a negative packet ID, and does not wait for a reply.

### Counting

**P-17** For the same datagrams in the same order, a receiver's byte, datagram, lost, and out-of-order counts equal those of a stock iperf 2.2.2 server. Its jitter is the interarrival jitter of RFC 1889. A final report holds the mean of the running jitter over the test.

**P-18** Jitter compares differences of transit time, so the sender's clock and the receiver's clock do not need to agree. A sender writes its own monotonic time to the prefix.

**P-19** When the caller sets an interval, the client and the server each report the bytes of each interval that ends while data moves. An empty interval is reported too. Bytes count toward the interval they arrived in. A test that ends inside an interval reports that last part before its final report. A UDP receiver adds the lost, out-of-order, and datagram counts and the jitter of the interval.

**P-20** At the end of a test, the client and the server each report the bytes and the duration. A UDP sender's bytes include its first end-of-test datagram, as a stock sender's bytes do. A UDP receiver adds the lost, out-of-order, and datagram counts and the jitter. A UDP client adds the server report when one arrived.

**P-25** A test's duration starts at the first bytes that this end sent or received. A sender's duration ends when its socket takes the last bytes, or at the receiver's close. A TCP receiver's duration ends at the close, or when it stops its wait for the close. A UDP receiver's duration ends at the end-of-test datagram, or at its last datagram when the end does not arrive.

## Drivers

**D-1** The core builds without any feature, for every target in the CI targets table, and does not allocate memory.

**D-2** The `std` feature adds a driver that runs a client or a server over the standard library's TCP and UDP sockets.

**D-3** The `embassy-net` feature adds an async driver that runs a client or a server over an embassy-net stack. The caller supplies the stack and every buffer.

**D-4** The `embassy-net` driver uses IPv4. The core does not depend on the address family, and the `std` driver takes IPv4 and IPv6.

**D-5** Each driver supplies the core's time from a monotonic clock in microseconds.

**D-6** Until a UDP server starts or refuses a test, a driver passes it the datagrams of every address and port. From then until the test ends, the driver passes only the datagrams of the address and port that started or refused it. A datagram from another address and port can end a test, as P-12 states. That datagram is the first datagram that the driver passes to the next server. After a server refuses a flow, the driver drops the later datagrams of that flow, and does not start a new server for each.

**D-7** A `std` server can have a start timeout. When nothing starts a test within it, `run_one` fails with an error of kind `TimedOut`. For TCP, a connection starts a test. For UDP, a datagram that starts or refuses a test does. The server can then run another test.

Without a start timeout, the server waits without a limit. A start timeout of zero is refused.

## Security

- A server refuses a test with a header that it cannot read.
- A server answers any client that reaches it, so it must run only on a network that the caller trusts.

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

| Board | Chip |
|---|---|
| ESP32-S31 Function CoreBoard-1 | ESP32-S31 |

## Constraints

- The crates build on stable Rust. The minimum supported Rust version (MSRV) is 1.91, with edition 2024.
- A public API names types from `embassy-net` only behind the `embassy-net` feature, and from the standard library only behind the `std` feature.

`EMBEDDED.md` names the lints that enforce the code constraints.

## Testing

### Samples

**T-1** `cargo xtask record` records the samples from a stock iperf 2.2.2 on loopback into `embedded-iperf2/tests/samples/2.2.2/`. The cases cover TCP and UDP, each forward and reverse, and each transport in time mode and in byte mode. The samples' `README.md` lists them. A test parses every message in every sample, and checks each field that the wire format fixes.

**T-2** A test gives the client datagrams of each forward UDP sample to the server, in their recorded order. The byte, datagram, lost, and out-of-order counts in the server's report equal those in the recorded server report.

### Unit and property tests

**T-3** Every parser has property tests:

- Any bytes never make it panic.
- A message that the encoder writes parses back to the same values.
- Every prefix of a complete message reports too few bytes.

### Fuzzing

**T-4** Each parser has a fuzz target. Its corpus starts with the messages in the samples.

### Interoperability tests

**T-5** On loopback, the `std` driver runs a client against a stock iperf 2.2.2 server, and a server against a stock iperf 2.2.2 client. Each runs TCP and UDP, forward and reverse, in time mode and byte mode. The report of the stock binary and the report of the driver agree on the bytes, and on the lost and out-of-order counts.

**T-6** The `embassy-net` driver runs a client against its own server over two embassy-net stacks in one process, in each mode of T-5.

### Hardware tests

**T-7** Firmware under `firmware/<board>/` runs a client and a server in each mode of T-5. It builds on the embedded-devkit board crate of each reference board. The other end is a stock iperf 2.2.2 or the `std` driver on the host. In each run, these must be true:

- The receiver loses no datagram.
- A test in time mode lasts its length at the receiver.
- Both ends count the same bytes. In a reverse TCP test in byte mode against a stock server, the receiver instead counts at least the byte count, and no more than the server sent.

`cargo xtask hil` runs these tests by hand, never in CI.

### Continuous integration

- CI runs `cargo xtask check --full` on every push to `main` and on every pull request. `CHECKS.md` and `AGENTS.md` list what it runs.
- CI runs cargo-mutants over the lines that each push or pull request changes.
- A full cargo-mutants run covers every crate nightly, and before each release.
- The nightly job checks every feature combination of every published crate with `cargo hack`. `cargo xtask check --full` covers each feature alone and each pair.

## Licence

Users may choose either the MIT licence or the Apache-2.0 licence. Contributions are licensed on the same terms.
