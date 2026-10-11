//! The server: it reads a test header, then receives or, in a reverse test, sends (P-8 to P-16, P-21).

use crate::config::{Action, ConfigError, ServerConfig, TestError, Transport};
use crate::counts::UdpReceiver;
use crate::datagram::UdpSender;
use crate::meter::{Meter, SECOND_US, timestamp};
use crate::report::Report;
use crate::stream::{Content, Next, Stop, TcpSender};
use crate::wire::{
    Amount, BURST_HEADER_LEN, HEADER_ACK_LEN, HeaderAck, PREFIX_LEN, Prefix, ServerReport,
    TestHeader, flags, header_length,
};

/// The longest header a client may state (W-7).
const MAX_HEADER_LEN: usize = 256;
/// The shortest first datagram of a UDP test (P-11).
const MIN_FIRST_DATAGRAM: usize = 40;
/// The longest datagram a reverse UDP test sends, over IPv4 and IPv6 (P-21).
const MAX_DATAGRAM_IPV4: usize = 1470;
const MAX_DATAGRAM_IPV6: usize = 1450;
/// The default length of a reverse TCP burst (P-16).
const DEFAULT_BURST: u32 = 131_072;
/// The smallest `mBufLen` a stock server ignores, one above the 16 MiB it takes.
const TOO_LONG_BUF_LEN: u32 = 16 * 1024 * 1024 + 1;
/// The silence after a server report that ends a UDP test, and the most reports sent (P-12).
const REPORT_WAIT_US: u64 = 250_000;
const REPORT_SENDS: u32 = 9;
/// The version a 2.2.2 server writes in its ack.
const VERSION: HeaderAck = HeaderAck {
    version_u: 0x0002_0002,
    version_l: 0x0002_0003,
};

/// The server of one test. The caller makes a new one for each TCP connection, or for each UDP flow, and
/// passes the time in as `now_us`.
///
/// A UDP server serves one peer. The caller passes it the datagrams of the address that sent the first one it
/// passed, and no others (D-6).
#[derive(Debug)]
pub struct Server<'b> {
    config: ServerConfig,
    buffer: &'b mut [u8],
    meter: Meter,
    state: State,
    now_us: u64,
    /// An interval that ended before bytes that arrived after it, for the next `poll`.
    pending: Option<Report>,
    /// Once the test has ended: the last part of an interval, the final report and the outcome.
    tail: Option<Report>,
    last: Option<Report>,
    outcome: Option<Result<(), TestError>>,
    /// Whether the test that ended received UDP, so its empty intervals hold counts.
    ended_udp: bool,
}

#[derive(Debug)]
enum State {
    /// A TCP server reads the test header.
    TcpHeader {
        stash: [u8; MAX_HEADER_LEN],
        len: usize,
    },
    /// A TCP server sends the header ack, then receives or sends.
    TcpAck {
        sent: usize,
        reverse: Option<TcpSender>,
        bytes: u64,
    },
    /// A forward TCP test receives until the client closes.
    TcpReceiving {
        bytes: u64,
    },
    /// A reverse TCP test sends.
    TcpSending(TcpSender),
    /// A reverse TCP test has sent, and waits for the client to close.
    TcpClosing {
        report: Report,
        close: bool,
    },
    /// A UDP server waits for the first datagram of a test.
    UdpWaiting,
    /// A forward UDP test receives.
    UdpReceiving {
        counts: UdpReceiver,
        wide: bool,
        last_us: u64,
    },
    /// A forward UDP test sends its report, again for each datagram that follows.
    UdpReporting {
        server: ServerReport,
        report: Report,
        sends: u32,
        again: bool,
        last_us: u64,
    },
    /// A reverse UDP test sends.
    UdpSending(UdpSender),
    /// A reverse UDP test sends its one end.
    UdpEnding {
        sender: UdpSender,
        report: Report,
    },
    Over,
}

