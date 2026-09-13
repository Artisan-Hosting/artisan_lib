use dusa_collection_utils::core::errors::{ErrorArrayItem, Errors};
use dusa_collection_utils::core::logger::LogLevel;
use dusa_collection_utils::core::types::pathtype::PathType;
use dusa_collection_utils::core::types::rb::RollingBuffer;
use dusa_collection_utils::core::types::rwarc::LockWithTimeout;
use dusa_collection_utils::log;
use libc::{c_int, kill, SIGKILL, SIGTERM};
use nix::sys::wait::{waitpid, WaitPidFlag, WaitStatus};
use nix::unistd::Pid;
use std::collections::{HashMap, HashSet, VecDeque};
use std::pin::Pin;
use std::process::Stdio;
use std::time::Duration;
use std::{io, thread};

use procfs::process::{all_processes, Process};
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::{Child, Command};
use tokio::task::JoinHandle;

use crate::aggregator::Metrics;
use crate::resource_monitor::{MonitorWatchdog, MonitorWatchdogSnapshot, ResourceMonitorLock};
use crate::state_persistence::{log_error, update_state, AppState};

const RESOURCE_MONITOR_SAMPLE_INTERVAL: Duration = Duration::from_millis(250);
const STDX_BUFFER_UPDATE_INTERVAL: Duration = Duration::from_millis(500);
/// A wrapper around [`LockWithTimeout<Child>`] that synchronizes access to a
/// [`tokio::process::Child`]. This allows safe concurrent reads/writes or attempts to kill
/// the child within specified timeouts.
pub struct ChildLock(pub LockWithTimeout<Child>);

/// Holds a [`ChildLock`] plus a resource monitor and an optional handle to a background
/// monitoring task. This structure is used to manage a spawned child process in an
/// asynchronous context (Tokio).
///
/// - The `monitor_handle` can be used to stop the resource monitor loop if needed.
/// - The `monitor_std` handle is used to monitor the process standard output/error streams.
/// - Lock-free watchdog snapshots expose monitor health without requiring access to task handles.
/// - It's up to the caller to decide if/how to store and use captured output lines.
/// - The resource monitor tracks CPU/memory usage via `/proc` (Linux-specific).
pub struct SupervisedChild {
    /// The locked child process.
    pub child: ChildLock,
    /// Resource-monitor lifecycle (sampling task + watchdog), shared with
    /// [`SupervisedProcess`] via [`ResourceSupervisor`].
    resources: ResourceSupervisor,
    /// An optional background task handle for monitoring std_out/err
    monitor_std: Option<JoinHandle<()>>,
    /// Internal tracker for standard out
    stdout_buffer: LockWithTimeout<RollingBuffer>,
    /// Internal tracker for standard err
    stderr_buffer: LockWithTimeout<RollingBuffer>,
    /// Health/heartbeat state for the stdout/stderr monitor loop.
    stdx_watchdog: MonitorWatchdog,
}

/// Represents a supervised process that may not have been spawned via [`tokio::process::Command`]
/// but is still tracked by a PID. Similar to `SupervisedChild`, but manages an existing
/// process rather than a newly spawned one.
pub struct SupervisedProcess {
    /// The process ID (PID) of the target process.
    pid: Pid,
    /// Resource-monitor lifecycle (sampling task + watchdog), shared with
    /// [`SupervisedChild`] via [`ResourceSupervisor`].
    resources: ResourceSupervisor,
}

/// The resource-monitor lifecycle -- the `/proc` sampling task plus its health
/// watchdog -- shared by [`SupervisedChild`] and [`SupervisedProcess`].
///
/// Both types used to carry this as three separate fields plus five near-identical
/// methods; the only difference between the two copies was a word in a log message.
/// Pulling it out here means there's exactly one place left that starts, stops, and
/// health-checks a resource-monitor task.
struct ResourceSupervisor {
    monitor: ResourceMonitorLock,
    handle: Option<JoinHandle<()>>,
    watchdog: MonitorWatchdog,
}

