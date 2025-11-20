use wasm_bindgen::prelude::*;
use serde_json::Value;
use crate::{parsers, query, LogEntry};

// This struct helps the JavaScript frontend understand the result easily.
// We derive Serialize so we can return it as a JSON string.
#[derive(serde::Serialize)]
struct WasmResult {
    is_match: bool,
    parsed_log: Option<Value>, // Returns the structured JSON/Logfmt object
    error: Option<String>,     // Returns query syntax errors if any
}

#[wasm_bindgen]
pub fn run_query(log_line: &str, query: &str) -> String {
    // 1. Parse the log line (Automatic detection)
    let entry = parsers::parse_log_line(log_line);

    match entry {
        LogEntry::Structured(value) => {
            // 2. Run the query against the structured data
            match query::evaluate(&value, log_line, query) {
                Ok(is_match) => {
                    let result = WasmResult {
                        is_match,
                        parsed_log: Some(value),
                        error: None,
                    };
                    serde_json::to_string(&result).unwrap_or_default()
                },
                Err(e) => {
                    // Query syntax error (e.g., missing quote)
                    let result = WasmResult {
                        is_match: false,
                        parsed_log: Some(value), // We still return the data so the user sees how it was parsed
                        error: Some(format!("Query Error: {}", e)),
                    };
                    serde_json::to_string(&result).unwrap_or_default()
                }
            }
        },
        LogEntry::Unstructured(_) => {
            // Parsing failed (not JSON or Logfmt)
            let result = WasmResult {
                is_match: false,
                parsed_log: None,
                error: Some("Could not parse log structure. Is it valid JSON or Logfmt?".to_string()),
            };
            serde_json::to_string(&result).unwrap_or_default()
        }
    }
}