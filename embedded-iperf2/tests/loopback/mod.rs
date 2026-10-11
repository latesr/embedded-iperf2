//! Runs our client and our server through the `std` driver on loopback, each on its own thread, for the tests
//! of the driver.

// Each test file compiles this module on its own and uses only part of it.
#![allow(dead_code)]

use embedded_iperf2::blocking::{TcpServer, UdpServer, run_client};
use embedded_iperf2::{Client, Config, Report, Server, ServerConfig, TestError, Transport};
use std::net::{SocketAddr, TcpListener, UdpSocket};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

/// The longest any end of a test may take. A driver that never ends fails its test after this, rather than hang
/// it.
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
    /// The result, or a panic when the work takes longer than `PATIENCE` or panicked.
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

/// Starts our server for one test on loopback, over IPv6 when `config` says so, and returns its address.
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

/// Starts our client of `config` against `server`.
pub fn start(config: Config, server: SocketAddr) -> Pending<End> {
    spawn(move || {
        let mut buffer = vec![0; 4096];
        let mut client = Client::new(config, &mut buffer).unwrap();
        let mut reports = Vec::new();
        let outcome = run_client(&mut client, server, |report| reports.push(*report)).unwrap();
        End { reports, outcome }
    })
}

/// Runs our client of `config` against `server` to its end.
pub fn run(config: Config, server: SocketAddr) -> End {
    start(config, server).get()
}
