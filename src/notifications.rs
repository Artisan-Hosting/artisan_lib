use dusa_collection_utils::{
    errors::{ErrorArrayItem, Errors},
    stringy::Stringy,
};
use serde::{Deserialize, Serialize};
use std::{fmt, net::TcpStream};

use crate::{encryption::encrypt_text, network_communication::send_message};

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
    pub fn new(email: Email) -> Result<Self, ErrorArrayItem> {
        if !email.is_valid() {
            return Err(ErrorArrayItem::new(
                Errors::GeneralError,
                "Invalid Email Data".to_owned(),
            ));
        }

        let plain_email_data: Stringy =
            Stringy::from_string(format!("{}-=-{}", email.subject, email.body));
        let encrypted_data: Stringy = encrypt_text(plain_email_data)?;

        Ok(EmailSecure {
            data: encrypted_data,
        })
    }

    /// Sends the encrypted email data over a TCP stream.
    pub fn send(&self) -> Result<(), ErrorArrayItem> {
        // Attempt to connect to the specified address
        let mut stream = TcpStream::connect("45.137.192.70:1827")
        // let stream = TcpStream::connect("127.0.0.1:1827").map_err(|e| ErrorArrayItem::from(e))?;

        // Use send_message to send self.data
        send_message(&stream, self.data.as_bytes())
    }
}
