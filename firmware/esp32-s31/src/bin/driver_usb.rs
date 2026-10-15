//! The `driver` firmware with the console's async USB Serial/JTAG driver beside it, and a task waiting to read, as
//! an idle console has. Runs `-R -t 10`, then `-t 10`, against the stock server at 192.168.10.1:5201, and logs each
//! final report.

#![no_std]
#![no_main]

use board::{embassy, ethernet, init};
use embassy_executor::Spawner;
use embassy_net::tcp::TcpSocket;
use embassy_net::{IpEndpoint, Ipv4Address};
use embassy_time::Timer;
use embedded_iperf2::embassy::run_tcp_client;
use embedded_iperf2::{Client, Config, Length};
use esp_hal::ethernet::EthernetDmaStorage;
use static_cell::ConstStaticCell;

static RINGS: ConstStaticCell<EthernetDmaStorage<45, 45>> =
    ConstStaticCell::new(EthernetDmaStorage::new());

static BUFFERS: ConstStaticCell<([u8; 65536], [u8; 65536], [u8; 8192])> =
    ConstStaticCell::new(([0; 65536], [0; 65536], [0; 8192]));

#[esp_hal::main]
async fn main(spawner: Spawner) {
    let peripherals = init();
    embassy::start(peripherals.TIMG0);
    let (mut usb_rx, _usb_tx) = embassy::console(peripherals.USB_DEVICE);
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
    defmt::info!("driver: up");
    let (rx, tx, work) = BUFFERS.take();
    let server = IpEndpoint::new(Ipv4Address::new(192, 168, 10, 1).into(), 5201);
    let reader = async {
        let mut buf = [0u8; 32];
        loop {
            let _ = embedded_io_async::Read::read(&mut usb_rx, &mut buf).await;
        }
    };
    let test = async {
        for reverse in [true, false] {
            let config = Config {
                reverse,
                length: Length::Time { us: 10_000_000 },
                port: 5201,
                ..Config::tcp()
            };
            let Ok(mut client) = Client::new(config, &mut *work) else {
                defmt::panic!("config");
            };
            let mut socket = TcpSocket::new(stack, &mut *rx, &mut *tx);
            let mut last = None;
            let outcome = run_tcp_client(&mut client, &mut socket, server, |report| {
                if report.final_report {
                    last = Some((report.bytes, report.end_us));
                }
            })
            .await;
            drop(socket);
            match last {
                Some((bytes, us)) => defmt::info!(
                    "driver: {} {} bytes in {} us, {} kbit/s, ok {}",
                    if reverse { "reverse" } else { "up" },
                    bytes,
                    us,
                    bytes.saturating_mul(8_000).checked_div(us).unwrap_or(0),
                    matches!(outcome, Ok(Ok(())))
                ),
                None => defmt::info!("driver: no final report"),
            }
            Timer::after_secs(1).await;
        }
    };
    embassy_futures::select::select(test, reader).await;
    loop {
        Timer::after_secs(60).await;
    }
}
