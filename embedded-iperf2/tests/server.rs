//! The server of P-8 to P-16 and P-21, driven with bytes and times and no socket, and with the bytes of the
//! stock clients in the samples (T-2).
//!
//! Every loop here stops after `LIMIT` polls, so a server that never ends fails a test within seconds instead of
//! hanging it.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

mod common;

use embedded_iperf2::wire::{
    Amount, BURST_HEADER_LEN, BurstHeader, HEADER_ACK_LEN, HeaderAck, PREFIX_LEN, Prefix,
    SERVER_REPORT_LEN, ServerReport, TEST_HEADER_LEN, TestHeader, Timestamp, flags,
};
use embedded_iperf2::{Action, ConfigError, Report, Server, ServerConfig, TestError, Transport};

/// The most polls a loop makes before it calls the server stuck.
const LIMIT: usize = 100_000;

fn send(action: Action<'_>) -> Vec<u8> {
    match action {
        Action::Send(bytes) => bytes.to_vec(),
        other => panic!("expected Send, got {other:?}"),
    }
}

fn report(action: Action<'_>) -> Report {
    match action {
        Action::Report(report) => report,
        other => panic!("expected Report, got {other:?}"),
    }
}

fn tcp() -> ServerConfig {
    ServerConfig {
        transport: Transport::Tcp,
        duration_us: None,
        interval_us: None,
        ipv6: false,
    }
}

fn udp() -> ServerConfig {
    ServerConfig {
        transport: Transport::Udp,
        ..tcp()
    }
}

/// A 2.2.2 client's TCP test header, as our client and the stock one write it.
fn tcp_header() -> TestHeader {
    TestHeader {
        flags: 0x4001_0080,
        num_threads: 1,
        port: 5001,
        buf_len: 16384,
        amount: Amount::Bytes(65536),
        upper_flags: 0,
        lower_flags: 0,
        version_u: 0x0002_0002,
        version_l: 0x0002_0003,
        rate: 0,
    }
}

/// A reverse TCP test header.
fn reverse_tcp_header() -> TestHeader {
    TestHeader {
        flags: 0x4401_0080,
        upper_flags: flags::REVERSE,
        ..tcp_header()
    }
}

fn udp_header() -> TestHeader {
    TestHeader {
        flags: 0x4801_00A0,
        buf_len: 512,
        amount: Amount::Time { centiseconds: 200 },
        rate: 262_144,
        ..tcp_header()
    }
}

fn reverse_udp_header() -> TestHeader {
    TestHeader {
        flags: 0xEC01_00A0,
        upper_flags: flags::REVERSE,
        ..udp_header()
    }
}

fn encode(header: &TestHeader) -> Vec<u8> {
    let mut bytes = vec![0; TEST_HEADER_LEN];
    header.encode(&mut bytes).unwrap();
    bytes
}

/// A UDP datagram of `len` bytes with packet ID `id`, sent at `sent_us`, and `header` after the prefix.
fn udp_datagram(id: i64, sent_us: u64, header: Option<&TestHeader>, len: usize) -> Vec<u8> {
    let mut datagram = vec![b'0'; len];
    Prefix {
        packet_id: id,
        sent: Timestamp {
            sec: (sent_us / 1_000_000) as u32,
            usec: (sent_us % 1_000_000) as u32,
        },
    }
    .encode(&mut datagram)
    .unwrap();
    if let Some(header) = header {
        header.encode(&mut datagram[PREFIX_LEN..]).unwrap();
    }
    datagram
}

/// Sends every chunk the server offers at `now_us`, and returns them joined, with what it asked for next.
fn send_all(server: &mut Server<'_>, now_us: u64) -> (Vec<u8>, String) {
    let mut stream = Vec::new();
    for _ in 0..LIMIT {
        match server.poll(now_us) {
            Action::Send(bytes) => {
                let bytes = bytes.to_vec();
                server.sent(bytes.len());
                stream.extend(bytes);
            }
            other => return (stream, format!("{other:?}")),
        }
    }
    panic!("the server sent without end");
}

/// What a server did from a start time until it reported its end.
struct Driven {
    sends: Vec<Vec<u8>>,
    reports: Vec<Report>,
    now: u64,
}

