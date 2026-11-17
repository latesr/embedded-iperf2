//! T-5: tests the `std` driver against a stock iperf 2.2.2 on loopback.
//!
//! The crate's client runs against a stock server, and a stock client runs against the crate's server. The tests
//! cover TCP and UDP, forward and reverse, in time and byte mode.
//!
//! Each test runs the binary that `MCU_IPERF2` names, and fails without that binary. `cargo xtask install-iperf`
//! builds the binary. The tests are ignored, so the pre-commit hook and cargo-mutants leave them out.
//! `cargo xtask check --push` and `--full` run them. The tests in `blocking.rs` cover the driver's code.

#![cfg(feature = "std")]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

mod loopback;

use embedded_iperf2::{Config, Length, Report, Transport};
use loopback::{End, serve, server_config, spawn, try_run};
use std::io::ErrorKind;
use std::net::{SocketAddr, TcpListener, UdpSocket};
use std::process::{Child, Command, Stdio};
use std::sync::OnceLock;
use std::thread;
use std::time::{Duration, Instant};

/// The longest time that a stock server gets to open its socket.
const SERVER_START: Duration = Duration::from_secs(3);
/// The number of stock servers that a test starts before it fails. A server can find its port taken by a probe.
const SERVER_TRIES: u32 = 3;
/// The time a stock server gets to exit after the check finds its port held. A server that fails its bind prints
/// the error at once, but exits about 1 s later. A server that has bound its port does not exit.
const SETTLE: Duration = Duration::from_millis(1500);
/// The time a stock process gets to exit, after the crate's server ends its test or after SIGINT.
const EXIT_WAIT: Duration = Duration::from_secs(3);
const RATE: u64 = 4_000_000;

#[derive(Clone, Copy, Debug)]
struct Mode {
    transport: Transport,
    reverse: bool,
    length: Length,
}

fn modes() -> Vec<Mode> {
    let mut modes = Vec::new();
    for transport in [Transport::Tcp, Transport::Udp] {
        for reverse in [false, true] {
            for length in [Length::Bytes(51_200), Length::Time { us: 500_000 }] {
                modes.push(Mode {
                    transport,
                    reverse,
                    length,
                });
            }
        }
    }
    modes
}

impl Mode {
    fn buf_len(self) -> u32 {
        match self.transport {
            Transport::Tcp => 1000,
            Transport::Udp => 512,
        }
    }

    fn config(self) -> Config {
        let base = match self.transport {
            Transport::Tcp => Config::tcp(),
            Transport::Udp => Config::udp(),
        };
        Config {
            reverse: self.reverse,
            length: self.length,
            buf_len: self.buf_len(),
            rate: RATE,
            ..base
        }
    }

    /// The arguments of a stock client for this mode, after `-c` and `-p`.
    fn client_args(self) -> Vec<String> {
        let mut args = vec![
            "-l".to_owned(),
            self.buf_len().to_string(),
            "-b".to_owned(),
            RATE.to_string(),
        ];
        match self.length {
            Length::Bytes(count) => args.extend(["-n".to_owned(), count.to_string()]),
            Length::Time { us } => args.extend(["-t".to_owned(), format!("{}", us as f64 / 1e6)]),
        }
        if self.transport == Transport::Udp {
            args.push("-u".to_owned());
        }
        if self.reverse {
            args.push("-R".to_owned());
        }
        args
    }
}

/// The stock binary, once it reports 2.2.2. The check runs once.
fn binary() -> &'static str {
    static BINARY: OnceLock<String> = OnceLock::new();
    BINARY.get_or_init(|| {
        let binary = std::env::var("MCU_IPERF2").expect(
            "MCU_IPERF2 must name an iperf 2.2.2 binary, which `cargo xtask install-iperf` builds",
        );
        let output = Command::new(&binary).arg("-v").output().unwrap();
        let version =
            String::from_utf8_lossy(&[output.stdout, output.stderr].concat()).into_owned();
        assert!(
            version.starts_with("iperf 2.2.2 "),
            "{binary} reports {version}"
        );
        binary
    })
}

/// A stock iperf process, which stops when it is dropped.
struct Stock(Option<Child>);

