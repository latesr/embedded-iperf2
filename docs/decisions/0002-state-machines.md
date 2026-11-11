# ADR-0002: The client and server state machines

**Status: accepted**

Covers: `Client` and `Server` in `embedded-iperf2`, and the modules under them: `config`, `report`, `counts`, `meter`, `stream`, and `datagram`.

## Context

The crate must run over any network stack, such as embassy-net on a board or the standard library on a host. It must not need threads or a heap, because a board often has neither. A stock iperf owns its sockets and gives each test a thread. That design ties the code to one network stack, and needs threads.

## Decision

- The core is a state machine that a driver calls. `poll` returns the next action, and the driver reports what was sent, received, or closed. The caller supplies the time and the buffer, so one core runs under both drivers.
- One `Client` and one `Server` cover TCP and UDP in both directions, because a driver cannot know the role of a test before it reads the header.
- The driver filters UDP datagrams by the peer's address, so the core does not hold an address type.
- A close or a deadline in any state ends the test with a report of the bytes so far. A client in byte mode closes as soon as it has its bytes, so a close during a test is a normal end. Only a close before the whole TCP test header is an error.
- A UDP server serves one flow at a time (P-14). After its report, it waits for a short silence, and answers each repeat of the client's end of the test (P-12).
- A datagram from a new flow ends that wait at once, and starts the next test. Back-to-back tests then lose none of their start. The driver keeps the datagram for the next server.
- The jitter is kept in integer nanoseconds, because a board without a floating point unit pays for floating point on every datagram.

## Rejected

- **An async core over embedded-io-async.** UDP datagrams do not fit `Read` and `Write`, and a host tool would need an executor.
- **Four public types, one for each role.** A driver would have to read the header before it could create the type that reads the header.
- **A close in the middle of a test as a protocol error.** A normal test in byte mode then ends in an error.
- **A thread and a socket for each UDP flow, as the stock server has.** A board runs one test at a time (P-14).
- **The wait after the report as part of the running test.** A test that starts within that wait loses its first datagrams.

## Consequences

- When the network loses a server report, the old client sends its end of the test again. Once a new flow has ended the wait, those datagrams reach the new test, and the driver drops them. The old client then ends without the server report.