/// Polls the server from `now` until its final report: sends everything it offers, calls `received` with the
/// datagram `each_send` gives after each send, moves the clock to each wait, and keeps the reports.
fn drive(server: &mut Server<'_>, mut now: u64, each_send: Option<&[u8]>) -> Driven {
    let mut sends = Vec::new();
    let mut reports = Vec::new();
    for _ in 0..LIMIT {
        match server.poll(now) {
            Action::Send(bytes) => {
                let bytes = bytes.to_vec();
                server.sent(bytes.len());
                sends.push(bytes);
                if let Some(datagram) = each_send {
                    now += 1000;
                    server.received(now, datagram);
                }
            }
            Action::Wait { until_us } => now = until_us.expect("a server alone waits for a time"),
            Action::Report(report) => {
                let last = report.final_report;
                reports.push(report);
                if last {
                    return Driven {
                        sends,
                        reports,
                        now,
                    };
                }
            }
            Action::Close => panic!("a UDP server does not close"),
            Action::Done(outcome) => panic!("done before a final report: {outcome:?}"),
        }
    }
    panic!("the server did not finish");
}

#[test]
fn a_server_needs_room_for_a_burst_header_or_the_longest_reverse_datagram() {
    assert!(matches!(
        Server::new(tcp(), &mut [0; 119]),
        Err(ConfigError::BufLenTooShort)
    ));
    assert!(Server::new(tcp(), &mut [0; 120]).is_ok());
    assert!(matches!(
        Server::new(udp(), &mut [0; 1469]),
        Err(ConfigError::BufLenTooShort)
    ));
    assert!(Server::new(udp(), &mut [0; 1470]).is_ok());
    let ipv6 = ServerConfig {
        ipv6: true,
        ..udp()
    };
    assert!(matches!(
        Server::new(ipv6, &mut [0; 1449]),
        Err(ConfigError::BufLenTooShort)
    ));
    assert!(Server::new(ipv6, &mut [0; 1450]).is_ok());
}

#[test]
fn tcp_acks_the_stock_header_and_counts_the_stream() {
    let sample = common::read("tcp");
    let mut buffer = [0; 2048];
    let mut server = Server::new(tcp(), &mut buffer).unwrap();
    assert_eq!(server.poll(0), Action::Wait { until_us: None });
    for (at, record) in sample.client.iter().enumerate() {
        server.received(at as u64 * 1000, record);
        if at == 0 {
            let (ack, next) = send_all(&mut server, 0);
            assert_eq!(
                ack,
                sample.server_stream(),
                "the stock ack, byte for byte (P-9)"
            );
            assert_eq!(next, "Wait { until_us: None }");
        }
    }
    server.closed(9_000);
    let last = report(server.poll(9_000));
    assert_eq!(
        (last.bytes, last.end_us, last.final_report),
        (65536, 9_000, true)
    );
    assert_eq!(server.poll(9_000), Action::Done(Ok(())));
}

#[test]
fn tcp_sends_the_rest_of_the_ack_after_a_short_write() {
    let mut buffer = [0; 2048];
    let mut server = Server::new(tcp(), &mut buffer).unwrap();
    server.received(0, &encode(&tcp_header()));
    let ack = send(server.poll(0));
    server.sent(10);
    assert_eq!(send(server.poll(0)), ack[10..]);
}

#[test]
fn tcp_reads_a_header_that_arrives_a_byte_at_a_time() {
    let mut buffer = [0; 2048];
    let mut server = Server::new(tcp(), &mut buffer).unwrap();
    for byte in encode(&tcp_header()) {
        assert!(matches!(server.poll(0), Action::Wait { .. }));
        server.received(0, &[byte]);
    }
    assert_eq!(send(server.poll(0)).len(), HEADER_ACK_LEN);
}

#[test]
fn tcp_without_a_header_sends_no_ack_and_does_not_count_the_flags_word() {
    let mut buffer = [0; 2048];
    let mut server = Server::new(tcp(), &mut buffer).unwrap();
    server.received(0, b"0123456789");
    assert_eq!(server.poll(0), Action::Wait { until_us: None });
    server.closed(10);
    assert_eq!(report(server.poll(10)).bytes, 6, "P-8");
}

