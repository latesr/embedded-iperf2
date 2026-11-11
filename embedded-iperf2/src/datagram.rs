//! The UDP sender: datagrams with the prefix, an optional test header, and fill, paced to the rate.

use crate::meter::{fill, timestamp};
use crate::stream::{Next, Stop};
use crate::wire::{PREFIX_LEN, Prefix, TEST_HEADER_LEN, TestHeader};

/// Sends the datagrams of a UDP test, then builds its end (W-2, W-3).
#[derive(Debug)]
pub(crate) struct UdpSender {
    len: usize,
    /// The test header that a client puts in every datagram. A server's datagrams do not have one (P-21).
    header: Option<TestHeader>,
    stop: Stop,
    /// The nanoseconds between datagrams at the rate, or 0 for no pacing.
    gap_ns: u64,
    /// When the next datagram is due, in nanoseconds on the caller's clock.
    due_ns: u64,
    next_id: i64,
    sent_bytes: u64,
    start_us: Option<u64>,
    /// Whether the buffer holds the fill. The sender fills it once, then rewrites only the prefix and the header.
    /// Other code must not write the buffer while the sender sends.
    filled: bool,
}

impl UdpSender {
    /// Creates a new sender of datagrams of `len` bytes at `rate` bits per second. `len` must be at least the
    /// prefix, and the header when there is one.
    pub(crate) fn new(len: usize, header: Option<TestHeader>, stop: Stop, rate: u64) -> Self {
        let gap_ns = if rate == 0 {
            0
        } else {
            let bits_ns = (len as u128).saturating_mul(8_000_000_000);
            let gap = bits_ns.checked_div(u128::from(rate)).unwrap_or_default();
            u64::try_from(gap).unwrap_or(u64::MAX)
        };
        Self {
            len,
            header,
            stop,
            gap_ns,
            due_ns: 0,
            next_id: 1,
            sent_bytes: 0,
            start_us: None,
            filled: false,
        }
    }

    /// The bytes of the datagrams of data sent so far.
    pub(crate) fn sent_bytes(&self) -> u64 {
        self.sent_bytes
    }

    /// The shortest datagram this sender builds: the prefix, and the header when there is one.
    fn shortest(&self) -> usize {
        match self.header {
            Some(_) => PREFIX_LEN.saturating_add(TEST_HEADER_LEN),
            None => PREFIX_LEN,
        }
    }

    /// Builds the next datagram of data in `buffer`.
    pub(crate) fn poll(&mut self, now_us: u64, buffer: &mut [u8]) -> Next {
        let start = *self.start_us.get_or_insert_with(|| {
            self.due_ns = now_us.saturating_mul(1_000);
            now_us
        });
        let length = match self.stop {
            Stop::After(duration) if now_us.saturating_sub(start) >= duration => {
                return Next::Finished;
            }
            Stop::After(_) => self.len,
            Stop::Bytes(total) if self.sent_bytes >= total => return Next::Finished,
            Stop::Bytes(total) => {
                let left = total.saturating_sub(self.sent_bytes);
                (self.len as u64).min(left).max(self.shortest() as u64) as usize
            }
        };
        if now_us.saturating_mul(1_000) < self.due_ns {
            return Next::Wait(self.due_ns.div_ceil(1_000));
        }
        self.build(buffer, self.next_id, length, now_us);
        Next::Send(length)
    }

    /// Records that the datagram of `length` bytes from the last `poll` went out.
    pub(crate) fn sent(&mut self, length: usize) {
        self.next_id = self.next_id.saturating_add(1);
        self.sent_bytes = self.sent_bytes.saturating_add(length as u64);
        self.due_ns = self.due_ns.saturating_add(self.gap_ns);
    }

    /// Builds the end of the test in `buffer`: the datagram with packet ID `-N` and then `-(N + repeat)` (W-3),
    /// as long as a datagram of data. Returns its length.
    pub(crate) fn end(&mut self, repeat: u32, now_us: u64, buffer: &mut [u8]) -> usize {
        let id = self
            .next_id
            .saturating_add(i64::from(repeat))
            .saturating_neg();
        self.build(buffer, id, self.len, now_us);
        self.len
    }

    /// Builds the datagram that asks for a reverse test in `buffer`: packet ID 1 and the header (P-6). Returns
    /// its length.
    pub(crate) fn ask(&mut self, now_us: u64, buffer: &mut [u8]) -> usize {
        self.build(buffer, 1, self.len, now_us);
        self.len
    }

    fn build(&mut self, buffer: &mut [u8], id: i64, length: usize, now_us: u64) {
        if !self.filled {
            fill(buffer.get_mut(..self.len).unwrap_or_default());
            self.filled = true;
        }
        let Some(datagram) = buffer.get_mut(..length) else {
            return;
        };
        let prefix = Prefix {
            packet_id: id,
            sent: timestamp(now_us),
        };
        let _ = prefix.encode(datagram);
        if let (Some(header), Some(rest)) = (self.header, datagram.get_mut(PREFIX_LEN..)) {
            let _ = header.encode(rest);
        }
    }
}

#[cfg(test)]
#[allow(clippy::arithmetic_side_effects)]
mod tests {
    use super::*;

    fn send(sender: &mut UdpSender, buffer: &mut [u8]) -> usize {
        match sender.poll(0, buffer) {
            Next::Send(length) => length,
            next => panic!("{next:?}"),
        }
    }

    #[test]
    fn a_sender_keeps_its_fill_between_datagrams() {
        let mut sender = UdpSender::new(100, None, Stop::Bytes(1000), 0);
        let mut buffer = [0; 100];
        let length = send(&mut sender, &mut buffer);
        sender.sent(length);
        buffer[99] = b'X';
        assert_eq!(send(&mut sender, &mut buffer), 100);
        assert_eq!(buffer[99], b'X');
    }

    #[test]
    fn each_datagram_has_its_own_prefix_and_fill_after_it() {
        let mut sender = UdpSender::new(100, None, Stop::Bytes(1000), 0);
        let mut buffer = [0; 100];
        let length = send(&mut sender, &mut buffer);
        sender.sent(length);
        send(&mut sender, &mut buffer);
        assert_eq!(Prefix::parse(&buffer).unwrap().packet_id, 2);
        for (at, byte) in buffer.iter().enumerate().skip(PREFIX_LEN) {
            assert_eq!(*byte, b'0' + (at % 10) as u8, "byte {at}");
        }
    }
}
