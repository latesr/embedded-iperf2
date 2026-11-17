// A copy from embedded-devkit/xtask/src/shared. Edit it there, then run `cargo xtask sync` in the devkit.

//! The address of this host that a board on the local network connects back to. The OS routes to the board
//! through any interface on its subnet, which can be Wi-Fi while a wired one is up, so the choice is made here
//! and printed.

// The devkit's own commands do not call it. Other repos' `cargo xtask hil` does. `mcu-scenario` holds the public
// copy, `mcu_scenario::host_address`, which scenario tests use. This copy goes when those `hil` commands move
// onto `mcu-scenario`.
#![allow(dead_code)]

use std::fmt;
use std::net::Ipv4Addr;
use std::path::Path;

/// How an interface reaches the network.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Medium {
    /// A cable, such as Ethernet over USB.
    Wired,
    /// Wi-Fi.
    WiFi,
}

/// The address a board connects back to, and the interface of this host that holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostAddress {
    /// The interface's IPv4 address.
    pub address: Ipv4Addr,
    /// The interface's name, such as `en6`.
    pub interface: String,
    /// How the interface reaches the network.
    pub medium: Medium,
}

impl fmt::Display for HostAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let medium = match self.medium {
            Medium::Wired => "wired",
            Medium::WiFi => "Wi-Fi",
        };
        write!(f, "{} on {}, {medium}", self.address, self.interface)
    }
}

/// The address of this host on `board`'s subnet. A wired interface is chosen over Wi-Fi, and Wi-Fi only when no
/// wired interface is on the subnet.
///
/// Fails when no interface is on the subnet, when two of the chosen medium are, or when the interfaces cannot be
/// listed. On macOS a Wi-Fi interface is the device of the `Wi-Fi` port that `networksetup -listallhardwareports`
/// lists. Elsewhere a Wi-Fi interface has a `/sys/class/net/<name>/wireless` folder.
pub fn host_address(board: Ipv4Addr) -> Result<HostAddress, String> {
    let wifi = wifi_devices()?;
    let interfaces: Vec<Interface> = if_addrs::get_if_addrs()
        .map_err(|error| format!("cannot list this host's interfaces: {error}"))?
        .into_iter()
        .filter_map(|interface| match interface.addr {
            if_addrs::IfAddr::V4(v4) => Some(Interface {
                medium: if wifi.contains(&interface.name) {
                    Medium::WiFi
                } else {
                    Medium::Wired
                },
                name: interface.name,
                address: v4.ip,
                netmask: v4.netmask,
            }),
            if_addrs::IfAddr::V6(_) => None,
        })
        .collect();
    choose(&interfaces, board)
}

/// The names of this host's Wi-Fi interfaces.
// It only runs networksetup on macOS or reads /sys/class/net elsewhere. wifi_ports and wireless_devices read
// that output, and are tested.
#[mutants::skip]
fn wifi_devices() -> Result<Vec<String>, String> {
    if cfg!(target_os = "macos") {
        let output = std::process::Command::new("networksetup")
            .arg("-listallhardwareports")
            .output()
            .map_err(|error| format!("networksetup did not run: {error}"))?;
        return Ok(wifi_ports(&String::from_utf8_lossy(&output.stdout)));
    }
    wireless_devices(Path::new("/sys/class/net"))
}

/// The devices in `net`, a folder laid out as Linux's `/sys/class/net`, that have a `wireless` folder.
fn wireless_devices(net: &Path) -> Result<Vec<String>, String> {
    let devices = std::fs::read_dir(net)
        .map_err(|error| format!("cannot read {}: {error}", net.display()))?;
    Ok(devices
        .flatten()
        .filter(|device| device.path().join("wireless").exists())
        .map(|device| device.file_name().to_string_lossy().into_owned())
        .collect())
}

/// One IPv4 address of one of this host's interfaces.
#[derive(Debug)]
struct Interface {
    name: String,
    address: Ipv4Addr,
    netmask: Ipv4Addr,
    medium: Medium,
}

/// The interface from `interfaces` that [`host_address`] chooses for `board`.
fn choose(interfaces: &[Interface], board: Ipv4Addr) -> Result<HostAddress, String> {
    let on_subnet = |medium| {
        interfaces
            .iter()
            .filter(move |interface| {
                let mask = u32::from(interface.netmask);
                interface.medium == medium
                    && u32::from(interface.address) & mask == u32::from(board) & mask
            })
            .collect::<Vec<_>>()
    };
    let mut chosen = on_subnet(Medium::Wired);
    if chosen.is_empty() {
        chosen = on_subnet(Medium::WiFi);
    }
    match chosen.as_slice() {
        [interface] => Ok(HostAddress {
            address: interface.address,
            interface: interface.name.clone(),
            medium: interface.medium,
        }),
        [] => Err(format!("no interface of this host is on {board}'s subnet")),
        several => {
            let names: Vec<&str> = several
                .iter()
                .map(|interface| interface.name.as_str())
                .collect();
            Err(format!(
                "{} are all on {board}'s subnet, so the board's traffic could take any of them. Take all but one down.",
                names.join(" and ")
            ))
        }
    }
}

