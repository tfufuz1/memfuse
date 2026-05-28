//! Metadata filtering expressions.
//!
//! Provides a structured way to define filters for metadata-based retrieval,
//! supporting logical operators and comparison.
//!
// ANCHOR:DOC AGENT:08 STATUS:DONE

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
