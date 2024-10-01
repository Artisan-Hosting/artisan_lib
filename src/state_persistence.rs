use serde::{Deserialize, Serialize};
use std::fs;

use dusa_collection_utils::{errors::ErrorArrayItem, stringy::Stringy};
use dusa_collection_utils::types::PathType;

use crate::{config::AppConfig, encryption::{decrypt_text, encrypt_text}};

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, PartialOrd, Ord)]
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

pub struct StatePersistence;

impl StatePersistence {
    pub fn save_state(state: &AppState, path: &PathType) -> Result<(), Box<dyn std::error::Error>> {
        let toml_str: Stringy = toml::to_string(state)?.into();
        let state_data = encrypt_text(toml_str)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.err_mesg))?;
        fs::write(path, state_data.to_string())?;
        Ok(())
    }

    pub fn load_state(path: &PathType) -> Result<AppState, Box<dyn std::error::Error>> {
        let encrypted_content: Stringy = fs::read_to_string(path)?.into();

        // You may want to improve this check or handle different encryption formats
        if !encrypted_content.contains("30312d") {
            return Err(Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Invalid encrypted data format",
            )));
        }

        let content: Stringy = decrypt_text(encrypted_content)
            .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidData, "Decryption failed"))?;
        let state: AppState = toml::from_str(&content)?;
        Ok(state)
    }
}
