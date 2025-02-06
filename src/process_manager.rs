use dusa_collection_utils::errors::Errors;
use dusa_collection_utils::log;
use dusa_collection_utils::log::LogLevel;
use dusa_collection_utils::rwarc::LockWithTimeout;
use libc::{c_int, kill, killpg, SIGKILL, SIGTERM};
use nix::sys::wait::waitpid;
use nix::unistd::Pid;
use std::process::Stdio;
use std::time::Duration;
use std::{io, thread};
use tokio::process::{Child, Command};
use tokio::task::JoinHandle;

use crate::aggregator::Metrics;
use crate::resource_monitor::ResourceMonitorLock;
use crate::state_persistence::{log_error, update_state, AppState};
use dusa_collection_utils::{errors::ErrorArrayItem, types::PathType};

/// A wrapper around [`LockWithTimeout<Child>`] that synchronizes access to a
/// [`tokio::process::Child`]. This allows safe concurrent reads/writes or attempts to kill
/// the child within specified timeouts.
pub struct ChildLock(pub LockWithTimeout<Child>);

/// Holds a [`ChildLock`] plus a resource monitor and an optional handle to a background
/// monitoring task. This structure is used to manage a spawned child process in an
/// asynchronous context (Tokio).
///
/// - The `monitor_handle` can be used to stop the resource monitor loop if needed.
/// - The resource monitor tracks CPU/memory usage via `/proc` (Linux-specific).
pub struct SupervisedChild {
    /// The locked child process.
    pub child: ChildLock,
    /// A lock-based resource monitor for CPU/memory usage, etc.
    pub monitor: ResourceMonitorLock,
    /// An optional background task handle for continuous resource monitoring.
    monitor_handle: Option<JoinHandle<()>>,
}

/// Represents a supervised process that may not have been spawned via [`tokio::process::Command`]
/// but is still tracked by a PID. Similar to `SupervisedChild`, but manages an existing
/// process rather than a newly spawned one.
pub struct SupervisedProcess {
    /// The process ID (PID) of the target process.
    pid: Pid,
    /// A resource monitor tracking CPU/memory usage.
    pub monitor: ResourceMonitorLock,
    /// An optional background task handle for continuous resource monitoring.
    monitor_handle: Option<JoinHandle<()>>,
}

impl SupervisedProcess {
    /// Creates a new `SupervisedProcess` from an existing PID. This checks if the PID is active
    /// (via `kill(pid, 0)`). If active, it initializes a resource monitor on that PID.
    ///
    /// # Errors
    /// - Returns an [`ErrorArrayItem`] if the PID is not active or if the resource monitor
    ///   fails to initialize.
    ///
    /// # Safety / Platform
    /// - **Linux-specific**: The `kill` check and `/proc` monitoring assume a Linux-like environment.
    /// - Using `kill(pid, 0)` is a non-destructive check that returns 0 if the process exists,
    ///   and `-1` if it doesn’t or if permissions are lacking.
    pub fn new(pid: Pid) -> Result<Self, ErrorArrayItem> {
        // Ensure pid is active by sending signal 0
        let active: bool = unsafe { kill(pid.as_raw(), 0) == 0 };

        let supervised_process: Option<SupervisedProcess> = if active {
            Some(SupervisedProcess {
                pid,
                monitor: ResourceMonitorLock::new(pid.as_raw())
                    .map_err(|err| ErrorArrayItem::new(Errors::GeneralError, err.to_string()))?,
                monitor_handle: None,
            })
        } else {
            None
        };

        match supervised_process {
            Some(sup) => Ok(sup),
            None => Err(ErrorArrayItem::new(
                Errors::SupervisedChild,
                format!(
                    "Failed to create SupervisedProcess; cannot determine status of PID: {}",
                    pid
                ),
            )),
        }
    }

    /// Returns the raw PID of this process.
    pub fn get_pid(&self) -> i32 {
        self.pid.as_raw()
    }

