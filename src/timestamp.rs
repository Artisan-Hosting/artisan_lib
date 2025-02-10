use std::time::{Duration, SystemTime, UNIX_EPOCH};

use dusa_collection_utils::types::stringy::Stringy;

/// Retrieves the current Unix timestamp in seconds.
pub fn current_timestamp() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let start = SystemTime::now();
    let since_the_epoch = start
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards");
    since_the_epoch.as_secs()
}

/// Converts a Unix timestamp to a human-readable string.
pub fn format_unix_timestamp(timestamp: u64) -> Stringy {
    let duration: Duration = Duration::from_secs(timestamp);
    let datetime: SystemTime = UNIX_EPOCH + duration;
    let now: SystemTime = SystemTime::now();

    let data = if let Ok(elapsed) = now.duration_since(datetime) {
        let seconds = elapsed.as_secs();
        format!(
            "{:02}:{:02}:{:02}",
            seconds / 3600,
            (seconds % 3600) / 60,
            seconds % 60
        )
    } else if let Ok(elapsed) = datetime.duration_since(now) {
        let seconds = elapsed.as_secs();
        format!(
            "-{:02}:{:02}:{:02}",
            seconds / 3600,
            (seconds % 3600) / 60,
            seconds % 60
        )
    } else {
        "Error in computing time".to_string()
    };

    return Stringy::from(data);
}
