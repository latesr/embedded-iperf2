//! Runs the crate's client and the crate's server through the `std` driver on loopback, for the tests of the
//! driver. Each end runs on its own thread.

// Each test file compiles this module on its own and uses only part of it.
#![allow(dead_code)]

use embedded_iperf2::blocking::{TcpServer, UdpServer, run_client};
use embedded_iperf2::{Client, Config, Report, Server, ServerConfig, TestError, Transport};
use std::io;
use std::net::{SocketAddr, TcpListener, UdpSocket};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

/// The longest time that any end of a test can take. A driver that never ends fails its test after this time,
/// and does not hang the test.
const PATIENCE: Duration = Duration::from_secs(10);

/// Every report an end gave, and how its test ended.
pub struct End {
    pub reports: Vec<Report>,
    pub outcome: Result<(), TestError>,
}

impl End {
    pub fn last(&self) -> Report {
        let last = *self.reports.last().expect("no report");
        assert!(last.final_report, "the last report is not final");
        last
    }

    pub fn intervals(&self) -> usize {
        self.reports
            .iter()
            .filter(|report| !report.final_report)
            .count()
    }
}

/// The result of work on another thread.
pub struct Pending<T>(mpsc::Receiver<T>);

impl<T> Pending<T> {
    /// The result of the work. Panics when the work takes longer than `PATIENCE`, or when the work panics.
    pub fn get(self) -> T {
        self.0
            .recv_timeout(PATIENCE)
            .unwrap_or_else(|error| panic!("the work did not end: {error}"))
    }
}

pub fn spawn<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> Pending<T> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let _ = sender.send(work());
    });
    Pending(receiver)
}

pub fn server_config(transport: Transport) -> ServerConfig {
    ServerConfig {
        transport,
        duration_us: None,
        interval_us: None,
        ipv6: false,
    }
}

/// Starts the crate's server for one test on loopback, over IPv6 when `config` says so, and returns its address.
pub fn serve(config: ServerConfig) -> (SocketAddr, Pending<End>) {
    let host = if config.ipv6 {
        "[::1]:0"
    } else {
        "127.0.0.1:0"
    };
    match config.transport {
        Transport::Tcp => {
            let listener = TcpListener::bind(host).unwrap();
            let address = listener.local_addr().unwrap();
            let end = spawn(move || {
                let mut driver = TcpServer::new(listener);
                let mut buffer = vec![0; 4096];
                let mut server = Server::new(config, &mut buffer).unwrap();
                let mut reports = Vec::new();
                let outcome = driver
                    .run_one(&mut server, |report| reports.push(*report))
                    .unwrap();
                End { reports, outcome }
            });
            (address, end)
        }
        Transport::Udp => {
            let socket = UdpSocket::bind(host).unwrap();
            let address = socket.local_addr().unwrap();
            let end = spawn(move || {
                let mut driver = UdpServer::new(socket);
                let mut buffer = vec![0; 4096];
                let mut server = Server::new(config, &mut buffer).unwrap();
                let mut reports = Vec::new();
                let outcome = driver
                    .run_one(&mut server, |report| reports.push(*report))
                    .unwrap();
                End { reports, outcome }
            });
            (address, end)
        }
    }
}

/// Starts the crate's client of `config` against `server`.
pub fn start(config: Config, server: SocketAddr) -> Pending<End> {
    spawn(move || try_run(config, server).unwrap())
}

/// Runs the crate's client of `config` against `server` on this thread, and returns the error of a socket that
/// fails.
pub fn try_run(config: Config, server: SocketAddr) -> io::Result<End> {
    let mut buffer = vec![0; 4096];
    let mut client = Client::new(config, &mut buffer).unwrap();
    let mut reports = Vec::new();
    let outcome = run_client(&mut client, server, |report| reports.push(*report))?;
    Ok(End { reports, outcome })
}

/// Runs the crate's client of `config` against `server` to its end.
pub fn run(config: Config, server: SocketAddr) -> End {
    start(config, server).get()
}
