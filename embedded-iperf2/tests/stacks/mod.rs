//! Two embassy-net stacks in one process, joined by an in-memory Ethernet link that records each frame. Our client
//! runs on one stack and our server on the other, through the `embassy-net` driver, on the host's clock.

// Each test file compiles this module on its own and uses only part of it.
#![allow(dead_code)]

use embassy_futures::block_on;
use embassy_futures::join::join;
use embassy_futures::select::{Either, select};
use embassy_net::tcp::TcpSocket;
use embassy_net::udp::{PacketMetadata, UdpSocket};
use embassy_net::{
    Config as NetConfig, Ipv4Address, Ipv4Cidr, Stack, StackResources, StaticConfigV4,
};
use embassy_net_driver::{Capabilities, Driver, HardwareAddress, LinkState, RxToken, TxToken};
use embassy_time::{Duration, Instant};
use embedded_iperf2::embassy::{Error, TcpServer, UdpServer, run_tcp_client, run_udp_client};
use embedded_iperf2::{Client, Config, Report, Server, ServerConfig, TestError, Transport};
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::task::{Context, Waker};
use std::thread;

/// The longest a test may take. A driver that never ends fails its test after this, rather than hang it.
const PATIENCE: std::time::Duration = std::time::Duration::from_secs(10);
pub const CLIENT: Ipv4Address = Ipv4Address::new(10, 0, 0, 2);
pub const SERVER: Ipv4Address = Ipv4Address::new(10, 0, 0, 1);
pub const PORT: u16 = 5001;
const MACS: [[u8; 6]; 2] = [[0x02, 0, 0, 0, 0, 2], [0x02, 0, 0, 0, 0, 1]];
const TCP_BUFFER: usize = 8192;
const UDP_BUFFER: usize = 16_384;
const UDP_PACKETS: usize = 32;

/// The frames in flight to each side, and every frame sent, with the side that sent it: 0 is the client's.
#[derive(Default)]
struct Wire {
    queues: [VecDeque<Vec<u8>>; 2],
    wakers: [Option<Waker>; 2],
    frames: Vec<(usize, Vec<u8>)>,
}

struct End {
    wire: Rc<RefCell<Wire>>,
    side: usize,
}

struct Rx(Vec<u8>);

struct Tx {
    wire: Rc<RefCell<Wire>>,
    from: usize,
}

impl RxToken for Rx {
    fn consume<R, F: FnOnce(&mut [u8]) -> R>(mut self, f: F) -> R {
        f(&mut self.0)
    }
}

impl TxToken for Tx {
    fn consume<R, F: FnOnce(&mut [u8]) -> R>(self, len: usize, f: F) -> R {
        let mut frame = vec![0; len];
        let result = f(&mut frame);
        let to = 1 - self.from;
        let mut wire = self.wire.borrow_mut();
        wire.frames.push((self.from, frame.clone()));
        wire.queues[to].push_back(frame);
        if let Some(waker) = wire.wakers[to].take() {
            waker.wake();
        }
        result
    }
}

impl Driver for End {
    type RxToken<'a> = Rx;
    type TxToken<'a> = Tx;

    fn receive(&mut self, cx: &mut Context) -> Option<(Rx, Tx)> {
        let mut wire = self.wire.borrow_mut();
        match wire.queues[self.side].pop_front() {
            Some(frame) => Some((
                Rx(frame),
                Tx {
                    wire: self.wire.clone(),
                    from: self.side,
                },
            )),
            None => {
                wire.wakers[self.side] = Some(cx.waker().clone());
                None
            }
        }
    }

    fn transmit(&mut self, _: &mut Context) -> Option<Tx> {
        Some(Tx {
            wire: self.wire.clone(),
            from: self.side,
        })
    }

    fn link_state(&mut self, _: &mut Context) -> LinkState {
        LinkState::Up
    }

    fn capabilities(&self) -> Capabilities {
        let mut capabilities = Capabilities::default();
        capabilities.max_transmission_unit = 1514;
        capabilities
    }

    fn hardware_address(&self) -> HardwareAddress {
        HardwareAddress::Ethernet(MACS[self.side])
    }
}

