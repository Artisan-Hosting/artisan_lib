use bincode;
use colored::{Color, Colorize};
use dusa_collection_utils::{errors::ErrorArrayItem, log, log::LogLevel};
use flate2::{read::GzDecoder, write::GzEncoder, Compression};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fmt::{self, Debug},
    io::{self, Cursor, Read, Write},
    time::Duration,
    vec,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpStream, UnixStream},
    time::timeout,
};

use crate::{
    encryption::{decrypt_data, encrypt_data},
    network::{get_header_version, get_local_ip},
};

const HEADER_VERSION_LEN: usize = 1; // u8
const HEADER_FLAGS_LEN: usize = 1; // u8
const HEADER_PAYLOAD_LENGTH_LEN: usize = 2; // u16
const HEADER_RESERVED_LEN: usize = 1; // u8
const HEADER_STATUS_LEN: usize = 1; // u8 for ProtocolStatus
const HEADER_ORIGIN_ADDRESS_LEN: usize = 4; // [u8; 4] for IPv4 address

// Calculate the fixed header length
pub const HEADER_LENGTH: usize = HEADER_VERSION_LEN
    + HEADER_FLAGS_LEN
    + HEADER_PAYLOAD_LENGTH_LEN
    + HEADER_RESERVED_LEN
    + HEADER_STATUS_LEN
    + HEADER_ORIGIN_ADDRESS_LEN;

pub const EOL: &str = "-EOL-";

bitflags::bitflags! {
    #[derive(Debug, Eq, PartialEq, Ord, PartialOrd, Clone, Copy)]
    pub struct ProtocolStatus: u8 {
        const OK        = 0b0000_0001;
        const ERROR     = 0b0000_0010;
        const WAITING   = 0b0000_0100;
        const TIMEDOUT  = 0b0000_1000;
        const MALFORMED = 0b0001_0000;
        const SIDEGRADE = 0b1001_0010;
        const REFUSED   = 0b0100_0010;
        const RESERVED  = 0b0010_0000;
        // Add other statuses as needed up to 8 bits
    }
}

impl ProtocolStatus {
    fn get_status_color(&self) -> Color {
        match *self {
            ProtocolStatus::OK => Color::Green,
            ProtocolStatus::ERROR => Color::Red,
            ProtocolStatus::WAITING => Color::Yellow,
            ProtocolStatus::RESERVED => Color::Blue,
            ProtocolStatus::SIDEGRADE => Color::Blue,
            ProtocolStatus::TIMEDOUT => Color::BrightYellow,
            ProtocolStatus::MALFORMED => Color::BrightYellow,
            _ => Color::White,
        }
    }

    pub fn expect(&self, val: ProtocolStatus) -> bool {
        // Checks if `self` contains exactly the same flags as `val`
        *self == val
    }
}

impl fmt::Display for ProtocolStatus {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let status_description = match *self {
            ProtocolStatus::OK => "OK",
            ProtocolStatus::ERROR => "Error",
            ProtocolStatus::WAITING => "Waiting",
            ProtocolStatus::RESERVED => "Reserved",
            ProtocolStatus::SIDEGRADE => "Client requested different flags",
            ProtocolStatus::TIMEDOUT => "Timed Out",
            ProtocolStatus::MALFORMED => "Malformed Response",
            _ => "Unknown",
        };
        write!(f, "{}", status_description.color(self.get_status_color()))
    }
}

bitflags::bitflags! {
    #[derive(Debug, Eq, PartialEq, Ord, PartialOrd, Clone, Copy)]
    pub struct Flags: u8 {
        const NONE       = 0b0000_0000;
        const COMPRESSED = 0b0000_0001;
        const ENCRYPTED  = 0b0000_0010;
        const ENCODED    = 0b0000_0100;
        const SIGNATURE  = 0b0000_1000;
        const OPTIMIZED  = 0b0000_1111; //
        // Add other flags as needed
    }
}

impl Flags {
    pub fn expect(&self, val: Flags) -> bool {
        // Checks if `self` contains exactly the same flags as `val`
        *self == val
    }
}

