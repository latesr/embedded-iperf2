//! The clock of one test: when it started, when each interval report is due, and the bytes at the last one.

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
}

impl Meter {
    pub(crate) fn new(interval_us: Option<u64>) -> Self {
        Self {
            interval_us: interval_us.filter(|interval| *interval > 0),
            ..Self::default()
        }
    }

    /// Starts the test at `now_us`, unless it has started.
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

    /// The interval report due at `now_us`, if one is. `bytes` is the total so far.
    pub(crate) fn interval(
        &mut self,
        now_us: u64,
        bytes: u64,
        udp: Option<UdpCounts>,
    ) -> Option<Report> {
        let due = self.next_due()?;
        if now_us < due {
            return None;
        }
        let start_us = self.reported_us;
        self.reported_us = self
            .reported_us
            .saturating_add(self.interval_us.unwrap_or_default());
        let report = Report {
            start_us,
            end_us: self.reported_us,
            bytes: bytes.saturating_sub(self.reported_bytes),
            udp,
            final_report: false,
            server: None,
        };
        self.reported_bytes = bytes;
        Some(report)
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

/// Writes the fill of a stock iperf, the digits `0` to `9` in turn, as the bytes at `offset` onwards of a stream.
pub(crate) fn fill(out: &mut [u8], offset: u64) {
    for (at, byte) in out.iter_mut().enumerate() {
        *byte = b'0'.wrapping_add((offset.wrapping_add(at as u64) % 10) as u8);
    }
}

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
        let last = meter.last(7_500, 30, None, None);
        assert_eq!(
            (last.start_us, last.end_us, last.bytes, last.final_report),
            (0, 2_500, 30, true)
        );
    }

    #[test]
    fn no_interval_means_no_interval_reports() {
        let mut meter = Meter::new(None);
        meter.start(0);
        assert_eq!(meter.interval(u64::MAX, 1, None), None);
        let mut meter = Meter::new(Some(0));
        meter.start(0);
        assert_eq!(meter.interval(u64::MAX, 1, None), None);
    }

    #[test]
    fn a_time_splits_into_seconds_and_microseconds() {
        assert_eq!(timestamp(3_000_004), Timestamp { sec: 3, usec: 4 });
    }

    #[test]
    fn the_fill_continues_from_its_offset() {
        let mut out = [0; 12];
        fill(&mut out, 7);
        assert_eq!(&out, b"789012345678");
    }
}
