# End of a UDP test

Covers: how the sender of a UDP test tells the receiver that the test has ended, and the server report that answers it.

UDP has no connection to close, so iperf 2 marks the end of a test with a datagram whose packet ID is negative. [The iperf 2.2.2 source](../summaries/iperf2-2.2.2.md#the-end-of-a-udp-test) states each step.

## A forward test

1. After its last datagram of data, the client sends a datagram with packet ID `-N`, where `N` is one above the last ID of data. It is a full datagram, header included. Its bytes count toward the server's byte total.
2. The client waits up to 10 ms for a reply. After each silent wait it sends the datagram again with an ID one lower, up to 200 sends, about 2 s.
3. At the first negative ID, the server ends the test and sends the server report: 128 bytes with the byte total, the duration, the lost, out-of-order and datagram counts and the jitter.
4. The server keeps listening for 250 ms. Each datagram that arrives in that time earns another reply, up to 9 replies.
5. Without a reply, the stock client prints "did not receive ack of last datagram" and ends the test without the server's counts.

A stock 2.2.2 server sends its own report only the first time. Each later reply holds the first 128 bytes of the datagram it last read. Its `flags` are then the client's header flags, which lack `HEADER_VERSION1`, so our client ignores such a reply.

## A reverse test

The server is the sender. It sends one datagram of negative ID and does not wait for a reply. The client sends no server report.

## A stream without an end

A sender that never sends a negative ID, such as one without a test header, leaves a stock server waiting. The server ends the test only at its own `-t`, when it was started with one.

Requirements that use the term: W-3, W-11, P-3, P-4, P-6, P-12, P-15 and P-16.
