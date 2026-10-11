//! The `std` driver: our client against our server on loopback, in each mode, and the rules that only a driver
//! keeps (P-14, D-4, D-5, D-6).

#![cfg(feature = "std")]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use embedded_iperf2::blocking::{TcpServer, UdpServer, run_client};
use embedded_iperf2::wire::{Amount, PREFIX_LEN, Prefix, TestHeader, Timestamp};
use embedded_iperf2::{Client, Config, Length, Report, Server, ServerConfig, TestError, Transport};
use std::io::{ErrorKind, Read};
use std::net::{SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

/// Every report an end gave, and how its test ended.
struct End {
    reports: Vec<Report>,
    outcome: Result<(), TestError>,
}

impl End {
    fn last(&self) -> Report {
        let last = *self.reports.last().expect("no report");
        assert!(last.final_report, "the last report is not final");
        last
    }

    fn intervals(&self) -> usize {
        self.reports
            .iter()
            .filter(|report| !report.final_report)
            .count()
    }
}

fn server_config(transport: Transport) -> ServerConfig {
    ServerConfig {
        transport,
        duration_us: None,
        interval_us: None,
        ipv6: false,
    }
}

/// The longest any end of a test here may take. A driver that never ends fails its test after this, rather than
/// hang it.
const PATIENCE: Duration = Duration::from_secs(5);

/// The result of work on another thread.
struct Pending<T>(mpsc::Receiver<T>);

impl<T> Pending<T> {
    /// The result, or a panic when the work takes longer than `PATIENCE` or panicked.
    fn get(self) -> T {
        self.0
            .recv_timeout(PATIENCE)
            .unwrap_or_else(|error| panic!("the work did not end: {error}"))
    }
}

fn spawn<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> Pending<T> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let _ = sender.send(work());
    });
    Pending(receiver)
}

/// Starts a server for one test on loopback, over IPv6 when `config` says so, and returns its address.
fn serve(config: ServerConfig) -> (SocketAddr, Pending<End>) {
    let host = if config.ipv6 {
        "[::1]:0"
    } else {
        "127.0.0.1:0"
    };
    match config.transport {
        Transport::Tcp => {
            let listener = TcpListener::bind(host).unwrap();
            let address = listener.local_addr().unwrap();
            let end = spawn(move || {
                let mut driver = TcpServer::new(listener);
                let mut buffer = vec![0; 4096];
                let mut server = Server::new(config, &mut buffer).unwrap();
                let mut reports = Vec::new();
                let outcome = driver
                    .run_one(&mut server, |report| reports.push(*report))
                    .unwrap();
                End { reports, outcome }
            });
            (address, end)
        }
        Transport::Udp => {
            let socket = UdpSocket::bind(host).unwrap();
            let address = socket.local_addr().unwrap();
            let end = spawn(move || {
                let mut driver = UdpServer::new(socket);
                let mut buffer = vec![0; 4096];
                let mut server = Server::new(config, &mut buffer).unwrap();
                let mut reports = Vec::new();
                let outcome = driver
                    .run_one(&mut server, |report| reports.push(*report))
                    .unwrap();
                End { reports, outcome }
            });
            (address, end)
        }
    }
}

/// Starts a client of `config` against `server`.
fn start(config: Config, server: SocketAddr) -> Pending<End> {
    spawn(move || {
        let mut buffer = vec![0; 4096];
        let mut client = Client::new(config, &mut buffer).unwrap();
        let mut reports = Vec::new();
        let outcome = run_client(&mut client, server, |report| reports.push(*report)).unwrap();
        End { reports, outcome }
    })
}

fn run(config: Config, server: SocketAddr) -> End {
    start(config, server).get()
}

fn config(transport: Transport, reverse: bool, length: Length) -> Config {
    let base = match transport {
        Transport::Tcp => Config::tcp(),
        Transport::Udp => Config::udp(),
    };
    Config {
        reverse,
        length,
        buf_len: if transport == Transport::Tcp {
            1000
        } else {
            512
        },
        rate: 4_096_000,
        interval_us: Some(100_000),
        ..base
    }
}

