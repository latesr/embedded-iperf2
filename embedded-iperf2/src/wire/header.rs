use super::{
    BufferTooSmall, Error, PREFIX_LEN, flags, get_u16, get_u32, put_u16, put_u32, take, take_mut,
};

/// The length of the test header after any prefix: `client_hdr_v1` and `client_hdrext`.
pub const TEST_HEADER_LEN: usize = 64;

/// The longest header length that `flags` can state.
const MAX_STATED_LEN: usize = 256;

/// The longest duration `mAmount` holds: its negation must keep the top bit set.
const MAX_CENTISECONDS: u32 = 0x8000_0000;

/// The length of a test: a duration or a byte count (W-6).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Amount {
    /// A duration in units of 10 ms, from 1 to 2^31. The encoder clamps a duration outside that range.
    Time {
        /// The duration in hundredths of a second.
        centiseconds: u32,
    },
    /// A byte count below 2^31. The encoder masks a larger count to 31 bits.
    Bytes(u32),
}

/// The test header of W-4, as the 64 bytes after any prefix. The flag words stay raw, so the caller decides
/// what each mode means.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TestHeader {
    /// The `flags` word. [`flags`](super::flags) names its bits.
    pub flags: u32,
    /// The client's stream count, 1 without `-P`.
    pub num_threads: u32,
    /// The port the client connected to.
    pub port: u32,
    /// The client's `-l`, or 0 when the client did not set it.
    pub buf_len: u32,
    /// The length of the test.
    pub amount: Amount,
    /// The `upperflags` word.
    pub upper_flags: u16,
    /// The `lowerflags` word.
    pub lower_flags: u16,
    /// The sender's version, high word. 0x00020002 for 2.2.2.
    pub version_u: u32,
    /// The sender's version, low word. 0x00020003 for 2.2.2.
    pub version_l: u32,
    /// The rate in bits per second, below 2^56. 0 means no rate.
    pub rate: u64,
}

impl TestHeader {
    /// Reads the test header from the start of `input`, which follows the prefix in a UDP datagram.
    ///
    /// Returns `Incomplete` when `input` is shorter than [`TEST_HEADER_LEN`]. The fields are not checked:
    /// [`header_length`] checks the length that `flags` states.
    pub fn parse(input: &[u8]) -> Result<Self, Error> {
        let bytes = take::<TEST_HEADER_LEN>(input)?;
        let amount = get_u32(bytes, 20);
        Ok(Self {
            flags: get_u32(bytes, 0),
            num_threads: get_u32(bytes, 4),
            port: get_u32(bytes, 8),
            buf_len: get_u32(bytes, 12),
            amount: if amount & 0x8000_0000 == 0 {
                Amount::Bytes(amount)
            } else {
                Amount::Time {
                    centiseconds: amount.wrapping_neg(),
                }
            },
            upper_flags: get_u16(bytes, 32),
            lower_flags: get_u16(bytes, 34),
            version_u: get_u32(bytes, 36),
            version_l: get_u32(bytes, 40),
            rate: u64::from(get_u32(bytes, 48)) | (u64::from(get_u32(bytes, 52) >> 8) << 32),
        })
    }

    /// Writes the test header to the start of `out` and returns [`TEST_HEADER_LEN`]. The fields a v1 client
    /// leaves at 0 are written as 0.
    ///
    /// Returns `BufferTooSmall` when `out` is shorter than [`TEST_HEADER_LEN`], and writes nothing.
    pub fn encode(&self, out: &mut [u8]) -> Result<usize, BufferTooSmall> {
        let bytes = take_mut::<TEST_HEADER_LEN>(out)?;
        bytes.fill(0);
        put_u32(bytes, 0, self.flags);
        put_u32(bytes, 4, self.num_threads);
        put_u32(bytes, 8, self.port);
        put_u32(bytes, 12, self.buf_len);
        let amount = match self.amount {
            Amount::Time { centiseconds } => centiseconds.clamp(1, MAX_CENTISECONDS).wrapping_neg(),
            Amount::Bytes(count) => count & 0x7FFF_FFFF,
        };
        put_u32(bytes, 20, amount);
        put_u16(bytes, 32, self.upper_flags);
        put_u16(bytes, 34, self.lower_flags);
        put_u32(bytes, 36, self.version_u);
        put_u32(bytes, 40, self.version_l);
        put_u32(bytes, 48, self.rate as u32);
        put_u32(bytes, 52, ((self.rate >> 32) as u32) << 8);
        Ok(TEST_HEADER_LEN)
    }

