# ADR-0002: The client and server state machines

**Status: accepted**

Covers: `Client` and `Server` in `embedded-iperf2`, and the modules under them: `config`, `report`, `counts`, `meter`, `stream`, and `datagram`.

## Context

The crate must run over any network stack, such as embassy-net on a board or the standard library on a host. It must not need threads or a heap, because a board often has neither. A stock iperf owns its sockets and gives each test a thread. That design ties the code to one network stack, and needs threads.

## Decision

- The core is a state machine that a driver calls. `poll` takes the time as a count of microseconds and returns the next action, and the driver reports what was sent, received, or closed. The caller lends the buffer that the core builds each send in. One core then runs over any network stack. The crate doc in `lib.rs` states the order of the calls.
- One `Client` and one `Server` cover TCP and UDP in both directions, because a driver cannot know whether a test sends or receives before it reads the header.
- The driver filters UDP datagrams by the peer's address, so the core does not hold an address type.
- The driver, not the core, keeps the datagram from a new flow that ends a test (P-27), and passes it to the `Server` of the next test. A `Server` holds one test from start to end.
- The jitter is kept in integer nanoseconds, because a board without a floating point unit pays for floating point on every datagram. The module doc of `counts.rs` states the formula.

## Rejected

- **An async core over embedded-io-async.** UDP datagrams do not fit `Read` and `Write`, and a host tool would need an executor.
- **Four public types, one for each role.** A driver would have to read the header before it could create the type that reads the header.
- **A thread and a socket for each UDP flow, as the stock server has.** A board runs one test at a time (P-14).
