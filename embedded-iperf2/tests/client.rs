//! Tests the client of P-1 to P-7, P-22, and P-23 with bytes and times, and without a socket.
//!
//! Every loop here stops after `LIMIT` polls, so a client that never ends fails a test within seconds instead of
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
    SERVER_REPORT_LEN, ServerReport, TestHeader, Timestamp, flags,
};
use embedded_iperf2::{Action, Client, Config, ConfigError, Length, Report, TestError, Transport};

/// The most polls a loop makes before it calls the client stuck.
const LIMIT: usize = 100_000;

/// The bytes of a `Send`, or a panic naming what came instead.
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

/// Sends every chunk the client offers at `now_us`, and returns them, until it asks for something else.
fn send_all(client: &mut Client<'_>, now_us: u64) -> (Vec<Vec<u8>>, String) {
    let mut chunks = Vec::new();
    for _ in 0..LIMIT {
        match client.poll(now_us) {
            Action::Send(bytes) => {
                let bytes = bytes.to_vec();
                client.sent(bytes.len());
                chunks.push(bytes);
            }
            other => return (chunks, format!("{other:?}")),
        }
    }
    panic!("the client sent without end");
}

/// Sends every UDP datagram of data the client offers at `now_us`, and returns them. It stops at the first end of
/// the test, and does not send that datagram.
fn send_data(client: &mut Client<'_>, now_us: u64) -> Vec<Vec<u8>> {
    let mut datagrams = Vec::new();
    for _ in 0..LIMIT {
        let Action::Send(bytes) = client.poll(now_us) else {
            return datagrams;
        };
        if Prefix::parse(bytes).unwrap().packet_id < 0 {
            return datagrams;
        }
        let bytes = bytes.to_vec();
        client.sent(bytes.len());
        datagrams.push(bytes);
    }
    panic!("the client sent without end");
}

/// What a client did from a start time until it was done.
struct Driven {
    sends: Vec<Vec<u8>>,
    reports: Vec<Report>,
    outcome: Result<(), TestError>,
    now: u64,
}

/// Polls the client from `now` until it is done: sends everything it offers, moves the clock to each wait, and
/// keeps the reports.
fn drive(client: &mut Client<'_>, mut now: u64) -> Driven {
    let mut sends = Vec::new();
    let mut reports = Vec::new();
    for _ in 0..LIMIT {
        match client.poll(now) {
            Action::Send(bytes) => {
                let bytes = bytes.to_vec();
                client.sent(bytes.len());
                sends.push(bytes);
            }
            Action::Wait { until_us } => now = until_us.expect("a client alone waits for a time"),
            Action::Report(report) => reports.push(report),
            Action::Done(outcome) => {
                return Driven {
                    sends,
                    reports,
                    outcome,
                    now,
                };
            }
            Action::Close => panic!("a UDP client does not close"),
        }
    }
    panic!("the client did not finish");
}

/// The packet ID of each datagram.
fn ids(datagrams: &[Vec<u8>]) -> Vec<i64> {
    datagrams
        .iter()
        .map(|datagram| Prefix::parse(datagram).unwrap().packet_id)
        .collect()
}

fn tcp_bytes(n: u64) -> Config {
    Config {
        length: Length::Bytes(n),
        buf_len: 16384,
        ..Config::tcp()
    }
}

fn udp_bytes(n: u64) -> Config {
    Config {
        length: Length::Bytes(n),
        buf_len: 512,
        rate: 0,
        ..Config::udp()
    }
}

fn reverse_tcp_time(us: u64) -> Config {
    Config {
        reverse: true,
        length: Length::Time { us },
        ..tcp_bytes(0)
    }
}

fn reverse_udp_time(us: u64) -> Config {
    Config {
        reverse: true,
        length: Length::Time { us },
        ..udp_bytes(0)
    }
}

/// A burst of `size` bytes with a burst header.
fn burst(burst_id: u32, size: u32) -> Vec<u8> {
    let mut burst = vec![b'x'; size as usize];
    BurstHeader {
        start: Timestamp::default(),
        write: Timestamp::default(),
        burst_id,
        burst_size: size,
        seqno: 0,
    }
    .encode(&mut burst)
    .unwrap();
    burst
}

