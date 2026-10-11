//! The client: it starts a test, and sends or, in a reverse test, receives (P-1 to P-7, P-22, P-23).

use crate::config::{Action, Config, ConfigError, Length, TestError, Transport};
use crate::counts::UdpReceiver;
use crate::datagram::UdpSender;
use crate::meter::{Meter, SECOND_US};
use crate::report::Report;
use crate::stream::{BurstReader, Content, Next, Stop, TcpSender};
use crate::wire::{Amount, PREFIX_LEN, Prefix, ServerReport, TEST_HEADER_LEN, TestHeader, flags};

/// The longest a TCP client waits for the server to close after it shuts down its write side, unless the test
/// lasted longer (P-1).
const CLOSE_WAIT_US: u64 = 2 * SECOND_US;
/// The time a reverse test's receiver waits after the test's duration (P-22).
const GRACE_US: u64 = 2 * SECOND_US;
/// The wait for a server report between two ends of a UDP test, and the most ends sent (P-3).
const END_WAIT_US: u64 = 10_000;
const END_SENDS: u32 = 200;
/// How often a reverse UDP client asks again, and how long a reverse test waits for data. P-6 waits for the first
/// datagram. P-5 and P-22 wait for the next bytes in byte mode.
const ASK_EVERY_US: u64 = SECOND_US;
const NO_DATA_US: u64 = 10 * SECOND_US;
/// The shortest datagram that starts a test at a server, and so at a reverse test's client (P-11, P-22).
const MIN_FIRST_DATAGRAM: usize = 40;
/// The version a 2.2.2 client writes.
const VERSION_U: u32 = 0x0002_0002;
const VERSION_L: u32 = 0x0002_0003;

/// The client of one test. The caller passes the time in as `now_us`, and moves the bytes that `poll` asks for.
#[derive(Debug)]
pub struct Client<'b> {
    config: Config,
    buffer: &'b mut [u8],
    meter: Meter,
    state: State,
    /// The time of the last `poll`, for the calls that do not take one.
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
    /// A forward TCP test sends.
    TcpSending(TcpSender),
    /// A forward TCP test has sent, and waits for the server to close.
    TcpClosing {
        report: Report,
        close: bool,
        until_us: u64,
    },
    /// A reverse TCP test sends its header.
    TcpAsking(TcpSender),
    /// A reverse TCP test receives, until the server closes, the byte count or the deadline.
    TcpReceiving {
        reader: BurstReader,
        until_us: Option<u64>,
    },
    /// A forward UDP test sends.
    UdpSending(UdpSender),
    /// A forward UDP test sends the end of the test and waits for the server report.
    UdpEnding {
        sender: UdpSender,
        report: Report,
        sends: u32,
        next_us: u64,
    },
    /// A reverse UDP test asks for data.
    UdpAsking {
        sender: UdpSender,
        first_us: Option<u64>,
        next_us: u64,
    },
    /// A reverse UDP test receives.
    UdpReceiving {
        counts: UdpReceiver,
        until_us: Option<u64>,
        last_us: u64,
    },
    Over,
}

