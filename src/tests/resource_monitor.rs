#[cfg(test)]
mod tests {
    use crate::resource_monitor::ResourceMonitorLock;
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
}
