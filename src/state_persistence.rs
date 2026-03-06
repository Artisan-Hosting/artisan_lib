use dusa_collection_utils::core::errors::{ErrorArrayItem, Errors};
use dusa_collection_utils::core::logger::{set_log_level, LogLevel};
use dusa_collection_utils::core::types::pathtype::PathType;
use dusa_collection_utils::core::types::stringy::Stringy;
use dusa_collection_utils::log;
use std::fs;

use crate::aggregator::{Metrics, Status};
use crate::config::WorkloadConfig;
use crate::encryption::{simple_decrypt, simple_encrypt};
use crate::state::{RuntimeState, WorkloadSnapshot};
use crate::timestamp::current_timestamp;

/// Provides utility methods for loading and saving runtime state snapshots from/to disk.
pub struct StatePersistence;

impl StatePersistence {
    /// Derives the default save path for the runtime state using `/opt/artisan/tmp/.<state_name>.state`.
    pub fn get_state_path(state_name: &str) -> PathType {
        PathType::Content(format!("/opt/artisan/tmp/.{}.state", state_name))
    }

    /// Saves the provided [`RuntimeState`] to the specified `path`.
    ///
    /// The data is serialized to TOML and encrypted via [`simple_encrypt`].
    pub async fn save_state(
        state: &RuntimeState,
        path: &PathType,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let toml_str: Stringy = toml::to_string(state)?.into();
        let state_data = simple_encrypt(toml_str.as_bytes()).map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, e.err_mesg.to_string())
        })?;

        fs::write(path, state_data.to_string())?;
        Ok(())
    }

    /// Loads a [`RuntimeState`] from the specified `path`.
    ///
    /// The file is decrypted and deserialized from TOML.
    pub async fn load_state(path: &PathType) -> Result<RuntimeState, Box<dyn std::error::Error>> {
        let encrypted_content: Stringy = fs::read_to_string(path)?.into();
        let content = simple_decrypt(encrypted_content.as_bytes()).map_err(|_| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, "Decryption failed")
        })?;

        let cipher_string = String::from_utf8(content).map_err(|_| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Failed to convert to string",
            )
        })?;

        let state: RuntimeState = toml::from_str(&cipher_string)?;
        Ok(state)
    }

    /// Saves the provided [`WorkloadSnapshot`] to the specified `path`.
    pub async fn save_snapshot(
        snapshot: &WorkloadSnapshot,
        path: &PathType,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let toml_str: Stringy = toml::to_string(snapshot)?.into();
        let state_data = simple_encrypt(toml_str.as_bytes()).map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, e.err_mesg.to_string())
        })?;

        fs::write(path, state_data.to_string())?;
        Ok(())
    }

    /// Loads a [`WorkloadSnapshot`] from the specified `path`.
    pub async fn load_snapshot(
        path: &PathType,
    ) -> Result<WorkloadSnapshot, Box<dyn std::error::Error>> {
        let encrypted_content: Stringy = fs::read_to_string(path)?.into();
        let content = simple_decrypt(encrypted_content.as_bytes()).map_err(|_| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, "Decryption failed")
        })?;

        let cipher_string = String::from_utf8(content).map_err(|_| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Failed to convert to string",
            )
        })?;

        let snapshot: WorkloadSnapshot = toml::from_str(&cipher_string)?;
        Ok(snapshot)
    }
}

/// Updates a [`RuntimeState`] with a new timestamp, increments the event counter, and saves it.
///
/// Behavior is unchanged from the previous state update path.
pub async fn update_state(state: &mut RuntimeState, path: &PathType, _metrics: Option<Metrics>) {
    state.last_updated = current_timestamp();
    state.event_counter += 1;

    if let Err(err) = StatePersistence::save_state(state, path).await {
        log!(LogLevel::Error, "Failed to save state: {}", err);
        state.error_log.push(ErrorArrayItem::new(
            Errors::GeneralError,
            format!("{}", err),
        ));
    }

    log!(LogLevel::Trace, "State Updated");
}

/// Performs final updates to the [`RuntimeState`] before application shutdown.
pub async fn wind_down_state(state: &mut RuntimeState, state_path: &PathType) {
    state.data = String::from("Terminated");
    state.status = Status::Stopping;
    state.error_log.push(ErrorArrayItem::new(
        Errors::GeneralError,
        "Wind down requested - check logs".to_owned(),
    ));
    update_state(state, state_path, None).await;
}

/// Logs an error, sets status to warning, and persists runtime state.
pub async fn log_error(state: &mut RuntimeState, error: ErrorArrayItem, path: &PathType) {
    log!(LogLevel::Error, "{}", error);
    state.error_log.push(error);
    state.status = Status::Warning;
    update_state(state, path, None).await;
}

/// If the current [`WorkloadConfig`] is in debug mode, sets global log level to debug.
pub fn debug_log_set(config: &WorkloadConfig) {
    log!(LogLevel::Trace, "Updating log level");
    if config.debug_mode() {
        set_log_level(LogLevel::Debug);
    }
}