impl<'b> Server<'b> {
    /// A server for one test, which builds each chunk or datagram it sends in `buffer`.
    ///
    /// Returns `BufLenTooShort` when `buffer` cannot hold a burst header, or a UDP datagram of the longest length
    /// a reverse UDP test sends: 1470 bytes, or 1450 over IPv6 (P-21).
    pub fn new(config: ServerConfig, buffer: &'b mut [u8]) -> Result<Self, ConfigError> {
        let shortest = match (config.transport, config.ipv6) {
            (Transport::Tcp, _) => BURST_HEADER_LEN,
            (Transport::Udp, false) => MAX_DATAGRAM_IPV4,
            (Transport::Udp, true) => MAX_DATAGRAM_IPV6,
        };
        if buffer.len() < shortest {
            return Err(ConfigError::BufLenTooShort);
        }
        let state = match config.transport {
            Transport::Tcp => State::TcpHeader {
                stash: [0; MAX_HEADER_LEN],
                len: 0,
            },
            Transport::Udp => State::UdpWaiting,
        };
        Ok(Self {
            config,
            buffer,
            meter: Meter::new(config.interval_us),
            state,
            now_us: 0,
            pending: None,
            tail: None,
            last: None,
            outcome: None,
            ended_udp: false,
        })
    }

    /// The settings this server was made with.
    pub fn config(&self) -> ServerConfig {
        self.config
    }

