# ADR-0001: The `wire` module

**Status: accepted**

Covers: the `wire` module of `embedded-iperf2`: the parsers and encoders of the UDP prefix, the test header, the header ack, the burst header, and the server report.

## Summary

- The test header keeps its three flag words as plain numbers. The server reads the bits, so it can tell a mode that it does not serve from corrupt bytes, and apply the stock server's ack rule (P-9, P-13).
- A parser refuses only bytes that cannot be the message, such as a header length out of bounds. Every other rule belongs to the server, so a parser never refuses a test that the server could answer (W-7, W-9, W-12).
- `header_length` reads the fill of a UDP client run with `-C` as no header, as the stock server does. The parser makes that check, so a caller cannot forget it (P-11).
- The server report sets `flags` without `EXTEND`, and writes zeros in the transit statistics. A stock client reads those statistics only when `EXTEND` is set, so it never sees the zeros (W-11).
- Every encoder writes the whole message, fill included. The bytes on the wire then do not depend on what the buffer held before.

## Context

The test header of iperf 2.2.2 holds a mode in its flag bits. A v1 server serves some of the modes, and must refuse the others (P-13). It sends a header ack for some combinations of bits and not for others (P-9).

A newcomer's first design is an enum of the v1 modes, which the parser builds. The parser then refuses every header outside v1 with the error that it gives for corrupt bytes. The server cannot tell a client that it does not serve from a broken stream. It also cannot apply the stock server's ack rule, because that rule reads bits that no v1 mode names, such as `V2PEERDETECT`.

The server report has 60 bytes of transit statistics after the counts. A stock client reads them only when the report's `flags` have `EXTEND` set. A stock server sets `EXTEND` only with `-e`. P-20 reports the counts, the duration, and the jitter.

A UDP client run with `-C` does not send a header, and the fill pattern is where `flags` would be. That word, 0x36373839, sets `VERSION2` and `LEN_BIT`, and states a length of 7196 bytes. The stock server checks for it before it reads any bit ([`src/Listener.cpp:1402`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Listener.cpp#l1402)).

## Decision

The parsers read fields, and check only what makes the bytes unreadable. `TestHeader` keeps the flag words raw, with `is_reverse` and `header_length` as helpers. The rules for which tests run belong to the `Server` state machine, which sees every bit.

`header_length` takes the transport, because the lower bound of W-7 is 64 bytes for TCP and 80 bytes for UDP. For UDP, it returns no header for the `-C` fill.

`ServerReport` holds the counts, the duration, and the jitter. Its encoder writes `VERSION1 | SEQNO64B` and zeros in the transit statistics. Its parser reads the high words of the counts only when `SEQNO64B` is set, as a stock client does.

## Alternatives

- **An enum of v1 modes in `TestHeader`.** Rejected, because the server cannot tell a mode that it does not serve from corrupt bytes, and the ack rule needs bits that no v1 mode names.
- **The `-C` check in each UDP caller.** Rejected, because every caller must then repeat it.
- **Computing the transit statistics.** Rejected, because a stock client does not read them without `EXTEND`. A board would keep four statistics for each test without a reader.

## Consequences

- The `Server` state machine applies P-9 and P-13 to the raw bits, and has its own tests for them.
- If the server ever sets `EXTEND`, a stock client run with `-e` prints zeros for the transit statistics. v1 does not set it.
- A reader of `TestHeader` must check `EXTEND` to match the stock server exactly, because `is_reverse` does not check it.
