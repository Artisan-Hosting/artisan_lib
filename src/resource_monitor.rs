// resource_monitor.rs
use dusa_collection_utils::{
    errors::{ErrorArrayItem, Errors},
    log,
    log::LogLevel,
    rwarc::LockWithTimeout,
    stringy::Stringy,
};
use gethostname::gethostname;
use procfs::process::{all_processes, Process};
use std::{
    collections::{HashMap, HashSet},
    fs::File,
    io::{self, Read},
    thread,
    time::Duration,
};
use sysinfo::System;

use crate::aggregator::Metrics;

pub struct ResourceMonitorLock(LockWithTimeout<ResourceMonitor>);

impl ResourceMonitorLock {
    pub fn new(pid: i32) -> Result<Self, Box<dyn std::error::Error>> {
        let resource_monitor: ResourceMonitor = ResourceMonitor::new(pid)?;
        let monitor_lock: ResourceMonitorLock =
            ResourceMonitorLock(LockWithTimeout::new(resource_monitor));
        Ok(monitor_lock)
    }

    pub async fn monitor(&self, delay: u64) {
        let new_monitor_lock: ResourceMonitorLock = self.clone();
        tokio::spawn(async move {
            loop {
                let mut monitor_lock = match new_monitor_lock.0.try_write_with_timeout(None).await {
                    Ok(new_monitor) => new_monitor,
                    Err(err) => {
                        log!(LogLevel::Error, "Error locking the child: {}", err);
                        break;
                    }
                };

                let new_process: ResourceMonitor = match ResourceMonitor::new(monitor_lock.pid) {
                    Ok(process) => process,
                    Err(e) => {
                        log!(LogLevel::Error, "Error getting process state: {}", e);
                        break;
                    }
                };

                // Aggregate usage for all processes in the tree
                if let Ok((total_cpu, total_ram)) = new_process.aggregate_tree_usage() {
                    monitor_lock.cpu = total_cpu;
                    monitor_lock.ram = total_ram;
                }

                drop(monitor_lock);
                log!(LogLevel::Trace, "Process monitor updated information");

                thread::sleep(Duration::from_secs(delay));
            }
        });
    }

    pub fn clone(&self) -> Self {
        let data = self;
        let cloned_data = data.0.clone();
        return ResourceMonitorLock(cloned_data);
    }

    pub async fn print_usage(&self) {
        let d0 = self.0.try_read().await.unwrap();
        println!("ram: {}", d0.ram);
        println!("cpu: {}", d0.cpu);
    }

    pub async fn get_metrics(&self) -> Result<Metrics, ErrorArrayItem> {
        let child_data = self.0.try_read().await?;
        Ok(Metrics {
            cpu_usage: child_data.cpu,
            memory_usage: child_data.ram,
            other: None,
        })
    }
}

#[derive(Clone)]
pub struct ResourceMonitor {
    pub pid: i32,
    pub ram: f32,
    pub cpu: f32,
    pub state: procfs::process::Stat,
}

impl ResourceMonitor {
    pub fn new(pid: i32) -> Result<Self, Box<dyn std::error::Error>> {
        let process = Process::new(pid)?;
        let state = process.stat()?;
        let usage = Self::get_usage(process)?;
        let cpu = usage.0;
        let ram = usage.1;
        Ok(ResourceMonitor {
            pid,
            ram,
            cpu,
            state,
        })
    }

    pub fn get_usage(process: Process) -> Result<(f32, f32), Box<dyn std::error::Error>> {
        let stat = process.stat()?;

        // Check if the process still exists
        if !process.is_alive() {
            log!(
                LogLevel::Error,
                "Process PID {} is no longer alive",
                process.pid
            );
            return Ok((0.0, 0.0));
        }

        let raw_memory = process.statm()?.resident as f32;
        log!(
            LogLevel::Trace,
            "Raw memory for PID {}: {}",
            process.pid,
            raw_memory
        );

        let mut memory = raw_memory * 4096.00;
        memory /= 1024.00;
        memory /= 1024.00; // Memory in MB
        log!(
            LogLevel::Trace,
            "Calculated memory for PID {}: {} MB",
            process.pid,
            memory
        );

        let cpu_usage = Self::calculate_cpu_usage(&stat)?;
        Ok((cpu_usage, memory))
    }

