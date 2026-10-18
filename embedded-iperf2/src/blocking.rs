//! The driver for a host: it runs a [`Client`] or a [`Server`] over the standard library's blocking sockets (D-2).
//!
//! The caller makes the client or the server, and the driver reads and writes the socket for it until the test
//! ends. The time is the microseconds since the call began, from [`Instant`] (D-5). Each report goes to
//! `on_report` as the core returns it, so a tool can print each interval while the test runs.
//!
//! Each turn of the loop first reads what has arrived without blocking, then polls. A peer's bytes or close can
//! then reach the core before it sends again, in any of its states. A wait reads with a timeout of at most
//! 100 ms. Before each read, the TCP server closes one connection that arrived while the test runs (P-14).
//!
//! A write that the peer refuses with a reset or a broken pipe reaches the core as a close (P-24, P-16).

use crate::machine::Machine;
use crate::{Action, Client, Report, Server, TestError, Transport};
use std::io::{self, ErrorKind, Read, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::time::{Duration, Instant};
use std::vec;

/// The longest a wait reads before the loop turns again.
const LONGEST_WAIT: Duration = Duration::from_millis(100);
/// The longest a client waits for a TCP connection. Without a limit, an address that does not answer holds the
/// caller for the kernel's own timeout, about 75 s on macOS.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
/// The length of the driver's receive buffer, which holds the longest UDP datagram.
const RECEIVE_LEN: usize = 65_536;
/// How many sends of one datagram in a row may fail, a millisecond apart, before the driver gives up. A host
/// refuses a UDP send for a moment when its socket buffer is full.
const UDP_SEND_TRIES: u32 = 1_000;

/// Runs the test of `client` against the server at `server`, and returns how it ended.
///
/// The driver connects over TCP or UDP, as the client's [`Config`](crate::Config) states, from an address of the
/// same family as `server`. `on_report` receives each interval and the final report.
///
/// Returns an error when a socket fails, or when the server does not accept a TCP connection within 5 s.
pub fn run_client(
    client: &mut Client<'_>,
    server: SocketAddr,
    on_report: impl FnMut(&Report),
) -> io::Result<Result<(), TestError>> {
    match client.config().transport {
        Transport::Tcp => {
            let stream = TcpStream::connect_timeout(&server, CONNECT_TIMEOUT)?;
            let mut link = TcpLink::new(stream, None)?;
            run(client, &mut link, on_report)
        }
        Transport::Udp => {
            let local = match server {
                SocketAddr::V4(_) => SocketAddr::from(([0, 0, 0, 0], 0)),
                SocketAddr::V6(_) => SocketAddr::from(([0_u16; 8], 0)),
            };
            let socket = UdpSocket::bind(local)?;
            let mut link = UdpLink {
                socket: &socket,
                peer: Some(server),
                refused: None,
                kept: false,
            };
            run(client, &mut link, on_report)
        }
    }
}

/// A TCP server that runs one test at a time on a listener.
#[derive(Debug)]
pub struct TcpServer {
    listener: TcpListener,
}

impl TcpServer {
    /// A server on `listener`.
    pub fn new(listener: TcpListener) -> Self {
        Self { listener }
    }

    /// Waits for a connection and runs the test of `server` on it, and returns how the test ended. While the
    /// test runs, the driver closes each other connection as it arrives (P-14).
    ///
    /// Returns an error of kind `InvalidInput` when `server` is not a TCP server, or an error when a socket
    /// fails.
    pub fn run_one(
        &mut self,
        server: &mut Server<'_>,
        on_report: impl FnMut(&Report),
    ) -> io::Result<Result<(), TestError>> {
        if server.config().transport != Transport::Tcp {
            return Err(io::Error::new(
                ErrorKind::InvalidInput,
                "a TcpServer runs a TCP server",
            ));
        }
        self.listener.set_nonblocking(false)?;
        let (stream, _) = self.listener.accept()?;
        self.listener.set_nonblocking(true)?;
        let mut link = TcpLink::new(stream, Some(&self.listener))?;
        run(server, &mut link, on_report)
    }
}

/// A UDP server that runs one test at a time on a socket.
///
/// A test belongs to the address and port of the datagram that started it, and the server ignores other
/// datagrams until the test ends. Until a test starts, it takes datagrams from any address, so a late repeat of an
/// old test's end does not take the server. Another address's datagram during the wait after the report ends the
/// test and stays in the socket as the next test's first. After a refused test, the server drops the later
/// datagrams of that address and port, until another address and port is refused (D-6).
#[derive(Debug)]
pub struct UdpServer {
    socket: UdpSocket,
    refused: Option<SocketAddr>,
}

impl UdpServer {
    /// A server on `socket`.
    pub fn new(socket: UdpSocket) -> Self {
        Self {
            socket,
            refused: None,
        }
    }

    /// Waits for the first datagram of a test and runs the test of `server`, and returns how the test ended.
    ///
    /// Returns an error of kind `InvalidInput` when `server` is not a UDP server, or when its `ipv6` setting does
    /// not match the family of the socket's address. Returns an error when the socket fails.
    pub fn run_one(
        &mut self,
        server: &mut Server<'_>,
        on_report: impl FnMut(&Report),
    ) -> io::Result<Result<(), TestError>> {
        let config = server.config();
        if config.transport != Transport::Udp {
            return Err(io::Error::new(
                ErrorKind::InvalidInput,
                "a UdpServer runs a UDP server",
            ));
        }
        if config.ipv6 != self.socket.local_addr()?.is_ipv6() {
            return Err(io::Error::new(
                ErrorKind::InvalidInput,
                "the server's ipv6 setting does not match the socket's address",
            ));
        }
        let mut link = UdpLink {
            socket: &self.socket,
            peer: None,
            refused: self.refused,
            kept: false,
        };
        let outcome = run(server, &mut link, on_report)?;
        if outcome == Err(TestError::Refused) {
            self.refused = link.peer;
        }
        Ok(outcome)
    }
}

/// What one read found.
#[derive(Debug, PartialEq)]
enum Arrived {
    Bytes(usize),
    Closed,
    Nothing,
}

/// What one write did.
#[derive(Debug, PartialEq)]
enum Written {
    Bytes(usize),
    /// The peer is gone.
    Closed,
}

/// A connection or a socket, as the loop sees it.
trait Link {
    /// Reads one chunk or datagram into `buffer` and passes it to `machine`, at the time `now` gives once it
    /// arrived. With `wait` of `None`, takes only what has already arrived. Returns whether the peer closed.
    fn read(
        &mut self,
        machine: &mut impl Machine,
        buffer: &mut [u8],
        wait: Option<Duration>,
        now: &impl Fn() -> u64,
    ) -> io::Result<bool>;
    fn write(&mut self, bytes: &[u8]) -> io::Result<Written>;
    fn close_write(&mut self) -> io::Result<()>;
}

/// Moves the bytes of `machine` over `link` until the test ends.
fn run(
    machine: &mut impl Machine,
    link: &mut impl Link,
    mut on_report: impl FnMut(&Report),
) -> io::Result<Result<(), TestError>> {
    let start = Instant::now();
    let now = || u64::try_from(start.elapsed().as_micros()).unwrap_or(u64::MAX);
    let mut buffer = vec![0; RECEIVE_LEN];
    let mut closed = false;
    loop {
        if !closed {
            closed = link.read(machine, &mut buffer, None, &now)?;
        }
        let now_us = now();
        match machine.poll(now_us) {
            Action::Send(bytes) => match link.write(bytes)? {
                Written::Bytes(count) => machine.sent(count),
                Written::Closed => {
                    closed = true;
                    machine.closed(now());
                }
            },
            Action::Wait { until_us } => {
                let wait = until_us.map_or(LONGEST_WAIT, |until| {
                    Duration::from_micros(until.saturating_sub(now_us)).min(LONGEST_WAIT)
                });
                if closed {
                    std::thread::sleep(wait);
                } else {
                    closed = link.read(machine, &mut buffer, Some(wait), &now)?;
                }
            }
            Action::Report(report) => on_report(&report),
            Action::Close => link.close_write()?,
            Action::Done(outcome) => return Ok(outcome),
        }
    }
}

/// Passes what a read found to `machine`. Returns whether the peer closed.
fn deliver(machine: &mut impl Machine, arrived: Arrived, buffer: &[u8], now_us: u64) -> bool {
    match arrived {
        Arrived::Bytes(count) => {
            machine.received(now_us, buffer.get(..count).unwrap_or_default());
            false
        }
        Arrived::Closed => {
            machine.closed(now_us);
            true
        }
        Arrived::Nothing => false,
    }
}

struct TcpLink<'l> {
    stream: TcpStream,
    /// The server's listener, whose connections the link refuses while the test runs.
    listener: Option<&'l TcpListener>,
}

