use colored::Colorize;
use dusa_collection_utils::{
    core::errors::{
        ErrorArrayItem, Errors, OkWarning, UnifiedResult, WarningArray, WarningArrayItem, Warnings,
    },
    core::logger::LogLevel,
    core::types::stringy::Stringy,
    log,
};
use serde::{Deserialize, Serialize};

#[cfg(target_os = "linux")]
use simple_comms::{
    network::send_receive::send_message,
    protocol::{flags::Flags, proto::Proto, status::ProtocolStatus},
};
use std::fmt;
use tokio::net::TcpStream;

/// Default mail server address. Used if no custom address is provided in [`Email::send`].
const MAIL_ADDRESS: &str = "185.187.235.4:1827";

/// Represents an email message containing a subject and a body.
///
/// # Overview
///
/// - **Subject** (`Stringy`): The headline or topic of the email.
/// - **Body** (`Stringy`): The main content of the email.
///
/// This struct provides methods for creating, validating, converting to/from JSON,
/// and sending the email over a TCP stream to a mail server.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Email {
    pub destination: Stringy,
    /// The subject of the email message.
    pub subject: Stringy,
    /// The body content of the email message.
    pub body: Stringy,
}

impl fmt::Display for Email {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "To: {}, Subject: {}, Body: {}",
            self.destination.bold().green(),
            self.subject.bold().blue(),
            self.body.bold().blue()
        )
    }
}

#[cfg(target_os = "linux")]
impl Email {
    /// Creates a new `Email` instance with the provided subject and body.
    ///
    /// # Arguments
    ///
    /// * `subject` - A [`Stringy`] value representing the email's subject line.
    /// * `body` - A [`Stringy`] value representing the email's main content.
    ///
    /// # Example
    /// ```rust
    /// # use dusa_collection_utils::core::types::stringy::Stringy;
    /// # use artisan_middleware::notifications::Email;
    /// let destination = Stringy::from("dwhitfield@artisanhosting.net");
    /// let subject = Stringy::from("Greetings");
    /// let body = Stringy::from("Hello, how are you?");
    /// let email = Email::new(destination, subject, body);
    /// ```
    pub fn new(destination: Stringy, subject: Stringy, body: Stringy) -> Self {
        Email {
            destination,
            subject,
            body,
        }
    }

    /// Checks if the `Email` fields are valid (i.e., not empty).
    ///
    /// # Returns
    ///
    /// * `true` if both `subject` and `body` are non-empty.
    /// * `false` otherwise.
    ///
    /// # Example
    /// ```rust
    /// # use artisan_middleware::notifications::Email;
    /// let email = Email::new("dwhitfield@artisanhosting.net".into(), "Subject".into(), "Body".into());
    /// assert!(email.is_valid());
    /// ```
    pub fn is_valid(&self) -> bool {
        !self.subject.is_empty() && !self.body.is_empty() && !self.destination.is_empty()
    }

    /// Converts this `Email` instance to a JSON string.
    ///
    /// # Errors
    ///
    /// Returns an [`ErrorArrayItem`] if the serialization fails.
    ///
    /// # Example
    /// ```rust
    /// # use artisan_middleware::notifications::Email;
    /// let email = Email::new("dwhitfield@artisanhosting.net".into(), "Subject".into(), "Body".into());
    /// match email.to_json() {
    ///     Ok(json_str) => println!("JSON: {}", json_str),
    ///     Err(err) => eprintln!("Could not serialize email: {}", err),
    /// }
    /// ```
    pub fn to_json(&self) -> Result<String, ErrorArrayItem> {
        serde_json::to_string(self).map_err(ErrorArrayItem::from)
    }

    /// Creates an `Email` instance from a JSON string.
    ///
    /// # Arguments
    ///
    /// * `json_data` - The JSON representation of an `Email`.
    ///
    /// # Errors
    ///
    /// Returns an [`ErrorArrayItem`] if deserialization fails.
    ///
    /// # Example
    /// ```rust
    /// # use artisan_middleware::notifications::Email;
    /// let json_data = r#"{"destination":"dwhitfield@artisanhosting.net","subject":"Hello","body":"World"}"#;
    /// match Email::from_json(json_data) {
    ///     Ok(email) => println!("Email Subject: {}", email.subject),
    ///     Err(err) => eprintln!("Could not deserialize email: {}", err),
    /// }
    /// ```
    pub fn from_json(json_data: &str) -> Result<Self, ErrorArrayItem> {
        serde_json::from_str(json_data).map_err(ErrorArrayItem::from)
    }

