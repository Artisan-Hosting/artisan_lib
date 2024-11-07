use dusa_collection_utils::{
    errors::{ErrorArrayItem, Errors}, log::LogLevel, log, stringy::Stringy
};
use serde::{Deserialize, Serialize};
use tokio::net::TcpStream;
use std::fmt;

use crate::{communication_proto::{send_message_tcp, Flags, ProtocolMessage, ProtocolStatus}, encryption::encrypt_text};

const MAIL_ADDRESS: &str = "45.137.192.70:1827";

/// Represents an email message.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Email {
    /// The subject of the email.
    pub subject: Stringy,
    /// The body of the email.
    pub body: Stringy,
}

/// Represents an encrypted email message.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct EmailSecure {
    /// The encrypted email data.
    pub data: Stringy,
}

// Display implementations
impl fmt::Display for Email {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{},{}", self.subject, self.body)
    }
}

impl fmt::Display for EmailSecure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.data)
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
}

impl EmailSecure {
    /// Creates a new EmailSecure instance by encrypting the provided email.
    pub async fn new(email: Email) -> Result<Self, ErrorArrayItem> {
        if !email.is_valid() {
            return Err(ErrorArrayItem::new(
                Errors::GeneralError,
                "Invalid Email Data".to_owned(),
            ));
        }

        let plain_email_data: Stringy =
            Stringy::from_string(format!("{}-=-{}", email.subject, email.body));
        let encrypted_data: Stringy = encrypt_text(plain_email_data).await?;

        Ok(EmailSecure {
            data: encrypted_data,
        })
    }

    pub fn to_json(&self) -> Result<String, ErrorArrayItem> {
        serde_json::to_string(self).map_err(|err| ErrorArrayItem::from(err))
    }

    /// Sends the encrypted email data over a TCP stream.
    pub async fn send(&self) -> Result<(), ErrorArrayItem> {
        // Attempt to connect to the specified address

        let mut message: ProtocolMessage<String> = ProtocolMessage::new(Flags::COMPRESSED | Flags::ENCODED, self.to_json()?)?;
        let mut stream = TcpStream::connect(MAIL_ADDRESS).await?;

        match send_message_tcp(&mut stream, &mut message).await.map_err(|err| ErrorArrayItem::from(err)) {
            Ok(status) => match status.expect(ProtocolStatus::OK){
                true => return Ok(()),
                false => {
                    log!(LogLevel::Error, "Email failed to send ? {}", status);
                    return Ok(())
                },
            },
            Err(error) => Err(error),
        }

    }
}
