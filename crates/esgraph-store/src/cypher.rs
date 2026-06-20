//! Cypher literal helpers for Ladybug ingest.

use lbug::Value;

/// Escape a string for use inside double-quoted Cypher literals.
pub fn cypher_str(s: &str) -> String {
    format!(
        "\"{}\"",
        s.replace('\\', "\\\\").replace('"', "\\\"")
    )
}

pub fn cypher_i64(v: i64) -> String {
    v.to_string()
}

pub fn cypher_bool(v: bool) -> String {
    if v { "true".into() } else { "false".into() }
}

/// Format an optional string property assignment (`n.key = "..."`).
pub fn opt_str_assign(prefix: &str, key: &str, value: Option<&str>) -> Option<String> {
    value.map(|v| format!("{prefix}.{key} = {}", cypher_str(v)))
}

/// Format an optional integer property assignment.
pub fn opt_i64_assign(prefix: &str, key: &str, value: Option<i64>) -> Option<String> {
    value.map(|v| format!("{prefix}.{key} = {}", cypher_i64(v)))
}

/// Format an optional boolean property assignment.
pub fn opt_bool_assign(prefix: &str, key: &str, value: Option<bool>) -> Option<String> {
    value.map(|v| format!("{prefix}.{key} = {}", cypher_bool(v)))
}

/// Join `prop = value` fragments into a SET clause body.
pub fn join_set_clauses(clauses: &[String]) -> String {
    clauses.join(", ")
}

/// Convert a Ladybug [`Value`] to a display string for CLI output.
pub fn value_to_string(value: &Value) -> String {
    match value {
        Value::Null(_) => String::new(),
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// Parse the first column of a count query as `u64`.
pub fn parse_count_value(value: &Value) -> Option<u64> {
    match value {
        Value::Int64(v) if *v >= 0 => Some(*v as u64),
        Value::UInt64(v) => Some(*v),
        Value::Int32(v) if *v >= 0 => Some(*v as u64),
        _ => value_to_string(value).parse().ok(),
    }
}
