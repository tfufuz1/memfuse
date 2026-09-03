// FILE-CONTEXT
// ZWECK: Kontextkompaktierung und Zusammenfassung langer Gesprächs- und Dokumentverläufe.
// INVARIANTEN: Provenance-Erhalt via Token-Budgeting; Rückfall auf Truncate/Summarize bei LLM-Ausfall.
// NICHT-OFFENSICHTLICH: StatusToken ermöglicht feingranulare Verfolgung des Kompaktierungszustands.
// STAND: TS:2026-08-29T17:22:29Z (SESSION: 0dcb9f3b)

// memfuse-db/src/context_compaction.rs
// Context Compaction Engine (Grok Pattern)

//! Context Compaction Engine (Grok Pattern)
//!
//! Replaces stale tool outputs and long conversation histories with compact status tokens
//! to preserve the LLM context window.

use crate::collection::Collection;
use memfuse_core::{ContextChunk, DocId, Result, StorageEngine, TokenBudget, TxId, VectorIndex};
use memfuse_ollama::OllamaClient;

/// Strategie für Context Compaction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompactionStrategy {
    /// Einfachste Strategie: Chunks über Token-Limit werden weggelassen.
    Truncate,
    /// Summarisierung via LLM (erfordert externen Summarizer-Trait).
    Summarize,
    /// Ersetze Tool-Outputs durch kompakte Status-Token.
    StatusToken,
    /// LLM-Zusammenfassung veralteter Chunks mit konfigurierbarem Batch-Limit.
    LlmSummarize {
        /// Maximale Anzahl von Chunks, die pro LLM-Aufruf zusammengefasst werden.
        max_input_chunks: usize,
    },
}

/// Kompaktierter Kontext für LLM-Übergabe.
#[derive(Debug, Clone)]
pub struct CompactedContext {
    /// Beibehaltene Chunks (innerhalb Budget).
    pub retained_chunks: Vec<ContextChunk>,
    /// Status-Token für kompaktierte Chunks.
    pub status_tokens: Vec<StatusToken>,
    /// Verbrauchte Tokens.
    pub tokens_used: usize,
    /// Ursprüngliche Quell-Dokument-IDs.
    pub source_doc_ids: Vec<DocId>,
}

/// Kompakter Stellvertreter für einen oder mehrere kompaktierte Chunks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusToken {
    /// Kompakter Beschreibungstext (z. B. "Tool-Output: DB-Abfrage lieferte 42 Ergebnisse").
    pub summary: String,
    /// Anzahl der ersetzten originalen Tokens.
    pub replaced_tokens: usize,
    /// Referenz auf die ersetzten Chunk-IDs.
    pub replaced_doc_ids: Vec<DocId>,
}

/// Context Compaction Engine.
#[derive(Debug)]
pub struct ContextCompactor {
    budget: TokenBudget,
    strategy: CompactionStrategy,
}

impl ContextCompactor {
    /// Creates a new `ContextCompactor` with given `TokenBudget` and `CompactionStrategy`.
    pub fn new(budget: TokenBudget, strategy: CompactionStrategy) -> Self {
        Self { budget, strategy }
    }