impl<'b> Client<'b> {
    /// A client of the test `config`, which builds each chunk or datagram in `buffer`.
    ///
    /// Returns `BufLenTooLong` when a UDP datagram of `buf_len` bytes does not fit in `buffer`, and
    /// `BufLenTooShort` when it cannot hold the prefix and the test header, or `buffer` is empty.
    pub fn new(config: Config, buffer: &'b mut [u8]) -> Result<Self, ConfigError> {
        let len = config.buf_len as usize;
        if buffer.is_empty() {
            return Err(ConfigError::BufLenTooShort);
        }
        if config.transport == Transport::Udp {
            if len > buffer.len() {
                return Err(ConfigError::BufLenTooLong);
            }
            if len < PREFIX_LEN + TEST_HEADER_LEN {
                return Err(ConfigError::BufLenTooShort);
            }
        }
        let header = test_header(&config);
        let state = match (config.transport, config.reverse) {
            (Transport::Tcp, false) => State::TcpSending(TcpSender::new(
                Content::Header(encode(&header)),
                stop(config.length),
                config.rate,
            )),
            (Transport::Tcp, true) => State::TcpAsking(TcpSender::new(
                Content::Header(encode(&header)),
                Stop::Bytes(TEST_HEADER_LEN as u64),
                0,
            )),
            (Transport::Udp, false) => State::UdpSending(UdpSender::new(
                len,
                Some(header),
                stop(config.length),
                config.rate,
            )),
            (Transport::Udp, true) => State::UdpAsking {
                sender: UdpSender::new(len, Some(header), Stop::Bytes(0), 0),
                first_us: None,
                next_us: 0,
            },
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

    /// The settings this client was made with.
    pub fn config(&self) -> Config {
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
        let next_due = self.meter.next_due();
        let wait = |until: Option<u64>| Action::Wait {
            until_us: match (until, next_due) {
                (Some(until), Some(due)) => Some(until.min(due)),
                (until, due) => until.or(due),
            },
        };
        match &mut self.state {
            State::TcpSending(sender) => {
                self.meter.start(now_us);
                match sender.poll(now_us, self.buffer) {
                    Next::Send(length) => {
                        Action::Send(self.buffer.get(..length).unwrap_or_default())
                    }
                    Next::Wait(until) => wait(Some(until)),
                    Next::Finished => {
                        let report = self.meter.last(now_us, sender.sent_bytes(), None, None);
                        let duration = match self.config.length {
                            Length::Time { us } => us,
                            Length::Bytes(_) => 0,
                        };
                        self.state = State::TcpClosing {
                            report,
                            close: true,
                            until_us: now_us.saturating_add(duration.max(CLOSE_WAIT_US)),
                        };
                        self.poll(now_us)
                    }
                }
            }
            State::TcpClosing {
                report,
                close,
                until_us,
            } => {
                if *close {
                    *close = false;
                    return Action::Close;
                }
                if now_us >= *until_us {
                    let report = *report;
                    self.finish(report);
                    return self.poll(now_us);
                }
                Action::Wait {
                    until_us: Some(*until_us),
                }
            }
            State::TcpAsking(sender) => match sender.poll(now_us, self.buffer) {
                Next::Send(length) => Action::Send(self.buffer.get(..length).unwrap_or_default()),
                Next::Wait(until) => wait(Some(until)),
                Next::Finished => {
                    self.receive_bursts();
                    self.poll(now_us)
                }
            },
            State::TcpReceiving { reader, until_us } => match *until_us {
                Some(until) if now_us >= until => {
                    let report = self.meter.last(now_us, reader.bytes(), None, None);
                    self.finish(report);
                    self.poll(now_us)
                }
                until => wait(until),
            },
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
                        self.state = State::UdpEnding {
                            sender,
                            report,
                            sends: 0,
                            next_us: now_us,
                        };
                        self.poll(now_us)
                    }
                }
            }
            State::UdpEnding {
                sender,
                report,
                sends,
                next_us,
            } => {
                if now_us < *next_us {
                    return Action::Wait {
                        until_us: Some(*next_us),
                    };
                }
                if *sends >= END_SENDS {
                    let report = *report;
                    self.finish(report);
                    return self.poll(now_us);
                }
                let length = sender.end(*sends, now_us, self.buffer);
                Action::Send(self.buffer.get(..length).unwrap_or_default())
            }
            State::UdpAsking {
                sender,
                first_us,
                next_us,
            } => {
                let first = *first_us.get_or_insert(now_us);
                let give_up = first.saturating_add(NO_DATA_US);
                if now_us >= give_up {
                    self.outcome = Some(Err(TestError::NoData));
                    self.state = State::Over;
                    return self.poll(now_us);
                }
                if now_us < *next_us {
                    return Action::Wait {
                        until_us: Some((*next_us).min(give_up)),
                    };
                }
                let length = sender.ask(now_us, self.buffer);
                Action::Send(self.buffer.get(..length).unwrap_or_default())
            }
            State::UdpReceiving {
                counts,
                until_us,
                last_us,
            } => match *until_us {
                Some(until) if now_us >= until => {
                    let report =
                        self.meter
                            .last(*last_us, counts.bytes(), Some(counts.total()), None);
                    self.finish(report);
                    self.poll(now_us)
                }
                until => wait(until),
            },
            State::Over => Action::Done(Ok(())),
        }
    }

    /// Records that the caller wrote `count` bytes of the last [`Action::Send`].
    pub fn sent(&mut self, count: usize) {
        match &mut self.state {
            State::TcpSending(sender) => sender.sent(count),
            State::TcpAsking(sender) => {
                sender.sent(count);
                // The server answers the last byte of the header with data, which may arrive before the next poll.
                if sender.sent_bytes() >= TEST_HEADER_LEN as u64 {
                    self.receive_bursts();
                }
            }
            State::UdpSending(sender) => sender.sent(count),
            State::UdpEnding { sends, next_us, .. } => {
                *sends = sends.saturating_add(1);
                *next_us = self.now_us.saturating_add(END_WAIT_US);
            }
            State::UdpAsking { next_us, .. } => *next_us = self.now_us.saturating_add(ASK_EVERY_US),
            _ => {}
        }
    }

    /// Passes in bytes from the server: a TCP read, or one UDP datagram.
    pub fn received(&mut self, now_us: u64, bytes: &[u8]) {
        if self.pending.is_none() {
            self.pending = self.interval(now_us);
        }
        match &mut self.state {
            State::TcpReceiving { reader, until_us } => {
                self.meter.start(now_us);
                if let Err(error) = reader.received(bytes) {
                    self.outcome = Some(Err(error));
                    self.state = State::Over;
                    return;
                }
                if let Length::Bytes(total) = self.config.length {
                    // A lost close must not hold a test in byte mode for ever (P-5).
                    *until_us = Some(now_us.saturating_add(NO_DATA_US));
                    if reader.bytes() >= total {
                        let report = self.meter.last(now_us, reader.bytes(), None, None);
                        self.finish(report);
                    }
                }
            }
            State::UdpEnding { report, .. } => {
                if let Ok(server) = ServerReport::parse(bytes) {
                    let report = Report {
                        server: Some(server),
                        ..*report
                    };
                    self.finish(report);
                }
            }
            State::UdpAsking { .. } => {
                // The first datagram must be one a server would start a test at (P-22, P-11).
                let starts = bytes.len() >= MIN_FIRST_DATAGRAM
                    && Prefix::parse(bytes).is_ok_and(|prefix| prefix.packet_id_32() > 0);
                if !starts {
                    return;
                }
                let until_us = match self.config.length {
                    Length::Time { us } => Some(now_us.saturating_add(us).saturating_add(GRACE_US)),
                    Length::Bytes(_) => None,
                };
                self.state = State::UdpReceiving {
                    counts: UdpReceiver::default(),
                    until_us,
                    last_us: now_us,
                };
                self.received(now_us, bytes);
            }
            State::UdpReceiving {
                counts,
                last_us,
                until_us,
            } => {
                let Ok(prefix) = Prefix::parse(bytes) else {
                    return;
                };
                self.meter.start(now_us);
                *last_us = now_us;
                if let Length::Bytes(_) = self.config.length {
                    // A lost end of the test must not hold a test in byte mode for ever (P-22).
                    *until_us = Some(now_us.saturating_add(NO_DATA_US));
                }
                let id = i64::from(prefix.packet_id_32());
                counts.datagram(id.saturating_abs(), prefix.sent, now_us, bytes.len());
                if id < 0 {
                    let report =
                        self.meter
                            .last(now_us, counts.bytes(), Some(counts.total()), None);
                    self.finish(report);
                }
            }
            _ => {}
        }
    }

    /// Records that the server closed the TCP connection.
    pub fn closed(&mut self, now_us: u64) {
        match &self.state {
            State::TcpClosing { report, .. } => {
                let report = *report;
                self.finish(report);
            }
            State::TcpReceiving { reader, .. } => {
                let report = self.meter.last(now_us, reader.bytes(), None, None);
                self.finish(report);
            }
            // The server ended the test, at its own duration or because it refused it (P-24).
            State::TcpSending(sender) => {
                let report = self.meter.last(now_us, sender.sent_bytes(), None, None);
                self.finish(report);
            }
            State::TcpAsking(_) => {
                self.outcome = Some(Err(TestError::Refused));
                self.state = State::Over;
            }
            _ => {}
        }
    }

    /// The interval report due at `now_us`: of the data the test moves, or an empty one left after it ended.
    fn interval(&mut self, now_us: u64) -> Option<Report> {
        let due = self.meter.next_due()?;
        if now_us < due {
            return None;
        }
        match &mut self.state {
            State::TcpSending(sender) => self.meter.interval(now_us, sender.sent_bytes(), None),
            State::UdpSending(sender) => self.meter.interval(now_us, sender.sent_bytes(), None),
            State::TcpReceiving { reader, .. } => self.meter.interval(now_us, reader.bytes(), None),
            State::UdpReceiving { counts, .. } => {
                let bytes = counts.bytes();
                self.meter.interval(now_us, bytes, Some(counts))
            }
            _ => self.meter.empty_interval(self.ended_udp),
        }
    }

    /// Starts to read the bursts of a reverse TCP test, once the header is out.
    fn receive_bursts(&mut self) {
        let until_us = match self.config.length {
            Length::Time { us } => Some(self.now_us.saturating_add(us).saturating_add(GRACE_US)),
            Length::Bytes(_) => None,
        };
        self.state = State::TcpReceiving {
            reader: BurstReader::new(),
            until_us,
        };
    }

    /// Ends the test with `report` as its final report, after the last part of an interval.
    fn finish(&mut self, report: Report) {
        let udp = match &mut self.state {
            State::UdpReceiving { counts, .. } => Some(counts.interval().1),
            _ => None,
        };
        self.ended_udp = udp.is_some();
        self.tail = self.meter.tail(report.end_us, report.bytes, udp);
        self.last = Some(report);
        self.outcome = Some(Ok(()));
        self.state = State::Over;
    }
}