/// A datagram from the server of a reverse UDP test: the prefix and the fill.
fn server_datagram(id: i64, sent_us: u64) -> Vec<u8> {
    let mut datagram = vec![b'6'; 512];
    Prefix {
        packet_id: id,
        sent: Timestamp {
            sec: (sent_us / 1_000_000) as u32,
            usec: (sent_us % 1_000_000) as u32,
        },
    }
    .encode(&mut datagram)
    .unwrap();
    datagram
}

#[test]
fn a_udp_datagram_must_fit_the_buffer_and_hold_the_header() {
    let mut buffer = [0; 1000];
    let config = Config {
        buf_len: 1001,
        ..Config::udp()
    };
    assert!(matches!(
        Client::new(config, &mut buffer),
        Err(ConfigError::BufLenTooLong)
    ));
    let config = Config {
        buf_len: 79,
        ..Config::udp()
    };
    assert!(matches!(
        Client::new(config, &mut buffer),
        Err(ConfigError::BufLenTooShort)
    ));
    let config = Config {
        buf_len: 80,
        ..Config::udp()
    };
    assert!(Client::new(config, &mut buffer).is_ok());
    let config = Config {
        buf_len: 1000,
        ..Config::udp()
    };
    assert!(Client::new(config, &mut buffer).is_ok());
    assert!(matches!(
        Client::new(Config::tcp(), &mut []),
        Err(ConfigError::BufLenTooShort)
    ));
}

#[test]
fn a_client_returns_the_settings_it_was_made_with() {
    let config = Config {
        reverse: true,
        length: Length::Bytes(7),
        buf_len: 100,
        rate: 9,
        interval_us: Some(11),
        port: 13,
        ..Config::udp()
    };
    let mut buffer = [0; 100];
    assert_eq!(Client::new(config, &mut buffer).unwrap().config(), config);
}

#[test]
fn the_defaults_are_those_of_a_stock_client() {
    let tcp = Config::tcp();
    assert_eq!(
        (tcp.length, tcp.buf_len, tcp.rate, tcp.port),
        (Length::Time { us: 10_000_000 }, 131_072, 0, 5001)
    );
    let udp = Config::udp();
    assert_eq!(
        (udp.transport, udp.buf_len, udp.rate),
        (Transport::Udp, 1470, 1_048_576)
    );
}

#[test]
fn tcp_sends_the_header_then_the_byte_count_then_closes() {
    let mut buffer = [0; 4096];
    let mut client = Client::new(tcp_bytes(10_000), &mut buffer).unwrap();
    let (chunks, next) = send_all(&mut client, 0);
    assert_eq!(next, "Close");
    let stream = chunks.concat();
    assert_eq!(
        stream.len(),
        10_000,
        "the header counts toward the byte count (P-1)"
    );
    let header = TestHeader::parse(&stream).unwrap();
    assert_eq!(header.flags, 0x4001_0080);
    assert_eq!(
        (header.num_threads, header.port, header.buf_len),
        (1, 5001, 16384)
    );
    assert_eq!(header.amount, Amount::Bytes(10_000));
    assert_eq!((header.upper_flags, header.rate), (0, 0));
    assert_eq!(
        (header.version_u, header.version_l),
        (0x0002_0002, 0x0002_0003)
    );
}

#[test]
fn tcp_reads_and_discards_until_the_server_closes() {
    let mut buffer = [0; 4096];
    let mut client = Client::new(tcp_bytes(64), &mut buffer).unwrap();
    send_all(&mut client, 0);
    assert_eq!(
        client.poll(0),
        Action::Wait {
            until_us: Some(2_000_000)
        }
    );
    let mut ack = [0; HEADER_ACK_LEN];
    HeaderAck {
        version_u: 0x0002_0002,
        version_l: 0x0002_0003,
    }
    .encode(&mut ack)
    .unwrap();
    client.received(10, &ack);
    assert_eq!(
        client.poll(20),
        Action::Wait {
            until_us: Some(2_000_000)
        }
    );
    client.closed(500);
    let last = report(client.poll(500));
    assert!(last.final_report);
    assert_eq!((last.bytes, last.end_us), (64, 0));
    assert_eq!(client.poll(500), Action::Done(Ok(())));
}

