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

pub const RELEASEINFO: VersionCode = VersionCode::Beta;

// // tests
// #[path = "../src/tests/encryption_test.rs"]
// mod encryption_test;

// #[path = "../src/tests/identity_test.rs"]
// mod identity_test;

#[path = "../src/tests/process_manager.rs"]
mod process_manager_test;

// #[path = "../src/tests/notification_test.rs"]
// mod notification_test;

// #[path = "../src/tests/state_persistence_test.rs"]
// mod state_persistence_test;

// #[path = "../src/tests/resource_monitor_test.rs"]
// mod resource_monitor_test;

// #[path = "../src/tests/git_action_tests.rs"]
// mod git_action_tests;

// #[path = "../src/tests/socket_communication.rs"]
// mod socket_communication_test;

// #[path = "../src/tests/network_communication_test.rs"]
// mod network_communication_test;
