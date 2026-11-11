# ADR-0001: The `wire` module

**Status: accepted**

Covers: the `wire` module of `embedded-iperf2`: the parsers and encoders of the UDP prefix, the test header, the header ack, the burst header and the server report.

## Summary

- The test header keeps its three flag words as plain numbers. The server reads the bits itself, so it can tell a mode it doesn't serve from corrupt bytes, and apply the stock server's ack rule (P-9, P-13).
- A parser only refuses bytes that can't be the message, such as a header length out of bounds. Every other rule belongs to the server, so a parser never refuses a test the server could have answered (W-7, W-9, W-12).
- `header_length` treats the fill of a UDP client run with `-C` as no header, as the stock server does. The parser makes that check, so a caller can't forget it (P-11).
- Our server report sets `flags` without `EXTEND` and writes zeros for the transit statistics. A stock client only reads those statistics when `EXTEND` is set, so it never sees the zeros (W-11).
- Every encoder writes the whole message, fill included. The bytes on the wire don't depend on what the buffer held before.

## Context

The test header of iperf 2.2.2 holds a mode in its flag bits. A v1 server serves a few of the modes and must refuse the rest (P-13). It sends a header ack for some combinations and not others (P-9).

A newcomer's first design is an enum of the v1 modes, built by the parser. The parser would then refuse every header outside v1 with the error it gives for corrupt bytes. The server couldn't tell a client it doesn't serve from a broken stream. It also couldn't apply the stock server's ack rule, because that rule looks at bits no v1 mode names, such as `V2PEERDETECT`.

The server report has 60 bytes of transit statistics after the counts. A stock client reads them only when the report's `flags` has `EXTEND`, which a stock server sets only with `-e`. The counts, the duration and the jitter are what P-20 reports.

A UDP client with `-C` sends no header, and the fill pattern lands where `flags` would be. That word, 0x36373839, sets `VERSION2` and `LEN_BIT` and states a length of 7196 bytes. The stock server checks for it before it reads any bit ([`src/Listener.cpp:1402`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Listener.cpp#l1402)).

## Decision

The parsers read fields and only check what makes the bytes unreadable. The flag words stay raw in `TestHeader`, with `is_reverse` and `header_length` as helpers. The rules for which tests run belong to the `Server` state machine (ADR-0002), which sees every bit.

`header_length` takes the transport, because W-7's lower bound is 64 for TCP and 80 for UDP. For UDP it returns no header for the `-C` fill, so a caller can't forget the check that the stock server makes first.

`ServerReport` holds the counts, the duration and the jitter. Its encoder writes `VERSION1 | SEQNO64B` and zeros in the transit statistics. Its parser reads the high words of the counts only with `SEQNO64B`, as a stock client does.

## Alternatives

- **An enum of v1 modes in `TestHeader`.** Rejected, because the server couldn't tell an unserved mode from corrupt bytes, and the ack rule needs bits no v1 mode names.
- **The `-C` check in each UDP caller.** Rejected, because every caller would have to repeat it. It was the first version, moved into `header_length` on 2026-10-05.
- **Computing the transit statistics.** Rejected, because a stock client doesn't read them without `EXTEND`, and a board would keep four statistics per test for nothing.

## Consequences

- The `Server` state machine applies P-9 and P-13 to the raw bits, and has its own tests for them.
- A stock client with `-e` would print zeros for the transit statistics of our server, if our server ever sets `EXTEND`. It doesn't in v1.
- A reader of `TestHeader` must check `EXTEND` itself to match the stock server exactly, because `is_reverse` doesn't.
