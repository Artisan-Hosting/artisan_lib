use dusa_collection_utils::{
    errors::{ErrorArrayItem, Errors as SE},
    stringy::Stringy,
    types::PathType,
};
use nix::unistd::{chown, Gid, Uid};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::PathBuf;
use dusa_collection_utils::errors::Errors;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::UnixStream,
};

use crate::{
    communication::{GeneralMessage, MessageType, Status},
    config::{Aggregator, AppConfig},
    log,
    logger::LogLevel,
    version::SoftwareVersion,
};

/// Encodes a message with a length prefix and sends it over the stream.
pub async fn send_message<T: Serialize>(
    stream: &mut UnixStream,
    message: &T,
) -> Result<(), ErrorArrayItem> {
    // Serialize the message into bytes
    let message_bytes = serde_json::to_vec(message).map_err(|e| {
        ErrorArrayItem::new(Errors::GeneralError, format!("Serialization error: {}", e))
    })?;
    
    // Get the length of the message and encode it as a 4-byte big-endian array
    let length_bytes = (message_bytes.len() as u32).to_be_bytes();
    
    // Send the length of the message first
    stream.write_all(&length_bytes).await.map_err(|e| {
        ErrorArrayItem::new(
            Errors::GeneralError,
            format!("Failed to write message length: {}", e),
        )
    })?;

    // Log the length bytes for debugging
    log!(
        LogLevel::Trace,
        "Sent message length: {:?} bytes: {:#?}",
        message_bytes.len(),
        length_bytes
    );
    
    // Send the actual message bytes
    stream.write_all(&message_bytes).await.map_err(|e| {
        ErrorArrayItem::new(
            Errors::GeneralError,
            format!("Failed to send message: {}", e),
        )
    })?;

    // Log the message bytes for debugging
    log!(
        LogLevel::Trace,
        "Sent message: {:#?}",
        message_bytes
    );

    Ok(())
}

/// Reads a length-prefixed message from the stream and decodes it.
pub async fn receive_message(stream: &mut UnixStream) -> Result<Vec<u8>, ErrorArrayItem> {
    let mut length_bytes = [0u8; 4];

    // Read the length prefix (4 bytes)
    stream
        .read_exact(&mut length_bytes)
        .await
        .map_err(|e| {
            ErrorArrayItem::new(
                Errors::GeneralError,
                format!("Failed to read message length: {}", e)
            )
        })?;

    // Convert the 4-byte array into a usize value
    let length = u32::from_be_bytes(length_bytes) as usize;

    // Log the received length for debugging
    log!(
        LogLevel::Trace,
        "Received message length: {} bytes",
        length
    );

    // Prepare a buffer of the size specified by the length prefix
    let mut message_bytes = vec![0u8; length];

    // Read the message body
    stream
        .read_exact(&mut message_bytes)
        .await
        .map_err(|e| {
            ErrorArrayItem::new(
                Errors::GeneralError,
                format!("Failed to read message body of length {}: {}", length, e)
            )
        })?;

    // Log the message bytes for debugging
    log!(
        LogLevel::Trace,
        "Received message: {:#?}",
        message_bytes
    );

    // Deserialize the message bytes into a `GeneralMessage`
    // let message: GeneralMessage = serde_json::from_slice(&message_bytes.as_slice()).map_err(|e| {
    //     ErrorArrayItem::new(
    //         Errors::GeneralError,
    //         format!(
    //             "Failed to deserialize message: {}, message bytes: {:?}",
    //             e,
    //             String::from_utf8_lossy(&message_bytes)
    //         )
    //     )
    // })?;

    Ok(message_bytes)
}

/// Sends an acknowledgment message over the stream.
pub async fn send_acknowledge(stream: &mut UnixStream, version: SoftwareVersion) {
    let ack_message = GeneralMessage {
        version: version,
        msg_type: MessageType::Acknowledgment,
        payload: json!({"message_received": true}),
        error: None,
    };
    // Fire-and-forget acknowledgment, ignoring result
    let _ = send_message(stream, &ack_message).await;
}

/// Reports status to the aggregator.
pub async fn report_status(status: Status, socket_path: &PathType) -> Result<(), ErrorArrayItem> {
    let mut stream: UnixStream = get_socket_stream(socket_path).await?;

    let general_message = GeneralMessage {
        version: SoftwareVersion::new(env!("CARGO_PKG_VERSION")),
        msg_type: MessageType::StatusUpdate,
        payload: serde_json::to_value(&status).map_err(ErrorArrayItem::from)?,
        error: None,
    };

    send_message(&mut stream, &general_message).await
}

/// Returns the path to the socket.
// pub fn get_socket_path(
//     int: bool,
//     mut errors: ErrorArray,
//     mut warnings: WarningArray,
// ) -> uf<OkWarning<PathType>> {
//     let socket_file = PathType::Content(String::from("/var/run/ais.sock"));
//     let socket_dir = match socket_file.ancestors().next() {
//         Some(d) => PathType::PathBuf(d.to_path_buf()),
//         None => {
//             errors.push(ErrorArrayItem::new(
//                 SE::InvalidFile,
//                 "Socket file not found".to_string(),
//             ));
//             return uf::new(Err(errors));
//         }
//     };

//     if int && socket_file.exists() {
//         match del_file(socket_file.clone(), errors.clone(), warnings.clone()).uf_unwrap() {
//             Ok(_) => return uf::new(Ok(OkWarning { data: socket_file, warning: warnings })),
//             Err(_) => warnings.push(WarningArrayItem::new(Warnings::OutdatedVersion)),
//         }
//     }

//     uf::new(Ok(OkWarning {
//         data: socket_file,
//         warning: warnings,
//     }))
// }

/// Gets a &mut stream from a path given if it's a valid socket
pub async fn get_socket_stream(path: &PathType) -> Result<UnixStream, ErrorArrayItem> {
    match path.exists() {
        true => UnixStream::connect(path)
            .await
            .map_err(ErrorArrayItem::from),
        false => {
            return Err(ErrorArrayItem::new(
                SE::InvalidFile,
                "File given doesn't exist".to_owned(),
            ))
        }
    }
}

pub fn get_socket(config: &AppConfig) -> PathType {
    let aggregator_info: &Option<Aggregator> = &config.aggregator;
    let socket_path = match aggregator_info {
        Some(agg) => {
            let path = agg.socket_path.clone();
            let permissions = agg.socket_permission;
            // * add logic to verify the socket has the right permissions
            path
        }
        None => PathType::Str("/tmp/monitor.sock".into()),
    };
    socket_path
}

/// Sets ownership of the socket file to the given UID and GID.
pub fn set_socket_ownership(path: &PathBuf, uid: Uid, gid: Gid) -> Result<(), ErrorArrayItem> {
    chown(path, Some(uid), Some(gid)).map_err(ErrorArrayItem::from)
}
