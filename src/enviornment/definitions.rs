use colored::Colorize;
use core::fmt;
use dusa_collection_utils::core::{
    errors::{ErrorArrayItem, Errors},
    logger::LogLevel,
    types::stringy::Stringy,
};
use serde::{Deserialize, Serialize};

use crate::config::GitConfig;
use crate::encryption::{simple_decrypt, simple_encrypt};

/// A string marker identifying version 1 of the `Enviornment` configuration format.
pub const VERSION_TAG_V1: &str = "#? version:1";
/// A string marker identifying version 2 of the `Enviornment` configuration format.
pub const VERSION_TAG_V2: &str = "#? version:2";
/// A string marker identifying version 3 of the `Enviornment` configuration format.
/// (Unused placeholder)
pub const VERSION_TAG_V3: &str = "#? version:3";

// A pool of ports that an application can bind to
type PortRange = (u16, u16);

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ExecutionUser {
    /// Default is the www-data user on the system
    Default,

    /// Artisan, a dedicated user, system level applications run here,
    Artisan,

    /// Random. For maximum security, the runner creates a temporary
    /// user and group just for runtime and applies aggressive sandboxing.
    Random,

    // Some other uid / gid combo
    Custom(u16, u16),
}

/// Represents different types of applications that can be built or run.
#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, PartialOrd, Eq, Ord)]
pub enum ApplicationType {
    /// A simple application type with minimal build steps.
    Simple,
    /// A Next.js application.
    Next,
    /// An Angular.js application.
    Angular,
    /// A Python-based application.
    Python,
    /// A custom application type not covered by the above.
    Custom,
}

impl fmt::Display for ApplicationType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self {
            ApplicationType::Simple => write!(f, "{}", "Simple".cyan()),
            ApplicationType::Next => write!(f, "{}", "Next.js".bold().cyan()),
            ApplicationType::Angular => write!(f, "{}", "Angular.js".bold().cyan()),
            ApplicationType::Python => write!(f, "{}", "Python".bold().yellow()),
            ApplicationType::Custom => write!(f, "{}", "CUSTOM".bold().purple()),
        }
    }
}

/// An overarching enum for environment configurations. Currently, it supports:
///
/// - **`V1`** (`Enviornment_V1`): A first-generation environment configuration.
/// - **`V2`** (`Enviornment_V2`): A second-generation environment configuration (not documented yet).
///
/// This enum’s [`parse`] method attempts to decrypt and parse raw bytes into one of the
/// available environment versions based on a version tag (like `#? version:1` or `#? version:2`).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, PartialOrd, Eq, Ord)]
pub enum Enviornment {
    /// Represents version 1 of the environment configuration.
    V1(Enviornment_V1),
    /// Represents version 2 of the environment configuration.
    /// (Implementation under development, not documented here.)
    V2(Enviornment_V2),
}

impl Enviornment {
    /// Creates a minimal V2 environment configuration that can be incrementally assembled.
    ///
    /// This returns an [`Enviornment_V2`] builder-like value. Call chainable `with_*`
    /// helpers and then call [`Enviornment_V2::finalize`] to validate and convert to
    /// [`Enviornment::V2`].
    pub fn new_v2() -> Enviornment_V2 {
        Enviornment_V2::new()
    }