#[test]
fn tcp_with_x_gets_the_ack() {
    let mut buffer = [0; 2048];
    let mut server = Server::new(tcp(), &mut buffer).unwrap();
    let header = TestHeader {
        flags: 0x4601_0080,
        ..tcp_header()
    };
    server.received(0, &encode(&header));
    let ack = send(server.poll(0));
    assert_eq!(HeaderAck::parse(&ack).unwrap().version_u, 0x0002_0002);
}

#[test]
fn tcp_with_version2_and_no_extend_gets_no_ack() {
    let mut buffer = [0; 2048];
    let mut server = Server::new(tcp(), &mut buffer).unwrap();
    let header = TestHeader {
        flags: flags::VERSION1 | flags::VERSION2,
        ..tcp_header()
    };
    server.received(0, &encode(&header));
    assert_eq!(server.poll(0), Action::Wait { until_us: None });
}

#[test]
fn tcp_refuses_a_mode_outside_v1() {
    let refused = [
        TestHeader {
            flags: flags::VERSION1 | flags::EXTEND | 0x0001_0080,
            ..tcp_header()
        },
        TestHeader {
            flags: tcp_header().flags | flags::KEYCHECK,
            ..tcp_header()
        },
        TestHeader {
            flags: tcp_header().flags | flags::BOUNCEBACK,
            ..tcp_header()
        },
        TestHeader {
            num_threads: 2,
            ..tcp_header()
        },
        TestHeader {
            upper_flags: 0x0800,
            ..tcp_header()
        },
        TestHeader {
            lower_flags: 1,
            ..tcp_header()
        },
        TestHeader {
            flags: 0x4001_0202,
            ..tcp_header()
        },
    ];
    for header in refused {
        let mut buffer = [0; 2048];
        let mut server = Server::new(tcp(), &mut buffer).unwrap();
        server.received(0, &encode(&header));
        assert_eq!(
            server.poll(0),
            Action::Done(Err(TestError::Refused)),
            "{header:?}"
        );
    }
}

#[test]
fn tcp_closed_before_the_test_starts_is_a_protocol_error() {
    let mut buffer = [0; 2048];
    let mut server = Server::new(tcp(), &mut buffer).unwrap();
    server.received(0, &encode(&tcp_header())[..30]);
    server.closed(10);
    assert_eq!(server.poll(10), Action::Done(Err(TestError::Protocol)));
}

#[test]
fn tcp_reverse_sends_bursts_of_the_stock_layout_then_closes() {
    let sample = common::read("tcp-reverse");
    let mut buffer = [0; 5000];
    let mut server = Server::new(tcp(), &mut buffer).unwrap();
    server.received(100, &sample.client_stream());
    let (stream, next) = send_all(&mut server, 100);
    assert_eq!(next, "Close");
    assert_eq!(
        stream.len(),
        65536,
        "no ack, and the byte count from the header"
    );
    for (index, burst) in stream.chunks(16384).enumerate() {
        let header = BurstHeader::parse(burst).unwrap();
        assert_eq!(header.burst_id, index as u32 + 1);
        assert_eq!(header.burst_size, 16384);
        assert_eq!(header.seqno, (index as u64 + 1) * 16384 + 1);
        assert_eq!(header.start, Timestamp { sec: 0, usec: 100 });
    }
    assert_eq!(server.poll(100), Action::Wait { until_us: None });
    server.closed(200);
    let last = report(server.poll(200));
    assert_eq!((last.bytes, last.final_report), (65536, true));
    assert_eq!(server.poll(200), Action::Done(Ok(())));
}

#[test]
fn tcp_reverse_takes_the_burst_length_from_the_header_within_bounds() {
    for (buf_len, burst) in [
        (4000, 4000),
        (2_000_000, 2_000_000),
        (16 * 1024 * 1024, 16 * 1024 * 1024),
        (16 * 1024 * 1024 + 1, 131_072),
        (0, 131_072),
        (50, 120),
    ] {
        let mut buffer = [0; 200];
        let mut server = Server::new(tcp(), &mut buffer).unwrap();
        let header = TestHeader {
            buf_len,
            amount: Amount::Bytes(100_000_000),
            ..reverse_tcp_header()
        };
        server.received(0, &encode(&header));
        let first = send(server.poll(0));
        assert_eq!(
            BurstHeader::parse(&first).unwrap().burst_size,
            burst,
            "-l {buf_len}"
        );
    }
}

