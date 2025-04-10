use std::error::Error;
use std::process::Child;
use std::process::ExitStatus;
use std::{fmt, io};

use dusa_collection_utils::types::stringy::Stringy;
type ID = Stringy;

/// Enum representing the possible statuses of a systemd service.
#[derive(Debug)]
pub enum ServiceStatus {
    Active,
    Inactive,
    Failed,
    Unknown,
}

impl fmt::Display for ServiceStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ServiceStatus::Active => write!(f, "Active"),
            ServiceStatus::Inactive => write!(f, "Inactive"),
            ServiceStatus::Failed => write!(f, "Failed"),
            ServiceStatus::Unknown => write!(f, "Unknown"),
        }
    }
}

/// Wrapper struct for a systemd service, providing control functions.
#[derive(Debug, Clone)]
pub struct SystemdService {
    service_name: ID,
}

impl SystemdService {
    /// Creates a new `SystemdService` instance with the specified service name.
    pub fn new(service_name: &str) -> io::Result<Self> {
        match systemctl::exists(&format!("{}.service", service_name))? {
            true => Ok(Self {
                service_name: Stringy::Immutable(service_name.into()),
            }),
            false => {
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    format! {"{} not found", service_name},
                ))
            }
        }
    }

    /// Start the service.
    pub fn start(&self) -> Result<(), Box<dyn Error>> {
        systemctl::start(&format!("{}.service", &self.service_name))?;
        Ok(())
    }

    /// Stop the service.
    pub fn stop(&self) -> Result<(), Box<dyn Error>> {
        systemctl::stop(&format!("{}.service", &self.service_name))?;
        Ok(())
    }

    /// kills a service and its children
    pub fn kill(&self) -> Result<(), Box<dyn Error>> {
        systemctl(["kill", &format!("{}.service", self.service_name)].to_vec())?;
        Ok(())
    }

    /// Restart the service.
    pub fn restart(&self) -> Result<(), Box<dyn Error>> {
        systemctl::reload_or_restart(&format!("{}.service", &self.service_name))?;
        Ok(())
    }

    /// Check if the service is active.
    pub fn is_active(&self) -> Result<bool, Box<dyn Error>> {
        Ok(systemctl::is_active(&format!(
            "{}.service",
            &self.service_name
        ))?)
    }
}

// Just ripped out of a crate
const SYSTEMCTL_PATH: &str = "/usr/bin/systemctl";

/// Invokes `systemctl $args`
fn spawn_child(args: Vec<&str>) -> std::io::Result<Child> {
    std::process::Command::new(std::env::var("SYSTEMCTL_PATH").unwrap_or(SYSTEMCTL_PATH.into()))
        .args(args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
}

/// Invokes `systemctl $args` silently
fn systemctl(args: Vec<&str>) -> std::io::Result<ExitStatus> {
    spawn_child(args)?.wait()
}