    /// Terminates the monitored process by:
    /// 1. Stopping any monitoring task.
    /// 2. Sending a `SIGTERM` to the entire process group (PGID == PID).
    /// 3. Reaping zombies (via `waitpid`) if the process immediately exits.
    /// 4. If still running after 400ms, sending `SIGKILL`.
    ///
    /// # Errors
    /// - Returns an I/O error if `kill` or `killpg` fails.
    /// - Also returns an error if the process cannot be reaped properly.
    ///
    /// # Why Reap Zombies?
    /// - In Linux, a process that has terminated but whose parent hasn't called `wait*()` is
    ///   marked as a "zombie." Reaping zombies avoids accumulation of defunct processes,
    ///   freeing kernel resources.
    pub fn kill(&mut self) -> Result<(), ErrorArrayItem> {
        self.terminate_monitor();
        let xid = self.pid.as_raw();

        // Kill the entire process group
        unsafe {
            // On Linux, setting PGID to the PID in `pre_exec()` ensures all child threads
            // belong to the same group. This allows group signals.
            let pgid = xid;
            killpg(pgid, SIGTERM);
            Self::reap_zombie_process(pgid.try_into().unwrap());
        };

        // Wait briefly for normal termination
        thread::sleep(Duration::from_millis(400));

        // Force kill if still running
        if Self::running(xid) {
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
                Ok(())
            }
        } else {
            Ok(())
        }
    }

    /// Returns `true` if the process is still active (PID exists), or `false` otherwise.
    pub fn active(&self) -> bool {
        Self::running(self.pid.as_raw())
    }

    /// Checks if a PID is running by sending signal 0.
    pub fn running(pid: c_int) -> bool {
        unsafe { kill(pid, 0) == 0 }
    }

    /// Reap (wait for) a zombie process, if any, to clean up system resources.
    ///
    /// # Note
    /// - This function calls [`nix::sys::wait::waitpid`] with no options.
    /// - If there's no zombie, it returns immediately without error.
    fn reap_zombie_process(pid: c_int) {
        let _ = waitpid(Pid::from_raw(pid), None);
    }

    /// Clones this `SupervisedProcess`, returning a new instance without a running monitor.
    /// The existing monitor is terminated before cloning.
    pub async fn clone(&mut self) -> Self {
        self.terminate_monitor();
        let monitor_lock: ResourceMonitorLock = self.monitor.clone();

        Self {
            pid: self.pid,
            monitor: monitor_lock,
            monitor_handle: None,
        }
    }

    /// Spawns an asynchronous resource monitoring loop that periodically queries
    /// `/proc/<pid>` for CPU/memory usage. The period is set to 2 seconds to reduce overhead
    /// and align with potential timeouts.
    ///
    /// # Note
    /// - This loop is attached to `monitor_handle`. Re-run this method only if `monitor_handle`
    ///   is `None`, to avoid multiple concurrent monitors on the same process.
    pub async fn monitor_usage(&mut self) {
        if self.monitor_handle.is_none() {
            let d0: &ResourceMonitorLock = &self.monitor.clone();
            let handle: JoinHandle<()> = d0.monitor(2).await; // 2-second interval
            self.monitor_handle = Some(handle)
        }
    }

    /// Terminates the resource monitor task, if any.
    ///
    /// # Note
    /// - Uses [`JoinHandle::abort()`] to stop the task immediately.
    pub fn terminate_monitor(&mut self) {
        if let Some(handle) = &self.monitor_handle {
            log!(LogLevel::Trace, "Terminating monitor");
            handle.abort();
            self.monitor_handle = None;
        }
    }

    /// Fetches resource usage metrics (CPU, memory, etc.) from the process-specific resource monitor.
    ///
    /// # Errors
    /// - Returns an [`ErrorArrayItem`] if the resource monitor fails to read from `/proc` or
    ///   if the process does not exist anymore.
    pub async fn get_metrics(&self) -> Result<Metrics, ErrorArrayItem> {
        self.monitor.get_metrics().await
    }
}

