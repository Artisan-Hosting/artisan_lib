use colored::Colorize;
use core::fmt;
use dusa_collection_utils::{
    functions::{create_hash, truncate}, log, logger::LogLevel, types::stringy::Stringy, version::SoftwareVersion
};
use serde::{Deserialize, Serialize};
use lz4::block::compress;

use crate::aggregator::Metrics;
#[allow(unused_imports)] // for documents
use crate::{
    aggregator::{AppStatus, Status},
    config::AppConfig,
    enviornment::definitions::{Enviornment, Enviornment_V1, Enviornment_V2},
    git_actions::GitCredentials,
    identity::Identifier,
};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum PortalMessage {
    Discover,
    IdRequest,
    IdResponse(Option<Identifier>),
    RegisterRequest(ManagerData),
    RegisterResponse(bool),
    Error(String),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ProjectInfo {
    pub project_id: Stringy,
    pub identity: Identifier,
    pub project_data: AppStatus,
}

impl ProjectInfo {
    #[allow(deprecated)]
    pub fn get_id(&self) -> Stringy {
        let data = format!("{}-{}", self.identity.id, self.project_data.get_id());
        let bytes = data.into_bytes();
        let compressed = match compress(&bytes, None, false) {
            Ok(data) => data,
            Err(err) => {
                log!(LogLevel::Warn, "Error compressing id: {}", err.to_string());
                let fallback = b"none";
                fallback.to_vec()
            },
        };
        let encoded = base64::encode(&compressed);
        let result = truncate(&encoded, 100).to_owned();
        return result;
    }
}
// =============================================================================
// API RESPONSES SECTION
// =============================================================================

/// A generic API response wrapper for all API endpoints.
///
/// This struct is designed to standardize the format of successful and error responses.
/// - The `status` field indicates whether the request succeeded (`"success"`) or encountered
///   an error (`"error"`).
/// - The `data` field, if present, contains the primary payload (e.g., node details,
///   a list of runners, etc.).
/// - The `errors` field is an array of [`ErrorInfo`] objects that provide more context when
///   `status` is `"error"`.
#[derive(Serialize, Deserialize, Debug)]
pub struct ApiResponse<T> {
    /// Represents whether the request was successful or encountered an error.
    /// Typical values: `"success"` or `"error"`.
    pub status: String,

    /// The main payload returned by the endpoint. This can be a single entity,
    /// a collection of items, or even a simple acknowledgment object. If the
    /// request fails or there is no payload, this may be `None`.
    pub data: Option<T>,

    /// A list of error objects (`[]` on success), each containing a code, message,
    /// and optional details to help diagnose any issues that arose during the request.
    pub errors: Vec<ErrorInfo>,
}

/// Enumerates common error codes for use within [`ApiResponse`] when `status` is `"error"`.
///
/// These codes can be matched in client logic or user interfaces to provide more specific
/// handling or localized error messages.
#[derive(Serialize, Deserialize, Debug)]
pub enum ErrorCode {
    /// Indicates that the requested node resource was not found on the server.
    NodeNotFound,

    /// Indicates that the requested runner resource was not found on the server.
    RunnerNotFound,

    /// Occurs when authentication credentials are invalid (e.g., wrong token or password).
    InvalidCredentials,

    /// Occurs when the user or client is not authorized to perform the requested action.
    NotAuthorized,

    /// Represents a generic internal server error when a more specific code is not available.
    InternalError,

    /// Indicates that the request timed out before completing.
    TimedOut,

    /// A catch-all for unexpected or miscellaneous errors.
    Whoops,
}

/// Encapsulates details about an individual error within an [`ApiResponse`].
///
/// This includes a machine-readable code (`code`), a human-readable description (`message`),
/// and an optional `details` object for storing extra contextual information.
#[derive(Serialize, Deserialize, Debug)]
pub struct ErrorInfo {
    /// A machine-readable error code, typically referencing an entry in [`ErrorCode`].
    pub code: ErrorCode,

    /// A human-readable explanation of what went wrong.
    pub message: String,

    /// An optional JSON value that can store arbitrary key-value pairs or nested
    /// structures to further describe the error.
    #[serde(default)]
    pub details: serde_json::Value,
}

// =============================================================================
// Node Data Structures
// =============================================================================

/// Represents high-level information about a single Node within the system.
///
/// This struct is often used when listing or retrieving basic node details. It includes
/// identifying information (`identity`, `hostname`), status, networking details, and
/// timestamps such as creation and last update times.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct NodeInfo {
    /// A custom identifier object containing the node’s unique ID or name.
    pub identity: Identifier,

    /// The host name of the machine or VM.
    pub hostname: Stringy,

    /// The node’s current health or operational state (e.g., `"healthy"`, `"degraded"`).
    pub status: Status,

    /// The IP address associated with this node.
    /// Uses a Rust `IpAddr` type to handle both IPv4 and IPv6.
    pub ip_address: std::net::IpAddr,

    /// A list of runner identifiers hosted on this node.
    pub runners: Vec<Stringy>,

    /// The Unix epoch timestamp (in seconds) when this node was first registered.
    pub created_at: Stringy,

    /// The Unix epoch timestamp (in seconds) of the most recent update to this node’s record.
    pub last_updated: Stringy,
}

