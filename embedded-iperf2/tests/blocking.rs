//! The `std` driver: our client against our server on loopback, in each mode, and the rules that only a driver
//! keeps (P-14, D-4, D-5, D-6, D-7).

#![cfg(feature = "std")]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

mod loopback;

use embedded_iperf2::blocking::{TcpServer, UdpServer};
use embedded_iperf2::wire::{Amount, PREFIX_LEN, Prefix, TestHeader, Timestamp};
use embedded_iperf2::{Config, Length, Report, Server, ServerConfig, TestError, Transport};
use loopback::{End, run, serve, server_config, spawn, start};
use std::io::{ErrorKind, Read};
use std::net::{SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::thread;
use std::time::{Duration, Instant};

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
                        assert_eq!(receiver.bytes, sender.bytes, "{case}: both count the end");
                    }
                }
                match length {
                    // 51200 bytes take 100 ms at the rate. A client whose close did not reach the server would
                    // wait 2 s for the server to close (P-1).
                    Length::Bytes(count) => {
                        let end = if transport == Transport::Udp { 512 } else { 0 };
                        assert_eq!(sender.bytes, count + end, "{case}");
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
    // A receiving server waits, and reads in each wait. A server that sends without a rate never waits, so only
    // the read before each poll finds the second connection.
    for (reverse, rate) in [(false, 1_000_000), (true, 0)] {
        let case = format!("reverse {reverse}, rate {rate}");
        let (address, server) = serve(server_config(Transport::Tcp));
        let client = start(
            Config {
                reverse,
                rate,
                length: Length::Time { us: 1_500_000 },
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
            Ok(count) => assert_eq!(count, 0, "{case}: the server sent to the second connection"),
            Err(error) => assert_eq!(error.kind(), ErrorKind::ConnectionReset, "{case}: {error}"),
        }
        assert_eq!(client.get().outcome, Ok(()), "{case}");
        assert_eq!(server.get().outcome, Ok(()), "{case}");
    }
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
    assert_eq!(server.last().bytes, client.last().bytes);
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

/// Runs `count` UDP tests in a row on `socket`, and returns each end with the time its `run_one` returned.
fn serve_in_a_row(socket: UdpSocket, count: usize) -> loopback::Pending<Vec<(End, Instant)>> {
    spawn(move || {
        let mut driver = UdpServer::new(socket);
        let mut buffer = vec![0; 4096];
        let mut ends = Vec::new();
        for _ in 0..count {
            let mut server = Server::new(server_config(Transport::Udp), &mut buffer).unwrap();
            let mut reports = Vec::new();
            let outcome = driver
                .run_one(&mut server, |report| reports.push(*report))
                .unwrap();
            ends.push((End { reports, outcome }, Instant::now()));
        }
        ends
    })
}

fn udp_bytes() -> Config {
    Config {
        length: Length::Bytes(5_120),
        buf_len: 512,
        ..Config::udp()
    }
}

/// The end of the test, as a stock client's late repeat of it, from an old test.
fn old_end() -> Vec<u8> {
    let mut datagram = first_datagram(1);
    Prefix {
        packet_id: -11,
        sent: Timestamp { sec: 0, usec: 0 },
    }
    .encode(&mut datagram)
    .unwrap();
    datagram
}

#[test]
fn a_udp_test_that_starts_during_the_last_report_wait_loses_nothing() {
    let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
    let address = socket.local_addr().unwrap();
    let servers = serve_in_a_row(socket, 2);
    let first = run(udp_bytes(), address);
    let second = run(udp_bytes(), address);
    let ends = servers.get();
    assert_eq!((first.outcome, second.outcome), (Ok(()), Ok(())));
    let served = ends[1].0.last();
    assert_eq!(served.bytes, 5_120 + 512, "(P-12, P-14)");
    assert_eq!(served.udp.unwrap().lost, 0);
    assert!(second.last().server.is_some());
}

#[test]
fn an_old_end_from_another_port_does_not_take_the_server() {
    let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
    let address = socket.local_addr().unwrap();
    let stray = UdpSocket::bind("127.0.0.1:0").unwrap();
    stray.send_to(&old_end(), address).unwrap();
    let servers = serve_in_a_row(socket, 2);
    let first = run(udp_bytes(), address);
    stray.send_to(&old_end(), address).unwrap();
    let second = run(udp_bytes(), address);
    let ends = servers.get();
    assert_eq!((first.outcome, second.outcome), (Ok(()), Ok(())));
    for (end, _) in &ends {
        assert_eq!(end.last().bytes, 5_120 + 512, "(D-6)");
    }
}

#[test]
fn a_refused_flow_does_not_end_the_report_wait() {
    let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
    let address = socket.local_addr().unwrap();
    let refused = UdpSocket::bind("127.0.0.1:0").unwrap();
    refused.send_to(&first_datagram(2), address).unwrap();
    let servers = serve_in_a_row(socket, 2);
    let client = run(udp_bytes(), address);
    let reported = Instant::now();
    refused.send_to(&first_datagram(2), address).unwrap();
    let ends = servers.get();
    assert_eq!(ends[0].0.outcome, Err(TestError::Refused));
    assert_eq!((client.outcome, ends[1].0.outcome), (Ok(()), Ok(())));
    let waited = ends[1].1.duration_since(reported);
    assert!(waited >= Duration::from_millis(200), "waited {waited:?}");
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

/// Each `run_one` of a server with a start timeout: its test, or the kind of its error, and how long it took.
type Timed = Vec<(Result<End, ErrorKind>, Duration)>;

/// Runs `count` tests in a row on a server of `transport` whose start timeout is `timeout`.
fn serve_with_start_timeout(
    transport: Transport,
    timeout: Duration,
    count: usize,
) -> (SocketAddr, loopback::Pending<Timed>) {
    enum Driver {
        Tcp(TcpServer),
        Udp(UdpServer),
    }
    let (address, mut driver) = match transport {
        Transport::Tcp => {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            (
                listener.local_addr().unwrap(),
                Driver::Tcp(TcpServer::new(listener)),
            )
        }
        Transport::Udp => {
            let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
            (
                socket.local_addr().unwrap(),
                Driver::Udp(UdpServer::new(socket)),
            )
        }
    };
    let ends = spawn(move || {
        match &mut driver {
            Driver::Tcp(driver) => driver.set_start_timeout(Some(timeout)).unwrap(),
            Driver::Udp(driver) => driver.set_start_timeout(Some(timeout)).unwrap(),
        }
        let mut buffer = vec![0; 4096];
        (0..count)
            .map(|_| {
                let mut server = Server::new(server_config(transport), &mut buffer).unwrap();
                let mut reports = Vec::new();
                let on_report = |report: &Report| reports.push(*report);
                let started = Instant::now();
                let outcome = match &mut driver {
                    Driver::Tcp(driver) => driver.run_one(&mut server, on_report),
                    Driver::Udp(driver) => driver.run_one(&mut server, on_report),
                };
                let took = started.elapsed();
                let end = outcome
                    .map(|outcome| End { reports, outcome })
                    .map_err(|error| error.kind());
                (end, took)
            })
            .collect()
    });
    (address, ends)
}

#[test]
fn a_server_without_a_client_gives_up_at_its_start_timeout() {
    for transport in [Transport::Tcp, Transport::Udp] {
        let (_, ends) = serve_with_start_timeout(transport, Duration::from_millis(200), 1);
        let (end, took) = ends.get().pop().unwrap();
        assert_eq!(end.err(), Some(ErrorKind::TimedOut), "{transport:?}");
        assert!(
            took >= Duration::from_millis(200),
            "{transport:?}: {took:?}"
        );
        assert!(took < Duration::from_secs(1), "{transport:?}: {took:?}");
    }
}

#[test]
fn a_test_that_starts_within_the_start_timeout_runs() {
    for transport in [Transport::Tcp, Transport::Udp] {
        let (address, ends) = serve_with_start_timeout(transport, Duration::from_secs(2), 1);
        let client = run(config(transport, false, Length::Bytes(51_200)), address);
        let (end, _) = ends.get().pop().unwrap();
        let served = end.unwrap();
        assert_eq!(
            (client.outcome, served.outcome),
            (Ok(()), Ok(())),
            "{transport:?}"
        );
        assert_eq!(served.last().bytes, client.last().bytes, "{transport:?}");
    }
}

#[test]
fn a_server_runs_the_next_test_after_a_start_timeout() {
    for transport in [Transport::Tcp, Transport::Udp] {
        let (address, ends) = serve_with_start_timeout(transport, Duration::from_millis(400), 2);
        thread::sleep(Duration::from_millis(600));
        let client = run(config(transport, false, Length::Bytes(51_200)), address);
        let ends = ends.get();
        assert_eq!(
            ends[0].0.as_ref().err(),
            Some(&ErrorKind::TimedOut),
            "{transport:?}"
        );
        let served = ends[1].0.as_ref().unwrap();
        assert_eq!(
            (client.outcome, served.outcome),
            (Ok(()), Ok(())),
            "{transport:?}"
        );
        assert_eq!(served.last().bytes, client.last().bytes, "{transport:?}");
    }
}

#[test]
fn old_ends_do_not_hold_off_a_udp_start_timeout() {
    let (address, ends) = serve_with_start_timeout(Transport::Udp, Duration::from_millis(300), 1);
    let stray = UdpSocket::bind("127.0.0.1:0").unwrap();
    let started = Instant::now();
    while started.elapsed() < Duration::from_millis(600) {
        stray.send_to(&old_end(), address).unwrap();
        thread::sleep(Duration::from_millis(20));
    }
    let (end, took) = ends.get().pop().unwrap();
    assert_eq!(end.err(), Some(ErrorKind::TimedOut));
    assert!(took < Duration::from_millis(500), "{took:?}");
}

#[test]
fn a_zero_start_timeout_is_invalid() {
    let mut tcp = TcpServer::new(TcpListener::bind("127.0.0.1:0").unwrap());
    let mut udp = UdpServer::new(UdpSocket::bind("127.0.0.1:0").unwrap());
    let zero = Some(Duration::ZERO);
    assert_eq!(
        tcp.set_start_timeout(zero).unwrap_err().kind(),
        ErrorKind::InvalidInput
    );
    assert_eq!(
        udp.set_start_timeout(zero).unwrap_err().kind(),
        ErrorKind::InvalidInput
    );
}
