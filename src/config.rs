use colored::Colorize;
// src/config.rs
use config::{Config, ConfigError, Environment, File};
use dusa_collection_utils::{
    core::logger::LogLevel, core::types::stringy::Stringy, core::version::SoftwareVersion,
};
use serde::{Deserialize, Serialize};
use std::{env, fmt, fs, path::Path};

use crate::{
    enviornment::definitions::{
        ApplicationType, Enviornment, Enviornment_V1, Enviornment_V2, ExecutionUser,
    },
    git_actions::GitServer,
};

/// Represents the application's configuration settings.
#[derive(Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord, Clone)]
pub struct AppConfig {
    /// A name for the application instance.
    pub app_name: Stringy, // TODO move this to the enviornment_v2 object

    /// Version of the application.
    // pub version: String,

    /// Maximum ram usage in MB
    pub max_ram_usage: usize, // TODO move this to the enviornment_v2 object

    /// Maximum cpu time usage
    /// This would be practically be used to restart a service
    /// when it gets to it's aloted cpu time. A pricing scale be
    /// set like this.
    pub max_cpu_usage: usize, // TODO move this to the enviornment_v2 object

    /// The environment the application is running in (e.g., development, staging, production).
    pub environment: Option<Enviornment>,

    /// Optional setting for enabling debug mode.
    pub debug_mode: bool, // TODO move this to the enviornment_v2 object

    /// Settings for what information is logged
    pub log_level: LogLevel, // TODO move this to the enviornment_v2 object

    /// Configuration related to the Git functionality.
    pub git: Option<GitConfig>, // TODO move this to the enviornment_v2 object

    /// Configuration related to the database (optional example). // TODO Depricate this field, we don't control db access in this plane any more
    pub database: Option<DatabaseConfig>,

    // / Configuration for Aggregator communication  // TODO Depricate this field, we don't use the aggregator any more
    pub aggregator: Option<Aggregator>, // Add other configuration sections as needed.
}

/// Intended/static workload configuration split out from runtime state.
#[derive(Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord, Clone)]
pub struct WorkloadConfig {
    pub enviornment: Enviornment,
}

impl WorkloadConfig {
    /// Creates a workload config wrapper from a concrete environment definition.
    pub fn new(enviornment: Enviornment) -> Self {
        Self { enviornment }
    }

    /// Convenience constructor for building a V2 environment using the builder API.
    pub fn new_v2() -> Enviornment_V2 {
        Enviornment::new_v2()
    }

    /// Returns the wrapped environment payload.
    pub fn get_enviornment(&self) -> &Enviornment {
        &self.enviornment
    }

    /// Returns `Some(&Enviornment_V2)` when the wrapped payload is V2.
    pub fn as_v2(&self) -> Option<&Enviornment_V2> {
        match &self.enviornment {
            Enviornment::V2(v2) => Some(v2),
            _ => None,
        }
    }

    /// Returns a best-effort RAM limit extracted from V2 configuration.
    pub fn max_ram_usage(&self) -> Option<usize> {
        self.as_v2().and_then(|v2| v2.max_ram_usage)
    }

    /// Returns a best-effort CPU limit extracted from V2 configuration.
    pub fn max_cpu_usage(&self) -> Option<usize> {
        self.as_v2().and_then(|v2| v2.max_cpu_usage)
    }

    /// Returns debug mode from V2 config, defaulting to `false` for non-V2 payloads.
    pub fn debug_mode(&self) -> bool {
        self.as_v2().map(|v2| v2.debug_mode).unwrap_or(false)
    }

    /// Returns log level from V2 config, defaulting to `Info` for non-V2 payloads.
    pub fn log_level(&self) -> LogLevel {
        self.as_v2()
            .map(|v2| v2.log_level)
            .unwrap_or(LogLevel::Info)
    }

    /// Returns git config from V2 payload when present.
    pub fn git_config(&self) -> Option<&GitConfig> {
        self.as_v2().and_then(|v2| v2.git.as_ref())
    }

    /// Returns a minimal, valid dummy workload configuration.
    pub fn dummy() -> Self {
        Self::new(Enviornment::V2(Enviornment::new_v2()))
    }
}

impl From<AppConfig> for WorkloadConfig {
    fn from(config: AppConfig) -> Self {
        if let Some(enviornment) = config.environment {
            return Self::new(enviornment);
        }

        Self::new(Enviornment::V2(Enviornment_V2 {
            max_ram_usage: Some(config.max_ram_usage),
            max_cpu_usage: Some(config.max_cpu_usage),
            debug_mode: config.debug_mode,
            log_level: config.log_level,
            git: config.git,
            execution_user: ExecutionUser::Default,
            port_range: None,
            secret_store: None,
            path_modifier: None,
            dependency_command: None,
            build_command: None,
            run_command: None,
            env_var_store: None,
        }))
    }
}

impl From<Enviornment> for WorkloadConfig {
    fn from(enviornment: Enviornment) -> Self {
        Self::new(enviornment)
    }
}

impl From<Enviornment_V2> for WorkloadConfig {
    fn from(enviornment_v2: Enviornment_V2) -> Self {
        Self::new(Enviornment::V2(enviornment_v2))
    }
}

impl fmt::Display for WorkloadConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "{}:", "WorkloadConfig".bold().underline().purple())?;
        writeln!(f, "  {}: {}", "Environment".bold().cyan(), self.enviornment)
    }
}

impl Default for WorkloadConfig {
    fn default() -> Self {
        Self::dummy()
    }
}