#[test]
fn every_mode_ends_with_both_reports_on_loopback() {
    for transport in [Transport::Tcp, Transport::Udp] {
        for reverse in [false, true] {
            for length in [Length::Bytes(51_200), Length::Time { us: 300_000 }] {
                let case = format!("{transport:?} reverse {reverse} {length:?}");
                let (address, server) = serve(server_config(transport));
                let started = Instant::now();
                let client = run(config(transport, reverse, length), address);
                let took = started.elapsed();
                let server = server.get();
                assert_eq!((client.outcome, server.outcome), (Ok(()), Ok(())), "{case}");
                let (sender, receiver) = if reverse {
                    (server.last(), client.last())
                } else {
                    (client.last(), server.last())
                };
                match transport {
                    Transport::Tcp => assert_eq!(receiver.bytes, sender.bytes, "{case}"),
                    Transport::Udp => {
                        let udp = receiver.udp.expect(&case);
                        assert_eq!((udp.lost, udp.out_of_order), (0, 0), "{case}");
                        assert_eq!(receiver.bytes, sender.bytes + 512, "{case}: the end counts");
                    }
                }
                match length {
                    // 51200 bytes take 100 ms at the rate. A client whose close did not reach the server would
                    // wait 2 s for the server to close (P-1).
                    Length::Bytes(count) => {
                        assert_eq!(sender.bytes, count, "{case}");
                        assert!(took < Duration::from_secs(1), "{case}: {took:?}");
                    }
                    // The clock is real time, so a test of 300 ms lasts that long and reports each 100 ms.
                    Length::Time { us } => {
                        assert!(sender.end_us >= us, "{case}: {} µs", sender.end_us);
                        assert!(sender.end_us < us + 200_000, "{case}: {} µs", sender.end_us);
                        assert!(client.intervals() >= 2, "{case}: {}", client.intervals());
                    }
                }
                if !reverse && transport == Transport::Udp {
                    let server_report = client.last().server.expect(&case);
                    assert_eq!(server_report.bytes, receiver.bytes, "{case}");
                }
            }
        }
    }
}

#[test]
fn a_tcp_client_ends_with_its_report_when_the_server_closes_while_it_sends() {
    let (address, server) = serve(ServerConfig {
        duration_us: Some(200_000),
        ..server_config(Transport::Tcp)
    });
    let started = Instant::now();
    let client = run(
        Config {
            rate: 0,
            length: Length::Time { us: 5_000_000 },
            ..Config::tcp()
        },
        address,
    );
    let server = server.get();
    assert_eq!(client.outcome, Ok(()));
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "{:?}",
        started.elapsed()
    );
    assert!(client.last().bytes >= server.last().bytes);
}

#[test]
fn a_tcp_server_closes_a_second_connection_while_a_test_runs() {
    let (address, server) = serve(server_config(Transport::Tcp));
    let client = start(
        Config {
            length: Length::Time { us: 1_500_000 },
            rate: 1_000_000,
            ..Config::tcp()
        },
        address,
    );
    thread::sleep(Duration::from_millis(200));
    let mut second = TcpStream::connect(address).unwrap();
    // The test runs for a second more, so only the server's refusal can end this read in time.
    second
        .set_read_timeout(Some(Duration::from_millis(400)))
        .unwrap();
    let mut byte = [0];
    match second.read(&mut byte) {
        Ok(count) => assert_eq!(count, 0, "the server sent to the second connection"),
        Err(error) => assert_eq!(error.kind(), ErrorKind::ConnectionReset, "{error}"),
    }
    assert_eq!(client.get().outcome, Ok(()));
    assert_eq!(server.get().outcome, Ok(()));
}

/// The first datagram of a UDP test with `num_threads` streams, which a server refuses unless it is 1 (P-13).
fn first_datagram(num_threads: u32) -> Vec<u8> {
    let mut datagram = vec![0; 512];
    Prefix {
        packet_id: 1,
        sent: Timestamp { sec: 0, usec: 0 },
    }
    .encode(&mut datagram)
    .unwrap();
    TestHeader {
        flags: 0x4801_00A0,
        num_threads,
        port: 5001,
        buf_len: 512,
        amount: Amount::Time { centiseconds: 100 },
        upper_flags: 0,
        lower_flags: 0,
        version_u: 0x0002_0002,
        version_l: 0x0002_0003,
        rate: 1_048_576,
    }
    .encode(&mut datagram[PREFIX_LEN..])
    .unwrap();
    datagram
}