impl ResourceSupervisor {
    fn new(pid: i32) -> Result<Self, ErrorArrayItem> {
        Ok(Self {
            monitor: ResourceMonitorLock::new(pid)?,
            handle: None,
            watchdog: MonitorWatchdog::new(),
        })
    }

    /// Starts the sampling loop if it isn't already running, restarting it if the
    /// previous task died unexpectedly. `pid_hint` is only used for the log message.
    async fn ensure_running(&mut self, pid_hint: Option<u32>) {
        if let Some(handle) = &self.handle {
            if handle.is_finished() {
                log!(
                    LogLevel::Warn,
                    "Resource monitor task finished unexpectedly for pid {:?}, restarting",
                    pid_hint
                );
                self.handle = None;
            } else {
                return;
            }
        }

        let monitor = self.monitor.clone();
        let handle: JoinHandle<()> = monitor
            .monitor_with_watchdog_interval(
                RESOURCE_MONITOR_SAMPLE_INTERVAL,
                Some(self.watchdog.clone()),
            )
            .await;
        self.handle = Some(handle);
    }

    /// Stops the sampling task, if any, via [`JoinHandle::abort()`].
    fn terminate(&mut self) {
        if let Some(handle) = &self.handle {
            log!(LogLevel::Trace, "Terminating monitor");
            handle.abort();
            self.handle = None;
            self.watchdog.mark_stopped();
        }
    }

    /// Returns whether the sampling task is currently running, clearing a
    /// finished-but-not-yet-noticed handle as a side effect.
    fn is_running(&mut self) -> bool {
        if let Some(handle) = &self.handle {
            if handle.is_finished() {
                self.handle = None;
                self.watchdog.mark_stopped();
                false
            } else {
                true
            }
        } else {
            false
        }
    }

    async fn get_metrics(&self) -> Result<Metrics, ErrorArrayItem> {
        self.monitor.get_metrics().await
    }

    fn watchdog_snapshot(&self) -> MonitorWatchdogSnapshot {
        self.watchdog.snapshot()
    }

    fn valid(&self, max_staleness: Duration, max_consecutive_failures: u64) -> bool {
        self.watchdog
            .snapshot()
            .is_valid(max_staleness, max_consecutive_failures)
    }

    /// Stops any running task on `self`, then returns a fresh instance sharing the
    /// same monitor and watchdog state but with no task of its own.
    fn clone_idle(&mut self) -> Self {
        self.terminate();
        Self {
            monitor: self.monitor.clone(),
            handle: None,
            watchdog: self.watchdog.clone(),
        }
    }
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
        if !is_pid_active(pid.as_raw()).unwrap_or(false) {
            return Err(ErrorArrayItem::new(
                Errors::SupervisedChild,
                format!(
                    "Failed to create SupervisedProcess; cannot determine status of PID: {}",
                    pid
                ),
            ));
        }

