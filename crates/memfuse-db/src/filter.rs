//! Advanced Metadata Filtering for MemFuse.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Operators for metadata filtering.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MetadataFilter {
    /// field == value
    Eq(String, Value),
    /// field != value
    Ne(String, Value),
    /// field > value (numeric)
    Gt(String, Value),
    /// field >= value (numeric)
    Gte(String, Value),
    /// field < value (numeric)
    Lt(String, Value),
    /// field <= value (numeric)
    Lte(String, Value),
    /// field in [value1, value2, ...]
    In(String, Vec<Value>),
    /// Logical AND of multiple filters
    And(Vec<MetadataFilter>),
    /// Logical OR of multiple filters
    Or(Vec<MetadataFilter>),
    /// Logical NOT of a filter
    Not(Box<MetadataFilter>),
}

impl MetadataFilter {
    /// Evaluates the filter against the provided metadata.
    pub fn matches(&self, metadata: &Value) -> bool {
        match self {
            MetadataFilter::Eq(field, val) => {
                metadata.get(field) == Some(val)
            }
            MetadataFilter::Ne(field, val) => {
                metadata.get(field) != Some(val)
            }
            MetadataFilter::Gt(field, val) => {
                compare_numeric(metadata.get(field), val, |a, b| a > b)
            }
            MetadataFilter::Gte(field, val) => {
                compare_numeric(metadata.get(field), val, |a, b| a >= b)
            }
            MetadataFilter::Lt(field, val) => {
                compare_numeric(metadata.get(field), val, |a, b| a < b)
            }
            MetadataFilter::Lte(field, val) => {
                compare_numeric(metadata.get(field), val, |a, b| a <= b)
            }
            MetadataFilter::In(field, vals) => {
                metadata.get(field).is_some_and(|v| vals.contains(v))
            }
            MetadataFilter::And(filters) => {
                filters.iter().all(|f| f.matches(metadata))
            }
            MetadataFilter::Or(filters) => {
                filters.iter().any(|f| f.matches(metadata))
            }
            MetadataFilter::Not(filter) => {
                !filter.matches(metadata)
            }
        }
    }
}

fn compare_numeric<F>(actual: Option<&Value>, expected: &Value, op: F) -> bool
where
    F: Fn(f64, f64) -> bool,
{
    match (actual.and_then(|v| v.as_f64()), expected.as_f64()) {
        (Some(a), Some(e)) => op(a, e),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_filter_eq() {
        let meta = json!({"topic": "rust", "priority": 1});
        assert!(MetadataFilter::Eq("topic".to_string(), json!("rust")).matches(&meta));
        assert!(!MetadataFilter::Eq("topic".to_string(), json!("python")).matches(&meta));
        assert!(!MetadataFilter::Eq("missing".to_string(), json!("any")).matches(&meta));
    }

    #[test]
    fn test_filter_numeric() {
        let meta = json!({"priority": 10});
        assert!(MetadataFilter::Gt("priority".to_string(), json!(5)).matches(&meta));
        assert!(MetadataFilter::Lte("priority".to_string(), json!(10)).matches(&meta));
        assert!(!MetadataFilter::Lt("priority".to_string(), json!(10)).matches(&meta));
    }

    #[test]
    fn test_filter_and_or() {
        let meta = json!({"topic": "rust", "priority": 10});
        let f_and = MetadataFilter::And(vec![
            MetadataFilter::Eq("topic".to_string(), json!("rust")),
            MetadataFilter::Gt("priority".to_string(), json!(5)),
        ]);
        assert!(f_and.matches(&meta));

        let f_or = MetadataFilter::Or(vec![
            MetadataFilter::Eq("topic".to_string(), json!("python")),
            MetadataFilter::Eq("priority".to_string(), json!(10)),
        ]);
        assert!(f_or.matches(&meta));
    }
}
