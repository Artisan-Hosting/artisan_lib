use dusa_collection_utils::{errors::{ErrorArray, ErrorArrayItem}, functions::current_timestamp, version::SoftwareVersion};
use serde::{Deserialize, Serialize};

use crate::{
    aggregator::Status, config::AppConfig, enviornment::definitions::Enviornment,
    state_persistence::AppState,
};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ApplicationConfig {
    pub state: AppState,
    pub config: AppConfig,
    pub enviornment: Option<Enviornment>,
    pub custom: Option<serde_json::Value>,
}

impl ApplicationConfig {
    pub fn new(state: AppState, enviornment: Option<Enviornment>, custom: Option<serde_json::Value>) -> Self {
        Self {
            config: state.clone().config,
            state,
            enviornment,
            custom,
        }
    }

    pub fn get_name(&self) -> String {
        self.config.app_name.to_string()
    }

    pub fn get_status(&self) -> Status {
        self.state.status
    }

    pub fn set_status(&mut self, status: Status) {
        self.state.status = status
    }

    pub fn get_version(&self) -> SoftwareVersion {
        self.state.version.clone()
    }

    pub fn get_config(&self) -> AppConfig {
        self.config.clone()
    }

    pub fn get_specfic_config(&self) -> Option<serde_json::Value> {
        self.custom.clone()
    }

    pub fn is_system_application(&self) -> bool {
        self.state.system_application
    }

    pub fn get_pid(&self) -> u32 {
        self.state.pid
    }

    pub fn set_pid(&mut self, pid: u32) {
        self.state.pid = pid
    }

    pub fn get_enviornmentals(&self) -> Option<Enviornment> {
        self.enviornment.clone()
    }

    pub fn get_state(&self) -> AppState {
        self.state.clone()
    }

    pub fn update_state(&mut self, state: AppState) {
        self.state = state.clone();
        self.config = state.config;
    }

    pub fn clear_errors(&mut self) {
        self.state.error_log.clear();
    }

    pub fn no_errors(&self) -> bool {
        self.state.error_log.is_empty()
    }

    pub fn update_error_log(&mut self, mut errors: Vec<ErrorArrayItem>, append: bool) {
        match append {
            true => {
                self.state.error_log.append(&mut errors);
            },
            false => {
                self.clear_errors();
                self.state.error_log = errors;
            },
        }
    }

    pub fn update_timestamp(&mut self) {
        self.state.last_updated = current_timestamp()
    }
}
