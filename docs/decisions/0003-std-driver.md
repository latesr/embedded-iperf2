# ADR-0003: The `std` driver

**Status: accepted**

Covers: the `blocking` module of `embedded-iperf2`, behind the `std` feature.

## Context

On a host, the driver joins the core to `std::net` sockets. Host tools use it, and so do the tests against a stock iperf. Most host network tools in Rust use tokio.

## Decision

- The driver uses blocking sockets on the caller's thread. A tool runs one test at a time, so nothing needs to run in parallel, and the `std` feature does not need a runtime. A caller on tokio can run the driver with `spawn_blocking`.
- The caller creates the `Client` or `Server` and passes it in. A bad setting then fails in `new`, and the driver returns only socket errors and the result of the test.
- The driver reads without blocking before every `poll`. A sender without a rate limit never waits, so a driver that reads only during a wait misses the peer's close. A TCP server also misses a second connection, which it must refuse (P-14). The module doc of `blocking.rs` gives the timeouts of the loop.
- A UDP server takes the source of the first datagram as its client, and drops the datagrams from other sources until the test ends. `std` cannot disconnect a UDP socket, so a connected socket could serve only its first client.
- A server can limit its wait for a test to start. The limit is a setting on the server, not a parameter of `run_one`, so callers that do not use it do not change.
- With a limit, a TCP server polls a non-blocking accept. `std` does not have an accept with a timeout, and nothing can wake a blocking accept.

## Rejected

- **An async driver on tokio.** It adds a dependency for every user of the `std` feature, and nothing runs in parallel.
- **A second thread that reads.** One read before each poll does the same work.
- **A deadline as a parameter of `run_one`, or a second method that takes one.** A parameter breaks every caller, and a second method does the same work as the first.
- **An accept with a timeout from `socket2`.** It adds a dependency for a wait before a test starts.