impl<'l> TcpLink<'l> {
    fn new(stream: TcpStream, listener: Option<&'l TcpListener>) -> io::Result<Self> {
        // A blocked write must not hold the loop past a deadline or an interval.
        stream.set_write_timeout(Some(LONGEST_WAIT))?;
        stream.set_nodelay(true)?;
        Ok(Self { stream, listener })
    }

    /// Closes one connection that waits on the listener, if there is one. A failed accept does not end the test
    /// that runs.
    fn refuse_waiting(&self) {
        // One a turn, so that clients that connect faster than the loop turns cannot stall the test. The kernel
        // drops connections beyond its listen backlog.
        if let Some(listener) = self.listener
            && let Ok((stream, _)) = listener.accept()
        {
            drop(stream);
        }
    }
}

impl Link for TcpLink<'_> {
    fn read(
        &mut self,
        machine: &mut impl Machine,
        buffer: &mut [u8],
        wait: Option<Duration>,
        now: &impl Fn() -> u64,
    ) -> io::Result<bool> {
        self.refuse_waiting();
        let read = match wait {
            None => {
                self.stream.set_nonblocking(true)?;
                let read = self.stream.read(buffer);
                self.stream.set_nonblocking(false)?;
                read
            }
            Some(wait) => {
                self.stream
                    .set_read_timeout(Some(wait.max(Duration::from_micros(1))))?;
                self.stream.read(buffer)
            }
        };
        Ok(deliver(machine, tcp_read(read)?, buffer, now()))
    }

    fn write(&mut self, bytes: &[u8]) -> io::Result<Written> {
        tcp_write(self.stream.write(bytes))
    }

    fn close_write(&mut self) -> io::Result<()> {
        tcp_shutdown(self.stream.shutdown(Shutdown::Write))
    }
}

