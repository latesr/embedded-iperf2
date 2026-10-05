//! The bits of a test header's `flags`, `upperflags` and `lowerflags`, with the names iperf 2.2.2 gives them.

/// The 24-byte first part of the header is valid. A stock client sets it for `-r` and `-d`, and for a reverse UDP
/// test with `-l`.
pub const VERSION1: u32 = 0x8000_0000;
/// The 40-byte extension follows the first part.
pub const EXTEND: u32 = 0x4000_0000;
/// `upperflags` holds UDP test flags.
pub const UDPTESTS: u32 = 0x2000_0000;
/// A permit key follows the header.
pub const KEYCHECK: u32 = 0x1000_0000;
/// UDP packet IDs are 64 bits, with `id2` as the high word.
pub const SEQNO64B: u32 = 0x0800_0000;
/// A test that may run reverse or full duplex.
pub const VERSION2: u32 = 0x0400_0000;
/// The client asks for the header ack, with `-X`.
pub const V2PEERDETECT: u32 = 0x0200_0000;
/// A bounceback test, with another header layout.
pub const BOUNCEBACK: u32 = 0x0080_0000;
/// The low 16 bits hold the header length, shifted left by 1.
pub const LEN_BIT: u32 = 0x0001_0000;
/// The bits of the header length, shifted left by 1.
pub const LEN_MASK: u32 = 0x0000_FFFE;

/// The `upperflags` bit of a reverse test.
pub const REVERSE: u16 = 0x0400;

/// Not a flag: the fill `6789` that a UDP client with `-C` sends where `flags` would be. A UDP datagram with it has
/// no test header (P-11).
pub const UDP_COMPAT_FILL: u32 = 0x3637_3839;

#[cfg(test)]
mod tests {
    use super::*;

    /// The wire module joins these with `|`, and `.cargo/mutants.toml` skips the mutant to `^` because no two
    /// overlap.
    #[test]
    fn each_flag_is_one_bit_and_none_overlap() {
        let bits = [
            VERSION1,
            EXTEND,
            UDPTESTS,
            KEYCHECK,
            SEQNO64B,
            VERSION2,
            V2PEERDETECT,
            BOUNCEBACK,
            LEN_BIT,
        ];
        let mut seen = LEN_MASK;
        for bit in bits {
            assert_eq!(bit.count_ones(), 1, "{bit:#x}");
            assert_eq!(seen & bit, 0, "{bit:#x}");
            seen |= bit;
        }
    }
}
