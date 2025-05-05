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

use chrono::Utc;
use colored::Colorize;
use dusa_collection_utils::log;
use dusa_collection_utils::logger::LogLevel;
use dusa_collection_utils::types::pathtype::PathType;
use dusa_collection_utils::types::stringy::Stringy;
use dusa_collection_utils::{errors::ErrorArrayItem, types::rwarc::LockWithTimeout};
use serde::{Deserialize, Serialize};
use serde_json::Error;
use std::collections::HashSet;
use std::fs::create_dir_all;
use std::io::BufRead;
use std::time::Duration;
use std::{
    collections::HashMap,
    fmt,
    fs::{File, OpenOptions},
    io::{Read, Write},
};
use tokio::sync::broadcast;
use tokio::time::interval;

use crate::config_bundle::ApplicationConfig;
use crate::encryption::{simple_decrypt, simple_encrypt};
use crate::portal::{ManagerData, ProjectInfo};

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
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, PartialOrd, Ord, Eq, Hash)]
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

/// Records network TXranmitted and RXcieved
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct NetworkUsage {
    pub rx_bytes: u64,
    pub tx_bytes: u64,
}

impl NetworkUsage {
    pub fn set(&mut self, other: &Self) {
        // self.rx_bytes += other.rx_bytes;
        // self.tx_bytes += other.tx_bytes;
        self.rx_bytes = other.rx_bytes;
        self.tx_bytes = other.tx_bytes;
    }
}

/// Contains runtime metrics for an application, such as CPU and memory usage.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Metrics {
    /// CPU usage in percent.
    pub cpu_usage: f32,
    /// Memory usage in MB.
    pub memory_usage: f64,
    /// An optional field for additional metrics or notes.
    pub other: Option<NetworkUsage>,
}

impl Metrics {
    pub fn set(&mut self, other: &Self) {
        self.cpu_usage = other.cpu_usage;
        self.memory_usage = other.memory_usage;

        match (&mut self.other, &other.other) {
            (Some(existing), Some(new)) => existing.set(new),
            (None, Some(new)) => self.other = Some(new.clone()),
            _ => {} // Either both None, or only other is None — do nothing
        }
    }
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
                Some(info) => format!(", {}: {:?}", "Other".bold().yellow(), info),
                None => "".to_string(),
            }
        )
    }
}

/// Represents a real-time resource usage report from a running instance.
///
/// This is typically collected every few seconds to minutes and used to
/// update an in-memory accumulator which is later persisted for billing.
///
/// ### Example:
/// ```rust
/// use artisan_middleware::aggregator::LiveMetrics;
/// LiveMetrics {
///     runner_id: "abc123".into(),
///     instance_id: "xyz456".into(),
///     cpu_percent: 12.5,
///     memory_mb: 256.0,
///     rx_bytes: 15000,
///     tx_bytes: 5000,
/// }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiveMetrics {
    pub runner_id: Stringy,
    pub instance_id: Stringy,
    pub cpu_usage: f32,
    pub memory_mb: f64,
    pub rx_bytes: u64,
    pub tx_bytes: u64,
}

/// A single aggregated usage record.
///
/// This structure is persisted to disk at regular intervals, containing
/// the summarized data of all metrics observed in that interval.
///
/// ### Fields:
/// - `timestamp_epoch`: The UNIX timestamp at which the aggregation occurred.
/// - `total_*`: Cumulative totals over the aggregation period.
/// - `peak_*`: Highest observed value during the period.
/// - `sample_count`: Number of metric samples aggregated.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageRecord {
    pub timestamp_epoch: i64,
    pub runner_id: Stringy,
    pub instance_id: Stringy,
    pub total_cpu: f32,
    pub peak_cpu: f32,
    pub total_memory: f64,
    pub peak_memory: f64,
    pub total_rx: u64,
    pub total_tx: u64,
    pub sample_count: u64,
}

