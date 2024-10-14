use colored::Colorize;
use serde::{Deserialize, Serialize};
use std::{fmt, fs};

use dusa_collection_utils::types::PathType;
use dusa_collection_utils::{errors::ErrorArrayItem, stringy::Stringy};

use crate::git_actions::GitServer;
use crate::timestamp::format_unix_timestamp;
use crate::{
    config::AppConfig,
    encryption::{decrypt_text, encrypt_text},
};

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, PartialOrd, Ord, Clone)]
pub struct AppState {
    pub data: String, // General-purpose data field for storing string data

    // The timestamp when the state was last updated
    pub last_updated: u64, // Unix timestamp in seconds

    // A counter for tracking the number of times an event has occurred
    pub event_counter: u32,

    // A flag indicating whether the application is in an active state
    pub is_active: bool,

    // List of errors that have occurred during runtime
    pub error_log: Vec<ErrorArrayItem>,

    // Configuration settings for the application
    pub config: AppConfig,
}

impl fmt::Display for AppState {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
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
        writeln!(
            f,
            "  {}: {}",
            "Is Active".bold().blue(),
            if self.is_active {
                "Yes".bold().green()
            } else {
                "No".bold().red()
            }
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
            "Version".bold().cyan(),
            self.config.version
        )?;
        writeln!(
            f,
            "    {}: {}",
            "Max Connections".bold().cyan(),
            self.config.max_connections
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

        Ok(())
    }
}

pub struct StatePersistence;

impl StatePersistence {
    pub fn get_state_path(config: &AppConfig) -> PathType {
        PathType::Content(format!("/tmp/.{}.state", config.app_name))
    }

    pub fn save_state(state: &AppState, path: &PathType) -> Result<(), Box<dyn std::error::Error>> {
        let toml_str: Stringy = toml::to_string(state)?.into();
        let state_data = encrypt_text(toml_str)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.err_mesg))?;
        fs::write(path, state_data.to_string())?;
        Ok(())
    }

    pub fn load_state(path: &PathType) -> Result<AppState, Box<dyn std::error::Error>> {
        let encrypted_content: Stringy = fs::read_to_string(path)?.into();
        let content: Stringy = decrypt_text(encrypted_content).map_err(|_| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, "Decryption failed")
        })?;
        let state: AppState = toml::from_str(&content)?;
        Ok(state)
    }
}
