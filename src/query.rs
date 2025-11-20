// File: src/pro/query_engine.rs

use crate::time as time_parser;
use serde_json::Value;
use std::fmt;
use regex::Regex; // ADDED

const OPERATORS: &[&str] = &[
    // Longer operators first to avoid substring matching issues
    "!contains+", "!contains-", // ADDED
    "!~=", "!contains", "!exists", "isnot", ">=", "<=", "==", "!=",
    "contains+", "contains-", // ADDED
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
fn extract_numbers(text: &str) -> Vec<f64> {
    // This regex finds numbers like -10, 500, 123.45
    let re = match Regex::new(r"-?\d+(\.\d+)?") {
        Ok(r) => r,
        Err(_) => return Vec::new(), // Should not happen with this regex
    };
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
            let field = condition.split(op).next().unwrap_or("").trim();
            
            let field_exists = get_value_by_field(value, field).is_some();

            return if *op == "exists" {
                Ok(field_exists)
            } else {
                Ok(!field_exists)
            };
        }

        let (field, op_str, query_value_str) = {
            let parts: Vec<&str> = condition.splitn(2, op).map(|s| s.trim()).collect();
            if parts.len() < 2 {
                return Err(QueryError::InvalidFormat(condition.to_string()));
            }
            (parts[0], *op, parts[1])
        };

        if TIMESTAMP_KEYS.contains(&field) {
            return match compare_time_values(value, query_value_str) {
                Some(ord) => match op_str {
                    ">" => Ok(ord == std::cmp::Ordering::Greater),
                    "<" => Ok(ord == std::cmp::Ordering::Less),
                    ">=" => Ok(ord != std::cmp::Ordering::Less),
                    "<=" => Ok(ord != std::cmp::Ordering::Greater),
                    _ => Err(QueryError::InvalidFormat(
                        "Timestamp fields only support >, <, >=, <= operators.".to_string(),
                    )),
                },
                None => Ok(false),
            };
        }

        // --- MODIFIED BLOCK ---
        if field == "text" {
            let search_value_clean = query_value_str
                .trim()
                .trim_matches(|c| c == '"' || c == '\'');

            return match op_str {
                "contains" | "!contains" => {
                    // Existing logic for substring search
                    let lower_raw_line = raw_line.to_lowercase();
                    let search_terms: Vec<String> = query_value_str // Use original query_value_str
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
                        // !contains
                        Ok(search_terms
                            .iter()
                            .all(|term| !lower_raw_line.contains(term)))
                    }
                }
                "contains+" | "!contains+" | "contains-" | "!contains-" => {
                    // New logic for number comparison
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
                        "contains+" => {
                            // Any number in line is >= query_num
                            Ok(numbers_in_line.iter().any(|&n| n >= query_num))
                        }
                        "!contains+" => {
                            // No number in line is >= query_num (i.e., all are <)
                            Ok(numbers_in_line.iter().all(|&n| n < query_num))
                        }
                        "contains-" => {
                            // Any number in line is <= query_num
                            Ok(numbers_in_line.iter().any(|&n| n <= query_num))
                        }
                        "!contains-" => {
                            // No number in line is <= query_num (i.e., all are >)
                            Ok(numbers_in_line.iter().all(|&n| n > query_num))
                        }
                        _ => unreachable!(), // We are inside this match arm
                    }
                }
                _ => Err(QueryError::InvalidFormat(
                    "The 'text' field only supports 'contains', '!contains', 'contains+', '!contains+', 'contains-', '!contains-' operators."
                        .to_string(),
                )),
            };
        }
        // --- END MODIFIED BLOCK ---

        if let Some(log_value) = get_value_by_field(value, field) {
            // Field EXISTS
            return match op_str {
                "~=" => Ok(compare_values(log_value, query_value_str, true) == Some(std::cmp::Ordering::Equal)),
                "!~=" => Ok(compare_values(log_value, query_value_str, true) != Some(std::cmp::Ordering::Equal)),
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