    /// Parses raw, encrypted data into either `Enviornment::V1` or `Enviornment::V2`.
    ///
    /// # Procedure
    /// - Decrypts the provided data using [`simple_decrypt`].
    /// - Reads the first line to determine the version tag (e.g., `#? version:1` or `#? version:2`).
    /// - If `version:1`, deserializes into [`Enviornment_V1`].
    /// - If `version:2`, deserializes into [`Enviornment_V2`].
    ///
    /// # Errors
    /// - Returns an [`ErrorArrayItem`] if decryption fails or if the version header is invalid.
    ///
    /// # Example
    /// ```rust,ignore
    /// let raw_data = /* some encrypted bytes for Enviornment_V1 */;
    /// match Enviornment::parse(&raw_data).await {
    ///     Ok(env) => println!("Successfully parsed environment config."),
    ///     Err(err) => eprintln!("Error parsing environment: {}", err),
    /// }
    /// ```
    pub async fn parse(data: &[u8]) -> Result<Self, ErrorArrayItem> {
        let data_bytes = simple_decrypt(data)?;
        let data_string = String::from_utf8(data_bytes).map_err(ErrorArrayItem::from)?;
        let data_lines: Vec<&str> = data_string.lines().map(|line| line).collect();

        match data_lines.first() {
            Some(line) if *line == VERSION_TAG_V1 || *line == VERSION_TAG_V2 => {
                if line.contains("1") {
                    // V1 environment format
                    let headerless_data = data_lines[1..].concat();
                    let env: Enviornment_V1 =
                        serde_json::from_str(&headerless_data).map_err(ErrorArrayItem::from)?;
                    return Ok(Self::V1(env));
                }
                if line.contains("2") {
                    // V2 environment format
                    let headerless_data = data_lines[1..].concat();
                    let env: Enviornment_V2 =
                        serde_json::from_str(&headerless_data).map_err(ErrorArrayItem::from)?;
                    return Ok(Self::V2(env));
                }
                Err(ErrorArrayItem::new(
                    Errors::ConfigParsing,
                    format!("Invalid version header: {}", line),
                ))
            }
            Some(line) => Err(ErrorArrayItem::new(
                Errors::ConfigParsing,
                format!("Invalid version header: {}", line),
            )),
            None => Err(ErrorArrayItem::new(
                Errors::ConfigParsing,
                "No data found to parse".to_string(),
            )),
        }
    }
}

impl fmt::Display for Enviornment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Enviornment::V1(enviornment_v1) => {
                write!(f, "{}", enviornment_v1)
            }
            Enviornment::V2(enviornment_v2) => {
                write!(f, "{}", enviornment_v2)
            }
        }
    }
}

/// **Environment V1**: A first-generation configuration struct containing info for building
/// and running an application. This includes user/group IDs, ports, secrets, build commands, etc.
///
/// # Fields
///
/// * `application_type` - An optional [`ApplicationType`] indicating the kind of application (e.g. Python, Angular).
/// * `execution_uid` - Optional user ID used when spawning child processes.
/// * `execution_gid` - Optional group ID used when spawning child processes.
/// * `primary_listening_port` - Port used as the main server or API listener.
/// * `secret_id` / `secret_passwd` - Commonly used to store credentials or tokens.
/// * `path_modifier` - An additional path to be appended.
/// * `pre_build_command` / `build_command` / `run_command` - Shell commands for building or running the app.
/// * `env_key_0` - A single custom environment variable in the form `(key, value)`.
#[allow(non_camel_case_types)]
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, PartialOrd, Eq, Ord)]
pub struct Enviornment_V1 {
    pub application_type: Option<ApplicationType>,
    pub execution_uid: Option<u16>,
    pub execution_gid: Option<u16>,
    pub primary_listening_port: Option<u16>,
    pub secret_id: Option<Stringy>,
    pub secret_passwd: Option<Stringy>,
    pub path_modifier: Option<Stringy>,
    pub pre_build_command: Option<Stringy>,
    pub build_command: Option<Stringy>,
    pub run_command: Option<Stringy>,
    pub env_key_0: Option<(Stringy, Stringy)>,
}

impl Enviornment_V1 {
    /// Encrypts this V1 environment configuration.  
    /// Returns a vector of bytes containing the encrypted JSON data.
    ///
    /// # Errors
    /// - Returns [`ErrorArrayItem`] if JSON serialization or encryption fails.
    pub async fn encrypt(&self) -> Result<Vec<u8>, ErrorArrayItem> {
        let data_json: String = self.to_json()?;
        let data_vec = data_json.as_bytes();
        match simple_encrypt(data_vec) {
            Ok(data) => Ok(data.as_bytes().to_vec()),
            Err(err) => Err(err),
        }
    }

