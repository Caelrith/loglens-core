// File: src/parsers/nginx.rs

use serde_json::{Map, Value};
use chrono::DateTime;

/// Optimised linear scanner.
/// It manually finds delimiters (' ', '[', '"') to slice the string.
/// This avoids the overhead of the Regex engine entirely.
pub fn parse_nginx_line(line: &str) -> Option<Value> {
    let mut remainder = line;

    // 1. Remote Addr (Stop at first space)
    let (remote_addr, rest) = split_once_char(remainder, ' ')?;
    remainder = rest.trim_start_matches('-'); // Skip the dash
    remainder = remainder.trim_start();       // Skip spaces

    // 2. Remote User (Stop at next space)
    // Usually "-" but could be a username
    let (_remote_user, rest) = split_once_char(remainder, ' ')?;
    remainder = rest.trim_start();

    // 3. Time (Between [ and ])
    if !remainder.starts_with('[') { return None; }
    let end_bracket = remainder.find(']')?;
    let raw_time = &remainder[1..end_bracket];
    remainder = &remainder[end_bracket+1..].trim_start();

    // 4. Request "METHOD PATH PROTO" (Between " and ")
    if !remainder.starts_with('"') { return None; }
    // Find the closing quote. We can't just look for " because the URL might contain escaped quotes
    // But for standard Nginx logs, looking for the next " is usually safe. 
    // For robustness, we look for the quote followed by a space.
    let end_quote = remainder[1..].find('"')? + 1; 
    let request_line = &remainder[1..end_quote];
    remainder = &remainder[end_quote+1..].trim_start();

    // Parse Request Line parts
    let mut req_parts = request_line.split_whitespace();
    let method = req_parts.next().unwrap_or("-");
    let path = req_parts.next().unwrap_or("-");
    // Protocol is the rest, or "-"
    
    // 5. Status (Int)
    let (status_str, rest) = split_once_char(remainder, ' ')?;
    remainder = rest;

    // 6. Body Bytes (Int)
    let (bytes_str, rest) = split_once_char(remainder, ' ')?;
    remainder = rest;

    // 7. Referer (Between " and ")
    let (referer, rest) = extract_quoted(remainder)?;
    remainder = rest;

    // 8. User Agent (Between " and ")
    let (ua, rest) = extract_quoted(remainder)?;
    remainder = rest;

    // 9. X-Forwarded-For (Optional, quoted)
    let x_forwarded = if !remainder.is_empty() {
        extract_quoted(remainder).map(|(val, _)| val)
    } else {
        None
    };

    // --- Construction ---

    let mut map = Map::with_capacity(10);
    
    map.insert("remote_addr".to_string(), Value::String(remote_addr.to_string()));
    map.insert("time_local".to_string(), Value::String(raw_time.to_string()));

    // Date Parsing (The heaviest part, but necessary for stats)
    if let Ok(dt) = DateTime::parse_from_str(raw_time, "%d/%b/%Y:%H:%M:%S %z") {
        map.insert("timestamp".to_string(), Value::String(dt.to_rfc3339()));
    } else {
        map.insert("timestamp".to_string(), Value::String(raw_time.to_string()));
    }

    map.insert("method".to_string(), Value::String(method.to_string()));
    map.insert("path".to_string(), Value::String(path.to_string()));

    if let Ok(n) = status_str.parse::<u64>() {
        map.insert("status".to_string(), Value::Number(n.into()));
        // Helper level
        let level = if n >= 500 { "ERROR" } else if n >= 400 { "WARN" } else { "INFO" };
        map.insert("level".to_string(), Value::String(level.to_string()));
    }

    if let Ok(n) = bytes_str.parse::<u64>() {
        map.insert("body_bytes_sent".to_string(), Value::Number(n.into()));
    }

    map.insert("http_referer".to_string(), Value::String(referer.to_string()));
    map.insert("http_user_agent".to_string(), Value::String(ua.to_string()));

    if let Some(xf) = x_forwarded {
        map.insert("x_forwarded_for".to_string(), Value::String(xf.to_string()));
    }

    Some(Value::Object(map))
}

// --- Helpers ---

#[inline(always)]
fn split_once_char(s: &str, delimiter: char) -> Option<(&str, &str)> {
    let idx = s.find(delimiter)?;
    Some((&s[..idx], &s[idx + 1..]))
}

#[inline(always)]
fn extract_quoted(s: &str) -> Option<(&str, &str)> {
    let start = s.find('"')?;
    let remainder_after_start = &s[start + 1..];
    let end = remainder_after_start.find('"')?;
    
    let content = &remainder_after_start[..end];
    let rest = &remainder_after_start[end + 1..].trim_start();
    
    Some((content, rest))
}