/// The devices of the `Wi-Fi` ports in the output of `networksetup -listallhardwareports`.
fn wifi_ports(listing: &str) -> Vec<String> {
    let mut port = "";
    let mut devices = Vec::new();
    for line in listing.lines() {
        if let Some(name) = line.strip_prefix("Hardware Port: ") {
            port = name;
        } else if let Some(device) = line.strip_prefix("Device: ")
            && port == "Wi-Fi"
        {
            devices.push(device.to_owned());
        }
    }
    devices
}

#[cfg(test)]
mod tests {
    use super::*;

    fn interface(name: &str, address: [u8; 4], prefix: u32, medium: Medium) -> Interface {
        Interface {
            name: name.to_owned(),
            address: Ipv4Addr::from(address),
            netmask: Ipv4Addr::from(!u32::MAX.wrapping_shr(prefix)),
            medium,
        }
    }

    // The addresses are from the documentation ranges of RFC 5737.
    const BOARD: Ipv4Addr = Ipv4Addr::new(192, 0, 2, 50);

    #[test]
    fn a_wired_interface_on_the_board_s_subnet_is_chosen_over_wi_fi() {
        let interfaces = [
            interface("en0", [192, 0, 2, 206], 24, Medium::WiFi),
            interface("en5", [198, 51, 100, 2], 24, Medium::Wired),
            interface("en6", [192, 0, 2, 216], 24, Medium::Wired),
        ];
        assert_eq!(
            choose(&interfaces, BOARD),
            Ok(HostAddress {
                address: Ipv4Addr::new(192, 0, 2, 216),
                interface: "en6".to_owned(),
                medium: Medium::Wired,
            })
        );
    }

    #[test]
    fn wi_fi_is_chosen_when_no_wired_interface_is_on_the_subnet() {
        let interfaces = [
            interface("en5", [198, 51, 100, 2], 24, Medium::Wired),
            interface("en0", [192, 0, 2, 206], 24, Medium::WiFi),
        ];
        let chosen = choose(&interfaces, BOARD).unwrap();
        assert_eq!(
            (chosen.interface.as_str(), chosen.medium),
            ("en0", Medium::WiFi)
        );
    }

    #[test]
    fn the_subnet_is_the_interface_s_netmask() {
        let interfaces = [
            interface("en6", [192, 0, 2, 216], 25, Medium::Wired),
            interface("en7", [192, 0, 2, 217], 24, Medium::Wired),
        ];
        assert_eq!(choose(&interfaces, BOARD).unwrap().interface, "en7");
    }

    #[test]
    fn no_interface_on_the_subnet_fails_and_names_the_board() {
        let interfaces = [interface("en5", [198, 51, 100, 2], 24, Medium::Wired)];
        let error = choose(&interfaces, BOARD).unwrap_err();
        assert!(error.contains("192.0.2.50"), "{error}");
    }

    #[test]
    fn two_wired_interfaces_on_the_subnet_fail_and_name_both() {
        let interfaces = [
            interface("en0", [192, 0, 2, 206], 24, Medium::WiFi),
            interface("en6", [192, 0, 2, 216], 24, Medium::Wired),
            interface("en7", [192, 0, 2, 217], 24, Medium::Wired),
        ];
        let error = choose(&interfaces, BOARD).unwrap_err();
        assert!(error.contains("en6") && error.contains("en7"), "{error}");
        assert!(!error.contains("en0"), "{error}");
    }

    #[test]
    fn a_host_address_prints_its_address_interface_and_medium() {
        let address = |medium| HostAddress {
            address: Ipv4Addr::new(192, 0, 2, 216),
            interface: "en6".to_owned(),
            medium,
        };
        assert_eq!(
            address(Medium::Wired).to_string(),
            "192.0.2.216 on en6, wired"
        );
        assert_eq!(
            address(Medium::WiFi).to_string(),
            "192.0.2.216 on en6, Wi-Fi"
        );
    }

    #[test]
    fn the_wi_fi_ports_are_the_devices_of_the_ports_named_wi_fi() {
        // A macOS 26 listing, with the hardware addresses removed.
        let listing = "\nHardware Port: USB 10/100/1000 LAN\nDevice: en6\nEthernet Address: xx\n\n\
                       Hardware Port: Wi-Fi\nDevice: en0\nEthernet Address: xx\n\n\
                       Hardware Port: Thunderbolt 1\nDevice: en1\nEthernet Address: xx\n\n\
                       VLAN Configurations\n===================\n";
        assert_eq!(wifi_ports(listing), ["en0"]);
    }

    #[test]
    fn the_wireless_devices_are_the_ones_with_a_wireless_folder() {
        let net = std::env::temp_dir().join(format!("host-net-{}", std::process::id()));
        std::fs::create_dir_all(net.join("eth0")).unwrap();
        std::fs::create_dir_all(net.join("wlan0/wireless")).unwrap();
        let devices = wireless_devices(&net);
        std::fs::remove_dir_all(&net).unwrap();
        assert_eq!(devices, Ok(vec!["wlan0".to_owned()]));
        assert!(wireless_devices(&net).unwrap_err().contains("host-net-"));
    }
}