    /// Converts this V1 environment configuration to a pretty-printed JSON string.
    ///
    /// # Errors
    /// - Returns [`ErrorArrayItem`] if serialization fails.
    pub fn to_json(&self) -> Result<String, ErrorArrayItem> {
        serde_json::to_string_pretty(&self).map_err(ErrorArrayItem::from)
    }

    /// Creates a version-tagged byte vector of this V1 environment configuration
    /// (including the `VERSION_TAG_V1` line). The data is then encrypted via [`simple_encrypt`].
    ///
    /// # Errors
    /// - Returns [`ErrorArrayItem`] if JSON serialization or encryption fails.
    pub async fn parse_to(&self) -> Result<Vec<u8>, ErrorArrayItem> {
        let mut json_data: String = self.to_json()?;
        // Insert the version header on its own line
        json_data.insert_str(0, &format!("{}\n", VERSION_TAG_V1));
        let bytes: Vec<u8> = simple_encrypt(json_data.as_bytes())?.as_bytes().to_vec();
        Ok(bytes)
    }

    /// Decrypts and deserializes the provided bytes to produce an `Enviornment_V1`.  
    /// The first line in the decrypted text is expected to be `VERSION_TAG_V1`.
    ///
    /// # Arguments
    /// * `data` - The encrypted bytes containing a `Enviornment_V1` configuration.
    ///
    /// # Errors
    /// - Returns [`ErrorArrayItem`] if decryption fails or if the version header is missing/invalid.
    pub async fn parse_from(data: &[u8]) -> Result<Self, ErrorArrayItem> {
        let data_bytes = simple_decrypt(data)?;
        let data_string = String::from_utf8(data_bytes).map_err(ErrorArrayItem::from)?;
        let data_lines: Vec<&str> = data_string.lines().map(|line| line).collect();

        match data_lines.first() {
            Some(line) if *line == VERSION_TAG_V1 => {
                // parse the correct version
                let headerless_data = data_lines[1..].concat();
                let env: Enviornment_V1 =
                    serde_json::from_str(&headerless_data).map_err(ErrorArrayItem::from)?;
                Ok(env)
            }
            Some(line) => Err(ErrorArrayItem::new(
                Errors::ConfigParsing,
                format!("Invalid version header: {}", line),
            )),
            None => Err(ErrorArrayItem::new(
                Errors::ConfigParsing,
                "No data found to parse".to_string(),
            )),
        }
    }
}

