// File: src/parsers/mod.rs

pub mod json;
pub mod logfmt;
pub mod nginx; // ADDED

use serde_json::Value;

/// A universal representation of a single log line.
#[derive(Debug)]
pub enum LogEntry {
    Structured(Value), // For JSON, Nginx, or other structured formats
    Unstructured(String), // For plain text
}

/// Parses a single line of text into a LogEntry using better heuristics.
pub fn parse_log_line(line: &str) -> LogEntry {
    let trimmed = line.trim();

    // 1. Strict JSON check.
    if trimmed.starts_with('{') && trimmed.ends_with('}') {
        if let Ok(json_val) = json::parse_json_line(trimmed) {
            return LogEntry::Structured(json_val);
        }
    }

    // 2. Nginx / Common Log Format check.
    // Heuristic: Starts with a number (IP) and contains standard date brackets `[`
    if (trimmed.starts_with(|c: char| c.is_ascii_digit()) || trimmed.starts_with(":")) 
        && trimmed.contains(" - - [") {
        if let Some(nginx_val) = nginx::parse_nginx_line(trimmed) {
            return LogEntry::Structured(nginx_val);
        }
    }

    // 3. Heuristic logfmt check.
    if trimmed.contains('=') {
        if let Ok(logfmt_val) = logfmt::parse_logfmt_line(trimmed) {
            if let Some(map) = logfmt_val.as_object() {
                if !map.is_empty() {
                    let total_keys = map.len();
                    let null_value_keys = map.values().filter(|v| v.is_null()).count();
                    // Basic heuristic: If less than half the keys have null values, it's likely logfmt
                    if null_value_keys < total_keys / 2 {
                        return LogEntry::Structured(logfmt_val);
                    }
                }
            }
        }
    }

    // 4. If all else fails, treat it as unstructured text.
    LogEntry::Unstructured(line.to_string())
}