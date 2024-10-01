use libc::{c_int, kill, SIGKILL, SIGTERM};
use nix::sys::wait::waitpid;
use nix::unistd::Pid;
use procfs::process::Process;
use std::process::Child;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use std::{io, thread};

pub struct ProcessManager;

impl ProcessManager {
    pub fn spawn_process(
        command: &str,
        args: &[&str],
        capture_output: bool,
    ) -> Result<Child, io::Error> {
        let mut cmd = Command::new(command);
        cmd.args(args);

        if capture_output {
            cmd.stdout(Stdio::piped());
            cmd.stderr(Stdio::piped());
        } else {
            cmd.stdout(Stdio::null());
            cmd.stderr(Stdio::null());
        }

        let child = cmd.spawn()?;
        Ok(child)
    }

    pub fn kill_process(pid: c_int) -> Result<(), Box<dyn std::error::Error>> {
        // Attempt to gracefully terminate first (send SIGTERM)
        unsafe {
            if kill(pid, SIGTERM) != 0 {
                return Err(io::Error::last_os_error().into());
            }
            Self::reap_zombie_process(pid);
        }

        // Wait for a moment to see if the process terminates
        thread::sleep(Duration::from_secs(1));

        // If still running, force kill the process (send SIGKILL)
        if Self::is_process_running(pid) {
            unsafe {
                if kill(pid, SIGKILL) != 0 {
                    return Err(io::Error::last_os_error().into());
                }
                Self::reap_zombie_process(pid);
            }
        }

        Ok(())
    }

    /// Check if a process is running based on its PID
    pub fn is_process_running(pid: c_int) -> bool {
        unsafe {
            // Sending signal 0 to a process is a way to check if it exists
            // If the process exists and we have permission, kill(pid, 0) will return 0
            // Otherwise, it will return -1 and set errno
            kill(pid, 0) == 0
        }
    }

    pub fn is_process_blocking(pid: i32) -> Result<bool, Box<dyn std::error::Error>> {
        let process = Process::new(pid)?;
        let state = process.stat()?.state();
        match state {
            Ok(procfs::process::ProcState::Running) => Ok(false), // Not blocking
            Ok(procfs::process::ProcState::Sleeping) => Ok(true), // Potentially blocked
            Ok(procfs::process::ProcState::Waiting) => Ok(true),  // Definitely blocking
            _ => Ok(true), // Other states like Stopped, Zombie, etc. could be considered blocked or non-killable
        }
    }

    /// Force kill a process using SIGKILL
    pub fn force_kill_process(pid: c_int) -> Result<(), io::Error> {
        unsafe {
            if kill(pid, SIGKILL) != 0 {
                return Err(io::Error::last_os_error());
            }
        }
        Self::reap_zombie_process(pid);

        Ok(())
    }

    /// Restart a process given a command and its arguments
    pub fn restart_process(
        pid: c_int,
        command: &str,
        args: &[&str],
    ) -> Result<Child, Box<dyn std::error::Error>> {
        // Kill the current process
        Self::kill_process(pid)?;

        // Wait for a moment to ensure the process is terminated
        thread::sleep(Duration::from_secs(1));

        // Start the process again
        let child = Self::spawn_process(command, args, true)?;
        Ok(child)
    }

    /// Send a specific signal to a process
    pub fn send_signal(pid: c_int, signal: c_int) -> Result<(), Box<dyn std::error::Error>> {
        unsafe {
            if kill(pid, signal) != 0 {
                return Err(io::Error::last_os_error().into());
            }
        }
        Ok(())
    }

    /// Get the status of a running process by its PID
    pub fn get_process_status(pid: c_int) -> Result<String, Box<dyn std::error::Error>> {
        let process = procfs::process::Process::new(pid)?;
        let stat = process.stat()?;
        let status = match stat.state() {
            Ok(procfs::process::ProcState::Running) => "Running",
            Ok(procfs::process::ProcState::Sleeping) => "Sleeping",
            Ok(procfs::process::ProcState::Zombie) => "Zombie",
            Ok(procfs::process::ProcState::Stopped) => "Stopped",
            Ok(procfs::process::ProcState::Dead) => "Dead",
            _ => "Unknown",
        };
        Ok(status.to_string())
    }

    /// Gracefully stop a process with a timeout
    pub fn stop_process(pid: c_int, timeout_secs: u64) -> Result<(), Box<dyn std::error::Error>> {
        // Send SIGTERM to the process
        unsafe {
            if kill(pid, SIGTERM) != 0 {
                return Err(io::Error::last_os_error().into());
            }
            Self::reap_zombie_process(pid);
        }

        // Wait for the process to exit, with a timeout
        let start_time = Instant::now();
        while start_time.elapsed() < Duration::from_secs(timeout_secs) {
            if !Self::is_process_running(pid) {
                return Ok(());
            }
            thread::sleep(Duration::from_millis(800));
        }

        // If the process did not exit, send SIGKILL
        unsafe {
            if kill(pid, SIGKILL) != 0 {
                return Err(io::Error::last_os_error().into());
            }
            Self::reap_zombie_process(pid);
        }

        // Wait again to ensure the process is killed
        let start_time = Instant::now();
        while start_time.elapsed() < Duration::from_secs(2) {
            if !Self::is_process_running(pid) {
                return Ok(());
            }
            thread::sleep(Duration::from_millis(100));
        }

        Err("Failed to kill the process after SIGKILL".into())
    }

    /// Ensure that we poll the system to clean up the process if it's a zombie
    fn reap_zombie_process(pid: i32) {
        let _ = waitpid(Pid::from_raw(pid), None);
    }
}