/// The two stacks, and the frames the link carried.
pub struct Net<'a> {
    pub client: Stack<'a>,
    pub server: Stack<'a>,
    wire: Rc<RefCell<Wire>>,
}

impl Net<'_> {
    /// The TCP flags of each frame the link carried to or from the server's `PORT`, with the side that sent it:
    /// 0 is the client's.
    pub fn tcp_flags(&self) -> Vec<(usize, u8)> {
        self.wire
            .borrow()
            .frames
            .iter()
            .filter_map(|(side, frame)| {
                let ethertype = u16::from_be_bytes([frame[12], frame[13]]);
                let ip = frame.get(14..)?;
                if ethertype != 0x0800 || ip[9] != 6 {
                    return None;
                }
                let tcp = ip.get(usize::from(ip[0] & 0x0f) * 4..)?;
                let source = u16::from_be_bytes([tcp[0], tcp[1]]);
                let destination = u16::from_be_bytes([tcp[2], tcp[3]]);
                (source == PORT || destination == PORT).then_some((*side, tcp[13]))
            })
            .collect()
    }
}

fn stack_config(address: Ipv4Address) -> NetConfig {
    NetConfig::ipv4_static(StaticConfigV4 {
        address: Ipv4Cidr::new(address, 24),
        gateway: None,
        dns_servers: Default::default(),
    })
}

/// Runs `test` with the two stacks, while both stacks run beside it. Panics when `test` panics, or takes longer
/// than `PATIENCE`.
///
/// The stacks and the test run on a thread of their own, and this thread watches the clock. A driver that loops
/// without yielding stops the executor and its timers, so only another thread can see that it never ends. After a
/// timeout that thread runs on until the test process ends.
pub fn run<T: Send + 'static>(test: impl AsyncFnOnce(&Net<'_>) -> T + Send + 'static) -> T {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let _ = sender.send(run_here(test));
    });
    match receiver.recv_timeout(PATIENCE) {
        Ok(result) => result,
        Err(RecvTimeoutError::Timeout) => panic!("the test did not end within {PATIENCE:?}"),
        Err(RecvTimeoutError::Disconnected) => panic!("the test panicked"),
    }
}

