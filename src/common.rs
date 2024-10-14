// Frequently used functions

use dusa_collection_utils::{
    errors::{ErrorArrayItem, Errors},
    types::PathType,
};

use crate::{
    log,
    logger::{set_log_level, LogLevel},
    state_persistence::{AppState, StatePersistence},
    timestamp::current_timestamp,
};

// Update state and persist it to disk
pub fn update_state(state: &mut AppState, path: &PathType) {
    state.last_updated = current_timestamp();
    if let Err(err) = StatePersistence::save_state(state, path) {
        log!(LogLevel::Error, "Failed to save state: {}", err);
        state.is_active = false;
        state.error_log.push(ErrorArrayItem::new(
            Errors::GeneralError,
            format!("{}", err),
        ));
    }
}

// Log an error and update the state
pub fn log_error(state: &mut AppState, error: ErrorArrayItem, path: &PathType) {
    log!(LogLevel::Error, "{}", error);
    state.error_log.push(error);
    update_state(state, path);
}

// setting the log level to debug
pub fn debug_log_set(state: &AppState) {
    log!(LogLevel::Trace, "Updating log level");
    if state.config.debug_mode {
        set_log_level(LogLevel::Debug);
    }
}