use dusa_collection_utils::errors::Errors;
use dusa_collection_utils::rwarc::LockWithTimeout;
use libc::{c_int, kill, killpg, SIGKILL, SIGTERM};
use nix::sys::wait::waitpid;
use nix::unistd::Pid;
use std::process::Stdio;
use std::time::Duration;
use std::{io, thread};
use tokio::process::{Child, Command};

use crate::resource_monitor::ResourceMonitorLock;
use crate::{
    common::{log_error, update_state},
    log,
    logger::LogLevel,
    state_persistence::AppState,
};
use dusa_collection_utils::{errors::ErrorArrayItem, types::PathType};

pub struct ChildLock(LockWithTimeout<Child>);

pub struct SupervisedChild {
    pub child: ChildLock,
    pub monitor: ResourceMonitorLock,
}

impl SupervisedChild {
    /// Default creates a complex service that captures the std.
    /// This also spawns in its own process group
    pub async fn new(command: Command) -> Result<Self, ErrorArrayItem> {
        spawn_complex_process(command, true, true).await
    }

    pub async fn get_pid(&self) -> Result<u32, ErrorArrayItem> {
        let child_lock = &self.child;
        let child_data = match child_lock.0.try_read().await {
            Ok(cd) => cd,
            Err(e) => {
                log!(LogLevel::Error, "{}", e);
                return Err(e);
            }
        };

        return match child_data.id() {
            Some(xid) => Ok(xid),
            None => Err(io::Error::new(io::ErrorKind::InvalidInput, "Invalid PID").into()),
        };
    }

    pub async fn clone(&self) -> Self {
        let monitor_lock: &ResourceMonitorLock = &self.monitor;
        let child_lock: &ChildLock = &self.child;

        let monitor: ResourceMonitorLock = monitor_lock.clone();
        let child: ChildLock = child_lock.clone();

        Self { child, monitor }
    }

    pub async fn kill(&self) -> Result<(), ErrorArrayItem> {
        self.child.kill().await
    }

    pub async fn running(&self) -> bool {
        let xid = match self.get_pid().await {
            Ok(xid) => xid,
            Err(_) => return false,
        };

        ChildLock::running(xid.try_into().unwrap())
    }

    /// Spawns a endless loop that updates the resource monitor from /proc
    pub async fn monitor_usage(&self) {
        let d0: &ResourceMonitorLock = &self.monitor;
        d0.update_loop(30).await; // 30 secs so most trys with timeouts will work
    }

    // pub async fn check_usage(&self) {
    //     self.monitor.print_usage().await;
    // }
}

impl ChildLock {
    pub fn new(child: Child) -> Self {
        let rw_lock: LockWithTimeout<Child> = LockWithTimeout::new(child);
        Self(rw_lock)
    }

    pub fn update(mut self, new_child: Child) -> Self {
        self.0 = LockWithTimeout::new(new_child);
        return self;
    }

    pub fn clone(&self) -> Self {
        ChildLock(self.0.clone())
    }

    pub async fn kill(&self) -> Result<(), ErrorArrayItem> {
        let child = self.0.try_read().await?;

        let xid = match child.id() {
            Some(xid) => xid,
            None => {
                return Err(ErrorArrayItem::new(
                    dusa_collection_utils::errors::Errors::InputOutput,
                    "No xid provided".to_owned(),
                ))
            }
        };

        // Kill the entire process group
        unsafe { // ! this will halt if the pid assigned is too long
            let pgid = xid; // Since we set pgid to pid in pre_exec
            killpg(pgid as i32, SIGTERM);
            Self::reap_zombie_process(pgid.try_into().unwrap());
        };

        // Wait for a moment to see if the process terminates
        thread::sleep(Duration::from_secs(1));

        // If still running, force kill the process (send SIGKILL)
        if let Ok(xid) = xid.try_into() {
            match Self::running(xid) {
                true => {
                    log!(
                        LogLevel::Warn,
                        "Process with PID: {} did not terminate, sending SIGKILL",
                        xid
                    );
                    unsafe {
                        if kill(xid, SIGKILL) != 0 {
                            return Err(io::Error::last_os_error().into());
                        }
                        Self::reap_zombie_process(xid);
                        log!(LogLevel::Trace, "Process with PID: {} terminated", xid);
                        return Ok(());
                    }
                }
                false => {
                    return Ok(())
                }
            }
        } else {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "Invalid PID").into());
        }
    }

    /// Check if a process is running based on its PID
    pub fn running(pid: c_int) -> bool {
        unsafe { kill(pid, 0) == 0 }
    }

    /// Reap zombie processes to clean up system resources
    fn reap_zombie_process(pid: c_int) {
        let _ = waitpid(Pid::from_raw(pid), None);
    }
}

/// Spawn an asynchronous process, similar to the `create_child` logic
pub async fn spawn_simple_process(
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
    let child: Result<Child, io::Error> = match cmd.spawn() {
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
            // let err_ref = e.get_ref().unwrap();

            log!(
                LogLevel::Error,
                "Failed to spawn child process: {}",
                e.to_string()
            );
            let error_item: ErrorArrayItem = ErrorArrayItem::new(
                dusa_collection_utils::errors::Errors::InputOutput,
                e.to_string(),
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
) -> Result<SupervisedChild, ErrorArrayItem> {
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

            let pid = match child.id() {
                Some(d) => d,
                None => {
                    return Err(ErrorArrayItem::new(
                        Errors::InputOutput,
                        "Couldn't determined if process spawned".to_owned(),
                    ))
                }
            };

            let monitor: ResourceMonitorLock = match ResourceMonitorLock::new(pid as i32) {
                Ok(resource_monitor) => resource_monitor,
                Err(e) => {
                    return Err(ErrorArrayItem::from(io::Error::new(
                        io::ErrorKind::InvalidData,
                        e.to_string(),
                    )))
                }
            };

            //  Creating the rw_lock for the child
            let child: ChildLock = ChildLock::new(child);

            let supervised_child: SupervisedChild = SupervisedChild { child, monitor };

            Ok(supervised_child)
        }
        Err(error) => {
            let error_ref = match error.get_ref() {
                Some(err_ref) => err_ref,
                None => unimplemented!(),
            };

            log!(
                LogLevel::Error,
                "Failed to spawn child process: {}",
                error_ref
            );

            return Err(ErrorArrayItem::from(error));
        }
    }
}
