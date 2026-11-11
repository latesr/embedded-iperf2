//! The clock of one test: when it started, when each interval report is due, and the bytes at the last one.
//!
//! Every interval that ends during a test is reported, an empty interval included. The last part of an interval
//! is reported before the final report, as a stock iperf prints them (P-19).

use crate::counts::UdpReceiver;
use crate::report::{Report, UdpCounts};
use crate::wire::{ServerReport, Timestamp};

/// The microseconds in one second.
pub(crate) const SECOND_US: u64 = 1_000_000;

/// When a test started, and the interval reports it has made.
#[derive(Debug, Default)]
pub(crate) struct Meter {
    interval_us: Option<u64>,
    start_us: Option<u64>,
    /// The end of the last interval reported, from the start of the test.
    reported_us: u64,
    reported_bytes: u64,
    /// Intervals that ended with no bytes, after the last one reported, still to report.
    empty: u64,
}

impl Meter {
    pub(crate) fn new(interval_us: Option<u64>) -> Self {
        Self {
            interval_us: interval_us.filter(|interval| *interval > 0),
            ..Self::default()
        }
    }

    /// Starts the test at `now_us`, if it has not started.
    pub(crate) fn start(&mut self, now_us: u64) {
        self.start_us.get_or_insert(now_us);
    }

    /// The time the test started, once it has.
    pub(crate) fn start_us(&self) -> Option<u64> {
        self.start_us
    }

    /// When the next interval report is due, on the caller's clock.
    pub(crate) fn next_due(&self) -> Option<u64> {
        let interval = self.interval_us?;
        Some(
            self.start_us?
                .saturating_add(self.reported_us)
                .saturating_add(interval),
        )
    }

    /// The interval report due at `now_us`, if one is. `bytes` is the total so far. `udp` is the receiver when
    /// the report holds its counts.
    ///
    /// The first interval due holds every byte since the last report. The intervals after it that have also ended
    /// are empty, because the bytes arrived after them.
    pub(crate) fn interval(
        &mut self,
        now_us: u64,
        bytes: u64,
        udp: Option<&mut UdpReceiver>,
    ) -> Option<Report> {
        let due = self.next_due()?;
        if now_us < due {
            return None;
        }
        if self.empty > 0 {
            return self.empty_interval(udp.is_some());
        }
        let interval = self.interval_us.unwrap_or_default();
        let start_us = self.reported_us;
        self.reported_us = self.reported_us.saturating_add(interval);
        self.empty = now_us
            .saturating_sub(due)
            .checked_div(interval)
            .unwrap_or_default();
        let report = Report {
            start_us,
            end_us: self.reported_us,
            bytes: bytes.saturating_sub(self.reported_bytes),
            udp: udp.map(|counts| counts.interval().1),
            final_report: false,
            server: None,
        };
        self.reported_bytes = bytes;
        Some(report)
    }

    /// The next empty interval of a stall, if one is left to report. `udp` says whether it holds counts.
    pub(crate) fn empty_interval(&mut self, udp: bool) -> Option<Report> {
        if self.empty == 0 {
            return None;
        }
        self.empty = self.empty.saturating_sub(1);
        let start_us = self.reported_us;
        self.reported_us = self
            .reported_us
            .saturating_add(self.interval_us.unwrap_or_default());
        Some(Report {
            start_us,
            end_us: self.reported_us,
            bytes: 0,
            udp: udp.then(UdpCounts::default),
            final_report: false,
            server: None,
        })
    }

    /// The last part of an interval, from the end of the intervals reported or still to report to `end_us` from
    /// the start, when the test ended inside one. `bytes` is the total of the test.
    pub(crate) fn tail(&self, end_us: u64, bytes: u64, udp: Option<UdpCounts>) -> Option<Report> {
        let interval = self.interval_us?;
        let start_us = self
            .reported_us
            .saturating_add(self.empty.saturating_mul(interval));
        (end_us > start_us).then(|| Report {
            start_us,
            end_us,
            bytes: bytes.saturating_sub(self.reported_bytes),
            udp,
            final_report: false,
            server: None,
        })
    }

