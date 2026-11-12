//! The driver for a board: it runs a [`Client`] or a [`Server`] over the sockets of an embassy-net stack (D-3).
//!
//! The caller makes the client or the server and the socket, and the driver reads and writes the socket for it
//! until the test ends. The caller's socket holds the stack and every buffer. The time is the microseconds since
//! the call began, from [`embassy_time::Instant`] (D-5). Each report goes to `on_report` as the core returns it.
//!
//! Each turn of the loop first takes what has arrived without a wait, then polls. The read without a wait leaves
//! a waker on the socket that does nothing. embassy-net reports the peer's FIN as `ConnectionReset`, as it reports
//! a RST, so both reach the core as a close. Each wait has a limit:
//!
//! - A read waits until something arrives or the core's deadline passes.
//! - A TCP write waits at most 100 ms, so a full send buffer does not stop the reads. On a board, this is also the
//!   longest time that the driver takes to see a close while it writes.
//! - A UDP send waits at most 1 s for space, then ends the run with `SendTimedOut`.
//! - A TCP connection waits at most 5 s.
//!
//! After a UDP test that did not fail, the driver waits up to 1 s for the stack to send the datagrams left in the
//! socket.
//!
//! After a TCP test, the driver closes the socket. It waits up to 1 s for the peer's FIN, and up to 1 s for the
//! acknowledgement of its own FIN. When it closed first, it waits 20 ms more, so the stack sends its last ACK. It
//! aborts the connection only when the connection is still open. The caller can then use the socket again, or
//! drop it.
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

/// The longest time that a TCP write waits for space in the send buffer.
const LONGEST_WRITE: Duration = Duration::from_millis(100);
/// The longest time that a UDP send waits for space in the send buffer (D-8). smoltcp keeps a datagram in the
/// buffer until ARP resolves.
///
/// See <https://docs.rs/crate/smoltcp/0.13.1/source/src/storage/packet_buffer.rs#191>.
const SEND_TIMEOUT: Duration = Duration::from_secs(1);
/// The longest time that a wait sleeps once the peer has closed, before the loop turns again.
const LONGEST_SLEEP: Duration = Duration::from_millis(100);
/// The longest time that a client waits for a TCP connection (D-9).
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
/// The longest time that each step of a TCP close waits for the peer.
const CLOSE_TIMEOUT: Duration = Duration::from_secs(1);
/// How long a socket in TIME-WAIT stays with the stack, so the stack sends the ACK of the peer's FIN. smoltcp
/// delays an ACK by 10 ms, and embassy-net 0.9.1 cannot turn the delay off. A socket that the caller drops, or
/// that accepts again, before then never sends the ACK, and the peer waits for it. `listen` resets the socket's
/// ACK timer.
///
/// See <https://docs.rs/crate/smoltcp/0.13.1/source/src/socket/tcp.rs#296>.
/// and <https://docs.rs/crate/smoltcp/0.13.1/source/src/socket/tcp.rs#911>.
/// and <https://docs.rs/crate/embassy-net/0.9.1/source/src/tcp.rs#467>.
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
    /// The socket's send buffer did not have space for 1 s, so the link is down or the stack is stuck.
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
    let outcome = run(client, &mut TcpLink { socket }, Instant::now(), on_report).await;
    end(socket).await;
    outcome
}

/// Runs the UDP test of `client` against `server` over `socket`, which the caller has bound, and returns how the
/// test ended. `on_report` receives each interval and the final report.
///
/// The driver passes the client only the datagrams from `server`.
///
/// # Errors
///
/// - `Mismatch` when `client` is not a UDP client, when `socket` is not bound, or when its send buffer cannot hold
///   one datagram.
/// - `Send` when a datagram cannot be sent.
/// - `SendTimedOut` when the send buffer does not have space for 1 s.
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
        stash: &mut [],
        kept: None,
        ended: false,
    };
    let outcome = run(client, &mut link, Instant::now(), on_report).await?;
    flush(socket).await;
    Ok(outcome)
}

/// A TCP server that runs one test at a time on one socket.
pub struct TcpServer<'s> {
    socket: TcpSocket<'s>,
    port: u16,
}

impl<'s> TcpServer<'s> {
    /// Creates a new server that accepts tests on `port` with `socket`. `socket` must not be connected.
    pub fn new(socket: TcpSocket<'s>, port: u16) -> Self {
        Self { socket, port }
    }

    /// Waits for a connection, runs the test of `server` on it, and returns how the test ended. The stack refuses
    /// each other connection while the test runs (P-14).
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
            Instant::now(),
            on_report,
        )
        .await;
        end(&mut self.socket).await;
        outcome
    }
}

/// A UDP server that runs one test at a time on one bound socket.
///
/// The rules for each datagram are those of D-6, D-11, and D-12:
///
/// - A test belongs to the address and port of the datagram that started it. The server ignores other datagrams
///   until the test ends.
/// - Until a test starts, the server takes datagrams from any address, so a late repeat of an old test's end does
///   not take the server.
/// - A datagram from another address during the wait after the report ends the test. The server keeps it in its
///   stash as the first datagram of the next test.
/// - After a refused test, the server drops the later datagrams of that address and port, until another address
///   and port is refused.
pub struct UdpServer<'s> {
    socket: UdpSocket<'s>,
    refused: Option<IpEndpoint>,
    stash: &'s mut [u8],
    kept: Option<Kept>,
}

/// Another flow's datagram that ended a test, at the start of the stash.
struct Kept {
    len: usize,
    from: IpEndpoint,
    at: Instant,
}

