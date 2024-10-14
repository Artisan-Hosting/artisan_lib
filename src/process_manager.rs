use libc::{c_int, kill, SIGKILL, SIGTERM};
use nix::sys::wait::waitpid;
use nix::unistd::Pid;
use std::process::Stdio;
use std::time::{Duration, Instant};
use std::{io, thread};
use tokio::process::{Child, Command};

use crate::{
    common::{log_error, update_state},
    log,
    logger::LogLevel,
    state_persistence::AppState,
};
use dusa_collection_utils::{errors::ErrorArrayItem, types::PathType};

pub struct ProcessManager;

impl ProcessManager {
    /// Spawn an asynchronous process, similar to the `create_child` logic
    pub async fn spawn_process(
        command: &str,
        args: &[&str],
        capture_output: bool,
        state: &mut AppState,
        state_path: &PathType,
    ) -> Result<Child, io::Error> {
        log!(
            LogLevel::Trace,
            "Spawning process: {} with args: {:?}",
            command,
            args
        );

        let mut cmd = Command::new(command);
        cmd.args(args);

        if capture_output {
            cmd.stdout(Stdio::piped());
            cmd.stderr(Stdio::piped());
        } else {
            cmd.stdout(Stdio::null());
            cmd.stderr(Stdio::null());
        }

        // Spawn child process
        let child = match cmd.spawn() {
            Ok(child_process) => {
                log!(
                    LogLevel::Trace,
                    "Child process spawned successfully: {:?}",
                    child_process
                );
                state.data = String::from("Process spawned");
                state.event_counter += 1;
                update_state(state, state_path);
                Ok(child_process)
            }
            Err(e) => {
                let err_ref = e.get_ref().unwrap();

                log!(
                    LogLevel::Error,
                    "Failed to spawn child process: {}",
                    err_ref
                );
                let error_item: ErrorArrayItem = ErrorArrayItem::new(
                    dusa_collection_utils::errors::Errors::InputOutput,
                    err_ref.to_string(),
                );
                log_error(state, error_item, state_path);
                Err(e)
            }
        };

        child
    }

    pub async fn spawn_complex_process(
        mut command: Command,
        independent_process_group: bool,
        capture_output: bool,
        state: &mut AppState,
        state_path: &PathType,
    ) -> Result<Child, io::Error> {
        log!(LogLevel::Trace, "Child to spawn: {:?}", &command);

        if independent_process_group {
            unsafe {
                command.pre_exec(|| {
                    if libc::setsid() == -1 {
                        return Err(io::Error::last_os_error());
                    }
                    Ok(())
                })
            };
        } else {
            log!(
                LogLevel::Trace,
                "Complex process being spawned in the same CGroup"
            );
        };

        if capture_output {
            command.stdout(Stdio::piped());
            command.stderr(Stdio::piped());
        } else {
            command.stdout(Stdio::null());
            command.stderr(Stdio::null());
        };

        match command.spawn() {
            Ok(child) => {
                log!(
                    LogLevel::Trace,
                    "Child process spawned successfully: {:#?}",
                    child
                );

                state.data = format!("PID: {}", child.id().unwrap_or(99999999));
                state.event_counter += 1;
                update_state(state, state_path);

                Ok(child)
            }
            Err(error) => {
                let error_ref = match error.get_ref() {
                    Some(err_ref) => err_ref,
                    None => unimplemented!(),
                };

                let error_item = ErrorArrayItem::new(
                    dusa_collection_utils::errors::Errors::InputOutput,
                    error_ref.to_string(),
                );

                log!(
                    LogLevel::Error,
                    "Failed to spawn child process: {}",
                    error_ref
                );

                log_error(state, error_item, state_path);

                return Err(error);
            }
        }
    }

    /// Kill a process with SIGTERM, followed by SIGKILL if the process doesn't exit
    pub async fn kill_process(
        pid: c_int,
        state: &mut AppState,
        state_path: &PathType,
    ) -> Result<(), Box<dyn std::error::Error>> {
        log!(
            LogLevel::Trace,
            "Attempting to gracefully terminate process with PID: {}",
            pid
        );

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
            log!(
                LogLevel::Warn,
                "Process with PID: {} did not terminate, sending SIGKILL",
                pid
            );
            unsafe {
                if kill(pid, SIGKILL) != 0 {
                    return Err(io::Error::last_os_error().into());
                }
                Self::reap_zombie_process(pid);
            }
        }

        log!(LogLevel::Trace, "Process with PID: {} terminated", pid);
        state.data = String::from("Process terminated");
        state.event_counter += 1;
        update_state(state, state_path);

        Ok(())
    }

    /// Check if a process is running based on its PID
    pub fn is_process_running(pid: c_int) -> bool {
        unsafe { kill(pid, 0) == 0 }
    }

    /// Gracefully stop a process with a timeout, falling back to SIGKILL if needed
    pub async fn stop_process(
        pid: c_int,
        timeout_secs: u64,
        state: &mut AppState,
        state_path: &PathType,
    ) -> Result<(), Box<dyn std::error::Error>> {
        log!(
            LogLevel::Trace,
            "Stopping process with PID: {} with a timeout of {} seconds",
            pid,
            timeout_secs
        );

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
                log!(LogLevel::Info, "Process with PID: {} has exited", pid);
                return Ok(());
            }
            thread::sleep(Duration::from_millis(800));
        }

        // If the process did not exit, send SIGKILL
        log!(
            LogLevel::Warn,
            "Process with PID: {} did not exit, sending SIGKILL",
            pid
        );
        unsafe {
            if kill(pid, SIGKILL) != 0 {
                return Err(io::Error::last_os_error().into());
            }
            Self::reap_zombie_process(pid);
        }

        log!(
            LogLevel::Info,
            "Process with PID: {} has been forcefully terminated",
            pid
        );
        state.data = String::from("Process forcefully terminated");
        state.event_counter += 1;
        update_state(state, state_path);

        Ok(())
    }

    /// Reap zombie processes to clean up system resources
    fn reap_zombie_process(pid: i32) {
        let _ = waitpid(Pid::from_raw(pid), None);
    }
}
