// FILE-CONTEXT
// ZWECK: Cryptographic deletion proof for GDPR Article 17 compliance verification across storage layers.
// INVARIANTEN: INV-DELETION-1: DeletionProof::create() MUST only be invoked AFTER physical layer sanitization.
// NICHT-OFFENSICHTLICH: HMAC-SHA256 signature covers scope, sorted key hash, and tx_id. verify() uses constant-time comparison.
// HOTSPOTS: [40-130]
// STAND: TS:2026-09-07T00:00:00Z

#![forbid(unsafe_code)]

//! Kryptographischer Löschbeweis für die Storage-Ebene.
//!
//! KRITISCHE DECKUNGSGRENZE (Pflicht in Enterprise-Doku und export_for_audit):
//! Dieser Proof deckt AUSSCHLIESSLICH: LSM, WAL, HNSW, CSR, KV-Cache.
//! Er KANN NICHT garantieren, dass Wissen aus Fine-Tuning-Zusammenfassungen
//! aus LLM-Parametern entfernbar ist. Referenz: arXiv:2505.16831.
//!
//! INVARIANTE INV-DELETION-1: DeletionProof::create() wird NUR nach
//! physischer Layer-Bereinigung aufgerufen. Proof vor Bereinigung = falsch.

use memfuse_core::{CollectionId, DocId, MemFuseError, Result, TenantId, TxId};
use serde::{Deserialize, Serialize};

/// Layer-explizite Coverage-Deklaration.
/// Jeder Layer MUSS physisch bereinigt sein bevor er hier deklariert wird.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum DeletionLayer {
    /// In-memory memtable records cleared.
    LsmMemtable,
    /// Persistent SSTable files purged across all LSM levels.
    SsTableAllLevels,
    /// HNSW graph nodes and tombstone references purged.
    HnswIndex,
    /// WAL log segments truncated/zeroized.
    WalAllSegments {
        /// Sequence number after which WAL truncation occurred.
        seq_after: u64,
    },
    /// Compressed Sparse Row knowledge graph edges purged.
    CsrGraph,
    /// Key-value cache entries invalidated.
    KvCacheSegments,
    /// In-memory vector embedding cache cleared.
    EmbeddingCache,
}

/// Explizite Nicht-Abdeckung — maschinenlesbar für Audit-Systeme.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ExcludedScope {
    /// Wissen in Zusammenfassungen die als LLM-Fine-Tuning-Input dienten.
    ConsolidatedAndDistilled,
    /// LLM-Modellparameter (arXiv:2505.16831 — Unlearning Isn't Deletion).
    LlmParameterMemory,
}

/// Target scope for physical deletion.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum DeletionScope {
    /// Individual document deletion.
    Document {
        /// DocId of target document.
        doc_id: DocId,
        /// TenantId of owning tenant.
        tenant_id: TenantId,
    },
    /// Collection-wide deletion.
    Collection {
        /// CollectionId of target collection.
        collection_id: CollectionId,
        /// TenantId of owning tenant.
        tenant_id: TenantId,
    },
    /// Tenant-wide deletion.
    Tenant {
        /// TenantId of target tenant.
        tenant_id: TenantId,
    },
}

/// Cryptographic proof of data deletion.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeletionProof {
    /// Target scope of deletion.
    pub scope: DeletionScope,
    /// Blake3-Hash aller gelöschten Dokumentschlüssel (sortiert → deterministisch).
    pub deleted_keys_hash: [u8; 32],
    /// TxId nach der kein gelöschtes Datum mehr im System vorhanden ist.
    /// ADR-016: TxId statt SystemTime für Determinismus.
    pub deleted_after_tx: TxId,
    /// HMAC-SHA256 über (scope_bytes || deleted_keys_hash || tx_bytes).
    pub signature: [u8; 32],
    /// List of physically sanitized storage layers.
    pub covered_layers: Vec<DeletionLayer>,
    /// Pflicht für DSGVO Art. 17-Compliance.
    pub excluded_scopes: Vec<ExcludedScope>,
}

