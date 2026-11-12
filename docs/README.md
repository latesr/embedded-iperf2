# Docs

Covers: the index of the Architectural Decision Records (ADRs).

| ADR | Summary |
|---|---|
| [ADR-0001: The `wire` module](decisions/0001-wire-format.md) | Parsers that check only the bytes, raw flag words, and the `-C` fill in `header_length` |
| [ADR-0002: The client and server state machines](decisions/0002-state-machines.md) | The shape of `Client` and `Server`, who filters the UDP peer and keeps the next test's datagram, and integer jitter |
| [ADR-0003: The `std` driver](decisions/0003-std-driver.md) | Who creates the client or server, blocking `std::net`, the read before each poll, the peek at each UDP source, and the start timeout |
| [ADR-0004: The `embassy-net` driver](decisions/0004-embassy-net-driver.md) | Who creates the socket, how the driver waits, how it ends a test, the UDP stash, and who bounds a UDP send |
