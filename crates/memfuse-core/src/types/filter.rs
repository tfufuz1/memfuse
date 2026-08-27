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
    /// Greater than: field > value
    Gt {
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
    /// In set: field IN (values)
    In {
        /// Target metadata field name.
        field: String,
        /// List of candidate matching values.
        values: Vec<serde_json::Value>,
    },
    /// Logical AND
    And(Box<FilterExpr>, Box<FilterExpr>),
    /// Logical OR
    Or(Box<FilterExpr>, Box<FilterExpr>),
    /// Logical NOT
    Not(Box<FilterExpr>),
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

        let ser = serde_json::to_string(&filter).unwrap(); // unwrap
        let deser: FilterExpr = serde_json::from_str(&ser).unwrap(); // unwrap
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
}