impl fmt::Display for Enviornment_V1 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let uid_string = if let Some(uid) = self.execution_uid {
            format!("UID: {}", uid.to_string().cyan())
        } else {
            format!("UID: {}", "None".cyan())
        };

        let gid_string = if let Some(gid) = self.execution_gid {
            format!("GID: {}", gid.to_string().cyan())
        } else {
            format!("GID: {}", "None".cyan())
        };

        let port_string = if let Some(port) = self.primary_listening_port {
            format!("LISTENING PORT: {}", port.to_string().bright_cyan())
        } else {
            format!("LISTENING PORT: {}", "None".bright_cyan())
        };

        let secret_id_string = if let Some(id) = &self.secret_id {
            format!("SECRET_ID: {}", id.to_string().yellow())
        } else {
            format!("SECRET_ID: {}", "None".yellow())
        };

        let secret_passwd_string = if let Some(_) = self.secret_passwd {
            format!("SECRET_PASSWD: {}", "Populated".bold().green())
        } else {
            format!("SECRET_PASSWD: {}", "None".bold().green())
        };

        let modifier_string = if let Some(string) = &self.path_modifier {
            format!("PATH: {}", string.bold().purple())
        } else {
            format!("PATH: {}", "None".bold().purple())
        };

        let build_command = if let Some(string) = &self.build_command {
            format!("BUILD COMMAND: {}", string.bold().purple())
        } else {
            format!("BUILD COMMAND: {}", "None".bold().purple())
        };

        let pre_build_command = if let Some(string) = &self.pre_build_command {
            format!("PRE BUILD COMMAND: {}", string.bold().purple())
        } else {
            format!("PRE BUILD COMMAND: {}", "None".bold().purple())
        };

        let env_key_0 = if let Some(string) = &self.env_key_0 {
            format!(
                "ENV MOD 0: {} = {}",
                string.0.bold().green(),
                string.1.bold().green()
            )
        } else {
            format!("ENV MOD 0: {}", "None".bold().green())
        };

        let app_type = if let Some(app_type) = &self.application_type {
            format!("APPLICATION: {}", app_type)
        } else {
            format!("APPLICATION: {}", "None".bold().blue())
        };

        let run_command = if let Some(string) = &self.run_command {
            format!("RUN: {}", string.bold().purple())
        } else {
            format!("RUN: {}", "None".bold().purple())
        };

        write!(
            f,
            "{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}",
            uid_string,
            gid_string,
            port_string,
            secret_id_string,
            secret_passwd_string,
            modifier_string,
            build_command,
            pre_build_command,
            env_key_0,
            app_type,
            run_command,
        )
    }
}

//================================================
// (Below code is intentionally left undocumented.
//  Enviornment_V2 is still under development.)
//================================================

#[allow(non_camel_case_types)]
#[rustfmt::skip]
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, PartialOrd, Eq, Ord)]
pub struct Enviornment_V2 {
    pub max_ram_usage:              Option<usize>, // From AppConfig
    pub max_cpu_usage:              Option<usize>, // From AppConfig
    pub debug_mode:                 bool, // From AppConfig
    pub log_level:                  LogLevel, // From AppConfig
    pub git:                        Option<GitConfig>, // From AppConfig
    //pub application_type:       Option<ApplicationType>, // Application for building
    pub execution_user:             ExecutionUser, // Defined user and gid to run a program as
    pub port_range:                 Option<PortRange>,
    pub secret_store:               Option<Vec<(String, String)>>, // Secrets written as environment variables
    pub path_modifier:              Option<Stringy>, // Data to append to the PATH string
    pub dependency_command:         Option<Stringy>, // i.e. npm install, command to handle dependencies
    pub build_command:              Option<Stringy>, // Command to build the project
    pub run_command:                Option<Stringy>, // Command to spawn the project
    pub env_var_store:              Option<Vec<(String, String)>>, // Optional environment overrides or variables
}

impl Enviornment_V2 {
    /// Creates a minimal V2 configuration.
    pub fn new() -> Self {
        Self {
            max_ram_usage: None,
            max_cpu_usage: None,
            debug_mode: false,
            log_level: LogLevel::Info,
            git: None,
            execution_user: ExecutionUser::Default,
            port_range: None,
            secret_store: None,
            path_modifier: None,
            dependency_command: None,
            build_command: None,
            run_command: None,
            env_var_store: None,
        }
    }

    /// Sets `max_ram_usage` (RAM limit in MB).
    pub fn set_max_ram_usage(&mut self, max_ram_usage: usize) -> &mut Self {
        self.max_ram_usage = Some(max_ram_usage);
        self
    }

    /// Sets `max_ram_usage` (RAM limit in MB) and returns `Self` for fluent chaining.
    #[must_use]
    pub fn with_max_ram_usage(mut self, max_ram_usage: usize) -> Self {
        self.set_max_ram_usage(max_ram_usage);
        self
    }

    /// Sets `max_cpu_usage` (CPU limit).
    pub fn set_max_cpu_usage(&mut self, max_cpu_usage: usize) -> &mut Self {
        self.max_cpu_usage = Some(max_cpu_usage);
        self
    }