#[test]
fn tcp_gives_up_on_the_close_after_the_larger_of_the_duration_and_2_s() {
    let mut buffer = [0; 4096];
    let config = Config {
        length: Length::Time { us: 3_000_000 },
        ..tcp_bytes(0)
    };
    let mut client = Client::new(config, &mut buffer).unwrap();
    let first = send(client.poll(0));
    client.sent(first.len());
    assert_eq!(client.poll(3_000_000), Action::Close);
    assert_eq!(
        client.poll(3_000_000),
        Action::Wait {
            until_us: Some(6_000_000)
        }
    );
    assert!(report(client.poll(6_000_000)).final_report);
    assert_eq!(client.poll(6_000_000), Action::Done(Ok(())));
}

#[test]
fn tcp_in_time_mode_sends_until_the_duration() {
    let mut buffer = [0; 1000];
    let config = Config {
        length: Length::Time { us: 2_000_000 },
        ..tcp_bytes(0)
    };
    let mut client = Client::new(config, &mut buffer).unwrap();
    let first = send(client.poll(0));
    assert_eq!(first.len(), 1000);
    assert_eq!(
        TestHeader::parse(&first).unwrap().amount,
        Amount::Time { centiseconds: 200 }
    );
    client.sent(1000);
    assert_eq!(send(client.poll(1_999_999)).len(), 1000);
    client.sent(400);
    assert_eq!(client.poll(2_000_000), Action::Close);
    client.closed(2_000_000);
    assert_eq!(report(client.poll(2_000_000)).bytes, 1400);
}

#[test]
fn a_short_tcp_write_sends_the_rest_next() {
    let mut buffer = [0; 100];
    let mut client = Client::new(tcp_bytes(200), &mut buffer).unwrap();
    let first = send(client.poll(0));
    client.sent(10);
    let second = send(client.poll(0));
    assert_eq!(second[..54], first[10..64]);
}

#[test]
fn tcp_with_a_rate_waits_for_it() {
    let mut buffer = [0; 1000];
    let config = Config {
        rate: 8_000_000,
        ..tcp_bytes(10_000)
    };
    let mut client = Client::new(config, &mut buffer).unwrap();
    let first = send(client.poll(0));
    client.sent(first.len());
    assert_eq!(
        client.poll(0),
        Action::Wait {
            until_us: Some(1_000)
        }
    );
    assert_eq!(send(client.poll(1_000)).len(), 1000);
    let header = TestHeader::parse(&first).unwrap();
    assert_eq!(header.rate, 8_000_000);
}

#[test]
fn tcp_reports_each_interval_while_it_sends() {
    let mut buffer = [0; 1000];
    let config = Config {
        length: Length::Time { us: 2_000_000 },
        interval_us: Some(1_000_000),
        rate: 8_000_000,
        ..tcp_bytes(0)
    };
    let mut client = Client::new(config, &mut buffer).unwrap();
    let mut now = 0;
    let mut intervals = Vec::new();
    for _ in 0..LIMIT {
        match client.poll(now) {
            Action::Send(bytes) => {
                let length = bytes.len();
                client.sent(length);
            }
            Action::Wait { until_us } => now = until_us.unwrap(),
            Action::Report(report) => intervals.push((report.start_us, report.bytes)),
            Action::Close => break,
            Action::Done(outcome) => panic!("{outcome:?}"),
        }
    }
    assert_eq!(
        intervals,
        [(0, 1_000_000), (1_000_000, 1_000_000)],
        "1 MB per second at 8 Mbit/s"
    );
}