impl fmt::Display for Flags {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut flags = vec![];
        if self.contains(Flags::COMPRESSED) {
            flags.push("Compressed".cyan().to_string());
        }
        if self.contains(Flags::ENCRYPTED) {
            flags.push("Encrypted".magenta().to_string());
        }
        if self.contains(Flags::ENCODED) {
            flags.push("Encoded".blue().to_string());
        }
        if self.contains(Flags::SIGNATURE) {
            flags.push("Signed".yellow().to_string());
        }
        if self.contains(Flags::OPTIMIZED) {
            flags.push("SECURE".bright_green().bold().to_string());
        }
        write!(f, "{}", flags.join(", "))
    }
}

bitflags::bitflags! {
    #[derive(Debug, Eq, PartialEq, Ord, PartialOrd, Clone, Copy)]
    pub struct Reserved: u8 {
        const NONE       = 0b0000_0000;
        // Add other flags as needed
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ProtocolHeader {
    pub version: u8,
    pub flags: u8,
    pub payload_length: u16,
    pub reserved: u8,
    pub status: u8, // Changed from u16 to u8
    pub origin_address: [u8; 4],
}

impl fmt::Display for ProtocolHeader {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{}\n{}\n{}\n{}\n{}\n{}\n",
            format!("Version:          {}", self.version).bold().green(),
            format!(
                "Flags:            {:#010b} ({})",
                self.flags,
                Flags::from_bits_truncate(self.flags)
            )
            .bold()
            .blue(),
            format!("Payload Length:   {}", self.payload_length)
                .bold()
                .purple(),
            format!("Reserved:         {:#010b}", self.reserved)
                .bold()
                .yellow(),
            format!(
                "Status:           {:#010b} ({})",
                self.status,
                ProtocolStatus::from_bits_truncate(self.status)
            )
            .bold()
            .red(),
            format!("Origin Address:   {}", self.get_origin_ip())
                .bold()
                .cyan(),
        )
    }
}

