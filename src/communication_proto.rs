use bincode;
use colored::{Color, ColoredString, Colorize};
use dusa_collection_utils::stringy::Stringy;
use dusa_collection_utils::{errors::ErrorArrayItem, log::LogLevel, version::Version};
use dusa_collection_utils::log;
use flate2::{read::GzDecoder, write::GzEncoder, Compression};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::net::IpAddr;
use std::{
    fmt::{self, Debug, Display},
    io::{self, Cursor, Read, Write},
    vec,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::version::aml_version;
use crate::{
    encryption::{decrypt_data, encrypt_data},
    network::{get_header_version, get_local_ip},
};

const HEADER_VERSION_LEN: usize = 2; // u16
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
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct ProtocolStatus: u8 {
        // Status Flags
        const OK        = 0b0000_0001;
        const ERROR     = 0b0000_0010;
        const WAITING   = 0b0000_0100;
        
        // Error Flags
        const MALFORMED = 0b0001_0000; // The message fit what we were expecting but was trash
        const REFUSED   = 0b0010_0000; // Don't retry
        const RESERVED  = 0b0100_0000; // Reciver needs to parse reserved field
        const VERSION   = 0b1000_0000; // The version communicated is the problem

        // Invalid Version Flags

        /// Way out of date. The connection 
        const OUTOFBAND = Self::ERROR.bits() | Self::REFUSED.bits() | Self::VERSION.bits();

        /// Not the current version but we can support you.
        const NOTINBAND = Self::OK.bits() | Self::VERSION.bits();

        // Sidegrade

        /// A request to change the flags the message was send with based on the reserved field
        const SIDEGRADE = Self::WAITING.bits() | Self::MALFORMED.bits() | Self::RESERVED.bits();

        // Time codes

        /// We connected to the client and started data and the they gohsted us
        const TIMEDOUT = Self::ERROR.bits() | Self::WAITING.bits();
        
        /// For uses like discovery where the target maynot exist
        const GAVEUP   = Self::OK.bits() | Self::WAITING.bits();

        /// Using the reserved field. tells client within X seconds I'll send the response to your query
        const WAITSEC  = Self::OK.bits() | Self::WAITING.bits() | Self::RESERVED.bits();
    }
}

impl ProtocolStatus {
    pub fn has_flag(&self, flag: ProtocolStatus) -> bool {
        self.contains(flag)
    }

    pub fn is_error(&self) -> bool {
        self.contains(ProtocolStatus::ERROR)
    }

    pub fn is_ok(&self) -> bool {
        self.contains(ProtocolStatus::OK)
    }

    pub fn is_waiting(&self) -> bool {
        self.contains(ProtocolStatus::WAITING)
    }
    
    pub fn get_status_color(&self) -> Color {
        match *self {
            ProtocolStatus::OK => Color::Green,
            ProtocolStatus::ERROR => Color::Red,
            ProtocolStatus::WAITING => Color::Yellow,
            _ => Color::White,
        }
    }
}

impl fmt::Display for ProtocolStatus {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let description = match *self {
            ProtocolStatus::OK => "OK",
            ProtocolStatus::ERROR => "Error",
            ProtocolStatus::WAITING => "Waiting",
            _ => "Unknown",
        };
        write!(f, "{}", description)
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

#[derive(Debug, Eq, PartialEq, Ord, PartialOrd, Clone, Copy)]
pub enum Proto {
    TCP,
    UNIX,
}

impl fmt::Display for Proto {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let protocol: ColoredString = "PROTOCOL".bold().blue();
        match &self {
            Proto::TCP => write!(f, "{}: {}", protocol, "TCP".green().bold()),
            Proto::UNIX => write!(f, "{}: {}", protocol, "UNIX".green().bold()),
        }
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
    pub version: u16,
    pub flags: u8,
    pub payload_length: u16,
    pub reserved: u8,
    pub status: u8, // Changed from u16 to u8
    pub origin_address: [u8; 4],
}

impl fmt::Display for ProtocolHeader {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let version: Version = Version::decode(self.version);
        