impl DeletionProof {
    /// Erstellt und signiert einen DeletionProof nach Layer-Bereinigung.
    ///
    /// AUFRUFREIHENFOLGE (INV-DELETION-1):
    /// 1. Alle covered_layers physisch bereinigen
    /// 2. WAL-Commit mit Lösch-Intent (P3)
    /// 3. DeletionProof::create() aufrufen
    pub fn create(
        scope: DeletionScope,
        mut deleted_keys: Vec<Vec<u8>>,
        deleted_after_tx: TxId,
        covered_layers: Vec<DeletionLayer>,
        excluded_scopes: Vec<ExcludedScope>,
        proof_key: &[u8],
    ) -> Result<Self> {
        // Keys sortieren für deterministischen Hash
        deleted_keys.sort();

        // Blake3 über alle sortierten Keys
        let mut hasher = blake3::Hasher::new();
        for key in &deleted_keys {
            hasher.update(key);
        }
        let deleted_keys_hash: [u8; 32] = *hasher.finalize().as_bytes();

        let scope_bytes = bincode::serialize(&scope)
            .map_err(|e| MemFuseError::Internal(e.to_string()))?;
        let tx_bytes = deleted_after_tx.0.to_le_bytes();

        let signature = compute_hmac_sha256(
            proof_key,
            &[&scope_bytes, &deleted_keys_hash, &tx_bytes],
        )?;

        Ok(Self {
            scope,
            deleted_keys_hash,
            deleted_after_tx,
            signature,
            covered_layers,
            excluded_scopes,
        })
    }

    /// Verifiziert Signatur (constant-time).
    /// NICHT-GARANTIE: Prüft nur Signatur, nicht ob Storage tatsächlich bereinigt ist.
    pub fn verify(&self, proof_key: &[u8]) -> Result<bool> {
        let scope_bytes = bincode::serialize(&self.scope)
            .map_err(|e| MemFuseError::Internal(e.to_string()))?;
        let tx_bytes = self.deleted_after_tx.0.to_le_bytes();

        let expected = compute_hmac_sha256(
            proof_key,
            &[&scope_bytes, &self.deleted_keys_hash, &tx_bytes],
        )?;

        use subtle::ConstantTimeEq;
        Ok(expected.ct_eq(&self.signature).into())
    }

    /// Exportiert Proof als JSON für Compliance-Dokumentation.
    /// ExcludedScope-Liste ist maschinenlesbar enthalten.
    pub fn export_for_audit(&self) -> Result<String> {
        serde_json::to_string_pretty(self)
            .map_err(|e| MemFuseError::Internal(e.to_string()))
    }

    /// Gibt die Tenant-ID aus dem Scope zurück.
    pub fn tenant_id(&self) -> TenantId {
        match &self.scope {
            DeletionScope::Document { tenant_id, .. } => *tenant_id,
            DeletionScope::Collection { tenant_id, .. } => *tenant_id,
            DeletionScope::Tenant { tenant_id } => *tenant_id,
        }
    }
}

