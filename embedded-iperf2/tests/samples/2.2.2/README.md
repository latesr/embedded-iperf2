# iperf 2.2.2 samples

Covers: the samples that `cargo xtask record` recorded from `iperf 2.2.2 (29 September 2026) pthreads (community)`.

Each case has three files:

- `<case>.bin` holds the bytes that the relay saw. The rustdoc of `xtask/src/record/sample.rs` states its layout.
- `<case>.client.txt` holds what the client printed.
- `<case>.server.txt` holds what the server printed.

The server ran as `iperf -s -p <port>`, with `-u` for UDP. The client ran as `iperf -c 127.0.0.1 -p <relay port>`, with `-u` for UDP, then the arguments in the table.

| Case | Transport | Client arguments |
|---|---|---|
| `tcp` | TCP | `-n 64K -l 16K` |
| `tcp-reverse` | TCP | `-n 64K -l 16K -R` |
| `tcp-time` | TCP | `-t 2 -i 1 -l 16K -b 256K` |
| `udp` | UDP | `-t 2 -i 1 -l 512 -b 256K` |
| `udp-reverse` | UDP | `-t 2 -i 1 -l 512 -b 256K -R` |
| `udp-bytes` | UDP | `-n 16K -l 512 -b 256K` |