    /// Kompaktiert eine Liste von Chunks auf das Token-Budget.
    ///
    /// Priorisiert nach Relevanz-Score. Tool-Output-Chunks (erkennbar an Metadata-Key "tool_output")
    /// werden zuerst kompaktiert.
    pub fn compact(&self, chunks: Vec<ContextChunk>) -> CompactedContext {
        let _source_doc_ids: Vec<DocId> = chunks.iter().map(|c| c.doc_id).collect();
        let max_tokens = self.budget.available();
        let mut tokens_used = 0;
        let mut retained = Vec::new();
        let mut status_tokens = Vec::new();

        // Sortierung: Tool-Outputs ans Ende (werden zuerst kompaktiert)
        let mut sorted = chunks;
        sorted.sort_by(|a, b| {
            let a_tool = Self::is_tool_output(a);
            let b_tool = Self::is_tool_output(b);
            match (a_tool, b_tool) {
                (true, false) => std::cmp::Ordering::Greater,
                (false, true) => std::cmp::Ordering::Less,
                _ => b
                    .relevance
                    .partial_cmp(&a.relevance)
                    .unwrap_or(std::cmp::Ordering::Equal),
            }
        });

        for chunk in sorted {
            let chunk_tokens = chunk.combined_token_count();

            if tokens_used + chunk_tokens <= max_tokens {
                tokens_used += chunk_tokens;
                retained.push(chunk);
            } else {
                // Kompaktierung
                match self.strategy {
                    CompactionStrategy::StatusToken => {
                        let summary = Self::generate_status_token(&chunk);
                        status_tokens.push(StatusToken {
                            summary,
                            replaced_tokens: chunk_tokens,
                            replaced_doc_ids: vec![chunk.doc_id],
                        });
                    }
                    CompactionStrategy::Truncate => {
                        // Chunk wird verworfen
                    }
                    CompactionStrategy::Summarize | CompactionStrategy::LlmSummarize { .. } => {
                        // Synchronous fallback in compact(): Status-Token.
                        // For full async LLM summarization, call consolidate_via_llm().
                        let summary = Self::generate_status_token(&chunk);
                        status_tokens.push(StatusToken {
                            summary,
                            replaced_tokens: chunk_tokens,
                            replaced_doc_ids: vec![chunk.doc_id],
                        });
                    }
                }
            }
        }

        let source_doc_ids = retained.iter().map(|c| c.doc_id).collect();
        CompactedContext {
            retained_chunks: retained,
            status_tokens,
            tokens_used,
            source_doc_ids,
        }
    }

    // AI-TAG[SMELL][MINOR][RESOLVED] Async LLM-Summarization for context compaction (ID: AGT-DB-004) (TS:2026-08-28T00:00:00Z)
    /// Consolidates multiple context chunks into a single summarized chunk using an external LLM via Ollama.
    ///
    /// Preserves strict provenance tracking in `source_doc_ids`. If the LLM call fails, the error is
    /// returned directly to the caller (no silent fallback to `StatusToken`).
    pub async fn consolidate_via_llm(
        &self,
        chunks: &[ContextChunk],
        ollama: &OllamaClient,
    ) -> Result<CompactedContext> {
        if chunks.is_empty() {
            return Ok(CompactedContext {
                retained_chunks: Vec::new(),
                status_tokens: Vec::new(),
                tokens_used: 0,
                source_doc_ids: Vec::new(),
            });
        }

        let mut source_doc_ids = Vec::with_capacity(chunks.len());
        let mut prompt_content = String::new();

        for chunk in chunks {
            source_doc_ids.push(chunk.doc_id);
            prompt_content.push_str(&format!(
                "- Chunk [DocId: {}]: {}\n",
                chunk.doc_id.0, chunk.content
            ));
        }

        let prompt = format!(
            "Fasse die folgenden Kontext-Informationen faktentreu zu einem prägnanten Überblick zusammen.\n\
             Erhalte wichtige Details und wahre den Bezug zu den ursprünglichen Dokumenten.\n\n\
             Kontext-Chunks:\n{}\n\nZusammenfassung:",
            prompt_content
        );

        let model = &ollama.config().model;
        let summary_text = ollama.generate_text(model, &prompt).await?;

        let estimated_tokens = crate::context::ContextManager::estimate_tokens(&summary_text);

        // Combine metadata if present
        let mut combined_metadata = serde_json::Map::new();
        combined_metadata.insert("llm_summarized".to_string(), serde_json::Value::Bool(true));
        combined_metadata.insert(
            "source_doc_count".to_string(),
            serde_json::Value::Number(chunks.len().into()),
        );

        // Generate a distinct deterministic DocId from the combination of source doc_ids
        let synthesized_doc_id = {
            let key = chunks
                .iter()
                .map(|c| c.doc_id.inner().to_string())
                .collect::<Vec<_>>()
                .join(":");
            DocId::from_key(&format!("memfuse:consolidated:{key}")).unwrap_or(chunks[0].doc_id)
        };

        let max_relevance = chunks.iter().fold(0.0f32, |max, c| max.max(c.relevance));

        let consolidated_chunk = ContextChunk {
            doc_id: synthesized_doc_id,
            content: summary_text,
            relevance: max_relevance,
            token_count: estimated_tokens,
            metadata: Some(serde_json::Value::Object(combined_metadata)),
            contextual_prefix: None,
            links: Vec::new(),
        };

        let tokens_used = consolidated_chunk.combined_token_count();

        Ok(CompactedContext {
            retained_chunks: vec![consolidated_chunk],
            status_tokens: Vec::new(),
            tokens_used,
            source_doc_ids,
        })
    }