    /// Sets `max_cpu_usage` (CPU limit) and returns `Self` for fluent chaining.
    #[must_use]
    pub fn with_max_cpu_usage(mut self, max_cpu_usage: usize) -> Self {
        self.set_max_cpu_usage(max_cpu_usage);
        self
    }

    /// Sets `debug_mode`.
    pub fn set_debug_mode(&mut self, debug_mode: bool) -> &mut Self {
        self.debug_mode = debug_mode;
        self
    }

    /// Sets `debug_mode` and returns `Self` for fluent chaining.
    #[must_use]
    pub fn with_debug_mode(mut self, debug_mode: bool) -> Self {
        self.set_debug_mode(debug_mode);
        self
    }

    /// Sets `log_level`.
    pub fn set_log_level(&mut self, log_level: LogLevel) -> &mut Self {
        self.log_level = log_level;
        self
    }

    /// Sets `log_level` and returns `Self` for fluent chaining.
    #[must_use]
    pub fn with_log_level(mut self, log_level: LogLevel) -> Self {
        self.set_log_level(log_level);
        self
    }

    /// Sets `git` configuration.
    pub fn set_git_config(&mut self, git: GitConfig) -> &mut Self {
        self.git = Some(git);
        self
    }

    /// Sets `git` configuration and returns `Self` for fluent chaining.
    #[must_use]
    pub fn with_git_config(mut self, git: GitConfig) -> Self {
        self.set_git_config(git);
        self
    }

    /// Sets `execution_user`.
    pub fn set_execution_user(&mut self, execution_user: ExecutionUser) -> &mut Self {
        self.execution_user = execution_user;
        self
    }

    /// Sets `execution_user` and returns `Self` for fluent chaining.
    #[must_use]
    pub fn with_execution_user(mut self, execution_user: ExecutionUser) -> Self {
        self.set_execution_user(execution_user);
        self
    }

    /// Sets `port_range` as `(start, end)`.
    pub fn set_port_range(&mut self, start: u16, end: u16) -> &mut Self {
        self.port_range = Some((start, end));
        self
    }

    /// Sets `port_range` and returns `Self` for fluent chaining.
    #[must_use]
    pub fn with_port_range(mut self, start: u16, end: u16) -> Self {
        self.set_port_range(start, end);
        self
    }

    /// Replaces `secret_store` with the provided list.
    pub fn set_secret_store(&mut self, secret_store: Vec<(String, String)>) -> &mut Self {
        self.secret_store = Some(secret_store);
        self
    }

    /// Replaces `secret_store` and returns `Self` for fluent chaining.
    #[must_use]
    pub fn with_secret_store(mut self, secret_store: Vec<(String, String)>) -> Self {
        self.set_secret_store(secret_store);
        self
    }

    /// Appends one `(key, value)` pair to `secret_store`.
    pub fn add_secret<K, V>(&mut self, key: K, value: V) -> &mut Self
    where
        K: Into<String>,
        V: Into<String>,
    {
        let mut secret_store = self.secret_store.clone().unwrap_or_default();
        secret_store.push((key.into(), value.into()));
        self.secret_store = Some(secret_store);
        self
    }

    /// Appends one secret entry and returns `Self` for fluent chaining.
    #[must_use]
    pub fn with_secret<K, V>(mut self, key: K, value: V) -> Self
    where
        K: Into<String>,
        V: Into<String>,
    {
        self.add_secret(key, value);
        self
    }

    /// Sets `path_modifier`.
    pub fn set_path_modifier<S>(&mut self, path_modifier: S) -> &mut Self
    where
        S: Into<String>,
    {
        let path_modifier_string = path_modifier.into();
        self.path_modifier = Some(Stringy::from(path_modifier_string.as_str()));
        self
    }

    /// Sets `path_modifier` and returns `Self` for fluent chaining.
    #[must_use]
    pub fn with_path_modifier<S>(mut self, path_modifier: S) -> Self
    where
        S: Into<String>,
    {
        self.set_path_modifier(path_modifier);
        self
    }