        Ok(SupervisedProcess {
            pid,
            resources: ResourceSupervisor::new(pid.as_raw())?,
        })
    }

    /// Returns the raw PID of this process.
    pub fn get_pid(&self) -> i32 {
        self.pid.as_raw()
    }

    /// Returns the resource monitor backing this process.
    pub fn monitor(&self) -> &ResourceMonitorLock {
        &self.resources.monitor
    }

    /// Terminates the monitored process by:
    /// 1. Stopping any monitoring task.
    /// 2. Recursively sending a `SIGTERM` to all processes in the PGID.
    /// 3. Reaping zombies (via `waitpid`) if the processes exit.
    /// 4. If any remain after 400ms, sending `SIGKILL`.
    ///
    /// # Errors
    /// - Returns an I/O error if any `kill` syscall fails unexpectedly.
    /// - Also returns an error if the process cannot be reaped properly.
    ///
    /// # Why Reap Zombies?
    /// - In Linux, a process that has terminated but whose parent hasn't called `wait*()` is
    ///   marked as a "zombie." Reaping zombies avoids accumulation of defunct processes,
    ///   freeing kernel resources.
    pub fn kill(&mut self) -> Result<(), ErrorArrayItem> {
        self.resources.terminate();
        let xid = self.pid.as_raw();
        log!(LogLevel::Trace, "Killing supervised pid {}", xid);

        kill_pgid_recursive(xid)?;
        Ok(())
    }

    /// Returns `true` if the process is still active (PID exists), or `false` otherwise.
    ///
    /// # Zombie caveat
    /// Unlike [`SupervisedChild::running`], this can't reap on your behalf: a
    /// `SupervisedProcess` wraps a bare PID that this instance is frequently *not*
    /// the real parent of (e.g. re-attached to a PID recorded before a watchdog
    /// restart). Only the actual parent -- or `init`/a subreaper, once the process
    /// is reparented -- can reap it. So a zombie still reports as "running" here;
    /// this only tells you whether the PID still exists in the process table.
    pub fn running(&self) -> bool {
        is_pid_active(self.pid.as_raw()).unwrap_or(false)
    }

    /// Alias for [`SupervisedProcess::running`].
    pub fn active(&self) -> bool {
        self.running()
    }

    /// Clones this `SupervisedProcess`, returning a new instance without a running monitor.
    /// The existing monitor is terminated before cloning.
    pub async fn clone(&mut self) -> Self {
        Self {
            pid: self.pid,
            resources: self.resources.clone_idle(),
        }
    }

    /// Spawns an asynchronous resource monitoring loop that periodically queries
    /// `/proc/<pid>` for CPU/memory usage.
    ///
    /// # Note
    /// - Calling this again is a no-op unless the previous monitor task has died.
    /// - A watchdog is updated on each loop iteration for out-of-band health checks.
    pub async fn monitor_usage(&mut self) {
        self.resources
            .ensure_running(Some(self.pid.as_raw() as u32))
            .await;
    }

    /// Terminates the resource monitor task, if any.
    ///
    /// # Note
    /// - Uses [`JoinHandle::abort()`] to stop the task immediately.
    pub fn terminate_monitor(&mut self) {
        self.resources.terminate();
    }

    /// Checks if there is currently a resource monitor running
    /// for a given [`SupervisedProcess`]
    pub fn monitoring(&mut self) -> bool {
        self.resources.is_running()
    }

    /// Fetches resource usage metrics (CPU, memory, etc.) from the process-specific resource monitor.
    ///
    /// # Errors
    /// - Returns an [`ErrorArrayItem`] if the resource monitor fails to read from `/proc` or
    ///   if the process does not exist anymore.
    pub async fn get_metrics(&self) -> Result<Metrics, ErrorArrayItem> {
        self.resources.get_metrics().await
    }

    /// Returns a lock-free watchdog snapshot for the resource monitor.
    pub fn resource_watchdog_snapshot(&self) -> MonitorWatchdogSnapshot {
        self.resources.watchdog_snapshot()
    }

    /// Returns whether the resource monitor appears healthy.
    pub fn resource_monitor_valid(
        &self,
        max_staleness: Duration,
        max_consecutive_failures: u64,
    ) -> bool {
        self.resources.valid(max_staleness, max_consecutive_failures)
    }
}

