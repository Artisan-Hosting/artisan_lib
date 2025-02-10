use colored::Colorize;
use dusa_collection_utils::errors::Errors;
use dusa_collection_utils::functions::current_timestamp;
use dusa_collection_utils::logger::{LogLevel, set_log_level};
use dusa_collection_utils::types::pathtype::PathType;
use dusa_collection_utils::types::stringy::Stringy;
use dusa_collection_utils::version::SoftwareVersion;
use serde::{Deserialize, Serialize};
use std::{fmt, fs};

use dusa_collection_utils::{errors::ErrorArrayItem};
use dusa_collection_utils::log;
use crate::aggregator::{Metrics, Status};
use crate::encryption::{simple_decrypt, simple_encrypt};
use crate::git_actions::GitServer;
use crate::timestamp::format_unix_timestamp;
use crate::config::AppConfig;

/// Represents the application’s overall state, including:
/// - **Application name and version**  
/// - **Status** (e.g., running, stopped)  
/// - **PID**  
/// - **Logs of any encountered errors**  
/// - **Configuration** settings  
/// - **Timestamps** for when the state was last updated
#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, PartialOrd, Ord, Clone)]
pub struct AppState {
    /// Name of the crate or application.
    pub name: String,

    /// The current software version of the application.
    pub version: SoftwareVersion,

    /// A general-purpose string for storing state-specific data (small pieces of persistent info).
    pub data: String,

    /// The current status of the application (e.g., Running, Stopped, Warning).
    pub status: Status,

    /// The PID of the running application process.
    pub pid: u32,

    /// A Unix timestamp representing when the state was last updated.
    pub last_updated: u64,

    /// A Unix timestamp created when the application was initially launched
    pub stared_at: u64,

    /// An incrementing counter used to detect if the application is actively performing actions 
    /// (e.g., each critical operation increases it by 1).
    pub event_counter: u32,

    /// A list of errors encountered during runtime. Useful for debugging or post-mortem analysis.
    pub error_log: Vec<ErrorArrayItem>,

    /// Configuration settings loaded from external sources (e.g., a config file).
    pub config: AppConfig,

    /// Indicates if the application is a core system process (`true`) or a user application (`false`).
    pub system_application: bool,
}

impl fmt::Display for AppState {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let version = &self.version;
        writeln!(f, "{}:", "AppState".bold().underline().cyan())?;
        writeln!(f, "  {}: {}", "Data".bold().green(), self.data)?;
        writeln!(
            f,
            "  {}: {}",
            "Seconds Since Update".bold().yellow(),
            format_unix_timestamp(self.last_updated)
        )?;
        writeln!(
            f,
            "  {}: {}",
            "Event Counter".bold().magenta(),
            self.event_counter
        )?;
        writeln!(f, "  {}:", "Error Log".bold().red())?;
        if self.error_log.is_empty() {
            writeln!(f, "    {}", "No errors".italic().dimmed())?;
        } else {
            for (i, error) in self.error_log.iter().enumerate() {
                writeln!(
                    f,
                    "    {}: {:#?} - {}",
                    format!("Error {}", i + 1).bold().yellow(),
                    error.err_type,
                    error.err_mesg
                )?;
            }
        }
        writeln!(f, "  {}:", "Config".bold().purple())?;
        writeln!(
            f,
            "    {}: {}",
            "App Name".bold().cyan(),
            self.config.app_name
        )?;
        writeln!(
            f,
            "    {}: {}",
            "Software Version".bold().cyan(),
            version.application
        )?;
        writeln!(
            f,
            "    {}: {}",
            "Library Version".bold().cyan(),
            version.library
        )?;
        writeln!(
            f,
            "    {}: {}",
            "Ram Limit".bold().cyan(),
            self.config.max_ram_usage
        )?;
        writeln!(f, "    {}: {}", "PID".bold().purple(), self.pid,)?;
        writeln!(
            f,
            "    {}: {}",
            "Cpu time limit".bold().cyan(),
            self.config.max_cpu_usage
        )?;
        writeln!(
            f,
            "    {}: {}",
            "Environment".bold().cyan(),
            self.config.environment
        )?;
        writeln!(
            f,
            "    {}: {}",
            "Debug Mode".bold().cyan(),
            if self.config.debug_mode {
                "Enabled".bold().green()
            } else {
                "Disabled".bold().red()
            }
        )?;

        if let Some(git) = &self.config.git {
            writeln!(f, "    {}:", "Git Configuration".bold().purple())?;
            writeln!(
                f,
                "      {}: {}",
                "Default Server".bold().cyan(),
                match &git.default_server {
                    GitServer::GitHub => "GitHub".bold(),
                    GitServer::GitLab => "GitLab".bold(),
                    GitServer::Custom(url) => format!("Custom ({})", url).bold(),
                }
            )?;
            writeln!(
                f,
                "      {}: {}",
                "Credentials File".bold().cyan(),
                git.credentials_file
            )?;
        } else {
            writeln!(f, "    {}", "Git Configuration: None".italic().dimmed())?;
        }

