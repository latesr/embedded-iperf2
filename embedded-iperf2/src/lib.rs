//! An iperf 2 client and server for no_std targets, wire compatible with iperf 2.2.2.
//!
//! [`Client`] and [`Server`] are sans-I/O state machines. The caller owns the socket and the clock. It passes the
//! time in as microseconds, calls `poll` for the next [`Action`], writes the bytes it asks for, and passes in the
//! bytes the peer sends. `REQUIREMENTS.md` in the repository states what this crate must do.

#![cfg_attr(not(test), no_std)]

mod client;
mod config;
mod counts;
mod datagram;
mod meter;
mod report;
mod server;
mod stream;
pub mod wire;

pub use client::Client;
pub use config::{Action, Config, ConfigError, Length, ServerConfig, TestError, Transport};
pub use report::{Report, UdpCounts};
pub use server::Server;