impl NodeInfo {
    /// Generates a shortened hash-based string derived from the node’s IP address
    /// and internal identity.
    ///
    /// This can be used for diagnostic logging, generating labels, or other reference points.
    /// The string is truncated to 20 characters for brevity.
    pub fn get_stringy(&self) -> Stringy {
        let data = format!("{}_-_{}", self.ip_address, self.identity.id);
        let hash = create_hash(data);
        truncate(&*hash, 20).to_owned()
    }
}

/// Provides detailed information about a node, typically returned by the "Get Node Details" endpoint.
///
/// Includes the node’s identity, status, the runners it hosts, and additional manager-side data
/// about the system version, uptime, etc.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct NodeDetails {
    /// A custom identifier object containing the node’s unique ID or name.
    pub identity: Identifier,

    /// The node’s current health or operational state (e.g., `"healthy"`, `"degraded"`).
    pub status: Status,

    /// A list of identifiers for the runners hosted on this node.
    pub runners: Vec<Stringy>,

    /// The Unix epoch timestamp (in seconds) when this node was first registered.
    pub created_at: Stringy,

    /// The Unix epoch timestamp (in seconds) of the most recent update to this node’s record.
    pub last_updated: Stringy,

    /// Contains additional information from the manager system overseeing this node,
    /// including version details, system metrics, and environment configuration.
    pub manager_data: ManagerData,
}

impl NodeDetails {
    /// Generates a shortened hash-based string derived from the manager’s address
    /// and the node’s internal identity.
    ///
    /// This can be used to produce a unique but compact identifier for logging
    /// or referencing node details. The string is truncated to 20 characters.
    pub fn get_stringy(&self) -> Stringy {
        let data = format!("{}_-_{}", self.manager_data.address, self.identity.id);
        let hash = create_hash(data);
        truncate(&*hash, 20).to_owned()
    }
}

/// Holds manager-related data for a particular node, including identity, versions, and metrics.
///
/// This structure is often embedded within a [`NodeDetails`] object to provide extra context
/// about the node’s environment and software state.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ManagerData {
    /// The manager’s own identity object.
    /// Useful when multiple managers oversee different sets of nodes.
    pub identity: Identifier,

    /// The software version of the manager controlling this node.
    pub version: SoftwareVersion,

    /// Credentials or configuration details for interacting with Git repositories.
    pub git_config: GitCredentials,

    /// Host name associated with the manager process or server.
    pub hostname: Stringy,

    /// IP address where the manager process or server is running.
    pub address: std::net::IpAddr,

    /// Number of system-level applications deployed or managed on this node.
    pub system_apps: u32,

    /// Number of client-level applications deployed on this node.
    pub client_apps: u32,

    /// A count of current warnings or alerts for this node, as determined by the manager.
    pub warning: u32,

    /// The total amount of time (in seconds) that this node has been under management.
    pub uptime: u64,
}

impl ManagerData {
    /// Generates a shortened hash-based string derived from the manager’s IP address
    /// and internal identity.
    ///
    /// Can be used for labeling or logging purposes. The resulting string is truncated
    /// to 20 characters for readability.
    pub fn get_stringy(&self) -> Stringy {
        let data = format!("{}_-_{}", self.address, self.identity.id);
        let hash = create_hash(data);
        truncate(&*hash, 20).to_owned()
    }
}

/// Represents the result of reloading a node within the system.
///
/// Often returned by an endpoint like "Reload a Node" to confirm that the node
/// was successfully refreshed or updated in the management layer.
#[derive(Serialize, Deserialize, Debug)]
pub struct NodeReloadResult {
    /// The unique identifier of the node that was reloaded.
    pub id: String,

    /// Indicates whether the reload operation completed successfully.
    pub reloaded: bool,
}

// =============================================================================
// Runner Data Structures
// =============================================================================