#[test]
fn a_burst_header_cut_by_a_short_write_keeps_its_write_time() {
    let mut buffer = [0; 5000];
    let mut server = Server::new(tcp(), &mut buffer).unwrap();
    let header = TestHeader {
        buf_len: 200,
        ..reverse_tcp_header()
    };
    server.received(0, &encode(&header));
    let first = send(server.poll(100));
    // The cut splits `write_tv_usec`, at offsets 24 to 27, where 100 and 900 differ.
    server.sent(26);
    let rest = send(server.poll(900));
    let header = BurstHeader::parse(&[&first[..26], &rest[..]].concat()).unwrap();
    assert_eq!(header.write, Timestamp { sec: 0, usec: 100 });
    server.sent(rest.len());
    let second = send(server.poll(900));
    assert_eq!(
        BurstHeader::parse(&second).unwrap().write,
        Timestamp { sec: 0, usec: 900 }
    );
}

#[test]
fn the_server_duration_caps_a_reverse_test() {
    let mut buffer = [0; 5000];
    let config = ServerConfig {
        duration_us: Some(1_000_000),
        ..tcp()
    };
    let mut server = Server::new(config, &mut buffer).unwrap();
    let header = TestHeader {
        amount: Amount::Time { centiseconds: 500 },
        rate: 8_000_000,
        ..reverse_tcp_header()
    };
    server.received(0, &encode(&header));
    send_all(&mut server, 0);
    let last = report(server.poll(1_000_000));
    assert_eq!((last.end_us, last.final_report), (1_000_000, true), "P-15");
    assert_eq!(server.poll(1_000_000), Action::Done(Ok(())));
}

#[test]
fn the_server_duration_ends_a_reverse_test_in_byte_mode() {
    let mut buffer = [0; 5000];
    let config = ServerConfig {
        duration_us: Some(1_000_000),
        ..udp()
    };
    let mut server = Server::new(config, &mut buffer).unwrap();
    let header = TestHeader {
        amount: Amount::Bytes(1_000_000_000),
        ..reverse_udp_header()
    };
    server.received(500, &udp_datagram(1, 0, Some(&header), 512));
    let driven = drive(&mut server, 500, None);
    assert_eq!(driven.now, 1_000_500, "P-15");
    assert!(*driven.sends.last().unwrap() != driven.sends[0]);
    assert!(
        Prefix::parse(driven.sends.last().unwrap())
            .unwrap()
            .packet_id
            < 0
    );
}

#[test]
fn the_server_duration_ends_a_forward_tcp_test() {
    let mut buffer = [0; 2048];
    let config = ServerConfig {
        duration_us: Some(1_000_000),
        ..tcp()
    };
    let mut server = Server::new(config, &mut buffer).unwrap();
    server.received(5_000, &encode(&tcp_header()));
    send_all(&mut server, 5_000);
    assert_eq!(
        server.poll(5_000),
        Action::Wait {
            until_us: Some(1_005_000)
        }
    );
    assert!(report(server.poll(1_005_000)).final_report);
    assert_eq!(server.poll(1_005_000), Action::Done(Ok(())));
}

#[test]
fn a_tcp_server_reports_each_interval_it_receives() {
    let mut buffer = [0; 2048];
    let config = ServerConfig {
        interval_us: Some(1_000_000),
        ..tcp()
    };
    let mut server = Server::new(config, &mut buffer).unwrap();
    server.received(0, &encode(&tcp_header()));
    send_all(&mut server, 0);
    server.received(500_000, &[0; 100]);
    let interval = report(server.poll(1_000_000));
    assert_eq!((interval.end_us, interval.bytes), (1_000_000, 164));
}

