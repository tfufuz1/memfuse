//! Advanced Metadata Filtering for MemFuse.
// ANCHOR:ARCH:FILTER-001 — Filtering Engine (Weiche — Layer 2).
// WP:WP-4.2 PRIO:2 NEEDS:COL-001
// AGENT:04 DATE:2026-05-18 STATUS:READY
// DESIGN: Unterstützt Pre-Filter (HNSW closure) und Post-Filter (Hydration side).

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Expression for metadata-based filtering.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FilterExpr {
    /// Field equals value.
    Eq(String, Value),
    /// Field does not equal value.
    NotEq(String, Value),
    /// Field is greater than value (Number or String).
    Gt(String, Value),
    /// Field is greater than or equal to value.
    Gte(String, Value),
    /// Field is less than value.
    Lt(String, Value),
    /// Field is less than or equal to value.
    Lte(String, Value),
    /// All sub-expressions must match.
    And(Vec<FilterExpr>),
    /// Any sub-expression must match.
    Or(Vec<FilterExpr>),
    /// Logical NOT.
    Not(Box<FilterExpr>),
    /// Field value is in a list of values.
    In(String, Vec<Value>),
}

impl FilterExpr {
    /// Evaluates the filter expression against a JSON metadata object.
    pub fn matches(&self, metadata: &Value) -> bool {
        match self {
            FilterExpr::Eq(field, val) => metadata.get(field) == Some(val),
            FilterExpr::NotEq(field, val) => metadata.get(field) != Some(val),
            FilterExpr::Gt(field, val) => {
                metadata.get(field).and_then(|v| compare_values(v, val)).is_some_and(|ord| ord.is_gt())
            }
            FilterExpr::Gte(field, val) => {
                 metadata.get(field).and_then(|v| compare_values(v, val)).is_some_and(|ord| ord.is_ge())
            }
            FilterExpr::Lt(field, val) => {
                metadata.get(field).and_then(|v| compare_values(v, val)).is_some_and(|ord| ord.is_lt())
            }
            FilterExpr::Lte(field, val) => {
                metadata.get(field).and_then(|v| compare_values(v, val)).is_some_and(|ord| ord.is_le())
            }
            FilterExpr::And(exprs) => exprs.iter().all(|e| e.matches(metadata)),
            FilterExpr::Or(exprs) => exprs.iter().any(|e| e.matches(metadata)),
            FilterExpr::Not(expr) => !expr.matches(metadata),
            FilterExpr::In(field, vals) => metadata.get(field).is_some_and(|v| vals.contains(v)),
        }
    }
}

fn compare_values(a: &Value, b: &Value) -> Option<std::cmp::Ordering> {
    match (a, b) {
        (Value::Number(n1), Value::Number(n2)) => n1.as_f64().partial_cmp(&n2.as_f64()),
        (Value::String(s1), Value::String(s2)) => s1.partial_cmp(s2),
        _ => None,
    }
}

/// Strategy for executing a filtered search.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FilterStrategy {
    /// Filter during HNSW traversal (high selectivity).
    PreFilter,
    /// Filter after vector search (low selectivity).
    PostFilter,
    /// Parallel execution or adaptive heuristic.
    Hybrid,
}

impl FilterStrategy {
    /// Chooses a filter strategy based on estimated selectivity and index size.
    /// Logic as per SPEC-SAOS-WP-5.4.
    pub fn choose(selectivity: f32, _index_size: usize) -> Self {
        if selectivity < 0.05 {
            FilterStrategy::PreFilter
        } else if selectivity > 0.50 {
            FilterStrategy::PostFilter
        } else {
            FilterStrategy::Hybrid
        }
    }
}