#[test]
fn udp_sends_numbered_datagrams_with_the_header_in_each() {
    let mut buffer = [0; 2000];
    let mut client = Client::new(udp_bytes(2000), &mut buffer).unwrap();
    let datagrams = send_data(&mut client, 7_000_001);
    assert_eq!(ids(&datagrams), [1, 2, 3, 4]);
    for datagram in &datagrams {
        let prefix = Prefix::parse(datagram).unwrap();
        assert_eq!(prefix.sent, Timestamp { sec: 7, usec: 1 });
        let header = TestHeader::parse(&datagram[PREFIX_LEN..]).unwrap();
        assert_eq!(header.flags, 0x4801_00A0);
        assert_eq!(
            (header.buf_len, header.amount, header.rate),
            (512, Amount::Bytes(2000), 0)
        );
    }
    let lengths: Vec<usize> = datagrams.iter().map(Vec::len).collect();
    assert_eq!(
        lengths,
        [512, 512, 512, 464],
        "the last is cut to the bytes left"
    );
}

#[test]
fn the_last_udp_datagram_is_never_shorter_than_the_header() {
    let mut buffer = [0; 2000];
    let mut client = Client::new(udp_bytes(520), &mut buffer).unwrap();
    let datagrams = send_data(&mut client, 0);
    let lengths: Vec<usize> = datagrams.iter().map(Vec::len).collect();
    assert_eq!(lengths, [512, 80]);
}

#[test]
fn udp_paces_the_datagrams_to_the_rate() {
    let mut buffer = [0; 2000];
    let config = Config {
        length: Length::Time { us: 1_000_000 },
        rate: 409_600,
        ..udp_bytes(0)
    };
    let mut client = Client::new(config, &mut buffer).unwrap();
    let driven = drive(&mut client, 0);
    let data = ids(&driven.sends).iter().filter(|id| **id > 0).count();
    assert_eq!(data, 100, "512 bytes every 10 ms for 1 s (P-2)");
    let early = send_data(&mut Client::new(config, &mut [0; 2000]).unwrap(), 0);
    assert_eq!(early.len(), 1, "the second datagram waits for its time");
}

#[test]
fn udp_ends_with_a_negative_id_and_repeats_it_until_the_report() {
    let mut buffer = [0; 2000];
    let mut client = Client::new(udp_bytes(1024), &mut buffer).unwrap();
    send_data(&mut client, 0);
    let end = send(client.poll(100));
    assert_eq!(end.len(), 512);
    assert_eq!(Prefix::parse(&end).unwrap().packet_id, -3);
    client.sent(512);
    assert_eq!(
        client.poll(100),
        Action::Wait {
            until_us: Some(10_100)
        }
    );
    let again = send(client.poll(10_100));
    assert_eq!(Prefix::parse(&again).unwrap().packet_id, -4);
    client.sent(512);

    let mut echo = again.clone();
    client.received(10_200, &echo[..100]);
    echo.truncate(SERVER_REPORT_LEN);
    client.received(10_200, &echo);
    assert!(
        matches!(client.poll(10_200), Action::Wait { .. }),
        "an echo is not a report (P-3)"
    );

    let server = ServerReport {
        bytes: 1536,
        duration: Timestamp { sec: 0, usec: 90 },
        lost: 0,
        out_of_order: 0,
        datagrams: 3,
        jitter: Timestamp { sec: 0, usec: 5 },
    };
    let mut bytes = [0; SERVER_REPORT_LEN];
    server.encode(&mut bytes).unwrap();
    client.received(10_300, &bytes);
    let last = report(client.poll(10_300));
    // The end of the test counts once, as a stock client counts it, so the bytes equal the server report's.
    assert_eq!(
        (last.final_report, last.bytes, last.server),
        (true, 1536, Some(server))
    );
    assert_eq!(client.poll(10_300), Action::Done(Ok(())));
}

#[test]
fn udp_without_a_report_after_200_sends_ends_without_one() {
    let mut buffer = [0; 2000];
    let mut client = Client::new(udp_bytes(512), &mut buffer).unwrap();
    send_data(&mut client, 0);
    let driven = drive(&mut client, 0);
    assert_eq!(ids(&driven.sends), (-201..=-2).rev().collect::<Vec<_>>());
    assert_eq!(driven.now, 2_000_000);
    assert_eq!(driven.reports.len(), 1);
    assert_eq!(driven.reports[0].server, None);
    assert_eq!(
        driven.reports[0].bytes, 1024,
        "one datagram, and 200 ends that count once"
    );
    assert_eq!(driven.outcome, Ok(()), "P-4");
}

