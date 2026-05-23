//! # Metadata Filtering Module
//!
//! This module provides the core types and logic for advanced metadata filtering
//! in MemFuse. It supports complex filter expressions including logical AND/OR/NOT
//! and a variety of comparison operators.
//!
//! ## Key Components
//! - [`MetadataFilter`]: The main enum representing a filter expression.
//! - [`FilterOp`]: Supported comparison operators (Eq, Ne, Gt, Gte, etc.).

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
                            if let Some(arr) = value.as_array() {
                                !arr.contains(actual_value)
                            } else {
                                true
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
