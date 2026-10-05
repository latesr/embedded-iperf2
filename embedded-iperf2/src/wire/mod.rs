//! The iperf 2.2.2 wire formats: the UDP prefix, the test header, the header ack, the burst header and the
//! server report.
//!
//! Every field is big-endian. Each message has a fixed length. `parse` reads one message from the start of a
//! slice, and `encode` writes one to the start of a slice and returns its length. `REQUIREMENTS.md` states each
//! layout under W-1 to W-13.

mod ack;
mod burst;
pub mod flags;
mod header;
mod prefix;
mod report;

pub use ack::{HEADER_ACK_LEN, HeaderAck};
pub use burst::{BURST_HEADER_LEN, BurstHeader};
pub use header::{Amount, TEST_HEADER_LEN, TestHeader, header_length};
pub use prefix::{PREFIX_LEN, Prefix};
pub use report::{SERVER_REPORT_LEN, ServerReport};

/// Why a slice does not hold a message.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// The slice ends before the message does. More bytes may complete it.
    Incomplete,
    /// The bytes cannot be the message, whatever follows them.
    Invalid,
}

/// The slice given to `encode` is shorter than the message.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BufferTooSmall;

/// A time as whole seconds and the microseconds below one second, as the wire holds it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Timestamp {
    /// Whole seconds.
    pub sec: u32,
    /// Microseconds below one second.
    pub usec: u32,
}

/// The first `N` bytes of `input`, or `Incomplete` when it is shorter.
fn take<const N: usize>(input: &[u8]) -> Result<&[u8; N], Error> {
    input
        .get(..N)
        .and_then(|bytes| bytes.try_into().ok())
        .ok_or(Error::Incomplete)
}

/// The first `N` bytes of `out`, or `BufferTooSmall` when it is shorter.
fn take_mut<const N: usize>(out: &mut [u8]) -> Result<&mut [u8; N], BufferTooSmall> {
    out.get_mut(..N)
        .and_then(|bytes| bytes.try_into().ok())
        .ok_or(BufferTooSmall)
}

/// The big-endian `u32` at `at`. Each caller passes an offset that fits in `N`.
fn get_u32<const N: usize>(bytes: &[u8; N], at: usize) -> u32 {
    bytes
        .get(at..at.saturating_add(4))
        .and_then(|word| word.try_into().ok())
        .map_or(0, u32::from_be_bytes)
}

/// The big-endian `u16` at `at`. Each caller passes an offset that fits in `N`.
fn get_u16<const N: usize>(bytes: &[u8; N], at: usize) -> u16 {
    bytes
        .get(at..at.saturating_add(2))
        .and_then(|word| word.try_into().ok())
        .map_or(0, u16::from_be_bytes)
}

/// Writes `value` big-endian at `at`. Each caller passes an offset that fits in `N`.
fn put_u32<const N: usize>(bytes: &mut [u8; N], at: usize, value: u32) {
    if let Some(word) = bytes.get_mut(at..at.saturating_add(4)) {
        word.copy_from_slice(&value.to_be_bytes());
    }
}

/// Writes `value` big-endian at `at`. Each caller passes an offset that fits in `N`.
fn put_u16<const N: usize>(bytes: &mut [u8; N], at: usize, value: u16) {
    if let Some(word) = bytes.get_mut(at..at.saturating_add(2)) {
        word.copy_from_slice(&value.to_be_bytes());
    }
}

/// The 64-bit value whose high word is `high` and low word is `low`.
fn join(high: u32, low: u32) -> u64 {
    (u64::from(high) << 32) | u64::from(low)
}

/// The high and low words of `value`.
fn split(value: u64) -> (u32, u32) {
    ((value >> 32) as u32, value as u32)
}