#[test]
fn a_sending_server_reports_each_interval() {
    for (header, config) in [
        (
            TestHeader {
                amount: Amount::Time { centiseconds: 300 },
                rate: 8_000_000,
                buf_len: 1000,
                ..reverse_tcp_header()
            },
            tcp(),
        ),
        (
            TestHeader {
                amount: Amount::Time { centiseconds: 300 },
                rate: 4_096_000,
                ..reverse_udp_header()
            },
            udp(),
        ),
    ] {
        let mut buffer = [0; 2048];
        let config = ServerConfig {
            interval_us: Some(1_000_000),
            ..config
        };
        let mut server = Server::new(config, &mut buffer).unwrap();
        let first = match config.transport {
            Transport::Tcp => encode(&header),
            Transport::Udp => udp_datagram(1, 0, Some(&header), 512),
        };
        server.received(0, &first);
        let mut now = 0;
        let mut intervals = Vec::new();
        for _ in 0..LIMIT {
            match server.poll(now) {
                Action::Send(bytes) => {
                    let length = bytes.len();
                    server.sent(length);
                }
                Action::Wait { until_us } => now = until_us.unwrap(),
                Action::Report(report) if !report.final_report => intervals.push(report.bytes),
                _ => break,
            }
        }
        let expected = match config.transport {
            Transport::Tcp => 1_000_000,
            Transport::Udp => 1000 * 512,
        };
        assert_eq!(intervals, [expected; 3], "{:?}", config.transport);
    }
}

/// Feeds the client datagrams of a forward UDP sample to a server up to the end of the test, one every
/// millisecond, and returns the server and the time of the last.
fn replay<'b>(case: &str, buffer: &'b mut [u8]) -> (Server<'b>, u64) {
    let sample = common::read(case);
    let mut server = Server::new(udp(), buffer).unwrap();
    let mut now = 0;
    for datagram in &sample.client {
        now += 1000;
        server.received(now, datagram);
        if Prefix::parse(datagram).unwrap().packet_id < 0 {
            break;
        }
        assert!(matches!(server.poll(now), Action::Wait { .. }));
    }
    (server, now)
}

#[test]
fn udp_counts_equal_the_stock_server_report() {
    for case in ["udp", "udp-bytes"] {
        let recorded = ServerReport::parse(&common::read(case).server[0]).unwrap();
        let mut buffer = [0; 2048];
        let (mut server, now) = replay(case, &mut buffer);
        let ours = ServerReport::parse(&send(server.poll(now))).unwrap();
        assert_eq!(
            (ours.bytes, ours.datagrams, ours.lost, ours.out_of_order),
            (
                recorded.bytes,
                recorded.datagrams,
                recorded.lost,
                recorded.out_of_order
            ),
            "{case} (T-2)"
        );
    }
}

#[test]
fn udp_repeats_the_report_while_ends_arrive_then_finishes() {
    let mut buffer = [0; 2048];
    let (mut server, now) = replay("udp", &mut buffer);
    let first = send(server.poll(now));
    assert_eq!(first.len(), SERVER_REPORT_LEN);
    server.sent(first.len());
    assert_eq!(
        server.poll(now),
        Action::Wait {
            until_us: Some(now + 250_000)
        }
    );
    let end = common::read("udp").client.last().unwrap().clone();
    server.received(now + 100_000, &end);
    assert_eq!(
        send(server.poll(now + 100_000)),
        first,
        "the same report again (P-12)"
    );
    server.sent(first.len());
    server.received(now + 350_000, &end);
    let last = report(server.poll(now + 350_000));
    assert_eq!(
        last.udp.unwrap().datagrams,
        130,
        "250 ms of silence ends it"
    );
    assert_eq!(last.bytes, 130 * 512);
    assert_eq!(server.poll(now + 350_000), Action::Done(Ok(())));
}

#[test]
fn udp_sends_at_most_nine_reports() {
    let mut buffer = [0; 2048];
    let (mut server, now) = replay("udp", &mut buffer);
    let end = common::read("udp").client.last().unwrap().clone();
    let driven = drive(&mut server, now, Some(&end));
    assert_eq!(driven.sends.len(), 9);
}

#[test]
fn udp_reports_the_jitter_in_microseconds() {
    let mut buffer = [0; 2048];
    let mut server = Server::new(udp(), &mut buffer).unwrap();
    server.received(100, &udp_datagram(1, 0, Some(&udp_header()), 512));
    server.received(1_300, &udp_datagram(2, 1_000, Some(&udp_header()), 512));
    server.received(2_300, &udp_datagram(-3, 2_000, Some(&udp_header()), 512));
    let ours = ServerReport::parse(&send(server.poll(2_300))).unwrap();
    assert_eq!(
        ours.jitter,
        Timestamp { sec: 0, usec: 12 },
        "the mean of 12500 and 11719 ns"
    );
    assert_eq!(
        ours.duration,
        Timestamp {
            sec: 0,
            usec: 2_200
        }
    );
}

