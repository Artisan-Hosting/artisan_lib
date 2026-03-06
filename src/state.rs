use dusa_collection_utils::{core::errors::ErrorArrayItem, core::version::SoftwareVersion};
use serde::{Deserialize, Serialize};
use std::fmt;

use crate::{aggregator::Status, config::WorkloadConfig, identity::IdentityContext};

/// Runtime-only state for a workload generation.
#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, PartialOrd, Ord, Clone)]
pub struct RuntimeState {
    pub name: String,
    pub version: SoftwareVersion,
    pub data: String,
    pub status: Status,
    pub pid: u32,
    pub last_updated: u64,
    pub started_at: u64,
    pub event_counter: u32,
    pub error_log: Vec<ErrorArrayItem>,
    pub system_application: bool,
    pub stdout: Vec<(u64, String)>,
    pub stderr: Vec<(u64, String)>,
}

impl fmt::Display for RuntimeState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "RuntimeState:")?;
        writeln!(f, "  name: {}", self.name)?;
        writeln!(f, "  status: {}", self.status)?;
        writeln!(f, "  pid: {}", self.pid)?;
        writeln!(f, "  last_updated: {}", self.last_updated)?;
        writeln!(f, "  started_at: {}", self.started_at)?;
        writeln!(f, "  event_counter: {}", self.event_counter)?;
        writeln!(f, "  system_application: {}", self.system_application)?;
        writeln!(f, "  error_count: {}", self.error_log.len())?;
        writeln!(f, "  stdout_lines: {}", self.stdout.len())?;
        writeln!(f, "  stderr_lines: {}", self.stderr.len())?;
        Ok(())
    }
}

/// Snapshot view composed of identity, static config, runtime state, and custom payload.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct WorkloadSnapshot {
    pub identity: IdentityContext,
    pub config: WorkloadConfig,
    pub runtime: RuntimeState,
    pub custom: Option<serde_json::Value>,
}

impl WorkloadSnapshot {
    pub fn new(
        identity: IdentityContext,
        config: WorkloadConfig,
        runtime: RuntimeState,
        custom: Option<serde_json::Value>,
    ) -> Self {
        Self {
            identity,
            config,
            runtime,
            custom,
        }
    }
}