#[test]
fn reverse_tcp_sends_the_header_and_reads_bursts() {
    let mut buffer = [0; 4096];
    let config = Config {
        reverse: true,
        ..tcp_bytes(40_000)
    };
    let mut client = Client::new(config, &mut buffer).unwrap();
    let (chunks, next) = send_all(&mut client, 0);
    let header = TestHeader::parse(&chunks.concat()).unwrap();
    assert_eq!(chunks.concat().len(), 64);
    assert_eq!(
        (header.flags, header.upper_flags),
        (0x4401_0080, flags::REVERSE)
    );
    assert!(next.starts_with("Wait"));
    for burst_id in 1..=3_u32 {
        let size = if burst_id < 3 {
            16384
        } else {
            40_000 - 2 * 16384
        };
        for piece in burst(burst_id, size).chunks(1000) {
            client.received(5, piece);
        }
    }
    let last = report(client.poll(10));
    assert_eq!(
        (last.bytes, last.final_report),
        (40_000, true),
        "byte mode ends at the count"
    );
    assert_eq!(client.poll(10), Action::Done(Ok(())));
}

#[test]
fn reverse_tcp_reads_a_burst_header_that_arrives_a_byte_at_a_time() {
    let mut buffer = [0; 4096];
    let config = Config {
        reverse: true,
        ..tcp_bytes(240)
    };
    let mut client = Client::new(config, &mut buffer).unwrap();
    send_all(&mut client, 0);
    for byte in [burst(1, 120), burst(2, 120)].concat() {
        client.received(5, &[byte]);
    }
    let last = report(client.poll(10));
    assert_eq!(last.bytes, 240, "a burst of exactly its header is valid");
}

#[test]
fn reverse_tcp_refuses_a_burst_shorter_than_its_header() {
    let mut buffer = [0; 4096];
    let config = Config {
        reverse: true,
        ..tcp_bytes(40_000)
    };
    let mut client = Client::new(config, &mut buffer).unwrap();
    send_all(&mut client, 0);
    let mut short = vec![0; BURST_HEADER_LEN];
    BurstHeader {
        start: Timestamp::default(),
        write: Timestamp::default(),
        burst_id: 1,
        burst_size: 119,
        seqno: 0,
    }
    .encode(&mut short)
    .unwrap();
    client.received(5, &short);
    assert_eq!(client.poll(5), Action::Done(Err(TestError::Protocol)));
}

#[test]
fn reverse_tcp_in_time_mode_ends_at_the_close() {
    let mut buffer = [0; 4096];
    let mut client = Client::new(reverse_tcp_time(1_000_000), &mut buffer).unwrap();
    send_all(&mut client, 0);
    assert_eq!(
        client.poll(0),
        Action::Wait {
            until_us: Some(3_000_000)
        }
    );
    client.closed(1_000_000);
    assert_eq!(report(client.poll(1_000_000)).bytes, 0);
}

#[test]
fn reverse_tcp_in_time_mode_gives_up_two_seconds_after_the_duration() {
    let mut buffer = [0; 4096];
    let mut client = Client::new(reverse_tcp_time(1_000_000), &mut buffer).unwrap();
    send_all(&mut client, 0);
    client.received(10, &burst(1, 200));
    assert_eq!(
        client.poll(2_999_999),
        Action::Wait {
            until_us: Some(3_000_000)
        }
    );
    let last = report(client.poll(3_000_000));
    assert_eq!((last.bytes, last.final_report), (200, true));
}

#[test]
fn reverse_tcp_reports_each_interval_it_receives() {
    let mut buffer = [0; 4096];
    let config = Config {
        interval_us: Some(1_000_000),
        ..reverse_tcp_time(3_000_000)
    };
    let mut client = Client::new(config, &mut buffer).unwrap();
    send_all(&mut client, 0);
    client.received(0, &burst(1, 300));
    client.received(1_500_000, &burst(2, 200));
    let first = report(client.poll(1_500_000));
    let second = report(client.poll(2_000_000));
    assert_eq!(
        (first.start_us, first.bytes, second.start_us, second.bytes),
        (0, 300, 1_000_000, 200)
    );
}

