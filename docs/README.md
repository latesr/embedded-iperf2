# Docs

Covers: the index of the Architectural Decision Records (ADRs).

| ADR | Summary |
|---|---|
| [ADR-0001: The `wire` module](decisions/0001-wire-format.md) | Raw flag words, the `-C` fill in `header_length`, and zeros in the server report's transit statistics |
| [ADR-0002: The client and server state machines](decisions/0002-state-machines.md) | The shape of `Client` and `Server`, who filters the UDP peer, integer jitter, and how events in any order end a test |
| [ADR-0003: The `std` driver](decisions/0003-std-driver.md) | Who creates the client or server, blocking `std::net`, the read before each poll, and the UDP source filter |
| [ADR-0004: The `embassy-net` driver](decisions/0004-embassy-net-driver.md) | Who creates the socket, how the driver waits, why the stack refuses a second connection, how it ends a test, and the UDP send limit |
