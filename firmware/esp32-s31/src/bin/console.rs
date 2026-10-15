//! A copy of the devkit's `console_ethernet` example, to change one part at a time while measuring `net iperf`.
//! It has the console of `console_async` with `mcu-net`'s `net` commands over Ethernet.

#![no_std]
#![no_main]

mod common;
const NAME: &str = "s31";
const IPERF_SOCKET: usize = 65536;
/// The send buffer, smaller than the devkit's so that the network's code fits in RAM beside it. A reverse test
/// only sends the 64-byte header.
const IPERF_SEND: usize = 16384;
const IPERF_WORK: usize = 8192;

use board::{boot_count, embassy, ethernet, info, init, psram, reset};
use common::{Echo, FIRMWARE};
use embassy_executor::Spawner;
use embassy_time::Delay;
use esp_hal::ethernet::EthernetDmaStorage;
use mcu_console::Config;
use mcu_net::NetCommands;
use static_cell::ConstStaticCell;

/// The Ethernet descriptor rings, 45 receive and 45 transmit slots. A firmware tunes throughput here: the transmit
/// ring's length is the driver's burst size, which caps the TCP window smoltcp advertises.
static RINGS: ConstStaticCell<EthernetDmaStorage<45, 45>> =
    ConstStaticCell::new(EthernetDmaStorage::new());

/// `net iperf`'s socket receive and send buffers, and the client's work buffer.
static IPERF: ConstStaticCell<([u8; IPERF_SOCKET], [u8; IPERF_SEND], [u8; IPERF_WORK])> =
    ConstStaticCell::new(([0; IPERF_SOCKET], [0; IPERF_SEND], [0; IPERF_WORK]));

#[esp_hal::main]
async fn main(spawner: Spawner) {
    let mut peripherals = init();
    let boot = boot_count();
    embassy::start(peripherals.TIMG0);
    let psram = psram(peripherals.PSRAM);
    let info = info(FIRMWARE, peripherals.FLASH.reborrow(), &psram);
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
    let Ok((stack, capabilities)) =
        ethernet::start(spawner, peripherals.ETH, pins, RINGS.take()).await
    else {
        defmt::panic!("Ethernet did not start");
    };
    let (iperf_rx, iperf_tx, iperf_work) = IPERF.take();
    let net = NetCommands::ethernet(stack, capabilities.max_transmission_unit)
        .with_iperf(iperf_rx, iperf_tx, iperf_work);
    defmt::info!("console firmware, boot {}", boot);
    let (rx, tx) = embassy::console(peripherals.USB_DEVICE);
    let config = Config {
        name: NAME,
        boot,
        write_timeout_ms: 500,
        reset,
    };
    let mut console: mcu_console::asynch::Console<_, _, _, _, 256> =
        mcu_console::asynch::Console::new(rx, tx, Delay, (info, (net, Echo)), config);
    let stop = console.run().await;
    defmt::panic!("the console stopped: {}", defmt::Debug2Format(&stop));
}