impl<'s> UdpServer<'s> {
    /// Creates a new server on `socket`, which the caller has bound.
    ///
    /// `stash` holds the first datagram of a test that starts while the last test waits after its report, until
    /// the next `run_one` (D-11). It must hold the client's datagram length, 1470 bytes by default. The server
    /// drops a longer first datagram, and that test counts it as lost.
    pub fn new(socket: UdpSocket<'s>, stash: &'s mut [u8]) -> Self {
        Self {
            socket,
            refused: None,
            stash,
            kept: None,
        }
    }

    /// Waits for the first datagram of a test, runs the test of `server`, and returns how the test ended.
    ///
    /// # Errors
    ///
    /// - `Mismatch` when `server` is not a UDP server, when its `ipv6` is set, or when the socket is not bound.
    /// - `Send` when a datagram cannot be sent, such as a reverse test's datagram that is longer than the socket's
    ///   send buffer.
    /// - `SendTimedOut` when the send buffer does not have space for 1 s.
    pub async fn run_one(
        &mut self,
        server: &mut Server<'_>,
        on_report: impl FnMut(&Report),
    ) -> Result<Result<(), TestError>, Error> {
        let config = server.config();
        if config.transport != Transport::Udp || config.ipv6 || self.socket.endpoint().port == 0 {
            return Err(Error::Mismatch);
        }
        let mut peer = None;
        let start = match self.kept.take() {
            Some(kept) => {
                let bytes = self.stash.get(..kept.len).unwrap_or_default();
                deliver_datagram(server, &mut peer, 0, bytes, kept.from);
                kept.at
            }
            None => Instant::now(),
        };
        let mut link = UdpLink {
            socket: &mut self.socket,
            peer,
            refused: self.refused,
            stash: self.stash,
            kept: None,
            ended: false,
        };
        let outcome = run(server, &mut link, start, on_report).await?;
        if outcome == Err(TestError::Refused) {
            self.refused = link.peer;
        }
        self.kept = link.kept;
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
    /// Waits for bytes, a datagram, or the peer's close, and passes it to `machine`. Returns whether the peer
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

/// Moves the bytes of `machine` over `link` until the test ends. The time counts from `start`.
async fn run(
    machine: &mut impl Machine,
    link: &mut impl Link,
    start: Instant,
    mut on_report: impl FnMut(&Report),
) -> Result<Result<(), TestError>, Error> {
    let clock = Clock { start };
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
/// and a socket in TIME-WAIT can connect or accept again. A peer that does not finish its side of the close
/// within the limit gets a RST instead.
///
/// See <https://docs.rs/crate/smoltcp/0.13.1/source/src/socket/tcp.rs#1122>.
async fn end(socket: &mut TcpSocket<'_>) {
    socket.close();
    let drain = async { while socket.read_with(|bytes| (bytes.len(), ())).await.is_ok() {} };
    let _ = select(drain, Timer::after(CLOSE_TIMEOUT)).await;
    let _ = select(socket.flush(), Timer::after(CLOSE_TIMEOUT)).await;
    if socket.state() == State::TimeWait {
        Timer::after(DELAYED_ACK_WAIT).await;
    } else {
        // A socket that closed cleanly from LAST-ACK does not have a peer left, so the abort sends nothing.
        socket.abort();
        let _ = select(socket.flush(), Timer::after(CLOSE_TIMEOUT)).await;
    }
}

/// Waits until the stack has sent the datagrams left in `socket`, such as the end of the test. A socket that the
/// caller drops removes its datagrams from the stack, sent or not.
///
/// See <https://docs.rs/crate/embassy-net/0.9.1/source/src/udp.rs#330>.
/// and <https://docs.rs/crate/embassy-net/0.9.1/source/src/udp.rs#395>.
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
    /// The address and port of the last refused flow. A server drops the datagrams of this flow.
    refused: Option<IpEndpoint>,
    /// Where a server keeps another flow's datagram that ended its test, for the next test. A client's stash is
    /// empty.
    stash: &'l mut [u8],
    kept: Option<Kept>,
    /// Whether another flow's datagram ended the test, so the link reads nothing more.
    ended: bool,
}

impl Link for UdpLink<'_, '_> {
    async fn read(&mut self, machine: &mut impl Machine, clock: &Clock) -> bool {
        if self.ended {
            return false;
        }
        let Self {
            socket,
            peer,
            refused,
            stash,
            kept,
            ended,
        } = self;
        socket
            .recv_from_with(|bytes, meta| {
                let from = meta.endpoint;
                if *refused == Some(from) {
                    return;
                }
                if peer.is_some_and(|peer| peer != from) {
                    // The stack has taken the datagram, so one that arrives in the turn before the report goes
                    // out is lost, and the next test counts it.
                    if machine.another_flow_arrived() {
                        *ended = true;
                        // A datagram longer than the stash is dropped rather than cut, so the next test counts
                        // only whole datagrams.
                        if let Some(to) = stash.get_mut(..bytes.len()) {
                            to.copy_from_slice(bytes);
                            *kept = Some(Kept {
                                len: bytes.len(),
                                from,
                                at: Instant::now(),
                            });
                        }
                    }
                    return;
                }
                deliver_datagram(machine, peer, clock.now_us(), bytes, from);
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

/// Passes a datagram of the test to `machine`, and makes its sender the peer once the machine has stopped waiting
/// for its first datagram.
fn deliver_datagram(
    machine: &mut impl Machine,
    peer: &mut Option<IpEndpoint>,
    now_us: u64,
    bytes: &[u8],
    from: IpEndpoint,
) {
    machine.received(now_us, bytes);
    if peer.is_none() && !machine.waits_for_first() {
        *peer = Some(from);
    }
}
