//! The TCP sender and the reader of a reverse test's bursts.

use crate::config::TestError;
use crate::meter::{SECOND_US, fill, timestamp};
use crate::wire::{BURST_HEADER_LEN, BurstHeader, TEST_HEADER_LEN, Timestamp};

/// When a sender stops.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Stop {
    /// After this many microseconds from its first chunk.
    After(u64),
    /// After this many bytes.
    Bytes(u64),
}

/// What a sender does next.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Next {
    /// The first `n` bytes of the buffer are the next chunk or datagram.
    Send(usize),
    /// The next send must wait until this time.
    Wait(u64),
    /// The sender has sent everything.
    Finished,
}

/// What the bytes of a TCP stream are.
#[derive(Debug)]
pub(crate) enum Content {
    /// The test header, then fill: a client's forward test.
    Header([u8; TEST_HEADER_LEN]),
    /// Bursts of this many bytes, each with a burst header: a server's reverse test (P-16).
    Bursts(u32),
}

/// Sends a TCP stream until its stop, no faster than its rate.
#[derive(Debug)]
pub(crate) struct TcpSender {
    content: Content,
    stop: Stop,
    rate: u64,
    sent: u64,
    start_us: Option<u64>,
    /// The burst of the header built last, and its write time. A header cut by a short write is built again with
    /// the same time.
    burst_write: Option<(u64, Timestamp)>,
    /// How many bytes at the start of the buffer hold a header rather than fill, or `None` before the first chunk
    /// has filled the buffer. The sender fills the buffer once, then rewrites only these bytes. Other code must
    /// not write the buffer while the sender sends.
    header_bytes: Option<usize>,
}

impl TcpSender {
    pub(crate) fn new(content: Content, stop: Stop, rate: u64) -> Self {
        Self {
            content,
            stop,
            rate,
            sent: 0,
            start_us: None,
            burst_write: None,
            header_bytes: None,
        }
    }

    /// The bytes sent so far, a header included.
    pub(crate) fn sent_bytes(&self) -> u64 {
        self.sent
    }

    /// Builds the next chunk in `buffer`.
    pub(crate) fn poll(&mut self, now_us: u64, buffer: &mut [u8]) -> Next {
        let start = *self.start_us.get_or_insert(now_us);
        let left = match self.stop {
            // A burst header states its length, so the last burst is sent whole (P-16).
            Stop::After(duration) if now_us.saturating_sub(start) >= duration => {
                match self.burst_left() {
                    0 => return Next::Finished,
                    left => left,
                }
            }
            Stop::After(_) => u64::MAX,
            Stop::Bytes(total) if self.sent >= total => return Next::Finished,
            Stop::Bytes(total) => total.saturating_sub(self.sent),
        };
        if let Some(until) = self.paced_until(start)
            && now_us < until
        {
            return Next::Wait(until);
        }
        let mut length = (buffer.len() as u64).min(left);
        if let Content::Bursts(size) = self.content {
            let into_burst = self.sent.checked_rem(u64::from(size)).unwrap_or_default();
            length = length.min(u64::from(size).saturating_sub(into_burst));
        }
        let stale = self.header_bytes.unwrap_or(buffer.len());
        fill(buffer.get_mut(..stale).unwrap_or_default());
        let length = length as usize;
        let chunk = buffer.get_mut(..length).unwrap_or_default();
        self.header_bytes = Some(self.build(chunk, now_us, start));
        Next::Send(length)
    }

    /// The bytes left of the burst under way, or 0 between bursts and for a stream without bursts.
    fn burst_left(&self) -> u64 {
        match self.content {
            Content::Bursts(size) => {
                let into = self.sent.checked_rem(u64::from(size)).unwrap_or_default();
                if into == 0 {
                    0
                } else {
                    u64::from(size).saturating_sub(into)
                }
            }
            Content::Header(_) => 0,
        }
    }