    /// What the caller does next, at `now_us`.
    pub fn poll(&mut self, now_us: u64) -> Action<'_> {
        self.now_us = now_us;
        if let Some(report) = self.pending.take() {
            return Action::Report(report);
        }
        if let Some(report) = self.interval(now_us) {
            return Action::Report(report);
        }
        if let Some(report) = self.tail.take().or_else(|| self.last.take()) {
            return Action::Report(report);
        }
        if let Some(outcome) = self.outcome {
            return Action::Done(outcome);
        }
        if self.over_time(now_us) {
            return self.poll(now_us);
        }
        let deadline = self.deadline();
        let next_due = self.meter.next_due();
        let wait = |until: Option<u64>| {
            let until = [until, deadline, next_due].into_iter().flatten().min();
            Action::Wait { until_us: until }
        };
        match &mut self.state {
            State::TcpHeader { .. } | State::UdpWaiting | State::TcpReceiving { .. } => wait(None),
            State::UdpReceiving { .. } => wait(None),
            State::TcpAck { sent, .. } => {
                let mut ack = [0; HEADER_ACK_LEN];
                let _ = VERSION.encode(&mut ack);
                let rest = ack.get(*sent..).unwrap_or_default();
                let length = rest.len();
                if let Some(out) = self.buffer.get_mut(..length) {
                    out.copy_from_slice(rest);
                }
                Action::Send(self.buffer.get(..length).unwrap_or_default())
            }
            State::TcpSending(sender) => {
                self.meter.start(now_us);
                match sender.poll(now_us, self.buffer) {
                    Next::Send(length) => {
                        Action::Send(self.buffer.get(..length).unwrap_or_default())
                    }
                    Next::Wait(until) => wait(Some(until)),
                    Next::Finished => {
                        let report = self.meter.last(now_us, sender.sent_bytes(), None, None);
                        self.state = State::TcpClosing {
                            report,
                            close: true,
                        };
                        self.poll(now_us)
                    }
                }
            }
            State::TcpClosing { close, .. } => {
                if *close {
                    *close = false;
                    return Action::Close;
                }
                wait(None)
            }
            State::UdpReporting {
                server,
                report,
                sends,
                again,
                last_us,
            } => {
                if *sends == 0 || *again {
                    let length = server.encode(self.buffer).unwrap_or_default();
                    return Action::Send(self.buffer.get(..length).unwrap_or_default());
                }
                let until = last_us.saturating_add(REPORT_WAIT_US);
                if now_us >= until {
                    let report = *report;
                    self.finish(report);
                    return self.poll(now_us);
                }
                Action::Wait {
                    until_us: Some(until),
                }
            }
            State::UdpSending(sender) => {
                self.meter.start(now_us);
                match sender.poll(now_us, self.buffer) {
                    Next::Send(length) => {
                        Action::Send(self.buffer.get(..length).unwrap_or_default())
                    }
                    Next::Wait(until) => wait(Some(until)),
                    Next::Finished => {
                        let report = self.meter.last(now_us, sender.sent_bytes(), None, None);
                        let State::UdpSending(sender) =
                            core::mem::replace(&mut self.state, State::Over)
                        else {
                            return Action::Done(Err(TestError::Protocol));
                        };
                        self.state = State::UdpEnding { sender, report };
                        self.poll(now_us)
                    }
                }
            }
            State::UdpEnding { sender, .. } => {
                let length = sender.end(0, now_us, self.buffer);
                Action::Send(self.buffer.get(..length).unwrap_or_default())
            }
            State::Over => Action::Done(Ok(())),
        }
    }

    /// Records that the caller wrote `count` bytes of the last [`Action::Send`].
    pub fn sent(&mut self, count: usize) {
        match &mut self.state {
            State::TcpAck {
                sent,
                reverse,
                bytes,
            } => {
                *sent = sent.saturating_add(count);
                if *sent >= HEADER_ACK_LEN {
                    let bytes = *bytes;
                    self.state = match reverse.take() {
                        Some(sender) => State::TcpSending(sender),
                        None => State::TcpReceiving { bytes },
                    };
                }
            }
            State::TcpSending(sender) => sender.sent(count),
            State::UdpSending(sender) => sender.sent(count),
            State::UdpReporting {
                sends,
                again,
                last_us,
                ..
            } => {
                *sends = sends.saturating_add(1);
                *again = false;
                *last_us = self.now_us;
            }
            State::UdpEnding { report, .. } => {
                // A stock sender counts the end of the test in its bytes, as the receiver counts it.
                let report = Report {
                    bytes: report.bytes.saturating_add(count as u64),
                    ..*report
                };
                self.finish(report);
            }
            _ => {}
        }
    }

    /// Passes in bytes from the client: a TCP read, or one UDP datagram.
    pub fn received(&mut self, now_us: u64, bytes: &[u8]) {
        if self.pending.is_none() {
            self.pending = self.interval(now_us);
        }
        match &mut self.state {
            State::TcpHeader { stash, len } => {
                self.meter.start(now_us);
                let take = bytes.len().min(MAX_HEADER_LEN.saturating_sub(*len));
                if let (Some(to), Some(from)) = (
                    stash.get_mut(*len..len.saturating_add(take)),
                    bytes.get(..take),
                ) {
                    to.copy_from_slice(from);
                }
                *len = len.saturating_add(take);
                let rest = bytes.get(take..).unwrap_or_default().len() as u64;
                let stash = *stash;
                let len = *len;
                self.read_header(&stash, len, rest);
            }
            State::TcpReceiving { bytes: count }
            | State::TcpAck {
                bytes: count,
                reverse: None,
                ..
            } => {
                *count = count.saturating_add(bytes.len() as u64);
            }
            State::UdpWaiting => self.first_datagram(now_us, bytes),
            State::UdpReceiving {
                counts,
                wide,
                last_us,
            } => {
                let Ok(prefix) = Prefix::parse(bytes) else {
                    return;
                };
                let id = if *wide {
                    prefix.packet_id
                } else {
                    i64::from(prefix.packet_id_32())
                };
                *last_us = now_us;
                counts.datagram(id.saturating_abs(), prefix.sent, now_us, bytes.len());
                if id < 0 {
                    self.report(now_us);
                }
            }
            State::UdpReporting {
                sends,
                last_us,
                again,
                ..
            } if *sends < REPORT_SENDS && now_us < last_us.saturating_add(REPORT_WAIT_US) => {
                *again = true;
            }
            _ => {}
        }
    }

    /// Records that the client closed the TCP connection.
    pub fn closed(&mut self, now_us: u64) {
        if let State::TcpHeader { .. } = self.state {
            self.outcome = Some(Err(TestError::Protocol));
            self.state = State::Over;
            return;
        }
        // A client may close as soon as it has sent or received its bytes, before the ack or the last chunk is
        // out, so the test ends with what moved (P-10, P-16).
        self.end_tcp(now_us);
    }

    /// Ends a TCP test at `now_us` with the bytes it moved. Returns whether the state was a TCP test's.
    fn end_tcp(&mut self, now_us: u64) -> bool {
        let report = match &self.state {
            State::TcpClosing { report, .. } => *report,
            State::TcpHeader { len, .. } => self.meter.last(now_us, *len as u64, None, None),
            State::TcpAck {
                reverse: Some(_), ..
            } => self.meter.last(now_us, 0, None, None),
            State::TcpAck { bytes, .. } | State::TcpReceiving { bytes } => {
                self.meter.last(now_us, *bytes, None, None)
            }
            State::TcpSending(sender) => self.meter.last(now_us, sender.sent_bytes(), None, None),
            _ => return false,
        };
        self.finish(report);
        true
    }

    /// Reads the TCP test header from the first `len` bytes of `stash`, once they hold it. `rest` bytes arrived
    /// after the stash filled.
    fn read_header(&mut self, stash: &[u8; MAX_HEADER_LEN], len: usize, rest: u64) {
        let Some(header_flags) = word(stash, 0).filter(|_| len >= 4) else {
            return;
        };
        let header_len = match header_length(header_flags, false) {
            Err(_) => return self.refuse(),
            Ok(None) => {
                // The flags word of a stream without a header is not counted (P-8).
                let bytes = (len as u64).saturating_sub(4).saturating_add(rest);
                self.state = State::TcpReceiving { bytes };
                return;
            }
            Ok(Some(header_len)) => header_len,
        };
        if len < header_len {
            return;
        }
        let Ok(header) = TestHeader::parse(stash) else {
            return self.refuse();
        };
        if refused(&header) {
            return self.refuse();
        }
        let bytes = (len as u64).saturating_add(rest);
        let reverse = header.is_reverse().then(|| {
            let burst = match header.buf_len {
                0 | TOO_LONG_BUF_LEN.. => DEFAULT_BURST,
                len => len,
            };
            TcpSender::new(
                Content::Bursts(burst.max(BURST_HEADER_LEN as u32)),
                self.stop(header.amount),
                header.rate,
            )
        });
        // P-9 acks EXTEND without VERSION2. A header that P-13 lets through has EXTEND or VERSION2, so EXTEND
        // does not need a check of its own.
        let acked = header_flags & flags::VERSION2 == 0 || header_flags & flags::V2PEERDETECT != 0;
        self.state = match (acked, reverse) {
            (true, reverse) => State::TcpAck {
                sent: 0,
                reverse,
                bytes,
            },
            (false, Some(sender)) => State::TcpSending(sender),
            (false, None) => State::TcpReceiving { bytes },
        };
    }

    /// Starts a UDP test at its first datagram, or ignores the datagram (P-11, P-13).
    fn first_datagram(&mut self, now_us: u64, datagram: &[u8]) {
        if datagram.len() < MIN_FIRST_DATAGRAM {
            return;
        }
        let Some(header_flags) = word(datagram, PREFIX_LEN) else {
            return;
        };
        let Ok(prefix) = Prefix::parse(datagram) else {
            return;
        };
        let wide = header_flags & flags::SEQNO64B != 0;
        let id = if wide {
            prefix.packet_id
        } else {
            i64::from(prefix.packet_id_32())
        };
        if id <= 0 {
            return;
        }
        let header = match header_length(header_flags, true) {
            Err(_) => return self.refuse(),
            Ok(None) => None,
            Ok(Some(_)) => {
                match TestHeader::parse(datagram.get(PREFIX_LEN..).unwrap_or_default()) {
                    Ok(header) if !refused(&header) => Some(header),
                    _ => return self.refuse(),
                }
            }
        };
        self.meter.start(now_us);
        if let Some(header) = header.filter(TestHeader::is_reverse) {
            let longest = if self.config.ipv6 {
                MAX_DATAGRAM_IPV6
            } else {
                MAX_DATAGRAM_IPV4
            };
            let len = match header.buf_len as usize {
                0 => longest,
                len => len.min(longest).max(PREFIX_LEN),
            };
            self.state = State::UdpSending(UdpSender::new(
                len,
                None,
                self.stop(header.amount),
                header.rate,
            ));
            return;
        }
        let mut counts = UdpReceiver::default();
        counts.datagram(id, prefix.sent, now_us, datagram.len());
        self.state = State::UdpReceiving {
            counts,
            wide,
            last_us: now_us,
        };
    }

    /// Ends a forward UDP test at `now_us` and starts to send its report.
    fn report(&mut self, now_us: u64) {
        let State::UdpReceiving {
            counts, last_us, ..
        } = &mut self.state
        else {
            return;
        };
        let total = counts.total();
        let report = self.meter.last(*last_us, counts.bytes(), Some(total), None);
        self.tail = self
            .meter
            .tail(report.end_us, report.bytes, Some(counts.interval().1));
        self.ended_udp = true;
        let server = ServerReport {
            bytes: counts.bytes(),
            duration: timestamp(report.end_us),
            lost: total.lost,
            out_of_order: total.out_of_order,
            datagrams: total.datagrams,
            jitter: timestamp(total.jitter_ns / 1_000),
        };
        self.state = State::UdpReporting {
            server,
            report,
            sends: 0,
            again: false,
            last_us: now_us,
        };
    }

    /// When the caller's duration ends the test (P-15).
    fn deadline(&self) -> Option<u64> {
        let duration = self.config.duration_us?;
        match self.state {
            // These end on their own, within 250 ms, or at once.
            State::UdpWaiting
            | State::UdpReporting { .. }
            | State::UdpEnding { .. }
            | State::Over => None,
            _ => Some(self.meter.start_us()?.saturating_add(duration)),
        }
    }

    /// Ends the test when the caller's duration has passed. Returns whether it did.
    fn over_time(&mut self, now_us: u64) -> bool {
        if self.deadline().is_none_or(|deadline| now_us < deadline) {
            return false;
        }
        if self.end_tcp(now_us) {
            return true;
        }
        match core::mem::replace(&mut self.state, State::Over) {
            State::UdpSending(sender) => {
                let report = self.meter.last(now_us, sender.sent_bytes(), None, None);
                self.state = State::UdpEnding { sender, report };
            }
            receiving => {
                self.state = receiving;
                self.report(now_us);
            }
        }
        true
    }

    /// The interval report due at `now_us`: of the data the test moves, or an empty one left after it ended.
    fn interval(&mut self, now_us: u64) -> Option<Report> {
        let due = self.meter.next_due()?;
        if now_us < due {
            return None;
        }
        match &mut self.state {
            State::TcpReceiving { bytes } => self.meter.interval(now_us, *bytes, None),
            State::TcpSending(sender) => self.meter.interval(now_us, sender.sent_bytes(), None),
            State::UdpSending(sender) => self.meter.interval(now_us, sender.sent_bytes(), None),
            State::UdpReceiving { counts, .. } => {
                let bytes = counts.bytes();
                self.meter.interval(now_us, bytes, Some(counts))
            }
            _ => self.meter.empty_interval(self.ended_udp),
        }
    }

    /// When a reverse test of `amount` stops, cut to the caller's duration (P-15).
    fn stop(&self, amount: Amount) -> Stop {
        match amount {
            Amount::Time { centiseconds } => {
                let us = u64::from(centiseconds).saturating_mul(SECOND_US / 100);
                Stop::After(
                    self.config
                        .duration_us
                        .map_or(us, |duration| us.min(duration)),
                )
            }
            Amount::Bytes(count) => Stop::Bytes(u64::from(count)),
        }
    }

    fn refuse(&mut self) {
        self.outcome = Some(Err(TestError::Refused));
        self.state = State::Over;
    }

    /// Ends the test with `report` as its final report, after the last part of an interval. A forward UDP test
    /// found its last part when it stopped receiving.
    fn finish(&mut self, report: Report) {
        if !self.ended_udp {
            self.tail = self.meter.tail(report.end_us, report.bytes, None);
        }
        self.last = Some(report);
        self.outcome = Some(Ok(()));
        self.state = State::Over;
    }
}

/// The big-endian word at `at` in `bytes`, when `bytes` holds it.
fn word(bytes: &[u8], at: usize) -> Option<u32> {
    let word = bytes.get(at..at.saturating_add(4))?;
    word.try_into().ok().map(u32::from_be_bytes)
}

/// Whether a test header asks for a mode outside v1 (P-13).
fn refused(header: &TestHeader) -> bool {
    let only_version1 = header.flags & flags::VERSION1 != 0 && header.flags & flags::VERSION2 == 0;
    only_version1
        || header.flags & (flags::KEYCHECK | flags::BOUNCEBACK) != 0
        || header.upper_flags & !flags::REVERSE != 0
        || header.lower_flags != 0
        || header.num_threads != 1
}
