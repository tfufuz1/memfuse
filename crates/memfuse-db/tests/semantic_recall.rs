// ANCHOR[PERF:EVAL-001] STATUS:DONE (TS:2026-08-29T00:00:00Z) — Semantic Retrieval Evaluation Framework
// ZIEL: Parameterisierte Recall@k (k=5, 10, 20) Messung für hybrid_search gegen Ground Truth
// AGENT:09 DATE:2026-08-29 STATUS:DONE

use memfuse_db::MemFuse;
use std::collections::HashSet;
use tempfile::TempDir;

const NUM_CLUSTERS: usize = 20;
const DOCS_PER_CLUSTER: usize = 50;
const EMBED_DIM: usize = 768;

/// Deterministic cluster topic definitions with distinct keyword sets
struct ClusterTopic {
    id: usize,
    name: &'static str,
    keywords: Vec<&'static str>,
}

fn get_cluster_topics() -> Vec<ClusterTopic> {
    vec![
        ClusterTopic { id: 0, name: "Database Storage LSM", keywords: vec!["sstable", "memtable", "wal", "compaction", "write-ahead-log"] },
        ClusterTopic { id: 1, name: "Vector Index HNSW", keywords: vec!["hnsw", "nearest-neighbor", "graph-index", "vector-search", "ef-search"] },
        ClusterTopic { id: 2, name: "BM25 Text Search", keywords: vec!["bm25", "inverted-index", "tf-idf", "tokenizer", "term-frequency"] },
        ClusterTopic { id: 3, name: "Graph CSR PPR", keywords: vec!["csr-graph", "pagerank", "personalized-pagerank", "entity-relation", "graph-traversal"] },
        ClusterTopic { id: 4, name: "Agent Checkpoints", keywords: vec!["agent-state", "checkpoint", "transaction-rollback", "state-snapshot", "persisted-workflow"] },
        ClusterTopic { id: 5, name: "Quantization SQ8", keywords: vec!["quantization", "sq8", "scalar-quantizer", "compression", "vector-codebook"] },
        ClusterTopic { id: 6, name: "SIMD Acceleration", keywords: vec!["simd", "avx2", "neon", "distance-metric", "dot-product"] },
        ClusterTopic { id: 7, name: "Ollama Embeddings", keywords: vec!["ollama", "embedding-model", "nomic-embed", "dense-vectors", "semantic-representation"] },
        ClusterTopic { id: 8, name: "Python Bindings", keywords: vec!["pyo3", "python-ffi", "search-result-doc", "maturin", "gil-safety"] },
        ClusterTopic { id: 9, name: "Tauri Desktop IPC", keywords: vec!["tauri", "error-dto", "ipc-command", "desktop-gui", "front-end-bridge"] },
        ClusterTopic { id: 10, name: "Security Encryption", keywords: vec!["blake3", "encryption-at-rest", "aes-gcm", "key-derivation", "crypto-nonce"] },
        ClusterTopic { id: 11, name: "ACID Concurrency", keywords: vec!["isolation-level", "2pc", "atomicity", "read-committed", "mvcc-snapshot"] },
        ClusterTopic { id: 12, name: "Context Compaction", keywords: vec!["context-window", "summarization", "llm-compactor", "token-budget", "provenance"] },
        ClusterTopic { id: 13, name: "FastAPI MCP Protocol", keywords: vec!["mcp-server", "model-context-protocol", "json-rpc", "tool-call", "agent-hub"] },
        ClusterTopic { id: 14, name: "DiskANN Vamana Graph", keywords: vec!["diskann", "vamana", "disk-backed-index", "beam-search", "sector-aligned"] },
        ClusterTopic { id: 15, name: "Distributed Raft Consensus", keywords: vec!["raft", "consensus", "log-replication", "leader-election", "quorum"] },
        ClusterTopic { id: 16, name: "Onnx Model Runtime", keywords: vec!["onnxruntime", "local-embeddings", "tensor-computation", "batch-inference", "model-weights"] },
        ClusterTopic { id: 17, name: "Garbage Collection Reaper", keywords: vec!["ttl-expiration", "reaper", "tombstone-cleanup", "vacuum", "expired-keys"] },
        ClusterTopic { id: 18, name: "Hybrid Signal Fusion", keywords: vec!["rrf", "reciprocal-rank-fusion", "signal-weight", "score-normalization", "fusion-weight"] },
        ClusterTopic { id: 19, name: "Performance Criterion Bench", keywords: vec!["criterion", "percentile-latency", "throughput", "microbenchmark", "performance-regression"] },
    ]
}

