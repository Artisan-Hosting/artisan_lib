use colored::Colorize;
use dusa_collection_utils::stringy::Stringy;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, fmt};

use crate::version::{SoftwareVersion, Version};

/// Represents the name of a service. Each service has a unique `ServiceName`.
///
/// # Example
/// ```
/// let service_name = ServiceName(Stringy::new("MyService"));
/// println!("Service Name: {}", service_name.0);
/// ```
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash)]
pub struct ServiceName(pub Stringy);

impl fmt::Display for ServiceName {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Service Name: {}", self.0)
    }
}

/// Enum representing the different types of queries that can be made to the system.
///
/// # Example
/// ```
/// let query = QueryType::Status;
/// match query {
///     QueryType::Status => println!("Querying specific status"),
///     QueryType::AllStatuses => println!("Querying all statuses"),
///     QueryType::Command => println!("Sending a command"),
///     QueryType::System => println!("Performing a system health check"),
/// }
/// ```
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub enum QueryType {
    /// Query the status of a specific service.
    Status,
    /// Query the statuses of all services.
    AllStatuses,
    /// Query to send a command to a specific service.
    Command,
    /// Query to check the health status of the system (health check).
    System,
}

impl fmt::Display for QueryType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let query_type = match self {
            QueryType::Status => "Status",
            QueryType::AllStatuses => "All Statuses",
            QueryType::Command => "Command",
            QueryType::System => "System Health Check",
        };
        write!(f, "Query Type: {}", query_type)
    }
}

/// Structure representing a query message that can be sent by a client.
/// It allows querying statuses or sending commands to services.
///
/// # Example
/// ```
/// let query_message = QueryMessage {
///     query_type: QueryType::Status,
///     service_name: Some(ServiceName(Stringy::new("MyService"))),
///     command: None,
/// };
/// println!("Query Type: {:?}", query_message.query_type);
/// ```
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct QueryMessage {
    /// The type of query (e.g., `Status`, `AllStatuses`, `Command`, or `System`).
    pub query_type: QueryType,
    /// Optional field specifying the name of the service for which the query is being made.
    pub service_name: Option<ServiceName>,
    /// Optional field for the command to be sent if the query is of type `Command`.
    pub command: Option<Command>,
}

impl fmt::Display for QueryMessage {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "Query Message: {{ query_type: {}, service_name: {:?}, command: {:?} }}",
            self.query_type,
            self.service_name.as_ref().map(|sn| sn.to_string()),
            self.command
        )
    }
}

/// Structure representing the response to a query.
/// It can return the status of a specific service, the statuses of all services, or command acknowledgments.
///
/// # Example
/// ```
/// let response = QueryResponse {
///     version: Stringy::new("1.0.0"),
///     service_status: None,
///     all_statuses: None,
///     command_ack: Some(String::from("Command Acknowledged")),
/// };
/// println!("Response Version: {}", response.version);
/// ```
#[derive(Serialize, Deserialize, Debug)]
pub struct QueryResponse {
    /// The version of the system or service.
    pub version: Version,
    /// The status of a specific service, if requested.
    pub service_status: Option<Status>,
    /// The statuses of all services, if requested.
    pub all_statuses: Option<HashMap<ServiceName, Status>>,
    /// An acknowledgment message if a command was sent.
    pub command_ack: Option<String>,
}

impl fmt::Display for QueryResponse {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "Query Response: {{ version: {}, service_status: {:?}, all_statuses: {:?}, command_ack: {:?} }}",
            self.version, self.service_status, self.all_statuses, self.command_ack
        )
    }
}

/// Represents the status of a service at a specific time.
///
/// # Example
/// ```
/// let status = Status {
///     service_name: ServiceName(Stringy::new("MyService")),
///     app_state: AppState::Running,
///     timestamp: 1632964800,
///     version: Stringy::new("1.0.0"),
/// };
/// println!("Service: {}, Status: {:?}", status.service_name.0, status.app_status);
/// ```
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct Status {
    /// The name of the service.
    pub service_name: ServiceName,
    /// The current status of the service (e.g., `Running`, `Stopped`).
    pub app_state: AppState,
    /// The timestamp indicating when this status was last reported.
    pub timestamp: u64,
    /// The version of the service.
    pub version: Version,
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{}: {{ {}: {}, {}: {}, {}: {}, {}: {} }}",
            "Status".bold().blue(),
            "service_name".bold().green(),
            self.service_name.0.yellow(),
            "app_status".bold().green(),
            self.app_state.to_string().red(),
            "timestamp".bold().green(),
            self.timestamp.to_string().magenta(),
            "version".bold().green(),
            self.version
        )
    }
}

