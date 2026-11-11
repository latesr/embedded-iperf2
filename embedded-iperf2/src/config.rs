//! What a caller asks for, and what the state machines answer with.

use crate::report::Report;

/// The transport of a test.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Transport {
    /// TCP, a stream.
    Tcp,
    /// UDP, datagrams with a prefix (W-1).
    Udp,
}

/// The length of a test: a duration, `-t`, or a byte count, `-n`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Length {
    /// A duration in microseconds. The wire holds it in units of 10 ms, so the server of a reverse test sees it
    /// rounded up to the next 10 ms.
    Time {
        /// The duration in microseconds.
        us: u64,
    },
    /// A byte count. The wire holds 31 bits, so the server of a reverse test sees it masked (W-6).
    Bytes(u64),
}

/// A client's test: what a stock client takes as `-u`, `-R`, `-t` or `-n`, `-l`, `-b`, `-i`, and `-p`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Config {
    /// The transport, `-u` for UDP.
    pub transport: Transport,
    /// Whether the server sends, `-R`.
    pub reverse: bool,
    /// The length of the test.
    pub length: Length,
    /// The length of a UDP datagram, or of a TCP burst in a reverse test, `-l`.
    pub buf_len: u32,
    /// The rate in bits per second, `-b`. 0 means as fast as the socket takes the bytes.
    pub rate: u64,
    /// The interval of the reports, `-i`, in microseconds. `None` means a final report only.
    pub interval_us: Option<u64>,
    /// The server's port, `-p`, which the test header holds.
    pub port: u16,
}

impl Config {
    /// Creates a new TCP test with the defaults of P-7 and W-8: 10 s, bursts of 131072 bytes, no rate limit, and
    /// port 5001.
    pub const fn tcp() -> Self {
        Self {
            transport: Transport::Tcp,
            reverse: false,
            length: Length::Time { us: 10_000_000 },
            buf_len: 131_072,
            rate: 0,
            interval_us: None,
            port: 5001,
        }
    }

    /// Creates a new UDP test with the defaults of P-7 and W-8: 10 s, datagrams of 1470 bytes, 1048576 bit/s, and
    /// port 5001.
    pub const fn udp() -> Self {
        Self {
            transport: Transport::Udp,
            buf_len: 1470,
            rate: 1_048_576,
            ..Self::tcp()
        }
    }
}

/// A server's settings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ServerConfig {
    /// The transport the server listens on.
    pub transport: Transport,
    /// The longest time that a test can last, in microseconds, whether or not its end arrives (P-15). `None` means
    /// no limit.
    pub duration_us: Option<u64>,
    /// The interval of the reports, in microseconds. `None` means a final report only.
    pub interval_us: Option<u64>,
    /// Whether the server runs over IPv6, which cuts a reverse UDP test's datagrams to 1450 bytes rather than
    /// 1470 (P-21).
    pub ipv6: bool,
}

/// Why `Client::new` or `Server::new` refused its settings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConfigError {
    /// A UDP datagram of `buf_len` bytes does not fit in the buffer.
    BufLenTooLong,
    /// A UDP datagram of `buf_len` bytes cannot hold the 80-byte header, or a buffer cannot hold a datagram or a
    /// burst header.
    BufLenTooShort,
}

/// Why a test ended without a final report.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TestError {
    /// The server refused the test, because it asks for a mode outside v1 (P-13). A close before a reverse test's
    /// header is out is a refusal too, as P-24 states.
    Refused,
    /// The client of a reverse UDP test received no data within 10 s (P-6).
    NoData,
    /// The peer sent bytes that break the protocol, such as a burst header with a `burst_size` below 120 (P-5).
    Protocol,
}

/// What the caller does next, from `poll`.
#[derive(Debug, PartialEq, Eq)]
pub enum Action<'a> {
    /// Write these bytes: one TCP chunk, or one UDP datagram. Then call `sent` with the count written. A TCP
    /// write can take fewer bytes, and the rest comes back in the next `Send`. A UDP datagram that could not be
    /// sent is not reported, and the next `poll` builds it again.
    Send(&'a [u8]),
    /// Wait for bytes from the peer until `until_us`, then poll again. `None` means wait for bytes or a close.
    Wait {
        /// The time to poll again by, on the caller's clock.
        until_us: Option<u64>,
    },
    /// An interval report, or the final report of the test.
    Report(Report),
    /// Shut down the write side of the TCP connection, and keep reading.
    Close,
    /// The test is over. The caller closes the socket.
    Done(Result<(), TestError>),
}
