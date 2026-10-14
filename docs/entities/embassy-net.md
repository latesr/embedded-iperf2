# embassy-net

Covers: embassy-net and the smoltcp release under it, which the `embassy` driver of `embedded-iperf2` runs on.

embassy-net is the async network stack of the Embassy project, at [embassy-rs/embassy](https://github.com/embassy-rs/embassy). It wraps smoltcp, a `no_std` TCP/IP stack. The driver uses embassy-net 0.9.1, which uses smoltcp 0.13.1.

## Observed

The tests in [`tests/embassy.rs`](../../embedded-iperf2/tests/embassy.rs) run two embassy-net 0.9.1 stacks in one process, over an in-memory Ethernet link. On 2026-10-14 they showed:

- A connection to a port whose one socket already holds a connection gets a RST, and `connect` returns `ConnectionReset`. See `a_tcp_server_refuses_a_second_connection_while_a_test_runs`.
- A UDP socket that is dropped right after its last `send_to` loses the datagrams still in its buffer. A reverse UDP test in byte mode never ended until the driver flushed the socket before it returned. See `every_mode_ends_with_both_reports_over_two_stacks`.
- A TCP socket that closes cleanly connects or accepts again, and no RST crosses the link. See `a_tcp_server_and_client_each_run_two_tests_on_one_socket_without_a_reset`.
- The first datagram of a UDP test, sent before ARP has resolved, arrives. See `every_mode_ends_with_both_reports_over_two_stacks`.
- Datagrams to an address that nobody on the link holds wait in the socket's buffer, and the socket reports no error for them. Once the buffer is full, `send_to` waits for room for as long as the address stays unresolved. See `a_udp_client_whose_datagrams_cannot_leave_ends_with_an_error_after_a_second`.
- A socket in TIME-WAIT that is dropped at once never acknowledges the peer's FIN. The peer waited in LAST-ACK until its own close timed out. The driver now keeps the socket 20 ms more. See `a_tcp_client_ends_with_its_report_when_the_server_closes_while_it_sends`.

## Read from the code

Each fact below was read in embassy-net 0.9.1 and smoltcp 0.13.1 on 2026-10-14.

### Reads and the peer's close

- [`TcpSocket::wait_read_ready`](https://docs.rs/crate/embassy-net/0.9.1/source/src/tcp.rs#507) is ready only when there are bytes to read. After the peer's FIN it never becomes ready.
- [`TcpSocket::read_with`](https://docs.rs/crate/embassy-net/0.9.1/source/src/tcp.rs#600) returns `Error::ConnectionReset` once the receive half is closed. It does so after a FIN and after a RST alike.
- [`UdpSocket::recv_from_with`](https://docs.rs/crate/embassy-net/0.9.1/source/src/udp.rs#177) returns only when a datagram arrives, and has no error.

### Closing and reusing a socket

- Dropping a [`TcpSocket`](https://docs.rs/crate/embassy-net/0.9.1/source/src/tcp.rs#467) or a [`UdpSocket`](https://docs.rs/crate/embassy-net/0.9.1/source/src/udp.rs#395) removes it from the stack, with whatever its buffers hold. It sends nothing more.
- smoltcp [delays an ACK by 10 ms](https://docs.rs/crate/smoltcp/0.13.1/source/src/socket/tcp.rs#296) by default. embassy-net's `TcpSocket` has no call that changes the delay. [`listen`](https://docs.rs/crate/smoltcp/0.13.1/source/src/socket/tcp.rs#911) resets the socket and its ACK timer, so a socket that accepts again at once loses an ACK it still owes, as a dropped socket does.
- [`UdpSocket::flush`](https://docs.rs/crate/embassy-net/0.9.1/source/src/udp.rs#330) waits until the send buffer is empty.
- smoltcp's [`is_open`](https://docs.rs/crate/smoltcp/0.13.1/source/src/socket/tcp.rs#1122) is false in `Closed` and in `TimeWait`. `listen` and `connect` refuse a socket only while it is open, so a socket in TIME-WAIT can accept or connect again.

### What the stack does without the driver

- smoltcp offers each TCP segment to each socket, and [answers with a RST](https://docs.rs/crate/smoltcp/0.13.1/source/src/iface/interface/tcp.rs#32) when none accepts it.
- When a packet's next hop has no known hardware address, smoltcp sends an ARP request instead. A socket's [`PacketBuffer::dequeue_with`](https://docs.rs/crate/smoltcp/0.13.1/source/src/storage/packet_buffer.rs#191) only removes the packet when the send succeeds, so the packet waits in the socket's buffer until ARP resolves.