    /// Sets `dependency_command`.
    pub fn set_dependency_command<S>(&mut self, dependency_command: S) -> &mut Self
    where
        S: Into<String>,
    {
        let dependency_command = dependency_command.into();
        self.dependency_command = Some(Stringy::from(dependency_command.as_str()));
        self
    }

    /// Sets `dependency_command` and returns `Self` for fluent chaining.
    #[must_use]
    pub fn with_dependency_command<S>(mut self, dependency_command: S) -> Self
    where
        S: Into<String>,
    {
        self.set_dependency_command(dependency_command);
        self
    }

    /// Sets `build_command`.
    pub fn set_build_command<S>(&mut self, build_command: S) -> &mut Self
    where
        S: Into<String>,
    {
        let build_command = build_command.into();
        self.build_command = Some(Stringy::from(build_command.as_str()));
        self
    }

    /// Sets `build_command` and returns `Self` for fluent chaining.
    #[must_use]
    pub fn with_build_command<S>(mut self, build_command: S) -> Self
    where
        S: Into<String>,
    {
        self.set_build_command(build_command);
        self
    }

    /// Sets `run_command`.
    pub fn set_run_command<S>(&mut self, run_command: S) -> &mut Self
    where
        S: Into<String>,
    {
        let run_command = run_command.into();
        self.run_command = Some(Stringy::from(run_command.as_str()));
        self
    }

    /// Sets `run_command` and returns `Self` for fluent chaining.
    #[must_use]
    pub fn with_run_command<S>(mut self, run_command: S) -> Self
    where
        S: Into<String>,
    {
        self.set_run_command(run_command);
        self
    }

    /// Replaces `env_var_store` with the provided list.
    pub fn set_env_var_store(&mut self, env_var_store: Vec<(String, String)>) -> &mut Self {
        self.env_var_store = Some(env_var_store);
        self
    }

    /// Replaces `env_var_store` and returns `Self` for fluent chaining.
    #[must_use]
    pub fn with_env_var_store(mut self, env_var_store: Vec<(String, String)>) -> Self {
        self.set_env_var_store(env_var_store);
        self
    }

    /// Appends one `(key, value)` pair to `env_var_store`.
    pub fn add_env_var<K, V>(&mut self, key: K, value: V) -> &mut Self
    where
        K: Into<String>,
        V: Into<String>,
    {
        let mut env_var_store = self.env_var_store.clone().unwrap_or_default();
        env_var_store.push((key.into(), value.into()));
        self.env_var_store = Some(env_var_store);
        self
    }

    /// Appends one environment variable entry and returns `Self` for fluent chaining.
    #[must_use]
    pub fn with_env_var<K, V>(mut self, key: K, value: V) -> Self
    where
        K: Into<String>,
        V: Into<String>,
    {
        self.add_env_var(key, value);
        self
    }

