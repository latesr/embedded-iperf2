# ADR-0003: The `std` driver

**Status: accepted**

Covers: the `blocking` module of `embedded-iperf2`, behind the `std` feature.

## Summary

- The caller creates the `Client` or `Server` and passes it to the driver. An invalid setting fails in `new`, so the driver reports only socket errors and the result of the test (D-2).
- The driver uses blocking `std::net` sockets, and runs on the caller's thread. It does not start a thread, and does not need an async runtime (D-2).
- The driver reads the socket for new data before every call to `poll`. A client that sends as fast as it can then sees the server close the connection, and a TCP server sees a new connection (P-14, P-24).
- A UDP server ignores the datagrams from any address other than the address of its current client. It checks the source of each datagram, so the next test can come from a different client (D-6).
- A server can limit how long `run_one` waits for a test to start. A TCP server with a limit polls a non-blocking accept, so the `std` feature does not need another dependency (D-7).

## Context

The core does not do I/O and does not keep time. It returns an action from `poll`, such as a send or a wait until a time, and the caller passes in what the peer sent. On a host, the driver connects the core to a `std::net` socket. The driver reads and writes the socket, waits when the core asks it to, and gives the core the time. Host tools such as `mcu-run` use the driver, and so do the tests against a stock iperf (T-5).

The driver could take a `Config` and create the client or server. But `Client::new` and `Server::new` can refuse a setting, such as a buffer too small for a datagram. The driver would then return that error together with its socket errors, and every caller would handle it on every run.

Most host network tools in Rust use tokio. This driver does not need it. The core decides when to send and when to wait, so the driver only blocks on one socket. A tool runs one test at a time, so nothing needs to run in parallel. tokio would also be a dependency of every user of the `std` feature. A caller that uses tokio can run the driver with `spawn_blocking`.

The simplest driver reads only during a wait. Most tests work with it, because the core waits whenever it paces its sends. A TCP sender without a rate limit never waits, so that driver never reads while it sends. It then misses the peer's close, and misses a second client that the server must refuse (P-14).

The stock server connects its UDP socket to its first client, and the kernel then drops the datagrams from other addresses. `std` cannot disconnect a UDP socket, so a server that connects can only serve its first client.

A server waits for its client without a limit. A test tool that runs the server on a thread then keeps a blocked thread when the board under test never connects. A tool that serves tests until Ctrl-C needs `run_one` to return from time to time, so the tool can stop. `std` does not have an accept with a timeout, and nothing can wake a blocking accept:

- `TcpListener` does not have a shutdown.
- A connection from the server's own address, to wake the accept, starts a test.

## Decision

`run_client`, `TcpServer::run_one`, and `UdpServer::run_one` take the client or server by `&mut`. Each blocks on the caller's thread until the test ends, and returns `io::Result<Result<(), TestError>>`. The outer error is the socket's error, and the inner one is the test's error.

Each turn of the driver's loop reads what has arrived without blocking, then calls `poll`. When the core waits, the driver reads with a timeout instead of a sleep. While a test runs, the TCP server accepts and closes one waiting connection on each turn. If it closed all of them, clients that connect faster than the loop turns would stall the test. The rustdoc of `blocking.rs` gives the timeouts, and how each socket error reaches the core.

The UDP server reads with `recv_from` and replies with `send_to`. It takes the source of the first datagram as its client, and drops the datagrams from other sources until the test ends.

`set_start_timeout` on each server limits the wait for a test to start. `run_one` then fails with `TimedOut`. The limit is a setting, as the socket timeouts of `std` are, so `run_one` keeps one signature.

A TCP server with a limit makes its listener non-blocking, and polls `accept` each millisecond. Without a limit, it keeps the blocking accept. The poll can accept a connection up to 1 ms late. The server's clock starts at the first bytes that it reads, so the clock can start up to 1 ms late. A UDP server already waits with a read timeout. While the core waits for the first datagram, the server caps the timeout at the deadline. A test that has started ends only by the limits of the core.

## Alternatives

- **The driver creates the client or server from a `Config`.** Rejected, because every caller must then handle a settings error on every run.
- **An async driver on tokio.** Rejected, because it adds a dependency, and nothing needs to run in parallel.
- **Read only while the core waits.** Rejected, because a sender without a rate limit misses the peer's close and a second client. A reverse TCP test at rate 0 shows the fault.
- **A second thread that reads.** Rejected, because one read before each poll does the same work without a thread and a channel.
- **A connected UDP socket.** Rejected, because `std` cannot disconnect it.
- **An accept with a timeout from `socket2`.** Rejected, because it adds a dependency to the `std` feature, and the poll costs at most 1 ms before a test.
- **A deadline as a parameter of `run_one`, or a second method that takes one.** Rejected, because a parameter breaks every caller, and a second method does the same work as the first.

## Consequences

- A tool runs a test with one call for each end, and prints each interval report from `on_report` as it arrives.
- The caller sets `ServerConfig::ipv6`. The driver refuses a server with an `ipv6` that does not match its socket, so it never sends datagrams of the wrong length.