#[test]
fn a_udp_server_ignores_datagrams_from_another_peer_during_a_test() {
    let (address, server) = serve(server_config(Transport::Udp));
    let stranger = UdpSocket::bind("127.0.0.1:0").unwrap();
    let client = start(
        Config {
            length: Length::Time { us: 400_000 },
            buf_len: 512,
            rate: 1_000_000,
            ..Config::udp()
        },
        address,
    );
    thread::sleep(Duration::from_millis(100));
    for _ in 0..20 {
        stranger.send_to(&first_datagram(1), address).unwrap();
    }
    let client = client.get();
    let server = server.get();
    assert_eq!(server.outcome, Ok(()));
    assert_eq!(server.last().bytes, client.last().bytes + 512);
    assert_eq!(server.last().udp.unwrap().lost, 0);
}

#[test]
fn a_udp_server_drops_a_refused_flow_and_serves_the_next_test() {
    let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
    let address = socket.local_addr().unwrap();
    // The refused flow sends on. The first server takes only its first datagram. Were the rest passed to the
    // second server, they would end that test as refused before the client's first datagram arrives.
    let refused = UdpSocket::bind("127.0.0.1:0").unwrap();
    for _ in 0..6 {
        refused.send_to(&first_datagram(2), address).unwrap();
    }
    let servers = spawn(move || {
        let mut driver = UdpServer::new(socket);
        let mut buffer = vec![0; 4096];
        let mut server = Server::new(server_config(Transport::Udp), &mut buffer).unwrap();
        let first = driver.run_one(&mut server, |_| {}).unwrap();
        let mut server = Server::new(server_config(Transport::Udp), &mut buffer).unwrap();
        let mut reports = Vec::new();
        let second = driver
            .run_one(&mut server, |report| reports.push(*report))
            .unwrap();
        (
            first,
            End {
                reports,
                outcome: second,
            },
        )
    });
    let client = start(
        Config {
            length: Length::Bytes(5_120),
            buf_len: 512,
            ..Config::udp()
        },
        address,
    );
    let (first, second) = servers.get();
    assert_eq!(first, Err(TestError::Refused));
    assert_eq!(second.outcome, Ok(()));
    assert_eq!(second.last().bytes, 5_120 + 512);
    assert_eq!(client.get().outcome, Ok(()));
}

#[test]
fn a_reverse_udp_test_over_ipv6_sends_datagrams_of_1450_bytes() {
    let (address, server) = serve(ServerConfig {
        ipv6: true,
        ..server_config(Transport::Udp)
    });
    let client = run(
        Config {
            reverse: true,
            length: Length::Bytes(14_500),
            buf_len: 1470,
            ..Config::udp()
        },
        address,
    );
    assert_eq!(server.get().outcome, Ok(()));
    let last = client.last();
    let udp = last.udp.unwrap();
    assert_eq!(last.bytes, udp.datagrams * 1450);
}

#[test]
fn a_server_refuses_settings_that_do_not_match_its_socket() {
    // A driver that took the settings would wait for a peer, so each check runs on its own thread.
    let refusal =
        |check: fn(&mut [u8]) -> std::io::Error| spawn(move || check(&mut [0; 4096]).kind()).get();
    let udp_on_tcp = |buffer: &mut [u8]| {
        let mut udp = Server::new(server_config(Transport::Udp), buffer).unwrap();
        let mut driver = TcpServer::new(TcpListener::bind("127.0.0.1:0").unwrap());
        driver.run_one(&mut udp, |_| {}).unwrap_err()
    };
    let ipv6_on_ipv4 = |buffer: &mut [u8]| {
        let config = ServerConfig {
            ipv6: true,
            ..server_config(Transport::Udp)
        };
        let mut ipv6 = Server::new(config, buffer).unwrap();
        let mut driver = UdpServer::new(UdpSocket::bind("127.0.0.1:0").unwrap());
        driver.run_one(&mut ipv6, |_| {}).unwrap_err()
    };
    let tcp_on_udp = |buffer: &mut [u8]| {
        let mut tcp = Server::new(server_config(Transport::Tcp), buffer).unwrap();
        let mut driver = UdpServer::new(UdpSocket::bind("127.0.0.1:0").unwrap());
        driver.run_one(&mut tcp, |_| {}).unwrap_err()
    };
    assert_eq!(refusal(udp_on_tcp), ErrorKind::InvalidInput);
    assert_eq!(refusal(ipv6_on_ipv4), ErrorKind::InvalidInput);
    assert_eq!(refusal(tcp_on_udp), ErrorKind::InvalidInput);
}