fn compute_hmac_sha256(key: &[u8], data_parts: &[&[u8]]) -> Result<[u8; 32]> {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    let mut mac = Hmac::<Sha256>::new_from_slice(key)
        .map_err(|e| MemFuseError::Internal(format!("HMAC key error: {e}")))?;
    for part in data_parts {
        mac.update(part);
    }
    Ok(mac.finalize().into_bytes().into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use memfuse_core::{DocId, TenantId, TxId};

    fn test_key() -> Vec<u8> {
        vec![0u8; 32]
    }

    #[test]
    fn test_deletion_proof_create_and_verify() {
        let tenant = TenantId::try_new(1).unwrap();
        let scope = DeletionScope::Document {
            doc_id: DocId(42),
            tenant_id: tenant,
        };
        let keys = vec![b"key1".to_vec(), b"key2".to_vec()];
        let proof = DeletionProof::create(
            scope,
            keys,
            TxId(100),
            vec![DeletionLayer::LsmMemtable, DeletionLayer::HnswIndex],
            vec![ExcludedScope::LlmParameterMemory],
            &test_key(),
        )
        .unwrap();

        assert!(proof.verify(&test_key()).unwrap());
    }

    #[test]
    fn test_deletion_proof_wrong_key_fails_verify() {
        let scope = DeletionScope::Tenant {
            tenant_id: TenantId::try_new(1).unwrap(),
        };
        let proof = DeletionProof::create(
            scope,
            vec![],
            TxId(1),
            vec![],
            vec![],
            &test_key(),
        )
        .unwrap();

        let wrong_key = vec![1u8; 32];
        assert!(!proof.verify(&wrong_key).unwrap());
    }

    #[test]
    fn test_deletion_proof_tampered_data_fails_verify() {
        let scope = DeletionScope::Tenant {
            tenant_id: TenantId::try_new(1).unwrap(),
        };
        let proof = DeletionProof::create(
            scope,
            vec![b"k1".to_vec()],
            TxId(10),
            vec![DeletionLayer::LsmMemtable],
            vec![ExcludedScope::LlmParameterMemory],
            &test_key(),
        )
        .unwrap();

        // Tamper with deleted_after_tx
        let mut tampered_tx = proof.clone();
        tampered_tx.deleted_after_tx = TxId(11);
        assert!(!tampered_tx.verify(&test_key()).unwrap());

        // Tamper with deleted_keys_hash
        let mut tampered_hash = proof.clone();
        tampered_hash.deleted_keys_hash[0] ^= 0xFF;
        assert!(!tampered_hash.verify(&test_key()).unwrap());

        // Tamper with scope
        let mut tampered_scope = proof.clone();
        tampered_scope.scope = DeletionScope::Tenant {
            tenant_id: TenantId::try_new(2).unwrap(),
        };
        assert!(!tampered_scope.verify(&test_key()).unwrap());

        // Untampered original must verify successfully
        assert!(proof.verify(&test_key()).unwrap());
    }

    #[test]
    fn test_deletion_proof_key_order_deterministic() {
        // Gleiche Keys in anderer Reihenfolge → gleicher Hash (weil sortiert)
        let keys_a = vec![b"b".to_vec(), b"a".to_vec()];
        let keys_b = vec![b"a".to_vec(), b"b".to_vec()];

        let proof_a = DeletionProof::create(
            DeletionScope::Tenant {
                tenant_id: TenantId::try_new(2).unwrap(),
            },
            keys_a,
            TxId(1),
            vec![],
            vec![],
            &test_key(),
        )
        .unwrap();
        let proof_b = DeletionProof::create(
            DeletionScope::Tenant {
                tenant_id: TenantId::try_new(2).unwrap(),
            },
            keys_b,
            TxId(1),
            vec![],
            vec![],
            &test_key(),
        )
        .unwrap();

        assert_eq!(proof_a.deleted_keys_hash, proof_b.deleted_keys_hash);
    }

    #[test]
    fn test_deletion_proof_audit_export_contains_excluded_scopes() {
        let scope = DeletionScope::Tenant {
            tenant_id: TenantId::try_new(1).unwrap(),
        };
        let proof = DeletionProof::create(
            scope,
            vec![],
            TxId(1),
            vec![],
            vec![
                ExcludedScope::LlmParameterMemory,
                ExcludedScope::ConsolidatedAndDistilled,
            ],
            &test_key(),
        )
        .unwrap();

        let json = proof.export_for_audit().unwrap();
        assert!(json.contains("LlmParameterMemory"));
        assert!(json.contains("ConsolidatedAndDistilled"));
    }

    #[test]
    fn test_deletion_proof_tenant_id_extraction() {
        let t1 = TenantId::try_new(10).unwrap();
        let doc_scope = DeletionScope::Document {
            doc_id: DocId(1),
            tenant_id: t1,
        };
        let p1 = DeletionProof::create(doc_scope, vec![], TxId(1), vec![], vec![], &test_key()).unwrap();
        assert_eq!(p1.tenant_id(), t1);

        let t2 = TenantId::try_new(20).unwrap();
        let col_scope = DeletionScope::Collection {
            collection_id: CollectionId(5),
            tenant_id: t2,
        };
        let p2 = DeletionProof::create(col_scope, vec![], TxId(1), vec![], vec![], &test_key()).unwrap();
        assert_eq!(p2.tenant_id(), t2);

        let t3 = TenantId::try_new(30).unwrap();
        let tenant_scope = DeletionScope::Tenant { tenant_id: t3 };
        let p3 = DeletionProof::create(tenant_scope, vec![], TxId(1), vec![], vec![], &test_key()).unwrap();
        assert_eq!(p3.tenant_id(), t3);
    }
}
