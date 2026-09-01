use colored::Colorize;
use dusa_collection_utils::{
    core::errors::{
        ErrorArrayItem, Errors,
    },
    core::logger::LogLevel,
    core::types::stringy::Stringy,
    log,
};
#[cfg(target_os = "linux")]
use serde::{Deserialize, Serialize};

#[cfg(target_os = "linux")]
use simple_comms::{
    network::send_receive::{establish_connection_initiator, send_message}, protocol::{flags::ConnectionParams, message::ConnectionCtx, proto::Proto},
};
use std::fmt;
use tokio::net::TcpStream;

/// Default mail server address. Used if no custom address is provided in [`Email::send`].
const MAIL_ADDRESS: [&str; 2] = ["172.237.134.238:1827", "172.234.222.191:1827"];

// it can't get more pinned than this
const MAIL_SERVER_PUB: [u8; 32] = 
[4, 174, 4, 246, 179, 162, 129, 67, 40, 38, 19, 206, 110, 212, 181, 156, 135, 163, 139, 211, 132, 147, 103, 80, 141, 7, 41, 46, 32, 80, 190, 84];


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
    pub async fn send(&self, addr: Option<&str>) -> Result<(), ErrorArrayItem> {
        // Validate email fields
        if !self.is_valid() {
            return Err(ErrorArrayItem::new(
                Errors::GeneralError,
                "Invalid Email Data".to_owned(),
            ));
        }

        let mailserver_addr: &str = if let Some(addr) = addr {
            addr
        } else {
            // TODO figure out how to randomise this
            MAIL_ADDRESS[0]
        };

        let mut stream: TcpStream = match TcpStream::connect(mailserver_addr).await {
            Ok(res) => {
                log!{LogLevel::Trace, "Connected to: {:#?}", res.peer_addr()?};
                Ok(res)
            },
            Err(e) => Err(ErrorArrayItem::new(Errors::ConnectionError, 
                format!("Failed to connect to mailserver: {}. {}", mailserver_addr, e))),
        }?;

        let mut conn: ConnectionCtx = establish_connection_initiator(&mut stream, &MAIL_SERVER_PUB, ConnectionParams::OPTIMIZED).await?;

        let email_data: String = self.to_json()?;

        let _: () = send_message(&mut stream, email_data, Proto::TCP, &mut conn).await?;

        Ok(())
    }
}
