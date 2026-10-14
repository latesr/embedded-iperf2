//! The driver for a board: it runs a [`Client`] or a [`Server`] over the sockets of an embassy-net stack (D-3).
//!
//! The caller makes the client or the server and the socket, and the driver reads and writes the socket for it
//! until the test ends. The caller's socket holds the stack and every buffer. The time is the microseconds since
//! the call began, from [`embassy_time::Instant`] (D-5). Each report goes to `on_report` as the core returns it.
//!
//! Each turn of the loop first takes what has arrived without waiting, then polls. A wait reads until something
//! arrives or the core's deadline passes. embassy-net reports the peer's FIN as `ConnectionReset`, as it reports a
//! RST, so both reach the core as a close. A TCP write waits at most 100 ms, so a full send buffer does not stop
//! the reads. On a board, that is also the longest the driver takes to see a close while it writes. The read
//! without waiting leaves a waker on the socket that does nothing. A UDP send waits at most 1 s for room, and then
//! ends the run with `SendTimedOut`. A TCP connection waits at most 5 s.
//!
//! After a UDP test that did not fail, the driver waits up to 1 s for the stack to send the datagrams left in the
//! socket.
//! After a TCP test, the driver closes the socket. It waits up to 1 s for the peer's FIN, and up to 1 s for its own
//! FIN to be acknowledged. When it closed first, it waits 20 ms more, so the stack sends its last ACK. It
//! aborts the connection only when it is still open. The caller can then use the socket again, or drop it.
//!
//! While a test runs, the server's socket holds the connection and nothing listens on the port, so the stack
//! answers a second connection with a RST (P-14). The driver uses IPv4 (D-4).

use crate::machine::Machine;
use crate::{Action, Client, Report, Server, TestError, Transport};
use core::task::Poll;
use embassy_futures::poll_once;
use embassy_futures::select::{Either, select};
use embassy_net::IpEndpoint;
use embassy_net::tcp::{AcceptError, ConnectError, State, TcpSocket};
use embassy_net::udp::{SendError, UdpSocket};
use embassy_time::{Duration, Instant, Timer};

/// The longest a TCP write waits for room in the send buffer.
const LONGEST_WRITE: Duration = Duration::from_millis(100);
/// The longest a UDP send waits for room in the send buffer. The core counts the end of a UDP test by its sends,
/// so a send that never leaves would hold the test for ever.
const SEND_TIMEOUT: Duration = Duration::from_secs(1);
/// The longest a wait sleeps once the peer has closed, before the loop turns again.
const LONGEST_SLEEP: Duration = Duration::from_millis(100);
/// The longest a client waits for a TCP connection.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
/// The longest each step of a TCP close waits for the peer.
const CLOSE_TIMEOUT: Duration = Duration::from_secs(1);
/// How long a socket in TIME-WAIT stays with the stack, so the stack sends the ACK of the peer's FIN. smoltcp
/// delays an ACK by 10 ms, and embassy-net 0.9.1 cannot turn the delay off. A socket that the caller drops, or
/// that accepts again, before then never sends the ACK, and the peer waits for it. `listen` resets the socket's
/// ACK timer.
const DELAYED_ACK_WAIT: Duration = Duration::from_millis(20);

/// Why a run failed before or outside its test.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// The TCP connection to the server failed, or took longer than 5 s.
    Connect(ConnectError),
    /// The TCP server could not accept a connection.
    Accept(AcceptError),
    /// A UDP datagram could not be sent.
    Send(SendError),
    /// The socket's send buffer had no room for 1 s, so the link is down or the stack is stuck.
    SendTimedOut,
    /// The socket or the settings do not fit the test. The function that returns it names each case.
    Mismatch,
}

/// Runs the TCP test of `client` against `server` over `socket`, and returns how the test ended.
///
/// `socket` must not be connected. It is closed again when the call returns, so the caller can use it for the
/// next test. `on_report` receives each interval and the final report.
///
/// Returns `Mismatch` when `client` is not a TCP client, and `Connect` when the connection fails or takes longer
/// than 5 s.
pub async fn run_tcp_client(
    client: &mut Client<'_>,
    socket: &mut TcpSocket<'_>,
    server: IpEndpoint,
    on_report: impl FnMut(&Report),
) -> Result<Result<(), TestError>, Error> {
    if client.config().transport != Transport::Tcp {
        return Err(Error::Mismatch);
    }
    match select(socket.connect(server), Timer::after(CONNECT_TIMEOUT)).await {
        Either::First(Ok(())) => {}
        Either::First(Err(error)) => return Err(Error::Connect(error)),
        Either::Second(()) => {
            // Without the abort, the socket would stay in SynSent and could not connect again.
            socket.abort();
            let _ = select(socket.flush(), Timer::after(CLOSE_TIMEOUT)).await;
            return Err(Error::Connect(ConnectError::TimedOut));
        }
    }
    let outcome = run(client, &mut TcpLink { socket }, on_report).await;
    end(socket).await;
    outcome
}

