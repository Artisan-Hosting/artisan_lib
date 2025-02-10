use core::fmt;
use colored::Colorize;
use dusa_collection_utils::{
    errors::{ErrorArrayItem, Errors}, log, logger::LogLevel, types::stringy::Stringy
};
use serde::{Deserialize, Serialize};

use crate::encryption::{simple_decrypt, simple_encrypt};

/// A string marker identifying version 1 of the `Enviornment` configuration format.
pub const VERSION_TAG_V1: &str = "#? version:1";
/// A string marker identifying version 2 of the `Enviornment` configuration format.
pub const VERSION_TAG_V2: &str = "#? version:2";
/// A string marker identifying version 3 of the `Enviornment` configuration format.
/// (Unused placeholder)
pub const VERSION_TAG_V3: &str = "#? version:3";

/// Represents different types of applications that can be built or run.
#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
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
#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum Enviornment {
    /// Represents version 1 of the environment configuration.
    V1(Enviornment_V1),
    /// Represents version 2 of the environment configuration.
    /// (Implementation under development, not documented here.)
    V2(Enviornment_V2),
}

impl Enviornment {
    /// Parses raw, encrypted data into either `Enviornment::V1` or `Enviornment::V2`.
    /// 
    /// # Procedure
    /// - Decrypts the provided data using [`simple_decrypt`].
    /// - Reads the first line to determine the version tag (e.g., `#? version:1` or `#? version:2`).
    /// - If `version:1`, deserializes into [`Enviornment_V1`].
    /// - If `version:2`, (currently unimplemented) would deserialize into `Enviornment_V2`.
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
                    log!(LogLevel::Error, "Version 2 not implemented");
                    unimplemented!();
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
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Enviornment_V1 {
    pub application_type:       Option<ApplicationType>,
    pub execution_uid:          Option<u16>,
    pub execution_gid:          Option<u16>,
    pub primary_listening_port: Option<u16>,
    pub secret_id:              Option<Stringy>,
    pub secret_passwd:          Option<Stringy>,
    pub path_modifier:          Option<Stringy>,
    pub pre_build_command:      Option<Stringy>,
    pub build_command:          Option<Stringy>,
    pub run_command:            Option<Stringy>,
    pub env_key_0:              Option<(Stringy, Stringy)>,
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
        json_data.insert_str(0, VERSION_TAG_V1);
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
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Enviornment_V2 {
    //pub application_type:       Option<ApplicationType>, // Application for building
    pub execution_uid:              Option<u16>, // user id to spawn the runner as
    pub execution_gid:              Option<u16>, // group id to spawn the runner as
    pub primary_listening_port:     Option<u16>, // ie: web server listener, api port
    pub secondary_listening_port:   Option<u16>, // ie: web server listener, api port
    pub secret_id:                  Option<Stringy>, // Secret data to pass
    pub secret_passwd:              Option<Stringy>, // Secret data to pass
    pub secret_extra:               Option<Stringy>, // Secret data to pass
    pub path_modifier:              Option<Stringy>, // Data to append the the string path 
    // pub pre_build_command:          Option<Stringy>, // i:e npm install, command to handle depends
    // pub build_command:              Option<Stringy>, // Command to build the project
    // pub env_key_0:                  Option<(Stringy, Stringy)>, // Setting custom env value
    // pub env_key_1:                  Option<(Stringy, Stringy)>, // Setting custom env value
    // pub env_key_2:                  Option<(Stringy, Stringy)>, // Setting custom env value
    // pub env_key_3:                  Option<(Stringy, Stringy)>, // Setting custom env value
    // pub env_key_4:                  Option<(Stringy, Stringy)>, // Setting custom env value
}

impl Enviornment_V2 {
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

    // Returns cipher text of the data
    #[allow(unreachable_code)]
    pub async fn parse(_data: &[u8]) -> Result<Self, ErrorArrayItem> {
        log!(LogLevel::Error, "Version 2 not implemented");
        unimplemented!();
        // let data_bytes = unsafe { clean_override_op(decrypt_data, _data).await? };
        let data_bytes = simple_decrypt(_data)?;
        let data_string = String::from_utf8(data_bytes).map_err(ErrorArrayItem::from)?;
        let data_lines: Vec<&str> = data_string.lines().map(|line| line).collect();
        match data_lines[0] == VERSION_TAG_V2 {
            true => {
                // parse the correct version
                let headerless_data = data_lines[1..].concat();
                let env: Enviornment_V2 =
                    serde_json::from_str(&headerless_data).map_err(ErrorArrayItem::from)?;
                Ok(env)
            }
            false => Err(ErrorArrayItem::new(
                Errors::ConfigParsing,
                format!("Invalid version header: {}", data_lines[0]),
            )),
        }
    }
}

impl fmt::Display for Enviornment_V2 {
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

        let second_port_string = if let Some(port) = self.secondary_listening_port {
            format!("SECOND PORT: {}", port.to_string().bright_cyan())
        } else {
            format!("SECOND PORT: {}", "None".bright_cyan())
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

        let secret_extra_string = if let Some(_) = self.secret_extra {
            format!("SECRET_EXTRA: {}", "Populated".bold().green())
        } else {
            format!("SECRET_EXTRA: {}", "None".bold().green())
        };

        let modifier_string = if let Some(string) = &self.path_modifier {
            format!("PATH: {}", string.bold().purple())
        } else {
            format!("PATH: {}", "None".bold().purple())
        };

        write!(
            f,
            "{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}",
            uid_string,
            gid_string,
            port_string,
            second_port_string,
            secret_id_string,
            secret_passwd_string,
            secret_extra_string,
            modifier_string
        )
    }
}