    fn is_tool_output(chunk: &ContextChunk) -> bool {
        chunk
            .metadata
            .as_ref()
            .and_then(|m| m.get("tool_output"))
            .is_some()
    }

    fn generate_status_token(chunk: &ContextChunk) -> String {
        let preview: String = chunk.content.chars().take(80).collect();
        format!(
            "[Kompaktiert: {} Tokens — {}...]",
            chunk.token_count, preview
        )
    }
}

/// Optimistic Concurrency Control (OCC) Consolidation Session for Sleep-Cycle Memory Compaction.
///
/// Prevents lost updates / phantom erasures by verifying that no source documents were modified
/// while asynchronous LLM summarization was in progress. Also journals a `CommitIntent::Consolidation`
/// entry into storage for crash resilience (INV-CONSOLIDATE-1, INV-CONSOLIDATE-2).
pub struct ConsolidationSession<'a, S: StorageEngine, V: VectorIndex = memfuse_index::HnswIndex> {
    /// Reference to the active collection.
    pub collection: &'a Collection<S, V>,
    /// Source document IDs and their transaction IDs captured at read snapshot time.
    pub source_docs: Vec<(DocId, TxId)>,
    /// Storage key for the consolidation intent in WAL/LSM.
    pub intent_key: Vec<u8>,
    /// Target document identifier for the synthesized memory.
    pub target_id: DocId,
    /// Base transaction ID allocated when session started.
    pub base_tx: TxId,
}

impl<'a, S: StorageEngine, V: VectorIndex> ConsolidationSession<'a, S, V> {
    /// Starts a consolidation session, snapshotting the version of each source document
    /// and writing a `CommitIntent::Consolidation` intent to storage.
    pub async fn start(
        collection: &'a Collection<S, V>,
        source_doc_ids: &[DocId],
        target_id: DocId,
    ) -> Result<Self> {
        let base_tx = collection.allocate_tx()?;
        let mut source_docs = Vec::with_capacity(source_doc_ids.len());
        for &doc_id in source_doc_ids {
            let tx = collection.get_doc_tx(doc_id).await?.unwrap_or(base_tx);
            source_docs.push((doc_id, tx));
        }

        let intent_key = collection.namespaced_key(&target_id.inner().to_le_bytes(), 3);
        let intent = crate::transaction::CommitIntent::Consolidation {
            source_docs: source_docs.clone(),
            target_id,
            base_tx,
        };
        let intent_bytes = serde_json::to_vec(&intent)?;
        collection
            .storage()
            .put(base_tx, &intent_key, &intent_bytes)
            .await?;
        collection.storage().commit(base_tx).await?;

        Ok(Self {
            collection,
            source_docs,
            intent_key,
            target_id,
            base_tx,
        })
    }

