//! Metadata filter expression DSL and evaluation engine.

// FILE-CONTEXT
// STAND: 2026-08-30T18:51:56Z (SESSION: e459bd5f)
// ZWECK: Kanonische Metadata Filter DSL (FilterExpr) und Evaluierungs-Engine.
// INVARIANTEN: Reines In-Memory Evaluieren gegen serde_json::Value ohne Nebenwirkungen.
// HOTSPOTS: 20-220
// NICHT-OFFENSICHTLICH: Legacy MetadataFilter aus memfuse-db konvertiert verlustfrei in FilterExpr.
// SIEHE AUCH: rules/tag_taxonomy.md, DECISIONS.md

use serde::{Deserialize, Serialize};

/// Metadata filter expressions for pre/post filtering.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum FilterExpr {
    /// Exact match: field == value
    Eq {
        /// Target metadata field name.
        field: String,
        /// Expected field value.
        value: serde_json::Value,
    },
    /// Not equal: field != value
    Ne {
        /// Target metadata field name.
        field: String,
        /// Disallowed field value.
        value: serde_json::Value,
    },
    /// Greater than: field > value
    Gt {
        /// Target metadata field name.
        field: String,
        /// Comparison threshold value.
        value: serde_json::Value,
    },
    /// Greater than or equal: field >= value
    Gte {
        /// Target metadata field name.
        field: String,
        /// Comparison threshold value.
        value: serde_json::Value,
    },
    /// Less than: field < value
    Lt {
        /// Target metadata field name.
        field: String,
        /// Comparison threshold value.
        value: serde_json::Value,
    },
    /// Less than or equal: field <= value
    Lte {
        /// Target metadata field name.
        field: String,
        /// Comparison threshold value.
        value: serde_json::Value,
    },
    /// In set: field IN (values)
    In {
        /// Target metadata field name.
        field: String,
        /// List of candidate matching values.
        values: Vec<serde_json::Value>,
    },
    /// Not in set: field NOT IN (values)
    NotIn {
        /// Target metadata field name.
        field: String,
        /// List of candidate matching values.
        values: Vec<serde_json::Value>,
    },
    /// Field existence check
    Exists {
        /// Target metadata field name.
        field: String,
        /// Whether the field should exist (true) or not (false).
        exists: bool,
    },
    /// Logical AND
    And(Box<FilterExpr>, Box<FilterExpr>),
    /// Logical OR
    Or(Box<FilterExpr>, Box<FilterExpr>),
    /// Logical NOT
    Not(Box<FilterExpr>),
}

fn compare_values(a: &serde_json::Value, b: &serde_json::Value) -> Option<std::cmp::Ordering> {
    match (a, b) {
        (serde_json::Value::Number(an), serde_json::Value::Number(bn)) => {
            if let (Some(af), Some(bf)) = (an.as_f64(), bn.as_f64()) {
                af.partial_cmp(&bf)
            } else {
                None
            }
        }
        (serde_json::Value::String(as_str), serde_json::Value::String(bs_str)) => {
            as_str.partial_cmp(bs_str)
        }
        _ => None,
    }
}

