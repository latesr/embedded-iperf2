//! T-6: the `embassy-net` driver, our client against our server over two embassy-net stacks in one process, in
//! each mode, and the rules that only a driver keeps (P-14, P-24, D-4, D-6).

#![cfg(feature = "embassy-net")]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

mod stacks;

use embassy_futures::join::{join, join3};
use embassy_net::tcp::ConnectError;
use embassy_time::{Duration, Instant, Timer};
use embedded_iperf2::embassy::{Error, TcpServer, UdpServer, run_tcp_client, run_udp_client};
use embedded_iperf2::wire::{Amount, PREFIX_LEN, Prefix, TestHeader, Timestamp};
use embedded_iperf2::{Client, Config, Length, Server, ServerConfig, TestError, Transport};
use stacks::{
    PORT, SERVER, TcpBuffers, UdpBuffers, client, run, serve, server_config, since, tcp_client,
    tcp_once, udp_client, udp_once,
};

/// The RST bit of a TCP header's flags.
const RST: u8 = 0x04;

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
fn every_mode_ends_with_both_reports_over_two_stacks() {
    for transport in [Transport::Tcp, Transport::Udp] {
        for reverse in [false, true] {
            for length in [Length::Bytes(51_200), Length::Time { us: 300_000 }] {
                let case = format!("{transport:?} reverse {reverse} {length:?}");
                run(async move |net| {
                    let started = Instant::now();
                    let (server, client) = join(
                        serve(net, server_config(transport)),
                        client(net, config(transport, reverse, length)),
                    )
                    .await;
                    let took = since(started);
                    assert_eq!(
                        (client.outcome, server.outcome),
                        (Ok(Ok(())), Ok(Ok(()))),
                        "{case}"
                    );
                    let (sender, receiver) = if reverse {
                        (server.last(), client.last())
                    } else {
                        (client.last(), server.last())
                    };
                    assert_eq!(receiver.bytes, sender.bytes, "{case}");
                    // Each run starts with empty ARP caches. smoltcp keeps the first datagram in the socket's
                    // buffer while ARP resolves, so nothing is lost.
                    if transport == Transport::Udp {
                        let udp = receiver.udp.expect(&case);
                        assert_eq!((udp.lost, udp.out_of_order), (0, 0), "{case}");
                    }
                    match length {
                        // 51200 bytes take 100 ms at the rate. An end that missed its peer's close would wait
                        // 2 s for it (P-1).
                        Length::Bytes(count) => {
                            let end = if transport == Transport::Udp { 512 } else { 0 };
                            assert_eq!(sender.bytes, count + end, "{case}");
                            assert!(took < Duration::from_secs(1), "{case}: {took:?}");
                        }
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
                });
            }
        }
    }
}

#[test]
fn a_server_that_waits_without_a_deadline_ends_when_the_client_closes() {
    // A TCP server without a duration or interval waits for bytes without a deadline, so only the client's close
    // ends its wait.
    run(async |net| {
        let started = Instant::now();
        let (server, client) = join(
            serve(net, server_config(Transport::Tcp)),
            client(
                net,
                Config {
                    length: Length::Bytes(10_000),
                    ..Config::tcp()
                },
            ),
        )
        .await;
        assert_eq!(server.outcome, Ok(Ok(())));
        assert_eq!(server.last().bytes, 10_000);
        assert_eq!(client.outcome, Ok(Ok(())));
        assert!(
            since(started) < Duration::from_secs(1),
            "{:?}",
            since(started)
        );
    });
}

#[test]
fn a_tcp_client_ends_with_its_report_when_the_server_closes_while_it_sends() {
    run(async |net| {
        let started = Instant::now();
        let (server, client) = join(
            serve(
                net,
                ServerConfig {
                    duration_us: Some(200_000),
                    ..server_config(Transport::Tcp)
                },
            ),
            client(
                net,
                Config {
                    rate: 0,
                    length: Length::Time { us: 5_000_000 },
                    ..Config::tcp()
                },
            ),
        )
        .await;
        assert_eq!(client.outcome, Ok(Ok(())));
        // The server closes first, and its socket is dropped as soon as its test ends. A client that read only when
        // its writes failed would see the close only from the server's RST, 1 s after the server's FIN. A server
        // whose socket went before the stack sent its last ACK would leave the client waiting 1 s for it.
        assert!(
            since(started) < Duration::from_millis(700),
            "{:?}",
            since(started)
        );
        assert!(client.last().bytes >= server.last().bytes);
    });
}

#[test]
fn a_tcp_server_refuses_a_second_connection_while_a_test_runs() {
    // A server that sends without a rate never waits, so the refusal must not depend on the driver reading.
    for (reverse, rate) in [(false, 1_000_000), (true, 0)] {
        let case = format!("reverse {reverse}, rate {rate}");
        run(async move |net| {
            let second = async {
                Timer::after(Duration::from_millis(100)).await;
                let mut buffers = TcpBuffers::new();
                let mut socket = buffers.socket(net.client);
                socket.connect((SERVER, PORT)).await
            };
            let (server, client, second) = join3(
                serve(net, server_config(Transport::Tcp)),
                client(
                    net,
                    Config {
                        reverse,
                        rate,
                        length: Length::Time { us: 400_000 },
                        ..Config::tcp()
                    },
                ),
                second,
            )
            .await;
            assert_eq!(second, Err(ConnectError::ConnectionReset), "{case}");
            assert_eq!(client.outcome, Ok(Ok(())), "{case}");
            assert_eq!(server.outcome, Ok(Ok(())), "{case}");
        });
    }
}

#[test]
fn a_tcp_server_and_client_each_run_two_tests_on_one_socket_without_a_reset() {
    run(async |net| {
        let mut server_buffers = TcpBuffers::new();
        let mut driver = TcpServer::new(server_buffers.socket(net.server), PORT);
        let mut client_buffers = TcpBuffers::new();
        let mut socket = client_buffers.socket(net.client);
        let test = Config {
            length: Length::Bytes(20_000),
            ..Config::tcp()
        };
        for round in 0..2 {
            let (server, client) = join(
                tcp_once(&mut driver, server_config(Transport::Tcp)),
                tcp_client(&mut socket, test),
            )
            .await;
            assert_eq!(server.outcome, Ok(Ok(())), "round {round}");
            assert_eq!(client.outcome, Ok(Ok(())), "round {round}");
            assert_eq!(server.last().bytes, 20_000, "round {round}");
        }
        let resets: Vec<_> = net
            .tcp_flags()
            .into_iter()
            .filter(|(_, flags)| flags & RST != 0)
            .collect();
        assert_eq!(resets, Vec::new());
    });
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
    run(async |net| {
        let stranger = async {
            Timer::after(Duration::from_millis(100)).await;
            let mut buffers = UdpBuffers::new();
            let socket = buffers.bound(net.client, 0);
            for _ in 0..20 {
                socket
                    .send_to(&first_datagram(1), (SERVER, PORT))
                    .await
                    .unwrap();
            }
        };
        let (server, client, ()) = join3(
            serve(net, server_config(Transport::Udp)),
            client(
                net,
                Config {
                    length: Length::Time { us: 400_000 },
                    buf_len: 512,
                    rate: 1_000_000,
                    ..Config::udp()
                },
            ),
            stranger,
        )
        .await;
        assert_eq!(server.outcome, Ok(Ok(())));
        assert_eq!(server.last().bytes, client.last().bytes);
        assert_eq!(server.last().udp.unwrap().lost, 0);
    });
}

#[test]
fn a_udp_server_drops_a_refused_flow_and_serves_the_next_test() {
    run(async |net| {
        let mut server_buffers = UdpBuffers::new();
        let mut driver = UdpServer::new(server_buffers.bound(net.server, PORT));
        // The refused flow sends on. The first test takes only its first datagram. Were the rest passed to the
        // second test, they would end it as refused before the client's first datagram arrives.
        let mut refused_buffers = UdpBuffers::new();
        let refused = refused_buffers.bound(net.client, 0);
        for _ in 0..6 {
            refused
                .send_to(&first_datagram(2), (SERVER, PORT))
                .await
                .unwrap();
        }
        let servers = async {
            let first = udp_once(&mut driver, server_config(Transport::Udp)).await;
            let second = udp_once(&mut driver, server_config(Transport::Udp)).await;
            (first, second)
        };
        let client = async {
            Timer::after(Duration::from_millis(100)).await;
            client(
                net,
                Config {
                    length: Length::Bytes(5_120),
                    buf_len: 512,
                    ..Config::udp()
                },
            )
            .await
        };
        let ((first, second), client) = join(servers, client).await;
        assert_eq!(first.outcome, Ok(Err(TestError::Refused)));
        assert_eq!(second.outcome, Ok(Ok(())));
        assert_eq!(second.last().bytes, 5_120 + 512);
        assert_eq!(client.outcome, Ok(Ok(())));
    });
}

#[test]
fn a_driver_refuses_a_socket_or_settings_that_do_not_fit_the_test() {
    run(async |net| {
        let mut buffer = vec![0; 4096];
        let ipv6 = ServerConfig {
            ipv6: true,
            ..server_config(Transport::Udp)
        };
        for (config, transport) in [
            (server_config(Transport::Udp), Transport::Tcp),
            (server_config(Transport::Tcp), Transport::Udp),
            (ipv6, Transport::Udp),
        ] {
            let case = format!("{config:?} on {transport:?}");
            let mut server = Server::new(config, &mut buffer).unwrap();
            let outcome = match transport {
                Transport::Tcp => {
                    let mut buffers = TcpBuffers::new();
                    let mut driver = TcpServer::new(buffers.socket(net.server), PORT);
                    driver.run_one(&mut server, |_| {}).await
                }
                Transport::Udp => {
                    let mut buffers = UdpBuffers::new();
                    let mut driver = UdpServer::new(buffers.bound(net.server, PORT));
                    driver.run_one(&mut server, |_| {}).await
                }
            };
            assert_eq!(outcome, Err(Error::Mismatch), "{case}");
        }

        let server = (SERVER, PORT).into();
        let mut udp = Client::new(Config::udp(), &mut buffer).unwrap();
        let mut tcp_buffers = TcpBuffers::new();
        let outcome = run_tcp_client(
            &mut udp,
            &mut tcp_buffers.socket(net.client),
            server,
            |_| {},
        )
        .await;
        assert_eq!(
            outcome,
            Err(Error::Mismatch),
            "a UDP client on a TCP socket"
        );

        let mut tcp = Client::new(Config::tcp(), &mut buffer).unwrap();
        let mut udp_buffers = UdpBuffers::new();
        let outcome = run_udp_client(
            &mut tcp,
            &mut udp_buffers.bound(net.client, 0),
            server,
            |_| {},
        )
        .await;
        assert_eq!(
            outcome,
            Err(Error::Mismatch),
            "a TCP client on a UDP socket"
        );

        let mut udp = Client::new(Config::udp(), &mut buffer).unwrap();
        let mut udp_buffers = UdpBuffers::new();
        let outcome = run_udp_client(
            &mut udp,
            &mut udp_buffers.unbound(net.client),
            server,
            |_| {},
        )
        .await;
        assert_eq!(outcome, Err(Error::Mismatch), "an unbound UDP socket");

        let config = Config {
            buf_len: 1470,
            ..Config::udp()
        };
        let mut udp = Client::new(config, &mut buffer).unwrap();
        let mut small = UdpBuffers::with_send_buffer(1000);
        let outcome =
            run_udp_client(&mut udp, &mut small.bound(net.client, 0), server, |_| {}).await;
        assert_eq!(outcome, Err(Error::Mismatch), "a send buffer of 1000 bytes");
    });
}

#[test]
fn a_udp_client_runs_with_a_send_buffer_of_one_datagram() {
    run(async |net| {
        let mut buffers = UdpBuffers::with_send_buffer(512);
        let config = Config {
            length: Length::Bytes(5_120),
            buf_len: 512,
            ..Config::udp()
        };
        let (server, client) = join(
            serve(net, server_config(Transport::Udp)),
            udp_client(&mut buffers.bound(net.client, 0), config),
        )
        .await;
        assert_eq!(client.outcome, Ok(Ok(())));
        assert_eq!(server.last().bytes, 5_120 + 512);
    });
}