#[test]
fn udp_ignores_a_short_first_datagram_or_one_without_a_positive_id() {
    let mut buffer = [0; 2048];
    let mut server = Server::new(udp(), &mut buffer).unwrap();
    server.received(0, &udp_datagram(1, 0, None, 39));
    server.received(0, &udp_datagram(0, 0, Some(&udp_header()), 512));
    server.received(0, &udp_datagram(-3, 0, Some(&udp_header()), 512));
    server.received(10, &udp_datagram(-1, 0, None, 512));
    assert_eq!(
        server.poll(10),
        Action::Wait { until_us: None },
        "no test has started"
    );
    server.received(20, &udp_datagram(1, 0, None, 40));
    server.received(30, &udp_datagram(-2, 0, None, 40));
    let ours = ServerReport::parse(&send(server.poll(30))).unwrap();
    assert_eq!(
        ours.datagrams, 2,
        "a first datagram of 40 bytes starts a test"
    );
}

#[test]
fn udp_ignores_an_id_of_zero_during_a_test() {
    let mut buffer = [0; 2048];
    let mut server = Server::new(udp(), &mut buffer).unwrap();
    server.received(0, &udp_datagram(1, 0, Some(&udp_header()), 512));
    server.received(10, &udp_datagram(0, 0, Some(&udp_header()), 512));
    assert!(matches!(server.poll(10), Action::Wait { .. }));
    server.received(20, &udp_datagram(-2, 0, Some(&udp_header()), 512));
    let ours = ServerReport::parse(&send(server.poll(20))).unwrap();
    assert_eq!((ours.datagrams, ours.bytes), (2, 1024));
}

#[test]
fn udp_reads_ids_as_32_bits_without_seqno64b() {
    let without = TestHeader {
        flags: 0x4001_00A0,
        ..udp_header()
    };
    let mut buffer = [0; 2048];
    let mut server = Server::new(udp(), &mut buffer).unwrap();
    let mut first = udp_datagram(1, 0, Some(&without), 512);
    first[12..16].copy_from_slice(&[0, 0, 0, 7]);
    server.received(0, &first);
    let mut end = udp_datagram(-2, 0, Some(&without), 512);
    end[12..16].copy_from_slice(&[0, 0, 0, 0]);
    server.received(10, &end);
    let ours = ServerReport::parse(&send(server.poll(10))).unwrap();
    assert_eq!((ours.datagrams, ours.lost), (2, 0));
}

#[test]
fn udp_reads_the_compat_fill_as_no_header() {
    let mut buffer = [0; 2048];
    let mut server = Server::new(udp(), &mut buffer).unwrap();
    let mut fill = udp_datagram(1, 0, None, 512);
    fill[PREFIX_LEN..PREFIX_LEN + 4].copy_from_slice(&flags::UDP_COMPAT_FILL.to_be_bytes());
    server.received(0, &fill);
    let mut end = fill.clone();
    end[0..4].copy_from_slice(&(-2_i32).to_be_bytes());
    server.received(10, &end);
    let ours = ServerReport::parse(&send(server.poll(10))).unwrap();
    assert_eq!((ours.datagrams, ours.lost), (2, 0), "P-11");
}

#[test]
fn udp_refuses_a_mode_outside_v1() {
    let mut buffer = [0; 2048];
    let mut server = Server::new(udp(), &mut buffer).unwrap();
    let header = TestHeader {
        num_threads: 4,
        ..udp_header()
    };
    server.received(0, &udp_datagram(1, 0, Some(&header), 512));
    assert_eq!(server.poll(0), Action::Done(Err(TestError::Refused)));
}