/// A minimal data structure containing summary information about a runner.
///
/// This struct can be used for listing runners for quickly describing
/// them in aggregate form. It includes key properties such as `name`, `status`,
/// version details, and an optional `uptime`. This is a combination of every instance
/// of a given runner across all nodes, for more specific info about instance of a runner
/// use ['RunnerDetails']
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RunnerSummary {
    /// A short name or identifier for the runner.
    pub name: Stringy,

    /// The current status of the runner (e.g., `"running"`, `"stopped"`, etc.).
    pub status: Status,

    /// The runner’s software version details (e.g., a semantic version number
    /// and a code like "Beta" or "Production").
    pub version: SoftwareVersion,

    /// A list of node IDs (as `u64`) indicating where this runner is deployed.
    /// Some runners may be replicated or load-balanced across multiple nodes.
    pub nodes: Vec<u64>,

    /// The total number of seconds this runner has been active, if known.
    /// When the runner is deployed on multiple nodes, this may be rounded
    /// to the closest hundred based on the longest-running node.
    pub uptime: Option<u64>,

    /// A collection of all metrics data across all available instances
    pub metrics: Option<Metrics>,
}

/// Provides detailed information about a single runner within the system.
///
/// Typically returned by the "Get Runner Details" endpoint, this struct contains
/// both fixed fields (like `id`, `status`, `version`) and flexible fields (`specific_config`)
/// that allow for runner-specific customization or dynamic configuration.
#[derive(Serialize, Deserialize, Debug)]
pub struct RunnerDetails {
    /// A unique identifier for the runner (e.g., "runner123").
    ///
    /// `Stringy` is a custom type that may encapsulate additional validation or formatting
    /// rules beyond a basic string.
    pub id: Stringy,

    /// Represents the current operating state of the runner (e.g., `"running"`, `"stopped"`, etc.).
    ///
    /// `Status` is a custom enumeration or type that captures all valid states a runner can have.
    pub status: Status,

    /// Indicates the software version in use by this runner.
    ///
    /// `SoftwareVersion` may include fields such as the main version number, a release code,
    /// and possibly other metadata about the software being run.
    pub version: SoftwareVersion,

    /// Stores high-level, Artisan-specific configuration for this runner.
    ///
    /// `AppConfig` often encompasses standardized settings across multiple services,
    /// ensuring consistency in how runners are deployed and managed.
    pub artisan_config: AppConfig,

    /// A optional JSON object containing runner-specific configuration options.
    ///
    /// Because each runner might require unique settings, `specific_config` is kept as raw
    /// JSON rather than a strongly-typed Rust struct. You can parse or transform it after
    /// deserialization if your application needs more granular control over these settings.
    pub specific_config: Option<serde_json::Value>,

    /// Holds environment-specific configuration for this runner, if available.
    ///
    /// [`Enviornment`] is an enum that can represent multiple versions of environment data
    /// e.g., [`Enviornment_V1`] or [`Enviornment_V2`]. If absent (`None`), the runner may either not rely on
    /// environment settings or be using defaults.
    pub enviornment: Option<Enviornment>,

    /// Optional health metrics and status for the runner, such as uptime or last check time.
    ///
    /// If `None`, health information may not be collected or may not be relevant for this runner.
    /// The `#[serde(default)]` annotation makes sure missing fields in JSON won't cause errors.
    #[serde(default)]
    pub health: Option<RunnerHealth>,

    /// A collection of recent log entries or references to logs for this runner, if available.
    ///
    /// Typically used to quickly inspect the runner's recent activity without making additional
    /// log-fetching requests. If `None`, logs may not be tracked or have not been retrieved yet.
    /// The `#[serde(default)]` annotation ensures missing fields in JSON are treated as `None`.
    #[serde(default)]
    pub logs: Option<RunnerLogs>,
}

/// Stores basic health metrics and status for a runner (e.g., uptime or last check time).
///
/// This structure can be omitted if health metrics are unavailable or not yet implemented.
#[derive(Serialize, Deserialize, Debug)]
pub struct RunnerHealth {
    /// The total number of seconds since the runner was started.
    pub uptime: u64,

    /// The timestamp (formatted as a string) when the runner last passed a health check.
    pub last_check: u64,

    /// Cpu usage
    pub cpu_usage: Stringy,

    /// Used ram
    pub ram_usage: Stringy,

    /// sent bytes
    pub tx_bytes: u64,

    /// recv bytes
    pub rx_bytes: u64,
}

/// Collects recent log entries for a runner, along with optional metadata about log storage.
///
/// This can include an array of `[LogEntry]` objects and potentially a `log_endpoint` for
/// retrieving more detailed or historical logs.
#[derive(Serialize, Deserialize, Debug)]
pub struct RunnerLogs {
    /// A list of recent log messages, including timestamps and textual data.
    pub recent: Vec<LogEntry>,
    // TODO Implement a log endpoint system for each instance, oneday
    // pub log_endpoint: String,
}

// TODO Document
#[derive(Debug, Serialize, Deserialize, Clone, Hash, PartialEq, PartialOrd, Eq, Ord)]
pub struct NetworkStats {
    pub rx_bytes: u64,
    pub tx_bytes: u64,
    pub note: Option<String>, // Future expansion
}