#[test]
fn reverse_udp_repeats_the_header_datagram_until_data_arrives() {
    let mut buffer = [0; 2000];
    let mut client = Client::new(reverse_udp_time(2_000_000), &mut buffer).unwrap();
    let first = send(client.poll(0));
    assert_eq!(Prefix::parse(&first).unwrap().packet_id, 1);
    let header = TestHeader::parse(&first[PREFIX_LEN..]).unwrap();
    assert_eq!(
        (header.flags, header.upper_flags),
        (0xEC01_00A0, flags::REVERSE)
    );
    assert!(header.is_reverse());
    client.sent(first.len());
    assert_eq!(
        client.poll(0),
        Action::Wait {
            until_us: Some(1_000_000)
        }
    );
    assert_eq!(send(client.poll(1_000_000)), {
        let mut again = first.clone();
        again[4..12].copy_from_slice(&[0, 0, 0, 1, 0, 0, 0, 0]);
        again
    });
}

#[test]
fn reverse_udp_without_data_for_10_s_fails() {
    let mut buffer = [0; 2000];
    let config = Config {
        reverse: true,
        ..udp_bytes(5000)
    };
    let mut client = Client::new(config, &mut buffer).unwrap();
    let driven = drive(&mut client, 0);
    assert_eq!(driven.sends.len(), 10, "one each second");
    assert_eq!(driven.outcome, Err(TestError::NoData));
    assert_eq!(driven.now, 10_000_000);
}

#[test]
fn reverse_udp_counts_the_datagrams_and_ends_at_a_negative_id() {
    let mut buffer = [0; 2000];
    let mut client = Client::new(reverse_udp_time(2_000_000), &mut buffer).unwrap();
    let first = send(client.poll(0));
    client.sent(first.len());
    for (id, at) in [(1, 100), (2, 200), (0, 250), (4, 300), (-5, 400)] {
        client.received(at + 50, &server_datagram(id, at));
    }
    let last = report(client.poll(500));
    let udp = last.udp.unwrap();
    assert_eq!((udp.datagrams, udp.lost, udp.out_of_order), (5, 1, 0));
    assert_eq!(
        (last.bytes, last.end_us),
        (4 * 512, 300),
        "an ID of 0 neither counts nor ends the test"
    );
    assert_eq!(
        client.poll(500),
        Action::Done(Ok(())),
        "no server report (P-23)"
    );
}

#[test]
fn reverse_udp_gives_up_two_seconds_after_the_duration() {
    let mut buffer = [0; 2000];
    let mut client = Client::new(reverse_udp_time(2_000_000), &mut buffer).unwrap();
    let first = send(client.poll(0));
    client.sent(first.len());
    client.received(100, &server_datagram(1, 0));
    assert_eq!(
        client.poll(200),
        Action::Wait {
            until_us: Some(4_000_100)
        }
    );
    assert!(report(client.poll(4_000_100)).final_report);
}

#[test]
fn reverse_udp_reports_the_counts_of_each_interval() {
    let mut buffer = [0; 2000];
    let config = Config {
        interval_us: Some(1_000_000),
        ..reverse_udp_time(3_000_000)
    };
    let mut client = Client::new(config, &mut buffer).unwrap();
    let first = send(client.poll(0));
    client.sent(first.len());
    client.received(0, &server_datagram(1, 0));
    client.received(500_000, &server_datagram(3, 500_000));
    assert!(matches!(client.poll(600_000), Action::Wait { .. }));
    client.received(1_500_000, &server_datagram(4, 1_500_000));
    let first = report(client.poll(1_500_000));
    let second = report(client.poll(2_000_000));
    let counts = |report: &Report| {
        let udp = report.udp.unwrap();
        (report.bytes, udp.datagrams, udp.lost)
    };
    assert_eq!(counts(&first), (1024, 3, 1));
    assert_eq!(counts(&second), (512, 1, 0));
}

