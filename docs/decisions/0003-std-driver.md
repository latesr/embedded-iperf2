# ADR-0003: The `std` driver

**Status: accepted**

Covers: the `blocking` module of `embedded-iperf2`, behind the `std` feature.

## Summary

- The caller creates the `Client` or `Server` and passes it to the driver. An invalid setting fails in `new`, so the driver only needs to report socket errors and the result of the test (D-2).
- The driver uses ordinary blocking `std::net` sockets and runs on the caller's thread. It doesn't start a thread, and it doesn't need an async runtime (D-2).
- The driver checks the socket for new data before every call to `poll`. This ensures a client that sends as fast as possible still sees the server close the connection, and a TCP server sees new connections (P-14, P-24).
- A UDP server ignores datagrams from any address other than its current client's. It checks the source of each datagram itself, so the next test can come from a different client (D-6).

## Context

The core doesn't do I/O and doesn't keep time (ADR-0002). On a host, the driver connects the core to a `std::net` socket. It reads and writes the socket, waits when the core asks it to, and gives the core the time. Host tools such as `mcu-run` use it, and so do the tests against a stock iperf (T-5).

The driver could take a `Config` and create the client or server itself. But `Client::new` and `Server::new` can refuse a setting, such as a buffer too small for a datagram. The driver would then have to return that error alongside its socket errors, and every caller would have to handle it on every run.

Most networked host tools in Rust use tokio. This driver doesn't need it. The core decides when to send and when to wait, so the driver only ever blocks on one socket. A tool runs one test at a time, so nothing needs to run in parallel. tokio would also become a dependency for everyone who uses the `std` feature. A caller that uses tokio can still run the driver with `spawn_blocking`.

The simplest driver only reads when the core asks it to wait. That's enough for most tests, because the core waits whenever it paces its sends. But a TCP sender without a rate limit never waits, so that driver would never read while it sends. It would miss the peer closing the connection, and it would miss a second client that P-14 says the server must refuse.

The stock server connects its UDP socket to its first client, and the kernel then drops datagrams from other addresses. `std` can't disconnect a UDP socket, so a server that connected could only ever serve its first client.

## Decision

`run_client`, `TcpServer::run_one` and `UdpServer::run_one` take the client or server by `&mut`, and block on the caller's thread until the test ends. Each returns `io::Result<Result<(), TestError>>`. The outer error is the socket's, and the inner one is the test's.

Each turn of the driver's loop reads whatever has arrived without blocking, then calls `poll`. When the core waits, the driver reads with a timeout instead of sleeping. While a test runs, the TCP server accepts and closes one waiting connection per turn. Closing all of them could stall the test if clients connect faster than the loop turns. The rustdoc of `blocking.rs` gives the timeouts and how each socket error reaches the core.

The UDP server reads with `recv_from` and replies with `send_to`. It takes the source of the first datagram as its client, and drops datagrams from other sources until the test ends.

## Alternatives

- **The driver creates the client or server from a `Config`.** Rejected, because every caller would have to handle a settings error on every run.
- **An async driver on tokio.** Rejected, because it adds a dependency and nothing needs to run in parallel.
- **Only read while the core waits.** Rejected, because a sender without a rate limit misses the peer closing and a second client connecting. A reverse TCP test at rate 0 shows it.
- **A second thread that reads.** This works, but it adds a thread and a channel to do what one read before each poll already does.
- **A connected UDP socket.** Rejected, because `std` can't disconnect it.

## Consequences

- A tool runs a test with one call per end, and prints each interval report from `on_report` as it arrives.
- The caller sets `ServerConfig::ipv6`. The driver refuses a server whose `ipv6` doesn't match its socket, so it never sends datagrams of the wrong length.
