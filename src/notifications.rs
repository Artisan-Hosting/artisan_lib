use colored::Colorize;
use dusa_collection_utils::{
    errors::{ErrorArrayItem, Errors, OkWarning, UnifiedResult, WarningArray, WarningArrayItem, Warnings},
    log::LogLevel,
    log,
    stringy::Stringy,
};
use serde::{Deserialize, Serialize};
use std::fmt;
use tokio::net::TcpStream;

use crate::communication_proto::{send_message, Flags, Proto, ProtocolStatus};

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
        write!(
            f,
            "Subject: {}, Body: {}",
            self.subject.bold().blue(),
            self.body.bold().blue()
        )
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
    #[rustfmt::skip]
    pub async fn send(&self, addr: Option<&str>) -> UnifiedResult<OkWarning<()>> {
        if !self.is_valid() {
            return UnifiedResult::new(Err(ErrorArrayItem::new(
                Errors::GeneralError,
                "Invalid Email Data".to_owned(),
            )));
        }

        let stream_result: Result<TcpStream, UnifiedResult<OkWarning<()>>> = match addr {
            Some(addr) => TcpStream::connect(addr).await,
            None => TcpStream::connect(MAIL_ADDRESS).await,
        }.map_err(|err| UnifiedResult::new(Err(ErrorArrayItem::from(err))));

        let mut stream = match stream_result {
            Ok(stream) => stream,
            Err(err) => return err,
        };

        log!{LogLevel::Trace, "Connected to: {:#?}", stream.peer_addr().unwrap()};

        let data_result: Result<String, UnifiedResult<OkWarning<()>>> = self.to_json()
            .map_err(|err| UnifiedResult::new(Err(err)));

        let data: String = match data_result {
            Ok(data) => data,
            Err(err) => return err,
        };

        match send_message::<TcpStream, String, ()>(
            &mut stream, Flags::OPTIMIZED, data,
            Proto::TCP, false
        ).await {
            Ok(response) => {
                match response {
                    Ok(response) => {
                        let warning: WarningArrayItem = 
                            WarningArrayItem::new_details(
                            Warnings::UnexpectedBehavior, format!("{}", 
                                    ProtocolStatus::from_bits_truncate(response.header.status))
                            );

                        return UnifiedResult::new(Ok(OkWarning{
                            data: response.payload,
                            warning: WarningArray::new(vec![warning]),
                        }))
                    },
                    Err(error_code) => {
                        let error: ErrorArrayItem = 
                            ErrorArrayItem::new(Errors::Network, format!("{}", error_code));
                        
                        return UnifiedResult::new(Err(error))
                    },
                }
            },
            Err(err) => {
                return UnifiedResult::new(Err(ErrorArrayItem::from(err)))
            },
        }
    }
}
