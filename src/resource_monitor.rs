use dusa_collection_utils::{
    core::errors::{ErrorArrayItem, Errors},
    log,
    core::logger::LogLevel,
    core::types::{rwarc::LockWithTimeout, stringy::Stringy},
};
use gethostname::gethostname;
use procfs::process::{all_processes, Process};
use std::{
    collections::{HashMap, HashSet},
    io::{self, BufRead},
    time::Duration,
};
use sysinfo::System;
use tokio::{task::JoinHandle, time::sleep};

use crate::aggregator::Metrics;

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
    /// - If locking fails (due to timeout or other issue), it logs an error and breaks the loop.
    pub async fn monitor(&self, delay: u64) -> JoinHandle<()> {
        let monitor_lock = self.clone();
        tokio::spawn(async move {
            loop {
                match monitor_lock.0.try_write_with_timeout(None).await {
                    Ok(mut monitor_guard) => {
                        if let Err(e) = monitor_guard.update_state() {
                            log!(LogLevel::Error, "Failed to update monitor state: {}", e);
                            break;
                        }
                    }
                    Err(err) => {
                        log!(LogLevel::Error, "Error locking monitor: {}", err);
                        break;
                    }
                }
                sleep(Duration::from_secs(delay)).await;
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
    /// Most recently measured RAM usage, in megabytes (MB).
    pub ram: f64,
    /// Most recently measured CPU usage, in "jiffies per second" form.
    /// (Can be interpreted as a CPU fraction if scaled properly.)
    pub cpu: f32,
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
        let (cpu, ram) = Self::get_usage(&process)?;
        Ok(ResourceMonitor { pid, ram, cpu })
    }

    /// Updates the stored CPU and RAM usage values by re-reading `/proc/<pid>`.
    ///
    /// # Errors
    /// - Returns an [`ErrorArrayItem`] if the process info cannot be read.  
    ///   If the process has exited, CPU and RAM values are set to 0.
    pub fn update_state(&mut self) -> Result<(), ErrorArrayItem> {
        let process = Process::new(self.pid)
            .map_err(|_| ErrorArrayItem::new(Errors::GeneralError, "Failed to read process"))?;
        let (cpu, ram) = Self::get_usage(&process)?;
        self.cpu = cpu;
        self.ram = ram;
        Ok(())
    }

    /// Retrieves the current CPU and RAM usage for a given [`Process`].
    ///
    /// - **RAM** is computed by taking the resident set size (RSS) from `statm` and converting
    ///   it to MB (`(RSS * 4096) / (1024 * 1024)`).
    /// - **CPU** usage is computed via [`calculate_cpu_usage`].
    ///
    /// # Returns
    /// A tuple `(cpu_usage, memory_usage_mb)`.
    ///
    /// # Errors
    /// - Returns [`ErrorArrayItem`] if the process stat cannot be read.
    fn get_usage(process: &Process) -> Result<(f32, f64), ErrorArrayItem> {
        let stat = process.stat().map_err(|_| {
            ErrorArrayItem::new(Errors::GeneralError, "Failed to retrieve process stat")
        })?;

        // If process is not alive, return zero usage
        if !process.is_alive() {
            return Ok((0.0, 0.0));
        }

        // Convert the resident set size (RSS) to MB
        let memory = process
            .statm()
            .map(|statm| (statm.resident as f64 * 4096.0) / (1024.0 * 1024.0))
            .unwrap_or(0.0);

        let cpu_usage = Self::calculate_cpu_usage(&stat)?;
        Ok((cpu_usage, memory))
    }

    /// Calculates CPU usage of the process based on its kernel ticks (user + system time) and
    /// the system uptime. Checks `/proc/uptime` for total system uptime, and uses process start time
    /// to derive how long the process has been running.
    ///
    /// # Returns
    /// A floating-point representation of CPU usage over its lifetime.  
    /// If the process hasn't yet existed for a full second (or if times are invalid), returns 0.0.
    ///
    /// # Errors
    /// - Returns an [`ErrorArrayItem`] if `/proc/uptime` cannot be read or parsed.
    fn calculate_cpu_usage(stat: &procfs::process::Stat) -> Result<f32, ErrorArrayItem> {
        let total_time = stat.utime + stat.stime + stat.cutime as u64 + stat.cstime as u64;
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
    /// (Sum CPU usage, sum RAM usage, then average CPU usage across all visited PIDs.)
    ///
    /// # Returns
    /// A tuple: `(average_cpu_usage, total_ram_usage)`.
    ///
    /// # Behavior
    /// - Recursively finds child processes, sums CPU and RAM usage.
    /// - A "visited" set is used to prevent counting the same PID multiple times.
    /// - If no PIDs are visited, the average CPU is set to `0.8827` by default (an internal fallback).
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

        let (total_cpu, total_ram) = Self::collect_usage(all_pids)?;

        let average_cpu = match visited.is_empty() {
            true => total_cpu / visited.len() as f32,
            false => 0.0,
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
                if let Ok((cpu, ram)) = Self::get_usage(&process) {
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