struct UdpLink<'s> {
    socket: &'s UdpSocket,
    /// The address and port of the test: the server for a client, and the sender of the first datagram for a
    /// server.
    peer: Option<SocketAddr>,
    /// The address and port of the last refused flow, whose datagrams a server drops.
    refused: Option<SocketAddr>,
    /// Whether another flow's datagram ended the test. It stays in the socket for the next test, so the link
    /// reads nothing more. The next test times it from when it reads it, not from its arrival, as a stock server
    /// does.
    kept: bool,
}

impl Link for UdpLink<'_> {
    fn read(
        &mut self,
        machine: &mut impl Machine,
        buffer: &mut [u8],
        wait: Option<Duration>,
        now: &impl Fn() -> u64,
    ) -> io::Result<bool> {
        if self.kept {
            return Ok(false);
        }
        let peeked = match wait {
            None => {
                self.socket.set_nonblocking(true)?;
                let peeked = self.socket.peek_from(buffer);
                self.socket.set_nonblocking(false)?;
                peeked
            }
            Some(wait) => {
                self.socket
                    .set_read_timeout(Some(wait.max(Duration::from_micros(1))))?;
                self.socket.peek_from(buffer)
            }
        };
        let Some((_, from)) = udp_read(peeked)? else {
            return Ok(false);
        };
        let refused = self.refused == Some(from);
        let ours = self.peer.is_none_or(|peer| peer == from);
        if !refused && !ours && machine.another_flow_arrived() {
            self.kept = true;
            return Ok(false);
        }
        let Some((count, _)) = udp_read(self.socket.recv_from(buffer))? else {
            return Ok(false);
        };
        if !refused && ours {
            machine.received(now(), buffer.get(..count).unwrap_or_default());
            if self.peer.is_none() && !machine.waits_for_first() {
                self.peer = Some(from);
            }
        }
        Ok(false)
    }

    fn write(&mut self, bytes: &[u8]) -> io::Result<Written> {
        let peer = self
            .peer
            .ok_or_else(|| io::Error::new(ErrorKind::NotConnected, "no datagram has arrived"))?;
        udp_send(bytes.len(), || self.socket.send_to(bytes, peer))
    }

    fn close_write(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// What the result of a TCP read means to the core.
fn tcp_read(read: io::Result<usize>) -> io::Result<Arrived> {
    match read {
        Ok(0) => Ok(Arrived::Closed),
        Ok(count) => Ok(Arrived::Bytes(count)),
        Err(error) if gone(&error) => Ok(Arrived::Closed),
        Err(error) if waited(&error) => Ok(Arrived::Nothing),
        Err(error) => Err(error),
    }
}

/// What the result of a TCP write means to the core. A write that timed out wrote nothing.
fn tcp_write(write: io::Result<usize>) -> io::Result<Written> {
    match write {
        Ok(count) => Ok(Written::Bytes(count)),
        Err(error) if gone(&error) => Ok(Written::Closed),
        Err(error) if waited(&error) => Ok(Written::Bytes(0)),
        Err(error) => Err(error),
    }
}

/// The result of shutting down the write side. A peer that has already gone does not need a shutdown.
fn tcp_shutdown(result: io::Result<()>) -> io::Result<()> {
    match result {
        Err(error) if !gone(&error) && error.kind() != ErrorKind::NotConnected => Err(error),
        _ => Ok(()),
    }
}

/// The datagram a UDP read returned, or `None` when nothing arrived. An ICMP port unreachable from an earlier send
/// reports as a reset or a refusal, because the peer may not listen yet.
fn udp_read(read: io::Result<(usize, SocketAddr)>) -> io::Result<Option<(usize, SocketAddr)>> {
    match read {
        Ok(read) => Ok(Some(read)),
        Err(error) if waited(&error) || gone(&error) => Ok(None),
        Err(error) => Err(error),
    }
}

/// Sends a datagram of `len` bytes with `send`. A send that reports the peer gone counts as sent, because the
/// datagram is lost as the network may lose any datagram. Any other failure is tried again a millisecond later,
/// up to `UDP_SEND_TRIES` sends.
fn udp_send(len: usize, mut send: impl FnMut() -> io::Result<usize>) -> io::Result<Written> {
    let mut tries: u32 = 0;
    loop {
        match send() {
            Ok(count) => return Ok(Written::Bytes(count)),
            Err(error) if gone(&error) => return Ok(Written::Bytes(len)),
            Err(error) => {
                tries = tries.saturating_add(1);
                if tries >= UDP_SEND_TRIES {
                    return Err(error);
                }
                std::thread::sleep(Duration::from_millis(1));
            }
        }
    }
}

/// Whether `error` says the peer closed or reset the connection, or is not listening.
fn gone(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        ErrorKind::ConnectionReset
            | ErrorKind::ConnectionAborted
            | ErrorKind::BrokenPipe
            | ErrorKind::ConnectionRefused
    )
}