/// Generates a cluster base vector and document vector with controlled noise
fn generate_cluster_vector(cluster_id: usize, doc_within_cluster: usize) -> Vec<f32> {
    let mut vec = vec![0.0f32; EMBED_DIM];
    // Base direction per cluster using orthogonal-ish phase
    for i in 0..EMBED_DIM {
        let angle = (cluster_id as f32 + 1.0) * (i as f32 + 1.0) * 0.01;
        let base_signal = angle.sin();
        // Controlled noise for document variant
        let noise = ((doc_within_cluster * 13 + i * 7) % 100) as f32 / 500.0;
        vec[i] = base_signal + noise;
    }
    // Normalize vector
    let norm: f32 = vec.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        for x in vec.iter_mut() {
            *x /= norm;
        }
    }
    vec
}

/// Generates a query vector for a cluster
fn generate_query_vector(cluster_id: usize, query_idx: usize) -> Vec<f32> {
    let mut vec = vec![0.0f32; EMBED_DIM];
    for i in 0..EMBED_DIM {
        let angle = (cluster_id as f32 + 1.0) * (i as f32 + 1.0) * 0.01;
        let base_signal = angle.sin();
        let noise = ((query_idx * 19 + i * 3) % 100) as f32 / 1000.0;
        vec[i] = base_signal + noise;
    }
    let norm: f32 = vec.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        for x in vec.iter_mut() {
            *x /= norm;
        }
    }
    vec
}

#[tokio::test]
async fn test_semantic_recall_evaluation() {
    let tmp = TempDir::new().unwrap(); // unwrap allowed
    let db = MemFuse::open(tmp.path()).await.unwrap(); // unwrap allowed

    let topics = get_cluster_topics();
    assert_eq!(topics.len(), NUM_CLUSTERS);

    // Track doc IDs by cluster for ground truth validation
    let mut cluster_doc_ids: Vec<HashSet<String>> = vec![HashSet::new(); NUM_CLUSTERS];

    // Populate index: 20 clusters x 50 docs = 1000 docs
    for topic in &topics {
        for d in 0..DOCS_PER_CLUSTER {
            let doc_id = format!("cluster-{:02}-doc-{:02}", topic.id, d);
            let vector = generate_cluster_vector(topic.id, d);

            // Text contains cluster keywords + specific content
            let kw_sample = topic.keywords[d % topic.keywords.len()];
            let text = format!(
                "Technical document regarding {} domain focusing on {} key concept.",
                topic.name, kw_sample
            );

            db.insert(
                &doc_id,
                &vector,
                Some(serde_json::json!({
                    "text": text,
                    "cluster_id": topic.id,
                    "doc_index": d
                })),
            )
            .await
            .unwrap(); // unwrap allowed

            cluster_doc_ids[topic.id].insert(doc_id);
        }
    }

    // Evaluate 5 test queries per cluster
    let mut total_queries = 0;
    let mut sum_recall_at_5 = 0.0f64;
    let mut sum_recall_at_10 = 0.0f64;
    let mut sum_recall_at_20 = 0.0f64;

    for topic in &topics {
        let ground_truth = &cluster_doc_ids[topic.id];

        for q in 0..5 {
            let query_vec = generate_query_vector(topic.id, q);
            let query_kw = topic.keywords[q % topic.keywords.len()];
            let query_text = format!("{} {}", topic.name, query_kw);

            // Execute hybrid_search for top 20
            let results = db
                .hybrid_search(&query_text, &query_vec, 20, None)
                .await
                .unwrap(); // unwrap allowed

            total_queries += 1;

            let evaluate_recall_at_k = |k: usize| -> f64 {
                let retrieved_at_k: HashSet<String> = results
                    .iter()
                    .take(k)
                    .map(|r| r.id.clone())
                    .collect();
                let relevant_and_retrieved = retrieved_at_k.intersection(ground_truth).count();
                // Recall@k = (relevant documents retrieved in top k) / min(k, total relevant documents)
                let max_possible = k.min(ground_truth.len());
                (relevant_and_retrieved as f64) / (max_possible as f64)
            };

            sum_recall_at_5 += evaluate_recall_at_k(5);
            sum_recall_at_10 += evaluate_recall_at_k(10);
            sum_recall_at_20 += evaluate_recall_at_k(20);
        }
    }

    let mean_recall_at_5 = sum_recall_at_5 / (total_queries as f64);
    let mean_recall_at_10 = sum_recall_at_10 / (total_queries as f64);
    let mean_recall_at_20 = sum_recall_at_20 / (total_queries as f64);

    println!("\n=== SEMANTIC RETRIEVAL EVALUATION RESULTS (1000 DOCS, 100 QUERIES) ===");
    println!("Mean Recall@5:  {:.4}", mean_recall_at_5);
    println!("Mean Recall@10: {:.4}", mean_recall_at_10);
    println!("Mean Recall@20: {:.4}", mean_recall_at_20);

    // Enforce high quality benchmark threshold
    // hybrid_search fuses 4 signals (dense vector, BM25 text, etc.)
    assert!(
        mean_recall_at_10 >= 0.80,
        "Mean Recall@10 standard expected >= 0.80, got {:.4}",
        mean_recall_at_10
    );
}
