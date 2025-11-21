// File: src/engine.rs

use crate::time as time_parser;
use serde_json::Value;
use std::fmt;
use regex::Regex;
use std::sync::OnceLock;

const OPERATORS: &[&str] = &[
    // Longer operators first to avoid substring matching issues
    "!contains+", "!contains-",
    "!between", // Range exclusion
    "!~=", "!contains", "!exists", "isnot", ">=", "<=", "==", "!=",
    "contains+", "contains-",
    "between", // Range inclusion
    "contains", "exists",
    // Shorter operators last
    "is", "~=", ">", "<",
];
const TIMESTAMP_KEYS: &[&str] = &["timestamp", "ts", "@timestamp"];

fn get_value_by_field<'a>(val: &'a Value, field_key: &str) -> Option<&'a Value> {
    if field_key.starts_with('/') {
        val.pointer(field_key)
    } else {
        val.get(field_key)
    }
}

/// Extracts all numbers (integers, floats, negatives) from a text string.
/// Optimized to compile the Regex only once.
fn extract_numbers(text: &str) -> Vec<f64> {
    static NUMBER_REGEX: OnceLock<Regex> = OnceLock::new();
    
    let re = NUMBER_REGEX.get_or_init(|| {
        Regex::new(r"-?\d+(\.\d+)?").expect("Invalid number regex")
    });

    re.find_iter(text)
        .filter_map(|mat| mat.as_str().parse::<f64>().ok())
        .collect()
}

#[derive(Debug)]
pub enum QueryError {
    InvalidFormat(String),
}

impl fmt::Display for QueryError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            QueryError::InvalidFormat(q) => write!(f, "Invalid query format: '{}'", q),
        }
    }
}

impl std::error::Error for QueryError {}

fn evaluate_and_clause(value: &Value, raw_line: &str, clause: &str) -> Result<bool, QueryError> {
    let conditions = clause.split("&&").map(|s| s.trim());
    for condition in conditions {
        if condition.is_empty() {
            continue;
        }
        let result = evaluate_single_condition(value, raw_line, condition)?;
        if !result {
            return Ok(false);
        }
    }
    Ok(true)
}

pub fn evaluate(value: &Value, raw_line: &str, query: &str) -> Result<bool, QueryError> {
    if query.trim().is_empty() {
        return Ok(true);
    }
    
    let is_structured_query = OPERATORS.iter().any(|op| query.contains(op));

    if !is_structured_query {
        let mut effective_query = query;
        let negate = query.starts_with('!');
        if negate {
            effective_query = &query[1..];
        }
        let matches = raw_line
            .to_lowercase()
            .contains(&effective_query.to_lowercase());
        return Ok(if negate { !matches } else { matches });
    }

    let normalized_query = query
        .replace(" OR ", "||")
        .replace(" or ", "||")
        .replace(" AND ", "&&")
        .replace(" and ", "&&");

    let or_clauses = normalized_query.split("||").map(|s| s.trim());

    for or_clause in or_clauses {
        if or_clause.is_empty() {
            continue;
        }
        if evaluate_and_clause(value, raw_line, or_clause)? {
            return Ok(true);
        }
    }
    
    Ok(false)
}

// --- Helper for BETWEEN operator logic ---
fn evaluate_between(
    log_value: &Value, 
    range_str: &str, 
    is_timestamp: bool
) -> Result<bool, QueryError> {
    let parts: Vec<&str> = range_str.split("..").collect();
    
    if parts.len() != 2 {
        return Err(QueryError::InvalidFormat(format!(
            "BETWEEN operator requires a range 'start..end'. Got: '{}'", 
            range_str
        )));
    }

    let start_str = parts[0].trim().trim_matches(|c| c == '"' || c == '\'');
    let end_str = parts[1].trim().trim_matches(|c| c == '"' || c == '\'');

    if is_timestamp {
        let log_time = match time_parser::extract_and_parse_timestamp(log_value) {
            Some(t) => t,
            None => return Ok(false), 
        };
        
        let t1 = time_parser::parse_time_string(start_str)
            .map_err(|_| QueryError::InvalidFormat(format!("Invalid start time: {}", start_str)))?;
        
        let t2 = time_parser::parse_time_string(end_str)
            .map_err(|_| QueryError::InvalidFormat(format!("Invalid end time: {}", end_str)))?;

        // AUTO-SWAP LOGIC: Ensure we always compare Low..High
        let (start, end) = if t1 < t2 { (t1, t2) } else { (t2, t1) };

        Ok(log_time >= start && log_time <= end)
    } else {
        // Numeric comparison
        if let Some(log_num) = log_value.as_f64() {
            let n1 = start_str.parse::<f64>()
                .map_err(|_| QueryError::InvalidFormat(format!("Invalid start number: {}", start_str)))?;
            let n2 = end_str.parse::<f64>()
                .map_err(|_| QueryError::InvalidFormat(format!("Invalid end number: {}", end_str)))?;

            let (start, end) = if n1 < n2 { (n1, n2) } else { (n2, n1) };

            Ok(log_num >= start && log_num <= end)
        } else {
            // String fallback (Lexicographical)
             if let Some(log_s) = log_value.as_str() {
                 Ok(log_s >= start_str && log_s <= end_str)
             } else {
                 Ok(false)
             }
        }
    }
}

