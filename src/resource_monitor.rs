use dusa_collection_utils::{
    core::errors::{ErrorArrayItem, Errors},
    core::logger::LogLevel,
    core::types::{rwarc::LockWithTimeout, stringy::Stringy},
    log,
};
use gethostname::gethostname;
use procfs::process::{all_processes, Process};
use std::{
    collections::{HashMap, HashSet},
    io::{self, BufRead},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc,
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use sysinfo::System;
use tokio::{task::JoinHandle, time::sleep};

use crate::aggregator::Metrics;

/// Lightweight lock-free watchdog that tracks monitor liveness and recent failures.
#[derive(Clone, Debug)]
pub struct MonitorWatchdog {
    state: Arc<MonitorWatchdogState>,
}

#[derive(Debug)]
struct MonitorWatchdogState {
    running: AtomicBool,
    start_count: AtomicU64,
    last_heartbeat_unix_ms: AtomicU64,
    last_success_unix_ms: AtomicU64,
    last_failure_unix_ms: AtomicU64,
    consecutive_failures: AtomicU64,
}

/// Snapshot view of a [`MonitorWatchdog`] that callers can inspect without touching monitor locks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MonitorWatchdogSnapshot {
    pub running: bool,
    pub start_count: u64,
    pub last_heartbeat_unix_ms: u64,
    pub last_success_unix_ms: u64,
    pub last_failure_unix_ms: u64,
    pub consecutive_failures: u64,
}

impl MonitorWatchdog {
    /// Creates a new watchdog with zeroed counters and `running = false`.
    pub fn new() -> Self {
        Self {
            state: Arc::new(MonitorWatchdogState {
                running: AtomicBool::new(false),
                start_count: AtomicU64::new(0),
                last_heartbeat_unix_ms: AtomicU64::new(0),
                last_success_unix_ms: AtomicU64::new(0),
                last_failure_unix_ms: AtomicU64::new(0),
                consecutive_failures: AtomicU64::new(0),
            }),
        }
    }

    /// Marks a monitor loop as started and bumps the start counter.
    pub fn mark_started(&self) {
        self.state.running.store(true, Ordering::Relaxed);
        self.state.start_count.fetch_add(1, Ordering::Relaxed);
        self.state
            .last_heartbeat_unix_ms
            .store(now_unix_ms(), Ordering::Relaxed);
    }

    /// Marks a monitor loop as stopped.
    pub fn mark_stopped(&self) {
        self.state.running.store(false, Ordering::Relaxed);
        self.state
            .last_heartbeat_unix_ms
            .store(now_unix_ms(), Ordering::Relaxed);
    }

    /// Records a successful monitor iteration and resets consecutive failures.
    pub fn record_success(&self) {
        let now = now_unix_ms();
        self.state.running.store(true, Ordering::Relaxed);
        self.state
            .last_heartbeat_unix_ms
            .store(now, Ordering::Relaxed);
        self.state
            .last_success_unix_ms
            .store(now, Ordering::Relaxed);
        self.state.consecutive_failures.store(0, Ordering::Relaxed);
    }

    /// Records a failed monitor iteration.
    pub fn record_failure(&self) {
        let now = now_unix_ms();
        self.state.running.store(true, Ordering::Relaxed);
        self.state
            .last_heartbeat_unix_ms
            .store(now, Ordering::Relaxed);
        self.state
            .last_failure_unix_ms
            .store(now, Ordering::Relaxed);
        self.state
            .consecutive_failures
            .fetch_add(1, Ordering::Relaxed);
    }

    /// Returns an atomic snapshot of current watchdog state.
    pub fn snapshot(&self) -> MonitorWatchdogSnapshot {
        MonitorWatchdogSnapshot {
            running: self.state.running.load(Ordering::Relaxed),
            start_count: self.state.start_count.load(Ordering::Relaxed),
            last_heartbeat_unix_ms: self.state.last_heartbeat_unix_ms.load(Ordering::Relaxed),
            last_success_unix_ms: self.state.last_success_unix_ms.load(Ordering::Relaxed),
            last_failure_unix_ms: self.state.last_failure_unix_ms.load(Ordering::Relaxed),
            consecutive_failures: self.state.consecutive_failures.load(Ordering::Relaxed),
        }
    }
}

