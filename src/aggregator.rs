//! # Manager Data Module
//!
//! This module provides structures, enums, and functions for managing and 
//! communicating application states in the AIS Manager system. It includes 
//! mechanisms for encrypting/decrypting data, sending/receiving messages 
//! through Unix sockets, and storing application status information locally.
//!
//! ## Overview
//! - **CommandType**: Represents different commands (Start, Stop, Restart, etc.).
//! - **Status**: Represents the lifecycle states of an application (Starting, Running, etc.).
//! - **Command**: Used for issuing a command to an application.
//! - **Metrics**: Holds runtime metrics (CPU, memory usage, etc.).
//! - **AppStatus**: A snapshot of an application's current state.
//! - **CommandResponse**: A response from the system after processing a command.
//! - **RegisterApp / DeregisterApp / UpdateApp**: Messages used for registering, 
//!   deregistering, or updating an application's status.
//! - **AppMessage**: Aggregates all message variants (Register, Deregister, Update, etc.).
//! - **save_registered_apps / load_registered_apps**: Handle storing and loading of 
//!   `AppStatus` data locally.
//! - **register_app**: Registers an application with a remote aggregator if configured.

use colored::Colorize;
use dusa_collection_utils::log;
use dusa_collection_utils::logger::LogLevel;
use dusa_collection_utils::types::stringy::Stringy;
use dusa_collection_utils::errors::ErrorArrayItem;
use serde::{Deserialize, Serialize};
use serde_json::Error;
use std::{
    fmt,
    fs::{File, OpenOptions},
    io::{Read, Write},
};

use crate::config_bundle::ApplicationConfig;
use crate::encryption::{simple_decrypt, simple_encrypt};
use crate::portal::ManagerData;

/// Path where the aggregator stores AIS Manager data.
pub const AGGREGATOR_PATH: &str = "/tmp/.ais_manager_data";

/// A convenience type alias for string-based identifiers in this module.
type ID = Stringy;

//
// Enums
//

/// Represents different commands that can be sent to an application.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum CommandType {
    /// Instructs the application to start.
    Start,
    /// Instructs the application to stop.
    Stop,
    /// Instructs the application to restart.
    Restart,
    /// Requests the application’s current status.
    Status,
    /// Requests the status of all registered applications.
    AllStatus,
    /// Requests manager-level information.
    Info,
    /// Sends a custom command with a user-defined string.
    Custom(String),
}

impl fmt::Display for CommandType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CommandType::Start => write!(f, "{}", "Start".green()),
            CommandType::Stop => write!(f, "{}", "Stop".red()),
            CommandType::Restart => write!(f, "{}", "Restart".yellow()),
            CommandType::Status => write!(f, "{}", "Status".cyan()),
            CommandType::AllStatus => write!(f, "{}", "All Info".cyan()),
            CommandType::Info => write!(f, "{}", "Manager Info".blue()),
            CommandType::Custom(cmd) => write!(f, "{}: {}", "Custom".purple(), cmd),
        }
    }
}

/// Represents different lifecycle states an application can be in.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, PartialOrd, Ord, Eq)]
pub enum Status {
    /// The application is in the process of starting.
    Starting,
    /// The application is currently running.
    Running,
    /// The application is running but idle.
    Idle,
    /// The application is in the process of stopping.
    Stopping,
    /// The application has stopped.
    Stopped,
    /// The application’s status cannot be determined.
    Unknown,
    /// The application is running with warnings.
    Warning,
    /// The application is in the process of building.
    Building,
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let status_str = match self {
            Status::Starting => "Starting".bright_green(),
            Status::Running => "Running".green().bold(),
            Status::Idle => "Idle".yellow(),
            Status::Stopping => "Stopping".bright_red(),
            Status::Stopped => "Stopped".red().bold(),
            Status::Unknown => "Unknown".bright_cyan().bold(),
            Status::Warning => "Warning".bright_yellow(),
            Status::Building => "Building".bright_blue(),
        };
        write!(f, "{}", status_str)
    }
}

//
// Structs
//

/// Represents a command that can be issued to an application, including the 
/// application identifier, command type, and timestamp.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Command {
    /// The unique identifier of the target application.
    pub app_id: ID,
    /// The type of command (start, stop, custom, etc.).
    pub command_type: CommandType,
    /// A Unix timestamp marking when the command was created.
    pub timestamp: u64,
}

impl fmt::Display for Command {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}: {}, {}: {}, {}: {}",
            "App ID".bold().cyan(),
            self.app_id,
            "Command Type".bold().cyan(),
            self.command_type,
            "Timestamp".bold().cyan(),
            self.timestamp
        )
    }
}

/// Contains runtime metrics for an application, such as CPU and memory usage.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Metrics {
    /// CPU usage in percent.
    pub cpu_usage: f32,
    /// Memory usage in MB.
    pub memory_usage: f32,
    /// An optional field for additional metrics or notes.
    pub other: Option<String>,
}

