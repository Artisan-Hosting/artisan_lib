#[cfg(test)]
mod tests {
    use crate::aggregator::Status;
    use crate::config::AppConfig;
    use crate::process_manager::{
        spawn_complex_process, spawn_simple_process, ChildLock, SupervisedChild, SupervisedProcess,
    };
    use crate::state_persistence::AppState;
    use crate::timestamp::current_timestamp;

    use dusa_collection_utils::core::errors::Errors;
    use dusa_collection_utils::core::types::pathtype::PathType;
    use dusa_collection_utils::core::version::SoftwareVersion;
    use nix::unistd::Pid;
    use std::path::PathBuf;
    use std::time::Duration;
    use tokio::process::Command;

    #[tokio::test]
    async fn test_supervised_child_spawn_and_kill() {
        // Spawn a simple "sleep 5" child
        let mut cmd = Command::new("sleep");
        cmd.arg("5");

        // Create a supervised child
        let mut supervised_child = SupervisedChild::new(&mut cmd, None)
            .await
            .expect("Failed to spawn supervised child");

        // Verify it is running
        assert!(supervised_child.running().await, "Child should be running");

        // Kill the child
        supervised_child
            .kill()
            .await
            .expect("Failed to kill process");

        // Verify it is no longer running
        assert!(
            !supervised_child.running().await,
            "Child should not be running"
        );
    }

    #[tokio::test]
    async fn test_supervised_child_clone() {
        // Spawn a "sleep 5" child
        let mut cmd = Command::new("sleep");
        cmd.arg("5");
        let mut original = SupervisedChild::new(&mut cmd, None)
            .await
            .expect("Failed to spawn child");

        // Clone it (this terminates the original's monitor before returning)
        let cloned = original.clone().await;

        // Check that the same ResourceMonitorLock references the same PID
        let orig_pid = original.get_pid().await.unwrap();
        let clone_pid = cloned.get_pid().await.unwrap();
        assert_eq!(orig_pid, clone_pid, "Cloned child must have same PID");

        // Clean up
        original.kill().await.expect("Failed to kill process");
    }

    #[tokio::test]
    async fn test_supervised_child_monitor_usage() {
        // Spawn a short sleep so we can grab metrics
        let mut cmd = Command::new("sleep");
        cmd.arg("2");
        let mut supervised_child = SupervisedChild::new(&mut cmd, None)
            .await
            .expect("Failed to spawn supervised child");

        // Start monitoring
        supervised_child.monitor_usage().await;

        // Wait a bit so the monitor can collect data
        tokio::time::sleep(Duration::from_millis(500)).await;

        // Get metrics
        let metrics = supervised_child
            .get_metrics()
            .await
            .expect("Failed to retrieve metrics");
        println!("Metrics from child: {:?}", metrics);

        // Kill child
        supervised_child.kill().await.unwrap();
    }

    #[tokio::test]
    async fn test_supervised_process_with_invalid_pid() {
        // Use a PID that's likely not valid
        let invalid_pid = 999999;
        let result = SupervisedProcess::new(Pid::from_raw(invalid_pid));

        assert!(
            result.is_err(),
            "Expected error when creating SupervisedProcess with invalid PID"
        );

        if let Err(e) = result {
            assert_eq!(e.err_type, Errors::SupervisedChild);
        }
    }

    #[tokio::test]
    async fn test_supervised_process_clone() {
        // We'll use the current process's PID just for demonstration.
        // This might fail if kill(0) on self is disallowed,
        // so adjust if needed, or spawn a separate child first.
        let pid = nix::unistd::getpid();
        let mut proc = SupervisedProcess::new(pid)
            .expect("Could not create SupervisedProcess for current PID");

        // Attempt to clone
        let cloned = proc.clone().await;
        assert_eq!(cloned.get_pid(), proc.get_pid());

        // We won't kill the current process (that would end the test)
        // but we can still confirm the structure works.
    }

    #[tokio::test]
    async fn test_supervised_process_kill() {
        // Spawn a separate process (sleep) to get a valid PID
        let mut cmd = Command::new("sleep");
        cmd.arg("5");
        let child = cmd.spawn().expect("Failed to spawn child for test");
        let pid = child.id().expect("No PID found") as i32;

        // Wrap it in a SupervisedProcess
        let mut sup = SupervisedProcess::new(Pid::from_raw(pid))
            .expect("Failed to create SupervisedProcess from existing PID");

        assert!(sup.active(), "Process should be active");

        // Kill it
        sup.kill().expect("Failed to kill the supervised process");

        assert!(!sup.active(), "Process should not be active after kill");
    }

