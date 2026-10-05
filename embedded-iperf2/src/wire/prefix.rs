use super::{BufferTooSmall, Error, Timestamp, get_u32, join, put_u32, split, take, take_mut};

/// The length of the prefix.
pub const PREFIX_LEN: usize = 16;

/// The first 16 bytes of every UDP datagram of a test (W-1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Prefix {
    /// The packet ID: `id` as the low 32 bits and `id2` as the high 32 bits. A negative ID ends the test (W-3).
    pub packet_id: i64,
    /// The sender's clock when it sent the datagram.
    pub sent: Timestamp,
}

impl Prefix {
    /// Reads the prefix from the start of `input`.
    ///
    /// Returns `Incomplete` when `input` is shorter than [`PREFIX_LEN`]. Every 16 bytes are a prefix.
    pub fn parse(input: &[u8]) -> Result<Self, Error> {
        let bytes = take::<PREFIX_LEN>(input)?;
        Ok(Self {
            packet_id: join(get_u32(bytes, 12), get_u32(bytes, 0)) as i64,
            sent: Timestamp {
                sec: get_u32(bytes, 4),
                usec: get_u32(bytes, 8),
            },
        })
    }

    /// Writes the prefix to the start of `out` and returns [`PREFIX_LEN`].
    ///
    /// Returns `BufferTooSmall` when `out` is shorter than [`PREFIX_LEN`], and writes nothing.
    pub fn encode(&self, out: &mut [u8]) -> Result<usize, BufferTooSmall> {
        let bytes = take_mut::<PREFIX_LEN>(out)?;
        let (high, low) = split(self.packet_id as u64);
        put_u32(bytes, 0, low);
        put_u32(bytes, 4, self.sent.sec);
        put_u32(bytes, 8, self.sent.usec);
        put_u32(bytes, 12, high);
        Ok(PREFIX_LEN)
    }

    /// The packet ID as a reader without 64-bit IDs takes it: `id` alone, as a signed 32-bit number (P-11).
    pub fn packet_id_32(&self) -> i32 {
        self.packet_id as i32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BYTES: [u8; 16] = [
        0xFF, 0xFF, 0xFF, 0x7E, 0x6A, 0xC3, 0x8B, 0xF1, 0x00, 0x09, 0x3B, 0x26, 0xFF, 0xFF, 0xFF,
        0xFF,
    ];

    const PREFIX: Prefix = Prefix {
        packet_id: -130,
        sent: Timestamp {
            sec: 0x6AC3_8BF1,
            usec: 0x0009_3B26,
        },
    };

    #[test]
    fn id_is_the_low_word_and_id2_the_high_word() {
        assert_eq!(Prefix::parse(&BYTES), Ok(PREFIX));
        let high = [0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2];
        assert_eq!(Prefix::parse(&high).map(|p| p.packet_id), Ok(0x2_0000_0001));
    }

    #[test]
    fn encode_writes_the_same_bytes() {
        let mut out = [0; 17];
        assert_eq!(PREFIX.encode(&mut out), Ok(PREFIX_LEN));
        assert_eq!(out[..16], BYTES);
    }

    #[test]
    fn short_input_is_incomplete_and_a_short_buffer_is_refused() {
        assert_eq!(Prefix::parse(&BYTES[..15]), Err(Error::Incomplete));
        let mut out = [7; 15];
        assert_eq!(PREFIX.encode(&mut out), Err(BufferTooSmall));
        assert_eq!(out, [7; 15]);
    }

    #[test]
    fn the_32_bit_id_ignores_id2() {
        let prefix = Prefix {
            packet_id: 0x5_FFFF_FFFE,
            sent: Timestamp::default(),
        };
        assert_eq!(prefix.packet_id_32(), -2);
    }
}
