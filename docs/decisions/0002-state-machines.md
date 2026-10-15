# ADR-0002: The client and server state machines

**Status: accepted**

Covers: `Client` and `Server` in `embedded-iperf2`, and the modules under them: `config`, `report`, `counts`, `meter`, `stream` and `datagram`.

## Summary

- The core doesn't do I/O and doesn't keep time. The caller gives it the time in microseconds, writes what `poll` asks for, and passes in what the peer sent, so the same core runs on a board and on a host (D-5).
- One `Client` and one `Server` serve TCP and UDP, forward and reverse. A driver handles two types, and never has to know the role of a test before it reads the header.
- The caller supplies the buffer that each chunk or datagram is built in. The core doesn't allocate (D-1).
- A sender fills its buffer once and then rewrites only the header bytes at its start: the test or burst header of a TCP chunk, and the prefix and header of a UDP datagram. The stock iperf does the same. A chunk or datagram then costs the sender almost nothing beyond the socket's copy, so a board measures its network rather than the sender.
- A UDP server only sees the datagrams of its first client, and the driver enforces that. The core never holds an address, so it doesn't depend on the address type of any network stack (D-6).
- Jitter is kept in integer nanoseconds. A board without an FPU would pay for a floating point update on every datagram (P-17, P-18).
- Bytes count toward the interval they arrived in, not the interval of the next `poll`. A driver that reads before it polls then reports each interval as the stock iperf does (P-19).
- A close or a deadline in any state ends the test with a report of the bytes sent or received so far. Only a close before the whole TCP test header is an error, because a client in byte mode closes as soon as it has its bytes (P-10, P-15, P-16, P-24).

## Context

A board runs the client or the server under embassy-net, and a host runs them over the standard library, in a blocking loop or a thread. A design that owns its sockets, as the stock iperf does, needs one copy for each I/O model. The state of an iperf test is small: a header, a few counters, a pacing clock and a report. It fits a state machine that a driver calls.

A test has four roles: a TCP sender, a TCP receiver, a UDP sender and a UDP receiver. A reverse test swaps the client's role with the server's. A newcomer might make the four roles the public types. A driver would then have to pick a role from the test header before it could create the client or server that reads that header.

Time on a board comes from embassy-time, and on a host from `std::time::Instant`. Jitter needs a resolution below one microsecond at high datagram rates. A board without an FPU pays for each soft float operation, and the jitter updates on every datagram.

The order of events reaches the core in many ways. A driver may read before it writes, or read twice before it polls. A peer may close at any moment, and the network may lose a close or an end of a test. The stock iperf meets each of these with a thread that blocks and a timeout of its own.

## Decision

`Client::poll` and `Server::poll` return an `Action`: send bytes, wait until a time, report, close the write side, or done. `sent`, `received` and `closed` update the client or server between polls. The roles are internal types that `Client` and `Server` hold in their states, so a driver handles two types whatever the test turns out to be.

The caller passes a buffer at construction, and each `Send` borrows from it. A TCP chunk is as long as the buffer. A UDP datagram is `buf_len` bytes, and the constructor refuses a buffer that can't hold one.

A UDP server doesn't keep an address. The stock server connects its socket to the first datagram's sender. A driver does the same, or drops datagrams from other sources when its socket can't disconnect. The core never names an address type.

Jitter uses `J += (D - J) / 16` on `i64` nanoseconds. It can differ from the stock `double` in the last nanosecond. P-17 compares only the counts.

`received` and `closed` may arrive in any state. Each state either uses the bytes or the close, or ignores them by rule:

- A server counts the bytes that arrive before its ack is out.
- A reverse TCP client starts to read once its last header byte is sent, not at its next poll.
- A close while a client or server sends ends the test with the bytes sent.
- A deadline covers every state of a server once its first byte arrived.
- A reverse test in byte mode ends 10 s after its last data, because a lost close or end of the test would hold it for ever.

Interval reports follow the bytes. `received` closes an interval that is due before it counts, and holds the report for the next `poll`. The intervals of a stall are reported empty, and a test that ends inside an interval reports that last part before the final report, as the stock iperf prints them.

## Alternatives

- **Four public role machines.** Rejected, because a driver must read the header before it knows the role.
- **An async core over embedded-io-async.** Rejected, because UDP datagrams don't fit `Read` and `Write`, and a host tool would need an executor.
- **A peer address in each `received`.** Rejected, because it brings an address type into the core for a rule the driver enforces anyway.
- **`f64` jitter.** Rejected, because a board without an FPU pays for it on every datagram.
- **A fill that runs on unbroken across the stream, written into each chunk.** Rejected, because no reader checks the fill, and writing it cost the sender a pass over every chunk. On the ESP32-S31 that held TCP upload to 29.5 Mbit/s, where a plain socket loop sends 57. It was the first version.
- **Intervals closed only in `poll`.** Rejected, because a driver that reads before it polls counted bytes in the interval before the one they arrived in. It was the first version.
- **A close in the middle of a test as a protocol error.** Rejected, because a client in byte mode closes as soon as it has its bytes. A normal test then ended in an error. It was the first version.

## Consequences

- A driver is a loop of four calls, and the tests drive the client and server with bytes and times, without a socket.
- Every behaviour that depends on the order of events needs a test that produces that order, because the in-memory pair test polls each end in turn.
- A driver over IPv6 sets `ServerConfig::ipv6` for the length of a reverse test's datagrams, because the core doesn't know the address family.
