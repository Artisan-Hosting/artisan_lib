use colored::Colorize;
use dusa_collection_utils::{
    errors::{ErrorArrayItem, Errors}, log::LogLevel, log, stringy::Stringy
};
use serde::{Deserialize, Serialize};
use tokio::net::TcpStream;
use std::fmt;

use crate::communication_proto::{send_message_tcp, Flags, ProtocolMessage, ProtocolStatus};

const MAIL_ADDRESS: &str = "45.137.192.70:1827";

/// Represents an email message.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Email {
    /// The subject of the email.
    pub subject: Stringy,
    /// The body of the email.
    pub body: Stringy,
}

// Display implementation for Email
impl fmt::Display for Email {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Subject: {}, Body: {}", self.subject.bold().blue(), self.body.bold().blue())
    }
}

impl Email {
    /// Creates a new Email instance with the given subject and body.
    pub fn new(subject: Stringy, body: Stringy) -> Self {
        Email { subject, body }
    }

    /// Checks if the email data is valid.
    pub fn is_valid(&self) -> bool {
        !self.subject.is_empty() && !self.body.is_empty()
    }

    /// Converts the email to JSON format.
    pub fn to_json(&self) -> Result<String, ErrorArrayItem> {
        serde_json::to_string(self).map_err(|err| ErrorArrayItem::from(err))
    }

    /// Creates an Email instance from JSON data.
    pub fn from_json(json_data: &str) -> Result<Self, ErrorArrayItem> {
        serde_json::from_str(json_data).map_err(|err| ErrorArrayItem::from(err))
    }

    /// Sends the email data over a TCP stream.
    pub async fn send(&self, addr: Option<&str>) -> Result<(), ErrorArrayItem> {
        if !self.is_valid() {
            return Err(ErrorArrayItem::new(
                Errors::GeneralError,
                "Invalid Email Data".to_owned(),
            ));
        }

        let mut message: ProtocolMessage<String> = ProtocolMessage::new(Flags::COMPRESSED | Flags::ENCODED | Flags::ENCRYPTED, self.to_json()?)?;

        let mut stream = match addr {
            Some(addr) => TcpStream::connect(addr).await,
            None => TcpStream::connect(MAIL_ADDRESS).await,
        }?;

        log! {LogLevel::Trace, "Connected to: {:#?}", stream.peer_addr().unwrap()};

        match send_message_tcp(&mut stream, &mut message).await.map_err(|err| ErrorArrayItem::from(err)) {
            Ok(status) => match status.expect(ProtocolStatus::OK) {
                true => Ok(()),
                false => {
                    log!(LogLevel::Error, "Email failed to send ? {}", status);
                    Ok(())
                },
            },
            Err(error) => Err(error),
        }
    }
}
