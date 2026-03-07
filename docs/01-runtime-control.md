# Runtime Control and Supervision

## `control` module

### Main structure

- `ToggleControl`: async pause/resume gate for cooperative tasks.

### Key functions

- `ToggleControl::new()`: creates unpaused control.
- `pause()`: marks paused and notifies waiters.
- `resume()`: clears pause and notifies waiters.
- `wait_if_paused().await`: blocks while paused.
- `wait_with_timeout(duration).await`: returns timeout error if still paused.
- `is_paused().await`: reads current paused state.

### Example

```rust,no_run
use std::sync::Arc;
use std::time::Duration;
use artisan_middleware::control::ToggleControl;

#[tokio::main]
async fn main() {
    let gate = Arc::new(ToggleControl::new());
    gate.pause();

    let gate2 = gate.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(1)).await;
        gate2.resume();
    });

    gate.wait_if_paused().await;
}
```

## `process_manager` module (Linux)

### Main structures

- `SupervisedChild`: managed spawned process with resource + stdio monitors.
- `SupervisedProcess`: monitor/kill an already-running PID.
- `ChildLock`: lock wrapper around `tokio::process::Child`.

### Key functions

- `spawn_simple_process(...)`: spawn with optional stdio capture, updates `RuntimeState`.
- `spawn_complex_process(...)`: spawn with optional process-group isolation and monitoring.
- `SupervisedChild::monitor_usage()`, `monitor_stdx()`, `kill()`.
- `SupervisedChild::get_metrics()`, `get_std_out()`, `get_std_err()`.
- `is_pid_active(pid)` helper.

### Example

```rust,no_run
use tokio::process::Command;
use artisan_middleware::process_manager::spawn_complex_process;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::new("sleep");
    cmd.arg("30");

    let mut child = spawn_complex_process(&mut cmd, None, true, true).await?;
    child.monitor_usage().await;
    child.monitor_stdx().await;

    let metrics = child.get_metrics().await?;
    println!("cpu={} mem={}", metrics.cpu_usage, metrics.memory_usage);

    child.kill().await?;
    Ok(())
}
```

## `resource_monitor` module (Linux)

### Main structures

- `MonitorWatchdog` + `MonitorWatchdogSnapshot`: lock-free liveness/health snapshot.
- `ResourceMonitorLock`: lock wrapper around `ResourceMonitor`.
- `ResourceMonitor`: `/proc` based CPU/RAM tracking for a PID.

### Key functions

- `ResourceMonitorLock::new(pid)`.
- `monitor_interval(...)` / `monitor_with_watchdog_interval(...)`.
- `get_metrics().await`.
- `ResourceMonitor::aggregate_tree_usage()`.
- `get_system_stats()` (legacy system snapshot helper).

### Watchdog check example

```rust,no_run
use std::time::Duration;
use artisan_middleware::resource_monitor::MonitorWatchdog;

fn healthy(snapshot: artisan_middleware::resource_monitor::MonitorWatchdogSnapshot) -> bool {
    snapshot.is_valid(Duration::from_secs(5), 3)
}
```