impl ProtocolHeader {
    pub fn get_origin_ip(&self) -> std::net::Ipv4Addr {
        std::net::Ipv4Addr::from(self.origin_address)
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ProtocolMessage<T> {
    pub header: ProtocolHeader,
    pub payload: T,
}

impl<T> ProtocolMessage<T>
where
    T: Serialize + for<'a> Deserialize<'a> + std::fmt::Debug + Clone,
{
    // Create a new protocol message
    pub fn new(flags: Flags, payload: T) -> io::Result<Self> {
        let origin_address = get_local_ip().octets();

        // This is to be removed when reserved has been
        // assigned
        let reserved = Reserved::NONE;

        let header = ProtocolHeader {
            version: get_header_version(),
            flags: flags.bits(),
            payload_length: 0, // Will be set in to_bytes
            reserved: reserved.bits(),
            status: ProtocolStatus::OK.bits(), // Set initial status
            origin_address,
        };

        Ok(Self { header, payload })
    }

    // Standardized order of processing flags: Compression -> Encoding -> Encryption
    fn ordered_flags() -> Vec<Flags> {
        vec![
            Flags::COMPRESSED,
            Flags::ENCODED,
            Flags::ENCRYPTED,
            Flags::SIGNATURE,
        ]
    }

    pub async fn to_bytes(&mut self) -> io::Result<Vec<u8>> {
        log!(LogLevel::Trace, "Starting to_bytes conversion.");

        // Serialize and process payload
        let mut payload_bytes = bincode::serialize(&self.payload)
            .map_err(|err| io::Error::new(io::ErrorKind::Other, err.to_string()))?;

        let flags = Flags::from_bits_truncate(self.header.flags);
        for flag in Self::ordered_flags() {
            if flags.contains(flag.clone()) {
                payload_bytes = match flag {
                    Flags::COMPRESSED => compress_data(&payload_bytes)?,
                    Flags::ENCODED => encode_data(&payload_bytes),
                    Flags::ENCRYPTED => encrypt_data(&payload_bytes).await.unwrap(),
                    Flags::SIGNATURE => generate_checksum(&mut payload_bytes),
                    _ => payload_bytes,
                };
            }
        }

        // Set payload length after transformations
        self.header.payload_length = payload_bytes.len() as u16;

        // Manually serialize the header fields into a fixed-size buffer
        let mut header_bytes: Vec<u8> = Vec::with_capacity(HEADER_LENGTH);
        header_bytes.extend(&self.header.version.to_be_bytes());
        header_bytes.extend(&self.header.flags.to_be_bytes());
        header_bytes.extend(&self.header.payload_length.to_be_bytes());
        header_bytes.extend(&self.header.reserved.to_be_bytes());
        header_bytes.extend(&self.header.status.to_be_bytes()); // Updated
        header_bytes.extend(&self.header.origin_address);
        log!(LogLevel::Debug, "Generated header \n{}", self.header);

        // Combine header and payload
        let mut buffer = Vec::with_capacity(HEADER_LENGTH + payload_bytes.len());
        buffer.extend(header_bytes);
        buffer.extend(payload_bytes);

        Ok(buffer)
    }

    pub async fn from_bytes(bytes: &[u8]) -> io::Result<Self> {
        log!(LogLevel::Trace, "Starting from_bytes conversion.");

        if bytes.len() < HEADER_LENGTH {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Byte array too short to contain valid header",
            ));
        }

        // remove eof

        let header_bytes: &[u8] = &bytes[..HEADER_LENGTH];
        let payload_bytes: &[u8] = &bytes[HEADER_LENGTH..];

        // Manually deserialize the header fields
        let mut cursor = Cursor::new(header_bytes);

        let mut version_bytes: [u8; 1] = [0u8; 1];
        read_with_std_io(&mut cursor, &mut version_bytes)?;
        let version = u8::from_be_bytes(version_bytes);

        let mut flags_bytes: [u8; 1] = [0u8; 1];
        read_with_std_io(&mut cursor, &mut flags_bytes)?;
        let flags = u8::from_be_bytes(flags_bytes);

        let mut payload_length_bytes: [u8; 2] = [0u8; 2];
        read_with_std_io(&mut cursor, &mut payload_length_bytes)?;
        let payload_length = u16::from_be_bytes(payload_length_bytes);

        let mut reserved_bytes: [u8; 1] = [0u8; 1];
        read_with_std_io(&mut cursor, &mut reserved_bytes)?;
        let reserved = u8::from_be_bytes(reserved_bytes);

        let mut status_byte: [u8; 1] = [0u8; 1];
        // cursor.clone().read_exact(&mut status_byte)?;
        read_with_std_io(&mut cursor, &mut status_byte)?;
        let status_bits: u8 = u8::from_be_bytes(status_byte);
        let status: ProtocolStatus = ProtocolStatus::from_bits_truncate(status_bits);

        let mut origin_address: [u8; 4] = [0u8; 4];
        read_with_std_io(&mut cursor, &mut origin_address)?;
        // cursor.read_exact(&mut origin_address)?;

        let header: ProtocolHeader = ProtocolHeader {
            version,
            flags,
            payload_length,
            reserved,
            status: status.bits(),
            origin_address,
        };
        log!(LogLevel::Debug, "Recieved header \n{}", header);

        // Validate header fields
        // if header.reserved != Reserved::NONE.bits() {
        //     return Err(io::Error::new(
        //         io::ErrorKind::InvalidData,
        //         "Reserved field must be zero",
        //     ));
        // }

        // Deserialize and process payload
        let mut payload = payload_bytes.to_vec();
        let flags = Flags::from_bits_truncate(header.flags);
        for flag in Self::ordered_flags().iter().rev() {
            if flags.contains(*flag) {
                payload = match flag {
                    &Flags::ENCRYPTED => decrypt_data(&payload).await.unwrap(),
                    &Flags::ENCODED => decode_data(&payload).unwrap(),
                    &Flags::COMPRESSED => decompress_data(&payload)?,
                    &Flags::SIGNATURE => verify_checksum(payload),
                    &Flags::NONE => payload,
                    _ => unreachable!(),
                };
            }
        }

        let payload: T = bincode::deserialize(&payload).map_err(|err| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Payload error: {}", err),
            )
        })?;

        Ok(Self { header, payload })
    }

    pub async fn get_payload(&self) -> T {
        return self.payload.clone();
    }

    pub async fn get_header(&self) -> ProtocolHeader {
        return self.header.clone();
    }
}

