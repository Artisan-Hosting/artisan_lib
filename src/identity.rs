use dusa_collection_utils::{
    errors::{ErrorArray, ErrorArrayItem},
    functions::{create_hash, truncate},
    stringy::Stringy,
    types::PathType,
};
use serde::{Deserialize, Serialize};
use std::{
    io::{Read, Write},
    net::{IpAddr, Ipv4Addr},
};

use crate::{
    encryption::encrypt_text, git_actions::GitCredentials, identity_test, log, logger::LogLevel, network_communication::get_local_ip, timestamp::{current_timestamp, format_unix_timestamp}
};

pub const IDENTITYPATHSTR: &str = "/usr/local/identity.jais";
pub const HASH_LENGTH: usize = 14;

#[derive(Serialize, Deserialize, Debug)]
pub struct Identifier {
    pub address: Ipv4Addr,
    pub repositories: GitCredentials,
    signature: Stringy,
}

pub struct IdentityInfo {
    /// An abridged version of the 'encrypted'
    pub hash: Stringy,
    pub encrypted: Stringy,
}

impl IdentityInfo {
    pub fn load() -> Result<Self, ErrorArrayItem> {
        let identifier: Identifier = Identifier::load()?;
        let identifier_json: String = identifier.to_json()?;
        let hash: Stringy = Stringy::from_string(truncate(&create_hash(identifier_json.clone()), HASH_LENGTH).to_owned());
        let encrypted: Stringy = encrypt_text(Stringy::from_string(identifier_json.clone()))?;
        Ok(Self {
            hash,
            encrypted,
        })
    }

    pub fn new(identity: Identifier) -> Result<Self, ErrorArrayItem> {
        let encrypted: Stringy = encrypt_text(identity.to_encrypted_json()?)?;
        let hash: Stringy =
            Stringy::Immutable(truncate(&create_hash(encrypted.clone().to_string()), HASH_LENGTH).into());

        Ok(Self { hash, encrypted })
    }
}

impl Identifier {
    /// loads the identifier from disk, creates a new one if needed 
    pub fn load() -> Result<Self, ErrorArrayItem> {
        let identifier_path: PathType = PathType::Str(IDENTITYPATHSTR.into());
        match identifier_path.exists() {
            true => {
                let identifier: Identifier = match Self::load_from_file() {
                    Ok(loaded_data) => loaded_data,
                    Err(err) => {
                        log!(LogLevel::Error, "Error loading identifier: {}, creating new info", err);
                        let new_identifier: Identifier = Self::new()?;
                        new_identifier.save_to_file()?;
                        new_identifier
                    },
                };
                Ok(identifier)
            },
            false => {
                log!(LogLevel::Warn, "Couldn't load identifier creating new one");
                let new_identifier: Identifier = Self::new()?;
                new_identifier.save_to_file()?;
                return Ok(new_identifier)
            },
        }
    }

    /// Creates a new identifier based on IP and timestamp
    pub fn new() -> Result<Self, ErrorArrayItem> {
        let address: Ipv4Addr = get_local_ip();
        let repositories: GitCredentials = match GitCredentials::new(None) {
            Ok(git_credentials) => git_credentials,
            Err(e) => {
                log!(LogLevel::Error, "Couldn't load git credentials");
                log!(LogLevel::Error, "{}", e);
                GitCredentials{
                    auth_items: vec![],
                }
            },
        };
        let signature: Stringy = Stringy::Immutable(
            create_hash(format_unix_timestamp(current_timestamp()).to_string()).into(),
        );
        Ok(Identifier {
            address,
            repositories,
            signature,
        })
    }

    /// Save the identifier to a file
    pub fn save_to_file(&self) -> Result<(), ErrorArrayItem> {
        let serialized_id = serde_json::to_string_pretty(&self)?;
        let mut file = std::fs::File::create(PathType::Str(IDENTITYPATHSTR.into()))?;
        file.write_all(serialized_id.as_bytes())?;
        Ok(())
    }

    /// Load the identifier from a file
    pub fn load_from_file() -> Result<Self, ErrorArrayItem> {
        let mut file = std::fs::File::open(PathType::Str(IDENTITYPATHSTR.into()))?;
        let mut content = String::new();
        file.read_to_string(&mut content)?;
        let identifier: Identifier = serde_json::from_str(&content)?;
        Ok(identifier)
    }

    /// Return a JSON string representation of the Identifier fields
    pub fn to_json(&self) -> Result<String, ErrorArrayItem> {
        let json_representation = serde_json::to_string_pretty(self)?;
        Ok(json_representation)
    }

    /// Return the JSON string and then encrypt it
    pub fn to_encrypted_json(&self) -> Result<Stringy, ErrorArrayItem> {
        let json_representation = self.to_json().map_err(|e| {
            ErrorArrayItem::new(
                dusa_collection_utils::errors::Errors::JsonCreation,
                e.to_string(),
            )
        })?;
        let encrypted_data = encrypt_text(Stringy::from_string(json_representation))?;
        Ok(encrypted_data)
    }
}
