use serde::{Deserialize, Serialize};
use std::fs::{File, OpenOptions};
use std::io::{self, BufReader, BufWriter};
use std::time::{SystemTime, UNIX_EPOCH};

pub const AGGREGATOR_PATH: &str = "/tmp/aggregator.recs";

// Command Type Enum
#[derive(Serialize, Deserialize, Debug)]
pub enum CommandType {
    Start,
    Stop,
    Restart,
    Status,
    Custom(String),
}

// Different status an application can be in
#[derive(Serialize, Deserialize, Debug)]
pub enum Status {
    Starting,
    Running,
    Idle,
    Stopping,
    Stopped,
    Unknown,
}

// Command Struct
#[derive(Serialize, Deserialize, Debug)]
struct Command {
    app_id: String,
    command_type: CommandType,
    timestamp: u64,
}

// Metrics Struct
#[derive(Serialize, Deserialize, Debug)]
struct Metrics {
    cpu_usage: f32,
    memory_usage: u64,
    // disk_usage: Option<u64>,
    other: Option<String>,
}

// App Status Struct
#[derive(Serialize, Deserialize, Debug)]
struct AppStatus {
    app_id: String,
    status: Status,
    uptime: Option<u64>,
    error: Option<String>,
    metrics: Option<Metrics>,
}

// Command Response Struct
#[derive(Serialize, Deserialize, Debug)]
struct CommandResponse {
    app_id: String,
    command_type: CommandType,
    success: bool,
    message: Option<String>,
}

// Register App Struct
#[derive(Serialize, Deserialize, Debug)]
struct RegisterApp {
    app_id: String,
    app_name: String,
    expected_status: Status,
    system_application: bool,
    registration_timestamp: u64,
}

// Deregister App Struct
#[derive(Serialize, Deserialize, Debug)]
struct DeregisterApp {
    app_id: String,
    deregistration_timestamp: u64,
}

// Function to save registered apps to a JSON file
pub fn save_registered_apps(apps: &[RegisterApp]) -> io::Result<()> {
    let file: File = OpenOptions::new().write(true).create(true).truncate(true).open(AGGREGATOR_PATH)?;
    let data: String = serde_json::to_string_pretty(apps)?;



    let writer = BufWriter::new(file);
    serde_json::to_writer_pretty(writer, apps)?;
    Ok(())
}

// Function to load registered apps from a JSON file
pub fn load_registered_apps() -> io::Result<Vec<RegisterApp>> {
    let file = File::open(AGGREGATOR_PATH)?;
    let reader = BufReader::new(file);
    let apps = serde_json::from_reader(reader)?;
    Ok(apps)
}

// fn main() -> io::Result<()> {
//     // Current Unix timestamp
//     let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();

//     // Example usage
//     let registered_apps = vec![
//         RegisterApp {
//             app_id: "app123".to_string(),
//             app_name: "Test Application".to_string(),
//             expected_status: "Running".to_string(),
//             registration_timestamp: timestamp,
//         },
//     ];

//     // Save example data to file
//     save_registered_apps(&registered_apps, "registered_apps.json")?;

//     // Load the data back from the file to demonstrate persistence
//     let loaded_apps = load_registered_apps("registered_apps.json")?;
//     println!("{:?}", loaded_apps); // Display loaded applications

//     Ok(())
// }
