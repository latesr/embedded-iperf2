//! `iperf -s [-u] [-p <port>]`: one iperf 2 server test over embedded-iperf2's embassy-net driver.
//!
//! The end writes `result: <a>-<b> s <bytes> bytes <rate> Mbit/s`, the shape of `net iperf`'s `result:` line, so
//! `cargo xtask hil` reads both with one parser. A UDP test adds `lost <n>/<n> ooo <n> jitter <t> ms`. Ctrl-C ends
//! the wait for a test, and a test that has started ends at the server's duration limit at the latest.

use core::fmt::{self, Display, Formatter};
use embassy_futures::select::{Either, select};
use embassy_net::Stack;
use embassy_net::tcp::TcpSocket;
use embassy_net::udp::{PacketMetadata, UdpSocket};
use embedded_iperf2::embassy::{Error, TcpServer, UdpServer};
use embedded_iperf2::{Report, Server, ServerConfig, TestError, Transport};
use mcu_console::{Args, Cancel, CommandResult, Commands, Help, Reply};

const USAGE: &str = "usage: iperf -s [-u] [-p port]";

/// The port an iperf 2.2.2 server listens on.
const PORT: u16 = 5001;

/// The longest a test lasts, so a client that stops sending cannot hold the console (P-15).
const LIMIT_US: u64 = 60_000_000;

/// The length of each socket buffer, receive and send. Internal RAM does not hold a second pair at `net iperf`'s
/// 64 KiB beside the Ethernet rings. 48 KiB is many times what a direct cable at 100 Mbit/s has in flight.
const SOCKET: usize = 49152;

/// How many datagrams the UDP socket queues each way: 32 datagrams of 1470 bytes fill 46 KiB, as in `mcu-net`.
const DATAGRAMS: usize = 32;

/// The longest datagram a reverse UDP test sends over IPv4, which the stash must hold (P-21).
const STASH: usize = 1470;

static HELP: [Help; 1] = [Help {
    usage: "iperf -s [-u] [-p port]",
    summary: "run one iperf 2 server test",
    secret: false,
}];

/// The socket buffers, the server's work buffer and the UDP stash of `iperf -s`.
pub struct Buffers {
    rx: [u8; SOCKET],
    tx: [u8; SOCKET],
    work: [u8; crate::WORK],
    stash: [u8; STASH],
    rx_meta: [PacketMetadata; DATAGRAMS],
    tx_meta: [PacketMetadata; DATAGRAMS],
}

impl Buffers {
    pub const fn new() -> Self {
        Self {
            rx: [0; SOCKET],
            tx: [0; SOCKET],
            work: [0; crate::WORK],
            stash: [0; STASH],
            rx_meta: [PacketMetadata::EMPTY; DATAGRAMS],
            tx_meta: [PacketMetadata::EMPTY; DATAGRAMS],
        }
    }
}

/// The `iperf -s` command.
pub struct IperfServer {
    stack: Stack<'static>,
    buffers: &'static mut Buffers,
}

impl IperfServer {
    pub fn new(stack: Stack<'static>, buffers: &'static mut Buffers) -> Self {
        Self { stack, buffers }
    }
}

impl Commands for IperfServer {
    fn help(&self) -> impl Iterator<Item = &'static Help> {
        HELP.iter()
    }

    async fn run<R: Reply>(
        &mut self,
        name: &str,
        args: Args<'_>,
        reply: &mut R,
        cancel: Cancel<'_>,
    ) -> CommandResult {
        if name != "iperf" {
            return CommandResult::Unknown;
        }
        let Some((transport, port)) = parse(args) else {
            return CommandResult::Err(USAGE);
        };
        let config = ServerConfig {
            transport,
            duration_us: Some(LIMIT_US),
            interval_us: None,
            ipv6: false,
        };
        let Buffers {
            rx,
            tx,
            work,
            stash,
            rx_meta,
            tx_meta,
        } = &mut *self.buffers;
        let Ok(mut server) = Server::new(config, work) else {
            return CommandResult::Err("the work buffer is too short");
        };
        let mut last = None;
        let on_report = |report: &Report| {
            if report.final_report {
                last = Some(*report);
            }
        };
        let test = match transport {
            Transport::Tcp => {
                let mut driver = TcpServer::new(TcpSocket::new(self.stack, rx, tx), port);
                select(driver.run_one(&mut server, on_report), cancel.requested()).await
            }
            Transport::Udp => {
                let mut socket = UdpSocket::new(self.stack, rx_meta, rx, tx_meta, tx);
                if socket.bind(port).is_err() {
                    return CommandResult::Err("the port is in use");
                }
                let mut driver = UdpServer::new(socket, stash);
                select(driver.run_one(&mut server, on_report), cancel.requested()).await
            }
        };
        let outcome = match test {
            Either::First(outcome) => outcome,
            Either::Second(()) => return CommandResult::Err("interrupted"),
        };
        if let Some(report) = last {
            reply.field("result", Measured(report)).await;
        }
        match outcome {
            Ok(Ok(())) => CommandResult::Ok,
            Ok(Err(TestError::Refused)) => CommandResult::Err("the server refused the test"),
            Ok(Err(TestError::NoData)) => CommandResult::Err("no data arrived"),
            Ok(Err(TestError::Protocol)) => CommandResult::Err("the client broke the protocol"),
            Err(Error::Accept(_)) => CommandResult::Err("the socket could not accept"),
            Err(Error::Send(_)) => CommandResult::Err("a datagram could not be sent"),
            Err(Error::SendTimedOut) => CommandResult::Err("the send buffer stayed full"),
            Err(Error::Connect(_) | Error::Mismatch) => CommandResult::Err("a driver mismatch"),
        }
    }
}

/// The transport and the port of an `iperf -s` line, or `None` when the line does not parse.
fn parse(mut args: Args<'_>) -> Option<(Transport, u16)> {
    if args.next()? != "-s" {
        return None;
    }
    let (mut transport, mut port) = (Transport::Tcp, PORT);
    while let Some(option) = args.next() {
        match option {
            "-u" => transport = Transport::Udp,
            "-p" => port = args.next()?.parse().ok()?,
            _ => return None,
        }
    }
    Some((transport, port))
}

/// A final report as `<a>-<b> s <bytes> bytes <rate> Mbit/s`, and a UDP receiver's counts.
struct Measured(Report);

impl Display for Measured {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let report = &self.0;
        let elapsed = report.end_us.saturating_sub(report.start_us);
        // Bytes per microsecond times 8 is Mbit/s, and 800 gives hundredths of it.
        let hundredths = report
            .bytes
            .saturating_mul(800)
            .checked_div(elapsed)
            .unwrap_or(0);
        write!(
            f,
            "{}-{} s {} bytes {}.{:02} Mbit/s",
            Seconds(report.start_us),
            Seconds(report.end_us),
            report.bytes,
            hundredths / 100,
            hundredths % 100
        )?;
        if let Some(udp) = report.udp {
            let jitter_us = udp.jitter_ns / 1_000;
            write!(
                f,
                " lost {}/{} ooo {} jitter {}.{:03} ms",
                udp.lost,
                udp.datagrams,
                udp.out_of_order,
                jitter_us / 1_000,
                jitter_us % 1_000
            )?;
        }
        Ok(())
    }
}

/// Microseconds written as seconds with two decimals, such as `1.50`.
struct Seconds(u64);

impl Display for Seconds {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}.{:02}",
            self.0 / 1_000_000,
            self.0 % 1_000_000 / 10_000
        )
    }
}
