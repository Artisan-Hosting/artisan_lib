use std::net::Ipv4Addr;
use get_if_addrs::get_if_addrs;

use get_if_addrs::IfAddr;

const MAJOR_VERSION: &str = env!("CARGO_PKG_VERSION_MAJOR");
const MINOR_VERSION: &str = env!("CARGO_PKG_VERSION_MINOR");

pub fn get_local_ip() -> Ipv4Addr {
    let if_addrs = match get_if_addrs() {
        Ok(addrs) => addrs,
        Err(_) => return Ipv4Addr::LOCALHOST, // Return loopback address if interface fetching fails
    };
    
    for iface in if_addrs {
        if let IfAddr::V4(v4_addr) = iface.addr {
            if !v4_addr.ip.is_loopback() { // Filter out loopback addresses
                return v4_addr.ip;
            }
        }
    }
    
    Ipv4Addr::LOCALHOST // Return loopback address if no suitable non-loopback address is found
}

pub fn get_header_version() -> u16 {
    let major_int = MAJOR_VERSION.parse::<u8>().unwrap_or(0);
    let minor_int = MINOR_VERSION.parse::<u8>().unwrap_or(0);

    ((major_int << 8) | (minor_int << 4) | 0).into() // Shifts and ignoring the patch number
}