    /// Records that the caller wrote `count` bytes of the last chunk.
    pub(crate) fn sent(&mut self, count: usize) {
        self.sent = self.sent.saturating_add(count as u64);
    }

    /// When the bytes sent so far are due at the rate. `None` without a rate.
    fn paced_until(&self, start: u64) -> Option<u64> {
        let bits_us = u128::from(self.sent).saturating_mul(8 * u128::from(SECOND_US));
        let due = bits_us.checked_div(u128::from(self.rate))?;
        Some(start.saturating_add(u64::try_from(due).unwrap_or(u64::MAX)))
    }

    /// Writes the part of a header that the next chunk starts with over the fill in `chunk`. Returns how many
    /// bytes it wrote.
    fn build(&mut self, chunk: &mut [u8], now_us: u64, start: u64) -> usize {
        match &self.content {
            Content::Header(header) => {
                let from = header
                    .get(self.sent.min(TEST_HEADER_LEN as u64) as usize..)
                    .unwrap_or_default();
                write_start(chunk, from)
            }
            Content::Bursts(size) => {
                let size = u64::from(*size);
                let index = self.sent.checked_div(size).unwrap_or_default();
                let into_burst = self.sent.checked_rem(size).unwrap_or_default() as usize;
                if into_burst >= BURST_HEADER_LEN {
                    return 0;
                }
                let write = match self.burst_write {
                    Some((built, write)) if built == index => write,
                    _ => {
                        let write = timestamp(now_us);
                        self.burst_write = Some((index, write));
                        write
                    }
                };
                let burst_size = match self.stop {
                    Stop::Bytes(total) => {
                        size.min(total.saturating_sub(index.saturating_mul(size)))
                    }
                    Stop::After(_) => size,
                };
                let header = BurstHeader {
                    start: timestamp(start),
                    write,
                    burst_id: index.saturating_add(1) as u32,
                    burst_size: burst_size as u32,
                    seqno: index
                        .saturating_mul(size)
                        .saturating_add(burst_size)
                        .saturating_add(1),
                };
                let mut bytes = [0; BURST_HEADER_LEN];
                if header.encode(&mut bytes).is_err() {
                    return 0;
                }
                write_start(chunk, bytes.get(into_burst..).unwrap_or_default())
            }
        }
    }
}

/// Copies as much of `from` as fits to the start of `chunk`. Returns how many bytes it copied.
fn write_start(chunk: &mut [u8], from: &[u8]) -> usize {
    let count = from.len().min(chunk.len());
    if let (Some(to), Some(from)) = (chunk.get_mut(..count), from.get(..count)) {
        to.copy_from_slice(from);
    }
    count
}

/// Reads the bursts of a reverse TCP test (P-5), and counts the bytes.
#[derive(Debug)]
pub(crate) struct BurstReader {
    bytes: u64,
    /// The bytes of the current burst read so far, and its size once its header is complete.
    into_burst: u64,
    burst_size: Option<u64>,
    header: [u8; BURST_HEADER_LEN],
}

impl BurstReader {
    pub(crate) fn new() -> Self {
        Self {
            bytes: 0,
            into_burst: 0,
            burst_size: None,
            header: [0; BURST_HEADER_LEN],
        }
    }

    /// The bytes received so far.
    pub(crate) fn bytes(&self) -> u64 {
        self.bytes
    }

