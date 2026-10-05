# ADR-0001: The wire module reads the bytes and leaves the rules to the protocol

**Status: accepted**

Covers: the `wire` module of `embedded-iperf2`: the parsers and encoders of the UDP prefix, the test header, the header ack, the burst header and the server report.

## Summary

- The test header keeps `flags`, `upperflags` and `lowerflags` as numbers. The server decides from the bits which tests it serves and when it acks (P-9, P-13).
- A parser refuses only bytes that cannot be the message: a stated header length out of bounds, an ack of another type or length, and a server report without `VERSION1` (W-7, W-9, W-12).
- `header_length` reads the fill of a UDP client with `-C` as no header, as a stock server does, so no caller has to check for it first (P-11).
- Our server report has `flags` without `EXTEND` and zeros in the transit statistics (W-11).
- Every encoder writes the whole message, fill included, so the bytes do not depend on what the buffer held before.

## Context

The test header of iperf 2.2.2 holds a mode in its flag bits. A v1 server serves a few of the modes and must refuse the rest (P-13). It sends a header ack for some combinations and not others (P-9). A newcomer's first design is an enum of the v1 modes, built by the parser. The parser would then refuse every header outside v1 with the same error it gives for corrupt bytes. The server could not tell a client it does not serve from a broken stream. It also could not apply the stock server's ack rule, which looks at bits that no v1 mode names, such as `V2PEERDETECT`.

The server report has 60 bytes of transit statistics after the counts. A stock client reads them only when the report's `flags` has `EXTEND`, which a stock server sets only with `-e`. The counts, the duration and the jitter are what P-20 reports.

A UDP client with `-C` sends no header, and the fill pattern lands where `flags` would be. That word, 0x36373839, sets `VERSION2` and `LEN_BIT` and states a length of 7196 bytes. The stock server checks for it before it reads any bit ([summary](../summaries/iperf2-2.2.2.md#how-the-server-reads-a-udp-flow)).

## Decision

The parsers read fields and check only what makes the bytes unreadable. The flag words stay raw in `TestHeader`, with `is_reverse` and `header_length` as helpers. The rules for which tests run belong to the server state machine of step 2, which sees every bit.

`header_length` takes the transport, because W-7's lower bound is 64 for TCP and 80 for UDP. For UDP it returns no header for the `-C` fill, so the check that the stock server makes first cannot be forgotten by a caller.

`ServerReport` holds the counts, the duration and the jitter. Its encoder writes `VERSION1 | SEQNO64B` and zeros in the transit statistics. Its parser reads the high words of the counts only with `SEQNO64B`, as a stock client does.

## Alternatives

- **An enum of v1 modes in `TestHeader`.** Rejected: the server could not tell an unserved mode from corrupt bytes, and the ack rule needs bits no v1 mode names.
- **The `-C` check in each UDP caller.** It was the first version. The reviewer found that every caller would have to repeat it, and the user chose to move it into `header_length` on 2026-10-05.
- **Computing the transit statistics.** It would need the minimum, maximum, mean and M2 of transit time, kept per test on a board, for fields a stock client does not read without `EXTEND`.

## Consequences

- The server state machine applies P-9 and P-13 to the raw bits, and has its own tests for them.
- A stock client with `-e` would print zeros for the transit statistics of our server, if our server ever sets `EXTEND`. It does not in v1.
- A reader of `TestHeader` must check `EXTEND` itself to match the stock server exactly, because `is_reverse` does not.