impl SupervisedChild {
    /// Spawns a new child process with its own process group and optionally captures stdout/stderr.
    /// The resulting process is wrapped in a [`SupervisedChild`] which provides:
    /// - Locking for the child handle
    /// - A resource monitor
    /// - Optional background monitoring
    ///
    /// # Behavior
    /// - Uses [`spawn_complex_process`] under the hood.
    /// - `true` for capturing output means the child's output is piped rather than inherited.
    /// - `true` for `independent_process_group` means it calls `setsid()` in `pre_exec` on Linux,
    ///   so the child won't receive signals from the parent TTY group directly.
    ///
    /// # Errors
    /// - Returns an [`ErrorArrayItem`] if spawning fails or if resource monitoring fails to initialize.
    pub async fn new(command: &mut Command) -> Result<Self, ErrorArrayItem> {
        let child = spawn_complex_process(command, None, true, true).await?;
        Ok(Self {
            child: child.child,
            monitor: child.monitor,
            monitor_handle: child.monitor_handle,
        })
    }

    /// Returns the process ID (`PID`) of the child, if available. If locked, tries for a
    /// read-lock on the child. If no PID is found, an error is returned.
    ///
    /// # Errors
    /// - Returns [`ErrorArrayItem`] if read-lock fails or the PID is invalid.
    pub async fn get_pid(&self) -> Result<u32, ErrorArrayItem> {
        let child_lock = &self.child;
        let child_data = child_lock.0.try_read().await?;
        match child_data.id() {
            Some(xid) => Ok(xid),
            None => Err(io::Error::new(io::ErrorKind::InvalidInput, "Invalid PID").into()),
        }
    }

    /// Clones this `SupervisedChild` without a running monitor. Terminates the existing monitor,
    /// then duplicates the resource monitor and child lock.
    pub async fn clone(&mut self) -> Self {
        self.terminate_monitor();
        let monitor_lock: ResourceMonitorLock = self.monitor.clone();
        let child_lock: ChildLock = self.child.clone();

        Self {
            child: child_lock,
            monitor: monitor_lock,
            monitor_handle: None,
        }
    }

    /// Sends a `SIGTERM` to the child’s process group. After a short wait, if the process is still
    /// running, sends `SIGKILL`. Also reaps any zombie.
    ///
    /// # Errors
    /// - Returns an [`ErrorArrayItem`] on I/O issues or if reaping fails.
    pub async fn kill(&mut self) -> Result<(), ErrorArrayItem> {
        self.terminate_monitor();
        self.child.kill().await
    }

    /// Checks if the child process is still running by retrieving its PID and sending signal 0.
    pub async fn running(&self) -> bool {
        let xid = match self.get_pid().await {
            Ok(xid) => xid,
            Err(_) => return false,
        };

        ChildLock::running(xid as c_int)
    }

    /// Spawns an asynchronous resource monitoring loop for this child. If a monitor is
    /// already running, this does nothing.
    ///
    /// # Behavior
    /// - Queries `/proc/<pid>` for CPU, memory, etc. every 2 seconds.
    /// - Use [`terminate_monitor`] to stop the task.
    pub async fn monitor_usage(&mut self) {
        if self.monitor_handle.is_none() {
            let d0: &ResourceMonitorLock = &self.clone().await.monitor;
            let handle: JoinHandle<()> = d0.monitor(2).await;
            self.monitor_handle = Some(handle)
        }
    }

    /// Terminates the resource monitor task, if any is currently running. This calls
    /// [`JoinHandle::abort()`] on the stored handle.
    pub fn terminate_monitor(&mut self) {
        if let Some(handle) = &self.monitor_handle {
            log!(LogLevel::Trace, "Terminating monitor");
            handle.abort();
            self.monitor_handle = None;
        }
    }

    /// Retrieves the current resource usage metrics from `/proc`.  
    /// Returns an error if the process has exited or if `/proc` parsing fails.
    pub async fn get_metrics(&self) -> Result<Metrics, ErrorArrayItem> {
        self.monitor.get_metrics().await
    }
}

impl ChildLock {
    /// Wraps a [`Child`] in a [`LockWithTimeout`], allowing timed read/write locks on the
    /// child handle.
    pub fn new(child: Child) -> Self {
        let rw_lock: LockWithTimeout<Child> = LockWithTimeout::new(child);
        Self(rw_lock)
    }

