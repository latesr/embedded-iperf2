//! What a client or a server reports during and at the end of a test (P-19, P-20).

use crate::wire::ServerReport;

/// The bytes of one interval, or of the whole test.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Report {
    /// The start of the interval, in microseconds from the start of the test. The test starts at the first bytes
    /// this end sent or received (P-25).
    pub start_us: u64,
    /// The end of the interval, in microseconds from the start of the test. A final report ends where P-25 states,
    /// which can be later than the test length the client asked for.
    pub end_us: u64,
    /// The bytes this end sent or received in the interval.
    pub bytes: u64,
    /// The counts of a UDP receiver. `None` for a sender, and for TCP.
    pub udp: Option<UdpCounts>,
    /// Whether this report covers the whole test. The last report of a test is final.
    pub final_report: bool,
    /// The server report of a forward UDP test, in the client's final report when one arrived (P-4).
    pub server: Option<ServerReport>,
}

/// What a UDP receiver counted (P-17). `docs/concepts/loss-and-jitter.md` in the repository defines each count.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UdpCounts {
    /// The datagrams the sender sent, from the highest packet ID received.
    pub datagrams: u64,
    /// The datagrams that did not arrive.
    pub lost: u64,
    /// The datagrams that arrived after one with a higher packet ID.
    pub out_of_order: u64,
    /// The jitter in nanoseconds. A final report holds the mean of the running jitter over the test, as a stock
    /// server's report does.
    pub jitter_ns: u64,
}