/// The result of a cost calculation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BillingCosts {
    pub cpu_cost: f64,
    pub ram_cost: f64,
    pub bandwidth_cost: f64,
    pub total_cost: f64,
    pub instances: u64
}

impl fmt::Display for BillingCosts {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "BillingCosts:\n\
             - CPU Cost: ${:.2}\n\
             - RAM Cost: ${:.2}\n\
             - Bandwidth Cost: ${:.2}\n\
             - Instance Cost: ${:.2}\n\
             - Total Cost: ${:.2}\n\
             - Instances: {}",
            self.cpu_cost,
            self.ram_cost,
            self.bandwidth_cost,
            (self.instances * 5),
            self.total_cost,
            self.instances
        )
    }
}

/// Accumulator that aggregates usage statistics over a time window.
///
/// This is stored in memory and updated every time a new `LiveMetrics` is received.
#[derive(Clone, Debug, Default)]
pub struct UsageAccumulator {
    pub total_cpu: f32,
    pub peak_cpu: f32,
    pub total_memory: f64,
    pub peak_memory: f64,
    pub total_rx: u64,
    pub total_tx: u64,
    pub last_rx: u64,
    pub last_tx: u64,
    pub sample_count: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BilledUsageSummary {
    pub runner_id: Stringy,
    pub instance_id: Stringy,
    pub total_cpu: f32,
    pub peak_cpu: f32,
    pub avg_memory: f64,
    pub peak_memory: f64,
    pub total_rx: u64,
    pub total_tx: u64,
    pub total_samples: u64,
    pub instances: u64,
}

pub fn load_usage_records_from_dir(dir: &PathType) -> Result<Vec<UsageRecord>, std::io::Error> {
    let mut all_records = Vec::new();

    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.extension().map(|ext| ext == "jsonl").unwrap_or(false) {
            let file = std::fs::File::open(&path)?;
            let reader = std::io::BufReader::new(file);

            for line_result in reader.lines() {
                if let Ok(line) = line_result {
                    if let Ok(record) = serde_json::from_str::<UsageRecord>(&line) {
                        all_records.push(record);
                    }
                }
            }
        }
    }

    Ok(all_records)
}

pub fn summarize_usage(records: &[UsageRecord]) -> Option<BilledUsageSummary> {
    if records.is_empty() {
        log!(LogLevel::Warn, "No records given");
        return None;
    }

    let mut total_cpu_points: f32 = 0.0;
    let mut peak_cpu: f32 = 0.0;
    let mut total_memory_sum: f64 = 0.0;
    let mut peak_memory: f64 = 0.0;
    let mut total_sample_count: u64 = 0;

    let mut min_rx: u64 = u64::MAX;
    let mut max_rx: u64 = 0;
    let mut min_tx: u64 = u64::MAX;
    let mut max_tx: u64 = 0;

    let runner_id = records[0].runner_id.clone();
    let instance_id = records[0].instance_id.clone();
    let mut instance_seen: HashSet<Stringy> = HashSet::new();

    for r in records {
        total_cpu_points += r.total_cpu;
        peak_cpu = peak_cpu.max(r.peak_cpu);

        total_memory_sum += r.total_memory;
        peak_memory = peak_memory.max(r.peak_memory);

        total_sample_count += r.sample_count;

        min_rx = min_rx.min(r.total_rx);
        max_rx = max_rx.max(r.total_rx);
        min_tx = min_tx.min(r.total_tx);
        max_tx = max_tx.max(r.total_tx);

        if !instance_seen.contains(&r.instance_id) {
            instance_seen.insert(r.instance_id.clone());
        }
    }

    let avg_memory = if total_sample_count > 0 {
        total_memory_sum / total_sample_count as f64
    } else {
        0.0
    };

    let total_rx = max_rx.saturating_sub(min_rx);
    let total_tx = max_tx.saturating_sub(min_tx);

    // Convert CPU% points → core-seconds → core-hours
    let total_core_hours = total_cpu_points;

    Some(BilledUsageSummary {
        runner_id,
        instance_id,
        total_cpu: total_core_hours, // << total_cpu is now "core-hours"
        peak_cpu,
        avg_memory,
        peak_memory,
        total_rx,
        total_tx,
        total_samples: total_sample_count,
        instances: instance_seen.len() as u64,
    })
}

