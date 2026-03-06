use dusa_collection_utils::{core::errors::ErrorArrayItem, core::functions::current_timestamp};

use crate::{
    aggregator::Status,
    config::WorkloadConfig,
    enviornment::definitions::Enviornment,
    state::{RuntimeState, WorkloadSnapshot},
};

impl WorkloadSnapshot {
    pub fn get_name(&self) -> String {
        self.runtime.name.clone()
    }

    pub fn get_status(&self) -> Status {
        self.runtime.status
    }

    pub fn set_status(&mut self, status: Status) {
        self.runtime.status = status
    }

    pub fn get_version(&self) -> dusa_collection_utils::core::version::SoftwareVersion {
        self.runtime.version.clone()
    }

    pub fn get_config(&self) -> WorkloadConfig {
        self.config.clone()
    }

    pub fn get_specfic_config(&self) -> Option<serde_json::Value> {
        self.custom.clone()
    }

    pub fn is_system_application(&self) -> bool {
        self.runtime.system_application
    }

    pub fn get_pid(&self) -> u32 {
        self.runtime.pid
    }

    pub fn set_pid(&mut self, pid: u32) {
        self.runtime.pid = pid
    }

    pub fn get_enviornmentals(&self) -> Option<Enviornment> {
        Some(self.config.enviornment.clone())
    }

    pub fn get_runtime(&self) -> RuntimeState {
        self.runtime.clone()
    }

    pub fn update_runtime(&mut self, runtime: RuntimeState) {
        self.runtime = runtime;
    }

    pub fn clear_errors(&mut self) {
        self.runtime.error_log.clear();
    }

    pub fn no_errors(&self) -> bool {
        self.runtime.error_log.is_empty()
    }

    pub fn update_error_log(&mut self, mut errors: Vec<ErrorArrayItem>, append: bool) {
        if append {
            self.runtime.error_log.append(&mut errors);
        } else {
            self.clear_errors();
            self.runtime.error_log = errors;
        }
    }

    pub fn update_timestamp(&mut self) {
        self.runtime.last_updated = current_timestamp();
    }
}
