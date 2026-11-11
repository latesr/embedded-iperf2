//! An iperf 2 client and server for microcontrollers, wire compatible with iperf 2.2.2.
//!
//! The core is `no_std` and does not allocate memory. [`Client`] and [`Server`] are sans-I/O state machines. The
//! caller owns the socket and the clock. It passes the time in as microseconds, and calls `poll` for the next
//! [`Action`]. It writes the bytes that the action asks for, and passes in the bytes that the peer sends.
//!
//! Two drivers behind cargo features do the I/O:
//!
//! - `embassy-net` adds the `embassy` module, which runs a test on a board over an embassy-net stack.
//! - `std` adds the `blocking` module, which runs a test on a host over the standard library's blocking sockets.
//!
//! `REQUIREMENTS.md` in the repository states what the crate must do.

#![cfg_attr(not(test), no_std)]

#[cfg(feature = "std")]
extern crate std;

#[cfg(feature = "std")]
pub mod blocking;
mod client;
mod config;
mod counts;
mod datagram;
#[cfg(feature = "embassy-net")]
pub mod embassy;
#[cfg(any(feature = "std", feature = "embassy-net"))]
mod machine;
mod meter;
mod report;
mod server;
mod stream;
pub mod wire;

pub use client::Client;
pub use config::{Action, Config, ConfigError, Length, ServerConfig, TestError, Transport};
pub use report::{Report, UdpCounts};
pub use server::Server;
