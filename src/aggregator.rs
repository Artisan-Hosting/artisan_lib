use colored::Colorize;
use dusa_collection_utils::{errors::ErrorArrayItem, stringy::Stringy};
use serde::{Deserialize, Serialize};
use std::{
    fmt, fs::{File, OpenOptions}, io::{Read, Write}
};

use crate::encryption::{decrypt_text, encrypt_text};

pub const AGGREGATOR_PATH: &str = "/tmp/aggregator.recs";
type ID = Stringy;

// Command Type Enum
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum CommandType {
    Start(ID),
    Stop(ID),
    Restart(ID),
    Status(ID),
    AllStatus,
    Custom(String),
}

impl fmt::Display for CommandType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CommandType::Start(id) => write!(f, "[{}] : {}", "Start".green(), id),
            CommandType::Stop(id) => write!(f, "[{}] : {}", "Stop".red(), id),
            CommandType::Restart(id) => write!(f, "[{}] : {}", "Restart".yellow(), id),
            CommandType::Status(id) => write!(f, "[{}] : {}", "Status".cyan(), id),
            CommandType::Custom(cmd) => write!(f, "{}: {}", "Custom".purple(), cmd),
            CommandType::AllStatus => write!(f, "{}", "All Status".purple()),
        }
    }
}


// Different status an application can be in
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, PartialOrd, Ord, Eq)]
pub enum Status {
    Starting,
    Running,
    Idle,
    Stopping,
    Stopped,
    Unknown,
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let status_str = match self {
            Status::Starting => "Starting".blue(),
            Status::Running => "Running".green(),
            Status::Idle => "Idle".yellow(),
            Status::Stopping => "Stopping".red(),
            Status::Stopped => "Stopped".bold(),
            Status::Unknown => "Unknown".purple(),
        };
        write!(f, "{}", status_str)
    }
}


// Command Struct
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Command {
    pub app_id: ID,
    pub command_type: CommandType,
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


// Metrics Struct
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Metrics {
    pub cpu_usage: f32,
    pub memory_usage: u64,
    // disk_usage: Option<u64>,
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


// App Status Struct
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AppStatus {
    pub app_id: ID,
    pub status: Status,
    pub uptime: Option<u64>,
    pub error: Option<String>,
    pub metrics: Option<Metrics>,
    pub timestamp: u64,
    pub expected_status: Status,
    pub system_application: bool,
}

impl fmt::Display for AppStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}: {}, {}: {}, {}: {} seconds, {}: {}, {}: {}, {}: {}",
            "App ID".bold().cyan(),
            self.app_id,
            "Status".bold().cyan(),
            self.status,
            "Uptime".bold().cyan(),
            self.uptime.unwrap_or(0),
            "Error".bold().cyan(),
            self.error.as_deref().unwrap_or("None"),
            "Metrics".bold().cyan(),
            self.metrics
                .as_ref()
                .map(|m| m.to_string())
                .unwrap_or_else(|| "None".to_string()),
            "Timestamp".bold().cyan(),
            self.timestamp
        )
    }
}

// Command Response Struct
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CommandResponse {
    pub app_id: ID,
    pub command_type: CommandType,
    pub success: bool,
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
            if self.success { "Yes".green() } else { "No".red() },
            "Message".bold().cyan(),
            self.message.as_deref().unwrap_or("None")
        )
    }
}

// Register App Struct
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RegisterApp {
    pub app_id: ID,
    pub app_name: String,
    pub expected_status: Status,
    pub system_application: bool,
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

// Deregister App Struct
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DeregisterApp {
    pub app_id: ID,
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

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct UpdateApp {
    pub app_id: ID,
    pub error: Option<String>,
    pub metrics: Option<Metrics>,
    pub status: Status,
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
            self.error.as_deref().unwrap_or("None"),
            "Timestamp".bold().cyan(),
            self.timestamp
        )
    }
}


#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum AppMessage {
    Register(RegisterApp),
    Deregister(DeregisterApp),
    Update(UpdateApp),
    Response(CommandResponse),
    Command(Command),
}

// Function to save registered apps to a JSON file
pub async fn save_registered_apps(apps: &[AppStatus]) -> Result<(), ErrorArrayItem> {
    let mut file: File = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(AGGREGATOR_PATH)
        .map_err(ErrorArrayItem::from)?;
    let data: String = serde_json::to_string_pretty(apps).map_err(ErrorArrayItem::from)?;
    let encrypted_data: Stringy = encrypt_text(data.into()).await?;
    match file.write_all(encrypted_data.as_bytes()) {
        Ok(_) => return Ok(()),
        Err(err) => return Err(ErrorArrayItem::from(err)),
    }
}

// Function to load registered apps from a JSON file
pub async fn load_registered_apps() -> Result<Vec<AppStatus>, ErrorArrayItem> {
    let mut file: File = File::open(AGGREGATOR_PATH)?;
    let mut encrypted_data: String = String::new();
    file.read_to_string(&mut encrypted_data)?;
    let data: Stringy = decrypt_text(Stringy::from(encrypted_data)).await?;
    let apps: Vec<AppStatus> = serde_json::from_str(&data)?;

    // let apps: Vec<RegisterApp> = serde_json::from_reader(reader)?;
    Ok(apps)
}
