//! Advanced Metadata Filtering for MemFuse.

use serde::{Deserialize, Serialize};

/// Metadata filter expression.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FilterExpr {
    /// Equality: field == value
    Eq(String, serde_json::Value),
    /// Not Equal: field != value
    Ne(String, serde_json::Value),
    /// Contains: value in field (for arrays or strings)
    Contains(String, serde_json::Value),
}

/// Strategy for applying filters during search.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterStrategy {
    /// Filter applied within the vector index (before k-NN).
    PreFilter,
    /// Filter applied to the results of the vector search.
    PostFilter,
    /// Both parallel, merge (not yet fully implemented).
    Hybrid,
}

/// Chooses the most efficient filter strategy based on estimated selectivity.
///
/// selectivity: 0.0 = matches nothing, 1.0 = matches everything.
pub fn choose_filter_strategy(
    filter_selectivity: f32,
    _index_size: usize,
) -> FilterStrategy {
    match filter_selectivity {
        s if s < 0.05 => FilterStrategy::PreFilter, // High selectivity -> PreFilter
        s if s > 0.50 => FilterStrategy::PostFilter, // Low selectivity -> PostFilter
        _ => FilterStrategy::Hybrid,
    }
}

impl FilterExpr {
    /// Evaluates the expression against a metadata object.
    pub fn matches(&self, metadata: &serde_json::Value) -> bool {
        match self {
            FilterExpr::Eq(field, value) => metadata
                .as_object()
                .and_then(|obj| obj.get(field))
                .is_some_and(|v| v == value),
            FilterExpr::Ne(field, value) => metadata
                .as_object()
                .and_then(|obj| obj.get(field))
                .is_some_and(|v| v != value),
            FilterExpr::Contains(field, value) => {
                if let Some(obj) = metadata.as_object() {
                    if let Some(v) = obj.get(field) {
                        if let Some(arr) = v.as_array() {
                            return arr.contains(value);
                        }
                        if let (Some(s), Some(target)) = (v.as_str(), value.as_str()) {
                            return s.contains(target);
                        }
                    }
                }
                false
            }
        }
    }
}