    /// The final report of a test that ended at `now_us`, with `bytes` in all.
    pub(crate) fn last(
        &self,
        now_us: u64,
        bytes: u64,
        udp: Option<UdpCounts>,
        server: Option<ServerReport>,
    ) -> Report {
        Report {
            start_us: 0,
            end_us: now_us.saturating_sub(self.start_us.unwrap_or(now_us)),
            bytes,
            udp,
            final_report: true,
            server,
        }
    }
}

/// `now_us` as the wire holds a time.
pub(crate) fn timestamp(now_us: u64) -> Timestamp {
    Timestamp {
        sec: (now_us / SECOND_US) as u32,
        usec: (now_us % SECOND_US) as u32,
    }
}

/// Writes the fill of a stock iperf: the byte at index `i` of `out` is the digit `i % 10`.
pub(crate) fn fill(out: &mut [u8]) {
    // The fill must be a copy of whole blocks. A remainder for each byte held a TCP sender on an ESP32-S31 to half
    // the rate of a plain socket loop.
    for block in out.chunks_mut(DIGITS.len()) {
        if let Some(from) = DIGITS.get(..block.len()) {
            block.copy_from_slice(from);
        }
    }
}

/// The digits `0` to `9` in turn, a whole number of times, so each block of the fill starts at `0`.
const DIGITS: &[u8; 60] = b"012345678901234567890123456789012345678901234567890123456789";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intervals_follow_the_start_and_report_their_own_bytes() {
        let mut meter = Meter::new(Some(1_000));
        assert_eq!(meter.next_due(), None);
        meter.start(5_000);
        meter.start(5_500);
        assert_eq!(meter.next_due(), Some(6_000));
        assert_eq!(meter.interval(5_999, 10, None), None);
        let first = meter.interval(6_000, 10, None).unwrap();
        assert_eq!((first.start_us, first.end_us, first.bytes), (0, 1_000, 10));
        let second = meter.interval(7_200, 25, None).unwrap();
        assert_eq!(
            (second.start_us, second.end_us, second.bytes),
            (1_000, 2_000, 15)
        );
        assert!(!second.final_report);
        let tail = meter.tail(2_500, 30, None).unwrap();
        assert_eq!((tail.start_us, tail.end_us, tail.bytes), (2_000, 2_500, 5));
        assert_eq!(meter.tail(2_000, 30, None), None);
        let last = meter.last(7_500, 30, None, None);
        assert_eq!(
            (last.start_us, last.end_us, last.bytes, last.final_report),
            (0, 2_500, 30, true)
        );
    }

    #[test]
    fn the_intervals_of_a_stall_are_empty() {
        let mut meter = Meter::new(Some(1_000));
        meter.start(0);
        let first = meter.interval(3_500, 10, None).unwrap();
        assert_eq!((first.end_us, first.bytes), (1_000, 10));
        assert_eq!(meter.tail(3_700, 12, None).unwrap().start_us, 3_000);
        let second = meter.interval(3_500, 12, None).unwrap();
        assert_eq!((second.end_us, second.bytes, second.udp), (2_000, 0, None));
        let third = meter.empty_interval(true).unwrap();
        assert_eq!(
            (third.end_us, third.udp),
            (3_000, Some(UdpCounts::default()))
        );
        assert_eq!(meter.empty_interval(true), None);
        assert_eq!(meter.interval(3_999, 12, None), None);
        assert_eq!(meter.interval(4_000, 12, None).unwrap().bytes, 2);
    }

    #[test]
    fn no_interval_means_no_interval_reports() {
        let mut meter = Meter::new(None);
        meter.start(0);
        assert_eq!(meter.interval(u64::MAX, 1, None), None);
        assert_eq!(meter.tail(5, 1, None), None);
        let mut meter = Meter::new(Some(0));
        meter.start(0);
        assert_eq!(meter.interval(u64::MAX, 1, None), None);
    }

    #[test]
    fn a_time_splits_into_seconds_and_microseconds() {
        assert_eq!(timestamp(3_000_004), Timestamp { sec: 3, usec: 4 });
    }

    #[test]
    fn the_fill_continues_across_its_blocks() {
        let mut out = [0; 131];
        fill(&mut out);
        for (at, byte) in out.iter().enumerate() {
            assert_eq!(*byte, b'0' + (at % 10) as u8, "byte {at}");
        }
    }
}