impl fmt::Display for Metrics {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}: {:.2}%, {}: {} MB{}",
            "CPU Usage".bold().yellow(),
            self.cpu_usage,
            "Memory Usage".bold().yellow(),
            self.memory_usage,
            match &self.other {
                Some(info) => format!(", {}: {}", "Other".bold().yellow(), info),
                None => "".to_string(),
            }
        )
    }
}

/// Represents a snapshot of an application’s state, including status, version, metrics, etc.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AppStatus {
    /// Unique identifier for the application.
    pub app_id: ID,
    /// Additional identifier (often a Git commit SHA or branch).
    pub git_id: ID,
    /// The application state and all configuration data associated, refer to [`ApplicationConfig`]
    pub app_data: ApplicationConfig,
    /// The reported uptime of the instance
    pub uptime: Option<u64>,
    /// A list of errors encountered by the application.
    pub metrics: Option<Metrics>,
    /// The Unix timestamp when this status was recorded.
    pub timestamp: u64,
    /// The expected status set for this application (Running, Stopped, etc.).
    pub expected_status: Status,
}

impl AppStatus {
    /// Converts the `AppStatus` to a JSON string. Returns `None` if serialization fails.
    pub fn to_json(&self) -> Option<String> {
        match serde_json::to_string(self) {
            Ok(data) => Some(data),
            Err(e) => {
                log!(LogLevel::Error, "{}", e);
                None
            }
        }
    }

    /// Creates an `AppStatus` instance from a JSON string. Returns a `Result` with either
    /// `AppStatus` or a `serde_json::Error`.
    pub fn from_json(json_str: &str) -> Result<Self, Error> {
        serde_json::from_str(json_str)
    }

    /// Unsafely converts the `AppStatus` to a `String` (via JSON).
    /// Uses `unwrap_unchecked`, so be certain the data is valid.
    pub unsafe fn to_string(&self) -> String {
        serde_json::to_string(self).unwrap_unchecked()
    }

    /// Returns the `app_id` (primary identifier).
    pub fn get_id(&self) -> Stringy {
        self.app_id.clone()
    }
}

impl fmt::Display for AppStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let system = match self.app_data.is_system_application() {
            true => "YES".bold().green(),
            false => "NO".bold().red(),
        };

        write!(
            f,
            "{}: {}, {}: {} seconds, {}: {}, {}: {}, {}: {}, {} {}",
            "App ID".bold().cyan(),
            self.app_id,
            "Uptime".bold().cyan(),
            self.uptime.unwrap_or(0),
            "Metrics".bold().cyan(),
            self.metrics
                .as_ref()
                .map(|m| m.to_string())
                .unwrap_or_else(|| "None".to_string()),
            "State Data".bold().cyan(),
            format!("{}\n{}", self.app_data.state, self.app_data.config),
            "Timestamp".bold().cyan(),
            self.timestamp,
            "System App".bold().cyan(),
            system,
        )
    }
}

/// A response structure returned after processing a command.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CommandResponse {
    /// The unique identifier of the target application.
    pub app_id: ID,
    /// The type of command that was processed.
    pub command_type: CommandType,
    /// Indicates whether the command was successful.
    pub success: bool,
    /// An optional message that can contain error details or additional info.
    pub message: Option<String>,
}

impl fmt::Display for CommandResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}: {}, {}: {}, {}: {}, {}: {}",
            "App ID".bold().cyan(),
            self.app_id,
            "Command Type".bold().cyan(),
            self.command_type,
            "Success".bold().cyan(),
            if self.success {
                "Yes".green()
            } else {
                "No".red()
            },
            "Message".bold().cyan(),
            self.message.as_deref().unwrap_or("None")
        )
    }
}

/// Used to register a new application with the system (local or aggregator).
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RegisterApp {
    /// A unique identifier for the application to be registered.
    pub app_id: ID,
    /// Human-readable name of the application.
    pub app_name: String,
    /// The status that we expect the application to have once registered.
    pub expected_status: Status,
    /// Indicates if this application is part of system processes.
    pub system_application: bool,
    /// The timestamp when registration was requested.
    pub registration_timestamp: u64,
}

impl fmt::Display for RegisterApp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}: {}, {}: {}, {}: {}, {}: {}",
            "App ID".bold().cyan(),
            self.app_id,
            "App Name".bold().cyan(),
            self.app_name,
            "Expected Status".bold().cyan(),
            self.expected_status,
            "Registration Timestamp".bold().cyan(),
            self.registration_timestamp
        )
    }
}

/// Used to deregister an application from the system (local or aggregator).
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DeregisterApp {
    /// Unique identifier of the application being deregistered.
    pub app_id: ID,
    /// Timestamp when deregistration was requested.
    pub deregistration_timestamp: u64,
}