#[test]
fn udp_reverse_sends_the_prefix_and_fill_then_one_end() {
    let sample = common::read("udp-reverse");
    let mut buffer = [0; 2048];
    let mut server = Server::new(udp(), &mut buffer).unwrap();
    server.received(0, &sample.client[0]);
    let driven = drive(&mut server, 0, None);
    let ids: Vec<i64> = driven
        .sends
        .iter()
        .map(|datagram| Prefix::parse(datagram).unwrap().packet_id)
        .collect();
    let count = ids.len() as i64 - 1;
    assert_eq!(ids, (1..=count).chain([-(count + 1)]).collect::<Vec<_>>());
    assert_eq!(count, 128, "2 s at 262144 bit/s in datagrams of 512 bytes");
    for datagram in &driven.sends {
        assert_eq!(datagram.len(), 512);
        assert_eq!(
            datagram[PREFIX_LEN..PREFIX_LEN + 4],
            *b"6789",
            "fill, without a header"
        );
    }
    assert_eq!(driven.reports.last().unwrap().bytes, 128 * 512);
    assert_eq!(server.poll(driven.now), Action::Done(Ok(())));
}

#[test]
fn udp_reverse_cuts_a_long_datagram() {
    for (buf_len, ipv6, sent) in [
        (8000, false, 1470),
        (8000, true, 1450),
        (0, false, 1470),
        (300, false, 300),
    ] {
        let mut buffer = [0; 2048];
        let config = ServerConfig { ipv6, ..udp() };
        let mut server = Server::new(config, &mut buffer).unwrap();
        let header = TestHeader {
            buf_len,
            ..reverse_udp_header()
        };
        server.received(0, &udp_datagram(1, 0, Some(&header), 512));
        assert_eq!(send(server.poll(0)).len(), sent, "P-21, -l {buf_len}");
    }
}

#[test]
fn the_server_duration_ends_a_forward_udp_test_with_a_report() {
    let mut buffer = [0; 2048];
    let config = ServerConfig {
        duration_us: Some(1_000_000),
        ..udp()
    };
    let mut server = Server::new(config, &mut buffer).unwrap();
    server.received(7_000, &udp_datagram(1, 0, Some(&udp_header()), 512));
    assert_eq!(
        server.poll(7_000),
        Action::Wait {
            until_us: Some(1_007_000)
        }
    );
    let ours = ServerReport::parse(&send(server.poll(1_007_000))).unwrap();
    assert_eq!(ours.datagrams, 1, "P-15");
}

#[test]
fn a_receiving_server_reports_each_interval() {
    let mut buffer = [0; 2048];
    let config = ServerConfig {
        interval_us: Some(1_000_000),
        ..udp()
    };
    let mut server = Server::new(config, &mut buffer).unwrap();
    server.received(0, &udp_datagram(1, 0, Some(&udp_header()), 512));
    server.received(500_000, &udp_datagram(3, 0, Some(&udp_header()), 512));
    assert_eq!(
        server.poll(500_000),
        Action::Wait {
            until_us: Some(1_000_000)
        }
    );
    let interval = report(server.poll(1_000_000));
    assert_eq!(
        (interval.start_us, interval.end_us, interval.bytes),
        (0, 1_000_000, 1024)
    );
    let udp = interval.udp.unwrap();
    assert_eq!((udp.datagrams, udp.lost), (3, 1));
    assert!(!interval.final_report);
}

#[test]
fn tcp_counts_the_bytes_that_arrive_before_the_ack_is_out() {
    let mut buffer = [0; 2048];
    let mut server = Server::new(tcp(), &mut buffer).unwrap();
    server.received(0, &encode(&tcp_header()));
    server.received(1, &[b'0'; 1000]);
    send_all(&mut server, 2);
    server.closed(3);
    assert_eq!(report(server.poll(3)).bytes, 1064);
}

#[test]
fn a_close_before_the_ack_is_out_ends_the_test_with_its_bytes() {
    let mut buffer = [0; 2048];
    let mut server = Server::new(tcp(), &mut buffer).unwrap();
    server.received(0, &[encode(&tcp_header()), vec![b'0'; 36]].concat());
    server.closed(5);
    assert_eq!(report(server.poll(5)).bytes, 100, "P-10");
    assert_eq!(server.poll(5), Action::Done(Ok(())));
}

#[test]
fn a_close_while_a_reverse_test_sends_ends_it_with_the_bytes_sent() {
    let mut buffer = [0; 5000];
    let mut server = Server::new(tcp(), &mut buffer).unwrap();
    server.received(0, &encode(&reverse_tcp_header()));
    let first = send(server.poll(0));
    server.sent(first.len());
    server.closed(10);
    let last = report(server.poll(10));
    assert_eq!((last.bytes, last.final_report), (5000, true), "P-16");
    assert_eq!(server.poll(10), Action::Done(Ok(())));
}