impl SupervisedChild {
    /// Spawns a new child process with its own process group and optionally captures stdout/stderr.
    /// The resulting process is wrapped in a [`SupervisedChild`] which provides:
    /// - Locking for the child handle
    /// - A resource monitor
    /// - Optional background monitoring
    /// - Initialized resource/stdx watchdogs
    ///
    /// # Behavior
    /// - Uses [`spawn_complex_process`] under the hood.
    /// - `true` for capturing output means the child's output is piped rather than inherited.
    /// - `true` for `independent_process_group` means it calls `setsid()` in `pre_exec` on Linux,
    ///   so the child won't receive signals from the parent TTY group directly.
    ///
    /// # Errors
    /// - Returns an [`ErrorArrayItem`] if spawning fails or if resource monitoring fails to initialize.
    pub async fn new(
        command: &mut Command,
        working_dir: Option<PathType>,
    ) -> Result<Self, ErrorArrayItem> {
        spawn_complex_process(command, working_dir, false, true).await // ! set process group back to false
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

    /// Clones this `SupervisedChild` without active monitor tasks.
    ///
    /// This aborts current monitor tasks, then clones the child lock, resource monitor lock,
    /// buffers, and watchdog state.
    pub async fn clone(&mut self) -> Self {
        self.terminate_stdx();
        let resources = self.resources.clone_idle();
        let child_lock: ChildLock = self.child.clone();

        Self {
            child: child_lock,
            resources,
            monitor_std: None,
            stdout_buffer: self.stdout_buffer.clone(),
            stderr_buffer: self.stderr_buffer.clone(),
            stdx_watchdog: self.stdx_watchdog.clone(),
        }
    }

    /// Recursively terminates the child process group. Sends `SIGTERM` to all
    /// descendant PIDs and then `SIGKILL` to any that remain.
    ///
    /// # Errors
    /// - Returns an [`ErrorArrayItem`] on I/O issues or if reaping fails.
    pub async fn kill(&mut self) -> Result<(), ErrorArrayItem> {
        self.resources.terminate();
        self.terminate_stdx();
        self.child.kill().await
    }

    /// Non-blocking check for whether this child has already exited, reaping it if so.
    ///
    /// See [`ChildLock::try_wait`] for details.
    pub async fn try_wait(&self) -> Result<Option<std::process::ExitStatus>, ErrorArrayItem> {
        self.child.try_wait().await
    }

    /// Checks if the child process is still running.
    ///
    /// See [`ChildLock::running`] -- this reaps the process via `try_wait` instead of
    /// signaling the raw PID, so an exited-but-unreaped child (a zombie) is correctly
    /// reported as not running instead of appearing alive.
    pub async fn running(&self) -> bool {
        self.child.running().await
    }

    /// Returns the resource monitor backing this child.
    pub fn monitor(&self) -> &ResourceMonitorLock {
        &self.resources.monitor
    }

    /// Spawns an asynchronous resource monitoring loop for this child. If a monitor is
    /// already running, this does nothing.
    ///
    /// # Behavior
    /// - Queries `/proc/<pid>` for CPU, memory, etc. on a sub-second sampling interval.
    /// - Use [`terminate_monitor`] to stop the task.
    /// - A watchdog is updated on each loop iteration for out-of-band health checks.
    pub async fn monitor_usage(&mut self) {
        let pid_hint = self.get_pid().await.ok();
        self.resources.ensure_running(pid_hint).await;
    }

    /// Returns whether the child resource monitor task is currently running.
    ///
    /// If the handle exists but has finished, it is cleared and `false` is returned.
    pub fn monitoring(&mut self) -> bool {
        self.resources.is_running()
    }

    /// Spawns an asynchronous resource monitoring loop for the standard out and standard error. If a monitor is
    /// already running, this does nothing.
    ///
    /// # Behavior
    /// - Acquires the child lock, takes stdout/stderr handles, and streams lines into rolling buffers.
    /// - Retries on transient lock errors instead of exiting permanently.
    /// - Use [`terminate_stdx`] to stop the task.
    pub async fn monitor_stdx(&mut self) {
        if let Some(handle) = &self.monitor_std {
            if handle.is_finished() {
                log!(
                    LogLevel::Warn,
                    "Stdout/stderr monitor finished unexpectedly for child pid {:?}, restarting",
                    self.get_pid().await.ok()
                );
                self.monitor_std = None;
            } else {
                return;
            }
        }

        let child_lock = self.child.clone();
        let stdout_buffer = self.stdout_buffer.clone();
        let stderr_buffer = self.stderr_buffer.clone();
        let stdx_watchdog = self.stdx_watchdog.clone();

        let monitor_handle = tokio::spawn(async move {
            let mut stdout_task = None;
            let mut stderr_task = None;
            stdx_watchdog.mark_started();

            loop {
                match child_lock.0.try_write().await {
                    Ok(mut child) => {
                        if let Some(stdout) = child.stdout.take() {
                            let reader = Box::pin(stdout) as Pin<Box<dyn AsyncRead + Send>>;
                            let buffer = stdout_buffer.clone();
                            stdout_task = Some(tokio::spawn(read_stream_to_buffer(
                                reader,
                                buffer,
                                STDX_BUFFER_UPDATE_INTERVAL,
                            )));
                        }

                        if let Some(stderr) = child.stderr.take() {
                            let reader = Box::pin(stderr) as Pin<Box<dyn AsyncRead + Send>>;
                            let buffer = stderr_buffer.clone();
                            stderr_task = Some(tokio::spawn(read_stream_to_buffer(
                                reader,
                                buffer,
                                STDX_BUFFER_UPDATE_INTERVAL,
                            )));
                        }
                        stdx_watchdog.record_success();
                        break;
                    }
                    Err(err) => {
                        log!(
                            LogLevel::Warn,
                            "Failed locking child for stdio monitor: {}",
                            err
                        );
                        stdx_watchdog.record_failure();
                        tokio::time::sleep(Duration::from_millis(250)).await;
                    }
                }
            }

            if let Some(task) = stdout_task {
                let _ = task.await;
            }
            if let Some(task) = stderr_task {
                let _ = task.await;
            }
            stdx_watchdog.mark_stopped();
        });

        self.monitor_std = Some(monitor_handle)
    }

    /// Returns whether the child stdout/stderr monitor task is currently running.
    ///
    /// If the handle exists but has finished, it is cleared and `false` is returned.
    pub fn monitoring_stdx(&mut self) -> bool {
        if let Some(handle) = &self.monitor_std {
            if handle.is_finished() {
                self.monitor_std = None;
                self.stdx_watchdog.mark_stopped();
                false
            } else {
                true
            }
        } else {
            false
        }
    }

    /// Gets the current value of the standard output [`RollingBuffer`] as `Vec<(timestamp, line)>`.
    pub async fn get_std_out(&self) -> Result<Vec<(u64, String)>, ErrorArrayItem> {
        let rb = self.stdout_buffer.try_read().await?;
        Ok(rb.get_latest_time())
    }

    /// Gets the current value of the standard error [`RollingBuffer`] as `Vec<(timestamp, line)>`.
    pub async fn get_std_err(&self) -> Result<Vec<(u64, String)>, ErrorArrayItem> {
        let rb = self.stderr_buffer.try_read().await?;
        Ok(rb.get_latest_time())
    }

    /// Terminates the resource monitor task, if any is currently running. This calls
    /// [`JoinHandle::abort()`] on the stored handle.
    pub fn terminate_monitor(&mut self) {
        self.resources.terminate();
    }

    /// Terminates the stdout/stderr monitor task, if currently running. This calls
    /// [`JoinHandle::abort()`] on the stored handle.
    pub fn terminate_stdx(&mut self) {
        if let Some(handle) = &self.monitor_std {
            log!(LogLevel::Trace, "Terminating Standart X monitor");
            handle.abort();
            self.monitor_std = None;
            self.stdx_watchdog.mark_stopped();
        }
    }

    /// Retrieves the current resource usage metrics from `/proc`.
    /// Returns an error if the process has exited or if `/proc` parsing fails.
    pub async fn get_metrics(&self) -> Result<Metrics, ErrorArrayItem> {
        self.resources.get_metrics().await
    }

    /// Returns a lock-free watchdog snapshot for the child resource monitor.
    pub fn resource_watchdog_snapshot(&self) -> MonitorWatchdogSnapshot {
        self.resources.watchdog_snapshot()
    }

    /// Returns a lock-free watchdog snapshot for the child stdout/stderr monitor.
    pub fn stdx_watchdog_snapshot(&self) -> MonitorWatchdogSnapshot {
        self.stdx_watchdog.snapshot()
    }

    /// Returns whether the child resource monitor appears healthy.
    pub fn resource_monitor_valid(
        &self,
        max_staleness: Duration,
        max_consecutive_failures: u64,
    ) -> bool {
        self.resources.valid(max_staleness, max_consecutive_failures)
    }

    /// Returns whether the child stdout/stderr monitor appears healthy.
    pub fn stdx_monitor_valid(
        &self,
        max_staleness: Duration,
        max_consecutive_failures: u64,
    ) -> bool {
        self.stdx_watchdog
            .snapshot()
            .is_valid(max_staleness, max_consecutive_failures)
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

    /// Recursively terminates the child's process group. Sends `SIGTERM` to all
    /// descendant PIDs and then `SIGKILL` to any that remain, logging progress
    /// at `Trace` level.
    ///
    /// # Errors
    /// - Returns an [`ErrorArrayItem`] on I/O issues or if reaping fails.
    /// - If the child’s PID is invalid, returns an error.
    pub async fn kill(&self) -> Result<(), ErrorArrayItem> {
        let child = self
            .0
            .try_read_with_timeout(Some(Duration::from_secs(5)))
            .await?;

        let xid = match child.id() {
            Some(xid) => xid,
            None => {
                return Err(ErrorArrayItem::new(
                    dusa_collection_utils::core::errors::Errors::InputOutput,
                    "No PID found in child process".to_owned(),
                ))
            }
        };

        log!(LogLevel::Trace, "Killing child pid {}", xid);

        if let Ok(xid) = xid.try_into() {
            kill_pgid_recursive(xid)?;
            Ok(())
        } else {
            Err(io::Error::new(io::ErrorKind::InvalidInput, "Invalid PID").into())
        }
    }

    /// Non-blocking check for whether the child has already exited.
    ///
    /// This calls [`tokio::process::Child::try_wait`] under the hood, which performs
    /// a `waitpid(..., WNOHANG)` on our behalf. Unlike a raw `kill(pid, 0)` signal
    /// check, this correctly reports an exited process as no longer running (rather
    /// than as a still-existing zombie), and it reaps the process in the same call
    /// so it never lingers as a zombie.
    ///
    /// # Returns
    /// - `Ok(Some(status))` if the process has already exited (now reaped).
    /// - `Ok(None)` if the process is still running.
    ///
    /// # Errors
    /// - Returns an [`ErrorArrayItem`] if the child lock can't be acquired in time.
    pub async fn try_wait(&self) -> Result<Option<std::process::ExitStatus>, ErrorArrayItem> {
        let mut child = self
            .0
            .try_write_with_timeout(Some(Duration::from_secs(1)))
            .await?;
        child.try_wait().map_err(ErrorArrayItem::from)
    }

    /// Checks if the child is still running -- the one true liveness check for a
    /// process we hold a real [`tokio::process::Child`] handle for.
    ///
    /// This reaps via `try_wait` rather than signaling the PID directly, so an
    /// exited-but-unreaped child (a zombie) is correctly reported as not running
    /// instead of appearing alive.
    ///
    /// A lock-acquisition timeout is treated as "unknown" and reported as still
    /// running, so callers don't tear down a healthy child on transient contention.
    /// Any other error (e.g. the OS reporting no such child, which happens if the
    /// process was already reaped elsewhere, such as by a concurrent `kill()`) is
    /// treated as "not running".
    pub async fn running(&self) -> bool {
        match self.try_wait().await {
            Ok(None) => true,
            Ok(Some(_)) => false,
            Err(err) if err.err_type == Errors::GeneralError => true,
            Err(_) => false,
        }
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
            log!(
                LogLevel::Trace,
                "Child process spawned successfully: {:?}",
                child_process
            );
            state.data = String::from("Process spawned");
            state.event_counter += 1;
            update_state(state, state_path, None).await;
            Ok(child_process)
        }
        Err(e) => {
            log!(
                LogLevel::Error,
                "Failed to spawn child process: {}",
                e.to_string()
            );
            let error_item: ErrorArrayItem = ErrorArrayItem::new(
                dusa_collection_utils::core::errors::Errors::InputOutput,
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
/// - Wraps the process in a [`SupervisedChild`] with initialized watchdogs.
///
/// # Arguments
/// * `command` - The [`Command`] to spawn.
/// * `working_dir` - Optional path to set as the child’s current directory.
/// * `independent_process_group` - If `true`, calls `setsid()` on spawn to isolate the process.
/// * `capture_output` - If `true`, captures stdout/stderr; otherwise inherits them.
///
/// # Returns
/// - `Ok(SupervisedChild)` containing the locked child process, resource monitor, and watchdogs.
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
        command.kill_on_drop(true);
        log!(
            LogLevel::Trace,
            "Complex process being spawned in the same process group"
        );
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
        Ok(mut child) => {
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
                        "Couldn't determine if process spawned".to_owned(),
                    ))
                }
            };

            let monitor = match ResourceMonitorLock::new(pid as i32) {
                Ok(resource_monitor) => resource_monitor,
                Err(e) => {
                    child.kill().await?;
                    return Err(ErrorArrayItem::from(io::Error::new(
                        io::ErrorKind::InvalidData,
                        e.to_string(),
                    )));
                }
            };

            let child = ChildLock::new(child);

            Ok(SupervisedChild {
                child,
                resources: ResourceSupervisor {
                    monitor,
                    handle: None,
                    watchdog: MonitorWatchdog::new(),
                },
                monitor_std: None,
                stdout_buffer: LockWithTimeout::new(RollingBuffer::new(500)),
                stderr_buffer: LockWithTimeout::new(RollingBuffer::new(500)),
                stdx_watchdog: MonitorWatchdog::new(),
            })
        }
        Err(error) => {
            log!(LogLevel::Error, "Failed to spawn child process: {}", error);
            Err(ErrorArrayItem::from(error))
        }
    }
}