/// Enum representing the possible statuses of a service.
///
/// # Example
/// ```
/// let app_state = AppState::Running;
/// match app_state {
///     AppState::Running => println!("Service is running"),
///     AppState::Stopped => println!("Service is stopped"),
///     AppState::TimedOut => println!("Service timed out"),
///     AppState::Warning => println!("Service has a warning"),
/// }
/// ```
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub enum AppState {
    /// The service is currently running.
    Running,
    /// The service has stopped.
    Stopped,
    /// The service has timed out.
    TimedOut,
    /// The service is running, but there is a warning.
    Warning,
}

impl fmt::Display for AppState {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let status = match self {
            AppState::Running => "Running",
            AppState::Stopped => "Stopped",
            AppState::TimedOut => "Timed Out",
            AppState::Warning => "Warning",
        };
        write!(f, "{}", status)
    }
}

/// Enum representing different commands that can be sent to a service.
///
/// # Example
/// ```
/// let command = Command::Restart;
/// match command {
///     Command::Restart => println!("Restarting the service"),
///     Command::Reload => println!("Reloading the service"),
///     Command::Stop => println!("Stopping the service"),
/// }
/// ```
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Command to restart the service.
    Restart,
    /// Command to reload the service configuration.
    Reload,
    /// Command to stop the service.
    Stop,
}

impl fmt::Display for Command {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let command = match self {
            Command::Restart => "Restart",
            Command::Reload => "Reload",
            Command::Stop => "Stop",
        };
        write!(f, "{}", command)
    }
}

/// Enum representing different types of messages exchanged between services and the system.
///
/// # Example
/// ```
/// let message_type = MessageType::StatusUpdate;
/// match message_type {
///     MessageType::StatusUpdate => println!("Status update message"),
///     MessageType::Acknowledgment => println!("Acknowledgment message"),
///     MessageType::Query => println!("Query message"),
///     MessageType::CommandResponse => println!("Command response message"),
/// }
/// ```
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub enum MessageType {
    /// A message containing a status update from a service.
    StatusUpdate,
    /// An acknowledgment message in response to a received message.
    Acknowledgment,
    /// A query message requesting information or sending a command.
    Query,
    /// A response message indicating the result of a command sent.
    CommandResponse,
    /// A response message indicating the result of a command sent.
    Command,
}

impl fmt::Display for MessageType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let message_type = match self {
            MessageType::StatusUpdate => "Status Update",
            MessageType::Acknowledgment => "Acknowledgment",
            MessageType::Query => "Query",
            MessageType::CommandResponse => "Command Response",
            MessageType::Command => "Command",
        };
        write!(f, "{}", message_type)
    }
}

/// A general message structure used for communication between services and the system.
/// It includes the message type, payload (actual data), and an optional error message.
///
/// # Example
/// ```
/// use serde_json::json;
///
/// let general_message = GeneralMessage {
///     version: Stringy::new("1.0.0"),
///     msg_type: MessageType::StatusUpdate,
///     payload: json!({
///         "service_name": "MyService",
///         "app_status": "Running",
///         "timestamp": 1632964800,
///         "version": "1.0.0"
///     }),
///     error: None,
/// };
/// println!("General Message: {:?}", general_message);
/// ```
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GeneralMessage {
    /// The version of the system or service that sent the message.
    pub version: SoftwareVersion,
    /// The type of message being sent (e.g., `StatusUpdate`, `Acknowledgment`, `Query`).
    pub msg_type: MessageType,
    /// The actual data being sent in the message, serialized as JSON for flexibility.
    pub payload: serde_json::Value,
    /// An optional error message, if applicable.
    pub error: Option<Stringy>,
}

impl fmt::Display for GeneralMessage {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "General Message: {{ version: {}, msg_type: {}, payload: {}, error: {:?} }}",
            self.version, self.msg_type, self.payload, self.error
        )
    }
}
