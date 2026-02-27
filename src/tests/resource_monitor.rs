#[cfg(test)]
mod tests {
    use crate::resource_monitor::ResourceMonitorLock;
    use std::time::Duration;
    use tokio::process::Command;

    #[tokio::test]
    async fn test_resource_monitor_invalid_pid() {
        let result = ResourceMonitorLock::new(999999);
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_resource_monitor_metrics() {
        let mut cmd = Command::new("sleep");
        cmd.arg("1");
        let mut child = cmd.spawn().expect("spawn");
        let pid = child.id().expect("pid") as i32;

        let monitor = ResourceMonitorLock::new(pid).expect("create monitor");
        let metrics = monitor.get_metrics().await.expect("metrics");
        assert!(metrics.memory_usage >= 0.0);
        child.kill().await.expect("kill child");
    }

    #[tokio::test]
    async fn test_resource_monitor_interval_cpu_sampling() {
        let mut cmd = Command::new("sh");
        cmd.arg("-c").arg("while :; do :; done");
        let mut child = cmd.spawn().expect("spawn cpu burner");
        let pid = child.id().expect("pid") as i32;

        let monitor = ResourceMonitorLock::new(pid).expect("create monitor");
        let initial = monitor.get_metrics().await.expect("initial metrics");
        assert_eq!(initial.cpu_usage, 0.0, "first sample is baseline");

        let mut saw_non_zero_cpu = false;
        for _ in 0..5 {
            tokio::time::sleep(Duration::from_millis(120)).await;
            {
                let mut guard = monitor.0.try_write().await.expect("write lock");
                guard.update_state().expect("update monitor state");
            }

            let metrics = monitor.get_metrics().await.expect("updated metrics");
            if metrics.cpu_usage > 0.0 {
                saw_non_zero_cpu = true;
                break;
            }
        }

        assert!(
            saw_non_zero_cpu,
            "expected interval-based cpu usage to become non-zero for busy process"
        );

        child.kill().await.expect("kill child");
    }
}
