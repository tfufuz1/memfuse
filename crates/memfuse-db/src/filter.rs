//! Metadata filtering for document retrieval.

#![forbid(unsafe_code)]

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
                            if let Some(arr) = actual_value.as_array() {
                                !arr.contains(value)
                            } else if let Some(arr) = value.as_array() {
                                !arr.contains(actual_value)
                            } else {
                                actual_value != value
                            }
                        }
                    }
                } else {
                    // Field not present
                    matches!(op, FilterOp::Ne | FilterOp::NotIn)
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
    fn test_filter_eq_ne() {
        let meta = json!({"category": "rust", "priority": 1});

        let f_eq = MetadataFilter::Condition {
            field: "category".to_string(),
            op: FilterOp::Eq,
            value: json!("rust"),
        };
        assert!(f_eq.matches(&meta));

        let f_ne = MetadataFilter::Condition {
            field: "category".to_string(),
            op: FilterOp::Ne,
            value: json!("python"),
        };
        assert!(f_ne.matches(&meta));
    }

    #[test]
    fn test_filter_comparison() {
        let meta = json!({"priority": 10});

        let f_gt = MetadataFilter::Condition {
            field: "priority".to_string(),
            op: FilterOp::Gt,
            value: json!(5),
        };
        assert!(f_gt.matches(&meta));

        let f_lt = MetadataFilter::Condition {
            field: "priority".to_string(),
            op: FilterOp::Lt,
            value: json!(20),
        };
        assert!(f_lt.matches(&meta));

        let f_gte = MetadataFilter::Condition {
            field: "priority".to_string(),
            op: FilterOp::Gte,
            value: json!(10),
        };
        assert!(f_gte.matches(&meta));
    }

    #[test]
    fn test_filter_in_notin() {
        let meta = json!({"tag": "ai"});

        let f_in = MetadataFilter::Condition {
            field: "tag".to_string(),
            op: FilterOp::In,
            value: json!(["ai", "db"]),
        };
        assert!(f_in.matches(&meta));

        let f_notin = MetadataFilter::Condition {
            field: "tag".to_string(),
            op: FilterOp::NotIn,
            value: json!(["web", "mobile"]),
        };
        assert!(f_notin.matches(&meta));
    }

    #[test]
    fn test_filter_logical() {
        let meta = json!({"category": "rust", "priority": 10});

        let f_and = MetadataFilter::And(vec![
            MetadataFilter::Condition {
                field: "category".to_string(),
                op: FilterOp::Eq,
                value: json!("rust"),
            },
            MetadataFilter::Condition {
                field: "priority".to_string(),
                op: FilterOp::Gt,
                value: json!(5),
            },
        ]);
        assert!(f_and.matches(&meta));

        let f_or = MetadataFilter::Or(vec![
            MetadataFilter::Condition {
                field: "category".to_string(),
                op: FilterOp::Eq,
                value: json!("python"),
            },
            MetadataFilter::Condition {
                field: "priority".to_string(),
                op: FilterOp::Eq,
                value: json!(10),
            },
        ]);
        assert!(f_or.matches(&meta));

        let f_not = MetadataFilter::Not(Box::new(MetadataFilter::Condition {
            field: "category".to_string(),
            op: FilterOp::Eq,
            value: json!("python"),
        }));
        assert!(f_not.matches(&meta));
    }

    #[test]
    fn test_nested_filters() {
        let meta = json!({"type": "agent", "status": "active", "load": 5});

        // (type == agent AND status == active) AND NOT (load > 10)
        let f = MetadataFilter::And(vec![
            MetadataFilter::And(vec![
                MetadataFilter::Condition {
                    field: "type".to_string(),
                    op: FilterOp::Eq,
                    value: json!("agent"),
                },
                MetadataFilter::Condition {
                    field: "status".to_string(),
                    op: FilterOp::Eq,
                    value: json!("active"),
                },
            ]),
            MetadataFilter::Not(Box::new(MetadataFilter::Condition {
                field: "load".to_string(),
                op: FilterOp::Gt,
                value: json!(10),
            })),
        ]);

        assert!(f.matches(&meta));
    }
}