fn compare_time_values(
    log_entry: &Value,
    query_time_str_raw: &str,
) -> Option<std::cmp::Ordering> {
    let log_time = time_parser::extract_and_parse_timestamp(log_entry)?;
    let query_time_str_clean = query_time_str_raw
        .trim()
        .trim_matches(|c| c == '"' || c == '\'');
    let query_time = time_parser::parse_time_string(query_time_str_clean).ok()?;
    log_time.partial_cmp(&query_time)
}

fn evaluate_single_condition(
    value: &Value,
    raw_line: &str,
    condition: &str,
) -> Result<bool, QueryError> {
    let operator = OPERATORS.iter().find(|&&op| condition.contains(op));

    if let Some(op) = operator {
        if *op == "exists" || *op == "!exists" {
            let field_part = condition.split(op).next().unwrap_or("").trim();
            
            // Handle basic num() stripping for exists check, though redundant logically
            let field = if field_part.starts_with("num(") && field_part.ends_with(')') {
                field_part[4..field_part.len()-1].trim()
            } else {
                field_part
            };

            let field_exists = get_value_by_field(value, field).is_some();

            return if *op == "exists" {
                Ok(field_exists)
            } else {
                Ok(!field_exists)
            };
        }

        let (field_raw, op_str, query_value_str) = {
            let parts: Vec<&str> = condition.splitn(2, op).map(|s| s.trim()).collect();
            if parts.len() < 2 {
                return Err(QueryError::InvalidFormat(condition.to_string()));
            }
            (parts[0], *op, parts[1])
        };

        // --- 1. Parse "num()" modifier ---
        let (field, force_numeric) = if field_raw.starts_with("num(") && field_raw.ends_with(')') {
            (field_raw[4..field_raw.len()-1].trim(), true)
        } else {
            (field_raw, false)
        };

        // --- 2. Handle BETWEEN for timestamps explicitly ---
        if TIMESTAMP_KEYS.contains(&field) {
             if op_str == "between" {
                 return evaluate_between(value, query_value_str, true);
             }
             if op_str == "!between" {
                 return evaluate_between(value, query_value_str, true).map(|b| !b);
             }
        }

        // --- 3. Standard Timestamp operators ---
        if TIMESTAMP_KEYS.contains(&field) {
            return match compare_time_values(value, query_value_str) {
                Some(ord) => match op_str {
                    ">" => Ok(ord == std::cmp::Ordering::Greater),
                    "<" => Ok(ord == std::cmp::Ordering::Less),
                    ">=" => Ok(ord != std::cmp::Ordering::Less),
                    "<=" => Ok(ord != std::cmp::Ordering::Greater),
                    _ => Err(QueryError::InvalidFormat(
                        "Timestamp fields only support >, <, >=, <=, between operators.".to_string(),
                    )),
                },
                None => Ok(false),
            };
        }

        // --- 4. "text" field logic (Searching raw line) ---
        if field == "text" {
            let search_value_clean = query_value_str
                .trim()
                .trim_matches(|c| c == '"' || c == '\'');

            return match op_str {
                "contains" | "!contains" => {
                    let lower_raw_line = raw_line.to_lowercase();
                    let search_terms: Vec<String> = query_value_str
                        .split(',')
                        .map(|s| {
                            s.trim()
                                .trim_matches(|c| c == '"' || c == '\'')
                                .to_lowercase()
                        })
                        .filter(|s| !s.is_empty())
                        .collect();

                    if search_terms.is_empty() {
                        return Ok(true);
                    }

                    if op_str == "contains" {
                        Ok(search_terms
                            .iter()
                            .all(|term| lower_raw_line.contains(term)))
                    } else {
                        Ok(search_terms
                            .iter()
                            .all(|term| !lower_raw_line.contains(term)))
                    }
                }
                // Support for 'text between 100..200'
                "between" | "!between" => {
                    let parts: Vec<&str> = query_value_str.split("..").collect();
                    if parts.len() != 2 {
                        return Err(QueryError::InvalidFormat(format!(
                            "Operator '{}' requires a range 'start..end'. Got: '{}'",
                            op_str, query_value_str
                        )));
                    }

                    let s1 = parts[0].trim().trim_matches(|c| c == '"' || c == '\'');
                    let s2 = parts[1].trim().trim_matches(|c| c == '"' || c == '\'');

                    let n1 = s1.parse::<f64>().map_err(|_| {
                        QueryError::InvalidFormat(format!("Invalid start number: {}", s1))
                    })?;
                    let n2 = s2.parse::<f64>().map_err(|_| {
                        QueryError::InvalidFormat(format!("Invalid end number: {}", s2))
                    })?;

                    // Auto-swap for safety
                    let (start, end) = if n1 < n2 { (n1, n2) } else { (n2, n1) };

                    // Extract all numbers from the raw line
                    let numbers_in_line = extract_numbers(raw_line);

                    // Check if ANY number in the line is within the range
                    let any_match = numbers_in_line.iter().any(|&n| n >= start && n <= end);

                    if op_str == "between" {
                        Ok(any_match)
                    } else {
                        Ok(!any_match) 
                    }
                }
                "contains+" | "!contains+" | "contains-" | "!contains-" => {
                    let query_num = match search_value_clean.parse::<f64>() {
                        Ok(n) => n,
                        Err(_) => {
                            return Err(QueryError::InvalidFormat(format!(
                                "Operator '{}' requires a numeric value, but got '{}'",
                                op_str, query_value_str
                            )));
                        }
                    };

                    let numbers_in_line = extract_numbers(raw_line);

                    match op_str {
                        "contains+" => Ok(numbers_in_line.iter().any(|&n| n >= query_num)),
                        "!contains+" => Ok(numbers_in_line.iter().all(|&n| n < query_num)),
                        "contains-" => Ok(numbers_in_line.iter().any(|&n| n <= query_num)),
                        "!contains-" => Ok(numbers_in_line.iter().all(|&n| n > query_num)),
                        _ => unreachable!(),
                    }
                }
                _ => Err(QueryError::InvalidFormat(
                    "The 'text' field only supports 'contains' and 'between' variations.".to_string(),
                )),
            };
        }

        // --- 5. Standard Field Logic ---
        if let Some(original_value) = get_value_by_field(value, field) {
            
            // Handle "num(field)" conversion logic
            let temp_numeric_value; 
            let log_value = if force_numeric {
                if let Some(_) = original_value.as_f64() {
                    original_value // Already a number
                } else if let Some(s) = original_value.as_str() {
                    // Try parsing string as float
                    match s.parse::<f64>() {
                        Ok(n) if n.is_finite() => {
                            temp_numeric_value = Some(Value::from(n));
                            temp_numeric_value.as_ref().unwrap()
                        },
                        _ => return Ok(false) // Cannot force to number -> No match
                    }
                } else {
                    // Booleans, Arrays, Objects cannot be forced to simple numbers for comparison
                    return Ok(false)
                }
            } else {
                original_value
            };

            // Field EXISTS and value prepared
            return match op_str {
                "between" => evaluate_between(log_value, query_value_str, false),
                "!between" => evaluate_between(log_value, query_value_str, false).map(|b| !b),

                "~=" => Ok(compare_values(log_value, query_value_str, true) == Some(std::cmp::Ordering::Equal)),
                "!~=" => Ok(compare_values(log_value, query_value_str, true) != Some(std::cmp::Ordering::Equal)),
                
                "contains" => {
                    let query_clean = query_value_str.trim().trim_matches(|c| c == '"' || c == '\'');
                    match log_value {
                        Value::String(s) => Ok(s.contains(query_clean)),
                        _ => Ok(false),
                    }
                },
                "!contains" => {
                    let query_clean = query_value_str.trim().trim_matches(|c| c == '"' || c == '\'');
                    match log_value {
                        Value::String(s) => Ok(!s.contains(query_clean)),
                        _ => Ok(true),
                    }
                },

                "==" | "is" => Ok(compare_values(log_value, query_value_str, false) == Some(std::cmp::Ordering::Equal)),
                "!=" | "isnot" => Ok(compare_values(log_value, query_value_str, false) != Some(std::cmp::Ordering::Equal)),
                ">" => Ok(compare_values(log_value, query_value_str, false) == Some(std::cmp::Ordering::Greater)),
                "<" => Ok(compare_values(log_value, query_value_str, false) == Some(std::cmp::Ordering::Less)),
                ">=" => Ok(compare_values(log_value, query_value_str, false).map_or(false, |ord| ord != std::cmp::Ordering::Less)),
                "<=" => Ok(compare_values(log_value, query_value_str, false).map_or(false, |ord| ord != std::cmp::Ordering::Greater)),
                _ => Ok(false),
            };
        } else {
            // Field DOES NOT EXIST
            return match op_str {
                "!=" | "isnot" => Ok(true),
                _ => Ok(false),
            };
        }
    } else {
        Err(QueryError::InvalidFormat(condition.to_string()))
    }
}

fn compare_values(
    log_value: &Value,
    query_value_str_raw: &str,
    case_insensitive: bool,
) -> Option<std::cmp::Ordering> {
    let query_value_clean = query_value_str_raw
        .trim()
        .trim_matches(|c| c == '"' || c == '\'');

    if let Some(log_num) = log_value.as_f64() {
        if let Ok(query_num) = query_value_clean.parse::<f64>() {
            return log_num.partial_cmp(&query_num);
        }
    }

    let log_str_equivalent = match log_value {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        _ => return None,
    };

    if case_insensitive {
        Some(
            log_str_equivalent
                .to_lowercase()
                .as_str()
                .cmp(&query_value_clean.to_lowercase()),
        )
    } else {
        Some(log_str_equivalent.as_str().cmp(query_value_clean))
    }
}