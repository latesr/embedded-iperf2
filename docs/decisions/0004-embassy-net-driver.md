# ADR-0004: The `embassy-net` driver

**Status: accepted**

Covers: the `embassy` module of `embedded-iperf2`, behind the `embassy-net` feature.

## Context

On a board, the driver joins the core to the TCP and UDP sockets of embassy-net. embassy-net 0.9.1 and smoltcp 0.13.1 behave differently from `std::net` in ways that change the driver's loop. The tests in `tests/embassy.rs` show each behaviour below.

## Decision

- The caller creates the socket and passes it in. The caller then sizes the stack and every buffer, and the driver holds none.
- The driver waits with a read against the core's deadline. [`wait_read_ready`](https://docs.rs/crate/embassy-net/0.9.1/source/src/tcp.rs#507) never wakes after the peer closes, and [`read_with`](https://docs.rs/crate/embassy-net/0.9.1/source/src/tcp.rs#600) returns at once after a FIN or a RST.
- The driver does not refuse a second TCP connection (P-14). embassy-net does not have a listener, and while the server's one socket holds a connection, the stack [answers another with a RST](https://docs.rs/crate/smoltcp/0.13.1/source/src/iface/interface/tcp.rs#32).
- After a test, the driver closes the TCP connection and waits for the peer's close and the acknowledgement of its own. It flushes a UDP socket. A socket dropped earlier loses what it still holds, such as the end of a UDP test, and the peer waits for it. The rustdoc of `embassy.rs` gives each limit.
- A UDP server takes a stash from the caller. The stash holds the datagram from a new flow that ends the wait after a report, as the first datagram of the next test. embassy-net 0.9.1 cannot peek at a datagram, so the driver must take it out of the socket.
- A UDP send that finds no space within a limit ends the run with `SendTimedOut`. A datagram [stays in the buffer](https://docs.rs/crate/smoltcp/0.13.1/source/src/storage/packet_buffer.rs#191) without an error until ARP resolves, and the core ends a UDP test only by a count of its sends. Without the limit, a run with the link down never ends.

## Rejected

- **The driver creates the socket.** It needs a public type that holds up to four buffers.
- **Drop or abort the socket at the end of a test.** The peer then waits for what the socket held, and an abort sends a reset after every test.
- **A send that timed out counted as not sent, or as lost.** The first holds the test for ever, and the second reports bytes that never left.
