//! Metadata filtering expressions and logic for MemFuse.

use serde::{Deserialize, Serialize};

/// Metadata filter expressions for pre/post filtering.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum FilterExpr {
    /// Exact match: field == value
    Eq {
        /// Field name.
        field: String,
        /// Expected value.
        value: serde_json::Value,
    },
    /// Greater than: field > value
    Gt {
        /// Field name.
        field: String,
        /// Comparison value.
        value: serde_json::Value,
    },
    /// Less than: field < value
    Lt {
        /// Field name.
        field: String,
        /// Comparison value.
        value: serde_json::Value,
    },
    /// In set: field IN (values)
    In {
        /// Field name.
        field: String,
        /// List of allowed values.
        values: Vec<serde_json::Value>,
    },
    /// Logical AND of two expressions.
    And(Box<FilterExpr>, Box<FilterExpr>),
    /// Logical OR of two expressions.
    Or(Box<FilterExpr>, Box<FilterExpr>),
    /// Logical NOT of an expression.
    Not(Box<FilterExpr>),
}