    /// Reads `bytes`. Fails on a burst header with a `burst_size` below the length of the header.
    pub(crate) fn received(&mut self, mut bytes: &[u8]) -> Result<(), TestError> {
        self.bytes = self.bytes.saturating_add(bytes.len() as u64);
        while !bytes.is_empty() {
            let size = match self.burst_size {
                Some(size) => size,
                None => {
                    let at = self.into_burst as usize;
                    let take = (BURST_HEADER_LEN.saturating_sub(at)).min(bytes.len());
                    if let (Some(to), Some(from)) = (
                        self.header.get_mut(at..at.saturating_add(take)),
                        bytes.get(..take),
                    ) {
                        to.copy_from_slice(from);
                    }
                    bytes = bytes.get(take..).unwrap_or_default();
                    self.into_burst = self.into_burst.saturating_add(take as u64);
                    if self.into_burst < BURST_HEADER_LEN as u64 {
                        return Ok(());
                    }
                    let header =
                        BurstHeader::parse(&self.header).map_err(|_| TestError::Protocol)?;
                    if (header.burst_size as usize) < BURST_HEADER_LEN {
                        return Err(TestError::Protocol);
                    }
                    self.burst_size = Some(u64::from(header.burst_size));
                    u64::from(header.burst_size)
                }
            };
            let take = size.saturating_sub(self.into_burst).min(bytes.len() as u64);
            bytes = bytes.get(take as usize..).unwrap_or_default();
            self.into_burst = self.into_burst.saturating_add(take);
            if self.into_burst == size {
                self.into_burst = 0;
                self.burst_size = None;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::arithmetic_side_effects)]
mod tests {
    use super::*;

    /// Asserts that `buffer` holds fill from `from` to its end.
    fn assert_fill(buffer: &[u8], from: usize) {
        for (at, byte) in buffer.iter().enumerate().skip(from) {
            assert_eq!(*byte, b'0' + (at % 10) as u8, "byte {at}");
        }
    }

    fn send(sender: &mut TcpSender, buffer: &mut [u8]) -> usize {
        match sender.poll(0, buffer) {
            Next::Send(length) => length,
            next => panic!("{next:?}"),
        }
    }

    #[test]
    fn a_sender_keeps_its_fill_between_chunks() {
        let mut sender = TcpSender::new(
            Content::Header([0xAA; TEST_HEADER_LEN]),
            Stop::Bytes(1000),
            0,
        );
        let mut buffer = [0; 100];
        let length = send(&mut sender, &mut buffer);
        sender.sent(length);
        buffer[99] = b'X';
        assert_eq!(send(&mut sender, &mut buffer), 100);
        assert_eq!(buffer[99], b'X');
    }

    #[test]
    fn the_test_header_leaves_the_chunks_after_it() {
        let mut sender = TcpSender::new(
            Content::Header([0xAA; TEST_HEADER_LEN]),
            Stop::Bytes(1000),
            0,
        );
        let mut buffer = [0; 100];
        assert_eq!(send(&mut sender, &mut buffer), 100);
        assert_eq!(buffer[..TEST_HEADER_LEN], [0xAA; TEST_HEADER_LEN]);
        assert_fill(&buffer, TEST_HEADER_LEN);
        sender.sent(100);
        assert_eq!(send(&mut sender, &mut buffer), 100);
        assert_fill(&buffer, 0);
    }

    #[test]
    fn a_cut_test_header_continues_in_the_next_chunk_then_leaves() {
        let mut sender = TcpSender::new(
            Content::Header([0xAA; TEST_HEADER_LEN]),
            Stop::Bytes(1000),
            0,
        );
        let mut buffer = [0; 100];
        send(&mut sender, &mut buffer);
        sender.sent(10);
        assert_eq!(send(&mut sender, &mut buffer), 100);
        assert_eq!(buffer[..TEST_HEADER_LEN - 10], [0xAA; TEST_HEADER_LEN - 10]);
        assert_fill(&buffer, TEST_HEADER_LEN - 10);
        sender.sent(100);
        send(&mut sender, &mut buffer);
        assert_fill(&buffer, 0);
    }

    #[test]
    fn a_burst_header_leaves_the_chunks_after_it() {
        let mut sender = TcpSender::new(Content::Bursts(300), Stop::Bytes(600), 0);
        let mut buffer = [0; 200];
        assert_eq!(send(&mut sender, &mut buffer), 200);
        sender.sent(200);
        assert_eq!(send(&mut sender, &mut buffer), 100);
        assert_fill(&buffer[..100], 0);
    }
}
