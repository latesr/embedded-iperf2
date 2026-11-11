//! The embedded-iperf2 firmware for the ESP32-S31-Function-CoreBoard-1. It runs the devkit's console over USB
//! Serial/JTAG. The console has `mcu-net`'s `net` commands over Ethernet, and `iperf -s`. `net iperf` runs the
//! crate's client, and `iperf -s` runs the crate's server. `cargo xtask hil` drives the firmware (T-7).

#![no_std]
#![no_main]

mod server;

use board::{boot_count, embassy, ethernet, info, init, psram, reset};
use embassy_executor::Spawner;
use embassy_time::Delay;
use esp_hal::ethernet::EthernetDmaStorage;
use mcu_console::Config;
use mcu_net::NetCommands;
use server::{Buffers, IperfServer};
use static_cell::ConstStaticCell;

const FIRMWARE: &str = concat!(env!("CARGO_PKG_NAME"), " ", env!("CARGO_PKG_VERSION"));

/// The length of each socket buffer, receive and send, as the devkit's S31 examples set. The receive buffer and
/// the Ethernet transmit ring's length cap the TCP window the board advertises.
const SOCKET: usize = 65536;

/// The length of `net iperf`'s work buffer, which holds one TCP write or one UDP datagram.
const WORK: usize = 8192;

/// The Ethernet descriptor rings, 45 receive and 45 transmit slots, as the devkit's `console_ethernet` has.
static RINGS: ConstStaticCell<EthernetDmaStorage<45, 45>> =
    ConstStaticCell::new(EthernetDmaStorage::new());

/// `net iperf`'s socket receive and send buffers, and the client's work buffer.
static CLIENT: ConstStaticCell<([u8; SOCKET], [u8; SOCKET], [u8; WORK])> =
    ConstStaticCell::new(([0; SOCKET], [0; SOCKET], [0; WORK]));

/// `iperf -s`'s buffers.
static SERVER: ConstStaticCell<Buffers> = ConstStaticCell::new(Buffers::new());

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
    let (client_rx, client_tx, client_work) = CLIENT.take();
    let net = NetCommands::ethernet(stack, capabilities.max_transmission_unit).with_iperf(
        client_rx,
        client_tx,
        client_work,
    );
    let server = IperfServer::new(stack, SERVER.take());
    defmt::info!("console firmware, boot {}", boot);
    let (rx, tx) = embassy::console(peripherals.USB_DEVICE);
    let config = Config {
        name: "s31",
        boot,
        write_timeout_ms: 500,
        reset,
    };
    let mut console: mcu_console::asynch::Console<_, _, _, _, 256> =
        mcu_console::asynch::Console::new(rx, tx, Delay, (info, (net, server)), config);
    let stop = console.run().await;
    defmt::panic!("the console stopped: {}", defmt::Debug2Format(&stop));
}
