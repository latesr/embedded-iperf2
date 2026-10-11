//! T-5: the `std` driver against a stock iperf 2.2.2 on loopback. Our client runs against a stock server, and a
//! stock client against our server, over TCP and UDP, forward and reverse, in time and byte mode.
//!
//! Each test runs the binary that `MCU_IPERF2` names, which `cargo xtask install-iperf` builds, and fails without
//! it. Each test's name starts with `stock_`, so that `.cargo/mutants.toml` can leave these tests out of mutation
//! runs. The tests in `blocking.rs` cover the driver's code.

#![cfg(feature = "std")]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use embedded_iperf2::blocking::{TcpServer, UdpServer, run_client};
use embedded_iperf2::{Client, Config, Length, Report, Server, ServerConfig, Transport};
use std::net::{SocketAddr, TcpListener, UdpSocket};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

/// The longest our end of one test may take.
const PATIENCE: Duration = Duration::from_secs(15);
/// The time a stock server gets to open its socket before our client starts.
const SERVER_START: Duration = Duration::from_millis(500);
/// The time a stock client gets to exit after our server ends the test.
const CLIENT_EXIT: Duration = Duration::from_secs(3);
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

/// The stock binary, once it reports 2.2.2.
fn binary() -> String {
    let binary = std::env::var("MCU_IPERF2").expect(
        "MCU_IPERF2 must name an iperf 2.2.2 binary, which `cargo xtask install-iperf` builds",
    );
    let output = Command::new(&binary).arg("-v").output().unwrap();
    let version = String::from_utf8_lossy(&[output.stdout, output.stderr].concat()).into_owned();
    assert!(
        version.starts_with("iperf 2.2.2 "),
        "{binary} reports {version}"
    );
    binary
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

    /// Stops the process with SIGINT, so it flushes its reports, and returns what it printed.
    fn interrupt(mut self) -> String {
        let child = self.0.take().unwrap();
        let status = Command::new("kill")
            .args(["-INT", &child.id().to_string()])
            .status()
            .unwrap();
        assert!(status.success(), "kill -INT failed");
        let output = child.wait_with_output().unwrap();
        String::from_utf8_lossy(&[output.stdout, output.stderr].concat()).into_owned()
    }

    /// Waits up to `CLIENT_EXIT` for the process to exit, then interrupts it, and returns what it printed.
    fn finish(mut self) -> String {
        let started = Instant::now();
        while started.elapsed() < CLIENT_EXIT {
            if self.0.as_mut().unwrap().try_wait().unwrap().is_some() {
                let output = self.0.take().unwrap().wait_with_output().unwrap();
                return String::from_utf8_lossy(&[output.stdout, output.stderr].concat())
                    .into_owned();
            }
            thread::sleep(Duration::from_millis(20));
        }
        self.interrupt()
    }
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

/// Reads the last report line of `output`. A TCP line has 9 fields, the bytes in the eighth. A UDP line adds the
/// jitter, the lost datagrams, the total, the percent lost and the out-of-order datagrams.
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

/// Checks that the stock end's last report and our final report agree on what the receiver counted.
fn agree(mode: Mode, ours_sends: bool, printed: &str, ours: &Report) {
    let stock = last_report(printed);
    let case = format!("{mode:?}, ours sends {ours_sends}:\n{printed}");
    match (mode.transport, ours_sends) {
        (Transport::Tcp, _) => assert_eq!(stock.bytes, ours.bytes, "{case}"),
        (Transport::Udp, false) => {
            let udp = ours.udp.expect(&case);
            let ours = Counts {
                bytes: ours.bytes,
                udp: Some((udp.lost, udp.out_of_order)),
            };
            assert_eq!(stock, ours, "{case}");
        }
        (Transport::Udp, true) => {
            // The receiver counts the end of the test as a datagram of data.
            assert_eq!(
                stock.bytes,
                ours.bytes + u64::from(mode.buf_len()),
                "{case}"
            );
            assert_eq!(stock.udp, Some((0, 0)), "{case}");
            // A forward client holds the server report. That report is the receiver's own count.
            if let Some(server) = ours.server {
                let reported = Counts {
                    bytes: server.bytes,
                    udp: Some((server.lost, server.out_of_order)),
                };
                assert_eq!(stock, reported, "{case}");
            }
        }
    }
}

/// Runs `work` on its own thread, and panics when it takes longer than `PATIENCE`.
fn within<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> T {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let _ = sender.send(work());
    });
    receiver
        .recv_timeout(PATIENCE)
        .unwrap_or_else(|error| panic!("our end did not finish: {error}"))
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

#[test]
fn stock_server_against_our_client() {
    for mode in modes() {
        let port = free_port(mode.transport);
        let mut args = vec!["-s".to_owned(), "-p".to_owned(), port.to_string()];
        if mode.transport == Transport::Udp {
            args.push("-u".to_owned());
        }
        let stock = Stock::start(&args);
        thread::sleep(SERVER_START);
        let server = SocketAddr::from(([127, 0, 0, 1], port));
        let (outcome, last) = within(move || {
            let mut buffer = vec![0; 4096];
            let mut client = Client::new(mode.config(), &mut buffer).unwrap();
            let mut last = None;
            let outcome = run_client(&mut client, server, |report| last = Some(*report)).unwrap();
            (outcome, last)
        });
        let printed = stock.interrupt();
        assert_eq!(outcome, Ok(()), "{mode:?}");
        agree(mode, !mode.reverse, &printed, &last.unwrap());
    }
}

#[test]
fn stock_client_against_our_server() {
    for mode in modes() {
        let config = ServerConfig {
            transport: mode.transport,
            duration_us: None,
            interval_us: None,
            ipv6: false,
        };
        let (port, ours) = match mode.transport {
            Transport::Tcp => {
                let listener = TcpListener::bind("127.0.0.1:0").unwrap();
                let port = listener.local_addr().unwrap().port();
                let mut driver = TcpServer::new(listener);
                (
                    port,
                    Box::new(
                        move |server: &mut Server<'_>, report: &mut dyn FnMut(&Report)| {
                            driver.run_one(server, report).unwrap()
                        },
                    )
                        as Box<dyn FnOnce(&mut Server<'_>, &mut dyn FnMut(&Report)) -> _ + Send>,
                )
            }
            Transport::Udp => {
                let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
                let port = socket.local_addr().unwrap().port();
                let mut driver = UdpServer::new(socket);
                (
                    port,
                    Box::new(
                        move |server: &mut Server<'_>, report: &mut dyn FnMut(&Report)| {
                            driver.run_one(server, report).unwrap()
                        },
                    )
                        as Box<dyn FnOnce(&mut Server<'_>, &mut dyn FnMut(&Report)) -> _ + Send>,
                )
            }
        };
        let ours = thread::spawn(move || {
            let mut buffer = vec![0; 4096];
            let mut server = Server::new(config, &mut buffer).unwrap();
            let mut last = None;
            let outcome = ours(&mut server, &mut |report| last = Some(*report));
            (outcome, last)
        });
        let mut args = vec![
            "-c".to_owned(),
            "127.0.0.1".to_owned(),
            "-p".to_owned(),
            port.to_string(),
        ];
        args.extend(mode.client_args());
        let stock = Stock::start(&args);
        let (outcome, last) = within(move || ours.join().unwrap());
        // A stock client of a reverse UDP test in byte mode prints its report and then never exits, so `finish`
        // interrupts it.
        let printed = stock.finish();
        assert_eq!(outcome, Ok(()), "{mode:?}");
        agree(mode, mode.reverse, &printed, &last.unwrap());
    }
}