    /// Validates the current V2 configuration values.
    pub fn validate(&self) -> Result<(), ErrorArrayItem> {
        if let Some(max_ram_usage) = self.max_ram_usage {
            if max_ram_usage == 0 {
                return Err(ErrorArrayItem::new(
                    Errors::ConfigParsing,
                    "max_ram_usage must be greater than 0".to_string(),
                ));
            }
        }

        if let Some(max_cpu_usage) = self.max_cpu_usage {
            if max_cpu_usage == 0 {
                return Err(ErrorArrayItem::new(
                    Errors::ConfigParsing,
                    "max_cpu_usage must be greater than 0".to_string(),
                ));
            }
        }

        if let Some((start, end)) = self.port_range {
            if start == 0 || end == 0 {
                return Err(ErrorArrayItem::new(
                    Errors::ConfigParsing,
                    "port_range values must be greater than 0".to_string(),
                ));
            }
            if start > end {
                return Err(ErrorArrayItem::new(
                    Errors::ConfigParsing,
                    "port_range start must be less than or equal to end".to_string(),
                ));
            }
        }

        if let Some(git) = &self.git {
            if git.credentials_file.trim().is_empty() {
                return Err(ErrorArrayItem::new(
                    Errors::ConfigParsing,
                    "git.credentials_file must not be empty".to_string(),
                ));
            }
        }

        if let Some(path_modifier) = &self.path_modifier {
            if path_modifier.to_string().trim().is_empty() {
                return Err(ErrorArrayItem::new(
                    Errors::ConfigParsing,
                    "path_modifier must not be empty".to_string(),
                ));
            }
        }

        if let Some(dependency_command) = &self.dependency_command {
            if dependency_command.to_string().trim().is_empty() {
                return Err(ErrorArrayItem::new(
                    Errors::ConfigParsing,
                    "dependency_command must not be empty".to_string(),
                ));
            }
        }

        if let Some(build_command) = &self.build_command {
            if build_command.to_string().trim().is_empty() {
                return Err(ErrorArrayItem::new(
                    Errors::ConfigParsing,
                    "build_command must not be empty".to_string(),
                ));
            }
        }

        if let Some(run_command) = &self.run_command {
            if run_command.to_string().trim().is_empty() {
                return Err(ErrorArrayItem::new(
                    Errors::ConfigParsing,
                    "run_command must not be empty".to_string(),
                ));
            }
        }

        if let Some(secret_store) = &self.secret_store {
            if secret_store
                .iter()
                .any(|(key, _value)| key.trim().is_empty())
            {
                return Err(ErrorArrayItem::new(
                    Errors::ConfigParsing,
                    "secret_store keys must not be empty".to_string(),
                ));
            }
        }

        if let Some(env_var_store) = &self.env_var_store {
            if env_var_store
                .iter()
                .any(|(key, _value)| key.trim().is_empty())
            {
                return Err(ErrorArrayItem::new(
                    Errors::ConfigParsing,
                    "env_var_store keys must not be empty".to_string(),
                ));
            }
        }

        Ok(())
    }

    /// Validates and converts this builder into `Enviornment::V2`.
    pub fn finalize(self) -> Result<Enviornment, ErrorArrayItem> {
        self.validate()?;
        Ok(Enviornment::V2(self))
    }

    // Returns cipher text of the data
    pub async fn encrypt(&self) -> Result<Vec<u8>, ErrorArrayItem> {
        let data_json: String = self.to_json()?;
        let data_vec = data_json.as_bytes();
        // unsafe { clean_override_op(encrypt_data, data_vec).await }
        Ok(simple_encrypt(data_vec)?.as_bytes().to_vec())
    }

    // return the json encoded data
    pub fn to_json(&self) -> Result<String, ErrorArrayItem> {
        serde_json::to_string_pretty(&self).map_err(ErrorArrayItem::from)
    }

    /// Creates a version-tagged byte vector of this V2 environment configuration
    /// (including the `VERSION_TAG_V2` line). The data is then encrypted via [`simple_encrypt`].
    pub async fn parse_to(&self) -> Result<Vec<u8>, ErrorArrayItem> {
        let mut json_data: String = self.to_json()?;
        json_data.insert_str(0, &format!("{}\n", VERSION_TAG_V2));
        Ok(simple_encrypt(json_data.as_bytes())?.as_bytes().to_vec())
    }

    /// Decrypts and deserializes encrypted bytes to produce an `Enviornment_V2`.
    /// The first line in the decrypted text is expected to be `VERSION_TAG_V2`.
    pub async fn parse(data: &[u8]) -> Result<Self, ErrorArrayItem> {
        let data_bytes = simple_decrypt(data)?;
        let data_string = String::from_utf8(data_bytes).map_err(ErrorArrayItem::from)?;
        let data_lines: Vec<&str> = data_string.lines().map(|line| line).collect();

        match data_lines.first() {
            Some(line) if *line == VERSION_TAG_V2 => {
                // parse the correct version
                let headerless_data = data_lines[1..].concat();
                serde_json::from_str(&headerless_data).map_err(ErrorArrayItem::from)
            }
            Some(line) => Err(ErrorArrayItem::new(
                Errors::ConfigParsing,
                format!("Invalid version header: {}", line),
            )),
            None => Err(ErrorArrayItem::new(
                Errors::ConfigParsing,
                "No data found to parse".to_string(),
            )),
        }
    }
}

