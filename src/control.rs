use dusa_collection_utils::log;
use dusa_collection_utils::log::LogLevel;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::time::Duration;
use tokio::{sync::Notify, time::timeout};

#[derive(Debug)]
pub struct ToggleControl {
    paused: AtomicBool,
    notify_pause: Notify,
    notify_resume: Notify,
}

impl ToggleControl {
    pub fn new() -> Self {
        Self {
            paused: AtomicBool::new(false),
            notify_pause: Notify::new(),
            notify_resume: Notify::new(),
        }
    }

    pub fn pause(&self) {
        self.paused.store(true, Ordering::SeqCst);
        self.notify_pause.notify_waiters();
    }

    pub fn resume(&self) {
        self.paused.store(false, Ordering::SeqCst);
        self.notify_resume.notify_waiters();
    }

    pub async fn wait_if_paused(&self) {
        log!(LogLevel::Trace, "In a wait loop");
        while self.paused.load(Ordering::SeqCst) {
            // Wait for the resume notification if paused
            self.notify_resume.notified().await;
        }
    }

    pub async fn wait_with_timeout(&self, duration: Duration) -> Result<(), &'static str> {
        if self.paused.load(Ordering::SeqCst) {
            match timeout(duration, self.notify_resume.notified()).await {
                Ok(_) => Ok(()), // Lock released within timeout
                Err(_) => Err("Timeout elapsed before lock was released"), // Timeout elapsed
            }
        } else {
            Ok(()) // Lock was not active
        }
    }

    pub async fn is_paused(&self) -> bool {
        return self.paused.load(Ordering::SeqCst);
    }
}