    #[tokio::test]
    async fn test_child_lock_concurrency() {
        // We'll spawn a child that sleeps
        let mut cmd = Command::new("sleep");
        cmd.arg("5");
        let child = cmd.spawn().expect("Failed to spawn child");
        let lock = ChildLock::new(child);

        // We'll try reading the lock in one task while we call kill in another
        let lock_clone = lock.clone();

        // Task A: read PID
        let handle_a = tokio::spawn(async move {
            let read_guard = lock_clone.0.try_read().await.unwrap();
            read_guard.id().expect("No PID from child")
        });

        // Task B: kill the process
        let handle_b = tokio::spawn(async move {
            lock.kill().await.expect("Failed to kill process from lock");
        });

        // Wait for both
        let (pid_res, kill_res) = tokio::join!(handle_a, handle_b);
        let pid = pid_res.unwrap();
        kill_res.unwrap();

        // Check that the process was definitely killed
        assert!(!ChildLock::running(pid as i32));
    }

    #[tokio::test]
    async fn test_spawn_simple_process_output_capture() {
        // We'll test capturing output. The `echo` command is often present, but if not,
        // replace with something that writes to stdout quickly.
        let mut state = AppState {
            data: String::new(),
            event_counter: 0,
            stared_at: current_timestamp(),
            name: String::new(),
            version: SoftwareVersion::dummy(),
            status: Status::Building,
            pid: 0,
            last_updated: current_timestamp(),
            error_log: Vec::new(),
            config: AppConfig::dummy(),
            system_application: false,
            stderr: Vec::new(),
            stdout: Vec::new(),
        };
        let state_path = PathType::PathBuf(PathBuf::from("/tmp/test_state.json"));

        let mut cmd = Command::new("echo");
        cmd.arg("HelloTest");
        let child = spawn_simple_process(&mut cmd, true, &mut state, &state_path)
            .await
            .expect("Failed to spawn with output capture");

        // Read stdout
        let output = child
            .wait_with_output()
            .await
            .expect("Failed to get child output");

        assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "HelloTest");
        assert!(
            output.stderr.is_empty(),
            "Expected no stderr output from echo"
        );
        assert_eq!(state.data, "Process spawned");
        assert_eq!(state.event_counter, 2);
    }

    #[tokio::test]
    async fn test_spawn_simple_process_inherit() {
        let mut state = AppState {
            data: String::new(),
            event_counter: 0,
            name: String::new(),
            stared_at: current_timestamp(),
            version: SoftwareVersion::dummy(),
            status: Status::Building,
            pid: 0,
            last_updated: current_timestamp(),
            error_log: Vec::new(),
            config: AppConfig::dummy(),
            system_application: false,
            stderr: Vec::new(),
            stdout: Vec::new(),
        };
        let state_path = PathType::PathBuf(PathBuf::from("/tmp/test_state_inherit.json"));

        // This command might just run quickly
        let mut cmd = Command::new("true");
        let mut child = spawn_simple_process(&mut cmd, false, &mut state, &state_path)
            .await
            .expect("Failed to spawn with inherited output");

        // No piped output, we can't read it, but we can verify it didn't fail
        let status = child.wait().await.expect("Failed waiting on child");
        assert!(status.success(), "Process with `true` should exit 0");
        assert_eq!(state.data, "Process spawned");
        assert_eq!(state.event_counter, 2);
    }

    #[tokio::test]
    async fn test_spawn_simple_process_failure() {
        let mut state = AppState {
            data: String::new(),
            event_counter: 0,
            name: String::new(),
            stared_at: current_timestamp(),
            version: SoftwareVersion::dummy(),
            status: Status::Building,
            pid: 0,
            last_updated: current_timestamp(),
            error_log: Vec::new(),
            config: AppConfig::dummy(),
            system_application: false,
            stderr: Vec::new(),
            stdout: Vec::new(),
        };
        let state_path = PathType::PathBuf(PathBuf::from("/tmp/test_state_failure.json"));

        // Use a bogus command that should fail to spawn
        let mut cmd = Command::new("no_such_command_abcdefg");
        let result = spawn_simple_process(&mut cmd, true, &mut state, &state_path).await;
        assert!(
            result.is_err(),
            "Expected spawn failure for invalid command"
        );

        // State should reflect an error was logged
        assert_ne!(state.data, "Process spawned");
        assert_eq!(state.event_counter, 1, "Error path also increments counter");
    }

    #[tokio::test]
    async fn test_spawn_complex_process() {
        // Spawn a complex process that runs in its own process group
        let mut cmd = Command::new("sleep");
        cmd.arg("5");
        let child = spawn_complex_process(
            &mut cmd, None, /* independent_process_group = */ true,
            /* capture_output = */ false,
        )
        .await
        .expect("Failed to spawn complex process");

        // We can check that the child is actually valid
        let pid = child
            .child
            .0
            .try_read()
            .await
            .expect("Failed to lock child")
            .id()
            .expect("No PID from child");
        assert!(ChildLock::running(pid as i32), "Child should be running");

        // Kill it
        child.child.kill().await.expect("Failed to kill child");
        assert!(!ChildLock::running(pid as i32), "Child should be dead");
    }
}
