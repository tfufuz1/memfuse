// FILE-CONTEXT
// ZWECK: Metadaten-Filterung und Extraktion von Kognitiven MemoryTypes (Episodic, Semantic, Working).
// INVARIANTEN: Keine Panics bei Typ-Mismatches im Filter-Evaluation-Pfad; Rückfall auf MemoryType::Semantic.
// NICHT-OFFENSICHTLICH: Deprecated MetadataFilter wandelt via TryFrom verlustfrei in memfuse_core::FilterExpr um.
// STAND: TS:2026-08-29T17:22:29Z (SESSION: 0dcb9f3b)

use serde::{Deserialize, Serialize};
use serde_json::Value;

use memfuse_core::{FilterExpr, MemoryType};

/// Extrahiert den MemoryType aus Dokument-Metadaten (Rückwärtskompatibel).
pub fn extract_memory_type(metadata: &Option<Value>) -> MemoryType {
    metadata
        .as_ref()
        .and_then(|m| m.get("memory_type"))
        .and_then(|v| serde_json::from_value::<MemoryType>(v.clone()).ok())
        .unwrap_or(MemoryType::Semantic) // Default für bestehende Dokumente
}

/// Operators for metadata filtering.
#[deprecated(
    since = "0.1.0",
    note = "Use memfuse_core::FilterExpr directly; conversion via TryFrom<MetadataFilter> for FilterExpr"
)]
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
    /// Field existence check
    Exists,
}

/// Advanced metadata filter for document retrieval and search.
#[deprecated(
    since = "0.1.0",
    note = "Use memfuse_core::FilterExpr directly; conversion via TryFrom<MetadataFilter> for FilterExpr"
)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MetadataFilter {
    /// A single condition on a metadata field.
    Condition {
        field: String,
        #[allow(deprecated)]
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

#[allow(deprecated)]
impl MetadataFilter {
    /// Evaluates the filter against a metadata object.
    #[deprecated(
        since = "0.1.0",
        note = "Use memfuse_core::FilterExpr directly; conversion via TryFrom<MetadataFilter> for FilterExpr"
    )]
    pub fn matches(&self, metadata: &Value) -> bool {
        if let Ok(expr) = FilterExpr::try_from(self.clone()) {
            expr.evaluate(metadata)
        } else {
            false
        }
    }
}

#[allow(deprecated)]
impl TryFrom<MetadataFilter> for FilterExpr {
    type Error = memfuse_core::MemFuseError;

    fn try_from(filter: MetadataFilter) -> Result<Self, Self::Error> {
        match filter {
            MetadataFilter::Condition { field, op, value } => match op {
                FilterOp::Eq => Ok(FilterExpr::Eq { field, value }),
                FilterOp::Ne => Ok(FilterExpr::Ne { field, value }),
                FilterOp::Gt => Ok(FilterExpr::Gt { field, value }),
                FilterOp::Gte => Ok(FilterExpr::Gte { field, value }),
                FilterOp::Lt => Ok(FilterExpr::Lt { field, value }),
                FilterOp::Lte => Ok(FilterExpr::Lte { field, value }),
                FilterOp::In => {
                    let values = if let Some(arr) = value.as_array() {
                        arr.clone()
                    } else {
                        vec![value]
                    };
                    Ok(FilterExpr::In { field, values })
                }
                FilterOp::NotIn => {
                    let values = if let Some(arr) = value.as_array() {
                        arr.clone()
                    } else {
                        vec![value]
                    };
                    Ok(FilterExpr::NotIn { field, values })
                }
                FilterOp::Exists => {
                    let exists = value.as_bool().unwrap_or(true);
                    Ok(FilterExpr::Exists { field, exists })
                }
            },
            MetadataFilter::And(filters) => {
                let mut iter = filters.into_iter();
                if let Some(first) = iter.next() {
                    let mut acc = FilterExpr::try_from(first)?;
                    for item in iter {
                        acc = FilterExpr::And(Box::new(acc), Box::new(FilterExpr::try_from(item)?));
                    }
                    Ok(acc)
                } else {
                    Err(memfuse_core::MemFuseError::invalid_input(
                        "Empty And filter",
                    ))
                }
            }
            MetadataFilter::Or(filters) => {
                let mut iter = filters.into_iter();
                if let Some(first) = iter.next() {
                    let mut acc = FilterExpr::try_from(first)?;
                    for item in iter {
                        acc = FilterExpr::Or(Box::new(acc), Box::new(FilterExpr::try_from(item)?));
                    }
                    Ok(acc)
                } else {
                    Err(memfuse_core::MemFuseError::invalid_input("Empty Or filter"))
                }
            }
            MetadataFilter::Not(filter) => {
                Ok(FilterExpr::Not(Box::new(FilterExpr::try_from(*filter)?)))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    #[allow(deprecated)]
    fn test_filter_in_nin_exists_operators() {
        let meta = json!({
            "category": "electronics",
            "tags": ["laptop", "gadget"],
            "price": 1000
        });

        // $in test
        let filter_in = MetadataFilter::Condition {
            field: "category".to_string(),
            op: FilterOp::In,
            value: json!(["electronics", "books"]),
        };
        assert!(filter_in.matches(&meta));

        let filter_in_tag = MetadataFilter::Condition {
            field: "tags".to_string(),
            op: FilterOp::In,
            value: json!("laptop"),
        };
        assert!(filter_in_tag.matches(&meta));

        // $nin test
        let filter_nin = MetadataFilter::Condition {
            field: "category".to_string(),
            op: FilterOp::NotIn,
            value: json!(["clothing", "books"]),
        };
        assert!(filter_nin.matches(&meta));

        // $exists test
        let filter_exists_true = MetadataFilter::Condition {
            field: "category".to_string(),
            op: FilterOp::Exists,
            value: json!(true),
        };
        assert!(filter_exists_true.matches(&meta));

        let filter_exists_false = MetadataFilter::Condition {
            field: "non_existent_field".to_string(),
            op: FilterOp::Exists,
            value: json!(false),
        };
        assert!(filter_exists_false.matches(&meta));

        let filter_exists_missing = MetadataFilter::Condition {
            field: "non_existent_field".to_string(),
            op: FilterOp::Exists,
            value: json!(true),
        };
        assert!(!filter_exists_missing.matches(&meta));
    }

    #[test]
    #[allow(deprecated)]
    fn test_filter_type_mismatch_safety() {
        let meta = json!({
            "count": 42
        });

        let filter_mismatch_in = MetadataFilter::Condition {
            field: "count".to_string(),
            op: FilterOp::In,
            value: json!("not_an_array_or_number"),
        };
        // Should return false, not panic
        assert!(!filter_mismatch_in.matches(&meta));
    }

    #[test]
    fn test_extract_memory_type_missing_key_returns_semantic() {
        let meta = Some(json!({"text": "hello"}));
        assert_eq!(extract_memory_type(&meta), MemoryType::Semantic);

        let meta_none: Option<Value> = None;
        assert_eq!(extract_memory_type(&meta_none), MemoryType::Semantic);

        let meta_episodic = Some(json!({"memory_type": "Episodic"}));
        assert_eq!(extract_memory_type(&meta_episodic), MemoryType::Episodic);
    }
}
