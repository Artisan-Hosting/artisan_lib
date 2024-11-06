use bincode;
use dusa_collection_utils::{
    errors::{ErrorArrayItem, Errors, UnifiedResult},
    log,
    log::LogLevel,
};
// use dusa_collection_utils::version::Version;
// For serialization/deserialization
use flate2::{read::GzDecoder, write::GzEncoder, Compression};
use recs::{decrypt_raw, encrypt_raw, initialize};
use serde::{Deserialize, Serialize};
// use serde_derive::{Deserialize, Serialize};
use std::{
    fmt::Debug,
    io::{self, Cursor, Read, Write},
    net::{TcpListener, TcpStream},
    os::unix::net::{UnixListener, UnixStream},
};

use crate::{encryption::{decrypt_data, encrypt_data}, network::{get_header_version, get_local_ip}};

#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
pub enum ProtocolStatus {
    Ok,

    Error,
    Waiting,
    Other(u8), // For extensibility, allows future custom statuses
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ProtocolHeader {
    pub version: u16,
    pub flags: u16,
    pub payload_length: u32,
    pub reserved: u16,
    pub status: ProtocolStatus,
    pub origin_address: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ProtocolMessage<T> {
    pub header: ProtocolHeader,
    pub payload: T,
}

// Define flags
bitflags::bitflags! {
    pub struct Flags: u16 {
        const NONE       = 0b0000_0000;
        const COMPRESSED = 0b0000_0001;
        const ENCRYPTED  = 0b0000_0010;
        const ENCODED    = 0b0000_0100;
        const RESERVED   = 0b0000_1000;
        // Add other flags as needed
    }
}

impl<T> ProtocolMessage<T>
where
    T: Serialize + for<'a> Deserialize<'a>,
{
    // Create a new protocol message
    pub fn new(flags: Flags, payload: T) -> io::Result<Self> {
        let header = ProtocolHeader {
            version: get_header_version(),
            flags: flags.bits(),
            payload_length: 0, // Will be set in to_bytes
            reserved: 0,
            status: ProtocolStatus::Ok,
            origin_address: Some(get_local_ip().to_string()),
        };

        Ok(Self { header, payload })
    }

    // Serialize the message into bytes with optional compression
    pub async fn to_bytes(&mut self) -> io::Result<Vec<u8>> {
        let mut payload_bytes = bincode::serialize(&self.payload)
            .map_err(|err| io::Error::new(io::ErrorKind::Other, err.to_string()))?;

        if Flags::from_bits_truncate(self.header.flags).contains(Flags::ENCRYPTED) {
            payload_bytes = encrypt_data(&payload_bytes)
                .await
                .uf_unwrap()
                .map_err(|err| io::Error::new(io::ErrorKind::Other, err.to_string()))?;
            log!(LogLevel::Trace, "Encryption Bit Set");
        }

        if Flags::from_bits_truncate(self.header.flags).contains(Flags::ENCODED) {
            payload_bytes = encode_data(&payload_bytes);
            log!(LogLevel::Trace, "Encoding Bit Set");
        }

        if Flags::from_bits_truncate(self.header.flags).contains(Flags::COMPRESSED) {
            payload_bytes = compress_data(&payload_bytes)?;
            log!(LogLevel::Trace, "Compression Bit Set");
        }

        // Update the payload_length in the header
        self.header.payload_length = payload_bytes.len() as u32;

        // Serialize the updated header
        let header_bytes = bincode::serialize(&self.header)
            .map_err(|err| io::Error::new(io::ErrorKind::Other, err.to_string()))?;

        let mut buffer = Vec::new();
        buffer.extend(header_bytes);
        buffer.extend(payload_bytes);
        Ok(buffer)
    }

    // Deserialize a message from bytes, decompress if needed
    pub async fn from_bytes(bytes: &[u8]) -> io::Result<Self> {
        let mut cursor = Cursor::new(bytes);
        log!(LogLevel::Debug, "Message length received: {}", bytes.len());

        // Deserialize the header
        let header: ProtocolHeader = bincode::deserialize_from(&mut cursor).map_err(|err| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Header deserialization error: {}", err),
            )
        })?;

        // Read the payload bytes
        let mut payload_bytes = vec![0u8; header.payload_length as usize];
        cursor.read_exact(&mut payload_bytes)?;

        if Flags::from_bits_truncate(header.flags).contains(Flags::ENCRYPTED) {
            payload_bytes = decrypt_data(&payload_bytes)
                .await
                .uf_unwrap()
                .map_err(|err| io::Error::new(io::ErrorKind::Other, err.to_string()))?;
            log!(LogLevel::Trace, "Encryption Bit Set");
        }

        if Flags::from_bits_truncate(header.flags).contains(Flags::ENCODED) {
            payload_bytes = decode_data(&payload_bytes)
                .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err.to_string()))?;
        }

        if Flags::from_bits_truncate(header.flags).contains(Flags::COMPRESSED) {
            payload_bytes = decompress_data(&payload_bytes).map_err(|err| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("Decompression error: {}", err),
                )
            })?;
        }

        let payload: T = bincode::deserialize(&payload_bytes).map_err(|err| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Payload deserialization error: {}", err),
            )
        })?;

        Ok(Self { header, payload })
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