    /// Sends this `Email` over a TCP stream to the specified address, or to the default
    /// [`MAIL_ADDRESS`] if `addr` is `None`.
    ///
    /// # Arguments
    ///
    /// * `addr` - An optional address in the format `host:port`. If `None`,
    ///   defaults to `MAIL_ADDRESS`.
    ///
    /// # Return
    ///
    /// Returns a [`UnifiedResult`] containing an [`OkWarning<()>`] on success,
    /// or an [`ErrorArrayItem`] if the connection fails, the email data is invalid,
    /// or the server indicates an error.
    ///
    /// # Errors
    ///
    /// - **`Errors::GeneralError`** if `subject` or `body` is empty.
    /// - **`Errors::Network`** for network-related issues.
    /// - **Other** potential errors based on serialization or internal server response codes.
    ///
    /// # Example
    /// ```rust
    /// # use tokio::runtime::Runtime;
    /// # use dusa_collection_utils::core::types::stringy::Stringy;
    /// # use artisan_middleware::notifications::Email;
    /// # let rt = Runtime::new().unwrap();
    /// # rt.block_on(async {
    /// let email = Email::new(Stringy::from("dwhitfield@artisanhosting.net"), Stringy::from("Test Subject"), Stringy::from("Test Body"));
    /// let result = email.send(None).await; // uses MAIL_ADDRESS by default
    /// match result.uf_unwrap() {
    ///     Ok(_) => println!("Email sent successfully!"),
    ///     Err(err) => eprintln!("Failed to send email: {}", err),
    /// }
    /// # });
    /// ```
    #[rustfmt::skip]
    pub async fn send(&self, addr: Option<&str>) -> UnifiedResult<OkWarning<()>> {
        // Validate email fields
        if !self.is_valid() {
            return UnifiedResult::new(Err(ErrorArrayItem::new(
                Errors::GeneralError,
                "Invalid Email Data".to_owned(),
            )));
        }

        // Attempt to connect to the specified address or default mail server
        let stream_result: Result<TcpStream, UnifiedResult<OkWarning<()>>> = match addr {
            Some(addr) => TcpStream::connect(addr).await,
            None => TcpStream::connect(MAIL_ADDRESS).await,
        }.map_err(|err| UnifiedResult::new(Err(ErrorArrayItem::from(err))));

        let mut stream = match stream_result {
            Ok(stream) => stream,
            Err(err) => return err,
        };

        log!{LogLevel::Trace, "Connected to: {:#?}", stream.peer_addr().unwrap()};

        // Serialize the email to JSON
        let data_result: Result<String, UnifiedResult<OkWarning<()>>> = self.to_json()
            .map_err(|err| UnifiedResult::new(Err(err)));

        let data: String = match data_result {
            Ok(data) => data,
            Err(err) => return err,
        };

        // Send the message and handle response
        match send_message::<TcpStream, String, ()>(
            &mut stream,
            Flags::OPTIMIZED,
            data,
            Proto::TCP,
            false
        ).await {
            Ok(response) => {
                match response {
                    Ok(response) => {
                        // We handle the server's status code as a potential "unexpected behavior" warning
                        let warning: WarningArrayItem =
                            WarningArrayItem::new_details(
                                Warnings::UnexpectedBehavior,
                                format!("{}", ProtocolStatus::from_bits_truncate(response.header.status))
                            );

                        UnifiedResult::new(Ok(OkWarning{
                            data: response.payload,
                            warning: WarningArray::new(vec![warning]),
                        }))
                    },
                    Err(error_code) => {
                        let error = ErrorArrayItem::new(Errors::Network, format!("{}", error_code));
                        UnifiedResult::new(Err(error))
                    },
                }
            },
            Err(err) => {
                UnifiedResult::new(Err(ErrorArrayItem::from(err)))
            },
        }
    }
}