#[test]
fn an_interval_reports_the_bytes_sent_in_it() {
    let mut buffer = [0; 2000];
    let config = Config {
        length: Length::Time { us: 3_000_000 },
        interval_us: Some(1_000_000),
        rate: 4_096_000,
        ..udp_bytes(0)
    };
    let mut client = Client::new(config, &mut buffer).unwrap();
    let driven = drive(&mut client, 0);
    let spans: Vec<(u64, u64, u64, bool)> = driven
        .reports
        .iter()
        .map(|r| (r.start_us, r.end_us, r.bytes, r.final_report))
        .collect();
    assert_eq!(
        spans,
        [
            (0, 1_000_000, 1000 * 512, false),
            (1_000_000, 2_000_000, 1000 * 512, false),
            (2_000_000, 3_000_000, 1000 * 512, false),
            // The end of the test goes out after the last interval closes, and counts in the final report.
            (0, 3_000_000, 3001 * 512, true)
        ]
    );
}

#[test]
fn the_stock_reverse_tcp_server_bytes_end_our_client_test() {
    let sample = common::read("tcp-reverse");
    let mut buffer = [0; 4096];
    let config = Config {
        reverse: true,
        ..tcp_bytes(65536)
    };
    let mut client = Client::new(config, &mut buffer).unwrap();
    send_all(&mut client, 0);
    for record in &sample.server {
        client.received(10, record);
    }
    let last = report(client.poll(10));
    assert_eq!(last.bytes, 65536);
    assert_eq!(client.poll(10), Action::Done(Ok(())));
}

#[test]
fn a_tcp_client_ends_with_its_report_when_the_server_closes_while_it_sends() {
    let mut buffer = [0; 1000];
    let config = Config {
        length: Length::Time { us: 10_000_000 },
        ..tcp_bytes(0)
    };
    let mut client = Client::new(config, &mut buffer).unwrap();
    let first = send(client.poll(0));
    client.sent(first.len());
    client.closed(500);
    let last = report(client.poll(500));
    assert_eq!((last.bytes, last.final_report), (1000, true), "P-24");
    assert_eq!(client.poll(500), Action::Done(Ok(())));
}

#[test]
fn a_close_before_the_reverse_header_is_out_is_a_refusal() {
    let mut buffer = [0; 10];
    let config = Config {
        reverse: true,
        ..tcp_bytes(1000)
    };
    let mut client = Client::new(config, &mut buffer).unwrap();
    let first = send(client.poll(0));
    client.sent(first.len());
    client.closed(10);
    assert_eq!(
        client.poll(10),
        Action::Done(Err(TestError::Refused)),
        "P-24"
    );
}

#[test]
fn reverse_udp_in_byte_mode_ends_10_s_after_the_last_datagram() {
    let mut buffer = [0; 2000];
    let config = Config {
        reverse: true,
        ..udp_bytes(5000)
    };
    let mut client = Client::new(config, &mut buffer).unwrap();
    let ask = send(client.poll(0));
    client.sent(ask.len());
    client.received(100, &server_datagram(1, 0));
    client.received(300, &server_datagram(2, 200));
    assert_eq!(
        client.poll(400),
        Action::Wait {
            until_us: Some(10_000_300)
        }
    );
    let last = report(client.poll(10_000_300));
    assert_eq!((last.bytes, last.final_report), (1024, true), "P-22");
}

#[test]
fn reverse_tcp_in_byte_mode_ends_10_s_after_the_last_byte() {
    let mut buffer = [0; 4096];
    let config = Config {
        reverse: true,
        ..tcp_bytes(40_000)
    };
    let mut client = Client::new(config, &mut buffer).unwrap();
    send_all(&mut client, 0);
    client.received(700, &burst(1, 300));
    assert_eq!(
        client.poll(800),
        Action::Wait {
            until_us: Some(10_000_700)
        }
    );
    assert_eq!(report(client.poll(10_000_700)).bytes, 300, "P-5");
}

