use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Operators for metadata filtering.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum FilterOp {
    /// Equal to
    Eq,
    /// Not equal to
    Ne,
    /// Greater than
    Gt,
    /// Greater than or equal to
    Gte,
    /// Less than
    Lt,
    /// Less than or equal to
    Lte,
    /// In a set of values
    In,
    /// Not in a set of values
    NotIn,
    /// Field existence check
    Exists,
}

/// Advanced metadata filter for document retrieval and search.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MetadataFilter {
    /// A single condition on a metadata field.
    Condition {
        field: String,
        op: FilterOp,
        value: Value,
    },
    /// Logical AND of multiple filters.
    And(Vec<MetadataFilter>),
    /// Logical OR of multiple filters.
    Or(Vec<MetadataFilter>),
    /// Logical NOT of a filter.
    Not(Box<MetadataFilter>),
}

impl MetadataFilter {
    /// Evaluates the filter against a metadata object.
    pub fn matches(&self, metadata: &Value) -> bool {
        match self {
            MetadataFilter::Condition { field, op, value } => {
                if let Some(actual_value) = metadata.get(field) {
                    match op {
                        FilterOp::Eq => actual_value == value,
                        FilterOp::Ne => actual_value != value,
                        FilterOp::Gt => {
                            compare_values(actual_value, value) == Some(std::cmp::Ordering::Greater)
                        }
                        FilterOp::Gte => matches!(
                            compare_values(actual_value, value),
                            Some(std::cmp::Ordering::Greater | std::cmp::Ordering::Equal)
                        ),
                        FilterOp::Lt => {
                            compare_values(actual_value, value) == Some(std::cmp::Ordering::Less)
                        }
                        FilterOp::Lte => matches!(
                            compare_values(actual_value, value),
                            Some(std::cmp::Ordering::Less | std::cmp::Ordering::Equal)
                        ),
                        FilterOp::In => {
                            if let Some(arr) = actual_value.as_array() {
                                arr.contains(value)
                            } else if let Some(arr) = value.as_array() {
                                arr.contains(actual_value)
                            } else {
                                false
                            }
                        }
                        FilterOp::NotIn => {
                            if let Some(arr) = value.as_array() {
                                !arr.contains(actual_value)
                            } else if let Some(arr) = actual_value.as_array() {
                                !arr.contains(value)
                            } else {
                                true
                            }
                        }
                        FilterOp::Exists => value.as_bool().unwrap_or(true),
                    }
                } else {
                    // Field not present
                    match op {
                        FilterOp::Ne | FilterOp::NotIn => true,
                        FilterOp::Exists => {
                            if let Some(b) = value.as_bool() {
                                !b
                            } else {
                                false
                            }
                        }
                        _ => false,
                    }
                }
            }
            MetadataFilter::And(filters) => filters.iter().all(|f| f.matches(metadata)),
            MetadataFilter::Or(filters) => filters.iter().any(|f| f.matches(metadata)),
            MetadataFilter::Not(filter) => !filter.matches(metadata),
        }
    }
}

fn compare_values(a: &Value, b: &Value) -> Option<std::cmp::Ordering> {
    match (a, b) {
        (Value::Number(an), Value::Number(bn)) => {
            if let (Some(af), Some(bf)) = (an.as_f64(), bn.as_f64()) {
                af.partial_cmp(&bf)
            } else {
                None
            }
        }
        (Value::String(as_str), Value::String(bs_str)) => as_str.partial_cmp(bs_str),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_filter_in_nin_exists_operators() {
        let meta = json!({
            "category": "electronics",
            "tags": ["laptop", "gadget"],
            "price": 1000
        });

        // $in test
        let filter_in = MetadataFilter::Condition {
            field: "category".to_string(),
            op: FilterOp::In,
            value: json!(["electronics", "books"]),
        };
        assert!(filter_in.matches(&meta));

        let filter_in_tag = MetadataFilter::Condition {
            field: "tags".to_string(),
            op: FilterOp::In,
            value: json!("laptop"),
        };
        assert!(filter_in_tag.matches(&meta));

        // $nin test
        let filter_nin = MetadataFilter::Condition {
            field: "category".to_string(),
            op: FilterOp::NotIn,
            value: json!(["clothing", "books"]),
        };
        assert!(filter_nin.matches(&meta));

        // $exists test
        let filter_exists_true = MetadataFilter::Condition {
            field: "category".to_string(),
            op: FilterOp::Exists,
            value: json!(true),
        };
        assert!(filter_exists_true.matches(&meta));

        let filter_exists_false = MetadataFilter::Condition {
            field: "non_existent_field".to_string(),
            op: FilterOp::Exists,
            value: json!(false),
        };
        assert!(filter_exists_false.matches(&meta));

        let filter_exists_missing = MetadataFilter::Condition {
            field: "non_existent_field".to_string(),
            op: FilterOp::Exists,
            value: json!(true),
        };
        assert!(!filter_exists_missing.matches(&meta));
    }

    #[test]
    fn test_filter_type_mismatch_safety() {
        let meta = json!({
            "count": 42
        });

        let filter_mismatch_in = MetadataFilter::Condition {
            field: "count".to_string(),
            op: FilterOp::In,
            value: json!("not_an_array_or_number"),
        };
        // Should return false, not panic
        assert!(!filter_mismatch_in.matches(&meta));
    }
}
