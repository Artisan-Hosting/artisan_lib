use std::{
    io::{Read, Write},
    net::{IpAddr, Ipv4Addr, TcpStream, ToSocketAddrs},
};
use get_if_addrs::get_if_addrs;

use dusa_collection_utils::errors::ErrorArrayItem;
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

pub fn send_message(mut stream: &TcpStream, payload: &[u8]) -> Result<(), ErrorArrayItem> {
    let major_version = MAJOR_VERSION.parse()?;
    let minor_version = MINOR_VERSION.parse()?;

    // Calculate the total length: payload + version fields.
    let length = 2 + payload.len() as u32;

    // Create the message buffer.
    let mut message = Vec::with_capacity(4 + 2 + payload.len());

    // Append the length (4 bytes).
    message.extend_from_slice(&length.to_be_bytes());

    // Append the version information (2 bytes).
    message.push(major_version);
    message.push(minor_version);

    // Append the payload.
    message.extend_from_slice(payload);

    // Send the message.
    stream.write_all(&message)?;

    Ok(())
}

pub fn read_message(mut stream: &TcpStream) -> Result<(u8, u8, Vec<u8>), ErrorArrayItem> {
    // Read the length prefix (4 bytes).
    let mut length_buf = [0u8; 4];
    stream.read_exact(&mut length_buf)?;
    let length = u32::from_be_bytes(length_buf);

    // Read the version fields (2 bytes).
    let mut version_buf = [0u8; 2];
    stream.read_exact(&mut version_buf)?;
    let major_version = version_buf[0];
    let minor_version = version_buf[1];

    // Ensure compatibility by checking the major version.
    let mv: u8 = MAJOR_VERSION.parse()?;
    if major_version != mv {
        return Err(ErrorArrayItem::from(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!(
                "Unsupported major version: {}. Expected: {}",
                major_version, MAJOR_VERSION
            ),
        )));
    }

    // Read the payload.
    let payload_length = (length - 2) as usize;
    let mut payload = vec![0u8; payload_length];
    stream.read_exact(&mut payload)?;

    Ok((major_version, minor_version, payload))
}
