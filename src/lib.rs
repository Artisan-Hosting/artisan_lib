// This is a successor of the artisan_platform
pub mod process_manager;
pub mod resource_monitor;
pub mod state_persistence;
pub mod notifications;
pub mod config;
pub mod encryption;
pub mod git_actions;

// tests
#[path = "../src/tests/encryption_test.rs"]
mod encryption_test;

#[path ="../src/tests/process_manager_test.rs"]
mod process_manager_test;

#[path ="../src/tests/notification_test.rs"]
mod notification_test;

#[path ="../src/tests/state_persistence_test.rs"]
mod state_persistence_test;

#[path ="../src/tests/resource_monitor_test.rs"]
mod resource_monitor_test;

#[path ="../src/tests/git_action_tests.rs"]
mod git_action_tests;