// =============================================================================
// Command Structures (Send / Check Status of Commands)
// =============================================================================

/// Represents a request body for issuing a command to a runner.
///
/// Common commands include:
/// - `"start-runner"`  
/// - `"stop-runner"`  
/// - `"restart-runner"`  
///
/// The `params` field enables additional parameters to be passed for more sophisticated commands.
#[derive(Serialize, Deserialize, Debug)]
pub struct CommandRequest {
    /// The command to be executed, as a string (e.g., "start-runner").
    pub command: String,

    /// A JSON object holding any additional parameters needed by the command.
    /// May be empty (`{}`) if no extra data is required.
    #[serde(default)]
    pub params: serde_json::Value,
}

/// The server’s response after accepting a command for a runner.
///
/// Often returned immediately after posting a command to the server. Includes details such as
/// the runner ID, the command issued, and an initial status (e.g., `"in-progress"`).
#[derive(Serialize, Deserialize, Debug)]
pub struct CommandResponse {
    /// The ID of the runner this command was sent to.
    #[serde(rename = "runnerId")]
    pub runner_id: String,

    /// A unique identifier for the command, useful for checking status later.
    #[serde(rename = "commandId")]
    pub command_id: String,

    /// The command name itself, echoing what was sent in the request.
    pub command: String,

    /// Any parameters that were sent along with the command, echoed back for clarity.
    #[serde(default)]
    pub params: serde_json::Value,

    /// The time the command was placed into the queue (in ISO 8601 format or similar).
    pub queued_at: u64,

    /// The current status of this command, such as `"in-progress"`, `"success"`, or `"error"`.
    pub status: Status,
}

/// Provides extended information about the status of a previously invoked command,
/// including start/finish times and any output messages.
#[derive(Serialize, Deserialize, Debug)]
pub struct CommandStatusResponse {
    /// The ID of the runner this command was sent to.
    #[serde(rename = "runnerId")]
    pub runner_id: String,

    /// A unique identifier for the command, matching the value in [`CommandResponse`].
    #[serde(rename = "commandId")]
    pub command_id: String,

    /// The command name itself, echoing what was sent in the request.
    pub command: String,

    /// When the command started executing, if known.
    #[serde(default)]
    pub started_at: Option<String>,

    /// When the command finished executing, if known.
    #[serde(default)]
    pub finished_at: Option<String>,

    /// The current status of this command, such as `"in-progress"`, `"success"`, or `"error"`.
    pub status: String,

    /// Optional output or message explaining the result of the command, if applicable.
    #[serde(default)]
    pub output: Option<String>,
}

// =============================================================================
// Logs / Monitoring
// =============================================================================

/// Represents a single log entry (for nodes or runners),
/// containing a timestamp and a message describing an event.
#[derive(Serialize, Deserialize, Debug)]
pub struct LogEntry {
    /// The time at which this log entry was recorded.
    pub timestamp: String,

    /// The text of the log entry, describing the event or status.
    pub message: String,
}

/// The response payload for an endpoint returning node-level logs.
///
/// This includes the node ID for context, as well as a collection of [`LogEntry`] objects.
#[derive(Serialize, Deserialize, Debug)]
pub struct NodeLogs {
    /// The ID of the node these logs pertain to.
    #[serde(rename = "nodeId")]
    pub node_id: String,

    /// A list of log entries recorded by this node.
    pub logs: Vec<LogEntry>,
}

/// The response payload for an endpoint returning runner-level logs.
///
/// This includes the runner ID for context, plus a collection of [`LogEntry`] objects.
#[derive(Serialize, Deserialize, Debug)]
pub struct RunnerLogResponse {
    /// The ID of the runner these logs pertain to.
    #[serde(rename = "runnerId")]
    pub runner_id: String,

    /// A list of log entries recorded by this runner.
    pub logs: Vec<LogEntry>,
}

// =============================================================================
// Display Implementations
// =============================================================================

impl fmt::Display for ManagerData {
    /// Renders a human-readable summary of `ManagerData` fields,
    /// applying colored output where applicable (e.g., version, git config, etc.).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}: {}\n{}: {}\n{}: {}\n{}: {}\n{}: {}\n",
            "Manager Version".bold(),
            self.version,
            "Git Configuration".bold().green(),
            self.git_config,
            "System apps",
            self.system_apps.to_string().bold().purple(),
            "Client apps",
            self.client_apps.to_string().bold().green(),
            "Warnings raised",
            self.warning.to_string().bold().yellow()
        )
    }
}

// ===============================================================================
// Billing Endpoint structs
// ===============================================================================

#[derive(Debug, Serialize, Deserialize)]
pub struct BillingParams {
    pub instances: u64,
}