        let origin_addr: Stringy = match self.get_origin_ip() == IpAddr::V4([0, 0, 0, 0].into()) {
            true => Stringy::from("Internal"),
            false => Stringy::from(self.get_origin_ip().to_string()),
        };
        
        write!(
            f,
            "{}\n{}\n{}\n{}\n{}\n{}\n",
            format!("Library Version:  {}", version).bold().green(),
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
            format!("Origin Address:   {}", origin_addr)
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

        let mut version_bytes: [u8; 2] = [0u8; 2];
        read_with_std_io(&mut cursor, &mut version_bytes)?;
        let version = u16::from_be_bytes(version_bytes);

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

        let header: ProtocolHeader = ProtocolHeader {
            version,
            flags,
            payload_length,
            reserved,
            status: status.bits(),
            origin_address,
        };
        log!(LogLevel::Debug, "Recieved header \n{}", header);

        // Deserialize and process payload
        let mut payload = payload_bytes.to_vec();
        let flags = Flags::from_bits_truncate(header.flags);
        for flag in Self::ordered_flags().iter().rev().cloned() {
            if flags.contains(flag) {
                payload = match flag {
                    Flags::ENCRYPTED => decrypt_data(&payload).await.unwrap(),
                    Flags::ENCODED => decode_data(&payload).unwrap(),
                    Flags::COMPRESSED => decompress_data(&payload)?,
                    Flags::SIGNATURE => verify_checksum(payload),
                    Flags::NONE => payload,
                    _ => payload,
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

    /// returns a sendable Vec<u8> with the EOL appended
    pub async fn format(self) -> Result<Vec<u8>, io::Error> {
        let mut message: ProtocolMessage<T> = self;
        let mut message_bytes: Vec<u8> = message.to_bytes().await?;
        message_bytes.extend_from_slice(EOL.as_bytes());
        return Ok(message_bytes);
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

pub async fn send_message<STREAM, DATA, RESPONSE>(
    mut stream: &mut STREAM,
    flags: Flags,
    data: DATA,
    proto: Proto,
    insecure: bool,
) -> Result<Result<ProtocolMessage<RESPONSE>, ProtocolStatus>, io::Error>
where
    STREAM: AsyncReadExt + AsyncWriteExt + Unpin,
    DATA: serde::de::DeserializeOwned + std::fmt::Debug + serde::Serialize + Clone + Unpin,
    RESPONSE: serde::de::DeserializeOwned + std::fmt::Debug + serde::Serialize + Clone + Unpin,
{
    let mut message: ProtocolMessage<DATA> = ProtocolMessage::new(flags, data.clone())?;

    match proto {
        Proto::TCP => message.header.origin_address = get_local_ip().octets(),
        Proto::UNIX => message.header.origin_address = [0, 0, 0, 0],
    };

    // Ensure that we send a header with empty reserved field
    // message.header.reserved = Flags::NONE.bits();

    // Creating message bytes and appending eol
    let mut serialized_message: Vec<u8> = message.to_bytes().await?;
    serialized_message.extend(EOL.as_bytes());

    log!(LogLevel::Trace, "message serialized for sending");

    // sending the data
    match proto {
        Proto::TCP => {
            send_data(stream, serialized_message, Proto::TCP).await?;
            log!(LogLevel::Trace, "Message sent over tcp");
        }
        Proto::UNIX => {
            send_data(stream, serialized_message, Proto::UNIX).await?;
            log!(LogLevel::Trace, "Message sent over unix socket")
        }
    }

    // Sleep a second for unix socket issues
    // tokio::time::sleep(Duration::from_micros(500)).await;
    match read_until(&mut stream, EOL.as_bytes().to_vec()).await {
        Ok(response_buffer) => {
            if response_buffer.is_empty() {
                log!(LogLevel::Error, "Received empty response data");
                stream.shutdown().await?;
                return Ok(Err(ProtocolStatus::MALFORMED));
            }

            let response: ProtocolMessage<RESPONSE> =
                ProtocolMessage::from_bytes(&response_buffer).await?;

            let response_status: ProtocolStatus =
                ProtocolStatus::from_bits_truncate(response.header.status);

            let response_reserved: Flags = Flags::from_bits_truncate(response.header.reserved);

            let response_version: Version = Version::decode(response.header.version);

            let in_band = Version::compare_versions(&aml_version(), &response_version);

            if !insecure {
                if !in_band {
                    return Ok(Err(ProtocolStatus::NOTINBAND))
                }
            }
            
            if response_status.has_flag(ProtocolStatus::SIDEGRADE) {
                log!(LogLevel::Debug, "SideGrade requested");
                match insecure {
                    true => {
                       return match proto {
                            Proto::TCP => Box::pin(send_message::<STREAM, DATA, RESPONSE>(stream, response_reserved, data, proto, insecure)).await,
                            Proto::UNIX => Box::pin(send_message::<STREAM, DATA, RESPONSE>(stream, response_reserved, data, proto, insecure)).await,
                        };
                    }
                    false => {
                        log!(LogLevel::Info, "Sidegrade not allowed dropping connections");
                        stream.shutdown().await?;
                        return Ok(Err(ProtocolStatus::REFUSED));
                    }
                }
            }
            log!(LogLevel::Trace, "Received response: {:?}", response);
            return Ok(Ok(response));
        }
        Err(err) => return Err(err),
    }
}

pub async fn receive_message<STREAM, RESPONSE>(
    stream: &mut STREAM,
    auto_reply: bool,
    proto: Proto,
) -> io::Result<ProtocolMessage<RESPONSE>>
where
    STREAM: AsyncReadExt + AsyncWriteExt + Unpin,
    RESPONSE: serde::de::DeserializeOwned + std::fmt::Debug + serde::Serialize + Clone + Display,
{
    let mut buffer: Vec<u8> = read_until(stream, EOL.as_bytes().to_vec()).await?;

    if proto == Proto::TCP {
        stream.flush().await?;
    }

    if let Some(pos) = buffer
        .windows(EOL.len())
        .rposition(|window| window == EOL.as_bytes())
    {
        buffer.truncate(pos);
    }

    match ProtocolMessage::<RESPONSE>::from_bytes(&buffer).await {
        Ok(message) => {
            log!(LogLevel::Debug, "Received message: {:?}", message);

            match auto_reply {
                true => {
                    send_empty_ok(stream, proto).await?;
                    return Ok(message)
                },
                false => return Ok(message),
            }
        }
        Err(err) => {
            log!(LogLevel::Error, "Deserialization error: {}", err);
            send_empty_err(stream, proto).await?;
            return Err(io::Error::new(io::ErrorKind::InvalidData, err));
        },
    }
}


// * Sending and recieving helpers
pub async fn create_response(status: ProtocolStatus) -> Result<Vec<u8>, io::Error> {
    let mut message: ProtocolMessage<()> = ProtocolMessage::new(Flags::NONE, ())?;
    message.header.status = status.bits();
    let mut message_bytes = message.to_bytes().await?;
    message_bytes.extend_from_slice(EOL.as_bytes());
    return Ok(message_bytes);
}

pub async fn send_empty_err<S>(stream: &mut S, proto: Proto) -> Result<(), io::Error>
where
    S: AsyncWriteExt + Unpin,
{
    let response: Vec<u8> = create_response(ProtocolStatus::ERROR).await?;
    send_data(stream, response, proto).await
}

pub async fn send_empty_ok<S>(stream: &mut S, proto: Proto) -> Result<(), io::Error>
where
    S: AsyncWriteExt + Unpin,
{
    let response: Vec<u8> = create_response(ProtocolStatus::OK).await?;
    send_data(stream, response, proto).await
}

pub async fn send_data<S>(stream: &mut S, data: Vec<u8>, proto: Proto) -> Result<(), io::Error>
where
    S: AsyncWriteExt + Unpin,
{
    if let Err(err) = stream.write_all(&data).await {
        return Err(err);
    }

    if proto == Proto::TCP {
        stream.flush().await?
    }

    Ok(())
}

// Read helpers
pub fn read_with_std_io<R: Read>(reader: &mut R, buffer: &mut [u8]) -> io::Result<()> {
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