use dusa_collection_utils::{
    errors::{ErrorArrayItem, Errors}, functions::{create_hash, truncate}, log::LogLevel, stringy::Stringy, types::PathType
};
use dusa_collection_utils::log;
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::{
    io::{Read, Write},
    time::Duration,
};
use tokio::time::sleep;

use crate::{
    encryption::simple_encrypt, timestamp::current_timestamp
};

pub const IDENTITYPATHSTR: &str = "/opt/artisan/identity";
pub const HASH_LENGTH: usize = 28;
pub const CUSTOM_EPOCH: u64 = 1_047_587_400;

pub struct SnowflakeIDGenerator {
    custom_epoch: u64,
    datacenter_id: u8,
    machine_id: u8,
    sequence: u16,
    last_timestamp: u64,
}

impl SnowflakeIDGenerator {
    pub fn new(datacenter_id: u8, machine_id: u8) -> Result<Self, ()> {
        if datacenter_id > 31 {
            log!(LogLevel::Error, "Datacenter ID must be between 0 and 31");
            return Err(());
        }

        if machine_id > 31 {
            log!(LogLevel::Error, "Machine ID must be between 0 and 31");
            return Err(());
        }

        Ok(Self {
            custom_epoch: CUSTOM_EPOCH,
            datacenter_id,
            machine_id,
            sequence: 0,
            last_timestamp: 0,
        })
    }

    fn wait_for_next_millis(last_timestamp: u64) -> u64 {
        let mut timestamp = current_timestamp();
        while timestamp <= last_timestamp {
            timestamp = current_timestamp();
        }
        timestamp
    }

    pub async fn generate_id(&mut self) -> u64 {
        let mut timestamp = current_timestamp();

        if timestamp < self.last_timestamp {
            sleep(Duration::from_millis(10)).await;
            if timestamp < self.last_timestamp {
                log!(
                    LogLevel::Error,
                    "Clock moved backwards. Refusing to generate ID."
                );
                return 0;
            }
        }

        if timestamp == self.last_timestamp {
            self.sequence = (self.sequence + 1) & 0xFFF; // 12 bits max
            if self.sequence == 0 {
                timestamp = Self::wait_for_next_millis(self.last_timestamp);
            }
        } else {
            self.sequence = 0;
        }

        self.last_timestamp = timestamp;

        // Construct the 64-bit ID
        ((timestamp - self.custom_epoch) << 22)
            | ((self.datacenter_id as u64) << 17)
            | ((self.machine_id as u64) << 12)
            | (self.sequence as u64)
    }
    
    // fn make_transit_safe(&self) -> Stringy {

    // }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Identifier {
    pub id: u64,
    _signature: Stringy,
}

impl Identifier {
    fn generate_signature(id: u64) -> Stringy {
        truncate(&*create_hash(format!("{}", id)), HASH_LENGTH)
    }

    pub async fn new() -> Result<Self, ErrorArrayItem> {
        // ! We're using the first 5 out of 31 randomly. incase we need to add order later
        let datacenter_id = rand::thread_rng().gen_range(1..=5);
        let machine_id = rand::thread_rng().gen_range(1..=5);

        let mut big_id: SnowflakeIDGenerator = SnowflakeIDGenerator::new(datacenter_id, machine_id).map_err(
            |_| ErrorArrayItem::new(Errors::GeneralError, "Error generating system id".to_owned()),
        )?;

        let id = big_id.generate_id().await;

        Ok(Self {
            id,
            _signature: Self::generate_signature(id),
        })
    }

    pub async fn verify(&self) -> bool {
        let given_signature = self._signature.clone();
        let new_signature = Self::generate_signature(self.id);
        return match given_signature == new_signature {
            true => true,
            false => false,
        };
    }

    /// loads the identifier from disk, creates a new one if needed
    pub async fn load() -> Result<Option<Self>, ErrorArrayItem> {
        let identifier_path: PathType = PathType::Str(IDENTITYPATHSTR.into());
        if identifier_path.exists() {
            match Self::load_from_file() {
                Ok(data) => return Ok(Some(data)),
                Err(err) => {
                    log!(LogLevel::Trace, "ERROR: Failed to load identy: {}", err);
                    return Ok(None);
                }
            }
        } else {
            return Ok(None);
        }
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
    pub async fn to_encrypted_json(&self) -> Result<Stringy, ErrorArrayItem> {
        let json_representation = self.to_json().map_err(|e| {
            ErrorArrayItem::new(
                dusa_collection_utils::errors::Errors::JsonCreation,
                e.to_string(),
            )
        })?;
        // let encrypted_data = encrypt_text(Stringy::from(json_representation)).await?;
        let encrypted_data = simple_encrypt(json_representation.as_bytes())?;
        Ok(encrypted_data)
    }

    pub fn display_id(&self) {
        log!(LogLevel::Debug, "ID: {}", self.id);
    }

    pub fn display_sig(&self) {
        log!(LogLevel::Debug, "SIG: {}", self._signature);
    }
}
