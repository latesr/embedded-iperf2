//! Every message in every sample recorded from iperf 2.2.2 parses, with the fields the format fixes (T-1).
//!
//! The samples' `README.md` lists the client arguments of each case, which set the expected fields.

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
    SERVER_REPORT_LEN, ServerReport, TEST_HEADER_LEN, TestHeader, flags, header_length,
};

const VERSION: HeaderAck = HeaderAck {
    version_u: 0x0002_0002,
    version_l: 0x0002_0003,
};

/// The fields every 2.2.2 test header in the samples shares, and the port the client connected to.
fn check_common(header: &TestHeader, port: u32) {
    assert_eq!(header.port, port);
    assert_eq!(header.num_threads, 1);
    assert_eq!(header.lower_flags, 0);
    assert_eq!(
        (header.version_u, header.version_l),
        (VERSION.version_u, VERSION.version_l)
    );
}

/// Checks the test header at the start of a TCP sample's client stream, and returns it.
fn tcp_header(sample: &common::Sample, flags_word: u32, amount: Amount, rate: u64) -> TestHeader {
    let header = TestHeader::parse(&sample.client_stream()).unwrap();
    assert_eq!(header.flags, flags_word);
    assert_eq!(
        header_length(header.flags, false),
        Ok(Some(TEST_HEADER_LEN))
    );
    assert_eq!(header.buf_len, 16384);
    assert_eq!(header.amount, amount);
    assert_eq!(header.rate, rate);
    check_common(&header, sample.port);
    header
}

#[test]
fn tcp_sends_the_header_and_the_server_acks_it() {
    let sample = common::read("tcp");
    let stream = sample.client_stream();
    let header = tcp_header(&sample, 0x4001_0080, Amount::Bytes(65536), 0);
    assert_eq!(header.upper_flags, 0);
    assert!(!header.is_reverse());
    assert_eq!(stream.len(), 65536, "the header counts toward -n");
    let reply = sample.server_stream();
    assert_eq!(reply.len(), HEADER_ACK_LEN);
    assert_eq!(HeaderAck::parse(&reply), Ok(VERSION));
}

#[test]
fn tcp_time_sends_the_duration_and_the_rate() {
    let sample = common::read("tcp-time");
    let header = tcp_header(
        &sample,
        0x4001_0080,
        Amount::Time { centiseconds: 200 },
        262_144,
    );
    assert_eq!(header.upper_flags, 0);
    assert_eq!(HeaderAck::parse(&sample.server_stream()), Ok(VERSION));
}

#[test]
fn tcp_reverse_data_comes_in_bursts_without_an_ack() {
    let sample = common::read("tcp-reverse");
    let stream = sample.client_stream();
    let header = tcp_header(&sample, 0x4401_0080, Amount::Bytes(65536), 0);
    assert_eq!(header.upper_flags, flags::REVERSE);
    assert!(header.is_reverse());
    assert_eq!(stream.len(), TEST_HEADER_LEN);

    let data = sample.server_stream();
    assert_eq!(data.len(), 65536);
    let mut at = 0;
    let mut seqno = 1;
    let mut start = None;
    for burst_id in 1.. {
        if at == data.len() {
            assert_eq!(burst_id, 5);
            break;
        }
        let burst = BurstHeader::parse(&data[at..]).unwrap();
        assert_eq!(burst.burst_id, burst_id);
        assert_eq!(burst.burst_size, 16384);
        seqno += u64::from(burst.burst_size);
        assert_eq!(burst.seqno, seqno);
        assert_eq!(*start.get_or_insert(burst.start), burst.start);
        assert!(burst.burst_size as usize >= BURST_HEADER_LEN);
        at += burst.burst_size as usize;
    }
}

/// Checks the datagrams a UDP sender sent: IDs from 1, then the end of the test. Returns the count of datagrams
/// of data.
fn check_ids(datagrams: &[Vec<u8>]) -> i64 {
    let ids: Vec<i64> = datagrams
        .iter()
        .map(|datagram| Prefix::parse(datagram).unwrap().packet_id)
        .collect();
    let data = ids.iter().take_while(|id| **id > 0).count();
    assert!(data > 0);
    for (index, id) in ids.iter().enumerate() {
        let expected = if index < data {
            index as i64 + 1
        } else {
            -(index as i64 + 1)
        };
        assert_eq!(*id, expected, "datagram {index}");
    }
    assert!(ids.len() > data, "the test ends with a negative ID");
    data as i64
}

/// Checks the test header of a UDP client's datagram, sent to `port`.
fn udp_header(datagram: &[u8], port: u32, flags_word: u32, amount: Amount) -> TestHeader {
    assert_eq!(datagram.len(), 512);
    let header = TestHeader::parse(&datagram[PREFIX_LEN..]).unwrap();
    assert_eq!(header.flags, flags_word);
    assert_eq!(
        header_length(header.flags, true),
        Ok(Some(PREFIX_LEN + TEST_HEADER_LEN))
    );
    assert_eq!(header.buf_len, 512);
    assert_eq!(header.amount, amount);
    assert_eq!(header.rate, 262_144);
    check_common(&header, port);
    header
}

/// Checks a forward UDP case: the header in every datagram, the IDs, and the server report.
fn check_forward_udp(case: &str, amount: Amount) {
    let sample = common::read(case);
    for datagram in &sample.client {
        let header = udp_header(datagram, sample.port, 0x4801_00A0, amount);
        assert_eq!(header.upper_flags, 0);
    }
    let data = check_ids(&sample.client);
    let report = sample.server.first().expect("a server report");
    assert_eq!(report.len(), SERVER_REPORT_LEN);
    assert_eq!(Prefix::parse(report).unwrap().packet_id, 0);
    let report = ServerReport::parse(report).unwrap();
    assert_eq!(
        report.datagrams,
        data as u64 + 1,
        "the end of the test counts as a datagram"
    );
    assert_eq!(report.bytes, report.datagrams * 512);
    assert_eq!((report.lost, report.out_of_order), (0, 0));
}

#[test]
fn udp_repeats_the_header_and_the_server_reports() {
    check_forward_udp("udp", Amount::Time { centiseconds: 200 });
}

#[test]
fn udp_bytes_sends_the_byte_count() {
    check_forward_udp("udp-bytes", Amount::Bytes(16384));
}

#[test]
fn udp_reverse_data_has_a_prefix_and_no_header() {
    let sample = common::read("udp-reverse");
    for datagram in &sample.client {
        let header = udp_header(
            datagram,
            sample.port,
            0xEC01_00A0,
            Amount::Time { centiseconds: 200 },
        );
        assert_eq!(header.upper_flags, flags::REVERSE);
        assert!(header.is_reverse());
        assert_eq!(Prefix::parse(datagram).unwrap().packet_id, 1);
    }
    check_ids(&sample.server);
    for datagram in &sample.server {
        assert_eq!(datagram.len(), 512);
        assert_eq!(
            datagram[PREFIX_LEN..PREFIX_LEN + 4],
            *b"6789",
            "fill, without a header"
        );
    }
}
