//! Advanced Metadata Filtering for MemFuse.
// ANCHOR:ARCH:FILTER-001 — Metadata Filtering Logic.
// WP:WP-4.2 PRIO:2 NEEDS:COLLECTION-001
// AGENT:04 DATE:2026-05-22 STATUS:READY

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Represents a filter criteria for document metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Filter {
    /// key == value
    Eq(String, Value),
    /// key != value
    Ne(String, Value),
    /// key > value (only for numbers and strings)
    Gt(String, Value),
    /// key < value (only for numbers and strings)
    Lt(String, Value),
    /// key >= value (only for numbers and strings)
    Gte(String, Value),
    /// key <= value (only for numbers and strings)
    Lte(String, Value),
    /// All filters must match
    And(Vec<Filter>),
    /// At least one filter must match
    Or(Vec<Filter>),
    /// Negate the filter
    Not(Box<Filter>),
    /// Key exists in metadata
    Exists(String),
}

impl Filter {
    /// Evaluates the filter against the given metadata.
    pub fn matches(&self, metadata: &Option<Value>) -> bool {
        match self {
            Filter::Eq(key, val) => {
                metadata.as_ref().and_then(|m| m.get(key)) == Some(val)
            }
            Filter::Ne(key, val) => {
                metadata.as_ref().and_then(|m| m.get(key)) != Some(val)
            }
            Filter::Gt(key, val) => {
                if let Some(m_val) = metadata.as_ref().and_then(|m| m.get(key)) {
                    match (m_val, val) {
                        (Value::Number(a), Value::Number(b)) => {
                            a.as_f64().unwrap_or(0.0) > b.as_f64().unwrap_or(0.0)
                        }
                        (Value::String(a), Value::String(b)) => a > b,
                        _ => false,
                    }
                } else {
                    false
                }
            }
            Filter::Lt(key, val) => {
                if let Some(m_val) = metadata.as_ref().and_then(|m| m.get(key)) {
                    match (m_val, val) {
                        (Value::Number(a), Value::Number(b)) => {
                            a.as_f64().unwrap_or(0.0) < b.as_f64().unwrap_or(0.0)
                        }
                        (Value::String(a), Value::String(b)) => a < b,
                        _ => false,
                    }
                } else {
                    false
                }
            }
            Filter::Gte(key, val) => {
                if let Some(m_val) = metadata.as_ref().and_then(|m| m.get(key)) {
                    match (m_val, val) {
                        (Value::Number(a), Value::Number(b)) => {
                            a.as_f64().unwrap_or(0.0) >= b.as_f64().unwrap_or(0.0)
                        }
                        (Value::String(a), Value::String(b)) => a >= b,
                        _ => m_val == val,
                    }
                } else {
                    false
                }
            }
            Filter::Lte(key, val) => {
                if let Some(m_val) = metadata.as_ref().and_then(|m| m.get(key)) {
                    match (m_val, val) {
                        (Value::Number(a), Value::Number(b)) => {
                            a.as_f64().unwrap_or(0.0) <= b.as_f64().unwrap_or(0.0)
                        }
                        (Value::String(a), Value::String(b)) => a <= b,
                        _ => m_val == val,
                    }
                } else {
                    false
                }
            }
            Filter::And(filters) => filters.iter().all(|f| f.matches(metadata)),
            Filter::Or(filters) => filters.iter().any(|f| f.matches(metadata)),
            Filter::Not(filter) => !filter.matches(metadata),
            Filter::Exists(key) => {
                metadata.as_ref().and_then(|m| m.get(key)).is_some()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_filter_eq() {
        let meta = Some(json!({"topic": "rust", "priority": 1}));
        assert!(Filter::Eq("topic".into(), json!("rust")).matches(&meta));
        assert!(!Filter::Eq("topic".into(), json!("python")).matches(&meta));
        assert!(Filter::Eq("priority".into(), json!(1)).matches(&meta));
    }

    #[test]
    fn test_filter_logic() {
        let meta = Some(json!({"age": 30, "city": "Berlin"}));
        let filter = Filter::And(vec![
            Filter::Gt("age".into(), json!(25)),
            Filter::Eq("city".into(), json!("Berlin")),
        ]);
        assert!(filter.matches(&meta));

        let filter_or = Filter::Or(vec![
            Filter::Eq("city".into(), json!("Munich")),
            Filter::Lt("age".into(), json!(40)),
        ]);
        assert!(filter_or.matches(&meta));
    }
}
