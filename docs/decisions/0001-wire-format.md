# ADR-0001: The `wire` module

**Status: accepted**

Covers: the `wire` module of `embedded-iperf2`: the parsers and encoders of the UDP prefix, the test header, the header ack, the burst header, and the server report.

## Context

The test header of iperf 2.2.2 holds the mode of a test in its flag bits. A v1 server serves some modes and must refuse the others (P-13). It sends a header ack for some combinations of bits and not for others (P-9). The server report holds 60 bytes of transit statistics, which a stock client reads only when the report's `flags` have `EXTEND` set.

## Decision

- A parser checks only that the bytes form the message, such as a length within bounds. It does not check which mode a test header asks for.
- `TestHeader` keeps its three flag words as plain numbers, and the server reads them. A header that asks for a mode outside v1 is then a valid header that the server refuses, not corrupt bytes. The server also needs the raw bits for the stock ack rule, which reads bits such as `V2PEERDETECT` that no v1 mode uses.
- `header_length` reads the fill of a UDP client run with `-C` as no header, as the stock server does ([`src/Listener.cpp:1402`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Listener.cpp#l1402)). The check is in the parser, so a caller cannot forget it.
- The server report does not set `EXTEND`, and holds zeros in the transit statistics, so a board does not compute statistics that nothing reads.

## Rejected

- **An enum of the v1 modes in `TestHeader`.** The parser then refuses an unsupported mode with the error for corrupt bytes, and the ack rule needs bits that no v1 mode names.
- **The `-C` check in each UDP caller.** Every caller must then repeat it.
