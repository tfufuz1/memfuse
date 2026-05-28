//! Metadata filtering expressions.
//!
//! This module defines the [`FilterExpr`] enum for constructing complex
//! metadata filter predicates for both pre- and post-filtering.

// ANCHOR:DOC AGENT:01 STATUS:DONE PRIO:3 — Missing module-level documentation.
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
