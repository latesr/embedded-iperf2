# Loss, out-of-order and jitter

Covers: the counts a UDP receiver keeps from the packet IDs and send times of the datagrams it receives.

The receiver of a UDP test keeps the highest packet ID it has seen. Each datagram's ID and send time update three numbers. [The iperf 2.2.2 source](../summaries/iperf2-2.2.2.md#loss-out-of-order-and-jitter) states each rule.

## Loss and out-of-order

- A datagram with an ID of 0 or below is not counted.
- An ID below the highest ID seen plus 1 counts as out of order. A duplicate counts as out of order, because there is no separate count for it.
- An ID above the highest ID seen plus 1 adds the gap to the lost count. The highest ID seen then rises to that ID.
- At the end of the test, the lost count is the lost total minus the out-of-order total, and never below 0. A datagram that arrives late counts first as lost and then as out of order, so it does not count as lost at the end.
- The datagram count is the highest ID seen. The end-of-test datagram has the next ID, so it counts as one datagram.

## Jitter

Jitter follows RFC 1889, the RTP specification:

1. The transit time of a datagram is its arrival time minus the send time in its prefix.
2. `D` is the transit time minus the transit time of the datagram before, without its sign.
3. From the second datagram on, the jitter `J` updates as `J = J + (D - J) / 16`.

An out-of-order datagram does not update the transit time or the jitter. The sender's clock and the receiver's clock need not agree, because a constant offset between them cancels in `D`. The server report holds the mean of the running jitter values over the test, not the last value.

Requirements that use the term: W-11, P-17, P-18, P-19 and P-20.
