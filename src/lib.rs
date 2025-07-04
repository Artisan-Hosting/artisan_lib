// re-exporting dusa-utils
pub use dusa_collection_utils;
use dusa_collection_utils::core::version::VersionCode;

// This is a successor of the artisan_platform
pub mod api;
pub mod aggregator;
pub mod cli;
pub mod config;
pub mod config_bundle;
pub mod control;
pub mod encryption;
pub mod enviornment;
pub mod git_actions;
pub mod historics;
pub mod identity;
#[cfg(target_os = "linux")]
pub mod network;
pub mod notifications;
pub mod portal;
#[cfg(target_os = "linux")]
pub mod process_manager;
#[cfg(target_os = "linux")]
pub mod resource_monitor;
pub mod state_persistence;
#[cfg(target_os = "linux")]
pub mod systemd;
pub mod timestamp;
#[cfg(target_os = "linux")]
pub mod users;
pub mod version;

pub const RELEASEINFO: VersionCode = VersionCode::ReleaseCandidate;

// // tests
#[path = "../src/tests/process_manager.rs"]
mod process_manager_test;
#[path = "../src/tests/encryption.rs"]
mod encryption_test;

#[path = "../src/tests/identity.rs"]
mod identity_test;

#[path = "../src/tests/git_action.rs"]
mod git_action_test;

#[path = "../src/tests/notification.rs"]
mod notification_test;

#[path = "../src/tests/state_persistence.rs"]
mod state_persistence_test;

#[cfg(target_os = "linux")]
#[path = "../src/tests/resource_monitor.rs"]
mod resource_monitor_test;

#[cfg(target_os = "linux")]
#[path = "../src/tests/network.rs"]
mod network_test;
