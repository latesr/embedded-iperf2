# ADR-0002: The client and server state machines

**Status: accepted**

Covers: `Client` and `Server` in `embedded-iperf2`, and the modules under them: `config`, `report`, `counts`, `meter`, `stream`, and `datagram`.

## Summary

- The core does not do I/O and does not keep time. The caller gives it the time in microseconds, writes what `poll` asks for, and passes in what the peer sent. The same core then runs on a board and on a host (D-5).
- One `Client` and one `Server` serve TCP and UDP, forward and reverse. A driver handles two types, and does not need to know the role of a test before it reads the header.
- The caller supplies the buffer that the core builds each chunk or datagram in. The core does not allocate memory (D-1).
- A sender fills its buffer once, then rewrites only the header bytes at its start for each send. A chunk or datagram then costs the sender almost nothing more than the socket's copy, so a board measures its network and not the sender.
- A UDP server sees the datagrams of every address until a test starts, and then only those of the client that started it. The driver enforces this rule, so the core does not hold an address or depend on the address type of a network stack (D-6).
- A datagram from another flow ends a UDP server's wait after the report, and that datagram starts the next test. Back-to-back tests from new ports then lose none of their start, as with the stock server (P-12, P-14, D-6).
- The jitter is kept in integer nanoseconds. A board without a floating point unit would otherwise pay for a floating point update on every datagram (P-17, P-18).
- Bytes count toward the interval they arrived in, not the interval of the next `poll`. A driver that reads before it polls then reports each interval as the stock iperf does (P-19).
- A close or a deadline in any state ends the test with a report of the bytes sent or received so far. Only a close before the whole TCP test header is an error. A client in byte mode closes as soon as it has its bytes, so a later close is a normal end (P-10, P-15, P-16, P-24).

## Context

A board runs the client or the server under embassy-net. A host runs them over the standard library, in a blocking loop or a thread. A design that owns its sockets, as the stock iperf does, needs one copy for each I/O model. The state of an iperf test is small: a header, some counters, a pacing clock, and a report. It fits a state machine that a driver calls.

A test has four roles: a TCP sender, a TCP receiver, a UDP sender, and a UDP receiver. A reverse test swaps the client's role with the server's role. A newcomer might make the four roles the public types. A driver must then pick a role from the test header before it can create the client or server that reads that header.

On a board, the time comes from embassy-time. On a host, it comes from `std::time::Instant`. At high datagram rates, the jitter needs a resolution below one microsecond. A board without a floating point unit (FPU) pays for each software floating point operation, and the jitter updates on every datagram.

The events reach the core in many orders. A driver can read before it writes, or read two times before it polls. A peer can close at any time, and the network can lose a close or an end of a test. The stock iperf handles each of these cases with a thread that blocks and has its own timeout.

## Decision

`Client::poll` and `Server::poll` return an `Action`: send bytes, wait until a time, report, close the write side, or done. `sent`, `received`, and `closed` update the client or server between polls. The roles are internal types that `Client` and `Server` hold in their states. A driver then handles two types, whatever the test is.

The caller passes a buffer to the constructor, and each `Send` borrows from it. A TCP chunk is as long as the buffer. A UDP datagram is `buf_len` bytes, and the constructor refuses a buffer that cannot hold one.

A sender writes the fill once. Before each send, it writes only the test or burst header of a TCP chunk, or the prefix and header of a UDP datagram. The stock iperf does the same.

A UDP server does not keep an address. The stock server connects its socket to the sender of the first datagram. A driver does the same, or drops the datagrams from other sources when its socket cannot disconnect. The core never names an address type.

Until a test starts, the driver passes the server the datagrams of every address. `Server::waits_for_first` tells the driver when to fix the address. A stock listener drops the end datagrams of old tests from any address in the same way ([`src/Listener.cpp:1030`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Listener.cpp#l1030)).

After its report, a UDP server waits for 250 ms of silence, and answers each repeat of the end of the test (P-12). The stock listener gives each flow a connected socket and a thread, then opens a new socket, so it serves a new flow during that wait ([`src/Listener.cpp:1092`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Listener.cpp#l1092)). This server serves one flow at a time (P-14). A driver that sees a datagram from another flow during the wait calls `Server::another_flow_arrived`. That call ends the test once the report has gone out. The driver keeps the datagram for the next test:

- The `std` driver leaves the datagram in the socket.
- The `embassy-net` driver copies the datagram to a stash that the caller lends, because embassy-net 0.9.1 cannot peek at a datagram.

The rule loses one case that the stock server handles. When the network loses the report, the old client sends its end again. Once a new flow has ended the wait, those ends reach the new test. The driver drops them, and the old client ends without the server report.

The jitter uses `J += (D - J) / 16` on `i64` nanoseconds. It can differ from the stock `double` in the last nanosecond. P-17 compares only the counts.

`received` and `closed` can arrive in any state. Each state uses the bytes or the close, or ignores them by a rule:

- A server counts the bytes that arrive before its ack is out.
- A reverse TCP client starts to read once its last header byte is sent, not at its next poll.
- A close while a client or a server sends ends the test with the bytes sent.
- A deadline covers every state of a server once its first byte has arrived.
- A reverse test in byte mode ends 10 s after its last data. A lost close or a lost end of the test would otherwise hold it for ever.

Interval reports follow the bytes. Before `received` counts its bytes, it closes an interval that is due, and holds the report for the next `poll`. The intervals of a stall are reported empty. A test that ends inside an interval reports that last part before the final report, as the stock iperf prints them.

## Alternatives

- **Four public role machines.** Rejected, because a driver must read the header before it knows the role.
- **An async core over embedded-io-async.** Rejected, because UDP datagrams do not fit `Read` and `Write`, and a host tool would need an executor.
- **A peer address in each `received`.** Rejected, because it adds an address type to the core for a rule that the driver enforces.
- **A thread and a socket for each UDP flow, as in the stock server.** Rejected, because a board runs one test at a time (P-14), and the core and its drivers do not have threads.
- **The driver answers the repeated ends of the old client while the next test runs.** The stock behaviour then stays the same when the network loses a report. Rejected, because it moves P-12 from the core into both drivers.
- **The wait after the report as part of the running test.** Rejected, because a test that starts within 250 ms of the last test loses its first datagrams. Back-to-back runs at 95 Mbit/s from an ESP32-S31 lost about 1,300 datagrams each.
- **`f64` jitter.** Rejected, because a board without an FPU pays for it on every datagram.
- **A fill that continues across the stream, written into each chunk.** Rejected, because the write costs the sender a pass over every chunk. No reader checks the fill. On the ESP32-S31, it held TCP upload to 29.5 Mbit/s, where a plain socket loop sends 57 Mbit/s.
- **Intervals closed only in `poll`.** Rejected, because a driver that reads before it polls counts bytes in the interval before the one they arrived in.
- **A close in the middle of a test as a protocol error.** Rejected, because a client in byte mode closes as soon as it has its bytes. A normal test then ends in an error.

## Consequences

- A driver is a loop of four calls. The tests drive the client and the server with bytes and times, without a socket.
- Every behaviour that depends on the order of events needs a test that makes that order, because the in-memory pair test polls each end in turn.
- A driver over IPv6 sets `ServerConfig::ipv6` for the length of a reverse test's datagrams, because the core does not know the address family.
