//! Advanced Metadata Filtering and Adaptive Strategy Selection.
// ANCHOR:ARCH:FILTER-001 — Adaptive Filter-Strategie (WP-4.2 / WP-5.4).
// WP:WP-4.2 PRIO:1 NEEDS:WP-1.2
// AGENT:04 DATE:2026-05-18 STATUS:READY

use serde_json::Value;

/// Strategy for combining vector search with metadata filtering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterStrategy {
    /// Filter documents first, then perform vector search on the remaining set.
    /// Efficient when the filter is highly selective (matches few documents).
    PreFilter,
    /// Perform vector search first, then filter the results.
    /// Efficient when the filter matches most documents.
    PostFilter,
    /// Parallel execution or intermediate approach.
    Hybrid,
}

impl FilterStrategy {
    /// Heuristic to choose the best filtering strategy based on selectivity.
    pub fn choose(selectivity: f32, _index_size: usize) -> Self {
        if selectivity < 0.05 {
            FilterStrategy::PreFilter
        } else if selectivity > 0.50 {
            FilterStrategy::PostFilter
        } else {
            // For intermediate ranges, we currently default to PreFilter
            // to ensure we get enough results, though Hybrid is the goal.
            FilterStrategy::PreFilter
        }
    }
}

/// A structured metadata filter.
#[derive(Debug, Clone)]
pub enum MetadataFilter {
    /// Matches if the field equals the value.
    Eq(String, Value),
    /// Matches if the field contains the value (for arrays).
    Contains(String, Value),
    /// Matches if all sub-filters match.
    And(Vec<MetadataFilter>),
    /// Matches if any sub-filter matches.
    Or(Vec<MetadataFilter>),
}

impl MetadataFilter {
    /// Evaluates the filter against a JSON value.
    pub fn matches(&self, metadata: &Value) -> bool {
        match self {
            MetadataFilter::Eq(field, val) => {
                if let Some(m_obj) = metadata.as_object() {
                    m_obj.get(field) == Some(val)
                } else {
                    false
                }
            }
            MetadataFilter::Contains(field, val) => {
                if let Some(m_obj) = metadata.as_object() {
                    if let Some(f_val) = m_obj.get(field) {
                        if let Some(arr) = f_val.as_array() {
                            return arr.contains(val);
                        }
                    }
                }
                false
            }
            MetadataFilter::And(filters) => filters.iter().all(|f| f.matches(metadata)),
            MetadataFilter::Or(filters) => filters.iter().any(|f| f.matches(metadata)),
        }
    }

    /// Estimates the selectivity of the filter (0.0 to 1.0).
    /// This is a rough heuristic without detailed statistics.
    pub fn estimate_selectivity(&self) -> f32 {
        match self {
            MetadataFilter::Eq(_, _) => 0.01, // Assume 1% match for equality
            MetadataFilter::Contains(_, _) => 0.05,
            MetadataFilter::And(filters) => {
                filters.iter().map(|f| f.estimate_selectivity()).product()
            }
            MetadataFilter::Or(filters) => {
                let s: f32 = filters.iter().map(|f| f.estimate_selectivity()).sum();
                s.min(1.0)
            }
        }
    }
}
