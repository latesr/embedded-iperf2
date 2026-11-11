//! Tests the crate's client against the crate's server in memory.
//!
//! The tests cover TCP and UDP, forward and reverse, in time and byte mode. A loop moves the bytes between the two
//! ends. It moves a shared clock to the earliest time that either end waits for.
//!
//! The loop polls each end in turn, so it makes only one order of events. A behaviour that depends on the order
//! of events must have a test that makes that order.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use embedded_iperf2::{
    Action, Client, Config, Length, Report, Server, ServerConfig, TestError, Transport,
};

/// What one end did on one poll.
enum Step {
    Bytes(Vec<u8>),
    Wait(Option<u64>),
    Report(Report),
    Close,
    Done(Result<(), TestError>),
}

fn step(action: Action<'_>) -> Step {
    match action {
        Action::Send(bytes) => Step::Bytes(bytes.to_vec()),
        Action::Wait { until_us } => Step::Wait(until_us),
        Action::Report(report) => Step::Report(report),
        Action::Close => Step::Close,
        Action::Done(result) => Step::Done(result),
    }
}

/// The final report and the outcome of each end.
struct Ends {
    client: (Option<Report>, Result<(), TestError>),
    server: (Option<Report>, Result<(), TestError>),
}

fn run(config: Config) -> Ends {
    let mut client_buffer = [0; 4096];
    let mut server_buffer = [0; 4096];
    let mut client = Client::new(config, &mut client_buffer).unwrap();
    let server_config = ServerConfig {
        transport: config.transport,
        duration_us: None,
        interval_us: None,
        ipv6: false,
    };
    let mut server = Server::new(server_config, &mut server_buffer).unwrap();
    let mut now = 0;
    let (mut client_end, mut server_end) = (None, None);
    let (mut client_last, mut server_last) = (None, None);
    for _ in 0..1_000_000 {
        let mut moved = false;
        let mut waits = Vec::new();
        if client_end.is_none() {
            match step(client.poll(now)) {
                Step::Bytes(bytes) => {
                    client.sent(bytes.len());
                    server.received(now, &bytes);
                    moved = true;
                }
                Step::Wait(until) => waits.push(until),
                Step::Report(report) => {
                    if report.final_report {
                        client_last = Some(report);
                    }
                    moved = true;
                }
                Step::Close => {
                    server.closed(now);
                    moved = true;
                }
                Step::Done(result) => {
                    client_end = Some(result);
                    if config.transport == Transport::Tcp {
                        server.closed(now);
                    }
                    moved = true;
                }
            }
        }
        if server_end.is_none() {
            match step(server.poll(now)) {
                Step::Bytes(bytes) => {
                    server.sent(bytes.len());
                    if client_end.is_none() {
                        client.received(now, &bytes);
                    }
                    moved = true;
                }
                Step::Wait(until) => waits.push(until),
                Step::Report(report) => {
                    if report.final_report {
                        server_last = Some(report);
                    }
                    moved = true;
                }
                Step::Close => {
                    client.closed(now);
                    moved = true;
                }
                Step::Done(result) => {
                    server_end = Some(result);
                    if config.transport == Transport::Tcp && client_end.is_none() {
                        client.closed(now);
                    }
                    moved = true;
                }
            }
        }
        if let (Some(client), Some(server)) = (client_end, server_end) {
            return Ends {
                client: (client_last, client),
                server: (server_last, server),
            };
        }
        if !moved {
            now = waits
                .into_iter()
                .flatten()
                .min()
                .unwrap_or_else(|| panic!("both ends wait for each other at {now}"));
        }
    }
    panic!("the test did not end");
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
        ..base
    }
}

#[test]
fn every_mode_ends_with_both_reports() {
    for transport in [Transport::Tcp, Transport::Udp] {
        for reverse in [false, true] {
            for length in [Length::Bytes(51_200), Length::Time { us: 100_000 }] {
                let ends = run(config(transport, reverse, length));
                let case = format!("{transport:?} reverse {reverse} {length:?}");
                assert_eq!((ends.client.1, ends.server.1), (Ok(()), Ok(())), "{case}");
                let client = ends.client.0.expect(&case);
                let server = ends.server.0.expect(&case);
                let (sender, receiver) = if reverse {
                    (server, client)
                } else {
                    (client, server)
                };
                match (transport, length) {
                    (Transport::Tcp, _) => assert_eq!(receiver.bytes, sender.bytes, "{case}"),
                    (Transport::Udp, _) => {
                        let udp = receiver.udp.expect(&case);
                        assert_eq!((udp.lost, udp.out_of_order), (0, 0), "{case}");
                        assert_eq!(udp.datagrams * 512, receiver.bytes, "{case}");
                        assert_eq!(receiver.bytes, sender.bytes, "{case}: both count the end");
                    }
                }
                if let Length::Bytes(count) = length {
                    let end = if transport == Transport::Udp { 512 } else { 0 };
                    assert_eq!(sender.bytes, count + end, "{case}");
                }
                if !reverse && transport == Transport::Udp {
                    let server_report = client.server.expect(&case);
                    assert_eq!(server_report.bytes, server.bytes, "{case}");
                }
            }
        }
    }
}