        if let Some(database) = &self.config.database {
            writeln!(f, "    {}:", "Database Configuration".bold().purple())?;
            writeln!(f, "      {}: {}", "URL".bold().cyan(), database.url)?;
            writeln!(
                f,
                "      {}: {}",
                "Connection Pool Size".bold().cyan(),
                database.pool_size
            )?;
        } else {
            writeln!(
                f,
                "    {}",
                "Database Configuration: None".italic().dimmed()
            )?;
        }

        if let Some(aggregator) = &self.config.aggregator {
            writeln!(f, "    {}:", "Aggregator Configuration".bold().purple())?;
            writeln!(
                f,
                "      {}: {}",
                "Path".bold().cyan(),
                aggregator.socket_path
            )?;
        } else {
            writeln!(
                f,
                "    {}",
                "Aggregator Configuration: None".italic().dimmed()
            )?;
        }

        writeln!(f, "Status: {}", &self.status)?;

        Ok(())
    }
}

/// Provides utility methods for loading and saving [`AppState`] from/to disk.
pub struct StatePersistence;

impl StatePersistence {
    /// Derives the default save path for the application state using `/tmp/.<app_name>.state`.
    ///
    /// # Example
    /// ```rust
    /// # use artisan_middleware::state_persistence::StatePersistence;
    /// # use artisan_middleware::config::AppConfig;
    /// let config = AppConfig::dummy();
    /// let path = StatePersistence::get_state_path(&config);
    /// println!("State file path: {:?}", path);
    /// ```
    pub fn get_state_path(config: &AppConfig) -> PathType {
        PathType::Content(format!("/tmp/.{}.state", config.app_name))
    }

    /// Saves the provided [`AppState`] to the specified `path`.  
    /// The data is serialized to TOML, then encrypted with [`simple_encrypt`].
    ///
    /// # Errors
    /// - Returns an `Err` if serialization, encryption, or writing to the file fails.
    pub async fn save_state(
        state: &AppState,
        path: &PathType,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let toml_str: Stringy = toml::to_string(state)?.into();
        let state_data = simple_encrypt(toml_str.as_bytes()).map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, e.err_mesg.to_string())
        })?;

        fs::write(path, state_data.to_string())?;
        Ok(())
    }

    /// Loads an [`AppState`] from the specified `path`.  
    /// Reads the file, then decrypts it with [`simple_decrypt`], and finally deserializes from TOML.
    ///
    /// # Errors
    /// - Returns an `Err` if decryption or TOML deserialization fails, or if the file is unreadable.
    pub async fn load_state(path: &PathType) -> Result<AppState, Box<dyn std::error::Error>> {
        let encrypted_content: Stringy = fs::read_to_string(path)?.into();
        let content = simple_decrypt(encrypted_content.as_bytes()).map_err(|_| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, "Decryption failed")
        })?;

        let cipher_string = String::from_utf8(content).map_err(|_| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, "Failed to convert to string")
        })?;

        let state: AppState = toml::from_str(&cipher_string)?;
        Ok(state)
    }
}

/// Updates an [`AppState`] with a new timestamp, increments the event counter, and saves it.
/// Optionally records resource usage metrics.
///
/// # Arguments
/// - `state`: A mutable reference to the current application state.
/// - `path`: The path where the state file is stored.
/// - `metrics`: Optional resource usage metrics to associate with this update.
///
/// # Note
/// - If saving fails, logs the error and pushes an [`ErrorArrayItem`] to `state.error_log`.
pub async fn update_state(state: &mut AppState, path: &PathType, _metrics: Option<Metrics>) {
    state.last_updated = current_timestamp();
    state.event_counter += 1;

    // Attempt to save the state to disk
    if let Err(err) = StatePersistence::save_state(state, path).await {
        log!(LogLevel::Error, "Failed to save state: {}", err);
        state.error_log.push(ErrorArrayItem::new(
            Errors::GeneralError,
            format!("{}", err),
        ));
    }

    log!(LogLevel::Debug, "State Updated");
}

/// Performs final updates to the [`AppState`] before application shutdown.  
/// Sets `state.data` to "Terminated" and `state.status` to `Stopping`, then saves the state.
pub async fn wind_down_state(state: &mut AppState, state_path: &PathType) {
    state.data = String::from("Terminated");
    state.status = Status::Stopping;
    state.error_log.push(ErrorArrayItem::new(
        Errors::GeneralError,
        "Wind down requested - check logs".to_owned(),
    ));
    update_state(state, &state_path, None).await;
}

/// Logs an error, adds it to `state.error_log`, updates the application status to `Warning`,
/// and saves the updated state.
pub async fn log_error(state: &mut AppState, error: ErrorArrayItem, path: &PathType) {
    log!(LogLevel::Error, "{}", error);
    state.error_log.push(error);
    state.status = Status::Warning;
    update_state(state, path, None).await;
}

/// If the current [`AppState`] is in debug mode, sets the global log level to [`LogLevel::Debug`].
/// Otherwise, the logging remains unchanged.
///
/// # Behavior
/// - Logs at `[Trace]` level that the log level is being updated (for troubleshooting).
pub fn debug_log_set(state: &AppState) {
    log!(LogLevel::Trace, "Updating log level");
    if state.config.debug_mode {
        set_log_level(LogLevel::Debug);
    }
}
