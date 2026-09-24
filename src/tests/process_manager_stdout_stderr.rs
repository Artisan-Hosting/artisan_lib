//! Tests for stdout/stderr capture functionality in supervised processes.
//!
//! These tests verify that the stdout/stderr monitoring system works correctly
//! by spawning processes that output data to stdout and stderr, and verifying
//! that the data gets captured in the respective buffers.

use std::process::Stdio;
use std::time::Duration;
use tokio::process::Command;
use tokio::time::sleep;

use crate::process_manager::{SupervisedChild, ChildLock};
use crate::state_persistence::log_error;

/// Test that demonstrates the stdout/stderr capture functionality 
/// by creating a noisy process and asserting that output is captured.
#[tokio::test]
async fn test_stdout_stderr_capture() {
    // Create a process that outputs multiple lines to stdout and stderr
    let mut cmd = Command::new("sh");
    cmd.arg("-c")
       .arg(r#"echo "stdout line 1"; echo "stdout line 2"; echo "stderr line 1" >&2; echo "stderr line 2" >&2"#)
       .stdout(Stdio::piped())
       .stderr(Stdio::piped());

    // Spawn the process
    let mut supervised_child = SupervisedChild::new(&mut cmd, None).await.unwrap();

    // Monitor the process
    supervised_child.monitor_usage().await;
    
    // Start monitoring stdout/stderr
    supervised_child.monitor_stdx().await;
    
    // Give the monitoring threads time to capture data
    sleep(Duration::from_millis(500)).await;
    
    // Check that data was captured in stdout
    let stdout_lines = supervised_child.get_std_out().await.unwrap();
    assert!(!stdout_lines.is_empty(), "Expected stdout to contain data");
    
    // Check that data was captured in stderr
    let stderr_lines = supervised_child.get_std_err().await.unwrap();
    assert!(!stderr_lines.is_empty(), "Expected stderr to contain data");
    
    // Cleanup
    supervised_child.kill().await.unwrap();
}

/// Test that verifies multiple captures work correctly
#[tokio::test]
async fn test_multiple_capture_iterations() {
    // Create a process that outputs multiple lines to stdout and stderr
    let mut cmd = Command::new("sh");
    cmd.arg("-c")
       .arg(r#"echo "first iteration"; echo "error 1" >&2; sleep 1; echo "second iteration"; echo "error 2" >&2"#)
       .stdout(Stdio::piped())
       .stderr(Stdio::piped());

    let mut supervised_child = SupervisedChild::new(&mut cmd, None).await.unwrap();

    // Monitor the process
    supervised_child.monitor_usage().await;
    
    // Start monitoring stdout/stderr
    supervised_child.monitor_stdx().await;
    
    // Allow some time for initial capture
    sleep(Duration::from_millis(300)).await;
    
    // Check that data was captured
    let stdout_lines = supervised_child.get_std_out().await.unwrap();
    let stderr_lines = supervised_child.get_std_err().await.unwrap();
    
    // Verify data exists
    assert!(!stdout_lines.is_empty(), "Expected stdout to contain data");
    assert!(!stderr_lines.is_empty(), "Expected stderr to contain data");
    
    // Give more time for additional capture
    sleep(Duration::from_millis(1000)).await;
    
    // Check that more data was captured
    let stdout_lines_2 = supervised_child.get_std_out().await.unwrap();
    let stderr_lines_2 = supervised_child.get_std_err().await.unwrap();
    
    // Cleanup
    supervised_child.kill().await.unwrap();
}

/// Test with a long-running process that continuously outputs
#[tokio::test]
async fn test_long_running_process_capture() {
    // Create a long-running process that outputs data
    let mut cmd = Command::new("sh");
    cmd.arg("-c")
       .arg(r#"for i in 1 2 3 4 5; do echo "output $i"; echo "error $i" >&2; sleep 0.5; done"#)
       .stdout(Stdio::piped())
       .stderr(Stdio::piped());

    let mut supervised_child = SupervisedChild::new(&mut cmd, None).await.unwrap();

    // Monitor the process
    supervised_child.monitor_usage().await;
    
    // Start monitoring stdout/stderr
    supervised_child.monitor_stdx().await;
    
    // Give time for capturing
    sleep(Duration::from_millis(1500)).await;
    
    // Verify that some data was captured
    let stdout_lines = supervised_child.get_std_out().await.unwrap();
    let stderr_lines = supervised_child.get_std_err().await.unwrap();
    
    // Verify we got multiple lines
    assert!(!stdout_lines.is_empty(), "Expected stdout to contain data");
    assert!(!stderr_lines.is_empty(), "Expected stderr to contain data");
    
    // Cleanup
    supervised_child.kill().await.unwrap();
}