// TCP communications
pub async fn send_message_tcp<T: serde::Serialize>(
    address: &str,
    message: &mut ProtocolMessage<T>,
) -> io::Result<ProtocolStatus>
where
    T: for<'de> Deserialize<'de>,
{
    let mut stream = TcpStream::connect(address)?;
    let serialized_data = message.to_bytes().await?;

    // Send the data over the stream
    stream.write_all(&serialized_data)?;
    log!(LogLevel::Trace, "Message sent to {}", address);

    // Wait for a response to check if it succeeded or failed
    let mut response_buffer = Vec::new();
    stream.read_to_end(&mut response_buffer)?;

    let response: ProtocolMessage<()> = ProtocolMessage::from_bytes(&response_buffer).await?;
    Ok(response.header.status)
}

pub async fn receive_message_tcp<T>(address: &str) -> io::Result<ProtocolMessage<T>>
where
    T: serde::de::DeserializeOwned + std::fmt::Debug + serde::Serialize,
{
    let listener = TcpListener::bind(address)?;

    for stream in listener.incoming() {
        let mut stream = stream?;
        let mut buffer = Vec::new();

        stream.read_to_end(&mut buffer)?;

        match ProtocolMessage::from_bytes(&buffer).await {
            Ok(message) => {
                log!(LogLevel::Trace, "Message received: {:?}", message);
                // Respond with a status message to acknowledge success
                let mut response = ProtocolMessage {
                    header: ProtocolHeader {
                        version: get_header_version(),
                        flags: 0,
                        payload_length: 0,
                        reserved: 0,
                        status: ProtocolStatus::Ok,
                        origin_address: Some(get_local_ip().to_string()), // Indicating the success of receiving and parsing
                    },
                    payload: (),
                };
                let response_bytes = response.to_bytes().await?;
                stream.write_all(&response_bytes)?;
                return Ok(message);
            }
            Err(err) => {
                log!(LogLevel::Error, "Deserialization error: {}", err);
                // Respond with an error status if deserialization fails
                let mut error_response = ProtocolMessage {
                    header: ProtocolHeader {
                        version: get_header_version(),
                        flags: 0,
                        payload_length: 0,
                        reserved: 0,
                        status: ProtocolStatus::Error,
                        origin_address: Some(get_local_ip().to_string()),
                    },
                    payload: (),
                };
                let error_bytes = error_response.to_bytes().await?;
                stream.write_all(&error_bytes)?;
                return Err(err);
            }
        }
    }

    Err(io::Error::new(io::ErrorKind::Other, "No message received"))
}


// Socket communications
pub async fn send_message_unix<T: serde::Serialize>(
    path: &str,
    message: &mut ProtocolMessage<T>,
) -> io::Result<()>
where
    T: for<'de> Deserialize<'de>,
{
    let mut stream = UnixStream::connect(path)?;
    let serialized_data = message.to_bytes().await?;

    // Send the data over the Unix socket
    stream.write_all(&serialized_data)?;
    log!(LogLevel::Trace, "Message sent to Unix socket at {}", path);
    Ok(())
}

pub async fn receive_message_unix<T>(path: &str) -> io::Result<ProtocolMessage<T>>
where
    T: serde::de::DeserializeOwned + serde::Serialize + Debug,
{
    let listener = UnixListener::bind(path)?;

    for stream in listener.incoming() {
        let mut stream = stream?;
        let mut buffer = Vec::new();

        // Read the incoming data into the buffer
        stream.read_to_end(&mut buffer)?;
        let message: ProtocolMessage<T> = ProtocolMessage::from_bytes(&buffer).await?;

        log!(LogLevel::Trace, "Message received: {:?}", message);
        return Ok(message);
    }

    Err(io::Error::new(io::ErrorKind::Other, "No message received"))
}