    /// Replaces the child handle within this lock. Typically used when restarting or
    /// re-spawning the same command.
    pub fn update(mut self, new_child: Child) -> Self {
        self.0 = LockWithTimeout::new(new_child);
        self
    }

    /// Clones the internal lock (i.e., `Arc`-based duplication). This does not duplicate
    /// the child process, only the lock mechanism that references it.
    pub fn clone(&self) -> Self {
        let child = &self.0;
        let lock_clone = child.clone();
        ChildLock { 0: lock_clone }
    }

    /// Sends a `SIGTERM` to the child’s process group, waits briefly, and
    /// sends `SIGKILL` if still running. Also reaps zombies by calling `waitpid`.
    ///
    /// # Errors
    /// - Returns an [`ErrorArrayItem`] on I/O issues or if reaping fails.
    /// - If the child’s PID is invalid, returns an error.
    pub async fn kill(&self) -> Result<(), ErrorArrayItem> {
        let child = self.0.try_read().await?;
        let xid = match child.id() {
            Some(xid) => xid,
            None => {
                return Err(ErrorArrayItem::new(
                    dusa_collection_utils::errors::Errors::InputOutput,
                    "No PID found in child process".to_owned(),
                ))
            }
        };

        // Kill the entire process group
        unsafe {
            let pgid = xid; // PGID set to the same as PID in `pre_exec()`.
            killpg(pgid as i32, SIGTERM);
            Self::reap_zombie_process(pgid.try_into().unwrap());
        };

        thread::sleep(Duration::from_millis(200));

        if let Ok(xid) = xid.try_into() {
            if Self::running(xid) {
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
                }
            }
            Ok(())
        } else {
            Err(io::Error::new(io::ErrorKind::InvalidInput, "Invalid PID").into())
        }
    }

    /// Checks if a process is running by sending signal 0 (non-destructive test).
    pub fn running(pid: c_int) -> bool {
        unsafe { kill(pid, 0) == 0 }
    }

    /// Reaps the (potential) zombie process. If the process isn't a zombie, `waitpid`
    /// returns immediately with an error (which we ignore).
    fn reap_zombie_process(pid: c_int) {
        let _ = waitpid(Pid::from_raw(pid), None);
    }
}

/// Spawns a simple child process asynchronously. Optionally captures the child's stdout/stderr,
/// or inherits them if `capture_output` is false. Updates the application’s [`AppState`]
/// and logs any errors.
///
/// # Arguments
/// * `command` - The [`Command`] to execute.
/// * `capture_output` - Whether to capture the child’s I/O or inherit it.
/// * `state` - Mutable reference to an [`AppState`] for logging or state updates.
/// * `state_path` - The location/path to which state updates are persisted.
///
/// # Returns
/// - `Ok(Child)` if the process spawned successfully.
/// - `Err(io::Error)` if spawning fails.
///
/// # Note
/// - Does **not** create a new process group or call `setsid()`.
/// - If you need a supervised child with reaping and resource monitoring, 
///   use [`spawn_complex_process`] or [`SupervisedChild::new`].
pub async fn spawn_simple_process(
    command: &mut Command,
    capture_output: bool,
    state: &mut AppState,
    state_path: &PathType,
) -> Result<Child, io::Error> {
    if capture_output {
        command.stdout(Stdio::piped());
        command.stderr(Stdio::piped());
    } else {
        command.stdout(Stdio::inherit());
        command.stderr(Stdio::inherit());
    }

    match command.spawn() {
        Ok(child_process) => {
            log!(LogLevel::Trace, "Child process spawned successfully: {:?}", child_process);
            state.data = String::from("Process spawned");
            state.event_counter += 1;
            update_state(state, state_path, None).await;
            Ok(child_process)
        }
        Err(e) => {
            log!(LogLevel::Error, "Failed to spawn child process: {}", e.to_string());
            let error_item: ErrorArrayItem = ErrorArrayItem::new(
                dusa_collection_utils::errors::Errors::InputOutput,
                e.to_string(),
            );
            log_error(state, error_item, state_path).await;
            Err(e)
        }
    }
}

