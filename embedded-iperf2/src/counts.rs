//! The counts a UDP receiver keeps from the packet ID and the send time of each datagram (P-17, P-18).
//!
//! The rules are those of the stock iperf 2.2.2 reporter:
//!
//! - A packet ID below the highest ID received plus 1 counts as out of order. A duplicate counts as out of order.
//! - A packet ID above the highest ID received plus 1 adds the gap to the lost count.
//! - The lost count in a report is the sum of the gaps minus the out-of-order count, so a late datagram is not lost.
//! - The datagram count is the highest packet ID received.
//! - The jitter follows RFC 1889. From the second datagram on, `D` is the change in transit time without its sign,
//!   and the jitter `J` updates as `J += (D - J) / 16`. A late datagram does not update the jitter.
//! - The final report holds the mean of the running jitter values.
//!
//! Jitter is kept in integer nanoseconds rather than the stock `double` seconds, so a board without an FPU does
//! not pay for soft floats on every datagram. The last transit time stays across interval reports, as in RFC
//! 1889. A stock server copies its transit statistics at each interval, so the first jitter update of an
//! interval can differ between the two. The counts do not depend on it.
//!
//! See <https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Reporter.c#l1406>
//! and <https://sourceforge.net/p/iperf2/code/ci/74d4b55321b9e7827c6567f3e48ae024ba3417c8/tree/src/Reporter.c#l887>.

use crate::report::UdpCounts;
use crate::wire::Timestamp;

/// The counts of one UDP test at its receiver.
#[derive(Debug, Default)]
pub(crate) struct UdpReceiver {
    /// The highest packet ID received. It never goes down.
    highest: u64,
    /// The sum of every gap in the packet IDs, before late datagrams are taken off.
    gaps: u64,
    out_of_order: u64,
    bytes: u64,
    /// The transit time of the last datagram that updated the jitter, and how many have.
    last_transit_ns: i64,
    transits: u64,
    jitter_ns: u64,
    /// The sum and count of the running jitter values, for the mean in the final report.
    jitter_sum_ns: u128,
    jitter_samples: u64,
    /// The totals when the last interval ended.
    interval_start: Totals,
}

/// Cumulative counts, kept so an interval reports only its own part.
#[derive(Clone, Copy, Debug, Default)]
struct Totals {
    highest: u64,
    gaps: u64,
    out_of_order: u64,
    bytes: u64,
}

impl UdpReceiver {
    /// Counts one datagram of `len` bytes with packet ID `id`, sent at `sent` on the sender's clock and received
    /// at `now_us` on the receiver's clock. An ID of 0 or below is not counted. The caller passes the end of the
    /// test with its ID made positive, so it counts as the last datagram (W-3).
    pub(crate) fn datagram(&mut self, id: i64, sent: Timestamp, now_us: u64, len: usize) {
        let Ok(id) = u64::try_from(id) else {
            return;
        };
        if id == 0 {
            return;
        }
        let next = self.highest.saturating_add(1);
        let late = id < next;
        if late {
            self.out_of_order = self.out_of_order.saturating_add(1);
        } else {
            self.gaps = self.gaps.saturating_add(id.saturating_sub(next));
            self.highest = id;
        }
        self.bytes = self.bytes.saturating_add(len as u64);
        if !late {
            self.transit(sent, now_us);
        }
    }

    /// Updates the jitter with the transit time of a datagram that was not late.
    fn transit(&mut self, sent: Timestamp, now_us: u64) {
        let sent_ns = i64::from(sent.sec)
            .saturating_mul(1_000_000_000)
            .saturating_add(i64::from(sent.usec).saturating_mul(1_000));
        let now_ns = i64::try_from(now_us)
            .unwrap_or(i64::MAX)
            .saturating_mul(1_000);
        let transit = now_ns.saturating_sub(sent_ns);
        let change = transit.saturating_sub(self.last_transit_ns).unsigned_abs();
        self.last_transit_ns = transit;
        self.transits = self.transits.saturating_add(1);
        if self.transits > 1 {
            let jitter = self.jitter_ns as i64;
            let step = (change as i64).saturating_sub(jitter) / 16;
            self.jitter_ns = jitter.saturating_add(step) as u64;
            self.jitter_sum_ns = self
                .jitter_sum_ns
                .saturating_add(u128::from(self.jitter_ns));
            self.jitter_samples = self.jitter_samples.saturating_add(1);
        }
    }

    /// The bytes received so far.
    pub(crate) fn bytes(&self) -> u64 {
        self.bytes
    }

    /// The counts since the last interval ended, and starts the next interval. The jitter is the running value.
    pub(crate) fn interval(&mut self) -> (u64, UdpCounts) {
        let now = self.totals();
        let start = core::mem::replace(&mut self.interval_start, now);
        let counts = counts(
            now.highest.saturating_sub(start.highest),
            now.gaps.saturating_sub(start.gaps),
            now.out_of_order.saturating_sub(start.out_of_order),
            self.jitter_ns,
        );
        (now.bytes.saturating_sub(start.bytes), counts)
    }

    /// The counts of the whole test. The jitter is the mean of the running values.
    pub(crate) fn total(&self) -> UdpCounts {
        let mean = self
            .jitter_sum_ns
            .checked_div(u128::from(self.jitter_samples))
            .unwrap_or_default();
        counts(
            self.highest,
            self.gaps,
            self.out_of_order,
            u64::try_from(mean).unwrap_or(u64::MAX),
        )
    }

