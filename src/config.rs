use colored::Colorize;
// src/config.rs
use config::{Config, ConfigError, Environment, File};
use dusa_collection_utils::{
    core::logger::LogLevel, core::types::stringy::Stringy, core::version::SoftwareVersion,
};
use serde::{Deserialize, Serialize};
use std::{env, fmt};

use crate::git_actions::GitServer;

/// Represents the application's configuration settings.
#[derive(Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord, Clone)]
pub struct AppConfig {
    /// A name for the application instance.
    pub app_name: Stringy,

    /// Version of the application.
    // pub version: String,

    /// Maximum ram usage in MB
    pub max_ram_usage: usize,

    /// Maximum cpu time usage
    /// This would be practically be used to restart a service
    /// when it gets to it's aloted cpu time. A pricing scale be
    /// set like this.
    pub max_cpu_usage: usize,

    /// The environment the application is running in (e.g., development, staging, production).
    pub environment: String,

    /// Optional setting for enabling debug mode.
    pub debug_mode: bool,

    /// Settings for what information is logged
    pub log_level: LogLevel,

    /// Configuration related to the Git functionality.
    pub git: Option<GitConfig>,

    /// Configuration related to the database (optional example).
    pub database: Option<DatabaseConfig>,

    /// Configuration for Aggregator communication
    pub aggregator: Option<Aggregator>, // Add other configuration sections as needed.
}

/// Configuration settings for aggregator communication
#[derive(Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord, Clone)]
pub struct Aggregator {
    /// Socket path that the application will use
    pub socket_path: String,

    /// Permissions for the socket
    pub socket_permission: Option<u32>,
}

/// Configuration settings specific to Git operations.
#[derive(Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord, Clone)]
pub struct GitConfig {
    /// The default Git server to use (e.g., "GitHub", "GitLab", or a custom URL).
    pub default_server: GitServer,

    /// Path to the file containing Git credentials.
    pub credentials_file: String,
    // /// Optional SSH key path for Git operations.
    // pub ssh_key_path: Option<String>,
}

/// Configuration settings specific to the database (optional example).
#[derive(Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord, Clone)]
pub struct DatabaseConfig {
    /// The database connection URL.
    pub url: String,

    /// The size of the connection pool.
    pub pool_size: u32,
}

impl AppConfig {
    /// Loads the configuration from files and environment variables using `ConfigBuilder`.
    ///
    /// # Returns
    ///
    /// Returns an `AppConfig` instance if successful.
    ///
    /// # Errors
    ///
    /// Returns a `ConfigError` if loading or parsing the configuration fails.
    pub fn new() -> Result<Self, ConfigError> {
        // Detect the run mode (e.g., development, production) from the RUN_MODE environment variable.
        let run_mode = env::var("RUN_MODE").unwrap_or_else(|_| "development".into());

        let version = serde_json::to_string(&SoftwareVersion::dummy())
            .map_err(|e| ConfigError::Foreign(Box::new(e)))?;

        // Start building the configuration using ConfigBuilder.
        let builder = Config::builder()
            // Set default values.
            .set_default("app_name", "MyApp")?
            .set_default("version", version)?
            .set_default("max_cpu_usage", 0)?
            .set_default("max_ram_usage", 0)?
            // .set_default("max_connections", 100)?
            .set_default("environment", "development")?
            .set_default("debug_mode", false)?
            .set_default("log_level", "Info")?
            .set_default("git", None::<String>)?
            // .set_default("git.default_server", "GitHub")?
            // .set_default("git.credentials_file", "/opt/artisan/artisan.cf")?
            // .set_default("git.ssh_key_path", None::<String>)?
            // Set defaults for optional database configuration.
            .set_default("database.url", "postgres://user:password@localhost/dbname")?
            .set_default("database.pool_size", 10)?;
        // Set defaults for aggregator communication.
        // .set_default("aggregator", value)?

        // Load the default configuration file (Settings.toml).
        let builder = builder.add_source(File::with_name("Overrides").required(false));

        // Load environment-specific configuration files (e.g., Settings.development.toml).
        let builder =
            builder.add_source(File::with_name(&format!("Settings.{}", run_mode)).required(false));

        // Add in settings from the environment (with a prefix of APP).
        // E.g., `APP_DEBUG_MODE=1` would set the `debug_mode` configuration.
        let builder = builder.add_source(Environment::with_prefix("APP").separator("__"));

        // Build the configuration.
        let config = builder.build()?;

        // Deserialize the configuration into the AppConfig struct.
        config.try_deserialize()
    }

