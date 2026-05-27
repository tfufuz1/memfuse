//! # Metadata Filtering
//!
//! Defines the expression language for filtering search results based on
//! document metadata.

use serde::{Deserialize, Serialize};

/// Metadata filter expressions for pre/post filtering.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum FilterExpr {
    /// Exact match: field == value
    Eq {
        field: String,
        value: serde_json::Value,
    },
    /// Greater than: field > value
    Gt {
        field: String,
        value: serde_json::Value,
    },
    /// Less than: field < value
    Lt {
        field: String,
        value: serde_json::Value,
    },
    /// In set: field IN (values)
    In {
        field: String,
        values: Vec<serde_json::Value>,
    },
    /// Logical AND
    And(Box<FilterExpr>, Box<FilterExpr>),
    /// Logical OR
    Or(Box<FilterExpr>, Box<FilterExpr>),
    /// Logical NOT
    Not(Box<FilterExpr>),
}
