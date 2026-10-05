//! Parses arbitrary bytes as every message, from the start and from after a UDP prefix. The properties are that no
//! parser panics, and that a message that parses encodes and parses back to the same value.

#![no_main]

use embedded_iperf2::wire::{
    BufferTooSmall, BurstHeader, Error, HeaderAck, PREFIX_LEN, Prefix, ServerReport, TestHeader, header_length,
};
use libfuzzer_sys::fuzz_target;

/// When `input` parses, encodes the value and checks that it parses back the same.
fn check<T: PartialEq + core::fmt::Debug>(
    input: &[u8],
    parse: impl Fn(&[u8]) -> Result<T, Error>,
    encode: impl Fn(&T, &mut [u8]) -> Result<usize, BufferTooSmall>,
) {
    if let Ok(value) = parse(input) {
        let mut out = [0; 256];
        let length = encode(&value, &mut out).expect("every message fits in 256 bytes");
        assert_eq!(parse(&out[..length]).as_ref(), Ok(&value));
    }
}

fuzz_target!(|data: &[u8]| {
    for input in [data, data.get(PREFIX_LEN..).unwrap_or_default()] {
        check(input, Prefix::parse, Prefix::encode);
        check(input, TestHeader::parse, TestHeader::encode);
        check(input, HeaderAck::parse, HeaderAck::encode);
        check(input, BurstHeader::parse, BurstHeader::encode);
        check(input, ServerReport::parse, ServerReport::encode);
        if let Some(word) = input.get(..4) {
            let flags = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
            for udp in [false, true] {
                if let Ok(Some(length)) = header_length(flags, udp) {
                    assert!(length <= 256);
                }
            }
        }
    }
});
