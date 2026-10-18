# The iperf 2.2.2 source

Covers: the claims this repo takes from the source of iperf 2.2.2 at commit `74d4b553`, the tip of the branch `2-2-2` of <https://sourceforge.net/p/iperf2/code/>, read on 2026-10-05.

Each claim links the file and line it comes from. The [iperf2](../entities/iperf2.md) page holds what the binary was observed to send. The source was read at commit `1779e3e3` on `master`, whose `src/`, `include/` and `compat/` match `74d4b553` byte for byte, so each line number holds for both.

## Byte order and defaults

- Every field of more than one byte is big-endian on the wire ([`src/Settings.cpp:3373`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Settings.cpp#l3373)).
- The wire structs sit inside `#pragma pack(push, 4)`, and every field is a 4-byte word or a pair of 2-byte halves ([`include/payloads.h:261`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/include/payloads.h#l261)).
- A 2.2.2 release writes `version_u` 0x00020002 and `version_l` 0x00020003 ([`include/version.h:5`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/include/version.h#l5)).
- The default port is 5001 for TCP and UDP ([`src/Settings.cpp:413`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Settings.cpp#l413)).
- The default `-l` is 131072 bytes for TCP ([`src/Settings.cpp:375`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Settings.cpp#l375)).
- The default `-l` of a UDP client is 1470 bytes over IPv4 and 1450 over IPv6 ([`src/Settings.cpp:1980`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Settings.cpp#l1980)).
- The default `-t` is 10 s ([`src/Settings.cpp:417`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Settings.cpp#l417)).
- The default UDP rate is 1048576 bit/s ([`src/Settings.cpp:374`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Settings.cpp#l374)).
- TCP has no rate limit by default ([`src/Settings.cpp:2014`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Settings.cpp#l2014)).
- `-i` sets a local report interval, and no header field holds it ([`include/payloads.h:294`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/include/payloads.h#l294)).

## The UDP prefix

- Every UDP datagram starts with the 16-byte `UDP_datagram`: `id`, `tv_sec`, `tv_usec` and `id2`, each 4 bytes ([`include/payloads.h:262`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/include/payloads.h#l262)).
- The client writes the low 32 bits of the signed 64-bit packet ID to `id` and the high 32 bits to `id2` ([`src/Client.cpp:2852`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Client.cpp#l2852)).
- The client's first datagram has ID 1 ([`src/Client.cpp:93`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Client.cpp#l93)).
- The ID rises by 1 after each write that succeeds ([`src/Client.cpp:2216`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Client.cpp#l2216)).
- The client writes its clock's time to `tv_sec` and `tv_usec` just before each write ([`src/Client.cpp:2143`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Client.cpp#l2143)).
- The server joins `id` and `id2` into one ID only when the flow's 64-bit flag is set ([`src/Server.cpp:1288`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Server.cpp#l1288)).
- Without that flag, the server reads `id` alone as a signed 32-bit value ([`src/Server.cpp:1322`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Server.cpp#l1322)).
- A negative ID marks the end of the test ([`src/Server.cpp:1328`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Server.cpp#l1328)).

## The test header

- `client_hdr_v1` is 24 bytes: `flags`, `numThreads`, `mPort`, `mBufLen`, `mWinBand` and `mAmount` ([`include/payloads.h:294`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/include/payloads.h#l294)).
- `client_hdrext` is 40 bytes and follows `client_hdr_v1` ([`include/payloads.h:451`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/include/payloads.h#l451)).
- The client zeroes the header before it fills the fields ([`src/Settings.cpp:3464`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Settings.cpp#l3464)).
- The client writes `mBufLen` only when `-l` was given, and 0 otherwise ([`src/Settings.cpp:3372`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Settings.cpp#l3372)).
- The client writes its `-p` port to `mPort` ([`src/Settings.cpp:3377`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Settings.cpp#l3377)).
- The client writes its stream count to `numThreads`. The count is 1 without `-P` ([`src/Settings.cpp:3390`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Settings.cpp#l3390)).
- In time mode, `mAmount` is `-t` in units of 10 ms, negated ([`src/Settings.cpp:3393`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Settings.cpp#l3393)).
- In byte mode, `mAmount` is the `-n` byte count masked with 0x7FFFFFFF ([`src/Settings.cpp:3395`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Settings.cpp#l3395)).
- The server reads a set top bit of `mAmount` as time mode ([`src/Settings.cpp:3120`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Settings.cpp#l3120)).
- The client writes `version_u`, `version_l` and `tos` in the TCP and UDP headers ([`src/Settings.cpp:3467`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Settings.cpp#l3467)).
- `lRate` holds the low 32 bits of the rate, and `uRate` holds the high 32 bits shifted left by 8 ([`src/Settings.cpp:3473`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Settings.cpp#l3473)).
- A UDP client without `-b` writes the default rate of 1048576 to `lRate` ([`src/Settings.cpp:3485`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Settings.cpp#l3485)).
- A TCP client writes the rate only when `-b` was given, and 0 otherwise ([`src/Settings.cpp:3609`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Settings.cpp#l3609)).

### Flags

- The `flags` word uses `HEADER_VERSION1` 0x80000000, `HEADER_EXTEND` 0x40000000, `HEADER_UDPTESTS` 0x20000000, `HEADER_KEYCHECK` 0x10000000, `HEADER_SEQNO64B` 0x08000000, `HEADER_VERSION2` 0x04000000, `HEADER_V2PEERDETECT` 0x02000000 and `HEADER_BOUNCEBACK` 0x00800000 ([`include/payloads.h:77`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/include/payloads.h#l77)).
- `HEADER_LEN_BIT` is 0x00010000, and the header length shifted left by 1 sits under the mask 0x0000FFFE ([`include/payloads.h:89`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/include/payloads.h#l89)).
- The `upperflags` bit for `-R` is `HEADER_REVERSE`, 0x0400 ([`include/payloads.h:164`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/include/payloads.h#l164)).
- The client sets `HEADER_VERSION1` for `-r` and `-d`, and never with `-C` ([`src/Settings.cpp:3398`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Settings.cpp#l3398)).
- The client sets `HEADER_REVERSE` in `upperflags` for `-R` ([`src/Settings.cpp:3439`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Settings.cpp#l3439)).
- The client writes the header length to the flags as `(len << 1) & 0xFFFE`, with `HEADER_LEN_BIT` ([`src/Settings.cpp:3583`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Settings.cpp#l3583)).

### TCP

- The TCP header is `client_hdr_v1` and then `client_hdrext`, 64 bytes in the v1 modes ([`src/Settings.cpp:3615`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Settings.cpp#l3615)).
- A TCP client always sets `HEADER_EXTEND` ([`src/Settings.cpp:3605`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Settings.cpp#l3605)).
- A TCP client adds `HEADER_VERSION2` for `-R` ([`src/Settings.cpp:3715`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Settings.cpp#l3715)).
- A TCP client adds `HEADER_V2PEERDETECT` and `HEADER_VERSION2` for `-X` ([`src/Settings.cpp:3617`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Settings.cpp#l3617)).
- The flags of a TCP test are 0x40010080, and 0x44010080 with `-R` ([`src/Settings.cpp:3720`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Settings.cpp#l3720)).
- The header is the first bytes of the TCP stream, written before the data ([`src/Client.cpp:3459`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Client.cpp#l3459)).
- The 64 header bytes count toward `-n` ([`src/Client.cpp:464`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Client.cpp#l464)).

### UDP

- The UDP header follows the prefix, so `flags` is at offset 16 ([`include/payloads.h:855`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/include/payloads.h#l855)).
- The client repeats the header in every datagram, and rewrites only the prefix ([`src/Client.cpp:2142`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Client.cpp#l2142)).
- A UDP client always sets `HEADER_SEQNO64B` and `HEADER_EXTEND` ([`src/Settings.cpp:3465`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Settings.cpp#l3465)).
- A UDP client adds `HEADER_UDPTESTS` and `HEADER_VERSION2` for `-R` ([`src/Settings.cpp:3521`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Settings.cpp#l3521)).
- A UDP client adds `HEADER_VERSION1` when `-R` and `-l` are both given ([`src/Settings.cpp:3586`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Settings.cpp#l3586)).
- The length in a UDP header's flags includes the prefix, so it is 16 + 24 + 40 = 80 ([`src/Settings.cpp:3560`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Settings.cpp#l3560)).
- The flags of a UDP test are 0x480100A0, 0x6C0100A0 with `-R`, and 0xEC0100A0 with `-R` and `-l` ([`src/Settings.cpp:3589`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Settings.cpp#l3589)).
- In byte mode, the client cuts the last datagram to the bytes left ([`src/Client.cpp:2178`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Client.cpp#l2178)).
- The UDP client aims for a gap of `-l` × 8 × 10^9 / rate nanoseconds between datagrams ([`src/Client.cpp:1916`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Client.cpp#l1916)).
- The client keeps a running delay that adds the gap and subtracts the time each send took ([`src/Client.cpp:2156`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Client.cpp#l2156)).

## The fill pattern

- The fill is the byte `'0' + (i % 10)` at index `i` of the buffer ([`compat/string.c:66`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/compat/string.c#l66)).
- With `-C`, the bytes at offsets 16 to 19 of a UDP datagram are fill, 0x36373839 ([`src/Listener.cpp:1402`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Listener.cpp#l1402)).

## How the server reads a TCP header

- The server reads the first 4 bytes of the stream as `flags` ([`src/Listener.cpp:1810`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Listener.cpp#l1810)).
- The header length is 0 unless `HEADER_VERSION1`, `HEADER_VERSION2` or `HEADER_EXTEND` is set ([`src/Settings.cpp:3744`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Settings.cpp#l3744)).
- With `HEADER_LEN_BIT`, the length is `(flags & 0xFFFE) >> 1` ([`src/Settings.cpp:3750`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Settings.cpp#l3750)).
- The server refuses a length above 256 ([`src/Settings.cpp:3751`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Settings.cpp#l3751)).
- Without `HEADER_LEN_BIT`, the length is 24 for `HEADER_VERSION1` or `HEADER_EXTEND`, plus 40 for `HEADER_VERSION2` or `HEADER_EXTEND` ([`src/Settings.cpp:3756`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Settings.cpp#l3756)).
- The server counts the header bytes as the first bytes of the test ([`src/Listener.cpp:2071`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Listener.cpp#l2071)).
- With `HEADER_VERSION2` and `HEADER_REVERSE`, the server sends the data ([`src/Listener.cpp:2229`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Listener.cpp#l2229)).
- With flags of 0, the server reads no header and sends no header ack ([`src/Listener.cpp:2264`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Listener.cpp#l2264)).
- The server sets `firstreadbytes` only when it reads a header ([`src/Listener.cpp:2071`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Listener.cpp#l2071)).
- The server counts `firstreadbytes` before its read loop, so without a header the 4 bytes read as `flags` are not counted ([`src/Server.cpp:1170`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Server.cpp#l1170)).
- The server's TCP read loop ends when the client closes ([`src/Server.cpp:472`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Server.cpp#l472)).

## The header ack

- The server sends the first 28 bytes of `client_hdr_ack` when the test does not ask for trip times ([`src/Listener.cpp:2295`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Listener.cpp#l2295)).
- The 28 bytes are `type` 2, `length` 28, `flags` 0, `version_u`, `version_l` and two reserved words of 0 ([`src/Listener.cpp:2279`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Listener.cpp#l2279)).
- The server sends the ack when `HEADER_EXTEND` is set without `HEADER_VERSION2`, or when `HEADER_V2PEERDETECT` is set ([`src/Listener.cpp:2265`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Listener.cpp#l2265)).
- The client reads the ack only with `-X` ([`src/Client.cpp:3464`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Client.cpp#l3464)).
- At the end of a test, the client reads and discards what the server sent until the server closes ([`src/Client.cpp:3345`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Client.cpp#l3345)).

## The end of a TCP test

- The client ends a TCP test with `shutdown(SHUT_WR)` ([`src/Client.cpp:3104`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Client.cpp#l3104)).
- The client then waits for the server to close, for the larger of `-t` and 2 s ([`src/Client.cpp:3340`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Client.cpp#l3340)).

## How the server reads a UDP flow

- The server marks a flow 64-bit when the first datagram's `flags` have `HEADER_SEQNO64B` ([`src/Listener.cpp:1024`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Listener.cpp#l1024)).
- The server drops a first datagram whose ID is 0 or negative, as a leftover of an old test ([`src/Listener.cpp:1030`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Listener.cpp#l1030)).
- At a first datagram with a positive ID, the listener hands its socket to the flow's server thread ([`src/Listener.cpp:1068`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Listener.cpp#l1068)).
- The listener then connects that socket to the sender ([`src/Listener.cpp:1090`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Listener.cpp#l1090)).
- The listener then opens a new listening socket on the port, so the next flow reaches the listener while the last flow's thread runs ([`src/Listener.cpp:1092`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Listener.cpp#l1092)). It skips this when the server was started with `-P 1`. Without `-P`, `mCount` is -1 and the listener opens the new socket ([`src/Listener.cpp:316`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Listener.cpp#l316)).
- The server refuses a first datagram shorter than 40 bytes ([`src/Listener.cpp:1391`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Listener.cpp#l1391)).
- The server reads no header when `flags` are the `-C` fill 0x36373839 ([`src/Listener.cpp:1402`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Listener.cpp#l1402)).
- The server reads the header fields only when `HEADER_VERSION1`, `HEADER_VERSION2` or `HEADER_EXTEND` is set ([`src/Listener.cpp:1455`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Listener.cpp#l1455)).
- The server reads `upperflags` and `lowerflags` when `HEADER_EXTEND` is set ([`src/Listener.cpp:1540`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Listener.cpp#l1540)).
- With `HEADER_VERSION2` and `HEADER_REVERSE`, the server sends the data ([`src/Listener.cpp:1646`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Listener.cpp#l1646)).
- The first datagram counts toward the server's totals ([`src/Listener.cpp:1115`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Listener.cpp#l1115)).
- The server ignores datagrams from a source port other than the flow's ([`src/Server.cpp:1221`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Server.cpp#l1221)).
- The server's UDP loop ends on a negative ID, an error, an interrupt or its own `-t` ([`src/Server.cpp:1498`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Server.cpp#l1498)).
- The server's own `-t` applies only when the server was started with `-t` ([`src/Server.cpp:185`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Server.cpp#l185)).

## A reverse test

- In a reverse test, the client connects as in a forward test and reads on the same socket ([`src/Launch.cpp:246`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Launch.cpp#l246)).
- A TCP client sends the 64-byte header and then only reads ([`src/Client.cpp:397`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Client.cpp#l397)).
- A UDP client sends the header datagram with ID 1 and waits up to 1 s for data ([`src/Client.cpp:270`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Client.cpp#l270)).
- A UDP client sends that datagram again each second until data arrives or 10 s pass ([`src/Client.cpp:260`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Client.cpp#l260)).
- The server sends on the accepted TCP connection, or to the UDP address the flow came from ([`src/Listener.cpp:1090`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Listener.cpp#l1090)).
- The server takes `-l` from `mBufLen` when it is not 0 ([`src/Settings.cpp:3091`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Settings.cpp#l3091)).
- The server ignores an `mBufLen` of 0 or below, or above 16 MiB ([`src/Settings.cpp:3102`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Settings.cpp#l3102)).
- A UDP server cuts its datagram length to 1470 bytes over IPv4 or 1450 over IPv6 when it is longer ([`src/Settings.cpp:3194`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Settings.cpp#l3194)).
- The server takes the length of the test from `mAmount` ([`src/Settings.cpp:3119`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Settings.cpp#l3119)).
- The server takes the rate from `lRate` and `uRate` ([`src/Settings.cpp:3217`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Settings.cpp#l3217)).
- A UDP server sends the prefix and fill, without a header ([`src/Client.cpp:395`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Client.cpp#l395)).
- A UDP server ends with one datagram of negative ID, and does not wait for a reply ([`src/Client.cpp:3158`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Client.cpp#l3158)).
- The client reads the server's UDP IDs as signed 32-bit values, because only the listener sets the 64-bit flag ([`src/Server.cpp:1322`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Server.cpp#l1322)).
- In time mode, the client stops reading at `-t` plus 2 s ([`src/Server.cpp:1151`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Server.cpp#l1151)).
- In byte mode, a TCP client stops reading after `-n` bytes ([`src/Server.cpp:496`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Server.cpp#l496)).
- A TCP server ends with `shutdown(SHUT_WR)` and waits for the client to close ([`src/Client.cpp:3131`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Client.cpp#l3131)).

### The burst header

- In a reverse TCP test, the server starts each burst of data with a `TCP_burst_payload` ([`src/Client.cpp:130`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Client.cpp#l130)), and the client reads the data as bursts ([`src/Server.cpp:135`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Server.cpp#l135)).
- `TCP_burst_payload` is 120 bytes ([`include/payloads.h:535`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/include/payloads.h#l535)).
- Each burst is `-l` bytes, or 120 when `-l` is smaller ([`src/Client.cpp:783`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Client.cpp#l783)).
- The sender writes the start time at offset 12, the write time at offset 20, `burst_id` at 44, `burst_size` at 48 and `seqno_lower` and `seqno_upper` at 52 ([`src/Client.cpp:2866`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Client.cpp#l2866)).
- The sender's count starts at 1, and the sender adds each `burst_size` to it before it writes `seqno_lower` and `seqno_upper` ([`src/Client.cpp:2869`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Client.cpp#l2869)).
- The reader reads 120 bytes, then `burst_size` minus 120 bytes as the body ([`src/Server.cpp:428`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Server.cpp#l428)).
- The reader fails an assertion when `burst_size` is 0 ([`src/Server.cpp:339`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Server.cpp#l339)).
- The reader marks the sender's TCP statistics as unknown when `reverse_rtt`, at offset 80, is 0xFFFFFFFF ([`src/Server.cpp:378`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Server.cpp#l378)).

## The end of a UDP test

- After its last datagram of data, the client sends a datagram with ID `-N`, where `N` is the next unused ID ([`src/Client.cpp:3158`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Client.cpp#l3158)).
- That datagram has the full `-l` length, header included ([`src/Client.cpp:3161`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Client.cpp#l3161)).
- The client waits for a reply in steps of 10 ms, at most 200 times ([`src/Client.cpp:3227`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Client.cpp#l3227)).
- After each silent step, the client sends the datagram again with the ID one lower: `-(N+1)`, then `-(N+2)` ([`src/Client.cpp:3247`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Client.cpp#l3247)).
- The client skips a reply of 52 bytes, the size of `client_hdr_ack`, whose `type` is 2 ([`src/Client.cpp:3263`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Client.cpp#l3263)).
- The client reads any other reply of positive length as the server report, from offset 16, without checking its length or flags ([`src/Client.cpp:3275`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Client.cpp#l3275)).
- Without a reply, the client prints "did not receive ack of last datagram" ([`src/Client.cpp:3289`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Client.cpp#l3289)).

### The server report

- The server replies when its loop ends, unless the flow is multicast or the client asked for no reply ([`src/Server.cpp:1545`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Server.cpp#l1545)).
- The reply is a 16-byte prefix of zeros and a 112-byte `server_hdr`, 128 bytes in all ([`src/Reports.c:1305`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Reports.c#l1305)).
- `flags` is `HEADER_VERSION1 | HEADER_SEQNO64B`, 0x88000000, and the server adds `HEADER_EXTEND` with `-e` ([`src/Reports.c:1319`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Reports.c#l1319)).
- The server writes the byte total, the duration, the lost, out-of-order and datagram counts, the jitter and the transit statistics ([`src/Reports.c:1324`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Reports.c#l1324)).
- The duration is the time from the server's start to its last datagram ([`src/Reporter.c:1581`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Reporter.c#l1581)).
- The reported jitter is the mean of the running jitter values, not the last one ([`src/Reports.c:1344`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Reports.c#l1344)).
- The client joins the high and low words of each count when `HEADER_SEQNO64B` is set ([`src/Reports.c:1231`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Reports.c#l1231)).
- After the reply, the server waits 250 ms for silence, and replies again for each datagram that arrives, up to 9 replies ([`src/Reports.c:1369`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Reports.c#l1369)).
- A reply that the server sends again holds the first 128 bytes of the datagram it read, not the report, because the read goes into the reply buffer ([`src/Reports.c:1401`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Reports.c#l1401)).

## Loss, out-of-order and jitter

- The server counts only datagrams with an ID above 0 ([`src/Reporter.c:1406`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Reporter.c#l1406)).
- An ID below the highest ID seen plus 1 counts as out of order ([`src/Reporter.c:1411`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Reporter.c#l1411)).
- An ID above the highest ID seen plus 1 adds the gap to the lost count ([`src/Reporter.c:1415`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Reporter.c#l1415)).
- The highest ID seen never goes down ([`src/Reporter.c:1419`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Reporter.c#l1419)).
- The lost count at the end is the lost total minus the out-of-order total, and never below 0 ([`src/Reporter.c:1917`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Reporter.c#l1917)).
- The datagram count at the end is the highest ID seen ([`src/Reporter.c:1921`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Reporter.c#l1921)).
- The bytes of the end-of-test datagram count toward the byte total ([`src/Reporter.c:1448`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Reporter.c#l1448)).
- The transit time is the arrival time minus the send time in the prefix ([`src/Reporter.c:887`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Reporter.c#l887)).
- `D` is the change in transit time from the previous datagram, without its sign ([`src/Reporter.c:892`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Reporter.c#l892)).
- The jitter `J` updates as `J += (D - J) / 16`, as in RFC 1889 ([`src/Reporter.c:947`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Reporter.c#l947)).
- The jitter first updates at the second transit time ([`src/Reporter.c:946`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Reporter.c#l946)).
- An out-of-order datagram does not update the transit time or the jitter ([`src/Reporter.c:1443`](https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Reporter.c#l1443)).