impl Stock {
    fn start(args: &[String]) -> Self {
        let child = Command::new(binary())
            .args(args)
            .args(["-y", "C"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        Self(Some(child))
    }

    /// Stops the process with SIGINT, and returns what it printed.
    ///
    /// SIGINT makes the process flush its reports. A process that still runs `EXIT_WAIT` later is killed, and the
    /// text says so.
    fn interrupt(mut self) -> String {
        let id = self.0.as_ref().unwrap().id().to_string();
        let status = Command::new("kill").args(["-INT", &id]).status().unwrap();
        assert!(status.success(), "kill -INT failed");
        if let Some(printed) = self.exited_within(EXIT_WAIT) {
            return printed;
        }
        let mut child = self.0.take().unwrap();
        let _ = child.kill();
        format!(
            "{}\n(the process ignored SIGINT for {EXIT_WAIT:?} and was killed)",
            printed(child)
        )
    }

    /// Waits up to `EXIT_WAIT` for the process to exit, and returns what it printed. Panics with what it printed
    /// when it does not exit in time.
    fn finish(mut self) -> String {
        if let Some(printed) = self.exited_within(EXIT_WAIT) {
            return printed;
        }
        panic!(
            "the stock client did not exit within {EXIT_WAIT:?}:\n{}",
            self.interrupt()
        );
    }

    /// What the process printed, once it exits within `limit`. A `limit` of zero checks once.
    fn exited_within(&mut self, limit: Duration) -> Option<String> {
        let started = Instant::now();
        loop {
            if self.0.as_mut().unwrap().try_wait().unwrap().is_some() {
                return Some(printed(self.0.take().unwrap()));
            }
            if started.elapsed() >= limit {
                return None;
            }
            thread::sleep(Duration::from_millis(20));
        }
    }
}

/// What `child` printed, on standard output and then standard error, once it exits.
fn printed(child: Child) -> String {
    let output = child.wait_with_output().unwrap();
    String::from_utf8_lossy(&[output.stdout, output.stderr].concat()).into_owned()
}

impl Drop for Stock {
    fn drop(&mut self) {
        if let Some(child) = &mut self.0 {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// The counts in the last report line that a stock iperf printed with `-y C`.
#[derive(Debug, PartialEq)]
struct Counts {
    bytes: u64,
    /// The lost and out-of-order datagrams of a UDP report.
    udp: Option<(u64, u64)>,
}

/// Reads the last report line of `output`. A TCP line has nine fields, and the eighth holds the bytes. A UDP line
/// adds the jitter, the lost datagrams, the total, the percent lost, and the out-of-order datagrams.
fn last_report(output: &str) -> Counts {
    let line = output
        .lines()
        .rfind(|line| matches!(line.split(',').count(), 9 | 14))
        .unwrap_or_else(|| panic!("no report in:\n{output}"));
    let fields: Vec<&str> = line.split(',').collect();
    let number = |at: usize| -> u64 { fields[at].parse().unwrap_or_else(|_| panic!("{line}")) };
    Counts {
        bytes: number(7),
        udp: (fields.len() == 14).then(|| (number(10), number(13))),
    }
}

/// Checks that the stock end's last report and the crate's final report agree. Both ends count the end of a UDP
/// test, so the bytes are equal in every mode.
fn agree(mode: Mode, ours_sends: bool, printed: &str, ours: &Report) {
    let stock = last_report(printed);
    let case = format!("{mode:?}, ours sends {ours_sends}:\n{printed}");
    assert_eq!(stock.bytes, ours.bytes, "{case}");
    if mode.transport == Transport::Tcp {
        return;
    }
    if ours_sends {
        // The stock receiver loses nothing on loopback. A forward client holds the server report. The stock
        // receiver writes that report from its own count.
        assert_eq!(stock.udp, Some((0, 0)), "{case}");
        if let Some(server) = ours.server {
            let reported = Counts {
                bytes: server.bytes,
                udp: Some((server.lost, server.out_of_order)),
            };
            assert_eq!(stock, reported, "{case}");
        }
    } else {
        // A stock sender's line always holds 0 lost and 0 out of order. The crate's end receives on loopback, so
        // its counts must be 0 too.
        let udp = ours.udp.expect(&case);
        assert_eq!((udp.lost, udp.out_of_order), (0, 0), "{case}");
    }
}

fn free_port(transport: Transport) -> u16 {
    match transport {
        Transport::Tcp => TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port(),
        Transport::Udp => UdpSocket::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port(),
    }
}

/// Starts a stock server, and returns it with its port once it has bound the port.
///
/// The crate's client counts a datagram that the kernel refuses as sent, so a UDP client that starts first loses
/// datagrams. A connection or a datagram that only checks the port would start a test on the server, so the check
/// binds the port instead. A server that binds while a check holds the port exits, and the next try uses a new port.
/// On Linux, a TCP check can also bind between the server's bind and its listen. The server then cannot listen, and
/// exits.
///
/// The server must bind `127.0.0.1`, not every address. Another test can bind `127.0.0.1` on the same port after
/// `free_port` lets it go, and a loopback connection or datagram then goes to that socket, not to the server. With
/// `-B`, the server cannot bind a port that another socket holds, and exits.
fn stock_server(transport: Transport) -> (Stock, u16) {
    let mut printed = String::new();
    for _ in 0..SERVER_TRIES {
        let port = free_port(transport);
        let mut args = vec![
            "-s".to_owned(),
            "-B".to_owned(),
            "127.0.0.1".to_owned(),
            "-p".to_owned(),
            port.to_string(),
        ];
        if transport == Transport::Udp {
            args.push("-u".to_owned());
        }
        let mut stock = Stock::start(&args);
        let started = Instant::now();
        while started.elapsed() < SERVER_START {
            // The bind check also passes when another socket holds the port and the server has exited, or has not yet
            // tried its bind. So the exit is checked before the bind check, and again after it.
            if let Some(output) = stock.exited_within(Duration::ZERO) {
                printed = output;
                break;
            }
            if bound(transport, port) {
                match stock.exited_within(SETTLE) {
                    Some(output) => {
                        printed = output;
                        break;
                    }
                    None => return (stock, port),
                }
            }
            thread::sleep(Duration::from_millis(10));
        }
    }
    panic!(
        "no stock server held its port within {SERVER_START:?} in {SERVER_TRIES} tries:\n{printed}"
    );
}

/// Returns whether another socket holds `port` on `127.0.0.1`.
fn bound(transport: Transport, port: u16) -> bool {
    let address = ("127.0.0.1", port);
    let error = match transport {
        Transport::Tcp => TcpListener::bind(address).err(),
        Transport::Udp => UdpSocket::bind(address).err(),
    };
    error.is_some_and(|error| error.kind() == ErrorKind::AddrInUse)
}

/// Runs the crate's client of `config` to its end, against the stock server on `port`. Then stops the server, and
/// returns the client's end with what the server printed.
///
/// A stock TCP server binds its port before it listens, so a connection can be refused after the port is bound.
/// A refused connection does not start a test, so the client connects again until `SERVER_START` passes. Panics
/// with what the server printed when the client fails with a socket error.
fn run_against_stock(config: Config, stock: Stock, port: u16) -> (End, String) {
    let server = SocketAddr::from(([127, 0, 0, 1], port));
    let ours = spawn(move || {
        let started = Instant::now();
        loop {
            match try_run(config, server) {
                Err(error)
                    if config.transport == Transport::Tcp
                        && error.kind() == ErrorKind::ConnectionRefused
                        && started.elapsed() < SERVER_START =>
                {
                    thread::sleep(Duration::from_millis(10));
                }
                ended => return ended,
            }
        }
    })
    .get();
    let printed = stock.interrupt();
    match ours {
        Ok(end) => (end, printed),
        Err(error) => panic!("the client failed: {error}\nThe stock server printed:\n{printed}"),
    }
}

#[test]
#[ignore = "runs a stock iperf for about ten seconds; `cargo xtask check --push` runs it"]
fn stock_server_against_our_client() {
    for mode in modes() {
        let (stock, port) = stock_server(mode.transport);
        let (ours, printed) = run_against_stock(mode.config(), stock, port);
        assert_eq!(ours.outcome, Ok(()), "{mode:?}");
        agree(mode, !mode.reverse, &printed, &ours.last());
    }
}

#[test]
#[ignore = "runs a stock iperf for about ten seconds; `cargo xtask check --push` runs it"]
fn stock_client_against_our_server() {
    for mode in modes() {
        let (address, ours) = serve(server_config(mode.transport));
        let mut args = vec![
            "-c".to_owned(),
            "127.0.0.1".to_owned(),
            "-p".to_owned(),
            address.port().to_string(),
        ];
        args.extend(mode.client_args());
        let stock = Stock::start(&args);
        let ours = ours.get();
        let printed = stock.finish();
        assert_eq!(ours.outcome, Ok(()), "{mode:?}");
        agree(mode, mode.reverse, &printed, &ours.last());
    }
}
