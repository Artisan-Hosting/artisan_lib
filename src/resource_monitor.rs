// resource_monitor.rs

use std::{fs::File, io::{self, Read}};

use procfs::process::Process;

pub struct ResourceMonitor;

impl ResourceMonitor {
    pub fn get_usage(pid: i32) -> Result<(f32, u64), Box<dyn std::error::Error>> {
        let process = Process::new(pid)?;
        let stat = process.stat()?;
        let memory = process.statm()?.resident * 4096; // Memory in bytes
        let cpu_usage = Self::calculate_cpu_usage(&stat)?;
        Ok((cpu_usage, memory))
    }

    pub fn calculate_cpu_usage(stat: &procfs::process::Stat) -> Result<f32, Box<dyn std::error::Error>> {
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