/// Recursively collect all descendant PIDs of a given process ID, including the parent PID.
fn collect_descendants(root_pid: i32) -> Result<HashSet<i32>, ErrorArrayItem> {
    let mut children_map: HashMap<i32, Vec<i32>> = HashMap::new();
    let mut result: HashSet<i32> = HashSet::new();

    for prc in all_processes()
        .map_err(|e| ErrorArrayItem::from(io::Error::new(io::ErrorKind::Other, e.to_string())))?
    {
        let process: Process = match prc {
            Ok(p) => p,
            Err(_) => continue,
        };
        if let Ok(stat) = process.stat() {
            children_map
                .entry(stat.ppid)
                .or_default()
                .push(process.pid());
        }
    }

    let mut queue: VecDeque<i32> = VecDeque::new();
    queue.push_back(root_pid);
    result.insert(root_pid);

    while let Some(pid) = queue.pop_front() {
        if let Some(children) = children_map.get(&pid) {
            for child in children {
                if result.insert(*child) {
                    queue.push_back(*child);
                }
            }
        }
    }

    Ok(result)
}

/// Makes one non-blocking reap attempt (`waitpid(pid, WNOHANG)`) on a bare PID.
///
/// This only actually reaps anything if we're the real parent of `pid`; otherwise
/// `waitpid` fails (typically ECHILD) and that failure is logged at `Trace` and
/// ignored, since there's nothing we can do about a process we don't own.
fn reap_zombie_process(pid: c_int) {
    match waitpid(Pid::from_raw(pid), Some(WaitPidFlag::WNOHANG)) {
        Ok(WaitStatus::Exited(_, status)) => {
            log!(
                LogLevel::Trace,
                "Reaped pid {} with exit status {}",
                pid,
                status
            )
        }
        Ok(WaitStatus::Signaled(_, sig, _)) => {
            log!(
                LogLevel::Trace,
                "Reaped pid {} terminated by signal {:?}",
                pid,
                sig
            )
        }
        Ok(WaitStatus::StillAlive) => {
            log!(
                LogLevel::Trace,
                "PID {} still alive when attempting reap",
                pid
            )
        }
        Ok(status) => {
            log!(LogLevel::Trace, "PID {} wait status: {:?}", pid, status)
        }
        Err(e) => {
            log!(LogLevel::Trace, "Failed to reap pid {}: {}", pid, e)
        }
    }
}