impl fmt::Display for DeregisterApp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}: {}, {}: {}",
            "App ID".bold().cyan(),
            self.app_id,
            "Deregistration Timestamp".bold().cyan(),
            self.deregistration_timestamp
        )
    }
}

/// A message for updating an application's status, errors, and metrics.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct UpdateApp {
    /// Identifier of the application to be updated.
    pub app_id: ID,
    /// A list of errors encountered by the application, if any.
    pub error: Option<Vec<ErrorArrayItem>>,
    /// Updated metrics (CPU, memory, etc.).
    pub metrics: Option<Metrics>,
    /// New status (running, stopped, warning, etc.).
    pub status: Status,
    /// The timestamp when the update was performed.
    pub timestamp: u64,
}

impl fmt::Display for UpdateApp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}: {}, {}: {}, {}: {}, {}: {}",
            "App ID".bold().cyan(),
            self.app_id,
            "Status".bold().cyan(),
            self.status,
            "Error".bold().cyan(),
            {
                let mut data = String::new();

                match self.error.clone() {
                    Some(err) => {
                        let errors = err.iter();
                        for e in errors {
                            data.push_str(&e.to_string());
                        }
                        data
                    }
                    None => "None".to_owned(),
                }
            },
            "Timestamp".bold().cyan(),
            self.timestamp
        )
    }
}

/// Encapsulates different message variants related to application registration, 
/// updates, and aggregator communication.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum AppMessage {
    /// Register a new application.
    Register(RegisterApp),
    /// Deregister an existing application.
    Deregister(DeregisterApp),
    /// Update an existing application’s status, metrics, etc.
    Update(UpdateApp),
    /// A response to a previously-issued command.
    Response(CommandResponse),
    /// A command targeted at an application.
    Command(Command),
    /// Manager-level information data.
    ManagerInfo(ManagerData),
}

impl fmt::Display for AppMessage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppMessage::Register(register) => write!(f, "Register: {}", register),
            AppMessage::Deregister(deregister) => write!(f, "Deregister: {}", deregister),
            AppMessage::Update(update) => write!(f, "Update: {}", update),
            AppMessage::Response(response) => write!(f, "Response: {}", response),
            AppMessage::Command(command) => write!(f, "Command: {}", command),
            AppMessage::ManagerInfo(manager_data) => write!(f, "Manager Data: {}", manager_data),
        }
    }
}

//
// Functions
//

/// Saves a slice of `AppStatus` objects to a JSON file at [`AGGREGATOR_PATH`], 
/// encrypting the data before writing.
///
/// # Arguments
///
/// * `apps` - A reference to a slice of `AppStatus` structs to be saved.
///
/// # Returns
///
/// * `Ok(())` on success.
/// * `Err(ErrorArrayItem)` containing file I/O or serialization errors on failure.
pub async fn save_registered_apps(apps: &[AppStatus]) -> Result<(), ErrorArrayItem> {
    let mut file: File = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(AGGREGATOR_PATH)
        .map_err(ErrorArrayItem::from)?;
    let data: String = serde_json::to_string_pretty(apps).map_err(ErrorArrayItem::from)?;
    // let encrypted_data: Stringy = encrypt_text(data.into()).await?;
    let encrypted_data: Stringy = simple_encrypt(data.as_bytes())?;
    match file.write_all(encrypted_data.as_bytes()) {
        Ok(_) => Ok(()),
        Err(err) => Err(ErrorArrayItem::from(err)),
    }
}

/// Loads a list of `AppStatus` objects from the JSON file at [`AGGREGATOR_PATH`], 
/// decrypting the data after reading.
///
/// # Returns
///
/// * `Ok(Vec<AppStatus>)` on success.
/// * `Err(ErrorArrayItem)` if file I/O, decryption, or JSON deserialization fails.
pub async fn load_registered_apps() -> Result<Vec<AppStatus>, ErrorArrayItem> {
    log!(LogLevel::Info, "Loading saved app status array");
    let mut file: File = File::open(AGGREGATOR_PATH)?;
    let mut encrypted_data: String = String::new();
    file.read_to_string(&mut encrypted_data)?;

    // let data: Stringy = decrypt_text(Stringy::from(encrypted_data)).await?;
    let data: Stringy = simple_decrypt(encrypted_data.as_bytes())
        .map(|data| -> Result<Stringy, ErrorArrayItem> {
            let d = String::from_utf8(data).map_err(ErrorArrayItem::from).map(Stringy::from)?;
            Ok(d)
        })??;

    let apps: Vec<AppStatus> = serde_json::from_str(&data)?;
    for app in apps.clone() {
        log!(LogLevel::Debug, "App Status from file: {} \n", app);
    }
    // Clearing screen can be done if needed:
    // print!("\x1B[2J\x1B[H");
    Ok(apps)
}