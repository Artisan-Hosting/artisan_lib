use dusa_collection_utils::log;
use dusa_collection_utils::logger::LogLevel;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::time::Duration;
use tokio::{sync::Notify, time::timeout};

/// A control structure used to toggle between "paused" and "resumed" states,
/// allowing asynchronous tasks to wait until they are resumed.
#[derive(Debug)]
pub struct ToggleControl {
    /// Atomic boolean indicating whether the control is currently in a paused state.
    paused: AtomicBool,
    /// Notification used to signal that the system has been paused.
    notify_pause: Notify,
    /// Notification used to signal that the system has been resumed.
    notify_resume: Notify,
}

impl ToggleControl {
    /// Creates a new `ToggleControl` instance in the "resumed" (unpaused) state.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use tokio::runtime::Runtime;
    /// # use std::time::Duration;
    /// # use artisan_middleware::control::ToggleControl;
    /// # let rt = Runtime::new().unwrap();
    /// # rt.block_on(async {
    ///     let control = ToggleControl::new();
    ///     assert_eq!(control.is_paused().await, false);
    /// # });
    /// ```
    pub fn new() -> Self {
        Self {
            paused: AtomicBool::new(false),
            notify_pause: Notify::new(),
            notify_resume: Notify::new(),
        }
    }

    /// Pauses the control. Any calls to `wait_if_paused` by other tasks will block
    /// until `resume` is called.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use tokio::runtime::Runtime;
    /// # use std::time::Duration;
    /// # use artisan_middleware::control::ToggleControl;
    /// # let rt = Runtime::new().unwrap();
    /// # rt.block_on(async {
    ///     let control = ToggleControl::new();
    ///     control.pause();
    ///     assert_eq!(control.is_paused().await, true);
    /// # });
    /// ```
    pub fn pause(&self) {
        self.paused.store(true, Ordering::SeqCst);
        self.notify_pause.notify_waiters();
    }

    /// Resumes the control. Any tasks currently waiting in `wait_if_paused` or `wait_with_timeout`
    /// will proceed once this method is called.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use tokio::runtime::Runtime;
    /// # use std::time::Duration;
    /// # use artisan_middleware::control::ToggleControl;
    /// # let rt = Runtime::new().unwrap();
    /// # rt.block_on(async {
    ///     let control = ToggleControl::new();
    ///     control.pause();
    ///     control.resume();
    ///     assert_eq!(control.is_paused().await, false);
    /// # });
    /// ```
    pub fn resume(&self) {
        self.paused.store(false, Ordering::SeqCst);
        self.notify_resume.notify_waiters();
    }

    /// Asynchronously waits as long as the control is paused. Once `resume` is called,
    /// this method returns, allowing the waiting task to proceed.
    ///
    /// # Notes
    /// This method uses a loop to re-check the pause state after each `notify_resume`
    /// notification in case the control is paused again in quick succession.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use tokio::runtime::Runtime;
    /// # use std::time::Duration;
    /// # use artisan_middleware::control::ToggleControl;
    /// # use std::sync::Arc;
    /// # let rt = Runtime::new().unwrap();
    /// # rt.block_on(async {
    ///     let control = Arc::new(ToggleControl::new());
    ///     control.pause();
    ///
    ///     // In another task or later in the same task:
    ///     tokio::spawn({
    ///         let control_clone = control.clone();
    ///         async move {
    ///             // Wait 1 second before resuming
    ///             tokio::time::sleep(Duration::from_secs(1)).await;
    ///             control_clone.resume();
    ///         }
    ///     });
    ///
    ///     // This will block until resume is called
    ///     control.wait_if_paused().await;
    /// # });
    /// ```
    pub async fn wait_if_paused(&self) {
        log!(LogLevel::Trace, "In a wait loop");
        while self.paused.load(Ordering::SeqCst) {
            // Wait for the resume notification if paused
            self.notify_resume.notified().await;
        }
    }

    /// Asynchronously waits with a timeout while the control is paused. If the
    /// control is resumed before the timeout elapses, the function returns `Ok(())`.
    /// Otherwise, it returns an `Err("Timeout elapsed...")`.
    ///
    /// # Arguments
    ///
    /// * `duration` - The maximum duration to wait for the control to resume.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use tokio::runtime::Runtime;
    /// # use std::time::Duration;
    /// # use artisan_middleware::control::ToggleControl;
    /// # let rt = Runtime::new().unwrap();
    /// # rt.block_on(async {
    /// let control = ToggleControl::new();
    /// control.pause();
    ///
    /// // This will fail if not resumed within 1 second
    /// match control.wait_with_timeout(Duration::from_secs(1)).await {
    ///     Ok(_) => println!("Resumed in time!"),
    ///     Err(msg) => println!("Timed out: {}", msg),
    /// }
    /// # });
    /// ```
    pub async fn wait_with_timeout(&self, duration: Duration) -> Result<(), &'static str> {
        if self.paused.load(Ordering::SeqCst) {
            match timeout(duration, self.notify_resume.notified()).await {
                Ok(_) => Ok(()), // Resumed within timeout
                Err(_) => Err("Timeout elapsed before lock was released"),
            }
        } else {
            Ok(())
        }
    }

    /// Checks if the control is currently paused, returning `true` if it is
    /// paused, and `false` otherwise.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use artisan_middleware::control::ToggleControl;
    /// # use tokio::runtime::Runtime;
    /// # let rt = Runtime::new().unwrap();
    /// # rt.block_on(async {
    /// let control = ToggleControl::new();
    /// control.pause();
    /// assert_eq!(control.is_paused().await, true);
    ///
    /// control.resume();
    /// assert_eq!(control.is_paused().await, false);
    /// # });
    /// ```
    pub async fn is_paused(&self) -> bool {
        self.paused.load(Ordering::SeqCst)
    }
}