#[test]
fn reverse_udp_does_not_start_on_a_datagram_a_server_would_ignore() {
    let mut buffer = [0; 2000];
    let mut client = Client::new(reverse_udp_time(2_000_000), &mut buffer).unwrap();
    let ask = send(client.poll(0));
    client.sent(ask.len());
    client.received(10, &server_datagram(1, 0)[..39]);
    client.received(20, &server_datagram(0, 0));
    assert_eq!(
        client.poll(30),
        Action::Wait {
            until_us: Some(1_000_000)
        },
        "still asking (P-22, P-11)"
    );
}

#[test]
fn the_last_part_of_an_interval_is_reported_before_the_final_report() {
    let mut buffer = [0; 2000];
    let config = Config {
        length: Length::Time { us: 2_500_000 },
        interval_us: Some(1_000_000),
        rate: 4_096_000,
        ..udp_bytes(0)
    };
    let mut client = Client::new(config, &mut buffer).unwrap();
    let driven = drive(&mut client, 0);
    let spans: Vec<(u64, u64, u64, bool)> = driven
        .reports
        .iter()
        .map(|r| (r.start_us, r.end_us, r.bytes, r.final_report))
        .collect();
    assert_eq!(
        spans,
        [
            (0, 1_000_000, 1000 * 512, false),
            (1_000_000, 2_000_000, 1000 * 512, false),
            (2_000_000, 2_500_000, 501 * 512, false),
            (0, 2_500_000, 2501 * 512, true)
        ],
        "P-19"
    );
}

#[test]
fn every_interval_of_a_stall_is_reported_empty() {
    let mut buffer = [0; 2000];
    let config = Config {
        interval_us: Some(1_000_000),
        ..reverse_udp_time(10_000_000)
    };
    let mut client = Client::new(config, &mut buffer).unwrap();
    let ask = send(client.poll(0));
    client.sent(ask.len());
    client.received(0, &server_datagram(1, 0));
    client.received(3_500_000, &server_datagram(2, 3_500_000));
    let mut intervals = Vec::new();
    for now in [3_500_000, 3_500_000, 3_500_000, 4_000_000] {
        let interval = report(client.poll(now));
        intervals.push((
            interval.start_us,
            interval.bytes,
            interval.udp.unwrap().datagrams,
        ));
    }
    assert_eq!(
        intervals,
        [
            (0, 512, 1),
            (1_000_000, 0, 0),
            (2_000_000, 0, 0),
            (3_000_000, 512, 1)
        ],
        "P-19"
    );
}

#[test]
fn a_reverse_udp_test_reports_the_counts_of_its_last_part() {
    let mut buffer = [0; 2000];
    let config = Config {
        interval_us: Some(1_000_000),
        ..reverse_udp_time(3_000_000)
    };
    let mut client = Client::new(config, &mut buffer).unwrap();
    let ask = send(client.poll(0));
    client.sent(ask.len());
    client.received(0, &server_datagram(1, 0));
    client.received(1_200_000, &server_datagram(3, 1_200_000));
    client.received(1_400_000, &server_datagram(-4, 1_400_000));
    let first = report(client.poll(1_400_000));
    let tail = report(client.poll(1_400_000));
    assert_eq!((first.end_us, first.bytes), (1_000_000, 512));
    let udp = tail.udp.unwrap();
    assert_eq!(
        (
            tail.start_us,
            tail.end_us,
            tail.bytes,
            udp.datagrams,
            udp.lost
        ),
        (1_000_000, 1_400_000, 1024, 3, 1)
    );
    assert!(report(client.poll(1_400_000)).final_report);
}

#[test]
fn the_stock_reverse_udp_server_datagrams_count_without_loss() {
    let sample = common::read("udp-reverse");
    let mut buffer = [0; 2000];
    let mut client = Client::new(reverse_udp_time(2_000_000), &mut buffer).unwrap();
    let ask = send(client.poll(0));
    client.sent(ask.len());
    for (at, datagram) in sample.server.iter().enumerate() {
        client.received(at as u64 * 1000, datagram);
    }
    let last = report(client.poll(1_000_000));
    let udp = last.udp.unwrap();
    assert_eq!(udp.datagrams, sample.server.len() as u64);
    assert_eq!((udp.lost, udp.out_of_order), (0, 0));
    assert_eq!(last.bytes, sample.server.len() as u64 * 512);
}