/// Spawns a more complex child process that:
/// - Optionally sets its own process group (via `setsid()` in a `pre_exec` hook),
/// - Optionally captures stdout/stderr,
/// - Initializes resource monitoring in [`ResourceMonitorLock`],
/// - Wraps the process in a [`SupervisedChild`].
///
/// # Arguments
/// * `command` - The [`Command`] to spawn.
/// * `working_dir` - Optional path to set as the child’s current directory.
/// * `independent_process_group` - If `true`, calls `setsid()` on spawn to isolate the process.
/// * `capture_output` - If `true`, captures stdout/stderr; otherwise inherits them.
///
/// # Returns
/// - `Ok(SupervisedChild)` containing the locked child process and resource monitor.
/// - `Err(ErrorArrayItem)` if there's an error spawning the child or initializing the monitor.
///
/// # Platform Details
/// - **Linux**: `setsid()` is called in `pre_exec()` to detach from the parent's controlling terminal,
///   giving the child a new session and making its PID the session and group leader.
pub async fn spawn_complex_process(
    command: &mut Command,
    working_dir: Option<PathType>,
    independent_process_group: bool,
    capture_output: bool,
) -> Result<SupervisedChild, ErrorArrayItem> {
    log!(LogLevel::Trace, "Child to spawn: {:?}", &command);

    // If we want a new process group, call setsid() in pre_exec()
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
        log!(LogLevel::Trace, "Complex process being spawned in the same process group");
    }

    if capture_output {
        command.stdout(Stdio::piped());
        command.stderr(Stdio::piped());
    } else {
        command.stdout(Stdio::inherit());
        command.stderr(Stdio::inherit());
    }

    if let Some(path) = working_dir {
        command.current_dir(path.canonicalize().map_err(ErrorArrayItem::from)?);
    }

    match command.spawn() {
        Ok(child) => {
            log!(LogLevel::Trace, "Child process spawned successfully: {:#?}", child);

            let pid = match child.id() {
                Some(d) => d,
                None => {
                    return Err(ErrorArrayItem::new(
                        Errors::InputOutput,
                        "Couldn't determine if process spawned".to_owned(),
                    ))
                }
            };

            let monitor = match ResourceMonitorLock::new(pid as i32) {
                Ok(resource_monitor) => resource_monitor,
                Err(e) => {
                    return Err(ErrorArrayItem::from(io::Error::new(
                        io::ErrorKind::InvalidData,
                        e.to_string(),
                    )))
                }
            };

            let child = ChildLock::new(child);

            Ok(SupervisedChild {
                child,
                monitor,
                monitor_handle: None,
            })
        }
        Err(error) => {
            log!(LogLevel::Error, "Failed to spawn child process: {}", error);
            Err(ErrorArrayItem::from(error))
        }
    }
}

/// Checks if a PID is active on the system by sending signal 0. This is a common method
/// for detecting whether a process still exists (and if permissions allow signals).
///
/// # Returns
/// - `Ok(true)` if the process exists or if we lack permissions (EPERM).
/// - `Ok(false)` if the process does not exist (ESRCH).
/// - `Err(io::Error)` for other system errors.
///
/// # Example
/// ```rust
/// # use artisan_middleware::process_manager::is_pid_active;
/// match is_pid_active(1234) {
///     Ok(true) => println!("PID 1234 is active"),
///     Ok(false) => println!("PID 1234 is not active"),
///     Err(e) => eprintln!("Error checking PID 1234: {}", e),
/// }
/// ```
pub fn is_pid_active(pid: i32) -> io::Result<bool> {
    // Send signal 0 to check for existence
    let ret = unsafe { libc::kill(pid, 0) };
    if ret == 0 {
        // kill returned 0 => process exists or permissions are allowed
        Ok(true)
    } else {
        // kill returned -1 => check errno
        match io::Error::last_os_error().raw_os_error() {
            Some(libc::ESRCH) => Ok(false), // No such process
            Some(libc::EPERM) => Ok(true),  // Process exists, but no permission
            Some(err) => Err(io::Error::from_raw_os_error(err)),
            None => Err(io::Error::new(io::ErrorKind::Other, "Unknown error")),
        }
    }
}
