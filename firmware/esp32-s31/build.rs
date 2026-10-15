//! Writes `rwtext_hook.x`, which esp-hal's linker script includes in `.rwtext`. With `IPERF_FW_TEXT_IN_RAM` set,
//! the hook places the network's code in internal RAM, so it runs without the flash and the instruction cache.
//! Without it, the hook is empty.
//!
//! All the code does not fit beside the 64 KiB socket buffers and the descriptor rings. The crates in `IN_FLASH`
//! stay in flash, because most of their code runs once at start-up, formats text or parses a command. A generic
//! function is compiled into the crate that uses it, so `HOT` also moves the functions of the network's crates
//! by their mangled name, from whichever crate holds them.

use std::path::PathBuf;

const IN_FLASH: &[&str] = &[
    "*libesp_hal-*",
    "*libcore-*",
    "*libdefmt*",
    "*libmcu_net-*",
    "*libmcu_console-*",
    "*libembedded_devkit*",
];

/// The mangled path segments of the network's crates: our client, embassy-net, smoltcp and esp-hal's Ethernet
/// driver.
const HOT: &[&str] = &[
    "15embedded_iperf2",
    "11embassy_net",
    "7smoltcp",
    "8ethernet",
];

fn main() {
    let out = PathBuf::from(std::env::var("OUT_DIR").expect("cargo sets OUT_DIR"));
    let hook = if std::env::var_os("IPERF_FW_TEXT_IN_RAM").is_some() {
        let flash = IN_FLASH.join(" ");
        let mut hook = String::from("*(");
        for section in [".literal", ".text", ".literal.*", ".text.*"] {
            hook.push_str(&format!("EXCLUDE_FILE({flash}) {section} "));
        }
        hook.push_str(")\n*(");
        for name in HOT {
            hook.push_str(&format!(".text.*{name}* .literal.*{name}* "));
        }
        hook.push_str(")\n");
        hook
    } else {
        String::new()
    };
    std::fs::write(out.join("rwtext_hook.x"), hook).expect("OUT_DIR is writable");
    println!("cargo:rustc-link-search={}", out.display());
    println!("cargo:rerun-if-env-changed=IPERF_FW_TEXT_IN_RAM");
}
