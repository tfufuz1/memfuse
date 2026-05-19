//! Advanced Metadata Filtering (WP-4.2).

use serde_json::Value;

/// A structured expression for metadata filtering.
#[derive(Debug, Clone)]
pub enum FilterExpr {
    /// Field equals a value.
    Eq(String, Value),
    /// All sub-expressions must be true.
    And(Vec<FilterExpr>),
    /// At least one sub-expression must be true.
    Or(Vec<FilterExpr>),
}

impl FilterExpr {
    /// Estimates the selectivity of the filter (0.0 to 1.0).
    /// Heuristic: Eq=0.01, And=multiplicative, Or=additive.
    pub fn estimate_selectivity(&self) -> f32 {
        match self {
            FilterExpr::Eq(_, _) => 0.01,
            FilterExpr::And(exprs) => {
                let mut s = 1.0;
                for e in exprs {
                    s *= e.estimate_selectivity();
                }
                s
            }
            FilterExpr::Or(exprs) => {
                let mut s = 0.0;
                for e in exprs {
                    s += e.estimate_selectivity();
                }
                s.min(1.0)
            }
        }
    }

    /// Checks if the filter matches the given metadata.
    pub fn matches(&self, metadata: &Value) -> bool {
        match self {
            FilterExpr::Eq(key, val) => metadata.get(key) == Some(val),
            FilterExpr::And(exprs) => exprs.iter().all(|e| e.matches(metadata)),
            FilterExpr::Or(exprs) => exprs.iter().any(|e| e.matches(metadata)),
        }
    }
}

/// Strategies for executing a filtered search.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterStrategy {
    /// Find all matching documents first, then search.
    PreFilter,
    /// Search neighbors and filter them during graph traversal.
    PostFilter,
    /// Intermediate strategy.
    Hybrid,
}

/// Advanced metadata filter with adaptive strategy selection.
pub struct MetadataFilter {
    /// The filter expression.
    pub expr: FilterExpr,
}

impl MetadataFilter {
    /// Creates a new metadata filter.
    pub fn new(expr: FilterExpr) -> Self {
        Self { expr }
    }

    /// Chooses the execution strategy based on estimated selectivity.
    pub fn choose_strategy(&self) -> FilterStrategy {
        let selectivity = self.expr.estimate_selectivity();
        if selectivity < 0.05 {
            FilterStrategy::PreFilter
        } else if selectivity > 0.50 {
            FilterStrategy::PostFilter
        } else {
            FilterStrategy::Hybrid
        }
    }
}