/// Runs the UDP test of `client` against `server` over `socket`, which the caller has bound, and returns how the
/// test ended. `on_report` receives each interval and the final report.
///
/// The driver passes the client only the datagrams from `server`. Returns `Mismatch` when `client` is not a UDP
/// client, when `socket` is not bound, or when its send buffer cannot hold one datagram. Returns `Send` when a
/// datagram cannot be sent, and `SendTimedOut` when the send buffer has no room for 1 s.
pub async fn run_udp_client(
    client: &mut Client<'_>,
    socket: &mut UdpSocket<'_>,
    server: IpEndpoint,
    on_report: impl FnMut(&Report),
) -> Result<Result<(), TestError>, Error> {
    let config = client.config();
    if config.transport != Transport::Udp
        || socket.endpoint().port == 0
        || socket.payload_send_capacity() < config.buf_len as usize
    {
        return Err(Error::Mismatch);
    }
    let mut link = UdpLink {
        socket,
        peer: Some(server),
        refused: None,
    };
    let outcome = run(client, &mut link, on_report).await?;
    flush(socket).await;
    Ok(outcome)
}

/// A TCP server that runs one test at a time on one socket.
pub struct TcpServer<'s> {
    socket: TcpSocket<'s>,
    port: u16,
}

impl<'s> TcpServer<'s> {
    /// A server that accepts tests on `port` with `socket`, which must not be connected.
    pub fn new(socket: TcpSocket<'s>, port: u16) -> Self {
        Self { socket, port }
    }

    /// Waits for a connection and runs the test of `server` on it, and returns how the test ended. The stack
    /// refuses each other connection while the test runs (P-14).
    ///
    /// Returns `Mismatch` when `server` is not a TCP server, and `Accept` when the socket cannot accept.
    pub async fn run_one(
        &mut self,
        server: &mut Server<'_>,
        on_report: impl FnMut(&Report),
    ) -> Result<Result<(), TestError>, Error> {
        if server.config().transport != Transport::Tcp {
            return Err(Error::Mismatch);
        }
        self.socket.accept(self.port).await.map_err(Error::Accept)?;
        let outcome = run(
            server,
            &mut TcpLink {
                socket: &mut self.socket,
            },
            on_report,
        )
        .await;
        end(&mut self.socket).await;
        outcome
    }
}

/// A UDP server that runs one test at a time on one bound socket.
///
/// A test belongs to the address and port of the datagram that started it, and the server ignores other
/// datagrams until the test ends. After a refused test, it drops the later datagrams of that address and port,
/// until another address and port is refused (D-6).
pub struct UdpServer<'s> {
    socket: UdpSocket<'s>,
    refused: Option<IpEndpoint>,
}

impl<'s> UdpServer<'s> {
    /// A server on `socket`, which the caller has bound.
    pub fn new(socket: UdpSocket<'s>) -> Self {
        Self {
            socket,
            refused: None,
        }
    }

    /// Waits for the first datagram of a test and runs the test of `server`, and returns how the test ended.
    ///
    /// Returns `Mismatch` when `server` is not a UDP server, when its `ipv6` is set, or when the socket is not
    /// bound. Returns `Send` when a datagram cannot be sent, such as a reverse test's datagram that is longer than
    /// the socket's send buffer. Returns `SendTimedOut` when the send buffer has no room for 1 s.
    pub async fn run_one(
        &mut self,
        server: &mut Server<'_>,
        on_report: impl FnMut(&Report),
    ) -> Result<Result<(), TestError>, Error> {
        let config = server.config();
        if config.transport != Transport::Udp || config.ipv6 || self.socket.endpoint().port == 0 {
            return Err(Error::Mismatch);
        }
        let mut link = UdpLink {
            socket: &mut self.socket,
            peer: None,
            refused: self.refused,
        };
        let outcome = run(server, &mut link, on_report).await?;
        if outcome == Err(TestError::Refused) {
            self.refused = link.peer;
        }
        flush(&mut self.socket).await;
        Ok(outcome)
    }
}

/// What one write did.
enum Written {
    Bytes(usize),
    /// The peer is gone.
    Closed,
}

/// A connection or a socket, as the loop sees it.
trait Link {
    /// Waits for bytes, a datagram or the peer's close, and passes it to `machine`. Returns whether the peer
    /// closed.
    async fn read(&mut self, machine: &mut impl Machine, clock: &Clock) -> bool;
    async fn write(&mut self, bytes: &[u8]) -> Result<Written, Error>;
    fn close_write(&mut self);
}

/// The time since the call began.
struct Clock {
    start: Instant,
}

impl Clock {
    fn now_us(&self) -> u64 {
        self.start.elapsed().as_micros()
    }

    fn at(&self, us: u64) -> Instant {
        self.start
            .checked_add(Duration::from_micros(us))
            .unwrap_or(Instant::MAX)
    }
}

