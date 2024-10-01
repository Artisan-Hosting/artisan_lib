#[cfg(test)]
mod tests {
    use crate::process_manager::ProcessManager;
    use crate::resource_monitor::ResourceMonitor;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn test_get_usage() {
        // Spawn a "yes" process to consume resources
        let child = ProcessManager::spawn_process("yes", &["test"], false)
            .expect("Failed to spawn process");

        let pid = child.id() as i32;

        // Give the process some time to run and accumulate CPU time
        thread::sleep(Duration::from_secs(2));

        // Get resource usage for the process
        let usage_result = ResourceMonitor::get_usage(pid);
        assert!(usage_result.is_ok(), "Failed to get resource usage");

        let (cpu_usage, memory_usage) = usage_result.unwrap();

        // Memory usage should be non-zero since the process is running
        assert!(memory_usage > 0, "Memory usage should be greater than zero");

        // CPU usage may fluctuate, but should be non-zero since the process is actively running
        assert!(cpu_usage > 0.0, "CPU usage should be greater than zero");

        // Kill the process after the test
        ProcessManager::kill_process(pid).expect("Failed to kill the process");
    }

    #[test]
    fn test_calculate_cpu_usage_zero_for_nonexistent_process() {
        // Attempting to get usage for a non-existent PID should not crash
        let nonexistent_pid = 99999; // This PID is very likely to not exist
        let result = ResourceMonitor::get_usage(nonexistent_pid);

        // Expect an error as the process does not exist
        assert!(
            result.is_err(),
            "Expected an error for non-existent process"
        );
    }
}