/// Key for mapping usage data per instance.
/// Tuple of (runner_id, instance_id).
pub type InstanceKey = (Stringy, Stringy);

/// Thread-safe map of usage accumulators.
/// Wrapped in a LockWithTimeout for safe concurrent access.
pub type UsageMap = LockWithTimeout<HashMap<InstanceKey, UsageAccumulator>>;

/// Shared application context containing communication and tracking handles.
#[derive(Clone)]
pub struct AppContext {
    pub usage_map: UsageMap,
    pub metrics_tx: broadcast::Sender<LiveMetrics>,
    pub project_tx: broadcast::Sender<ProjectInfo>,
}

/// Updates the accumulator with new live metrics.
///
/// This function is intended to be called every time an instance
/// reports its current resource usage.
///
/// It handles delta calculations for network traffic and peak tracking
/// for CPU and memory.
///
/// Returns an error if the lock on the usage map could not be acquired.
pub async fn update_metrics(live: LiveMetrics, usage_map: &UsageMap) -> Result<(), ErrorArrayItem> {
    let mut map = usage_map.try_write().await?;
    let key = (live.runner_id.clone(), live.instance_id.clone());
    let entry = map.entry(key).or_default();

    // CPU & RAM
    entry.total_cpu += live.cpu_usage;
    entry.total_memory += live.memory_mb;
    entry.peak_cpu = entry.peak_cpu.max(live.cpu_usage);
    entry.peak_memory = entry.peak_memory.max(live.memory_mb);
    entry.sample_count += 1;

    // Network deltas
    let rx_delta = live.rx_bytes.saturating_sub(entry.last_rx);
    let tx_delta = live.tx_bytes.saturating_sub(entry.last_tx);

    if live.rx_bytes < entry.last_rx || live.tx_bytes < entry.last_tx {
        // Instance likely restarted
        entry.last_rx = 0;
        entry.last_tx = 0;
    }

    entry.total_rx += rx_delta;
    entry.total_tx += tx_delta;
    entry.last_rx = live.rx_bytes;
    entry.last_tx = live.tx_bytes;
    Ok(())
}

/// Spawns a background task that flushes all current usage accumulators to disk.
///
/// This function is meant to be called once at startup. It sets up a background task
/// that runs every 5 minutes, serializing the accumulated usage data into JSONL files.
/// Each day's data is written into a separate file (e.g., `usage-2025-04-16.jsonl`).
///
/// Logs an error if the usage map cannot be written at flush time.
pub async fn spawn_flush_task(usage_map: UsageMap, output_dir: PathType) {
    create_dir_all(&output_dir).unwrap();
    tokio::spawn(async move {
        // let mut tick = interval(Duration::from_secs(30)); // every 5 min
    let mut tick = interval(Duration::from_secs(300)); // every 5 min
        loop {
            tick.tick().await;

            let mut map = match usage_map.try_write().await {
                Ok(val) => val,
                Err(err) => {
                    log!(LogLevel::Error, "Failed to access the usage map: {}", err);
                    continue;
                }
            };

            let now = Utc::now();
            let epoch = now.timestamp();
            for ((runner_id, instance_id), acc) in map.drain() {
                let record = UsageRecord {
                    timestamp_epoch: epoch,
                    runner_id,
                    instance_id,
                    total_cpu: acc.total_cpu,
                    peak_cpu: acc.peak_cpu,
                    total_memory: acc.total_memory,
                    peak_memory: acc.peak_memory,
                    total_rx: acc.total_rx,
                    total_tx: acc.total_tx,
                    sample_count: acc.sample_count,
                };

                let filename = output_dir.join(format!("usage-{}.jsonl", now.format("%Y-%m-%d")));
                if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(filename) {
                    if let Ok(line) = serde_json::to_string(&record) {
                        if let Err(err) = writeln!(file, "{}", line) {
                            log!(
                                LogLevel::Error,
                                "Error flushing metrics data: {}",
                                err.to_string()
                            );
                            continue;
                        };
                    } else {
                        log!(LogLevel::Error, "Error serializing json data");
                        continue;
                    }
                } else {
                    log!(LogLevel::Error, "Error Opening File");
                    continue;
                }
            }
        }
    });
}