// Helper functions for compression
pub fn compress_data(data: &[u8]) -> io::Result<Vec<u8>> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(data)?;
    encoder
        .finish()
        .map_err(|err| io::Error::new(io::ErrorKind::Other, format!("Compression error: {}", err)))
}

pub fn decompress_data(data: &[u8]) -> io::Result<Vec<u8>> {
    let mut decoder = GzDecoder::new(data);
    let mut decompressed_data = Vec::new();
    decoder.read_to_end(&mut decompressed_data)?;
    Ok(decompressed_data)
}

pub fn generate_checksum(data: &mut Vec<u8>) -> Vec<u8> {
    let mut hasher = Sha256::new();
    hasher.update(data.clone());
    let mut checksum: Vec<u8> = hasher.finalize().to_vec();
    data.append(&mut checksum);
    data.to_vec()
}

pub fn verify_checksum(data_with_checksum: Vec<u8>) -> Vec<u8> {
    // Check that the data has at least a SHA-256 checksum length appended
    if data_with_checksum.len() < 32 {
        return Vec::new();
    }

    // Separate the data and the appended checksum
    let data_len = data_with_checksum.len() - 32;
    let (data, checksum) = data_with_checksum.split_at(data_len);

    // Generate the checksum for the data portion
    let mut hasher = Sha256::new();
    hasher.update(data);
    let calculated_checksum = hasher.finalize().to_vec();

    // Compare the calculated checksum with the provided checksum
    if checksum == calculated_checksum.as_slice() {
        data.to_vec() // Return original data if checksum is valid
    } else {
        Vec::new() // Return an empty Vec<u8> if checksum is invalid
    }
}

pub fn encode_data(data: &[u8]) -> Vec<u8> {
    // Encode the data into a hex string and convert it into bytes
    hex::encode(data).into_bytes()
}

pub fn decode_data(data: &[u8]) -> Result<Vec<u8>, ErrorArrayItem> {
    // Convert the input bytes to a string
    let hex_string = String::from_utf8(data.to_vec()).map_err(|err| ErrorArrayItem::from(err))?;
    // Decode the hex string back into bytes
    hex::decode(hex_string).map_err(|err| ErrorArrayItem::from(err))
}

// Updated function to return `impl Future`
pub async fn send_message_tcp<'a, T>(
    stream: &'a mut TcpStream,
    message: &'a mut ProtocolMessage<T>,
    sidegrade: bool,
) -> io::Result<ProtocolStatus>
where
    T: serde::Serialize + DeserializeOwned + std::fmt::Debug + Send + Clone,
{
    // Serialize the message data
    let mut serialized_data = message.to_bytes().await?;
    // Adding the EOL
    serialized_data.append(&mut EOL.as_bytes().to_vec());

    // Send the data asynchronously
    stream.write_all(&serialized_data).await?;
    stream.flush().await?;
    log!(
        LogLevel::Trace,
        "Message sent to {:#?}",
        stream.peer_addr().unwrap()
    );

    // Timeout duration for receiving a response
    let mut response_buffer: Vec<u8> = vec![0u8; 10]; // Adjust this as necessary if response size differs

    // Read the response data with a timeout
    log!(LogLevel::Trace, "Reading response data...");
    match timeout(
        Duration::from_secs(3),
        stream.read_exact(&mut response_buffer),
    )
    .await
    {
        Ok(result) => {
            match result {
                Ok(bytes_read) => {
                    if bytes_read == 0 {
                        log!(LogLevel::Error, "Invalid response data received");
                        stream.shutdown().await?;
                        return Ok(ProtocolStatus::MALFORMED);
                    }

                    // Parse the response
                    let response: ProtocolMessage<()> =
                        match ProtocolMessage::<()>::from_bytes(&response_buffer).await {
                            Ok(res) => res,
                            Err(_) => {
                                stream.shutdown().await?;
                                return Ok(ProtocolStatus::MALFORMED);
                            }
                        };

                    let response_status: ProtocolStatus =
                        ProtocolStatus::from_bits_truncate(response.header.status);

                    if response_status.expect(ProtocolStatus::SIDEGRADE) {
                        log!(LogLevel::Debug, "SideGrade requested");
                        if sidegrade {
                            message.header.flags = response.header.reserved;
                            log!(LogLevel::Debug, "Sending the sidegrade request");
                            return Box::pin(send_message_tcp(stream, message, sidegrade)).await;
                        } else {
                            log!(
                                LogLevel::Debug,
                                "The server requested a sidegrade for a message we won't sidegrade"
                            );
                            stream.shutdown().await?;
                            return Ok(ProtocolStatus::REFUSED);
                        }
                    }

                    stream.shutdown().await?;
                    return Ok(response_status);
                }
                Err(err) => {
                    stream.shutdown().await?;
                    match err.kind() {
                        io::ErrorKind::UnexpectedEof => return Ok(ProtocolStatus::MALFORMED),
                        _ => return Err(err),
                    }
                }
            }
        }
        Err(_) => {
            stream.shutdown().await?;
            return Ok(ProtocolStatus::TIMEDOUT);
        }
    }
}