    fn totals(&self) -> Totals {
        Totals {
            highest: self.highest,
            gaps: self.gaps,
            out_of_order: self.out_of_order,
            bytes: self.bytes,
        }
    }
}

/// The counts a report shows. A late datagram first counted as part of a gap, so it comes off the lost count.
fn counts(datagrams: u64, gaps: u64, out_of_order: u64, jitter_ns: u64) -> UdpCounts {
    UdpCounts {
        datagrams,
        lost: gaps.saturating_sub(out_of_order),
        out_of_order,
        jitter_ns,
    }
}

#[cfg(test)]
#[allow(clippy::arithmetic_side_effects)]
mod tests {
    use super::*;

    const LEN: usize = 512;

    /// A datagram sent at `sent_us` and received at `now_us`.
    fn receive(receiver: &mut UdpReceiver, id: i64, sent_us: u64, now_us: u64) {
        let sent = Timestamp {
            sec: (sent_us / 1_000_000) as u32,
            usec: (sent_us % 1_000_000) as u32,
        };
        receiver.datagram(id, sent, now_us, LEN);
    }

    fn ids(ids: &[i64]) -> UdpReceiver {
        let mut receiver = UdpReceiver::default();
        for id in ids {
            receive(&mut receiver, *id, 0, 100);
        }
        receiver
    }

    #[test]
    fn datagrams_in_order_lose_nothing() {
        let receiver = ids(&[1, 2, 3, 4, 5]);
        assert_eq!(
            receiver.total(),
            UdpCounts {
                datagrams: 5,
                lost: 0,
                out_of_order: 0,
                jitter_ns: 0,
            }
        );
        assert_eq!(receiver.bytes(), 5 * LEN as u64);
    }

    #[test]
    fn a_gap_counts_as_lost_until_the_late_datagram_arrives() {
        let receiver = ids(&[1, 2, 5]);
        assert_eq!((receiver.total().datagrams, receiver.total().lost), (5, 2));
        let receiver = ids(&[1, 2, 5, 3]);
        let total = receiver.total();
        assert_eq!((total.datagrams, total.lost, total.out_of_order), (5, 1, 1));
    }

    #[test]
    fn a_duplicate_counts_as_out_of_order() {
        let total = ids(&[1, 2, 2, 3]).total();
        assert_eq!((total.datagrams, total.lost, total.out_of_order), (3, 0, 1));
    }

    #[test]
    fn lost_never_goes_below_zero() {
        let total = ids(&[3, 1, 2, 2]).total();
        assert_eq!((total.lost, total.out_of_order), (0, 3));
    }

    #[test]
    fn an_id_of_zero_or_below_is_not_counted() {
        let receiver = ids(&[0, -4, 1]);
        assert_eq!(receiver.total().datagrams, 1);
        assert_eq!(receiver.bytes(), LEN as u64);
    }

    #[test]
    fn jitter_follows_rfc_1889_from_the_second_datagram() {
        let mut receiver = UdpReceiver::default();
        receive(&mut receiver, 1, 1_000_000, 1_000_100);
        receive(&mut receiver, 2, 1_001_000, 1_001_300);
        assert_eq!(receiver.interval().1.jitter_ns, 12_500);
        receive(&mut receiver, 3, 1_002_000, 1_002_300);
        assert_eq!(receiver.interval().1.jitter_ns, 11_719);
        assert_eq!(receiver.total().jitter_ns, (12_500 + 11_719) / 2);
    }

    #[test]
    fn the_sender_clock_offset_cancels() {
        let mut receiver = UdpReceiver::default();
        receive(&mut receiver, 1, 4_000_000_000, 100);
        receive(&mut receiver, 2, 4_000_001_000, 1_100);
        assert_eq!(receiver.total().jitter_ns, 0);
    }

    #[test]
    fn a_late_datagram_does_not_update_the_jitter() {
        let mut receiver = UdpReceiver::default();
        receive(&mut receiver, 1, 0, 100);
        receive(&mut receiver, 3, 1_000, 1_100);
        receive(&mut receiver, 2, 500, 9_000);
        assert_eq!(receiver.total().jitter_ns, 0);
    }

    #[test]
    fn an_interval_counts_only_its_own_datagrams() {
        let mut receiver = ids(&[1, 2, 4]);
        let (bytes, counts) = receiver.interval();
        assert_eq!(bytes, 3 * LEN as u64);
        assert_eq!(
            (counts.datagrams, counts.lost, counts.out_of_order),
            (4, 1, 0)
        );
        receive(&mut receiver, 3, 0, 100);
        receive(&mut receiver, 7, 0, 100);
        let (bytes, counts) = receiver.interval();
        assert_eq!(bytes, 2 * LEN as u64);
        assert_eq!(
            (counts.datagrams, counts.lost, counts.out_of_order),
            (3, 1, 1)
        );
        let (bytes, counts) = receiver.interval();
        assert_eq!((bytes, counts.datagrams, counts.lost), (0, 0, 0));
        let total = receiver.total();
        assert_eq!((total.datagrams, total.lost, total.out_of_order), (7, 2, 1));
    }
}
