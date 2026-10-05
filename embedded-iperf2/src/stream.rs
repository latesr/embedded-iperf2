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
    /// Nothing may be sent before this time.
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
    /// The burst whose header was last built, and its write time. A header cut by a short write is built again
    /// with the same time.
    burst_write: Option<(u64, Timestamp)>,
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
            Stop::After(duration) if now_us.saturating_sub(start) >= duration => {
                return Next::Finished;
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
        let length = length as usize;
        let chunk = buffer.get_mut(..length).unwrap_or_default();
        self.build(chunk, now_us, start);
        Next::Send(length)
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

    fn build(&mut self, chunk: &mut [u8], now_us: u64, start: u64) {
        fill(chunk, self.sent);
        match &self.content {
            Content::Header(header) => {
                if let Some(from) = header.get(self.sent.min(TEST_HEADER_LEN as u64) as usize..) {
                    let count = from.len().min(chunk.len());
                    if let (Some(to), Some(from)) = (chunk.get_mut(..count), from.get(..count)) {
                        to.copy_from_slice(from);
                    }
                }
            }
            Content::Bursts(size) => {
                let size = u64::from(*size);
                let index = self.sent.checked_div(size).unwrap_or_default();
                let into_burst = self.sent.checked_rem(size).unwrap_or_default() as usize;
                if into_burst >= BURST_HEADER_LEN {
                    return;
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
                if header.encode(&mut bytes).is_ok() {
                    let from = bytes.get(into_burst..).unwrap_or_default();
                    let count = from.len().min(chunk.len());
                    if let (Some(to), Some(from)) = (chunk.get_mut(..count), from.get(..count)) {
                        to.copy_from_slice(from);
                    }
                }
            }
        }
    }
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

    /// Reads `bytes`. Fails on a burst header whose `burst_size` is below its own length.
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
