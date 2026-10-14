# Docs

Covers: the index of every page under `docs/`.

| Page | Type | Summary |
|---|---|---|
| [iperf2](entities/iperf2.md) | Entity | Where the pinned 2.2.2 comes from, and what it was observed to send |
| [embassy-net](entities/embassy-net.md) | Entity | What the `embassy` driver relies on in embassy-net 0.9.1 and smoltcp 0.13.1: reads after a close, reusing a socket, and what the stack refuses by itself |
| [The iperf 2.2.2 source](summaries/iperf2-2.2.2.md) | Summary | The claims taken from the source: the layouts, the flags, the end of a test and the counts |
| [Test header](concepts/test-header.md) | Concept | What a client sends at the start of a test, and how a server reads it |
| [End of a UDP test](concepts/udp-end-of-test.md) | Concept | The negative packet ID that ends a UDP test, and the server report that answers it |
| [Loss, out-of-order and jitter](concepts/loss-and-jitter.md) | Concept | How a UDP receiver counts from the packet IDs and send times |
| [ADR-0001: The `wire` module](decisions/0001-wire-format.md) | ADR | Raw flag words, the `-C` fill in `header_length`, and zeros in the server report's transit statistics |
| [ADR-0002: The client and server state machines](decisions/0002-state-machines.md) | ADR | The shape of `Client` and `Server`, who filters the UDP peer, integer jitter, and how events in any order end a test |
| [ADR-0003: The `std` driver](decisions/0003-std-driver.md) | ADR | Who creates the client or server, blocking `std::net`, the read before each poll, and the UDP source filter |
| [ADR-0004: The `embassy-net` driver](decisions/0004-embassy-net-driver.md) | ADR | Who creates the socket, how the driver waits, why the stack refuses a second connection, how it ends a test, and the UDP send limit |
| [Findings not yet fixed](todo.md) | Findings | What a piece of work saw and did not fix, with the commit that found it |