pub async fn receive_message_tcp<T>(
    stream: &mut TcpStream,
    auto_reply: bool,
) -> io::Result<ProtocolMessage<T>>
where
    T: serde::de::DeserializeOwned + std::fmt::Debug + serde::Serialize + Clone,
{
    let mut buffer: Vec<u8> = read_until(stream, EOL.as_bytes().to_vec()).await?;
    stream.flush().await?;

    if let Some(pos) = buffer
        .windows(EOL.len())
        .rposition(|window| window == EOL.as_bytes())
    {
        buffer.truncate(pos);
    }

    // log!(LogLevel::Info, "Recieved Buffer: {:#?}", buffer);

    match ProtocolMessage::<T>::from_bytes(&buffer).await {
        Ok(message) => {
            log!(LogLevel::Trace, "Received message: {:?}", message);

            if auto_reply {
                let mut response: ProtocolMessage<()> =
                    ProtocolMessage::new(Flags::NONE, ()).unwrap();
                response.header.status = ProtocolStatus::OK.bits();
                response.header.reserved = Reserved::NONE.bits();

                // Connect to the server asynchronously
                let serialized_data = response.to_bytes().await?;

                // Send the length prefix and the data asynchronously
                let scratch = stream.try_write(&serialized_data)?;
                log!(LogLevel::Trace, "Response bytes sent: {scratch}");
                log!(LogLevel::Debug, "Sent header: {}", response.header);
            }

            stream.flush().await?;

            Ok(message)
        }
        Err(e) => {
            log!(LogLevel::Error, "Deserialization error: {}", e);
            let mut error_response = ProtocolMessage::new(Flags::NONE, ()).unwrap();
            error_response.header.status = ProtocolStatus::ERROR.bits();
            let error = error_response.to_bytes().await?;
            stream.write_all(&error).await?;
            stream.flush().await?;

            return Err(io::Error::new(io::ErrorKind::InvalidData, e));
        }
    }
}