impl fmt::Display for Enviornment_V2 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let ram_limit_string = if let Some(limit) = self.max_ram_usage {
            format!("RAM LIMIT: {}", limit.to_string().cyan())
        } else {
            format!("RAM LIMIT: {}", "None".cyan())
        };

        let cpu_limit_string = if let Some(limit) = self.max_cpu_usage {
            format!("CPU LIMIT: {}", limit.to_string().cyan())
        } else {
            format!("CPU LIMIT: {}", "None".cyan())
        };

        let debug_mode_string = if self.debug_mode {
            format!("DEBUG MODE: {}", "Enabled".bold().green())
        } else {
            format!("DEBUG MODE: {}", "Disabled".bold().red())
        };

        let log_level_string = format!("LOG LEVEL: {}", self.log_level.to_string().cyan());

        let git_string = if self.git.is_some() {
            format!("GIT CONFIG: {}", "Populated".bold().green())
        } else {
            format!("GIT CONFIG: {}", "None".bold().green())
        };

        let execution_user_string = match self.execution_user {
            ExecutionUser::Default => format!("EXECUTION USER: {}", "Default".cyan()),
            ExecutionUser::Artisan => format!("EXECUTION USER: {}", "Artisan".cyan()),
            ExecutionUser::Random => format!("EXECUTION USER: {}", "Random".cyan()),
            ExecutionUser::Custom(uid, gid) => {
                format!(
                    "EXECUTION USER: {}:{}",
                    uid.to_string().cyan(),
                    gid.to_string().cyan()
                )
            }
        };

        let port_range_string = if let Some(range) = self.port_range {
            format!("PORT RANGE: {}-{}", range.0, range.1)
        } else {
            format!("PORT RANGE: {}", "None".bright_cyan())
        };

        let secret_store_string = if let Some(secret_store) = &self.secret_store {
            format!("SECRETS: {}", secret_store.len().to_string().bold().green())
        } else {
            format!("SECRETS: {}", "0".bold().green())
        };

        let env_var_store_string = if let Some(env_var_store) = &self.env_var_store {
            format!(
                "ENV VARS: {}",
                env_var_store.len().to_string().bold().green()
            )
        } else {
            format!("ENV VARS: {}", "0".bold().green())
        };

        let modifier_string = if let Some(string) = &self.path_modifier {
            format!("PATH: {}", string.bold().purple())
        } else {
            format!("PATH: {}", "None".bold().purple())
        };

        let dependency_string = if let Some(string) = &self.dependency_command {
            format!("DEPENDENCY COMMAND: {}", string.bold().purple())
        } else {
            format!("DEPENDENCY COMMAND: {}", "None".bold().purple())
        };

        let build_string = if let Some(string) = &self.build_command {
            format!("BUILD COMMAND: {}", string.bold().purple())
        } else {
            format!("BUILD COMMAND: {}", "None".bold().purple())
        };

        let run_string = if let Some(string) = &self.run_command {
            format!("RUN COMMAND: {}", string.bold().purple())
        } else {
            format!("RUN COMMAND: {}", "None".bold().purple())
        };

        write!(
            f,
            "{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}",
            ram_limit_string,
            cpu_limit_string,
            debug_mode_string,
            log_level_string,
            git_string,
            execution_user_string,
            port_range_string,
            secret_store_string,
            env_var_store_string,
            modifier_string,
            dependency_string,
            build_string,
            run_string,
        )
    }
}
