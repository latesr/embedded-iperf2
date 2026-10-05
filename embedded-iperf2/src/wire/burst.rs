use super::{BufferTooSmall, Error, Timestamp, get_u32, join, put_u32, split, take, take_mut};

/// The length of the burst header.
pub const BURST_HEADER_LEN: usize = 120;

/// The value of a TCP statistic that a stock reader takes as unknown.
const UNKNOWN: u32 = 0xFFFF_FFFF;

/// The header at the start of each burst of a reverse TCP test (W-10).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BurstHeader {
    /// The sender's clock at the start of the test.
    pub start: Timestamp,
    /// The sender's clock when it wrote the burst.
    pub write: Timestamp,
    /// 1 for the first burst, and one more for each later burst.
    pub burst_id: u32,
    /// The length of the burst, this header included.
    pub burst_size: u32,
    /// 1 plus the bytes of every burst so far, this one included.
    pub seqno: u64,
}

impl BurstHeader {
    /// Reads the burst header from the start of `input`.
    ///
    /// Returns `Incomplete` when `input` is shorter than [`BURST_HEADER_LEN`]. The fields are not checked, and
    /// the sender's TCP statistics are not read.
    pub fn parse(input: &[u8]) -> Result<Self, Error> {
        let bytes = take::<BURST_HEADER_LEN>(input)?;
        Ok(Self {
            start: Timestamp {
                sec: get_u32(bytes, 12),
                usec: get_u32(bytes, 16),
            },
            write: Timestamp {
                sec: get_u32(bytes, 20),
                usec: get_u32(bytes, 24),
            },
            burst_id: get_u32(bytes, 44),
            burst_size: get_u32(bytes, 48),
            seqno: join(get_u32(bytes, 56), get_u32(bytes, 52)),
        })
    }

    /// Writes the burst header to the start of `out` and returns [`BURST_HEADER_LEN`]. The sender's TCP
    /// statistics are written as unknown, and the other bytes as fill (W-13).
    ///
    /// Returns `BufferTooSmall` when `out` is shorter than [`BURST_HEADER_LEN`], and writes nothing.
    pub fn encode(&self, out: &mut [u8]) -> Result<usize, BufferTooSmall> {
        let bytes = take_mut::<BURST_HEADER_LEN>(out)?;
        for (byte, digit) in bytes.iter_mut().zip((b'0'..=b'9').cycle()) {
            *byte = digit;
        }
        put_u32(bytes, 12, self.start.sec);
        put_u32(bytes, 16, self.start.usec);
        put_u32(bytes, 20, self.write.sec);
        put_u32(bytes, 24, self.write.usec);
        put_u32(bytes, 36, UNKNOWN);
        put_u32(bytes, 44, self.burst_id);
        put_u32(bytes, 48, self.burst_size);
        let (high, low) = split(self.seqno);
        put_u32(bytes, 52, low);
        put_u32(bytes, 56, high);
        for at in (76..108).step_by(4) {
            put_u32(bytes, at, UNKNOWN);
        }
        for at in (108..BURST_HEADER_LEN).step_by(4) {
            put_u32(bytes, at, 0);
        }
        Ok(BURST_HEADER_LEN)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The first burst header of the `tcp-reverse` sample.
    const FIRST_BURST: [u8; 120] = [
        0x30, 0x31, 0x32, 0x33, 0x34, 0x35, 0x36, 0x37, 0x38, 0x39, 0x30, 0x31, 0x6A, 0xC3, 0x8F,
        0x7D, 0x00, 0x01, 0xBE, 0x17, 0x6A, 0xC3, 0x8F, 0x7D, 0x00, 0x01, 0xBE, 0x1B, 0x38, 0x39,
        0x30, 0x31, 0x32, 0x33, 0x34, 0x35, 0xFF, 0xFF, 0xFF, 0xFF, 0x30, 0x31, 0x32, 0x33, 0x00,
        0x00, 0x00, 0x01, 0x00, 0x00, 0x40, 0x00, 0x00, 0x00, 0x40, 0x01, 0x00, 0x00, 0x00, 0x00,
        0x30, 0x31, 0x32, 0x33, 0x34, 0x35, 0x36, 0x37, 0x38, 0x39, 0x30, 0x31, 0x32, 0x33, 0x34,
        0x35, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
        0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
        0xFF, 0xFF, 0xFF, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    ];

    fn first_burst() -> BurstHeader {
        BurstHeader {
            start: Timestamp {
                sec: 0x6AC3_8F7D,
                usec: 0x0001_BE17,
            },
            write: Timestamp {
                sec: 0x6AC3_8F7D,
                usec: 0x0001_BE1B,
            },
            burst_id: 1,
            burst_size: 16384,
            seqno: 16385,
        }
    }

    #[test]
    fn parse_reads_each_field_at_its_offset() {
        assert_eq!(BurstHeader::parse(&FIRST_BURST), Ok(first_burst()));
    }

    #[test]
    fn encode_writes_the_stock_first_burst() {
        let mut out = [0xAA; 121];
        assert_eq!(first_burst().encode(&mut out), Ok(BURST_HEADER_LEN));
        assert_eq!(out[..120], FIRST_BURST);
        assert_eq!(out[120], 0xAA);
    }

    #[test]
    fn the_sequence_count_has_a_high_word() {
        let mut header = first_burst();
        header.seqno = 0x3_0000_0005;
        let mut out = [0; 120];
        header.encode(&mut out).unwrap();
        assert_eq!(out[52..60], [0, 0, 0, 5, 0, 0, 0, 3]);
        assert_eq!(BurstHeader::parse(&out).unwrap().seqno, header.seqno);
    }

    #[test]
    fn short_input_is_incomplete_and_a_short_buffer_is_refused() {
        assert_eq!(
            BurstHeader::parse(&FIRST_BURST[..119]),
            Err(Error::Incomplete)
        );
        let mut out = [7; 119];
        assert_eq!(first_burst().encode(&mut out), Err(BufferTooSmall));
        assert_eq!(out, [7; 119]);
    }
}
