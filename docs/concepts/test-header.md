# Test header

Covers: the test header that an iperf 2 client sends at the start of a test, and how a server reads it.

The test header tells the server what the client wants: the direction, the length of the test, the datagram length and the rate. A stock iperf 2.2.2 client sends one in every test except with `-C`. [The iperf 2.2.2 source](../summaries/iperf2-2.2.2.md#the-test-header) states each field.

## Where it sits

- In a TCP test, the header is the first 64 bytes of the stream. The data follows it on the same connection.
- In a UDP test, the header follows the 16-byte prefix, and the client repeats it in every datagram. The server reads it from the first datagram of the flow.
- In a reverse test, the client sends the header and the server sends the data. A reverse UDP client sends one header datagram, and again each second until data arrives.

## The flags

The first word of the header is `flags`. Its high bits name the parts of the header that are present and the kind of test. Its low 16 bits hold the header length, shifted left by 1, when `HEADER_LEN_BIT` is set. A 2.2.2 client always sets that bit, so the length in a TCP header's flags is 64 and in a UDP header's flags is 80. The UDP length includes the prefix.

A server reads no header when `flags` have none of `HEADER_VERSION1`, `HEADER_VERSION2` and `HEADER_EXTEND`. That covers a stream that starts with zeros, and the fill pattern of a `-C` client. On TCP, the server then does not count the 4 bytes it read as `flags`. On UDP, the server reads 32-bit packet IDs, because `HEADER_SEQNO64B` is clear.

## The length of the test

`mAmount` holds the duration or the byte count. A duration is in units of 10 ms and is negated, so its top bit is set. A byte count has its top bit cleared. The server needs it only in a reverse test, where it is the sender.

## What the server sends back

A TCP server answers a forward test's header with the 28-byte header ack. A stock client reads the ack only with `-X`, and otherwise reads and discards it when the test ends. A TCP server answers a reverse test's header with data in bursts, each with a burst header.

Requirements that use the term: W-4 to W-8, P-1, P-5, P-6, P-8, P-9, P-11, P-13, P-16 and P-21.
