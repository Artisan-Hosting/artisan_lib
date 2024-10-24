// resource_monitor.rs

use std::{
    collections::HashMap, fs::File, io::{self, Read}, thread, time::Duration
};
use gethostname::gethostname;
use dusa_collection_utils::{rwarc::LockWithTimeout, stringy::Stringy};
use procfs::process::Process;
use sysinfo::System;

use crate::{log, logger::LogLevel};

pub struct ResourceMonitorLock(LockWithTimeout<ResourceMonitor>);

impl ResourceMonitorLock {
    pub fn new(pid: i32) -> Result<Self, Box<dyn std::error::Error>> {
        let resource_monitor: ResourceMonitor = ResourceMonitor::new(pid)?;
        let monitor_lock: ResourceMonitorLock =
            ResourceMonitorLock(LockWithTimeout::new(resource_monitor));
        Ok(monitor_lock)
    }

    pub async fn update_loop(&self, delay: u64) {
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

                monitor_lock.ram = new_process.ram;
                monitor_lock.cpu = new_process.cpu;
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
}

pub struct ResourceMonitor {
    pub pid: i32,
    pub ram: u64,
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

    pub fn get_usage(process: Process) -> Result<(f32, u64), Box<dyn std::error::Error>> {
        let stat = process.stat()?;
        let memory = process.statm()?.resident * 4096; // Memory in bytes
        let cpu_usage = Self::calculate_cpu_usage(&stat)?;
        Ok((cpu_usage, memory))
    }

    pub fn calculate_cpu_usage(
        stat: &procfs::process::Stat,
    ) -> Result<f32, Box<dyn std::error::Error>> {
        // Get the relevant fields from the stat
        let utime = stat.utime; // User mode time
        let stime = stat.stime; // Kernel mode time
        let cutime = stat.cutime; // User mode time of children
        let cstime = stat.cstime; // Kernel mode time of children

        // Total time used by the process (in clock ticks)
        let total_time = utime + stime + cutime as u64 + cstime as u64;

        // Get the elapsed time since the process started
        let start_time = stat.starttime;

        // Read system uptime from /proc/uptime
        let mut file = File::open("/proc/uptime")?;
        let mut uptime_str = String::new();
        file.read_to_string(&mut uptime_str)?;
        let uptime: f64 = uptime_str
            .split_whitespace()
            .next()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "Failed to parse uptime"))?
            .parse()?;

        // Calculate the process uptime in seconds
        let ticks_per_second = procfs::ticks_per_second() as f64;
        let process_uptime = uptime - (start_time as f64 / ticks_per_second);

        // Avoid division by zero
        if process_uptime <= 0.0 {
            return Ok(0.0);
        }

        // Calculate CPU usage percentage
        let cpu_usage = (total_time as f64 / ticks_per_second) / process_uptime * 100.0;

        Ok(cpu_usage as f32)
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
        Stringy::from(format!("{} MB", system.used_memory() / 1024000).trim_end_matches('0').to_string()),
    );
    stats.insert(
        Stringy::new("Total Swap"),
        Stringy::from_string(format!("{} MB", system.total_swap() / 1024)),
    );
    stats.insert(
        Stringy::new("Used Swap"),
        Stringy::from_string(format!("{} MB", system.used_swap() / 1024000).trim_end_matches('0').to_string()),
    );
    stats.insert(Stringy::new("Hostname"), Stringy::from_string(format!("{:?}", gethostname())));

    stats
}