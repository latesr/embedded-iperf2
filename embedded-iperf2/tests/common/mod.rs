//! Reads the recorded samples. The rustdoc of `xtask/src/record/sample.rs` states the layout: each record is a
//! direction byte, a 32-bit big-endian length and that many bytes.

use std::path::PathBuf;

/// The bytes one side sent, in the order the relay saw them.
pub struct Sample {
    /// Each record from the client to the server. A UDP record is one datagram.
    pub client: Vec<Vec<u8>>,
    /// Each record from the server to the client.
    pub server: Vec<Vec<u8>>,
    /// The port the client connected to, from the line `Client connecting to 127.0.0.1, TCP port N` that it
    /// printed. It is the relay's port, and changes with each recording.
    pub port: u32,
}

impl Sample {
    /// Every record from the client, joined, as a TCP stream.
    pub fn client_stream(&self) -> Vec<u8> {
        self.client.concat()
    }

    /// Every record from the server, joined, as a TCP stream.
    pub fn server_stream(&self) -> Vec<u8> {
        self.server.concat()
    }
}

/// Reads the sample of `case` recorded from iperf 2.2.2.
pub fn read(case: &str) -> Sample {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("tests/samples/2.2.2/{case}.bin"));
    let bytes = std::fs::read(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let printed = path.with_extension("client.txt");
    let printed = std::fs::read_to_string(&printed)
        .unwrap_or_else(|error| panic!("{}: {error}", printed.display()));
    let port = printed
        .lines()
        .find_map(|line| line.strip_prefix("Client connecting to 127.0.0.1, "))
        .and_then(|rest| rest.split(" port ").nth(1))
        .and_then(|port| port.trim().parse().ok())
        .unwrap_or_else(|| panic!("{case}: no port in client.txt"));
    let mut sample = Sample {
        client: Vec::new(),
        server: Vec::new(),
        port,
    };
    let mut rest = bytes.as_slice();
    while let [direction, a, b, c, d, tail @ ..] = rest {
        let length = u32::from_be_bytes([*a, *b, *c, *d]) as usize;
        let (record, after) = tail.split_at(length);
        match direction {
            0 => sample.client.push(record.to_vec()),
            1 => sample.server.push(record.to_vec()),
            other => panic!("{case}: direction {other}"),
        }
        rest = after;
    }
    assert!(
        rest.is_empty(),
        "{case}: {} bytes after the last record",
        rest.len()
    );
    sample
}