    /// Returns whether the header asks for a reverse test: `VERSION2` in `flags` and `REVERSE` in `upperflags`.
    ///
    /// It does not check `EXTEND`. A stock server reads `upperflags` only with `EXTEND`, and every 2.2.2 client
    /// sets it. A caller that must match the stock server checks `EXTEND` too.
    pub fn is_reverse(&self) -> bool {
        self.flags & flags::VERSION2 != 0 && self.upper_flags & flags::REVERSE != 0
    }
}

/// The header length that `flags` states (W-7), counted from the start of the stream for TCP and of the datagram
/// for UDP, so a UDP length includes the prefix.
///
/// Returns `None` when `flags` does not have `VERSION1`, `VERSION2`, or `EXTEND` set, because the stream then does
/// not have a test header. Returns `None` for UDP when `flags` is [`UDP_COMPAT_FILL`](flags::UDP_COMPAT_FILL), as a
/// stock server does before it reads the bits (P-11). Returns `Invalid` for a length above 256, or below the 64
/// bytes of a TCP header or the 80 bytes of a UDP header.
pub fn header_length(flags: u32, udp: bool) -> Result<Option<usize>, Error> {
    if udp && flags == flags::UDP_COMPAT_FILL {
        return Ok(None);
    }
    if flags & (flags::VERSION1 | flags::VERSION2 | flags::EXTEND) == 0 {
        return Ok(None);
    }
    let length = if flags & flags::LEN_BIT != 0 {
        ((flags & flags::LEN_MASK) >> 1) as usize
    } else {
        let first: usize = if flags & (flags::VERSION1 | flags::EXTEND) != 0 {
            24
        } else {
            0
        };
        let extension = if flags & (flags::VERSION2 | flags::EXTEND) != 0 {
            40
        } else {
            0
        };
        first.saturating_add(extension)
    };
    let minimum = if udp {
        PREFIX_LEN.saturating_add(TEST_HEADER_LEN)
    } else {
        TEST_HEADER_LEN
    };
    if (minimum..=MAX_STATED_LEN).contains(&length) {
        Ok(Some(length))
    } else {
        Err(Error::Invalid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The test header of the `udp-reverse` sample, from offset 16.
    const UDP_REVERSE: [u8; 64] = [
        0xEC, 0x01, 0x00, 0xA0, 0, 0, 0, 1, 0, 0, 0xEF, 0x0C, 0, 0, 2, 0, 0, 0, 0, 0, 0xFF, 0xFF,
        0xFF, 0x38, 0, 0, 0, 0, 0, 0, 0, 0, 0x04, 0x00, 0, 0, 0, 2, 0, 2, 0, 2, 0, 3, 0, 0, 0, 0,
        0, 4, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    ];

    fn udp_reverse() -> TestHeader {
        TestHeader {
            flags: 0xEC01_00A0,
            num_threads: 1,
            port: 0xEF0C,
            buf_len: 512,
            amount: Amount::Time { centiseconds: 200 },
            upper_flags: flags::REVERSE,
            lower_flags: 0,
            version_u: 0x0002_0002,
            version_l: 0x0002_0003,
            rate: 262_144,
        }
    }

    #[test]
    fn parse_reads_each_field_at_its_offset() {
        assert_eq!(TestHeader::parse(&UDP_REVERSE), Ok(udp_reverse()));
    }

    #[test]
    fn encode_writes_the_same_bytes() {
        let mut out = [0xAA; 64];
        assert_eq!(udp_reverse().encode(&mut out), Ok(TEST_HEADER_LEN));
        assert_eq!(out, UDP_REVERSE);
    }

    #[test]
    fn a_byte_count_has_its_top_bit_clear_and_a_duration_set() {
        let mut header = udp_reverse();
        header.amount = Amount::Bytes(65536);
        let mut out = [0; 64];
        header.encode(&mut out).unwrap();
        assert_eq!(out[20..24], [0, 1, 0, 0]);
        assert_eq!(
            TestHeader::parse(&out).unwrap().amount,
            Amount::Bytes(65536)
        );
        header.amount = Amount::Bytes(0x8000_0001);
        header.encode(&mut out).unwrap();
        assert_eq!(out[20..24], [0, 0, 0, 1]);
        header.amount = Amount::Time {
            centiseconds: 0x8000_0000,
        };
        header.encode(&mut out).unwrap();
        assert_eq!(out[20..24], [0x80, 0, 0, 0]);
        assert_eq!(TestHeader::parse(&out).unwrap().amount, header.amount);
    }

    #[test]
    fn the_rate_splits_into_lrate_and_urate_shifted_by_8() {
        let mut header = udp_reverse();
        header.rate = 0x12_3456_789A_BCDE;
        let mut out = [0; 64];
        header.encode(&mut out).unwrap();
        assert_eq!(
            out[48..56],
            [0x78, 0x9A, 0xBC, 0xDE, 0x12, 0x34, 0x56, 0x00]
        );
        assert_eq!(TestHeader::parse(&out).unwrap().rate, header.rate);
    }

    #[test]
    fn short_input_is_incomplete_and_a_short_buffer_is_refused() {
        assert_eq!(
            TestHeader::parse(&UDP_REVERSE[..63]),
            Err(Error::Incomplete)
        );
        let mut out = [7; 63];
        assert_eq!(udp_reverse().encode(&mut out), Err(BufferTooSmall));
        assert_eq!(out, [7; 63]);
    }

    #[test]
    fn reverse_needs_version2_and_the_upperflags_bit() {
        assert!(udp_reverse().is_reverse());
        let mut header = udp_reverse();
        header.flags &= !flags::VERSION2;
        assert!(!header.is_reverse());
        let mut header = udp_reverse();
        header.upper_flags = 0;
        assert!(!header.is_reverse());
    }

    #[test]
    fn the_stated_length_comes_from_the_low_bits() {
        assert_eq!(header_length(0x4001_0080, false), Ok(Some(64)));
        assert_eq!(header_length(0x4801_00A0, true), Ok(Some(80)));
        assert_eq!(header_length(0xEC01_00A0, true), Ok(Some(80)));
        assert_eq!(header_length(0x4001_0200, false), Ok(Some(256)));
    }

    #[test]
    fn without_len_bit_the_length_comes_from_the_parts() {
        assert_eq!(header_length(flags::EXTEND, false), Ok(Some(64)));
        assert_eq!(
            header_length(flags::VERSION1 | flags::VERSION2, false),
            Ok(Some(64))
        );
        assert_eq!(
            header_length(flags::VERSION1 | flags::VERSION2, true),
            Err(Error::Invalid)
        );
        assert_eq!(header_length(flags::VERSION1, false), Err(Error::Invalid));
        assert_eq!(header_length(flags::VERSION2, false), Err(Error::Invalid));
    }

    #[test]
    fn no_header_part_means_no_header() {
        assert_eq!(header_length(0, false), Ok(None));
        assert_eq!(header_length(0x3031_3233, false), Ok(None));
    }

    #[test]
    fn a_length_out_of_bounds_is_invalid() {
        assert_eq!(header_length(0x4001_0202, false), Err(Error::Invalid));
        assert_eq!(header_length(0x4001_007E, false), Err(Error::Invalid));
        assert_eq!(header_length(0x4801_009E, true), Err(Error::Invalid));
        assert_eq!(header_length(0x4801_0080, true), Err(Error::Invalid));
    }

    #[test]
    fn the_udp_fill_of_a_compat_client_is_no_header() {
        assert_eq!(header_length(flags::UDP_COMPAT_FILL, true), Ok(None));
        assert_eq!(
            header_length(flags::UDP_COMPAT_FILL, false),
            Err(Error::Invalid)
        );
    }

    #[test]
    fn a_duration_outside_its_range_is_clamped() {
        let mut header = udp_reverse();
        let mut out = [0; 64];
        for (centiseconds, written) in [(0, 1), (0x8000_0001, 0x8000_0000), (u32::MAX, 0x8000_0000)]
        {
            header.amount = Amount::Time { centiseconds };
            header.encode(&mut out).unwrap();
            assert_eq!(
                TestHeader::parse(&out).unwrap().amount,
                Amount::Time {
                    centiseconds: written
                }
            );
        }
    }
}