    /// Validates optimistic concurrency control: checks that every source document
    /// has NOT been mutated since the consolidation session started.
    pub async fn validate_occ(&self) -> Result<()> {
        for &(doc_id, expected_tx) in &self.source_docs {
            match self.collection.get_doc_tx(doc_id).await? {
                Some(current_tx) => {
                    if current_tx.inner() > expected_tx.inner() {
                        return Err(memfuse_core::MemFuseError::StaleRead(format!(
                            "OCC conflict: Document {:?} was mutated during consolidation (snapshot tx={}, current tx={})",
                            doc_id, expected_tx, current_tx
                        )));
                    }
                }
                None => {
                    return Err(memfuse_core::MemFuseError::StaleRead(format!(
                        "OCC conflict: Document {:?} was deleted or missing during consolidation",
                        doc_id
                    )));
                }
            }
        }
        Ok(())
    }

    /// Refreshes the consolidation session by reading the current transaction IDs of all source documents.
    /// This is used to retry consolidation after an OCC conflict, allowing the caller to re-summarize
    /// only the documents that have actually changed.
    /// Returns a list of document IDs that were mutated or deleted since the session started.
    pub async fn refresh(&mut self) -> Result<Vec<DocId>> {
        let mut changed_docs = Vec::new();
        let mut new_source_docs = Vec::with_capacity(self.source_docs.len());

        let new_base_tx = self.collection.allocate_tx()?;

        for &(doc_id, expected_tx) in &self.source_docs {
            match self.collection.get_doc_tx(doc_id).await? {
                Some(current_tx) => {
                    if current_tx.inner() > expected_tx.inner() {
                        changed_docs.push(doc_id);
                    }
                    new_source_docs.push((doc_id, current_tx));
                }
                None => {
                    changed_docs.push(doc_id);
                    // If deleted, we just track the current new_base_tx
                    new_source_docs.push((doc_id, new_base_tx));
                }
            }
        }

        self.source_docs = new_source_docs;

        // Update intent in storage
        let intent = crate::transaction::CommitIntent::Consolidation {
            source_docs: self.source_docs.clone(),
            target_id: self.target_id,
            base_tx: new_base_tx,
        };
        let intent_bytes = serde_json::to_vec(&intent)?;
        self.collection
            .storage()
            .put(new_base_tx, &self.intent_key, &intent_bytes)
            .await?;
        self.collection.storage().commit(new_base_tx).await?;
        self.base_tx = new_base_tx;

        Ok(changed_docs)
    }

    /// Cancels / aborts the consolidation session, removing the intent key.
    pub async fn abort(self) -> Result<()> {
        let abort_tx = self.collection.allocate_tx()?;
        self.collection
            .storage()
            .delete(abort_tx, &self.intent_key)
            .await?;
        self.collection.storage().commit(abort_tx).await?;
        Ok(())
    }