impl MonitorWatchdogSnapshot {
    /// Returns true when the watchdog indicates:
    /// - monitor is running
    /// - heartbeat is newer than `max_staleness`
    /// - failures have not exceeded `max_consecutive_failures`
    pub fn is_valid(&self, max_staleness: Duration, max_consecutive_failures: u64) -> bool {
        if !self.running {
            return false;
        }

        if self.consecutive_failures > max_consecutive_failures {
            return false;
        }

        let age_ms = match now_unix_ms().checked_sub(self.last_heartbeat_unix_ms) {
            Some(age) => age,
            None => return false,
        };
        age_ms <= max_staleness.as_millis() as u64
    }
}

impl Default for MonitorWatchdog {
    fn default() -> Self {
        Self::new()
    }
}

fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// A lock-based wrapper around a [`ResourceMonitor`], providing concurrent access with
/// timeouts. Useful when multiple tasks might try to read/update resource metrics at once.
pub struct ResourceMonitorLock(pub LockWithTimeout<ResourceMonitor>);

impl ResourceMonitorLock {
    /// Creates a new [`ResourceMonitorLock`] from a given process ID (`pid`).
    ///
    /// # Errors
    /// - Returns an [`ErrorArrayItem`] if the underlying [`ResourceMonitor`] fails to initialize
    ///   (e.g., the PID does not exist or `procfs` cannot read process data).
    ///
    /// # Example
    /// ```rust
    /// # use artisan_middleware::resource_monitor::ResourceMonitorLock;
    /// let pid = 1234;
    /// match ResourceMonitorLock::new(pid) {
    ///     Ok(monitor_lock) => {
    ///         // monitor usage, get metrics, etc.
    ///     }
    ///     Err(err) => eprintln!("Failed to create ResourceMonitorLock: {}", err),
    /// }
    /// ```
    pub fn new(pid: i32) -> Result<Self, ErrorArrayItem> {
        let resource_monitor = ResourceMonitor::new(pid)?;
        Ok(ResourceMonitorLock(LockWithTimeout::new(resource_monitor)))
    }

    /// Spawns a background task that periodically updates the resource monitor’s internal
    /// CPU and RAM usage data (by calling `update_state`).
    ///
    /// # Arguments
    /// * `delay` - Interval in seconds between consecutive updates.
    ///
    /// # Return
    /// A [`JoinHandle`] for the spawned task. You can call `handle.abort()` to terminate it.
    ///
    /// # Behavior
    /// - Attempts to acquire a write lock on the monitor every `delay` seconds.
    /// - If locking or sampling fails, it logs and retries on the next interval.
    pub async fn monitor(&self, delay: u64) -> JoinHandle<()> {
        self.monitor_interval(Duration::from_secs(delay)).await
    }

    /// Same as [`monitor`], but accepts a full [`Duration`] for sub-second sampling.
    pub async fn monitor_interval(&self, delay: Duration) -> JoinHandle<()> {
        self.monitor_with_watchdog_interval(delay, None).await
    }

    /// Same as [`monitor`], but also updates a watchdog snapshot for callers that need
    /// out-of-band monitor health checks.
    pub async fn monitor_with_watchdog(
        &self,
        delay: u64,
        watchdog: Option<MonitorWatchdog>,
    ) -> JoinHandle<()> {
        self.monitor_with_watchdog_interval(Duration::from_secs(delay), watchdog)
            .await
    }

    /// Same as [`monitor_with_watchdog`], but accepts a full [`Duration`] for
    /// sub-second sampling.
    pub async fn monitor_with_watchdog_interval(
        &self,
        delay: Duration,
        watchdog: Option<MonitorWatchdog>,
    ) -> JoinHandle<()> {
        let monitor_lock = self.clone();
        tokio::spawn(async move {
            if let Some(watchdog) = &watchdog {
                watchdog.mark_started();
            }

            loop {
                match monitor_lock.0.try_write_with_timeout(None).await {
                    Ok(mut monitor_guard) => {
                        if let Err(e) = monitor_guard.update_state() {
                            log!(LogLevel::Warn, "Failed to update monitor state: {}", e);
                            if let Some(watchdog) = &watchdog {
                                watchdog.record_failure();
                            }
                        } else if let Some(watchdog) = &watchdog {
                            watchdog.record_success();
                        }
                    }
                    Err(err) => {
                        log!(LogLevel::Warn, "Error locking monitor: {}", err);
                        if let Some(watchdog) = &watchdog {
                            watchdog.record_failure();
                        }
                    }
                }
                sleep(delay).await;
            }
        })
    }

