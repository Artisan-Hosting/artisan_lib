use std::time::{Duration, SystemTime, UNIX_EPOCH};

use chrono::{Local, NaiveDateTime, TimeZone, Utc};
use dusa_collection_utils::{log, logger::LogLevel, types::stringy::Stringy};

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
pub fn timesince_unix_timestamp(timestamp: u64) -> Stringy {
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

/// Converts a `u64` Unix timestamp (seconds since epoch) into
/// a human-readable string in UTC, e.g. "2025-02-07 14:05:00".
pub fn format_unix_timestamp(timestamp: u64) -> String {
    let utc_datetime = Utc.timestamp_opt(timestamp as i64, 0).single();

    match utc_datetime {
        Some(dt_utc) => {
            // Convert that UTC datetime to the local timezone.
            let local_time = dt_utc.with_timezone(&Local);
            // Format as desired
            local_time.format("%Y-%m-%d %H:%M:%S").to_string()
        }
        None => "Invalid timestamp".to_string(),
    }
}


pub fn time_to_unix_timestamp(datetime: &str) -> Option<u64> {
    match NaiveDateTime::parse_from_str(datetime, "%Y-%m-%d %H:%M:%S") {
        Ok(_) => todo!(),
        Err(err) => {
            log!(LogLevel::Error, "Error converting time to timestamp: {}", err.to_string());
            None
        },
    }
}