    pub fn calculate_cpu_usage(
        stat: &procfs::process::Stat,
    ) -> Result<f32, Box<dyn std::error::Error>> {
        let utime = stat.utime;
        let stime = stat.stime;
        let cutime = stat.cutime;
        let cstime = stat.cstime;
        let total_time = utime + stime + cutime as u64 + cstime as u64;

        let start_time = stat.starttime;

        let mut file = File::open("/proc/uptime")?;
        let mut uptime_str = String::new();
        file.read_to_string(&mut uptime_str)?;
        let uptime: f64 = uptime_str
            .split_whitespace()
            .next()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "Failed to parse uptime"))?
            .parse()?;

        let ticks_per_second = procfs::ticks_per_second() as f64;
        let process_uptime = uptime - (start_time as f64 / ticks_per_second);

        if process_uptime <= 0.0 {
            return Ok(0.0);
        }

        let cpu_usage = (total_time as f64 / ticks_per_second) / process_uptime * 20.0;

        Ok(cpu_usage as f32)
    }

    pub fn get_child_pids(&self) -> Result<Vec<i32>, ErrorArrayItem> {
        let pid_vec = all_processes()
            // .unwrap()
            .map_err(|err| ErrorArrayItem::new(Errors::GeneralError, err.to_string()))?
            // .into_iter()
            .filter_map(|process| {
                let process = process.ok()?;
                if process.stat().ok()?.ppid == self.pid {
                    Some(process.pid)
                } else {
                    None
                }
            })
            .collect();

        Ok(pid_vec)
    }

    pub fn aggregate_tree_usage_recursive(
        pid: i32,
        visited: &mut HashSet<i32>,
    ) -> Result<(f32, f32), ErrorArrayItem> {
        // Check if the PID has already been processed
        if !visited.insert(pid) {
            log!(LogLevel::Trace, "PID {} already visited, skipping...", pid);
            return Ok((0.0, 0.0));
        }

        let mut total_cpu = 0.0;
        let mut total_ram = 0.0;

        // Get the current process
        if let Ok(process) = Process::new(pid) {
            if let Ok((cpu, ram)) = Self::get_usage(process) {
                total_cpu += cpu;
                total_ram += ram;
            }
        } else {
            log!(LogLevel::Error, "Failed to get process info for PID {}", pid);
        }

        // Get all child PIDs of the current process
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

        // Recursively calculate resource usage for child processes
        for child_pid in child_pids {
            let (child_cpu, child_ram) = Self::aggregate_tree_usage_recursive(child_pid, visited)?;
            total_cpu += child_cpu;
            total_ram += child_ram;
        }

        Ok((total_cpu, total_ram))
    }

    pub fn aggregate_tree_usage(&self) -> Result<(f32, f32), ErrorArrayItem> { // cpu, ram, number of children
        let mut visited = HashSet::new();
        let vals = Self::aggregate_tree_usage_recursive(self.pid, &mut visited)?;
        Ok(((vals.0 / visited.len() as f32), vals.1))
    }
}

// ! LEGACY for welcome
pub fn get_system_stats() -> HashMap<Stringy, Stringy> {
    let mut system = System::new_all();
    system.refresh_all();

    let mut stats: HashMap<Stringy, Stringy> = HashMap::new();
    stats.insert(
        Stringy::new("CPU Usage"),
        Stringy::from_string(format!("{:.2}%", system.global_cpu_usage())),
    );
    stats.insert(
        Stringy::new("Total RAM"),
        Stringy::from_string(format!("{} MB", system.total_memory() / 1024)),
    );
    stats.insert(
        Stringy::new("Used RAM"),
        Stringy::from(
            format!("{} MB", system.used_memory() / 1024000)
                .trim_end_matches('0')
                .to_string(),
        ),
    );
    stats.insert(
        Stringy::new("Total Swap"),
        Stringy::from_string(format!("{} MB", system.total_swap() / 1024)),
    );
    stats.insert(
        Stringy::new("Used Swap"),
        Stringy::from_string(
            format!("{} MB", system.used_swap() / 1024000)
                .trim_end_matches('0')
                .to_string(),
        ),
    );
    stats.insert(
        Stringy::new("Hostname"),
        Stringy::from_string(format!("{:?}", gethostname())),
    );

    stats
}