    /// Retrieves the current CPU and memory usage metrics from the monitor.  
    /// Returns a [`Metrics`] struct populated with `cpu_usage` and `memory_usage`.
    ///
    /// # Errors
    /// - Returns an [`ErrorArrayItem`] if the read lock cannot be acquired.
    pub async fn get_metrics(&self) -> Result<Metrics, ErrorArrayItem> {
        let monitor = self.0.try_read().await.map_err(|_| {
            ErrorArrayItem::new(
                Errors::LockWithTimeoutRead,
                "Failed to read lock".to_string(),
            )
        })?;
        Ok(Metrics {
            cpu_usage: monitor.cpu,
            memory_usage: monitor.ram,
            other: None,
        })
    }

    /// Creates a new reference to the same underlying [`ResourceMonitor`] via an `Arc`,
    /// retaining the existing lock state.
    pub fn clone(&self) -> Self {
        ResourceMonitorLock(self.0.clone())
    }
}

/// Tracks resource usage (CPU and RAM) for a single process on a Linux system using `/proc`.
#[derive(Clone)]
pub struct ResourceMonitor {
    /// The PID of the process being monitored.
    pub pid: i32,
    /// Most recently measured RAM usage (instantaneous RSS), in megabytes (MB).
    pub ram: f64,
    /// Most recently measured CPU usage as an interval-based percent value.
    pub cpu: f32,
    /// Previous sampled user+system CPU ticks for interval CPU calculations.
    last_total_ticks: Option<u64>,
    /// Wall-clock timestamp for the previous CPU sample.
    last_sample_at: Option<Instant>,
}

impl ResourceMonitor {
    /// Creates a new [`ResourceMonitor`] instance by reading data from `/proc/<pid>`.
    ///
    /// # Arguments
    /// * `pid` - The process ID to be monitored.
    ///
    /// # Errors
    /// - Returns an [`ErrorArrayItem`] if `/proc/<pid>` cannot be read or the process
    ///   does not exist.
    pub fn new(pid: i32) -> Result<Self, ErrorArrayItem> {
        let process = Process::new(pid)
            .map_err(|err| ErrorArrayItem::new(Errors::GeneralError, err.to_string()))?;
        let mut monitor = ResourceMonitor {
            pid,
            ram: 0.0,
            cpu: 0.0,
            last_total_ticks: None,
            last_sample_at: None,
        };
        monitor.apply_sample(&process)?;
        Ok(monitor)
    }

    /// Updates the stored CPU and RAM usage values by re-reading `/proc/<pid>`.
    ///
    /// # Errors
    /// - Returns an [`ErrorArrayItem`] if the process info cannot be read.  
    ///   If the process has exited, CPU and RAM values are set to 0.
    pub fn update_state(&mut self) -> Result<(), ErrorArrayItem> {
        let process = match Process::new(self.pid) {
            Ok(process) => process,
            Err(_) => {
                self.cpu = 0.0;
                self.ram = 0.0;
                self.last_total_ticks = None;
                self.last_sample_at = None;
                return Ok(());
            }
        };

        self.apply_sample(&process)?;
        Ok(())
    }

    /// Reads and stores CPU + RAM usage from a process sample.
    ///
    /// # Errors
    /// - Returns [`ErrorArrayItem`] if the process stat cannot be read.
    fn apply_sample(&mut self, process: &Process) -> Result<(), ErrorArrayItem> {
        // If process is not alive, reset usage values.
        if !process.is_alive() {
            self.cpu = 0.0;
            self.ram = 0.0;
            self.last_total_ticks = None;
            self.last_sample_at = None;
            return Ok(());
        }

        let stat = process.stat().map_err(|_| {
            ErrorArrayItem::new(Errors::GeneralError, "Failed to retrieve process stat")
        })?;
        let total_ticks = Self::cpu_total_ticks(&stat);
        self.cpu = self.calculate_interval_cpu_usage(total_ticks);
        self.ram = Self::memory_usage_mb(process);
        Ok(())
    }

    /// Converts RSS pages to MB using the runtime page size.
    fn memory_usage_mb(process: &Process) -> f64 {
        let page_size = procfs::page_size() as f64;
        let page_size = if page_size > 0.0 { page_size } else { 4096.0 };
        process
            .statm()
            .map(|statm| (statm.resident as f64 * page_size) / (1024.0 * 1024.0))
            .unwrap_or(0.0)
    }

