//! TCP straight over embassy-net sockets, without embedded-iperf2, as the measure the client is compared with.
//! It first reads and discards one connection on port 5001, then writes to 192.168.10.1:5201 for
//! 10 s. Then it asks the stock server at 192.168.10.1:5201 for a 10 s reverse test, and reads and discards what
//! it sends. It logs the bytes and the rate of each.

#![no_std]
#![no_main]

use board::{embassy, ethernet, init};
use embassy_executor::Spawner;
use embassy_net::tcp::TcpSocket;
use embassy_net::{IpEndpoint, Ipv4Address};
use embassy_time::{Duration, Instant, Timer};
use esp_hal::ethernet::EthernetDmaStorage;
use static_cell::ConstStaticCell;

static RINGS: ConstStaticCell<EthernetDmaStorage<45, 45>> =
    ConstStaticCell::new(EthernetDmaStorage::new());

static BUFFERS: ConstStaticCell<([u8; 65536], [u8; 65536], [u8; 32768])> =
    ConstStaticCell::new(([0; 65536], [0; 65536], [0; 32768]));

/// The 64-byte test header of embedded-iperf2's client for `-R -t 10 -p 5201`, from its `Client::poll`.
const REVERSE_HEADER: [u8; 64] = [
    68, 1, 0, 128, 0, 0, 0, 1, 0, 0, 20, 81, 0, 2, 0, 0, 0, 0, 0, 0, 255, 255, 252, 24, 0, 0, 0, 0,
    0, 0, 0, 0, 4, 0, 0, 0, 0, 2, 0, 2, 0, 2, 0, 3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0,
];

/// Kilobits per second for `bytes` over `micros`.
fn kbps(bytes: u64, micros: u64) -> u64 {
    bytes.saturating_mul(8_000).checked_div(micros).unwrap_or(0)
}

#[esp_hal::main]
async fn main(spawner: Spawner) {
    let peripherals = init();
    embassy::start(peripherals.TIMG0);
    let pins = ethernet::Pins {
        mdc: peripherals.GPIO5,
        mdio: peripherals.GPIO6,
        phy_reset: peripherals.GPIO7,
        txd0: peripherals.GPIO8,
        txd1: peripherals.GPIO9,
        txd2: peripherals.GPIO10,
        txd3: peripherals.GPIO11,
        tx_ctl: peripherals.GPIO12,
        tx_clk: peripherals.GPIO13,
        rx_clk: peripherals.GPIO14,
        rx_ctl: peripherals.GPIO15,
        rxd3: peripherals.GPIO16,
        rxd2: peripherals.GPIO17,
        rxd1: peripherals.GPIO18,
        rxd0: peripherals.GPIO19,
    };
    let Ok((stack, _)) = ethernet::start(spawner, peripherals.ETH, pins, RINGS.take()).await else {
        defmt::panic!("Ethernet did not start");
    };
    stack.wait_config_up().await;
    defmt::info!("raw: up, {}", defmt::Debug2Format(&stack.config_v4()));
    let (rx, tx, chunk) = BUFFERS.take();

    let mut socket = TcpSocket::new(stack, &mut *rx, &mut *tx);
    if socket.accept(5001).await.is_err() {
        defmt::panic!("accept failed");
    }
    let start = Instant::now();
    let mut bytes = 0u64;
    loop {
        match socket.read_with(|data| (data.len(), data.len())).await {
            Ok(0) | Err(_) => break,
            Ok(count) => bytes = bytes.saturating_add(count as u64),
        }
    }
    let micros = start.elapsed().as_micros();
    defmt::info!(
        "raw: down {} bytes in {} us, {} kbit/s",
        bytes,
        micros,
        kbps(bytes, micros)
    );
    socket.close();
    let _ = socket.flush().await;
    Timer::after_millis(100).await;
    socket.abort();
    let _ = socket.flush().await;
    drop(socket);

    let mut socket = TcpSocket::new(stack, &mut *rx, &mut *tx);
    let server = IpEndpoint::new(Ipv4Address::new(192, 168, 10, 1).into(), 5201);
    if socket.connect(server).await.is_err() {
        defmt::panic!("connect failed");
    }
    let start = Instant::now();
    let mut bytes = 0u64;
    while start.elapsed() < Duration::from_secs(10) {
        match socket.write(chunk).await {
            Ok(count) => bytes = bytes.saturating_add(count as u64),
            Err(_) => break,
        }
    }
    let micros = start.elapsed().as_micros();
    socket.close();
    let _ = socket.flush().await;
    defmt::info!(
        "raw: up {} bytes in {} us, {} kbit/s",
        bytes,
        micros,
        kbps(bytes, micros)
    );
    Timer::after_millis(100).await;
    socket.abort();
    let _ = socket.flush().await;
    drop(socket);

    let mut socket = TcpSocket::new(stack, &mut *rx, &mut *tx);
    if socket.connect(server).await.is_err() {
        defmt::panic!("connect failed");
    }
    let mut header: &[u8] = &REVERSE_HEADER;
    while !header.is_empty() {
        match socket.write(header).await {
            Ok(count) => header = header.get(count..).unwrap_or_default(),
            Err(_) => defmt::panic!("header write failed"),
        }
    }
    let start = Instant::now();
    let mut bytes = 0u64;
    loop {
        match socket.read_with(|data| (data.len(), data.len())).await {
            Ok(0) | Err(_) => break,
            Ok(count) => bytes = bytes.saturating_add(count as u64),
        }
    }
    let micros = start.elapsed().as_micros();
    socket.close();
    let _ = socket.flush().await;
    defmt::info!(
        "raw: reverse {} bytes in {} us, {} kbit/s",
        bytes,
        micros,
        kbps(bytes, micros)
    );
    loop {
        Timer::after_secs(60).await;
    }
}
