// src/pro/time_parser.rs

use chrono::{DateTime, Utc, TimeZone};
use humantime::parse_duration;
use serde_json::Value;
use std::time::SystemTime;

/// Parses a user-provided time string into a DateTime object.
/// Handles both relative times ("1h ago") and absolute timestamps.
pub fn parse_time_string(time_str: &str) -> Result<DateTime<Utc>, String> {
    if time_str.to_lowercase() == "now" {
        return Ok(Utc::now());
    }

    // Try parsing as a relative duration (e.g., "15m", "2h ago")
    let clean_str = time_str.strip_suffix(" ago").unwrap_or(time_str);
    if let Ok(duration) = parse_duration(clean_str) {
        let now = SystemTime::now();
        let target_time = now - duration;
        return Ok(target_time.into());
    }

    // Try parsing as an absolute timestamp (RFC3339 / ISO 8601)
    if let Ok(datetime) = DateTime::parse_from_rfc3339(time_str) {
        return Ok(datetime.with_timezone(&Utc));
    }

    Err(format!("Could not parse time string: {}", time_str))
}

/// Extracts and parses a timestamp from a JSON log entry.
/// Tries a list of common timestamp field names.
pub fn extract_and_parse_timestamp(value: &Value) -> Option<DateTime<Utc>> {
    const COMMON_KEYS: [&str; 3] = ["timestamp", "ts", "@timestamp"];

    for key in COMMON_KEYS {
        if let Some(ts_value) = value.get(key) {
            if let Some(ts_str) = ts_value.as_str() {
                // Parse string timestamp
                if let Ok(datetime) = DateTime::parse_from_rfc3339(ts_str) {
                    return Some(datetime.with_timezone(&Utc));
                }
            } else if let Some(ts_unix) = ts_value.as_i64() {
                // Parse Unix timestamp (seconds)
                if let Some(datetime) = Utc.timestamp_opt(ts_unix, 0).single() {
                    return Some(datetime);
                }
            }
        }
    }
    None
}
