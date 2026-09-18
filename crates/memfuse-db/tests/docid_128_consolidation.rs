use memfuse_core::types::DocId;
#[cfg(feature = "volatile-vault")]
use memfuse_core::types::TxId;
use memfuse_db::memory_consolidation::compute_community_hash;
#[cfg(feature = "volatile-vault")]
use memfuse_db::volatile_vault::{SignalModality, VaultChunk};

#[cfg(feature = "volatile-vault")]
#[test]
fn test_vault_chunk_docid_type() {
    if let Ok(doc_id) = DocId::from_key("vault_test_doc_key") {
        let chunk = VaultChunk::new(
            doc_id,
            b"sensitive payload".to_vec(),
            SignalModality::TextInput,
            TxId::new(1),
        );

        assert_eq!(chunk.id, doc_id);
        assert_eq!(chunk.content, b"sensitive payload");
    }
}

#[test]
fn test_compute_community_hash_deterministic() {
    if let (Ok(doc_id_1), Ok(doc_id_2)) = (
        DocId::from_key("community_doc_1"),
        DocId::from_key("community_doc_2"),
    ) {
        let hash_1 = compute_community_hash(&[doc_id_1, doc_id_2]);
        let hash_2 = compute_community_hash(&[doc_id_2, doc_id_1]);

        assert_eq!(
            hash_1, hash_2,
            "Community hash must be order-independent due to internal sorting"
        );
    }
}
