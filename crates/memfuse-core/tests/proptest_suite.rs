use memfuse_core::types::{DocId, EntityId, FilterExpr, TxId};
use proptest::prelude::*;

proptest! {
    #[test]
    fn prop_doc_id_ordering_and_equality(val in any::<u64>()) {
        let doc1 = DocId::new(val);
        let doc2 = DocId::new(val);
        assert_eq!(doc1, doc2);
        assert_eq!(doc1.inner(), val);
    }

    #[test]
    fn prop_entity_id_ordering_and_equality(val in any::<u64>()) {
        let ent1 = EntityId::new(val);
        let ent2 = EntityId::new(val);
        assert_eq!(ent1, ent2);
        assert_eq!(ent1.inner(), val);
    }

    #[test]
    fn prop_tx_id_ordering_and_arithmetic(a in 0..u64::MAX - 100, b in 1..100u64) {
        let tx1 = TxId::new(a);
        let tx2 = TxId::new(a + b);
        assert!(tx2 > tx1);
        assert_eq!(tx2.inner() - tx1.inner(), b);
    }

    #[test]
    fn prop_filter_expr_combinatorics(
        field in "[a-z]{1,10}",
        val in "[a-zA-Z0-9]{1,20}"
    ) {
        let filter = FilterExpr::Eq {
            field: field.clone(),
            value: serde_json::json!(val),
        };

        let serialized = serde_json::to_string(&filter).unwrap();
        let deserialized: FilterExpr = serde_json::from_str(&serialized).unwrap();
        assert_eq!(filter, deserialized);
    }
}
