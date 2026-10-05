# Docs

Covers: the index of every page under `docs/`.

| Page | Type | Summary |
|---|---|---|
| [iperf2](entities/iperf2.md) | Entity | Where the pinned 2.2.2 comes from, and what it was observed to send |
| [The iperf 2.2.2 source](summaries/iperf2-2.2.2.md) | Summary | The claims taken from the source: the layouts, the flags, the end of a test and the counts |
| [Test header](concepts/test-header.md) | Concept | What a client sends at the start of a test, and how a server reads it |
| [End of a UDP test](concepts/udp-end-of-test.md) | Concept | The negative packet ID that ends a UDP test, and the server report that answers it |
| [Loss, out-of-order and jitter](concepts/loss-and-jitter.md) | Concept | How a UDP receiver counts from the packet IDs and send times |
| [Findings not yet fixed](todo.md) | Findings | What a piece of work saw and did not fix, with the commit that found it |