/// Immediately flushes all current usage accumulators to disk.
///
/// This function is useful during shutdown, before reloads, or when manually
/// triggering a flush for billing or debugging purposes.
pub async fn flush_metrics_to_disk(
    usage_map: &UsageMap,
    output_dir: &PathType,
) -> Result<(), ErrorArrayItem> {
    let mut map = usage_map.try_write().await?;

    let now = Utc::now();
    let epoch = now.timestamp();

    for ((runner_id, instance_id), acc) in map.drain() {
        let record = UsageRecord {
            timestamp_epoch: epoch,
            runner_id,
            instance_id,
            total_cpu: acc.total_cpu,
            peak_cpu: acc.peak_cpu,
            total_memory: acc.total_memory,
            peak_memory: acc.peak_memory,
            total_rx: acc.total_rx,
            total_tx: acc.total_tx,
            sample_count: acc.sample_count,
        };

        let filename = output_dir.join(format!("usage-{}.jsonl", now.format("%Y-%m-%d")));
        if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(filename) {
            if let Ok(line) = serde_json::to_string(&record) {
                writeln!(file, "{}", line).map_err(ErrorArrayItem::from)?;
            } else {
                log!(LogLevel::Error, "Error serializing json data");
                continue;
            }
        } else {
            log!(LogLevel::Error, "Error Opening File");
            continue;
        }
    }
    Ok(())
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
    let data: Stringy = simple_decrypt(encrypted_data.as_bytes()).map(
        |data| -> Result<Stringy, ErrorArrayItem> {
            let d = String::from_utf8(data)
                .map_err(ErrorArrayItem::from)
                .map(Stringy::from)?;
            Ok(d)
        },
    )??;

    let apps: Vec<AppStatus> = serde_json::from_str(&data)?;
    for app in apps.clone() {
        log!(LogLevel::Debug, "App Status from file: {} \n", app);
    }
    // Clearing screen can be done if needed:
    // print!("\x1B[2J\x1B[H");
    Ok(apps)
}

/// Sets up the metrics system and project queue for asynchronous processing.
///
/// This function spawns the background task that flushes the usage map to disk,
/// and it also spawns a task that listens to the metrics broadcast channel to
/// update in-memory usage data. The returned `project_rx` should be wired into
/// a dedicated task that handles insertion of project data via the Clipas system.
///
/// Returns an `AppContext` to be passed throughout the application.
pub async fn initialize_app_context(
    output_dir: PathType,
) -> (
    AppContext,
    broadcast::Receiver<ProjectInfo>,
    // tokio::sync::mpsc::UnboundedReceiver<ProjectInfo>,
) {
    let usage_map: UsageMap = LockWithTimeout::new(HashMap::new());
    let (metrics_tx, mut metrics_rx) = broadcast::channel::<LiveMetrics>(2048);
    let (project_tx, project_rx) = broadcast::channel::<ProjectInfo>(2048);

    let usage_map_clone = usage_map.clone();
    tokio::spawn(async move {
        while let Ok(metric) = metrics_rx.recv().await {
            if let Err(err) = update_metrics(metric, &usage_map_clone).await{
                log!(LogLevel::Warn, "Error monitoring usage data: {}", err.err_mesg)
            }
        }
    });

    spawn_flush_task(usage_map.clone(), output_dir).await;

    let context = AppContext {
        usage_map,
        metrics_tx,
        project_tx,
    };

    (context, project_rx)
}