    /// User + system ticks for the target process.
    fn cpu_total_ticks(stat: &procfs::process::Stat) -> u64 {
        stat.utime + stat.stime
    }

    /// Calculates interval-based `%CPU` similar to `top`:
    /// `delta_process_cpu_time / delta_wall_time * 100`.
    ///
    /// The first sample has no prior baseline and returns `0.0`.
    fn calculate_interval_cpu_usage(&mut self, current_total_ticks: u64) -> f32 {
        let now = Instant::now();
        let mut cpu_usage = 0.0;

        if let (Some(last_total), Some(last_sample_at)) =
            (self.last_total_ticks, self.last_sample_at)
        {
            let delta_ticks = current_total_ticks.saturating_sub(last_total);
            let elapsed_seconds = now.saturating_duration_since(last_sample_at).as_secs_f64();
            if elapsed_seconds > 0.0 {
                let delta_cpu_seconds = delta_ticks as f64 / procfs::ticks_per_second() as f64;
                cpu_usage = ((delta_cpu_seconds / elapsed_seconds) * 100.0) as f32;
            }
        }

        self.last_total_ticks = Some(current_total_ticks);
        self.last_sample_at = Some(now);
        cpu_usage.max(0.0)
    }

    /// Retrieves point-in-time CPU and RAM usage for a given [`Process`].
    ///
    /// This is a fallback snapshot calculation (lifetime-averaged CPU) used by
    /// tree aggregation helpers that don't hold per-PID sample history.
    fn get_usage_snapshot(process: &Process) -> Result<(f32, f64), ErrorArrayItem> {
        let stat = process.stat().map_err(|_| {
            ErrorArrayItem::new(Errors::GeneralError, "Failed to retrieve process stat")
        })?;

        if !process.is_alive() {
            return Ok((0.0, 0.0));
        }

        let memory = Self::memory_usage_mb(process);
        let cpu_usage = Self::calculate_lifetime_cpu_usage(&stat)?;
        Ok((cpu_usage, memory))
    }

    /// Lifetime-average CPU usage for a single snapshot.
    ///
    /// Retained for tree-aggregation helpers that are not sampled continuously.
    fn calculate_lifetime_cpu_usage(stat: &procfs::process::Stat) -> Result<f32, ErrorArrayItem> {
        let total_time = Self::cpu_total_ticks(stat) + stat.cutime as u64 + stat.cstime as u64;
        let start_time = stat.starttime as f64;

        let mut uptime = String::new();
        io::BufReader::new(std::fs::File::open("/proc/uptime").map_err(|e| {
            ErrorArrayItem::new(
                Errors::GeneralError,
                format!("Failed to open /proc/uptime: {}", e),
            )
        })?)
        .read_line(&mut uptime)
        .map_err(|e| {
            ErrorArrayItem::new(
                Errors::GeneralError,
                format!("Failed to read uptime: {}", e),
            )
        })?;

        // Parse the system uptime from the first token
        let system_uptime = uptime
            .split_whitespace()
            .next()
            .ok_or_else(|| ErrorArrayItem::new(Errors::GeneralError, "Missing uptime data"))?
            .parse::<f64>()
            .map_err(|e| {
                ErrorArrayItem::new(
                    Errors::GeneralError,
                    format!("Invalid uptime format: {}", e),
                )
            })?;

        let process_uptime = system_uptime - (start_time / procfs::ticks_per_second() as f64);
        if process_uptime <= 0.0 {
            return Ok(0.0);
        }

        // CPU usage is total_time / process_uptime
        Ok((total_time as f64 / process_uptime) as f32)
    }

    /// Recursively collects all PID values in the descendant tree of the given `pid`.
    /// (Finds child processes, then children of children, etc.)
    ///
    /// # Arguments
    /// - `pid`: The root PID to start from.
    /// - `visited`: A [`HashSet`] to track visited PIDs (avoid cycles).
    ///
    /// # Returns
    /// A `Vec<i32>` containing all PIDs in the process subtree.
    ///
    /// # Errors
    /// - Returns an [`ErrorArrayItem`] if enumerating processes via `procfs::process::all_processes`
    ///   fails.
    pub fn collect_all_pids(
        pid: i32,
        visited: &mut HashSet<i32>,
    ) -> Result<Vec<i32>, ErrorArrayItem> {
        if !visited.insert(pid) {
            return Ok(vec![]);
        }

        let mut pids = vec![pid];
        let child_pids = all_processes()
            .map_err(|err| ErrorArrayItem::new(Errors::GeneralError, err.to_string()))?
            .filter_map(|process_result| {
                let process = process_result.ok()?;
                if process.stat().ok()?.ppid == pid {
                    Some(process.pid)
                } else {
                    None
                }
            })
            .collect::<Vec<i32>>();

        for child_pid in child_pids {
            if !visited.contains(&child_pid) {
                pids.extend(Self::collect_all_pids(child_pid, visited)?);
            }
        }

        Ok(pids)
    }