/// Whether `error` says only that nothing arrived or left within the timeout.
fn waited(error: &io::Error) -> bool {
    matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut)
}

#[cfg(test)]
mod tests {
    use super::*;

    const GONE: [ErrorKind; 4] = [
        ErrorKind::ConnectionReset,
        ErrorKind::ConnectionAborted,
        ErrorKind::BrokenPipe,
        ErrorKind::ConnectionRefused,
    ];
    const WAITED: [ErrorKind; 2] = [ErrorKind::WouldBlock, ErrorKind::TimedOut];

    fn kind<T: core::fmt::Debug>(result: io::Result<T>) -> ErrorKind {
        result.unwrap_err().kind()
    }

    #[test]
    fn a_tcp_server_refuses_one_waiting_connection_per_turn() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let _client = TcpStream::connect(address).unwrap();
        let (served, _) = listener.accept().unwrap();
        listener.set_nonblocking(true).unwrap();
        let _waiting: Vec<TcpStream> = (0..3)
            .map(|_| TcpStream::connect(address).unwrap())
            .collect();
        let link = TcpLink::new(served, Some(&listener)).unwrap();
        link.refuse_waiting();
        let mut left = 0;
        while listener.accept().is_ok() {
            left += 1;
        }
        assert_eq!(left, 2);
    }

    #[test]
    fn a_tcp_read_of_nothing_or_a_reset_is_a_close() {
        assert_eq!(tcp_read(Ok(0)).unwrap(), Arrived::Closed);
        assert_eq!(tcp_read(Ok(7)).unwrap(), Arrived::Bytes(7));
        for gone in GONE {
            assert_eq!(
                tcp_read(Err(gone.into())).unwrap(),
                Arrived::Closed,
                "{gone:?}"
            );
        }
        for waited in WAITED {
            assert_eq!(
                tcp_read(Err(waited.into())).unwrap(),
                Arrived::Nothing,
                "{waited:?}"
            );
        }
        assert_eq!(
            kind(tcp_read(Err(ErrorKind::InvalidData.into()))),
            ErrorKind::InvalidData
        );
    }

    #[test]
    fn a_tcp_write_to_a_gone_peer_is_a_close() {
        assert_eq!(tcp_write(Ok(7)).unwrap(), Written::Bytes(7));
        for gone in GONE {
            assert_eq!(
                tcp_write(Err(gone.into())).unwrap(),
                Written::Closed,
                "{gone:?}"
            );
        }
        for waited in WAITED {
            assert_eq!(
                tcp_write(Err(waited.into())).unwrap(),
                Written::Bytes(0),
                "{waited:?}"
            );
        }
        assert_eq!(
            kind(tcp_write(Err(ErrorKind::InvalidData.into()))),
            ErrorKind::InvalidData
        );
    }

    #[test]
    fn a_shutdown_of_a_gone_connection_succeeds() {
        assert!(tcp_shutdown(Ok(())).is_ok());
        for gone in GONE.into_iter().chain([ErrorKind::NotConnected]) {
            assert!(tcp_shutdown(Err(gone.into())).is_ok(), "{gone:?}");
        }
        assert_eq!(
            kind(tcp_shutdown(Err(ErrorKind::InvalidData.into()))),
            ErrorKind::InvalidData
        );
    }

    #[test]
    fn a_udp_read_that_waited_or_met_an_icmp_error_found_nothing() {
        let from = SocketAddr::from(([127, 0, 0, 1], 5001));
        assert_eq!(udp_read(Ok((7, from))).unwrap(), Some((7, from)));
        for nothing in GONE.into_iter().chain(WAITED) {
            assert_eq!(udp_read(Err(nothing.into())).unwrap(), None, "{nothing:?}");
        }
        assert_eq!(
            kind(udp_read(Err(ErrorKind::InvalidData.into()))),
            ErrorKind::InvalidData
        );
    }

    #[test]
    fn a_udp_send_counts_a_gone_peer_as_sent_and_tries_a_full_buffer_again() {
        assert_eq!(udp_send(9, || Ok(9)).unwrap(), Written::Bytes(9));
        assert_eq!(
            udp_send(9, || Err(ErrorKind::ConnectionRefused.into())).unwrap(),
            Written::Bytes(9)
        );
        let mut fails = 3;
        let full = || {
            if fails == 0 {
                return Ok(9);
            }
            fails -= 1;
            Err(io::Error::other("no buffer space"))
        };
        assert_eq!(udp_send(9, full).unwrap(), Written::Bytes(9));

        let mut tries = 0;
        let always_full = || {
            tries += 1;
            Err(io::Error::other("no buffer space"))
        };
        assert_eq!(kind(udp_send(9, always_full)), ErrorKind::Other);
        assert_eq!(tries, UDP_SEND_TRIES);
    }
}