/// Moves the bytes of `machine` over `link` until the test ends.
async fn run(
    machine: &mut impl Machine,
    link: &mut impl Link,
    mut on_report: impl FnMut(&Report),
) -> Result<Result<(), TestError>, Error> {
    let clock = Clock {
        start: Instant::now(),
    };
    let mut closed = false;
    loop {
        if !closed && let Poll::Ready(close) = poll_once(link.read(machine, &clock)) {
            closed = close;
        }
        let now_us = clock.now_us();
        match machine.poll(now_us) {
            Action::Send(bytes) => match link.write(bytes).await? {
                Written::Bytes(count) => machine.sent(count),
                Written::Closed => {
                    closed = true;
                    machine.closed(clock.now_us());
                }
            },
            Action::Wait { until_us } => {
                let deadline = until_us.map_or(Instant::MAX, |until| clock.at(until));
                if closed {
                    Timer::at(deadline.min(Instant::now().saturating_add(LONGEST_SLEEP))).await;
                } else if let Either::First(close) =
                    select(link.read(machine, &clock), Timer::at(deadline)).await
                {
                    closed = close;
                }
            }
            Action::Report(report) => on_report(&report),
            Action::Close => link.close_write(),
            Action::Done(outcome) => return Ok(outcome),
        }
    }
}

/// Closes `socket` after a test, so that it can connect or accept again. A clean close leaves it in TIME-WAIT,
/// from which it can. A peer that does not finish its side of the close within the limit gets a RST instead.
async fn end(socket: &mut TcpSocket<'_>) {
    socket.close();
    let drain = async { while socket.read_with(|bytes| (bytes.len(), ())).await.is_ok() {} };
    let _ = select(drain, Timer::after(CLOSE_TIMEOUT)).await;
    let _ = select(socket.flush(), Timer::after(CLOSE_TIMEOUT)).await;
    if socket.state() == State::TimeWait {
        Timer::after(DELAYED_ACK_WAIT).await;
    } else {
        // A socket that closed cleanly from LAST-ACK has no peer left, so the abort sends nothing.
        socket.abort();
        let _ = select(socket.flush(), Timer::after(CLOSE_TIMEOUT)).await;
    }
}

/// Waits until the stack has sent the datagrams left in `socket`, such as the end of the test. A socket that the
/// caller drops removes its datagrams from the stack, sent or not.
async fn flush(socket: &mut UdpSocket<'_>) {
    let _ = select(socket.flush(), Timer::after(CLOSE_TIMEOUT)).await;
}

struct TcpLink<'l, 's> {
    socket: &'l mut TcpSocket<'s>,
}

impl Link for TcpLink<'_, '_> {
    async fn read(&mut self, machine: &mut impl Machine, clock: &Clock) -> bool {
        let read = self.socket.read_with(|bytes| {
            machine.received(clock.now_us(), bytes);
            (bytes.len(), ())
        });
        match read.await {
            Ok(()) => false,
            Err(_) => {
                machine.closed(clock.now_us());
                true
            }
        }
    }

    async fn write(&mut self, bytes: &[u8]) -> Result<Written, Error> {
        match select(self.socket.write(bytes), Timer::after(LONGEST_WRITE)).await {
            Either::First(Ok(count)) => Ok(Written::Bytes(count)),
            Either::First(Err(_)) => Ok(Written::Closed),
            Either::Second(()) => Ok(Written::Bytes(0)),
        }
    }

    fn close_write(&mut self) {
        self.socket.close();
    }
}

struct UdpLink<'l, 's> {
    socket: &'l mut UdpSocket<'s>,
    /// The address and port of the test: the server for a client, and the sender of the first datagram for a
    /// server.
    peer: Option<IpEndpoint>,
    /// The address and port of the last refused flow, whose datagrams a server drops.
    refused: Option<IpEndpoint>,
}

impl Link for UdpLink<'_, '_> {
    async fn read(&mut self, machine: &mut impl Machine, clock: &Clock) -> bool {
        let peer = &mut self.peer;
        let refused = self.refused;
        self.socket
            .recv_from_with(|bytes, meta| {
                let from = meta.endpoint;
                if refused == Some(from) || peer.is_some_and(|peer| peer != from) {
                    return;
                }
                *peer = Some(from);
                machine.received(clock.now_us(), bytes);
            })
            .await;
        false
    }

    async fn write(&mut self, bytes: &[u8]) -> Result<Written, Error> {
        // The core sends only after the peer's first datagram, which sets `peer`.
        let peer = self.peer.ok_or(Error::Mismatch)?;
        match select(self.socket.send_to(bytes, peer), Timer::after(SEND_TIMEOUT)).await {
            Either::First(sent) => sent.map_err(Error::Send)?,
            Either::Second(()) => return Err(Error::SendTimedOut),
        }
        Ok(Written::Bytes(bytes.len()))
    }

    fn close_write(&mut self) {}
}