impl FilterExpr {
    /// Evaluates the filter expression against a JSON metadata object.
    pub fn evaluate(&self, metadata: &serde_json::Value) -> bool {
        match self {
            FilterExpr::Eq { field, value } => {
                if let Some(actual) = metadata.get(field) {
                    actual == value
                } else {
                    false
                }
            }
            FilterExpr::Ne { field, value } => {
                if let Some(actual) = metadata.get(field) {
                    actual != value
                } else {
                    true
                }
            }
            FilterExpr::Gt { field, value } => {
                if let Some(actual) = metadata.get(field) {
                    compare_values(actual, value) == Some(std::cmp::Ordering::Greater)
                } else {
                    false
                }
            }
            FilterExpr::Gte { field, value } => {
                if let Some(actual) = metadata.get(field) {
                    matches!(
                        compare_values(actual, value),
                        Some(std::cmp::Ordering::Greater | std::cmp::Ordering::Equal)
                    )
                } else {
                    false
                }
            }
            FilterExpr::Lt { field, value } => {
                if let Some(actual) = metadata.get(field) {
                    compare_values(actual, value) == Some(std::cmp::Ordering::Less)
                } else {
                    false
                }
            }
            FilterExpr::Lte { field, value } => {
                if let Some(actual) = metadata.get(field) {
                    matches!(
                        compare_values(actual, value),
                        Some(std::cmp::Ordering::Less | std::cmp::Ordering::Equal)
                    )
                } else {
                    false
                }
            }
            FilterExpr::In { field, values } => {
                if let Some(actual) = metadata.get(field) {
                    if let Some(arr) = actual.as_array() {
                        values.iter().any(|v| arr.contains(v))
                    } else {
                        values.contains(actual)
                    }
                } else {
                    false
                }
            }
            FilterExpr::NotIn { field, values } => {
                if let Some(actual) = metadata.get(field) {
                    if let Some(arr) = actual.as_array() {
                        !values.iter().any(|v| arr.contains(v))
                    } else {
                        !values.contains(actual)
                    }
                } else {
                    true
                }
            }
            FilterExpr::Exists { field, exists } => {
                let is_present = metadata.get(field).is_some();
                is_present == *exists
            }
            FilterExpr::And(left, right) => left.evaluate(metadata) && right.evaluate(metadata),
            FilterExpr::Or(left, right) => left.evaluate(metadata) || right.evaluate(metadata),
            FilterExpr::Not(expr) => !expr.evaluate(metadata),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_filter_serialization() {
        let filter = FilterExpr::And(
            Box::new(FilterExpr::Eq {
                field: "category".to_string(),
                value: json!("electronics"),
            }),
            Box::new(FilterExpr::Gt {
                field: "price".to_string(),
                value: json!(100),
            }),
        );

        let ser = serde_json::to_string(&filter).unwrap(); // unwrap #[cfg(test)]
        let deser: FilterExpr = serde_json::from_str(&ser).unwrap(); // unwrap #[cfg(test)]
        assert_eq!(filter, deser);
    }

    #[test]
    fn test_filter_variants() {
        let f1 = FilterExpr::In {
            field: "tags".to_string(),
            values: vec![json!("a"), json!("b")],
        };
        let f2 = FilterExpr::Not(Box::new(f1));
        let f3 = FilterExpr::Or(
            Box::new(f2),
            Box::new(FilterExpr::Lt {
                field: "v".to_string(),
                value: json!(1),
            }),
        );

        assert!(matches!(f3, FilterExpr::Or(_, _)));
    }

    #[test]
    fn test_filter_expr_evaluate_all_operators() {
        let meta = json!({
            "category": "electronics",
            "price": 500,
            "rating": 4.5,
            "tags": ["laptop", "gadget"],
            "archived": false
        });

        // Eq & Ne
        assert!(FilterExpr::Eq {
            field: "category".to_string(),
            value: json!("electronics")
        }
        .evaluate(&meta));

        assert!(!FilterExpr::Eq {
            field: "category".to_string(),
            value: json!("books")
        }
        .evaluate(&meta));

        assert!(FilterExpr::Ne {
            field: "category".to_string(),
            value: json!("books")
        }
        .evaluate(&meta));

        assert!(!FilterExpr::Ne {
            field: "category".to_string(),
            value: json!("electronics")
        }
        .evaluate(&meta));

        // Missing field for Eq / Ne
        assert!(!FilterExpr::Eq {
            field: "missing".to_string(),
            value: json!("val")
        }
        .evaluate(&meta));

        assert!(FilterExpr::Ne {
            field: "missing".to_string(),
            value: json!("val")
        }
        .evaluate(&meta));

        // Gt, Gte, Lt, Lte
        assert!(FilterExpr::Gt {
            field: "price".to_string(),
            value: json!(100)
        }
        .evaluate(&meta));

        assert!(FilterExpr::Gte {
            field: "price".to_string(),
            value: json!(500)
        }
        .evaluate(&meta));

        assert!(FilterExpr::Lt {
            field: "price".to_string(),
            value: json!(1000)
        }
        .evaluate(&meta));

        assert!(FilterExpr::Lte {
            field: "price".to_string(),
            value: json!(500)
        }
        .evaluate(&meta));

        // In & NotIn
        assert!(FilterExpr::In {
            field: "category".to_string(),
            values: vec![json!("electronics"), json!("books")]
        }
        .evaluate(&meta));

        assert!(FilterExpr::In {
            field: "tags".to_string(),
            values: vec![json!("laptop")]
        }
        .evaluate(&meta));

        assert!(FilterExpr::NotIn {
            field: "category".to_string(),
            values: vec![json!("clothing"), json!("books")]
        }
        .evaluate(&meta));

        assert!(FilterExpr::NotIn {
            field: "missing".to_string(),
            values: vec![json!("anything")]
        }
        .evaluate(&meta));

        // Exists
        assert!(FilterExpr::Exists {
            field: "category".to_string(),
            exists: true
        }
        .evaluate(&meta));

        assert!(!FilterExpr::Exists {
            field: "category".to_string(),
            exists: false
        }
        .evaluate(&meta));

        assert!(FilterExpr::Exists {
            field: "non_existent".to_string(),
            exists: false
        }
        .evaluate(&meta));

        assert!(!FilterExpr::Exists {
            field: "non_existent".to_string(),
            exists: true
        }
        .evaluate(&meta));
    }

    #[test]
    fn test_filter_evaluate_null_and_empty_meta() {
        let null_meta = json!(null);
        let empty_meta = json!({});

        let eq = FilterExpr::Eq {
            field: "key".to_string(),
            value: json!("val"),
        };
        assert!(!eq.evaluate(&null_meta));
        assert!(!eq.evaluate(&empty_meta));

        let exists_false = FilterExpr::Exists {
            field: "key".to_string(),
            exists: false,
        };
        assert!(exists_false.evaluate(&null_meta));
        assert!(exists_false.evaluate(&empty_meta));

        let exists_true = FilterExpr::Exists {
            field: "key".to_string(),
            exists: true,
        };
        assert!(!exists_true.evaluate(&null_meta));
        assert!(!exists_true.evaluate(&empty_meta));
    }

    #[test]
    fn test_nested_filter_expression() {
        let meta = json!({
            "category": "electronics",
            "price": 500,
            "rating": 4.5
        });

        // Nested expression: And(Or(Eq(category, "electronics"), Eq(category, "books")), Not(Lt(price, 100)))
        let expr = FilterExpr::And(
            Box::new(FilterExpr::Or(
                Box::new(FilterExpr::Eq {
                    field: "category".to_string(),
                    value: json!("electronics"),
                }),
                Box::new(FilterExpr::Eq {
                    field: "category".to_string(),
                    value: json!("books"),
                }),
            )),
            Box::new(FilterExpr::Not(Box::new(FilterExpr::Lt {
                field: "price".to_string(),
                value: json!(100),
            }))),
        );

        assert!(expr.evaluate(&meta));

        // Nested expression: And(Or(Eq, Eq), Not(Lt)) where price is 50 (< 100), so Not(Lt) is false -> false
        let meta_cheap = json!({
            "category": "electronics",
            "price": 50
        });
        assert!(!expr.evaluate(&meta_cheap));
    }
}
