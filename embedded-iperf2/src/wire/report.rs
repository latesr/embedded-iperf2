use super::{
    BufferTooSmall, Error, Timestamp, flags, get_u32, join, put_u32, split, take, take_mut,
};

/// The length of the server report, prefix included.
pub const SERVER_REPORT_LEN: usize = 128;

/// The `flags` that the server writes: `VERSION1` and `SEQNO64B`, without `EXTEND`, so a client does not read the
/// transit statistics.
const REPORT_FLAGS: u32 = flags::VERSION1 | flags::SEQNO64B;

/// The datagram a UDP server sends at the end of a test (W-11).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ServerReport {
    /// The bytes the server received.
    pub bytes: u64,
    /// The duration of the test at the server.
    pub duration: Timestamp,
    /// The datagrams the server counted as lost.
    pub lost: u64,
    /// The datagrams the server counted as out of order.
    pub out_of_order: u64,
    /// The highest packet ID the server received.
    pub datagrams: u64,
    /// The jitter.
    pub jitter: Timestamp,
}

impl ServerReport {
    /// Reads the server report from the start of `input`, the whole datagram with its prefix. The high words of
    /// the counts are read only when `flags` has `SEQNO64B`, as a stock client does.
    ///
    /// Returns `Incomplete` when `input` is shorter than [`SERVER_REPORT_LEN`]. Returns `Invalid` when `flags`
    /// does not have `VERSION1` set. A stock server's later replies echo a client datagram without it.
    pub fn parse(input: &[u8]) -> Result<Self, Error> {
        let bytes = take::<SERVER_REPORT_LEN>(input)?;
        let report_flags = get_u32(bytes, 16);
        if report_flags & flags::VERSION1 == 0 {
            return Err(Error::Invalid);
        }
        let wide = report_flags & flags::SEQNO64B != 0;
        let count = |high_at: usize, low_at: usize| {
            let high = if wide { get_u32(bytes, high_at) } else { 0 };
            join(high, get_u32(bytes, low_at))
        };
        Ok(Self {
            bytes: join(get_u32(bytes, 20), get_u32(bytes, 24)),
            duration: Timestamp {
                sec: get_u32(bytes, 28),
                usec: get_u32(bytes, 32),
            },
            lost: count(116, 36),
            out_of_order: count(120, 40),
            datagrams: count(124, 44),
            jitter: Timestamp {
                sec: get_u32(bytes, 48),
                usec: get_u32(bytes, 52),
            },
        })
    }

    /// Writes the server report to the start of `out` and returns [`SERVER_REPORT_LEN`]. The prefix and the
    /// transit statistics are written as zeros.
    ///
    /// Returns `BufferTooSmall` when `out` is shorter than [`SERVER_REPORT_LEN`], and writes nothing.
    pub fn encode(&self, out: &mut [u8]) -> Result<usize, BufferTooSmall> {
        let bytes = take_mut::<SERVER_REPORT_LEN>(out)?;
        bytes.fill(0);
        put_u32(bytes, 16, REPORT_FLAGS);
        let mut count = |high_at: usize, low_at: usize, value: u64| {
            let (high, low) = split(value);
            put_u32(bytes, high_at, high);
            put_u32(bytes, low_at, low);
        };
        count(20, 24, self.bytes);
        count(116, 36, self.lost);
        count(120, 40, self.out_of_order);
        count(124, 44, self.datagrams);
        put_u32(bytes, 28, self.duration.sec);
        put_u32(bytes, 32, self.duration.usec);
        put_u32(bytes, 48, self.jitter.sec);
        put_u32(bytes, 52, self.jitter.usec);
        Ok(SERVER_REPORT_LEN)
    }
}

#[cfg(test)]
#[allow(clippy::arithmetic_side_effects)]
mod tests {
    use super::*;

    /// The server report of the `udp` sample.
    const STOCK: [u8; 128] = [
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x88, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x04, 0x00, 0x00, 0x00,
        0x00, 0x02, 0x00, 0x00, 0x45, 0x57, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x82, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xA8, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x77, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x15, 0xC9, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0xA6, 0x14, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x47, 0x01, 0xF9,
        0x4D, 0xC2, 0x00, 0x09, 0x81, 0xE4, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0x00,
        0x00, 0x00, 0x82, 0x00, 0x00, 0x00, 0x40, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];

    fn stock() -> ServerReport {
        ServerReport {
            bytes: 66560,
            duration: Timestamp {
                sec: 2,
                usec: 17751,
            },
            lost: 0,
            out_of_order: 0,
            datagrams: 130,
            jitter: Timestamp { sec: 0, usec: 168 },
        }
    }

    /// A report with a different value in each word, so a field read from the wrong offset shows.
    fn distinct() -> ServerReport {
        ServerReport {
            bytes: 0x1_0000_0002,
            duration: Timestamp { sec: 3, usec: 4 },
            lost: 0x5_0000_0006,
            out_of_order: 0x7_0000_0008,
            datagrams: 0x9_0000_000A,
            jitter: Timestamp { sec: 11, usec: 12 },
        }
    }

    #[test]
    fn parse_reads_the_stock_report() {
        assert_eq!(ServerReport::parse(&STOCK), Ok(stock()));
    }

    #[test]
    fn encode_writes_the_stock_report_without_the_transit_statistics() {
        let mut expected = STOCK;
        expected[56..116].fill(0);
        let mut out = [0xAA; 129];
        assert_eq!(stock().encode(&mut out), Ok(SERVER_REPORT_LEN));
        assert_eq!(out[..128], expected);
        assert_eq!(out[128], 0xAA);
    }

    #[test]
    fn each_count_splits_into_a_high_and_a_low_word() {
        let mut out = [0; 128];
        distinct().encode(&mut out).unwrap();
        let word = |at: usize| u32::from_be_bytes(out[at..at + 4].try_into().unwrap());
        let words = [20, 24, 28, 32, 36, 40, 44, 48, 52, 116, 120, 124].map(word);
        assert_eq!(words, [1, 2, 3, 4, 6, 8, 10, 11, 12, 5, 7, 9]);
        assert_eq!(ServerReport::parse(&out), Ok(distinct()));
    }

    #[test]
    fn without_seqno64b_the_high_words_are_not_read() {
        let mut out = [0; 128];
        distinct().encode(&mut out).unwrap();
        out[16..20].copy_from_slice(&flags::VERSION1.to_be_bytes());
        let report = ServerReport::parse(&out).unwrap();
        assert_eq!(
            (
                report.bytes,
                report.lost,
                report.out_of_order,
                report.datagrams
            ),
            (0x1_0000_0002, 6, 8, 10)
        );
    }

    #[test]
    fn flags_without_version1_are_invalid() {
        let mut echo = STOCK;
        echo[16..20].copy_from_slice(&0x4801_00A0_u32.to_be_bytes());
        assert_eq!(ServerReport::parse(&echo), Err(Error::Invalid));
    }

    #[test]
    fn short_input_is_incomplete_and_a_short_buffer_is_refused() {
        assert_eq!(ServerReport::parse(&STOCK[..127]), Err(Error::Incomplete));
        let mut out = [7; 127];
        assert_eq!(stock().encode(&mut out), Err(BufferTooSmall));
        assert_eq!(out, [7; 127]);
    }
}
