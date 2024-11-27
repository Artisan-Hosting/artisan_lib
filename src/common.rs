// Frequently used functions
use dusa_collection_utils::{
    errors::{ErrorArrayItem, Errors},
    log,
    log::{set_log_level, LogLevel},
    types::PathType,
};
use tokio::net::UnixStream;

use crate::{
    aggregator::{AppMessage, Status, UpdateApp},
    communication_proto::{send_message, Flags, Proto},
    state_persistence::{AppState, StatePersistence},
    timestamp::current_timestamp,
};

// Update state and persist it to disk
pub async fn update_state(state: &mut AppState, path: &PathType) {
    state.last_updated = current_timestamp();
    state.event_counter += 1;

    // reporting to aggregator
    if let Some(agg) = &state.config.aggregator {
        if PathType::Content(agg.socket_path.clone()).exists() {
            let app_message = AppMessage::Update(UpdateApp {
                app_id: state.config.app_name.clone(),
                error: Some(state.error_log.clone()),
                metrics: None,
                status: Status::Running,
                timestamp: current_timestamp(),
            });

            if let Ok(mut stream) = UnixStream::connect(agg.socket_path.clone()).await {
                if let Ok(message) = send_message::<UnixStream, AppMessage, AppMessage>(&mut stream, Flags::OPTIMIZED, app_message, Proto::UNIX, true).await {

                    match message {
                        Ok(response) => {
                            let payload = response.get_payload().await;
                            match payload {
                                AppMessage::Response(command_response) => {
                                    if command_response.success {
                                        log!(LogLevel::Trace, "State updated with aggregator !");
                                    }
                                },
                                _ => log!(LogLevel::Warn, "Illegal response recieved while reporting status"),
                            }
                        },
                        Err(err) => {
                            log!(LogLevel::Warn, "Updaitng app status with aggregator failed. Recieved {} from server", err);
                        },
                    }
                }
            }
        }
    }

    // saving the state info
    if let Err(err) = StatePersistence::save_state(state, path).await {
        log!(LogLevel::Error, "Failed to save state: {}", err);
        state.is_active = false;
        state.error_log.push(ErrorArrayItem::new(
            Errors::GeneralError,
            format!("{}", err),
        ));
    }
}

// Update the state file in the case of a un handled error
pub async fn wind_down_state(state: &mut AppState, state_path: &PathType) {
    state.is_active = false;
    state.data = String::from("Terminated");
    state.last_updated = current_timestamp();
    state.error_log.push(ErrorArrayItem::new(
        Errors::GeneralError,
        "Wind down requested check logs".to_owned(),
    ));
    update_state(state, &state_path).await;
}

// Log an error and update the state
pub async fn log_error(state: &mut AppState, error: ErrorArrayItem, path: &PathType) {
    log!(LogLevel::Error, "{}", error);
    state.error_log.push(error);
    update_state(state, path).await;
}

// setting the log level to debug
pub fn debug_log_set(state: &AppState) {
    log!(LogLevel::Trace, "Updating log level");
    if state.config.debug_mode {
        set_log_level(LogLevel::Debug);
    }
}