    /// Commits the consolidated document and removes the source documents.
    pub async fn commit(
        self,
        target_string_id: &str,
        embedding: &[f32],
        summary_content: &str,
        metadata: Option<serde_json::Value>,
    ) -> Result<()> {
        let _guard = self.collection.insert_lock.lock().await;

        // 1. Strict OCC validation under lock
        self.validate_occ().await?;

        // 2. Prepare transaction
        let mut db_tx = self.collection.begin_transaction()?;

        // 3. Insert target consolidated document
        let mut final_metadata = metadata.unwrap_or_else(|| serde_json::json!({}));
        if let Some(obj) = final_metadata.as_object_mut() {
            obj.insert("consolidated".to_string(), serde_json::json!(true));
            obj.insert(
                "source_doc_ids".to_string(),
                serde_json::json!(self
                    .source_docs
                    .iter()
                    .map(|(d, _)| d.inner())
                    .collect::<Vec<_>>()),
            );
            obj.insert("summary".to_string(), serde_json::json!(summary_content));
        }

        self.collection
            .insert_op(&db_tx, target_string_id, embedding, Some(final_metadata))
            .await?;

        // 4. Delete source docs
        for &(src_id, _) in &self.source_docs {
            let doc_key = self
                .collection
                .namespaced_key(&src_id.inner().to_le_bytes(), 1);
            if let Some(val) = self.collection.storage().get(&doc_key).await? {
                let meta: crate::collection::StoredDocumentMeta = serde_json::from_slice(&val)?;
                self.collection.delete_op(&mut db_tx, &meta.id).await?;
            }
        }

        // 5. Delete intent key
        let commit_tx = db_tx.tx_id;
        self.collection
            .storage()
            .delete(commit_tx, &self.intent_key)
            .await?;

        // 6. Commit transaction
        db_tx.commit().await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_chunk(id: u64, content: &str, relevance: f32, is_tool: bool) -> ContextChunk {
        let metadata = if is_tool {
            Some(serde_json::json!({"tool_output": true}))
        } else {
            None
        };
        ContextChunk {
            doc_id: DocId::new(id),
            content: content.to_string(),
            relevance,
            token_count: content.len(),
            metadata,
            contextual_prefix: None,
            links: Vec::new(),
        }
    }

    #[test]
    fn test_compactor_within_budget() {
        let budget = TokenBudget::new(100, 0);
        let compactor = ContextCompactor::new(budget, CompactionStrategy::StatusToken);

        let chunks = vec![
            make_chunk(1, "chunk 1", 0.9, false),
            make_chunk(2, "chunk 2", 0.8, false),
        ];

        let result = compactor.compact(chunks);
        assert_eq!(result.retained_chunks.len(), 2);
        assert!(result.status_tokens.is_empty());
        assert_eq!(result.tokens_used, "chunk 1".len() + "chunk 2".len());
    }

    #[test]
    fn test_compactor_tool_output_compacted_first() {
        let budget = TokenBudget::new(20, 0);
        let compactor = ContextCompactor::new(budget, CompactionStrategy::StatusToken);

        let chunks = vec![
            make_chunk(1, "tool output data 12345", 0.95, true), // 22 bytes, tool output
            make_chunk(2, "important context", 0.8, false),      // 17 bytes, normal
        ];

        let result = compactor.compact(chunks);
        // Important context should be retained (17 bytes <= 20 max_tokens),
        // tool output should be sorted last and converted to status token.
        assert_eq!(result.retained_chunks.len(), 1);
        assert_eq!(result.retained_chunks[0].doc_id, DocId::new(2));
        assert_eq!(result.status_tokens.len(), 1);
        assert_eq!(
            result.status_tokens[0].replaced_doc_ids,
            vec![DocId::new(1)]
        );
    }

    #[test]
    fn test_compactor_truncate_strategy() {
        let budget = TokenBudget::new(10, 0);
        let compactor = ContextCompactor::new(budget, CompactionStrategy::Truncate);

        let chunks = vec![
            make_chunk(1, "small", 0.9, false),            // 5 bytes
            make_chunk(2, "exceeding budget", 0.8, false), // 16 bytes
        ];

        let result = compactor.compact(chunks);
        assert_eq!(result.retained_chunks.len(), 1);
        assert_eq!(result.retained_chunks[0].doc_id, DocId::new(1));
        assert!(result.status_tokens.is_empty());
    }

    #[test]
    fn test_compactor_summarize_strategy_fallback() {
        let budget = TokenBudget::new(10, 0);
        let compactor = ContextCompactor::new(budget, CompactionStrategy::Summarize);

        let chunks = vec![
            make_chunk(1, "small", 0.9, false),
            make_chunk(2, "large content for summarization", 0.8, false),
        ];

        let result = compactor.compact(chunks);
        assert_eq!(result.retained_chunks.len(), 1);
        assert_eq!(result.status_tokens.len(), 1);
        assert_eq!(
            result.status_tokens[0].replaced_doc_ids,
            vec![DocId::new(2)]
        );
    }

    #[test]
    fn test_compact_with_contextual_prefix_respects_budget() {
        let budget = TokenBudget::new(20, 0); // 20 tokens available
        let compactor = ContextCompactor::new(budget, CompactionStrategy::Truncate);

        // Chunk: token_count=10, prefix adds ~5 tokens → combined=15
        let chunk = ContextChunk {
            doc_id: DocId::new(1),
            content: "content".to_string(),
            relevance: 1.0,
            token_count: 10,
            metadata: None,
            contextual_prefix: Some("1234567890123456789012".to_string()), // 22 chars → +5 tokens
            links: Vec::new(),
        };

        // Without prefix: chunk fits (10 <= 20)
        // With prefix: chunk fits (15 <= 20)
        let result = compactor.compact(vec![chunk]);
        // combined_token_count=15 <= budget=20 → retained
        assert_eq!(result.retained_chunks.len(), 1);
        assert_eq!(result.tokens_used, 15); // combined_token_count, not raw token_count
    }

    #[tokio::test]
    async fn test_consolidate_via_llm_error_propagation_on_unreachable_client() {
        let budget = TokenBudget::new(100, 0);
        let compactor = ContextCompactor::new(
            budget,
            CompactionStrategy::LlmSummarize {
                max_input_chunks: 5,
            },
        );

        // Client pointing to an unreachable / closed port
        let dead_client = OllamaClient::new("http://127.0.0.1:1");

        let chunks = vec![
            make_chunk(101, "First chunk content", 0.9, false),
            make_chunk(102, "Second chunk content", 0.8, false),
        ];

        let res = compactor.consolidate_via_llm(&chunks, &dead_client).await;
        // Must return an Error and NOT fall back silently to StatusToken inside compaction.rs
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn test_consolidate_via_llm_provenance_and_empty() {
        let budget = TokenBudget::new(100, 0);
        let compactor = ContextCompactor::new(budget, CompactionStrategy::Summarize);
        let dead_client = OllamaClient::new("http://127.0.0.1:1");

        // Empty chunks slice test
        let empty_res = compactor.consolidate_via_llm(&[], &dead_client).await;
        assert!(empty_res.is_ok());
        let empty_ctx = empty_res.unwrap(); // unwrap allowed (in test)
        assert!(empty_ctx.retained_chunks.is_empty());
        assert!(empty_ctx.source_doc_ids.is_empty());
    }

    #[test]
    fn test_compact_empty_chunks_returns_empty_compacted_context() {
        let budget = TokenBudget::new(100, 0);
        let compactor = ContextCompactor::new(budget, CompactionStrategy::Truncate);
        let result = compactor.compact(vec![]);
        assert!(result.retained_chunks.is_empty());
        assert!(result.status_tokens.is_empty());
        assert_eq!(result.tokens_used, 0);
        assert!(result.source_doc_ids.is_empty());
    }

    use memfuse_core::StorageStats;
    use memfuse_graph::CsrGraph;
    use memfuse_index::{HnswConfig, HnswIndex};
    use memfuse_store::{LsmConfig, LsmStorage};
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::sync::Arc;

    #[derive(Clone)]
    struct FaultyDeleteStorage {
        inner: Arc<LsmStorage>,
        fail_delete: Arc<AtomicBool>,
    }

    #[async_trait::async_trait]
    impl StorageEngine for FaultyDeleteStorage {
        async fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
            self.inner.get(key).await
        }

        async fn get_at_seq(&self, key: &[u8], seq: u64) -> Result<Option<Vec<u8>>> {
            self.inner.get_at_seq(key, seq).await
        }

        async fn put(&self, tx_id: TxId, key: &[u8], value: &[u8]) -> Result<()> {
            self.inner.put(tx_id, key, value).await
        }

        async fn delete(&self, tx_id: TxId, key: &[u8]) -> Result<()> {
            if self.fail_delete.load(Ordering::SeqCst) {
                return Err(memfuse_core::MemFuseError::Transaction(
                    "INJECTED FAULT: Storage delete failure".into(),
                ));
            }
            self.inner.delete(tx_id, key).await
        }

        async fn commit(&self, tx_id: TxId) -> Result<()> {
            self.inner.commit(tx_id).await
        }

        async fn rollback(&self, tx_id: TxId) -> Result<()> {
            self.inner.rollback(tx_id).await
        }

        async fn rollback_to_tx(&self, tx_id: TxId) -> Result<()> {
            self.inner.rollback_to_tx(tx_id).await
        }

        async fn flush(&self) -> Result<()> {
            self.inner.flush().await
        }

        async fn stats(&self) -> Result<StorageStats> {
            self.inner.stats().await
        }

        async fn last_seq_no(&self) -> Result<u64> {
            self.inner.last_seq_no().await
        }

        async fn last_tx_id(&self) -> Result<TxId> {
            self.inner.last_tx_id().await
        }

        async fn pin_checkpoint(&self, seq_no: u64) -> Result<()> {
            self.inner.pin_checkpoint(seq_no).await
        }

        async fn unpin_checkpoint(&self, seq_no: u64) -> Result<()> {
            self.inner.unpin_checkpoint(seq_no).await
        }

        async fn scan_prefix(&self, prefix: &[u8]) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
            self.inner.scan_prefix(prefix).await
        }

        async fn scan_prefix_at(
            &self,
            prefix: &[u8],
            seq_no: u64,
        ) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
            self.inner.scan_prefix_at(prefix, seq_no).await
        }

