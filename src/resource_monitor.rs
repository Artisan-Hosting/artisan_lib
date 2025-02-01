use dusa_collection_utils::{
    errors::{ErrorArrayItem, Errors},
    log::LogLevel,
    log,
    rwarc::LockWithTimeout, stringy::Stringy,
};
use procfs::process::{all_processes, Process};
use gethostname::gethostname;
use sysinfo::System;
use std::{
    collections::{HashMap, HashSet}, io::{self, BufRead}, time::Duration
};
use tokio::{task::JoinHandle, time::sleep};

use crate::aggregator::Metrics;

pub struct ResourceMonitorLock(pub LockWithTimeout<ResourceMonitor>);

impl ResourceMonitorLock {
    pub fn new(pid: i32) -> Result<Self, ErrorArrayItem> {
        let resource_monitor = ResourceMonitor::new(pid)?;
        Ok(ResourceMonitorLock(LockWithTimeout::new(resource_monitor)))
    }

    pub async fn monitor(&self, delay: u64) -> JoinHandle<()> {
        let monitor_lock = self.clone();
        tokio::spawn(async move {
            loop {
                match monitor_lock.0.try_write_with_timeout(None).await {
                    Ok(mut monitor_lock) => {
                        if let Err(e) = monitor_lock.update_state() {
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

    pub fn clone(&self) -> Self {
        ResourceMonitorLock(self.0.clone())
    }
}

#[derive(Clone)]
pub struct ResourceMonitor {
    pub pid: i32,
    pub ram: f32,
    pub cpu: f32,
}

impl ResourceMonitor {
    pub fn new(pid: i32) -> Result<Self, ErrorArrayItem> {
        let process = Process::new(pid).map_err(|err| ErrorArrayItem::new(Errors::GeneralError, err.to_string()))?;
        let (cpu, ram) = Self::get_usage(&process)?;
        Ok(ResourceMonitor { pid, ram, cpu })
    }

    pub fn update_state(&mut self) -> Result<(), ErrorArrayItem> {
        let process = Process::new(self.pid)
            .map_err(|_| ErrorArrayItem::new(Errors::GeneralError, "Failed to read process"))?;
        let (cpu, ram) = Self::get_usage(&process)?;
        self.cpu = cpu;
        self.ram = ram;
        Ok(())
    }

    fn get_usage(process: &Process) -> Result<(f32, f32), ErrorArrayItem> {
        let stat = process.stat().map_err(|_| {
            ErrorArrayItem::new(Errors::GeneralError, "Failed to retrieve process stat")
        })?;

        if !process.is_alive() {
            log!(LogLevel::Warn, "Process {} is no longer alive", process.pid);
            return Ok((0.0, 0.0));
        }

        let memory = process
            .statm()
            .map(|statm| (statm.resident as f32 * 4096.0) / (1024.0 * 1024.0))
            .unwrap_or(0.0);

        let cpu_usage = Self::calculate_cpu_usage(&stat)?;
        Ok((cpu_usage, memory))
    }

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
            ErrorArrayItem::new(Errors::GeneralError, format!("Failed to read uptime: {}", e))
        })?;

        let system_uptime = uptime
            .split_whitespace()
            .next()
            .ok_or_else(|| ErrorArrayItem::new(Errors::GeneralError, "Missing uptime data"))?
            .parse::<f64>()
            .map_err(|e| {
                ErrorArrayItem::new(Errors::GeneralError, format!("Invalid uptime format: {}", e))
            })?;

        let process_uptime = system_uptime - (start_time / procfs::ticks_per_second() as f64);
        if process_uptime <= 0.0 {
            return Ok(0.0);
        }

        Ok((total_time as f64 / process_uptime) as f32)
    }

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
            .filter_map(|process| {
                let process = process.ok()?;
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

    pub fn aggregate_tree_usage(&self) -> Result<(f32, f32), ErrorArrayItem> {
        let mut visited = HashSet::new();

        let mut all_pids = Self::collect_all_pids(self.pid, &mut visited)?;
        log!(LogLevel::Trace, "All collected PIDs: {:?}", all_pids);
        all_pids.remove(0);

        let (total_cpu, total_ram) = Self::collect_usage(all_pids)?;

        let average_cpu = if visited.is_empty() {
            0.8827
        } else {
            total_cpu / visited.len() as f32
        };

        Ok((average_cpu, total_ram))
    }

    fn collect_usage(pids: Vec<i32>) -> Result<(f32, f32), ErrorArrayItem> {
        let mut total_cpu = 0.0;
        let mut total_ram = 0.0;

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
                log!(
                    LogLevel::Error,
                    "Failed to get process info for PID {}",
                    pid
                );
            }
        }

        Ok((total_cpu, total_ram))
    }
}

// ! LEGACY for welcome
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
