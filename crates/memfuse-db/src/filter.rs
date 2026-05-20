//! Metadata filtering expressions and evaluation logic.
// ANCHOR:ARCH:FILTER-001 — Metadata Filtering (WP-4.2).
// WP:WP-4.2 PRIO:1 NEEDS:NONE
// AGENT:04 DATE:2026-05-20 STATUS:WIP

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Expressions for filtering documents based on their metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FilterExpr {
    /// Field equals value
    Eq(String, Value),
    /// Field does not equal value
    Neq(String, Value),
    /// Field is greater than value
    Gt(String, Value),
    /// Field is greater than or equal to value
    Gte(String, Value),
    /// Field is less than value
    Lt(String, Value),
    /// Field is less than or equal to value
    Lte(String, Value),
    /// Field is in a list of values
    In(String, Vec<Value>),
    /// Logical AND of multiple expressions
    And(Vec<FilterExpr>),
    /// Logical OR of multiple expressions
    Or(Vec<FilterExpr>),
    /// Logical NOT of an expression
    Not(Box<FilterExpr>),
}

impl FilterExpr {
    /// Evaluates if the filter matches the given metadata.
    pub fn matches(&self, metadata: &Value) -> bool {
        match self {
            FilterExpr::Eq(key, val) => metadata.get(key) == Some(val),
            FilterExpr::Neq(key, val) => metadata.get(key) != Some(val),
            FilterExpr::Gt(key, val) => {
                if let Some(m_val) = metadata.get(key) {
                    return compare_values(m_val, val) == Some(std::cmp::Ordering::Greater);
                }
                false
            }
            FilterExpr::Gte(key, val) => {
                if let Some(m_val) = metadata.get(key) {
                    let ord = compare_values(m_val, val);
                    return ord == Some(std::cmp::Ordering::Greater) || ord == Some(std::cmp::Ordering::Equal);
                }
                false
            }
            FilterExpr::Lt(key, val) => {
                if let Some(m_val) = metadata.get(key) {
                    return compare_values(m_val, val) == Some(std::cmp::Ordering::Less);
                }
                false
            }
            FilterExpr::Lte(key, val) => {
                if let Some(m_val) = metadata.get(key) {
                    let ord = compare_values(m_val, val);
                    return ord == Some(std::cmp::Ordering::Less) || ord == Some(std::cmp::Ordering::Equal);
                }
                false
            }
            FilterExpr::In(key, vals) => {
                if let Some(m_val) = metadata.get(key) {
                    return vals.contains(m_val);
                }
                false
            }
            FilterExpr::And(exprs) => exprs.iter().all(|e| e.matches(metadata)),
            FilterExpr::Or(exprs) => exprs.iter().any(|e| e.matches(metadata)),
            FilterExpr::Not(expr) => !expr.matches(metadata),
        }
    }
}

fn compare_values(a: &Value, b: &Value) -> Option<std::cmp::Ordering> {
    match (a, b) {
        (Value::Number(n1), Value::Number(n2)) => {
            if let (Some(f1), Some(f2)) = (n1.as_f64(), n2.as_f64()) {
                f1.partial_cmp(&f2)
            } else {
                None
            }
        }
        (Value::String(s1), Value::String(s2)) => Some(s1.cmp(s2)),
        _ => None,
    }
}
