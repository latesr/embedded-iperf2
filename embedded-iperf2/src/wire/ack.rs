use super::{BufferTooSmall, Error, get_u32, put_u32, take, take_mut};

/// The length of the header ack.
pub const HEADER_ACK_LEN: usize = 28;

/// The `type` of a header ack, `CLIENTHDRACK`.
const ACK_TYPE: u32 = 2;

/// The 28 bytes a TCP server sends after the test header (W-9).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HeaderAck {
    /// The server's version, high word.
    pub version_u: u32,
    /// The server's version, low word.
    pub version_l: u32,
}

impl HeaderAck {
    /// Reads the header ack from the start of `input`.
    ///
    /// Returns `Incomplete` when `input` is shorter than [`HEADER_ACK_LEN`]. Returns `Invalid` when `type` is
    /// not 2 or `length` is not 28.
    pub fn parse(input: &[u8]) -> Result<Self, Error> {
        let bytes = take::<HEADER_ACK_LEN>(input)?;
        if get_u32(bytes, 0) != ACK_TYPE || get_u32(bytes, 4) != HEADER_ACK_LEN as u32 {
            return Err(Error::Invalid);
        }
        Ok(Self {
            version_u: get_u32(bytes, 12),
            version_l: get_u32(bytes, 16),
        })
    }

    /// Writes the header ack to the start of `out` and returns [`HEADER_ACK_LEN`].
    ///
    /// Returns `BufferTooSmall` when `out` is shorter than [`HEADER_ACK_LEN`], and writes nothing.
    pub fn encode(&self, out: &mut [u8]) -> Result<usize, BufferTooSmall> {
        let bytes = take_mut::<HEADER_ACK_LEN>(out)?;
        bytes.fill(0);
        put_u32(bytes, 0, ACK_TYPE);
        put_u32(bytes, 4, HEADER_ACK_LEN as u32);
        put_u32(bytes, 12, self.version_u);
        put_u32(bytes, 16, self.version_l);
        Ok(HEADER_ACK_LEN)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The header ack of the `tcp` sample.
    const ACK: [u8; 28] = [
        0, 0, 0, 2, 0, 0, 0, 0x1C, 0, 0, 0, 0, 0, 2, 0, 2, 0, 2, 0, 3, 0, 0, 0, 0, 0, 0, 0, 0,
    ];

    const VERSION: HeaderAck = HeaderAck {
        version_u: 0x0002_0002,
        version_l: 0x0002_0003,
    };

    #[test]
    fn parse_reads_the_version() {
        assert_eq!(HeaderAck::parse(&ACK), Ok(VERSION));
    }

    #[test]
    fn encode_writes_the_stock_ack() {
        let mut out = [0xAA; 29];
        assert_eq!(VERSION.encode(&mut out), Ok(HEADER_ACK_LEN));
        assert_eq!(out[..28], ACK);
        assert_eq!(out[28], 0xAA);
    }

    #[test]
    fn another_type_or_length_is_invalid() {
        let mut other_type = ACK;
        other_type[3] = 1;
        assert_eq!(HeaderAck::parse(&other_type), Err(Error::Invalid));
        let mut other_length = ACK;
        other_length[7] = 52;
        assert_eq!(HeaderAck::parse(&other_length), Err(Error::Invalid));
    }

    #[test]
    fn short_input_is_incomplete_and_a_short_buffer_is_refused() {
        assert_eq!(HeaderAck::parse(&ACK[..27]), Err(Error::Incomplete));
        let mut out = [7; 27];
        assert_eq!(VERSION.encode(&mut out), Err(BufferTooSmall));
        assert_eq!(out, [7; 27]);
    }
}