    /// Aggregates CPU and RAM usage across the entire descendant tree of this monitor’s `pid`.
    /// (Sum CPU usage, sum RAM usage, then average CPU usage across collected child PIDs.)
    ///
    /// # Returns
    /// A tuple: `(average_cpu_usage, total_ram_usage)`.
    ///
    /// # Behavior
    /// - Recursively finds child processes, sums CPU and RAM usage.
    /// - A "visited" set is used to prevent counting the same PID multiple times.
    /// - If no child PIDs are found, the average CPU is `0.0`.
    ///
    /// # Errors
    /// - Returns an [`ErrorArrayItem`] if any process info cannot be retrieved.
    pub fn aggregate_tree_usage(&self) -> Result<(f32, f64), ErrorArrayItem> {
        let mut visited = HashSet::new();

        let mut all_pids = Self::collect_all_pids(self.pid, &mut visited)?;
        log!(LogLevel::Trace, "All collected PIDs: {:?}", all_pids);
        // The first element is the root PID itself; remove it before usage calculations
        if !all_pids.is_empty() {
            all_pids.remove(0);
        }
        let pid_count = all_pids.len();

        let (total_cpu, total_ram) = Self::collect_usage(all_pids)?;
        let average_cpu = if pid_count == 0 {
            0.0
        } else {
            total_cpu / pid_count as f32
        };

        Ok((average_cpu, total_ram))
    }

    /// Helper function to sum CPU and RAM usage across multiple process IDs.
    ///
    /// # Returns
    /// `(sum_cpu_usage, sum_ram_usage)`.
    ///
    /// Logs warnings for processes that cannot be read or if `Process::new(pid)` fails.
    fn collect_usage(pids: Vec<i32>) -> Result<(f32, f64), ErrorArrayItem> {
        let mut total_cpu: f32 = 0.0;
        let mut total_ram: f64 = 0.0;

        for pid in pids {
            if let Ok(process) = Process::new(pid) {
                if let Ok((cpu, ram)) = Self::get_usage_snapshot(&process) {
                    total_cpu += cpu;
                    total_ram += ram;
                    log!(
                        LogLevel::Trace,
                        "PID {} - CPU: {}, RAM: {:.4} MB",
                        pid,
                        cpu,
                        ram / 1024.0
                    );
                }
            } else {
                log!(LogLevel::Warn, "Failed to get process info for PID {}", pid);
            }
        }

        Ok((total_cpu, total_ram))
    }
}

/// **LEGACY** function (kept for a welcome screen on login) that retrieves basic
/// system-wide metrics: CPU usage, total/used RAM, total/used Swap, and the hostname.
///
/// # Returns
/// A [`HashMap<Stringy, Stringy>`] with keys such as `"CPU Usage"`, `"Total RAM"`, etc.
pub fn get_system_stats() -> HashMap<Stringy, Stringy> {
    let mut system = System::new_all();
    system.refresh_all();

    let mut stats: HashMap<Stringy, Stringy> = HashMap::new();
    stats.insert(
        Stringy::from("CPU Usage"),
        Stringy::from(format!("{:.2}%", system.global_cpu_usage())),
    );
    stats.insert(
        Stringy::from("Total RAM"),
        Stringy::from(format!("{} MB", system.total_memory() / 1024)),
    );
    stats.insert(
        Stringy::from("Used RAM"),
        Stringy::from(
            format!("{} MB", system.used_memory() / 1024000)
                .trim_end_matches('0')
                .to_string(),
        ),
    );
    stats.insert(
        Stringy::from("Total Swap"),
        Stringy::from(format!("{} MB", system.total_swap() / 1024)),
    );
    stats.insert(
        Stringy::from("Used Swap"),
        Stringy::from(
            format!("{} MB", system.used_swap() / 1024000)
                .trim_end_matches('0')
                .to_string(),
        ),
    );
    stats.insert(
        Stringy::from("Hostname"),
        Stringy::from(format!("{:?}", gethostname())),
    );

    stats
}
