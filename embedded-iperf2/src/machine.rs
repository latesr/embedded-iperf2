//! The calls a driver makes, which [`Client`] and [`Server`] share, so one loop in each driver runs either.

use crate::{Action, Client, Server};

/// The calls a driver makes. A client never waits for its first datagram and never gives up its peer.
pub(crate) trait Machine {
    fn poll(&mut self, now_us: u64) -> Action<'_>;
    fn sent(&mut self, count: usize);
    fn received(&mut self, now_us: u64, bytes: &[u8]);
    fn closed(&mut self, now_us: u64);
    fn waits_for_first(&self) -> bool {
        false
    }
    fn another_flow_arrived(&mut self) -> bool {
        false
    }
}

impl Machine for Client<'_> {
    fn poll(&mut self, now_us: u64) -> Action<'_> {
        Client::poll(self, now_us)
    }
    fn sent(&mut self, count: usize) {
        Client::sent(self, count);
    }
    fn received(&mut self, now_us: u64, bytes: &[u8]) {
        Client::received(self, now_us, bytes);
    }
    fn closed(&mut self, now_us: u64) {
        Client::closed(self, now_us);
    }
}

impl Machine for Server<'_> {
    fn poll(&mut self, now_us: u64) -> Action<'_> {
        Server::poll(self, now_us)
    }
    fn sent(&mut self, count: usize) {
        Server::sent(self, count);
    }
    fn received(&mut self, now_us: u64, bytes: &[u8]) {
        Server::received(self, now_us, bytes);
    }
    fn closed(&mut self, now_us: u64) {
        Server::closed(self, now_us);
    }
    fn waits_for_first(&self) -> bool {
        Server::waits_for_first(self)
    }
    fn another_flow_arrived(&mut self) -> bool {
        Server::another_flow_arrived(self)
    }
}
