use core::fmt;

use colored::Colorize;
use dusa_collection_utils::{
    errors::{ErrorArrayItem, Errors}, log, stringy::Stringy, log::LogLevel
};
use serde::{Deserialize, Serialize};

use crate::encryption::{clean_override_op, decrypt_data, encrypt_data};

pub const VERSION_TAG_V1: &str = "#? version:1";
pub const VERSION_TAG_V2: &str = "#? version:2";
pub const VERSION_TAG_V3: &str = "#? version:3";

#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
pub enum ApplicationType {
    Simple,
    Next,
    Angular,
    Python,
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

pub enum Enviornment {
    V1(Enviornment_V1),
    V2(Enviornment_V2),
}

impl Enviornment {
    // Returns cipher text of the data
    pub async fn parse(data: &[u8]) -> Result<Self, ErrorArrayItem> {
        let data_bytes = unsafe { clean_override_op(decrypt_data, data).await? };
        let data_string = String::from_utf8(data_bytes).map_err(ErrorArrayItem::from)?;
        let data_lines: Vec<&str> = data_string.lines().map(|line| line).collect();
        match data_lines[0] == VERSION_TAG_V1 || data_lines[0] == VERSION_TAG_V2 {
            true => {
                if data_lines[0].contains("1") {
                    let headerless_data = data_lines[1..].concat();
                    let env: Enviornment_V1 =
                        serde_json::from_str(&headerless_data).map_err(ErrorArrayItem::from)?;
                    return Ok(Self::V1(env));
                }
                #[allow(unreachable_code)]
                if data_lines[0].contains("2") {
                    log!(LogLevel::Error, "Version 2 not implemented");
                    unimplemented!();
                    let headerless_data = data_lines[1..].concat();
                    let env: Enviornment_V2 =
                        serde_json::from_str(&headerless_data).map_err(ErrorArrayItem::from)?;
                    return Ok(Self::V2(env));
                }
                return Err(ErrorArrayItem::new(
                    Errors::ConfigParsing,
                    format!("Invalid version header: {}", data_lines[0]),
                ));
            }
            false => {
                return Err(ErrorArrayItem::new(
                    Errors::ConfigParsing,
                    format!("Invalid version header: {}", data_lines[0]),
                ))
            }
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

//#? version:1
#[allow(non_camel_case_types)]
#[rustfmt::skip]
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Enviornment_V1 {
    pub application_type:       Option<ApplicationType>, // Application for building
    pub execution_uid:          Option<u16>, // user id to spawn the runner as
    pub execution_gid:          Option<u16>, // group id to spawn the runner as
    pub primary_listening_port: Option<u16>, // ie: web server listener, api port
    pub secret_id:              Option<Stringy>, // Secret data to pass
    pub secret_passwd:          Option<Stringy>, // Secret data to pass
    pub path_modifier:          Option<Stringy>, // Data to append the the string path 
    pub pre_build_command:      Option<Stringy>, // i:e npm install, command to handle depends
    pub build_command:          Option<Stringy>, // Command to build the project
    pub run_command:            Option<Stringy>, // Run command
    pub env_key_0:              Option<(Stringy, Stringy)>, // Setting custom env values
}

impl Enviornment_V1 {
    // Returns cipher text of the data
    pub async fn encrypt(&self) -> Result<Vec<u8>, ErrorArrayItem> {
        let data_json: String = self.to_json()?;
        let data_vec = data_json.as_bytes();
        unsafe { clean_override_op(encrypt_data, data_vec).await }
    }

    // return the json encoded data
    pub fn to_json(&self) -> Result<String, ErrorArrayItem> {
        serde_json::to_string_pretty(&self).map_err(ErrorArrayItem::from)
    }

    // Struct -> File bytes
    pub async fn parse_to(&self) -> Result<Vec<u8>, ErrorArrayItem> {
        let mut json_data: String = self.to_json()?;
        json_data.insert_str(0, VERSION_TAG_V1);
        let bytes: Vec<u8> =
            unsafe { clean_override_op(encrypt_data, json_data.as_bytes()).await? };
        Ok(bytes)
    }

    // Reading the bytes FILE -> Struct
    pub async fn parse_from(data: &[u8]) -> Result<Self, ErrorArrayItem> {
        let data_bytes = unsafe { clean_override_op(decrypt_data, data).await? };
        let data_string = String::from_utf8(data_bytes).map_err(ErrorArrayItem::from)?;
        let data_lines: Vec<&str> = data_string.lines().map(|line| line).collect();
        match data_lines[0] == VERSION_TAG_V1 {
            true => {
                // parse the correct version
                let headerless_data = data_lines[1..].concat();
                let env: Enviornment_V1 =
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

        let app_type = if let Some(string) = &self.application_type {
            format!("APPLICATION: {}", string)
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

//#? version:2
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
        unsafe { clean_override_op(encrypt_data, data_vec).await }
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
        let data_bytes = unsafe { clean_override_op(decrypt_data, _data).await? };
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