fn run_here<T>(test: impl AsyncFnOnce(&Net<'_>) -> T) -> T {
    let wire = Rc::new(RefCell::new(Wire::default()));
    let mut client_resources = StackResources::<8>::new();
    let mut server_resources = StackResources::<8>::new();
    let (client, mut client_runner) = embassy_net::new(
        End {
            wire: wire.clone(),
            side: 0,
        },
        stack_config(CLIENT),
        &mut client_resources,
        1,
    );
    let (server, mut server_runner) = embassy_net::new(
        End {
            wire: wire.clone(),
            side: 1,
        },
        stack_config(SERVER),
        &mut server_resources,
        2,
    );
    let net = Net {
        client,
        server,
        wire,
    };
    block_on(async {
        let stacks = join(client_runner.run(), server_runner.run());
        match select(stacks, test(&net)).await {
            Either::First(_) => unreachable!("the stacks never return"),
            Either::Second(result) => result,
        }
    })
}

/// Every report an end gave, and how its test ended.
#[derive(Debug)]
pub struct Ended {
    pub reports: Vec<Report>,
    pub outcome: Result<Result<(), TestError>, Error>,
}

impl Ended {
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

pub fn server_config(transport: Transport) -> ServerConfig {
    ServerConfig {
        transport,
        duration_us: None,
        interval_us: None,
        ipv6: false,
    }
}

/// The buffers of one TCP socket.
pub struct TcpBuffers {
    rx: Vec<u8>,
    tx: Vec<u8>,
}

impl TcpBuffers {
    pub fn new() -> Self {
        Self {
            rx: vec![0; TCP_BUFFER],
            tx: vec![0; TCP_BUFFER],
        }
    }

    pub fn socket<'a>(&'a mut self, stack: Stack<'a>) -> TcpSocket<'a> {
        TcpSocket::new(stack, &mut self.rx, &mut self.tx)
    }
}

/// The buffers of one UDP socket.
pub struct UdpBuffers {
    rx_meta: Vec<PacketMetadata>,
    rx: Vec<u8>,
    tx_meta: Vec<PacketMetadata>,
    tx: Vec<u8>,
}

impl UdpBuffers {
    pub fn new() -> Self {
        Self::with_send_buffer(UDP_BUFFER)
    }

    pub fn with_send_buffer(len: usize) -> Self {
        Self {
            rx_meta: vec![PacketMetadata::EMPTY; UDP_PACKETS],
            rx: vec![0; UDP_BUFFER],
            tx_meta: vec![PacketMetadata::EMPTY; UDP_PACKETS],
            tx: vec![0; len],
        }
    }

    /// A socket on `stack`, bound to `port`, or to a port the stack picks when `port` is 0.
    pub fn bound<'a>(&'a mut self, stack: Stack<'a>, port: u16) -> UdpSocket<'a> {
        let mut socket = self.unbound(stack);
        socket.bind(port).unwrap();
        socket
    }

    pub fn unbound<'a>(&'a mut self, stack: Stack<'a>) -> UdpSocket<'a> {
        UdpSocket::new(
            stack,
            &mut self.rx_meta,
            &mut self.rx,
            &mut self.tx_meta,
            &mut self.tx,
        )
    }
}

/// Runs one test of `server` on `driver`.
pub async fn tcp_once(driver: &mut TcpServer<'_>, config: ServerConfig) -> Ended {
    let mut buffer = vec![0; 4096];
    let mut server = Server::new(config, &mut buffer).unwrap();
    let mut reports = Vec::new();
    let outcome = driver
        .run_one(&mut server, |report| reports.push(*report))
        .await;
    Ended { reports, outcome }
}

/// Runs one test of `server` on `driver`.
pub async fn udp_once(driver: &mut UdpServer<'_>, config: ServerConfig) -> Ended {
    let mut buffer = vec![0; 4096];
    let mut server = Server::new(config, &mut buffer).unwrap();
    let mut reports = Vec::new();
    let outcome = driver
        .run_one(&mut server, |report| reports.push(*report))
        .await;
    Ended { reports, outcome }
}

/// Runs our server for one test on the server's stack, on a socket of its own.
pub async fn serve(net: &Net<'_>, config: ServerConfig) -> Ended {
    match config.transport {
        Transport::Tcp => {
            let mut buffers = TcpBuffers::new();
            let mut driver = TcpServer::new(buffers.socket(net.server), PORT);
            tcp_once(&mut driver, config).await
        }
        Transport::Udp => {
            let mut buffers = UdpBuffers::new();
            let mut driver = UdpServer::new(buffers.bound(net.server, PORT));
            udp_once(&mut driver, config).await
        }
    }
}

/// Runs our client of `config` over a TCP `socket` against the server.
pub async fn tcp_client(socket: &mut TcpSocket<'_>, config: Config) -> Ended {
    let mut buffer = vec![0; 4096];
    let mut client = Client::new(config, &mut buffer).unwrap();
    let mut reports = Vec::new();
    let outcome = run_tcp_client(&mut client, socket, (SERVER, PORT).into(), |report| {
        reports.push(*report)
    })
    .await;
    Ended { reports, outcome }
}

/// Runs our client of `config` over a UDP `socket` against the server.
pub async fn udp_client(socket: &mut UdpSocket<'_>, config: Config) -> Ended {
    let mut buffer = vec![0; 4096];
    let mut client = Client::new(config, &mut buffer).unwrap();
    let mut reports = Vec::new();
    let outcome = run_udp_client(&mut client, socket, (SERVER, PORT).into(), |report| {
        reports.push(*report)
    })
    .await;
    Ended { reports, outcome }
}

/// Runs our client of `config` on the client's stack, on a socket of its own.
pub async fn client(net: &Net<'_>, config: Config) -> Ended {
    match config.transport {
        Transport::Tcp => {
            let mut buffers = TcpBuffers::new();
            tcp_client(&mut buffers.socket(net.client), config).await
        }
        Transport::Udp => {
            let mut buffers = UdpBuffers::new();
            udp_client(&mut buffers.bound(net.client, 0), config).await
        }
    }
}

/// The time since `started`.
pub fn since(started: Instant) -> Duration {
    Instant::now() - started
}