// Socket communications
pub async fn send_message_unix<T>(
    path: &str,
    message: &mut ProtocolMessage<T>,
) -> io::Result<ProtocolStatus>
where
    T: serde::Serialize + DeserializeOwned + std::fmt::Debug + Clone,
{
    // set origin to 0.0.0.0 to indicate local transfer
    message.header.origin_address = [0, 0, 0, 0];
    let mut stream: UnixStream = UnixStream::connect(path).await?;
    let mut serialized_data = message.to_bytes().await?;
    serialized_data.extend_from_slice(EOL.as_bytes()); // Append EOL for message termination

    // Send the data over the Unix socket
    stream.write_all(&serialized_data).await?;
    log!(LogLevel::Trace, "Message sent to Unix socket at {}", path);

    // Timeout duration for receiving a response
    let mut response_buffer = Vec::with_capacity(10);

    // Read the response data with a timeout
    log!(LogLevel::Trace, "Reading response data...");
    tokio::time::sleep(Duration::from_micros(1)).await;
    match stream.read_buf(&mut response_buffer).await {
        Ok(bytes_read) => {
            if bytes_read == 0 {
                log!(LogLevel::Error, "Received empty response data");
                stream.shutdown().await?;
                return Ok(ProtocolStatus::MALFORMED);
            }

            let response: ProtocolMessage<()> =
                ProtocolMessage::from_bytes(&response_buffer).await?;
            let response_status = ProtocolStatus::from_bits_truncate(response.header.status);
            log!(LogLevel::Trace, "Received response: {:?}", response);
            return Ok(response_status);
        }
        Err(err) => return Err(err),
    };
}

pub async fn receive_message_unix<T>(mut stream: &mut UnixStream) -> io::Result<ProtocolMessage<T>>
where
    T: serde::de::DeserializeOwned + std::fmt::Debug + serde::Serialize + Clone,
{
    // Read until EOL to get the entire message
    let mut buffer: Vec<u8> = read_until(&mut stream, EOL.as_bytes().to_vec()).await?;

    // Truncate the EOL from the buffer
    if let Some(pos) = buffer
        .windows(EOL.len())
        .rposition(|window| window == EOL.as_bytes())
    {
        buffer.truncate(pos);
    }

    // Deserialize and handle the message
    match ProtocolMessage::<T>::from_bytes(&buffer).await {
        Ok(message) => {
            log!(LogLevel::Debug, "Received message: {:?}", message);

            // Prepare a response message
            let mut response: ProtocolMessage<()> = ProtocolMessage::new(Flags::NONE, ())?;
            response.header.status = ProtocolStatus::OK.bits();
            response.header.reserved = Reserved::NONE.bits();
            response.header.origin_address = [0, 0, 0, 0];

            let serialized_response = response.to_bytes().await?;
            let num: usize = stream.write(&serialized_response).await?;
            log!(LogLevel::Trace, "Number of bytes sent in response: {}", num);

            Ok(message)
        }
        Err(e) => {
            log!(LogLevel::Error, "Deserialization error: {}", e);

            // Send an error response if deserialization fails
            let mut error_response = ProtocolMessage::new(Flags::ENCODED, ()).unwrap();
            error_response.header.status = ProtocolStatus::ERROR.bits();
            let error_bytes = error_response.to_bytes().await?;
            stream.write_all(&error_bytes).await?;
            stream.shutdown().await?;

            Err(io::Error::new(io::ErrorKind::InvalidData, e))
        }
    }
}

// Helpers
fn read_with_std_io<R: Read>(reader: &mut R, buffer: &mut [u8]) -> io::Result<()> {
    reader.read_exact(buffer)?;
    Ok(())
}

pub async fn read_with_tokio_io<R: AsyncReadExt + Unpin>(
    reader: &mut R,
    buffer: &mut Vec<u8>,
) -> io::Result<()> {
    reader.read_to_end(buffer).await?;
    Ok(())
}

pub async fn read_until<T>(stream: &mut T, delimiter: Vec<u8>) -> io::Result<Vec<u8>>
where
    T: AsyncReadExt + Unpin,
{
    let mut result_buffer: Vec<u8> = Vec::new();
    let delimiter_len = delimiter.len();

    loop {
        // Buffer for reading a single byte at a time
        let mut byte = [0u8];

        // Read one byte
        let bytes_read = stream.read(&mut byte).await?;
        if bytes_read == 0 {
            // End of stream reached without finding the delimiter
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "Delimiter not found",
            ));
        }

        // Append the byte to the result buffer
        result_buffer.push(byte[0]);

        // Check if the end of result_buffer matches the delimiter
        if result_buffer.len() >= delimiter_len
            && result_buffer[result_buffer.len() - delimiter_len..] == delimiter[..]
        {
            // Found the delimiter; return the buffer up to (and including) it
            return Ok(result_buffer);
        }
    }
}
