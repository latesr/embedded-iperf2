//! Property tests for the wire format (T-3).
//!
//! Three properties hold for each message. Any bytes parse or fail without a panic. A message the encoder writes
//! parses back to the same value. Every slice shorter than the message is `Incomplete`.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use embedded_iperf2::wire::{
    Amount, BURST_HEADER_LEN, BurstHeader, Error, HEADER_ACK_LEN, HeaderAck, PREFIX_LEN, Prefix,
    SERVER_REPORT_LEN, ServerReport, TEST_HEADER_LEN, TestHeader, Timestamp, flags, header_length,
};
use proptest::prelude::*;

fn timestamp() -> impl Strategy<Value = Timestamp> {
    (any::<u32>(), any::<u32>()).prop_map(|(sec, usec)| Timestamp { sec, usec })
}

fn amount() -> impl Strategy<Value = Amount> {
    prop_oneof![
        (1..=0x8000_0000_u32).prop_map(|centiseconds| Amount::Time { centiseconds }),
        (0..0x8000_0000_u32).prop_map(Amount::Bytes),
    ]
}

fn test_header() -> impl Strategy<Value = TestHeader> {
    (
        any::<[u32; 6]>(),
        any::<[u16; 2]>(),
        amount(),
        0..(1_u64 << 56),
    )
        .prop_map(
            |(
                [flags, num_threads, port, buf_len, version_u, version_l],
                [upper_flags, lower_flags],
                amount,
                rate,
            )| {
                TestHeader {
                    flags,
                    num_threads,
                    port,
                    buf_len,
                    amount,
                    upper_flags,
                    lower_flags,
                    version_u,
                    version_l,
                    rate,
                }
            },
        )
}

fn burst_header() -> impl Strategy<Value = BurstHeader> {
    (
        timestamp(),
        timestamp(),
        any::<u32>(),
        any::<u32>(),
        any::<u64>(),
    )
        .prop_map(|(start, write, burst_id, burst_size, seqno)| BurstHeader {
            start,
            write,
            burst_id,
            burst_size,
            seqno,
        })
}

fn server_report() -> impl Strategy<Value = ServerReport> {
    (any::<[u64; 4]>(), timestamp(), timestamp()).prop_map(
        |([bytes, lost, out_of_order, datagrams], duration, jitter)| ServerReport {
            bytes,
            duration,
            lost,
            out_of_order,
            datagrams,
            jitter,
        },
    )
}

/// Encodes `value`, checks that it parses back, and that every shorter slice is `Incomplete`.
fn round_trip<T: PartialEq + core::fmt::Debug>(
    value: &T,
    len: usize,
    encode: impl Fn(&T, &mut [u8]) -> Result<usize, embedded_iperf2::wire::BufferTooSmall>,
    parse: impl Fn(&[u8]) -> Result<T, Error>,
) {
    let mut out = vec![0; len];
    assert_eq!(encode(value, &mut out), Ok(len));
    assert_eq!(parse(&out).as_ref(), Ok(value));
    for end in 0..len {
        assert_eq!(parse(&out[..end]), Err(Error::Incomplete), "{end} bytes");
    }
}

proptest! {
    #[test]
    fn any_bytes_parse_or_fail_without_a_panic(bytes in proptest::collection::vec(any::<u8>(), 0..160), udp: bool) {
        let _ = Prefix::parse(&bytes);
        let _ = TestHeader::parse(&bytes);
        let _ = HeaderAck::parse(&bytes);
        let _ = BurstHeader::parse(&bytes);
        let _ = ServerReport::parse(&bytes);
        if let Some(word) = bytes.get(..4) {
            let _ = header_length(u32::from_be_bytes(word.try_into().unwrap()), udp);
        }
    }

    #[test]
    fn a_prefix_round_trips(packet_id: i64, sent in timestamp()) {
        round_trip(&Prefix { packet_id, sent }, PREFIX_LEN, Prefix::encode, Prefix::parse);
    }

    #[test]
    fn a_test_header_round_trips(header in test_header()) {
        round_trip(&header, TEST_HEADER_LEN, TestHeader::encode, TestHeader::parse);
    }

    #[test]
    fn a_header_ack_round_trips(version_u: u32, version_l: u32) {
        round_trip(&HeaderAck { version_u, version_l }, HEADER_ACK_LEN, HeaderAck::encode, HeaderAck::parse);
    }

    #[test]
    fn a_burst_header_round_trips(header in burst_header()) {
        round_trip(&header, BURST_HEADER_LEN, BurstHeader::encode, BurstHeader::parse);
    }

    #[test]
    fn a_server_report_round_trips(report in server_report()) {
        round_trip(&report, SERVER_REPORT_LEN, ServerReport::encode, ServerReport::parse);
    }

    #[test]
    fn a_stated_length_is_in_bounds_or_invalid(flags: u32, udp: bool) {
        let minimum = if udp { 80 } else { 64 };
        match header_length(flags, udp) {
            Ok(Some(length)) => prop_assert!((minimum..=256).contains(&length)),
            Ok(None) => prop_assert!(
                flags & (flags::VERSION1 | flags::VERSION2 | flags::EXTEND) == 0
                    || (udp && flags == flags::UDP_COMPAT_FILL)
            ),
            Err(error) => prop_assert_eq!(error, Error::Invalid),
        }
    }
}
