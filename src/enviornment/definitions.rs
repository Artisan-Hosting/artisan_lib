use core::fmt;

use colored::Colorize;
use dusa_collection_utils::{
    errors::{ErrorArrayItem, Errors},
    stringy::Stringy,
};
use serde::{Deserialize, Serialize};

use crate::encryption::{clean_override_op, decrypt_data, encrypt_data};

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
        match data_lines[0] == "#? version:1" || data_lines[0] == "#? version:2" {
            true => {
                if data_lines[0].contains("1") {
                    let headerless_data = data_lines[1..].concat();
                    let env: Enviornment_V1 =
                        serde_json::from_str(&headerless_data).map_err(ErrorArrayItem::from)?;
                    return Ok(Self::V1(env));
                }
                if data_lines[0].contains("2") {
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
            },
            Enviornment::V2(enviornment_v2) => {
                write!(f, "{}", enviornment_v2)
            },
        }
    }
}

//#? version:1
#[allow(non_camel_case_types)]
#[rustfmt::skip]
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Enviornment_V1 {
    pub execution_uid:          Option<u16>, // user id to spawn the runner as
    pub execution_gid:          Option<u16>, // group id to spawn the runner as
    pub primary_listening_port: Option<u16>, // ie: web server listener, api port
    pub secret_id:              Option<Stringy>, // Secret data to pass
    pub secret_passwd:          Option<Stringy>, // Secret data to pass
    pub path_modifier:          Option<Stringy>, // Data to append the the string path 
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

    // Returns cipher text of the data
    pub async fn parse(data: &[u8]) -> Result<Self, ErrorArrayItem> {
        let data_bytes = unsafe { clean_override_op(decrypt_data, data).await? };
        let data_string = String::from_utf8(data_bytes).map_err(ErrorArrayItem::from)?;
        let data_lines: Vec<&str> = data_string.lines().map(|line| line).collect();
        match data_lines[0] == "#? version:1" {
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

        write!(
            f,
            "{}\n{}\n{}\n{}\n{}\n{}",
            uid_string,
            gid_string,
            port_string,
            secret_id_string,
            secret_passwd_string,
            modifier_string
        )
    }
}

//#? version:2
#[allow(non_camel_case_types)]
#[rustfmt::skip]
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Enviornment_V2 {
    pub execution_uid:              Option<u16>, // user id to spawn the runner as
    pub execution_gid:              Option<u16>, // group id to spawn the runner as
    pub primary_listening_port:     Option<u16>, // ie: web server listener, api port
    pub secondary_listening_port:   Option<u16>, // ie: web server listener, api port
    pub secret_id:                  Option<String>, // Secret data to pass
    pub secret_passwd:              Option<String>, // Secret data to pass
    pub secret_extra:               Option<String>, // Secret data to pass
    pub path_modifier:              Option<String>, // Data to append the the string path 
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
    pub async fn parse(data: &[u8]) -> Result<Self, ErrorArrayItem> {
        let data_bytes = unsafe { clean_override_op(decrypt_data, data).await? };
        let data_string = String::from_utf8(data_bytes).map_err(ErrorArrayItem::from)?;
        let data_lines: Vec<&str> = data_string.lines().map(|line| line).collect();
        match data_lines[0] == "#? version:2" {
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