/// When a sender of `length` stops.
fn stop(length: Length) -> Stop {
    match length {
        Length::Time { us } => Stop::After(us),
        Length::Bytes(count) => Stop::Bytes(count),
    }
}

/// The test header of W-4 and W-5 for `config`.
fn test_header(config: &Config) -> TestHeader {
    let (base, length) = match config.transport {
        Transport::Tcp => (flags::EXTEND, TEST_HEADER_LEN),
        Transport::Udp => (
            flags::EXTEND | flags::SEQNO64B,
            PREFIX_LEN + TEST_HEADER_LEN,
        ),
    };
    let reverse = match (config.transport, config.reverse) {
        (_, false) => 0,
        (Transport::Tcp, true) => flags::VERSION2,
        // A stock client adds VERSION1 to a reverse UDP test when it sets -l, which ours always does (W-5).
        (Transport::Udp, true) => flags::VERSION2 | flags::UDPTESTS | flags::VERSION1,
    };
    TestHeader {
        flags: base | reverse | flags::LEN_BIT | ((length as u32) << 1),
        num_threads: 1,
        port: u32::from(config.port),
        buf_len: config.buf_len,
        amount: match config.length {
            Length::Time { us } => Amount::Time {
                centiseconds: u32::try_from(us.div_ceil(10_000)).unwrap_or(u32::MAX),
            },
            Length::Bytes(count) => Amount::Bytes(u32::try_from(count).unwrap_or(u32::MAX)),
        },
        upper_flags: if config.reverse { flags::REVERSE } else { 0 },
        lower_flags: 0,
        version_u: VERSION_U,
        version_l: VERSION_L,
        rate: config.rate,
    }
}

/// The 64 bytes of `header`.
fn encode(header: &TestHeader) -> [u8; TEST_HEADER_LEN] {
    let mut bytes = [0; TEST_HEADER_LEN];
    let _ = header.encode(&mut bytes);
    bytes
}