        async fn scan(
            &self,
            start: std::ops::Bound<&[u8]>,
            end: std::ops::Bound<&[u8]>,
        ) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
            self.inner.scan(start, end).await
        }
    }

    #[tokio::test]
    async fn test_consolidation_commit_aborts_on_delete_failure(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        let tmp = tempfile::tempdir()?;
        let lsm_config = LsmConfig {
            path: tmp.path().to_path_buf(),
            ..Default::default()
        };
        let lsm = Arc::new(LsmStorage::new(lsm_config).await?);
        let fail_delete = Arc::new(AtomicBool::new(false));
        let faulty_storage = FaultyDeleteStorage {
            inner: lsm,
            fail_delete: fail_delete.clone(),
        };

        let dim = 4;
        let hnsw_config = HnswConfig {
            dimension: dim,
            ..Default::default()
        };
        let hnsw = Arc::new(HnswIndex::try_new(hnsw_config)?);
        let graph = Arc::new(CsrGraph::with_storage(Arc::new(faulty_storage.clone())));
        let next_tx = Arc::new(AtomicU64::new(1));

        let col = Collection::new(
            "test_col".to_string(),
            Arc::new(faulty_storage),
            hnsw,
            graph,
            next_tx,
            dim,
            memfuse_text::Language::English,
        );

        // 1. Insert source document
        let src_id_str = "source_doc_1";
        let src_doc_id = DocId::from_key(src_id_str)?;
        col.insert(
            src_id_str,
            &[0.1, 0.2, 0.3, 0.4],
            Some(serde_json::json!({"text": "source text"})),
        )
        .await?;

        // 2. Start consolidation session
        let target_str_id = "summary_target_doc";
        let target_doc_id = DocId::from_key(target_str_id)?;
        let session = ConsolidationSession::start(&col, &[src_doc_id], target_doc_id).await?;

        // 3. Configure delete_op to fail via storage delete failure
        fail_delete.store(true, Ordering::SeqCst);

        // 4. Call commit
        let res = session
            .commit(
                target_str_id,
                &[0.1, 0.2, 0.3, 0.4],
                "Summary of source doc",
                None,
            )
            .await;

        // 5. Assert commit returned Err
        assert!(
            res.is_err(),
            "commit() must return Err when delete_op fails"
        );

        // 6. Assert summary target doc was not persisted
        let summary_doc = col.get(target_str_id).await?;
        assert!(
            summary_doc.is_none(),
            "Target summary document must not be persisted if commit aborts"
        );

        Ok(())
    }
}
