// src/parsers/logfmt.rs

use serde_json::{Map, Value};

/// Attempts to parse a single line as logfmt using the correct library API.
pub fn parse_logfmt_line(line: &str) -> Result<Value, String> {
    // 1. The `parse` function directly returns a Vec<Pair>.
    let pairs = logfmt::parse(line);

    // 2. If the vector is empty, it means nothing was parsed.
    if pairs.is_empty() {
        return Err("Not a valid logfmt line.".to_string());
    }

    let mut map = Map::new();

    // 3. We can now iterate directly over the successful pairs.
    for pair in pairs {
        // The `pair.val` is an Option<String>, which we convert to a serde_json::Value
        let value = match pair.val {
            Some(v) => Value::String(v),
            None => Value::Null,
        };
        map.insert(pair.key, value);
    }

    Ok(Value::Object(map))
}