impl AppConfig {
    /// Transitional helper to convert legacy `AppConfig` into the environment-backed `WorkloadConfig`.
    pub fn into_workload_config(self) -> WorkloadConfig {
        self.into()
    }
}

impl WorkloadConfig {
    /// Transitional helper to convert back to legacy `AppConfig` where older APIs still require it.
    pub fn into_app_config(self) -> AppConfig {
        self.into()
    }
}

impl From<&WorkloadConfig> for AppConfig {
    fn from(config: &WorkloadConfig) -> Self {
        match &config.enviornment {
            Enviornment::V2(v2) => Self {
                app_name: Stringy::from("Workload"),
                max_ram_usage: v2.max_ram_usage.unwrap_or(0),
                max_cpu_usage: v2.max_cpu_usage.unwrap_or(0),
                environment: Some(config.enviornment.clone()),
                debug_mode: v2.debug_mode,
                log_level: v2.log_level,
                git: v2.git.clone(),
                database: None,
                aggregator: None,
            },
            Enviornment::V1(v1) => Self {
                app_name: Stringy::from("Workload"),
                max_ram_usage: 0,
                max_cpu_usage: 0,
                environment: Some(Enviornment::V1(v1.clone())),
                debug_mode: false,
                log_level: LogLevel::Info,
                git: None,
                database: None,
                aggregator: None,
            },
        }
    }
}

impl From<WorkloadConfig> for AppConfig {
    fn from(config: WorkloadConfig) -> Self {
        AppConfig::from(&config)
    }
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
}

/// Configuration settings specific to the database (optional example).
#[derive(Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord, Clone)]
pub struct DatabaseConfig {
    /// The database connection URL.
    pub url: String,

    /// The size of the connection pool.
    pub pool_size: u32,
}

impl Aggregator {
    /// Returns a dummy `Aggregator` configuration.
    pub fn dummy() -> Self {
        Self {
            socket_path: "/tmp/artisan-aggregator.sock".to_owned(),
            socket_permission: Some(0o660),
        }
    }
}

impl GitConfig {
    /// Returns a dummy `GitConfig`.
    pub fn dummy() -> Self {
        Self {
            default_server: GitServer::GitHub,
            credentials_file: "/opt/artisan/artisan.cf".to_owned(),
        }
    }
}

impl DatabaseConfig {
    /// Returns a dummy `DatabaseConfig`.
    pub fn dummy() -> Self {
        Self {
            url: "postgres://artisan:dummy_password@localhost:5432/artisan".to_owned(),
            pool_size: 10,
        }
    }
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
            environment: None,
            debug_mode: true,
            log_level: LogLevel::Debug,
            git: None,
            database: None,
            aggregator: None,
        }
    }

    /// Returns a fully-populated dummy `AppConfig` that includes all optional sections.
    pub fn dummy_populated() -> Self {
        Self {
            app_name: Stringy::from("MyDummyApp"),
            max_ram_usage: 512,
            max_cpu_usage: 80,
            environment: Some(Enviornment::V1(Enviornment_V1 {
                application_type: Some(ApplicationType::Simple),
                execution_uid: Some(1000),
                execution_gid: Some(1000),
                primary_listening_port: Some(8080),
                secret_id: Some(Stringy::from("dummy-secret-id")),
                secret_passwd: Some(Stringy::from("dummy-secret-password")),
                path_modifier: Some(Stringy::from("/opt/artisan/bin")),
                pre_build_command: Some(Stringy::from("echo prebuild")),
                build_command: Some(Stringy::from("cargo build --release")),
                run_command: Some(Stringy::from("./target/release/my_dummy_app")),
                env_key_0: Some((Stringy::from("APP_ENV"), Stringy::from("development"))),
            })),
            debug_mode: true,
            log_level: LogLevel::Debug,
            git: Some(GitConfig::dummy()),
            database: Some(DatabaseConfig::dummy()),
            aggregator: Some(Aggregator::dummy()),
        }
    }

    /// Serializes a fully-populated dummy config to pretty TOML.
    pub fn dummy_populated_toml() -> Result<String, ConfigError> {
        toml::to_string_pretty(&Self::dummy_populated())
            .map_err(|e| ConfigError::Foreign(Box::new(e)))
    }

    /// Writes a fully-populated dummy config file to `path`.
    pub fn write_dummy_file<P: AsRef<Path>>(path: P) -> Result<(), ConfigError> {
        let toml = Self::dummy_populated_toml()?;
        fs::write(path, toml).map_err(|e| ConfigError::Foreign(Box::new(e)))
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
        writeln!(
            f,
            "  {}: {}",
            "Environment".bold().cyan(),
            match &self.environment {
                Some(data) => {
                    match data {
                        Enviornment::V1(enviornment_v1) => enviornment_v1.to_string(),
                        Enviornment::V2(enviornment_v2) => enviornment_v2.to_string(),
                    }
                }
                None => "None set".bold().red().to_string(),
            }
        )?;

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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn dummy_populated_sets_all_optional_sections() {
        let config = AppConfig::dummy_populated();

        assert!(config.environment.is_some());
        assert!(config.git.is_some());
        assert!(config.database.is_some());
        assert!(config.aggregator.is_some());
    }

    #[test]
    fn write_dummy_file_writes_round_trippable_toml() {
        let temp_dir = tempdir().unwrap();
        let path = temp_dir.path().join("reference-config.toml");

        AppConfig::write_dummy_file(&path).unwrap();

        let file_contents = fs::read_to_string(&path).unwrap();
        assert!(file_contents.contains("app_name"));

        let parsed: AppConfig = toml::from_str(&file_contents).unwrap();
        assert_eq!(parsed, AppConfig::dummy_populated());
    }
}
