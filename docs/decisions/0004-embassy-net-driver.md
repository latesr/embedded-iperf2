# ADR-0004: The `embassy-net` driver

**Status: accepted**

Covers: the `embassy` module of `embedded-iperf2`, behind the `embassy-net` feature.

## Context

On a board, the driver joins the core to the TCP and UDP sockets of embassy-net. embassy-net 0.9.1 and smoltcp 0.13.1 behave differently from `std::net` in ways that change the driver's loop. The tests in `tests/embassy.rs` show each behaviour below.

## Decision

- The caller creates the socket and passes it in. The caller then sizes the stack and every buffer, and the driver holds none.
- The driver's wait is a read that ends at the core's deadline. [`wait_read_ready`](https://docs.rs/crate/embassy-net/0.9.1/source/src/tcp.rs#507) never wakes after the peer closes, and [`read_with`](https://docs.rs/crate/embassy-net/0.9.1/source/src/tcp.rs#600) returns at once after a FIN or a RST.
- After a TCP test, the driver closes the connection, then waits for the peer's close and for the acknowledgement of its own close. A socket dropped earlier loses the acknowledgement, and the peer waits for it. The rustdoc of `embassy.rs` gives each limit.
- After a UDP test, the driver waits until the socket has sent every datagram. A socket dropped earlier loses the end of the test, and the peer waits for it.
- A UDP server takes a stash buffer from the caller. When a datagram from a new flow ends the wait after a report, the driver keeps that datagram in the stash, and gives it to the next server as its first datagram. The driver must take the datagram out of the socket, because embassy-net 0.9.1 cannot peek at one.
- The driver, not the core, bounds a UDP send (D-8). The core counts its sends, and does not hold a limit on one.

## Rejected

- **The driver creates the socket.** It needs a public type that holds up to four buffers.
- **Drop or abort the socket at the end of a test.** The peer then waits for what the socket held, and an abort sends a reset after every test.
