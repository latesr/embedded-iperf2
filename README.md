# embedded-iperf2

An iperf 2 client and server for microcontrollers, wire compatible with iperf 2.2.2. A board runs a test against a stock `iperf` on a host, over TCP or UDP, in either direction.

The core is `no_std` and does not allocate memory. `Client` and `Server` are sans-I/O state machines: the caller moves the bytes and supplies the time. Two drivers behind cargo features do the I/O:

| Feature | Module | Runs on |
|---|---|---|
| `embassy-net` | `embassy` | A board, over the TCP and UDP sockets of an embassy-net stack |
| `std` | `blocking` | A host, over the standard library's blocking sockets |

The `embassy-net` caller supplies the stack and every buffer. The `std` driver runs on the caller's thread.

[`REQUIREMENTS.md`](REQUIREMENTS.md) states what the crate must do, and what v1 leaves out. [`docs/`](docs/README.md) holds the Architectural Decision Records (ADRs).

## The stock iperf

The tests compare the crate with a stock iperf 2.2.2. A package manager's `iperf` is another version. To build the pinned version into `~/.local/opt/iperf/2.2.2/iperf`, run:

```
cargo xtask install-iperf
```

The build needs `git`, a C and C++ compiler, and `make`. Then set `MCU_IPERF2` to the path that the command prints.

## Tests

`cargo xtask check` runs the checks of a change: the links and the prose, the format, the lints, the tests, and the docs. `cargo xtask check --full` runs every check that CI runs. The interoperability tests need `MCU_IPERF2`, and fail without it.

## Licence

Users may choose either the [MIT licence](LICENSE-MIT) or the [Apache-2.0 licence](LICENSE-APACHE).