    /// Validates the configuration values.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if validation passes.
    ///
    /// # Errors
    ///
    /// Returns a `String` with an error message if validation fails.
    pub fn validate(&self) -> Result<(), String> {
        if self.max_cpu_usage.lt(&10) {
            return Err("The cpu time won't allow the program to run".into());
        }
        if self.max_cpu_usage.lt(&0) {
            return Err("Ram limit can't be less that 0".into());
        }
        if <std::option::Option<GitConfig> as Clone>::clone(&self.git)
            .unwrap()
            .credentials_file
            .is_empty()
        {
            return Err("git.credentials_file must be provided".into());
        }
        if self.app_name.is_empty() {
            return Err("app_name must be provided".into());
        }
        // Add more validation checks as needed.

        Ok(())
    }

    // pub fn get_version(&self) -> Result<SoftwareVersion, ErrorArrayItem> {
    // let version: SoftwareVersion = serde_json::from_str(&self.version)?;
    // Ok(version)
    // }

    /// Returns a dummy `AppConfig` with hardcoded placeholder values.
    pub fn dummy() -> Self {
        AppConfig {
            app_name: Stringy::from("MyDummyApp"),
            // version: SoftwareVersion::dummy().to_string(),
            max_ram_usage: 512,
            max_cpu_usage: 80,
            environment: "development".to_string(),
            debug_mode: true,
            log_level: LogLevel::Debug,
            git: None,
            database: None,
            aggregator: None,
        }
    }
}

impl fmt::Display for AppConfig {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        // let version = self.get_version().unwrap_or(SoftwareVersion::dummy());
        writeln!(f, "{}:", "AppConfig".bold().underline().purple())?;
        writeln!(f, "  {}: {}", "App Name".bold().cyan(), self.app_name)?;
        // writeln!(
        // f,
        // "  {}: {}",
        // "Application Version".bold().cyan(),
        // version.application
        // )?;
        // writeln!(
        // f,
        // "  {}: {}",
        // "Library Version".bold().cyan(),
        // version.library
        // )?;
        writeln!(f, "  {}: {}", "Log Level".bold().cyan(), self.log_level)?;
        writeln!(f, "  {}: {}", "Ram Limit".bold().cyan(), self.max_ram_usage)?;
        writeln!(
            f,
            "  {}: {}",
            "Cpu time limit".bold().cyan(),
            self.max_cpu_usage
        )?;
        writeln!(f, "  {}: {}", "Environment".bold().cyan(), self.environment)?;
        writeln!(
            f,
            "  {}: {}",
            "Debug Mode".bold().cyan(),
            if self.debug_mode {
                "Enabled".bold().green()
            } else {
                "Disabled".bold().red()
            }
        )?;

        if let Some(git) = &self.git {
            writeln!(f, "  {}:", "Git Configuration".bold().yellow())?;
            writeln!(
                f,
                "    {}: {}",
                "Default Server".bold().cyan(),
                match &git.default_server {
                    GitServer::GitHub => "GitHub".bold(),
                    GitServer::GitLab => "GitLab".bold(),
                    GitServer::Custom(url) => format!("Custom ({})", url).bold(),
                }
            )?;
            writeln!(
                f,
                "    {}: {}",
                "Credentials File".bold().cyan(),
                git.credentials_file
            )?;
        } else {
            writeln!(f, "  {}", "Git Configuration: None".italic().dimmed())?;
        }

        if let Some(database) = &self.database {
            writeln!(f, "  {}:", "Database Configuration".bold().yellow())?;
            writeln!(f, "    {}: {}", "URL".bold().cyan(), database.url)?;
            writeln!(
                f,
                "    {}: {}",
                "Connection Pool Size".bold().cyan(),
                database.pool_size
            )?;
        } else {
            writeln!(f, "  {}", "Database Configuration: None".italic().dimmed())?;
        }

        if let Some(aggregator) = &self.aggregator {
            writeln!(f, "  {}:", "Aggregator Configuration".bold().yellow())?;
            writeln!(
                f,
                "    {}: {}",
                "Socket Path".bold().cyan(),
                aggregator.socket_path
            )?;
            if let Some(permission) = aggregator.socket_permission {
                writeln!(
                    f,
                    "    {}: {}",
                    "Socket Permission".bold().cyan(),
                    format!("{:#o}", permission).bold()
                )?;
            } else {
                writeln!(f, "    {}", "Socket Permission: None".italic().dimmed())?;
            }
        } else {
            writeln!(
                f,
                "  {}",
                "Aggregator Configuration: None".italic().dimmed()
            )?;
        }

        Ok(())
    }
}