/// Kill all processes belonging to a PGID and all of their descendants.
fn kill_pgid_recursive(pgid: i32) -> Result<(), ErrorArrayItem> {
    log!(LogLevel::Trace, "Recursively killing pgid: {}", pgid);
    let pids = collect_descendants(pgid)?;
    log!(LogLevel::Trace, "Found descendant pids: {:?}", pids);

    for pid in &pids {
        let res = unsafe { kill(*pid, SIGTERM) };
        if res == 0 {
            log!(LogLevel::Trace, "Sent SIGTERM to pid: {}", pid);
        } else {
            let err = io::Error::last_os_error();
            if err.raw_os_error() == Some(libc::ESRCH) {
                log!(LogLevel::Trace, "PID {} already exited", pid);
            } else {
                log!(
                    LogLevel::Warn,
                    "Failed to send SIGTERM to pid {}: {}",
                    pid,
                    err
                );
            }
        }
    }

    thread::sleep(Duration::from_millis(400));

    for pid in &pids {
        reap_zombie_process(*pid);
        if is_pid_active(*pid).unwrap_or(false) {
            log!(LogLevel::Warn, "PID {} still running; sending SIGKILL", pid);
            let res = unsafe { kill(*pid, SIGKILL) };
            if res != 0 {
                let err = io::Error::last_os_error();
                if err.raw_os_error() != Some(libc::ESRCH) {
                    return Err(ErrorArrayItem::from(err));
                }
            }
            reap_zombie_process(*pid);
            if !is_pid_active(*pid).unwrap_or(false) {
                log!(LogLevel::Trace, "PID {} terminated", pid);
            } else {
                log!(LogLevel::Warn, "PID {} survived SIGKILL", pid);
            }
        } else {
            log!(LogLevel::Trace, "PID {} terminated gracefully", pid);
        }
    }

    Ok(())
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

use bytes::BytesMut;

async fn flush_lines_to_buffer(
    buffer: &LockWithTimeout<RollingBuffer>,
    pending_lines: &mut Vec<String>,
) {
    if pending_lines.is_empty() {
        return;
    }

    if let Ok(mut b) = buffer.try_write().await {
        for line in pending_lines.drain(..) {
            b.push(line);
        }
    }
}

async fn read_stream_to_buffer<R>(
    mut reader: R,
    buffer: LockWithTimeout<RollingBuffer>,
    flush_interval: Duration,
) where
    R: Unpin + AsyncRead,
{
    let mut buf = BytesMut::with_capacity(1024);
    let mut partial = String::new();
    let mut pending_lines: Vec<String> = Vec::new();
    let mut last_flush = std::time::Instant::now();

    loop {
        let remaining_until_flush = flush_interval.saturating_sub(last_flush.elapsed());
        match tokio::time::timeout(remaining_until_flush, reader.read_buf(&mut buf)).await {
            Err(_) => {
                flush_lines_to_buffer(&buffer, &mut pending_lines).await;
                last_flush = std::time::Instant::now();
                continue;
            }
            Ok(result) => match result {
                Ok(n) if n == 0 => break, // EOF
                Ok(_) => {}
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => {
                    log!(LogLevel::Warn, "Read error in stdio monitor: {}", e);
                    break;
                }
            },
        };

        let chunk = String::from_utf8_lossy(&buf);
        partial.push_str(&chunk);

        while let Some(pos) = partial.find('\n') {
            let line = partial[..pos].to_string();
            pending_lines.push(line);
            partial.drain(..=pos); // remove up to and including newline
        }

        buf.clear();
        if last_flush.elapsed() >= flush_interval {
            flush_lines_to_buffer(&buffer, &mut pending_lines).await;
            last_flush = std::time::Instant::now();
        }
    }

    // Push any trailing partial line
    if !partial.is_empty() {
        pending_lines.push(partial);
    }
    flush_lines_to_buffer(&buffer, &mut pending_lines).await;
}
