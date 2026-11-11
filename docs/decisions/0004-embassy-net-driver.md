# ADR-0004: The `embassy-net` driver

**Status: accepted**

Covers: the `embassy` module of `embedded-iperf2`, behind the `embassy-net` feature.

## Summary

- The caller creates the socket and passes it to the driver, with one client function for each transport. The socket holds the stack and every buffer, so the caller sizes them all and the driver holds none (D-3).
- The driver reads before each poll, and its wait is a read against the core's deadline. A read is the one wait that ends when the peer closes, so a client that waits for the server's close sees it (P-24).
- The driver leaves a second connection to the stack. While the server's one socket holds a connection, nothing listens on the port, and the stack refuses the connection (P-14).
- After a TCP test, the driver closes the connection. After a UDP test, it waits for the stack to send what the socket holds. It then returns, so the peer gets the close or the end of the test, and the caller can reuse or drop the socket (P-3, P-21).
- A UDP send that finds no space in the socket's buffer within a limit ends the run with an error. The core ends a UDP test by a count of its sends, so only the driver can end a run when the sends never leave (P-3).

## Context

The core does not do I/O and does not keep time. It returns an action from `poll`, such as a send or a wait until a time, and the caller passes in what the peer sent. On a board, the `embassy` module connects the core to the TCP and UDP sockets of embassy-net. The caller builds an embassy-net socket from a stack and its own buffers, two for TCP and four for UDP.

The `std` driver reads the socket without blocking before each poll, and waits with a read timeout. embassy-net 0.9.1 and smoltcp 0.13.1 behave differently from `std::net` in ways that change this loop. The tests in `tests/embassy.rs` run two stacks over an in-memory link, and show each behaviour below.

A newcomer's first wait is [`wait_read_ready`](https://docs.rs/crate/embassy-net/0.9.1/source/src/tcp.rs#507), raced against a timer. It wakes only when there are bytes to read. After the peer closes, no bytes arrive, so a client that waits for the server's close waits for ever. [`read_with`](https://docs.rs/crate/embassy-net/0.9.1/source/src/tcp.rs#600) returns `ConnectionReset` at once after a FIN or a RST.

The `std` driver accepts and closes each extra connection, because the kernel queues connections on a listener. embassy-net does not have a listener. A TCP socket accepts one connection. When no socket accepts a connection, the stack [answers with a RST](https://docs.rs/crate/smoltcp/0.13.1/source/src/iface/interface/tcp.rs#32).

Dropping a [`TcpSocket`](https://docs.rs/crate/embassy-net/0.9.1/source/src/tcp.rs#467) or a [`UdpSocket`](https://docs.rs/crate/embassy-net/0.9.1/source/src/udp.rs#395) removes it from the stack, with everything still in it:

- A UDP socket dropped right after its last send loses the end of the test. [`UdpSocket::flush`](https://docs.rs/crate/embassy-net/0.9.1/source/src/udp.rs#330) waits until the send buffer is empty.
- A TCP socket dropped in TIME-WAIT loses the acknowledgement of the peer's close. smoltcp [delays an acknowledgement by 10 ms](https://docs.rs/crate/smoltcp/0.13.1/source/src/socket/tcp.rs#296), and embassy-net 0.9.1 cannot turn the delay off.
- smoltcp's [`listen`](https://docs.rs/crate/smoltcp/0.13.1/source/src/socket/tcp.rs#911) resets the acknowledgement timer, so a socket that accepts again at once also loses the acknowledgement.

In each case, the peer waits for something that never comes. A socket in TIME-WAIT is [not open](https://docs.rs/crate/smoltcp/0.13.1/source/src/socket/tcp.rs#1122), so it can accept or connect again.

`send_to` waits for space in the socket's send buffer. When the next hop does not have a known hardware address, a datagram [stays in the buffer](https://docs.rs/crate/smoltcp/0.13.1/source/src/storage/packet_buffer.rs#191) until ARP resolves, without an error. With the link down or the peer not resolved, nothing leaves, and the wait never ends. The core cannot end the test either. Its UDP end of test sends a fixed number of datagrams, and only a reported send moves it on.

## Decision

`run_tcp_client` and `run_udp_client` take the client and the caller's socket. `TcpServer` and `UdpServer` hold the caller's socket between tests, and `run_one` runs one test. Each returns `Result<Result<(), TestError>, Error>`. Before any traffic, `Error::Mismatch` refuses a socket or a setting that does not fit the test. The rustdoc of each function lists the cases. One case is a server's `ipv6`, because the driver uses IPv4 (D-4).

Each turn of the loop polls the read one time without a wait, then calls `poll`. A wait races the read against the core's deadline. A TCP write waits for space only briefly, so the loop continues to read while the send buffer is full. A UDP send waits longer, then the run returns `Error::SendTimedOut`.

After a TCP test, the driver closes the socket, reads until the peer closes, and waits for the acknowledgement of its own close. Each step has a limit. A socket that closed first stays a little longer, so the stack sends the acknowledgement that it delays. The driver aborts any other socket. After a UDP test that did not fail, the driver flushes the socket. The rustdoc of `embassy.rs` gives each limit.

## Alternatives

- **The driver creates the socket from the stack and the buffers.** Rejected, because it needs a public type that holds up to four buffers, and the caller already knows how to build a socket.
- **One `run_client` that takes an enum of the two sockets.** Rejected, because each function would then check the transport two times, in the enum and in the settings.
- **Wait with `wait_read_ready`.** Rejected, because it never wakes after the peer closes.
- **Refuse extra connections in the driver, as the `std` driver does.** Rejected, because the stack already refuses them, and there is no listener to accept them from.
- **Drop or abort the socket at the end of a test.** Rejected, because the peer then waits for the end of a UDP test, or for the acknowledgement of its close. An abort also sends a reset after every test.
- **Treat a UDP send that timed out as not sent, or as lost.** Rejected, because the core ends a UDP test by a count of its sends. A send that is not reported holds the test for ever. A send counted as lost puts bytes that never left in the report (P-20).

## Consequences

- A board runs a test with one call for each end, on sockets that it can reuse for the next test.
- When the link goes down during a UDP test, a board gets `SendTimedOut`, and not a task that never returns.
- The wait for the delayed acknowledgement can go when embassy-net exposes the acknowledgement delay setting of smoltcp.
