// File: src/parsers/mod.rs

pub mod json;
pub mod logfmt;

use serde_json::Value;

/// A universal representation of a single log line.
#[derive(Debug)]
pub enum LogEntry {
    Structured(Value), // For JSON or other structured formats
    Unstructured(String), // For plain text
}

/// Parses a single line of text into a LogEntry using better heuristics.
pub fn parse_log_line(line: &str) -> LogEntry {
    let trimmed = line.trim();

    // 1. Strict JSON check. If it looks like JSON, it MUST be valid JSON.
    // This prevents invalid JSON from being passed to other parsers.
    if trimmed.starts_with('{') && trimmed.ends_with('}') {
        return match json::parse_json_line(trimmed) {
            Ok(json_val) => {
                // Successfully parsed JSON
                LogEntry::Structured(json_val)
            }
            Err(_) => {
                // Failed to parse as JSON, treat as malformed/unstructured
                LogEntry::Unstructured(line.to_string())
            }
        };
    }

    // 2. Heuristic logfmt check.
    if trimmed.contains('=') {
        if let Ok(logfmt_val) = logfmt::parse_logfmt_line(trimmed) {
            if let Some(map) = logfmt_val.as_object() {
                if !map.is_empty() {
                    let total_keys = map.len();
                    let null_value_keys = map.values().filter(|v| v.is_null()).count();
                    // Basic heuristic: If less than half the keys have null values, it's likely logfmt
                    if null_value_keys < total_keys / 2 {
                        // Parsed as logfmt
                        return LogEntry::Structured(logfmt_val);
                    }
                }
            }
        }
    }

    // 3. If all else fails, treat it as unstructured text.
    LogEntry::Unstructured(line.to_string())
}