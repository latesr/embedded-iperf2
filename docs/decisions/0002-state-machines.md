# ADR-0002: One sans-I/O client and one sans-I/O server, driven by `poll`

**Status: accepted**

Covers: `Client` and `Server` in `embedded-iperf2`, and the modules under them: `config`, `report`, `counts`, `meter`, `stream` and `datagram`.

## Summary

- The core does no I/O and keeps no time. The caller passes the time in as microseconds, writes what `poll` asks for, and passes in what the peer sends (D-5).
- One `Client` and one `Server` each serve TCP and UDP, forward and reverse. The four roles of sender and receiver stay inside.
- The caller supplies the buffer each chunk or datagram is built in. The core does not allocate (D-1).
- The driver keeps a UDP server to the peer of its first datagram, and drops a refused flow (D-6).
- Jitter is kept in integer nanoseconds (P-17, P-18).
- Bytes count toward the interval they arrived in. `received` closes a due interval before it counts new bytes (P-19).
- A close or a deadline in any state ends a test with a report of what moved. Only a close before a whole TCP test header is an error (P-10, P-15, P-16, P-24).

## Context

A board runs the client or the server under embassy-net, and a host runs them over the standard library, in a blocking loop or a thread. A design that owns its sockets, as the stock iperf does, needs one copy for each I/O model. The state of an iperf test is small: a header, a few counters, a pacing clock and a report. It fits a state machine that a driver moves.

A test has four roles: a TCP sender, a TCP receiver, a UDP sender and a UDP receiver. A reverse test swaps the client's role with the server's. A newcomer might make the four roles the public types. A driver would then pick a role from the test header before it could build the machine that reads that header.

Time on a board comes from embassy-time, and on a host from `std::time::Instant`. Jitter needs a resolution below one microsecond at high datagram rates. A board without an FPU pays for each soft float operation, and the jitter updates on every datagram.

The order of events reaches the core in many ways. A driver may read before it writes, or read twice before it polls. A peer may close at any moment, and the network may lose a close or an end of a test. The stock iperf meets each of these with a thread that blocks and a timeout of its own.

## Decision

`Client::poll` and `Server::poll` return an `Action`: send bytes, wait until a time, report, close the write side, or done. `sent`, `received` and `closed` move the machine between polls. The roles are internal types that `Client` and `Server` hold in their states, so a driver handles two types whatever the test turns out to be.

The caller passes a buffer at construction, and each `Send` borrows from it. A TCP chunk is as long as the buffer. A UDP datagram is `buf_len` bytes, and the constructor refuses a buffer that cannot hold one.

A UDP server does not keep an address. The stock server connects its socket to the first datagram's sender. A driver does the same, so the core never names an address type.

Jitter uses `J += (D - J) / 16` on `i64` nanoseconds. It can differ from the stock `double` in the last nanosecond. P-17 compares only the counts.

`received` and `closed` may arrive in any state. Each state either uses the bytes or the close, or ignores them by rule:

- A server counts the bytes that arrive before its ack is out.
- A reverse TCP client starts to read once its last header byte is sent, not at its next poll.
- A close while a machine sends ends the test with the bytes sent.
- A deadline covers every state of a server once its first byte arrived.
- A reverse test in byte mode ends 10 s after its last data, because a lost close or end of the test would hold it for ever.

Interval reports follow the bytes. `received` closes an interval that is due before it counts, and holds the report for the next `poll`. The intervals of a stall are reported empty, and a test that ends inside an interval reports that last part before the final report, as the stock iperf prints them.

## Alternatives

- **Four public role machines.** Rejected: a driver must read the header before it knows the role.
- **An async core over embedded-io-async.** Rejected in the plan: UDP datagrams do not fit `Read` and `Write`, and a host test tool would need an executor.
- **A peer address in each `received`.** Rejected: it brings an address type into the core for a rule the driver's socket enforces anyway.
- **`f64` jitter.** Rejected for the cost the Context states.
- **Intervals closed only in `poll`.** It was the first version. A driver that reads before it polls counted bytes in the interval before the one they arrived in.
- **A close in the middle of a test as a protocol error.** It was the first version. A client in byte mode closes as soon as it has its bytes, which can be before the server's next poll, so a normal test ended in an error.

## Consequences

- A driver is a loop of four calls, and the tests drive the machines with bytes and times, without a socket.
- Every behaviour that depends on the order of events needs a test that produces that order, because the in-memory pair test polls each end in turn.
- A driver over IPv6 sets `ServerConfig::ipv6` for the length of a reverse test's datagrams, because the core does not know the address family.
