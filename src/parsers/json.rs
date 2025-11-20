// src/parsers/json.rs

use serde_json::{Result, Value};

/// Attempts to parse a single line as a JSON object.
pub fn parse_json_line(line: &str) -> Result<Value> {
    serde_json::from_str(line)
}
