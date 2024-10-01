use std::{collections::HashMap, path::PathBuf};
use dusa_collection_utils::{
    errors::{
        ErrorArrayItem, Errors as SE,
    }, stringy::Stringy, types::PathType
};
use nix::unistd::{chown, Gid, Uid};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::UnixStream,
};

/// Encodes a message with a length prefix and sends it over the stream.
pub async fn send_message<T: Serialize>(
    stream: &mut UnixStream,
    message: &T,
) -> Result<(), ErrorArrayItem> {
    let message_bytes = serde_json::to_vec(message).map_err(ErrorArrayItem::from)?;
    let length_bytes = (message_bytes.len() as u32).to_be_bytes();

    stream
        .write_all(&length_bytes)
        .await
        .map_err(ErrorArrayItem::from)?;

    stream
        .write_all(&message_bytes)
        .await
        .map_err(ErrorArrayItem::from)?;

    Ok(())
}

/// Reads a length-prefixed message from the stream and decodes it.
pub async fn receive_message(stream: &mut UnixStream) -> Result<GeneralMessage, ErrorArrayItem> {
    let mut length_bytes = [0u8; 4];
    stream
        .read_exact(&mut length_bytes)
        .await
        .map_err(ErrorArrayItem::from)?;

    let length = u32::from_be_bytes(length_bytes) as usize;
    let mut message_bytes = vec![0u8; length];
    stream
        .read_exact(&mut message_bytes)
        .await
        .map_err(ErrorArrayItem::from)?;

    serde_json::from_slice(&message_bytes).map_err(ErrorArrayItem::from)
}

/// Sends an acknowledgment message over the stream.
pub async fn send_acknowledge(stream: &mut UnixStream) {
    let ack_message = GeneralMessage {
        version: Stringy::from_string(env!("CARGO_PKG_VERSION").to_string()),
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
        version: Stringy::from_string(env!("CARGO_PKG_VERSION").to_string()),
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
        true => {
            UnixStream::connect(path)
            .await
            .map_err(ErrorArrayItem::from)
        },
        false => return Err(ErrorArrayItem::new(SE::InvalidFile, "File given doesn't exist".to_owned())),
    }
}

/// Sets ownership of the socket file to the given UID and GID.
pub fn set_socket_ownership(path: &PathBuf, uid: Uid, gid: Gid) -> Result<(), ErrorArrayItem> {
    chown(path, Some(uid), Some(gid)).map_err(ErrorArrayItem::from)
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub enum QueryType {
    Status,
    AllStatuses,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct QueryMessage {
    pub query_type: QueryType,
    pub app_name: Option<AppName>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct QueryResponse {
    pub version: Stringy,
    pub app_status: Option<Status>,
    pub all_statuses: Option<HashMap<AppName, Status>>, // New field for all statuses
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub enum MessageType {
    StatusUpdate,
    Acknowledgment,
    Query,
}

/// General structure for messages
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GeneralMessage {
    pub version: Stringy,
    pub msg_type: MessageType,
    pub payload: serde_json::Value,
    pub error: Option<Stringy>, // Simplified for this example
}

/// Enum representing the status of an application.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub enum AppStatus {
    Running,
    Stopped,
    TimedOut,
    Warning,
}

/// Enum representing the name of an application.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, PartialOrd, Ord, Eq, Hash)]
pub enum AppName {
    // These are artisan_platform components
    Github,
    Directive,
    Apache,
    Systemd,
    // Firewall,
    Security,
}

/// Struct representing the status of an application at a specific time.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct Status {
    pub app_name: AppName,
    pub app_status: AppStatus,
    pub timestamp: u64,
    pub version: Stringy, // Add version field
}