#[test]
fn a_close_before_the_ack_of_a_reverse_test_reports_nothing_sent() {
    let mut buffer = [0; 5000];
    let mut server = Server::new(tcp(), &mut buffer).unwrap();
    let header = TestHeader {
        flags: 0x4601_0080,
        ..reverse_tcp_header()
    };
    server.received(0, &encode(&header));
    server.closed(5);
    assert_eq!(report(server.poll(5)).bytes, 0, "the server is the sender");
}

#[test]
fn the_server_duration_ends_a_test_stalled_in_its_header() {
    let mut buffer = [0; 2048];
    let config = ServerConfig {
        duration_us: Some(1_000_000),
        ..tcp()
    };
    let mut server = Server::new(config, &mut buffer).unwrap();
    server.received(0, &encode(&tcp_header())[..30]);
    assert_eq!(
        server.poll(0),
        Action::Wait {
            until_us: Some(1_000_000)
        }
    );
    let last = report(server.poll(1_000_000));
    assert_eq!(last.bytes, 30, "P-15");
    assert_eq!(server.poll(1_000_000), Action::Done(Ok(())));
}

#[test]
fn the_server_duration_ends_a_reverse_test_whose_client_never_closes() {
    let mut buffer = [0; 5000];
    let config = ServerConfig {
        duration_us: Some(1_000_000),
        ..tcp()
    };
    let mut server = Server::new(config, &mut buffer).unwrap();
    let header = TestHeader {
        amount: Amount::Bytes(5000),
        ..reverse_tcp_header()
    };
    server.received(0, &encode(&header));
    let (_, next) = send_all(&mut server, 0);
    assert_eq!(next, "Close");
    assert_eq!(
        server.poll(0),
        Action::Wait {
            until_us: Some(1_000_000)
        }
    );
    assert_eq!(report(server.poll(1_000_000)).bytes, 5000, "P-15");
}

#[test]
fn a_time_mode_reverse_test_finishes_its_last_burst() {
    let mut buffer = [0; 120];
    let header = TestHeader {
        buf_len: 1000,
        amount: Amount::Time { centiseconds: 100 },
        ..reverse_tcp_header()
    };
    let mut server = Server::new(tcp(), &mut buffer).unwrap();
    server.received(0, &encode(&header));
    let first = send(server.poll(0));
    server.sent(first.len());
    let (rest, next) = send_all(&mut server, 1_000_000);
    assert_eq!(rest.len(), 880, "the burst stated 1000 bytes (P-16)");
    assert_eq!(next, "Close");
}

#[test]
fn a_udp_server_reports_the_last_part_of_an_interval() {
    let mut buffer = [0; 2048];
    let config = ServerConfig {
        interval_us: Some(1_000_000),
        ..udp()
    };
    let mut server = Server::new(config, &mut buffer).unwrap();
    server.received(0, &udp_datagram(1, 0, Some(&udp_header()), 512));
    server.received(1_500_000, &udp_datagram(2, 0, Some(&udp_header()), 512));
    server.received(1_700_000, &udp_datagram(-3, 0, Some(&udp_header()), 512));
    let driven = drive(&mut server, 1_700_000, None);
    let spans: Vec<(u64, u64, u64, u64)> = driven
        .reports
        .iter()
        .map(|r| (r.start_us, r.end_us, r.bytes, r.udp.unwrap().datagrams))
        .collect();
    assert_eq!(
        spans,
        [
            (0, 1_000_000, 512, 1),
            (1_000_000, 1_700_000, 1024, 2),
            (0, 1_700_000, 1536, 3)
        ],
        "P-19"
    );
}

#[test]
fn a_burst_header_buffer_shorter_than_the_header_still_builds_it() {
    let mut buffer = [0; 120];
    let mut server = Server::new(tcp(), &mut buffer).unwrap();
    server.received(0, &encode(&reverse_tcp_header()));
    let first = send(server.poll(0));
    assert_eq!(first.len(), BURST_HEADER_LEN);
}
