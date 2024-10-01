#[cfg(test)]
mod tests {
    use crate::process_manager::ProcessManager;
    use libc::{c_int, SIGCONT, SIGSTOP};
    use std::thread;
    use std::time::{Duration, Instant};

    #[test]
    fn test_spawn_process() {
        // Spawn a "yes" process that runs indefinitely
        let process = ProcessManager::spawn_process("yes", &["test"], false)
            .expect("Failed to spawn process");

        // Assert that the process is running
        let pid = process.id() as c_int;
        assert!(
            ProcessManager::is_process_running(pid),
            "Process should be running"
        );

        // Kill the process after the test
        // ProcessManager::kill_process(pid).expect("Failed to kill process");
        ProcessManager::force_kill_process(pid).expect("Failed to kill");

        // Wait for a short period to ensure the process is killed
        let start_time = Instant::now();
        while ProcessManager::is_process_running(pid)
            && start_time.elapsed() < Duration::from_secs(5)
        {
            thread::sleep(Duration::from_millis(100));
        }

        assert!(
            !ProcessManager::is_process_running(pid),
            "Process should not be running"
        );
    }

    #[test]
    fn test_kill_process() {
        // Spawn a "yes" process
        let process = ProcessManager::spawn_process("yes", &["test"], false)
            .expect("Failed to spawn process");
        let pid = process.id() as c_int;

        // Kill the process
        ProcessManager::kill_process(pid).expect("Failed to kill process");

        // Wait for the process to be killed
        let start_time = Instant::now();
        while ProcessManager::is_process_running(pid)
            && start_time.elapsed() < Duration::from_secs(5)
        {
            thread::sleep(Duration::from_millis(100));
        }

        assert!(
            !ProcessManager::is_process_running(pid),
            "Process should not be running"
        );
    }

    #[test]
    fn test_restart_process() {
        // Spawn a "yes" process
        let process = ProcessManager::spawn_process("yes", &["test"], false)
            .expect("Failed to spawn process");
        let pid = process.id() as c_int;

        // Restart the process
        let new_process = ProcessManager::restart_process(pid, "yes", &["test"])
            .expect("Failed to restart process");

        // Assert that the new process is running and has a different PID
        let new_pid = new_process.id() as c_int;
        assert!(
            ProcessManager::is_process_running(new_pid),
            "New process should be running"
        );
        assert_ne!(pid, new_pid, "PID should be different after restart");

        // Kill the new process after the test
        ProcessManager::kill_process(new_pid).expect("Failed to kill new process");
    }

    #[test]
    fn test_send_signal() {
        // Spawn a "yes" process
        let process = ProcessManager::spawn_process("yes", &["test"], false)
            .expect("Failed to spawn process");
        let pid = process.id() as c_int;

        // Send SIGSTOP to the process
        ProcessManager::send_signal(pid, SIGSTOP).expect("Failed to send SIGSTOP signal");

        // Wait for a short time to let the signal take effect
        thread::sleep(Duration::from_millis(500));

        // Assert that the process is stopped
        let status = ProcessManager::get_process_status(pid).expect("Failed to get process status");
        assert_eq!(status, "Stopped", "Process should be stopped");

        // Send SIGCONT to resume the process
        ProcessManager::send_signal(pid, SIGCONT).expect("Failed to send SIGCONT signal");

        // Assert that the process is running
        thread::sleep(Duration::from_millis(500));
        assert!(
            ProcessManager::is_process_running(pid),
            "Process should be running"
        );

        // Kill the process after the test
        ProcessManager::kill_process(pid).expect("Failed to kill process");
    }

    #[test]
    fn test_stop_process() {
        // Spawn a "yes" process that runs indefinitely
        let process = ProcessManager::spawn_process("yes", &["test"], false)
            .expect("Failed to spawn process");
        let pid = process.id() as c_int;

        // Stop the process gracefully
        ProcessManager::stop_process(pid, 5).expect("Failed to stop process");

        // Wait for a short period to ensure the process is killed
        let start_time = Instant::now();
        while ProcessManager::is_process_running(pid)
            && start_time.elapsed() < Duration::from_secs(5)
        {
            thread::sleep(Duration::from_millis(100));
        }

        assert!(
            !ProcessManager::is_process_running(pid),
            "Process should not be running"
        );
    }
}
