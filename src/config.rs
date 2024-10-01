// src/config.rs

use dusa_collection_utils::types::PathType;
use nix::NixPath;
use serde::{Deserialize, Serialize};
use config::{Config, ConfigError, Environment, File};
use std::env;

use crate::git_actions::GitServer;

/// Represents the application's configuration settings.
#[derive(Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord, Clone)]
pub struct AppConfig {
    /// A name for the application instance.
    pub app_name: String,

    /// Version of the application.
    pub version: String,

    /// Maximum allowed number of connections or some other resource limit.
    pub max_connections: u32,

    /// The environment the application is running in (e.g., development, staging, production).
    pub environment: String,

    /// Optional setting for enabling debug mode.
    pub debug_mode: bool,

    /// Configuration related to the Git functionality.
    pub git: Option<GitConfig>,

    /// Configuration related to the database (optional example).
    pub database: Option<DatabaseConfig>,

    // Add other configuration sections as needed.
}

/// Configuration settings specific to Git operations.
#[derive(Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord, Clone)]
pub struct GitConfig {
    /// The default Git server to use (e.g., "GitHub", "GitLab", or a custom URL).
    pub default_server: GitServer,

    /// Path to the file containing Git credentials.
    pub credentials_file: PathType,

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

        // Start building the configuration using ConfigBuilder.
        let builder = Config::builder()
            // Set default values.
            .set_default("app_name", "MyApp")?
            .set_default("version", "1.0.0")?
            .set_default("max_connections", 100)?
            .set_default("environment", "development")?
            .set_default("debug_mode", false)?
            .set_default("git.default_server", "GitHub")?
            .set_default("git.credentials_file", "/opt/artisan/artisan.cf")?
            .set_default("git.ssh_key_path", None::<String>)?
            // Set defaults for optional database configuration.
            .set_default("database.url", "postgres://user:password@localhost/dbname")?
            .set_default("database.pool_size", 10)?;

        // Load the default configuration file (Settings.toml).
        let builder = builder.add_source(File::with_name("Settings").required(false));

        // Load environment-specific configuration files (e.g., Settings.development.toml).
        let builder = builder.add_source(
            File::with_name(&format!("Settings.{}", run_mode)).required(false),
        );

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
        if self.max_connections == 0 {
            return Err("max_connections must be greater than 0".into());
        }
        if <std::option::Option<GitConfig> as Clone>::clone(&self.git).unwrap().credentials_file.is_empty() {
            return Err("git.credentials_file must be provided".into());
        }
        if self.app_name.is_empty() {
            return Err("app_name must be provided".into());
        }
        // Add more validation checks as needed.

        Ok(())
    }
}
