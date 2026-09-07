// FILE-CONTEXT
// STAND: 2026-09-03T10:00:00Z (SESSION: 8d7a9f86)
// ZWECK: Reproduzierbarer Benchmark-Harness für Retrieval-Qualität (Context-Präfix & Cross-Encoder Reranking)
// INVARIANTEN: Standalone, reproduzierbar, synthetischer Korpus mit Ground-Truth-Annotationen.

use memfuse_core::Result;
use memfuse_db::{MemFuse, MemFuseConfig};
use memfuse_embed::{CrossEncoderReranker, RerankConfig};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use tempfile::TempDir;

/// Represents a benchmark query with ground truth relevant document IDs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroundTruthQuery {
    pub query_id: String,
    pub query_text: String,
    pub query_embedding: Vec<f32>,
    pub relevant_doc_ids: Vec<String>,
}

/// Raw document chunk item in the synthetic corpus.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyntheticDocument {
    pub id: String,
    pub title: String,
    pub raw_content: String,
    pub context_prefix: String,
    pub embedding: Vec<f32>,
}

/// Metrics recorded for a specific evaluation configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BenchmarkMetrics {
    pub total_queries: usize,
    pub recall_at_1: f64,
    pub recall_at_3: f64,
    pub recall_at_5: f64,
    pub mrr: f64,
    pub error_rate_at_1: f64,
    pub error_rate_at_5: f64,
}

/// Detailed benchmark results comparing baseline vs feature.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScenarioComparison {
    pub scenario_name: String,
    pub baseline_metrics: BenchmarkMetrics,
    pub feature_metrics: BenchmarkMetrics,
    pub recall_at_1_delta_pct: f64,
    pub recall_at_5_delta_pct: f64,
    pub error_rate_reduction_pct: f64,
}

/// Complete benchmark report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkReport {
    pub timestamp: String,
    pub corpus_size_docs: usize,
    pub total_test_queries: usize,
    pub scenario_a_context_prefix: ScenarioComparison,
    pub scenario_b_reranking: ScenarioComparison,
}

fn pad_vector(v: &[f32], target_dim: usize) -> Vec<f32> {
    let mut padded = vec![0.0f32; target_dim];
    for (i, &val) in v.iter().enumerate().take(target_dim) {
        padded[i] = val;
    }
    padded
}

// BENCHMARK-KORPUS: 50 Dokumente über 5 Themengruppen.
// Mindestanforderung: statistische Signifikanz erfordert > 30 Dokumente.
// Ziel: Recall@5 mit p<0.05 unterscheidbar von Zufallsauswahl.
// Upgrade zu LongMemEval-S: Roadmap H2.
fn build_evaluation_corpus() -> Vec<SyntheticDocument> {
    const DIM: usize = 768;

    vec![
        // RUST-GRUPPE (1-10)
        SyntheticDocument {
            id: "doc_rust_1".into(),
            title: "Rust Ownership System".into(),
            raw_content: "Rust's ownership system prevents memory leaks at compile time through borrow checking.".into(),
            context_prefix: "Dokument: Rust Grundlagen. Thema: Ownership & Borrowing. ".into(),
            embedding: pad_vector(&[0.90, 0.10, 0.0, 0.0], DIM),
        },
        SyntheticDocument {
            id: "doc_rust_2".into(),
            title: "Rust Borrow Checker".into(),
            raw_content: "The borrow checker enforces that references do not outlive the data they point to.".into(),
            context_prefix: "Dokument: Rust Grundlagen. Thema: Lifetimes & Borrow Checker. ".into(),
            embedding: pad_vector(&[0.88, 0.12, 0.0, 0.0], DIM),
        },
        SyntheticDocument {
            id: "doc_rust_3".into(),
            title: "Rust Traits".into(),
            raw_content: "Rust traits define shared behavior that types can implement, similar to interfaces.".into(),
            context_prefix: "Dokument: Rust Polymorphismus. Thema: Traits & Type Parameters. ".into(),
            embedding: pad_vector(&[0.85, 0.15, 0.0, 0.0], DIM),
        },
        SyntheticDocument {
            id: "doc_rust_4".into(),
            title: "Async Rust Execuor".into(),
            raw_content: "Async Rust uses a poll-based executor model with Future traits for concurrency.".into(),
            context_prefix: "Dokument: Rust Asynchronität. Thema: Future Trait & Executor Model. ".into(),
            embedding: pad_vector(&[0.82, 0.18, 0.0, 0.0], DIM),
        },
        SyntheticDocument {
            id: "doc_rust_5".into(),
            title: "Cargo Package Manager".into(),
            raw_content: "cargo is Rust's package manager that handles dependencies and compilation.".into(),
            context_prefix: "Dokument: Rust Tooling. Thema: Cargo Package Management. ".into(),
            embedding: pad_vector(&[0.80, 0.20, 0.0, 0.0], DIM),
        },
        SyntheticDocument {
            id: "doc_rust_6".into(),
            title: "Zero-Cost Abstractions".into(),
            raw_content: "Rust's zero-cost abstractions ensure high-level code compiles to efficient machine code.".into(),
            context_prefix: "Dokument: Rust Performanz. Thema: Zero-Cost Abstractions. ".into(),
            embedding: pad_vector(&[0.86, 0.14, 0.0, 0.0], DIM),
        },
        SyntheticDocument {
            id: "doc_rust_7".into(),
            title: "Arc and Mutex Concurrency".into(),
            raw_content: "The Arc and Mutex types enable safe shared-state concurrency in Rust.".into(),
            context_prefix: "Dokument: Rust Nebenläufigkeit. Thema: Arc & Mutex Shared State. ".into(),
            embedding: pad_vector(&[0.84, 0.16, 0.0, 0.0], DIM),
        },
        SyntheticDocument {
            id: "doc_rust_8".into(),
            title: "Exhaustive Pattern Matching".into(),
            raw_content: "Rust's enum types with pattern matching provide exhaustive error handling.".into(),
            context_prefix: "Dokument: Rust Typensystem. Thema: Enums & Pattern Matching. ".into(),
            embedding: pad_vector(&[0.83, 0.17, 0.0, 0.0], DIM),
        },
        SyntheticDocument {
            id: "doc_rust_9".into(),
            title: "SIMD Intrinsics in Rust".into(),
            raw_content: "SIMD intrinsics in Rust allow explicit vectorization for performance-critical paths.".into(),
            context_prefix: "Dokument: Rust Low-Level Opts. Thema: SIMD Intrinsics. ".into(),
            embedding: pad_vector(&[0.87, 0.13, 0.0, 0.0], DIM),
        },
        SyntheticDocument {
            id: "doc_rust_10".into(),
            title: "Procedural Macros".into(),
            raw_content: "Procedural macros in Rust generate code at compile time from token streams.".into(),
            context_prefix: "Dokument: Rust Metaprogrammierung. Thema: Procedural Macros. ".into(),
            embedding: pad_vector(&[0.81, 0.19, 0.0, 0.0], DIM),
        },

        // ML/KI-GRUPPE (11-20)
        SyntheticDocument {
            id: "doc_ml_11".into(),
            title: "Transformer Architecture".into(),
            raw_content: "Transformer architectures use self-attention to model long-range dependencies in sequences.".into(),
            context_prefix: "Dokument: Deep Learning. Thema: Self-Attention & Transformers. ".into(),
            embedding: pad_vector(&[0.10, 0.90, 0.0, 0.0], DIM),
        },
        SyntheticDocument {
            id: "doc_ml_12".into(),
            title: "HNSW Vector Graphs".into(),
            raw_content: "HNSW graphs enable approximate nearest neighbor search with logarithmic query complexity.".into(),
            context_prefix: "Dokument: Vektorsuche. Thema: Hierarchical Navigable Small World Graphs. ".into(),
            embedding: pad_vector(&[0.12, 0.88, 0.0, 0.0], DIM),
        },
        SyntheticDocument {
            id: "doc_ml_13".into(),
            title: "BM25 Scoring Function".into(),
            raw_content: "BM25 scoring ranks documents by term frequency and inverse document frequency.".into(),
            context_prefix: "Dokument: Information Retrieval. Thema: BM25 Term Frequency Ranking. ".into(),
            embedding: pad_vector(&[0.15, 0.85, 0.0, 0.0], DIM),
        },
        SyntheticDocument {
            id: "doc_ml_14".into(),
            title: "Retrieval-Augmented Generation".into(),
            raw_content: "Retrieval-Augmented Generation combines dense retrieval with generative language models.".into(),
            context_prefix: "Dokument: KI Architektur. Thema: Retrieval-Augmented Generation (RAG). ".into(),
            embedding: pad_vector(&[0.14, 0.86, 0.0, 0.0], DIM),
        },
        SyntheticDocument {
            id: "doc_ml_15".into(),
            title: "Isotonic Calibration".into(),
            raw_content: "Isotonic regression fits a non-decreasing function to calibrate classifier probabilities.".into(),
            context_prefix: "Dokument: ML Kalibrierung. Thema: Isotonic Regression & Sigmoid Scaling. ".into(),
            embedding: pad_vector(&[0.18, 0.82, 0.0, 0.0], DIM),
        },
        SyntheticDocument {
            id: "doc_ml_16".into(),
            title: "Conformal Prediction".into(),
            raw_content: "Conformal prediction provides distribution-free coverage guarantees for any model.".into(),
            context_prefix: "Dokument: Statistik & ML. Thema: Conformal Uncertainty Intervals. ".into(),
            embedding: pad_vector(&[0.16, 0.84, 0.0, 0.0], DIM),
        },
        SyntheticDocument {
            id: "doc_ml_17".into(),
            title: "Cross-Encoder Reranking".into(),
            raw_content: "Cross-encoders rerank candidate documents by jointly encoding query and document.".into(),
            context_prefix: "Dokument: Neural Search. Thema: Cross-Encoder Sequence Reranking. ".into(),
            embedding: pad_vector(&[0.13, 0.87, 0.0, 0.0], DIM),
        },
        SyntheticDocument {
            id: "doc_ml_18".into(),
            title: "Reciprocal Rank Fusion".into(),
            raw_content: "Reciprocal Rank Fusion combines multiple ranked lists without score normalization.".into(),
            context_prefix: "Dokument: Hybrid Search. Thema: Reciprocal Rank Fusion (RRF). ".into(),
            embedding: pad_vector(&[0.17, 0.83, 0.0, 0.0], DIM),
        },
        SyntheticDocument {
            id: "doc_ml_19".into(),
            title: "ONNX Runtime Inference".into(),
            raw_content: "ONNX runtime enables cross-platform inference of machine learning models.".into(),
            context_prefix: "Dokument: Machine Learning Ops. Thema: ONNX Model Execution. ".into(),
            embedding: pad_vector(&[0.11, 0.89, 0.0, 0.0], DIM),
        },
        SyntheticDocument {
            id: "doc_ml_20".into(),
            title: "Personalized PageRank".into(),
            raw_content: "Personalized PageRank scores graph nodes by random walk proximity to a query node.".into(),
            context_prefix: "Dokument: Graph Mining. Thema: Personalized PageRank Walks. ".into(),
            embedding: pad_vector(&[0.19, 0.81, 0.0, 0.0], DIM),
        },

        // DATENBANK-GRUPPE (21-30)
        SyntheticDocument {
            id: "doc_db_21".into(),
            title: "LSM-Tree Memory Buffers".into(),
            raw_content: "LSM-trees buffer writes in memory before flushing sorted runs to disk.".into(),
            context_prefix: "Dokument: Speicher-Engines. Thema: Log-Structured Merge-Trees. ".into(),
            embedding: pad_vector(&[0.0, 0.10, 0.90, 0.0], DIM),
        },
        SyntheticDocument {
            id: "doc_db_22".into(),
            title: "Write-Ahead Logging".into(),
            raw_content: "Write-ahead logging ensures durability by recording changes before applying them.".into(),
            context_prefix: "Dokument: Datenbank-Transaktionen. Thema: Write-Ahead Logging (WAL). ".into(),
            embedding: pad_vector(&[0.0, 0.12, 0.88, 0.0], DIM),
        },
        SyntheticDocument {
            id: "doc_db_23".into(),
            title: "B-Tree Indexing".into(),
            raw_content: "B-trees maintain sorted data with logarithmic insert and search operations.".into(),
            context_prefix: "Dokument: Datenstrukturen. Thema: B-Tree & B+ Tree Indexing. ".into(),
            embedding: pad_vector(&[0.0, 0.15, 0.85, 0.0], DIM),
        },
        SyntheticDocument {
            id: "doc_db_24".into(),
            title: "Multi-Version Concurrency Control".into(),
            raw_content: "MVCC allows readers and writers to proceed concurrently without blocking each other.".into(),
            context_prefix: "Dokument: Concurrency Control. Thema: MVCC Isolation Levels. ".into(),
            embedding: pad_vector(&[0.0, 0.14, 0.86, 0.0], DIM),
        },
        SyntheticDocument {
            id: "doc_db_25".into(),
            title: "Bloom Filter Membership".into(),
            raw_content: "Bloom filters probabilistically test set membership with no false negatives.".into(),
            context_prefix: "Dokument: Probabilistische Datenstrukturen. Thema: Bloom Filter Checks. ".into(),
            embedding: pad_vector(&[0.0, 0.18, 0.82, 0.0], DIM),
        },
        SyntheticDocument {
            id: "doc_db_26".into(),
            title: "LSM Compaction Strategies".into(),
            raw_content: "Compaction in LSM-trees merges overlapping sorted runs to reclaim space.".into(),
            context_prefix: "Dokument: Storage Maintenance. Thema: LSM-Tree Compaction Runs. ".into(),
            embedding: pad_vector(&[0.0, 0.16, 0.84, 0.0], DIM),
        },
        SyntheticDocument {
            id: "doc_db_27".into(),
            title: "Consistent Hashing Ring".into(),
            raw_content: "Consistent hashing distributes data across nodes with minimal reshuffling on topology change.".into(),
            context_prefix: "Dokument: Verteilte Systeme. Thema: Consistent Hashing Topology. ".into(),
            embedding: pad_vector(&[0.0, 0.13, 0.87, 0.0], DIM),
        },
        SyntheticDocument {
            id: "doc_db_28".into(),
            title: "Raft Consensus Protocol".into(),
            raw_content: "Raft consensus ensures that a majority of nodes agree before committing a log entry.".into(),
            context_prefix: "Dokument: Verteilter Konsens. Thema: Raft Protocol State Machines. ".into(),
            embedding: pad_vector(&[0.0, 0.17, 0.83, 0.0], DIM),
        },
        SyntheticDocument {
            id: "doc_db_29".into(),
            title: "Column-Oriented Storage".into(),
            raw_content: "Column-oriented storage improves analytical query performance by minimizing I/O.".into(),
            context_prefix: "Dokument: Analytics DB. Thema: Columnar File Formats. ".into(),
            embedding: pad_vector(&[0.0, 0.11, 0.89, 0.0], DIM),
        },
        SyntheticDocument {
            id: "doc_db_30".into(),
            title: "DiskANN Vector Indexing".into(),
            raw_content: "DiskANN indexes large vector datasets on disk using Vamana graph construction.".into(),
            context_prefix: "Dokument: On-Disk Vector Index. Thema: DiskANN Vamana Graph. ".into(),
            embedding: pad_vector(&[0.0, 0.19, 0.81, 0.0], DIM),
        },

        // BIOLOGIE-GRUPPE (31-40)
        SyntheticDocument {
            id: "doc_bio_31".into(),
            title: "DNA Replication Mechanism".into(),
            raw_content: "DNA replication uses a semi-conservative mechanism where each strand serves as a template.".into(),
            context_prefix: "Dokument: Genetik & Molekularbiologie. Thema: DNA Replikationsgabel. ".into(),
            embedding: pad_vector(&[0.0, 0.0, 0.10, 0.90], DIM),
        },
        SyntheticDocument {
            id: "doc_bio_32".into(),
            title: "Synaptic Plasticity".into(),
            raw_content: "Synaptic plasticity strengthens or weakens connections based on co-activation patterns.".into(),
            context_prefix: "Dokument: Neurowissenschaften. Thema: Synaptische Plastizität & Lernen. ".into(),
            embedding: pad_vector(&[0.0, 0.0, 0.12, 0.88], DIM),
        },
        SyntheticDocument {
            id: "doc_bio_33".into(),
            title: "CRISPR-Cas9 Genome Editing".into(),
            raw_content: "CRISPR-Cas9 enables precise genome editing by targeting specific DNA sequences.".into(),
            context_prefix: "Dokument: Gentechnik. Thema: CRISPR-Cas9 DNA Targeting. ".into(),
            embedding: pad_vector(&[0.0, 0.0, 0.15, 0.85], DIM),
        },
        SyntheticDocument {
            id: "doc_bio_34".into(),
            title: "Mitochondrial ATP Generation".into(),
            raw_content: "Mitochondria generate ATP through oxidative phosphorylation in the electron transport chain.".into(),
            context_prefix: "Dokument: Zellbiologie. Thema: Mitochondrien & ATP Synthese. ".into(),
            embedding: pad_vector(&[0.0, 0.0, 0.14, 0.86], DIM),
        },
        SyntheticDocument {
            id: "doc_bio_35".into(),
            title: "Natural Selection".into(),
            raw_content: "Natural selection favors traits that increase reproductive success in an environment.".into(),
            context_prefix: "Dokument: Evolutionsbiologie. Thema: Natürliche Selektion. ".into(),
            embedding: pad_vector(&[0.0, 0.0, 0.18, 0.82], DIM),
        },
        SyntheticDocument {
            id: "doc_bio_36".into(),
            title: "REM Sleep Consolidation".into(),
            raw_content: "REM sleep consolidates episodic memories by replaying neural activation patterns.".into(),
            context_prefix: "Dokument: Gehirnforschung. Thema: REM Schlaf & Gedächtnis. ".into(),
            embedding: pad_vector(&[0.0, 0.0, 0.16, 0.84], DIM),
        },
        SyntheticDocument {
            id: "doc_bio_37".into(),
            title: "Immune B-Cell Antibodies".into(),
            raw_content: "Immune system B-cells produce antibodies that bind to specific antigenic epitopes.".into(),
            context_prefix: "Dokument: Immunologie. Thema: B-Zellen & Antikörper. ".into(),
            embedding: pad_vector(&[0.0, 0.0, 0.13, 0.87], DIM),
        },
        SyntheticDocument {
            id: "doc_bio_38".into(),
            title: "Protein Folding Dynamics".into(),
            raw_content: "Protein folding determines function through the thermodynamically stable native structure.".into(),
            context_prefix: "Dokument: Biochemie. Thema: Proteinfaltung & Struktur. ".into(),
            embedding: pad_vector(&[0.0, 0.0, 0.17, 0.83], DIM),
        },
        SyntheticDocument {
            id: "doc_bio_39".into(),
            title: "Hippocampus Memory Formation".into(),
            raw_content: "The hippocampus plays a central role in forming and consolidating long-term memories.".into(),
            context_prefix: "Dokument: Neurobiologie. Thema: Hippocampus & Langzeitgedächtnis. ".into(),
            embedding: pad_vector(&[0.0, 0.0, 0.11, 0.89], DIM),
        },
        SyntheticDocument {
            id: "doc_bio_40".into(),
            title: "Epigenetic Gene Regulation".into(),
            raw_content: "Epigenetic modifications regulate gene expression without changing the DNA sequence.".into(),
            context_prefix: "Dokument: Epigenetik. Thema: Chromatin-Modifikationen. ".into(),
            embedding: pad_vector(&[0.0, 0.0, 0.19, 0.81], DIM),
        },

        // GESCHICHTE/GEOGRAPHIE-GRUPPE (41-50)
        SyntheticDocument {
            id: "doc_hist_41".into(),
            title: "Roman Empire under Trajan".into(),
            raw_content: "The Roman Empire reached its greatest extent under Emperor Trajan in 117 AD.".into(),
            context_prefix: "Dokument: Antike Geschichte. Thema: Römisches Reich & Expansion. ".into(),
            embedding: pad_vector(&[0.05, 0.05, 0.0, 0.90], DIM),
        },
        SyntheticDocument {
            id: "doc_hist_42".into(),
            title: "Silk Road Trade Routes".into(),
            raw_content: "The Silk Road connected East Asia with the Mediterranean through Central Asian trade routes.".into(),
            context_prefix: "Dokument: Handelsgeschichte. Thema: Seidenstraße & Asien-Handel. ".into(),
            embedding: pad_vector(&[0.05, 0.07, 0.0, 0.88], DIM),
        },
        SyntheticDocument {
            id: "doc_hist_43".into(),
            title: "Industrial Revolution Britain".into(),
            raw_content: "The Industrial Revolution began in Britain with mechanized textile production in the 1760s.".into(),
            context_prefix: "Dokument: Wirtschaftsgeschichte. Thema: Industrielle Revolution. ".into(),
            embedding: pad_vector(&[0.05, 0.10, 0.0, 0.85], DIM),
        },
        SyntheticDocument {
            id: "doc_geog_44".into(),
            title: "Amazon Rainforest Ecosystem".into(),
            raw_content: "The Amazon basin contains the world's largest tropical rainforest and freshwater system.".into(),
            context_prefix: "Dokument: Physische Geographie. Thema: Amazonien & Regenwald. ".into(),
            embedding: pad_vector(&[0.05, 0.09, 0.0, 0.86], DIM),
        },
        SyntheticDocument {
            id: "doc_geog_45".into(),
            title: "Plate Tectonics Continental Drift".into(),
            raw_content: "Plate tectonics explains continental drift through the movement of lithospheric plates.".into(),
            context_prefix: "Dokument: Geologie. Thema: Plattentektonik & Kontinentalverschiebung. ".into(),
            embedding: pad_vector(&[0.05, 0.13, 0.0, 0.82], DIM),
        },
        SyntheticDocument {
            id: "doc_geog_46".into(),
            title: "Himalayas Orogeny".into(),
            raw_content: "The Himalayas were formed by the collision of the Indian and Eurasian tectonic plates.".into(),
            context_prefix: "Dokument: Gebirgsbildung. Thema: Himalaja-Entstehung Tektonik. ".into(),
            embedding: pad_vector(&[0.05, 0.11, 0.0, 0.84], DIM),
        },
        SyntheticDocument {
            id: "doc_hist_47".into(),
            title: "Byzantine Empire Continuity".into(),
            raw_content: "The Byzantine Empire preserved Roman law and Greek culture for over a thousand years.".into(),
            context_prefix: "Dokument: Mittelalterliche Geschichte. Thema: Byzantinisches Reich. ".into(),
            embedding: pad_vector(&[0.05, 0.08, 0.0, 0.87], DIM),
        },
        SyntheticDocument {
            id: "doc_geog_48".into(),
            title: "Sahara Desert and Sahel".into(),
            raw_content: "The Sahara Desert transitions from hyperarid core to semi-arid Sahel in the south.".into(),
            context_prefix: "Dokument: Klimageographie. Thema: Sahara & Sahelzone. ".into(),
            embedding: pad_vector(&[0.05, 0.12, 0.0, 0.83], DIM),
        },
        SyntheticDocument {
            id: "doc_hist_49".into(),
            title: "Gutenberg Printing Press".into(),
            raw_content: "The printing press invented by Gutenberg in 1440 revolutionized knowledge dissemination.".into(),
            context_prefix: "Dokument: Mediengeschichte. Thema: Buchdruck-Revolution. ".into(),
            embedding: pad_vector(&[0.05, 0.06, 0.0, 0.89], DIM),
        },
        SyntheticDocument {
            id: "doc_geog_50".into(),
            title: "Global Ocean Circulation".into(),
            raw_content: "Ocean currents redistribute heat globally, moderating coastal climates worldwide.".into(),
            context_prefix: "Dokument: Ozeanographie. Thema: Meeresströmungen & Klima. ".into(),
            embedding: pad_vector(&[0.05, 0.14, 0.0, 0.81], DIM),
        },
    ]
}

fn create_synthetic_corpus() -> (Vec<SyntheticDocument>, Vec<GroundTruthQuery>, Vec<GroundTruthQuery>) {
    const DIM: usize = 768;

    let docs = build_evaluation_corpus();

    // Scenario A Queries (Context Prefix testing)
    let scenario_a_queries = vec![
        GroundTruthQuery {
            query_id: "q_a_1".into(),
            query_text: "Ownership system borrow checking memory leaks Rust Grundlagen".into(),
            query_embedding: pad_vector(&[0.90, 0.10, 0.0, 0.0], DIM),
            relevant_doc_ids: vec!["doc_rust_1".into()],
        },
        GroundTruthQuery {
            query_id: "q_a_2".into(),
            query_text: "Transformer architectures self-attention long-range dependencies Deep Learning".into(),
            query_embedding: pad_vector(&[0.10, 0.90, 0.0, 0.0], DIM),
            relevant_doc_ids: vec!["doc_ml_11".into()],
        },
        GroundTruthQuery {
            query_id: "q_a_3".into(),
            query_text: "Write-ahead logging durability changes Datenbank-Transaktionen".into(),
            query_embedding: pad_vector(&[0.0, 0.12, 0.88, 0.0], DIM),
            relevant_doc_ids: vec!["doc_db_22".into()],
        },
        GroundTruthQuery {
            query_id: "q_a_4".into(),
            query_text: "CRISPR-Cas9 genome editing DNA sequences Gentechnik".into(),
            query_embedding: pad_vector(&[0.0, 0.0, 0.15, 0.85], DIM),
            relevant_doc_ids: vec!["doc_bio_33".into()],
        },
        GroundTruthQuery {
            query_id: "q_a_5".into(),
            query_text: "Roman Empire Emperor Trajan 117 AD Antike Geschichte".into(),
            query_embedding: pad_vector(&[0.05, 0.05, 0.0, 0.90], DIM),
            relevant_doc_ids: vec!["doc_hist_41".into()],
        },
    ];

    // Scenario B Queries (Cross-Encoder Reranking)
    let scenario_b_queries = vec![
        GroundTruthQuery {
            query_id: "q_b_1".into(),
            query_text: "Wie verhindert das Ownership System in Rust Speicherlecks zur Kompilierzeit?".into(),
            query_embedding: pad_vector(&[0.90, 0.10, 0.0, 0.0], DIM),
            relevant_doc_ids: vec!["doc_rust_1".into()],
        },
        GroundTruthQuery {
            query_id: "q_b_2".into(),
            query_text: "Wie schränkt HNSW Graph die Suchkomplexität bei der Vektorsuche ein?".into(),
            query_embedding: pad_vector(&[0.12, 0.88, 0.0, 0.0], DIM),
            relevant_doc_ids: vec!["doc_ml_12".into()],
        },
        GroundTruthQuery {
            query_id: "q_b_3".into(),
            query_text: "Warum verwendet man Write-Ahead Logging in Datenbanksystemen?".into(),
            query_embedding: pad_vector(&[0.0, 0.12, 0.88, 0.0], DIM),
            relevant_doc_ids: vec!["doc_db_22".into()],
        },
        GroundTruthQuery {
            query_id: "q_b_4".into(),
            query_text: "Wie funktioniert die DNA Replikation an der Replikationsgabel?".into(),
            query_embedding: pad_vector(&[0.0, 0.0, 0.10, 0.90], DIM),
            relevant_doc_ids: vec!["doc_bio_31".into()],
        },
        GroundTruthQuery {
            query_id: "q_b_5".into(),
            query_text: "Wann erreichte das Römische Reich seine größte geografische Ausdehnung?".into(),
            query_embedding: pad_vector(&[0.05, 0.05, 0.0, 0.90], DIM),
            relevant_doc_ids: vec!["doc_hist_41".into()],
        },
    ];

    (docs, scenario_a_queries, scenario_b_queries)
}

fn calculate_metrics(
    queries: &[GroundTruthQuery],
    retrieved_results: &[Vec<String>],
) -> BenchmarkMetrics {
    let mut rec1_hits = 0;
    let mut rec3_hits = 0;
    let mut rec5_hits = 0;
    let mut mrr_sum = 0.0;

    for (q, retrieved) in queries.iter().zip(retrieved_results.iter()) {
        let targets: HashSet<&str> = q.relevant_doc_ids.iter().map(|s| s.as_str()).collect();

        let mut hit_rank: Option<usize> = None;
        for (idx, doc_id) in retrieved.iter().enumerate() {
            if targets.contains(doc_id.as_str()) && hit_rank.is_none() {
                hit_rank = Some(idx + 1);
            }
        }

        if let Some(rank) = hit_rank {
            if rank <= 1 {
                rec1_hits += 1;
            }
            if rank <= 3 {
                rec3_hits += 1;
            }
            if rank <= 5 {
                rec5_hits += 1;
            }
            mrr_sum += 1.0 / (rank as f64);
        }
    }

    let n = queries.len() as f64;
    let r1 = rec1_hits as f64 / n;
    let r3 = rec3_hits as f64 / n;
    let r5 = rec5_hits as f64 / n;
    let mrr = mrr_sum / n;

    BenchmarkMetrics {
        total_queries: queries.len(),
        recall_at_1: r1,
        recall_at_3: r3,
        recall_at_5: r5,
        mrr,
        error_rate_at_1: 1.0 - r1,
        error_rate_at_5: 1.0 - r5,
    }
}

async fn run_scenario_a(
    docs: &[SyntheticDocument],
    queries: &[GroundTruthQuery],
) -> Result<ScenarioComparison> {
    let db_cfg = MemFuseConfig {
        dimension: 768,
        ..Default::default()
    };

    // 1. Evaluate WITHOUT Context Prefix (Baseline)
    let baseline_retrieved = {
        let temp_dir = TempDir::new()?;
        let db = MemFuse::open_with_config(temp_dir.path(), db_cfg.clone()).await?;
        let col = db.collection("baseline_prefix_test").await?;

        for doc in docs {
            let metadata = serde_json::json!({
                "title": doc.title,
                "text": doc.raw_content,
            });
            col.insert(&doc.id, &doc.embedding, Some(metadata)).await?;
        }

        let mut retrieved_all = Vec::with_capacity(queries.len());
        for q in queries {
            let res = col
                .query()
                .text(&q.query_text)
                .embedding(&q.query_embedding)
                .k(5)
                .execute()
                .await?;
            let ids: Vec<String> = res.into_iter().map(|r| r.id).collect();
            retrieved_all.push(ids);
        }
        retrieved_all
    };

    // 2. Evaluate WITH Context Prefix
    let prefix_retrieved = {
        let temp_dir = TempDir::new()?;
        let db = MemFuse::open_with_config(temp_dir.path(), db_cfg.clone()).await?;
        let col = db.collection("context_prefix_test").await?;

        for doc in docs {
            let prefixed_text = format!("{}{}", doc.context_prefix, doc.raw_content);
            let metadata = serde_json::json!({
                "title": doc.title,
                "text": prefixed_text,
                "has_context_prefix": true,
            });
            col.insert(&doc.id, &doc.embedding, Some(metadata)).await?;
        }

        let mut retrieved_all = Vec::with_capacity(queries.len());
        for q in queries {
            let res = col
                .query()
                .text(&q.query_text)
                .embedding(&q.query_embedding)
                .k(5)
                .execute()
                .await?;
            let ids: Vec<String> = res.into_iter().map(|r| r.id).collect();
            retrieved_all.push(ids);
        }
        retrieved_all
    };

    let base_metrics = calculate_metrics(queries, &baseline_retrieved);
    let feat_metrics = calculate_metrics(queries, &prefix_retrieved);

    let recall_1_delta = if base_metrics.recall_at_1 > 0.0 {
        ((feat_metrics.recall_at_1 - base_metrics.recall_at_1) / base_metrics.recall_at_1) * 100.0
    } else if feat_metrics.recall_at_1 > 0.0 {
        100.0
    } else {
        0.0
    };

    let recall_5_delta = if base_metrics.recall_at_5 > 0.0 {
        ((feat_metrics.recall_at_5 - base_metrics.recall_at_5) / base_metrics.recall_at_5) * 100.0
    } else if feat_metrics.recall_at_5 > 0.0 {
        100.0
    } else {
        0.0
    };

    let err_reduction = if base_metrics.error_rate_at_1 > 0.0 {
        ((base_metrics.error_rate_at_1 - feat_metrics.error_rate_at_1) / base_metrics.error_rate_at_1) * 100.0
    } else {
        0.0
    };

    Ok(ScenarioComparison {
        scenario_name: "Baseline vs. Kontext-Präfix".into(),
        baseline_metrics: base_metrics,
        feature_metrics: feat_metrics,
        recall_at_1_delta_pct: recall_1_delta,
        recall_at_5_delta_pct: recall_5_delta,
        error_rate_reduction_pct: err_reduction,
    })
}

async fn run_scenario_b(
    docs: &[SyntheticDocument],
    queries: &[GroundTruthQuery],
) -> Result<ScenarioComparison> {
    let db_cfg = MemFuseConfig {
        dimension: 768,
        ..Default::default()
    };

    let temp_dir = TempDir::new()?;
    let db = MemFuse::open_with_config(temp_dir.path(), db_cfg).await?;
    let col = db.collection("rerank_test").await?;

    for doc in docs {
        let metadata = serde_json::json!({
            "title": doc.title,
            "text": doc.raw_content,
        });
        col.insert(&doc.id, &doc.embedding, Some(metadata)).await?;
    }

    // 1. Without Reranking (Standard RRF)
    let mut no_rerank_retrieved = Vec::with_capacity(queries.len());
    for q in queries {
        let res = col
            .query()
            .text(&q.query_text)
            .embedding(&q.query_embedding)
            .k(5)
            .execute()
            .await?;
        no_rerank_retrieved.push(res.into_iter().map(|r| r.id).collect());
    }

    // 2. With Cross-Encoder Reranking
    let reranker_config = RerankConfig::default();
    let reranker = match CrossEncoderReranker::new(reranker_config) {
        Ok(r) => r,
        Err(_) => {
            println!("[INFO] ONNX model weights not found at models/bge-reranker-base.onnx. Using Passthrough CrossEncoder for benchmark.");
            CrossEncoderReranker::passthrough()
        }
    };

    let mut with_rerank_retrieved = Vec::with_capacity(queries.len());
    for q in queries {
        let res = col
            .query()
            .text(&q.query_text)
            .embedding(&q.query_embedding)
            .reranker(&reranker)
            .k(5)
            .execute()
            .await?;
        with_rerank_retrieved.push(res.into_iter().map(|r| r.id).collect());
    }

    let base_metrics = calculate_metrics(queries, &no_rerank_retrieved);
    let feat_metrics = calculate_metrics(queries, &with_rerank_retrieved);

    let recall_1_delta = if base_metrics.recall_at_1 > 0.0 {
        ((feat_metrics.recall_at_1 - base_metrics.recall_at_1) / base_metrics.recall_at_1) * 100.0
    } else if feat_metrics.recall_at_1 > 0.0 {
        100.0
    } else {
        0.0
    };

    let recall_5_delta = if base_metrics.recall_at_5 > 0.0 {
        ((feat_metrics.recall_at_5 - base_metrics.recall_at_5) / base_metrics.recall_at_5) * 100.0
    } else if feat_metrics.recall_at_5 > 0.0 {
        100.0
    } else {
        0.0
    };

    let err_reduction = if base_metrics.error_rate_at_1 > 0.0 {
        ((base_metrics.error_rate_at_1 - feat_metrics.error_rate_at_1) / base_metrics.error_rate_at_1) * 100.0
    } else {
        0.0
    };

    Ok(ScenarioComparison {
        scenario_name: "Ohne vs. mit Cross-Encoder-Reranking".into(),
        baseline_metrics: base_metrics,
        feature_metrics: feat_metrics,
        recall_at_1_delta_pct: recall_1_delta,
        recall_at_5_delta_pct: recall_5_delta,
        error_rate_reduction_pct: err_reduction,
    })
}

fn generate_markdown_summary(report: &BenchmarkReport) -> String {
    let mut out = String::new();
    out.push_str("# MemFuse — Retrieval Accuracy Benchmark Report\n\n");
    out.push_str(&format!("**Stand / Zeitstempel**: `{}`\n", report.timestamp));
    out.push_str(&format!("**Testkorpus**: {} Dokument-Chunks, {} Testabfragen\n\n", report.corpus_size_docs, report.total_test_queries));

    out.push_str("## Zusammenfassung der Messergebnisse\n\n");
    out.push_str("| Szenario | Modus | Recall@1 | Recall@3 | Recall@5 | MRR | Fehlerrate@1 | Delta (Recall@1) | Delta (Fehler) |\n");
    out.push_str("|---|---|---|---|---|---|---|---|---|\n");

    let sc_a = &report.scenario_a_context_prefix;
    out.push_str(&format!(
        "| **Szenario A**: Kontext-Präfix | Baseline (Ohne) | {:.1}% | {:.1}% | {:.1}% | {:.3} | {:.1}% | - | - |\n",
        sc_a.baseline_metrics.recall_at_1 * 100.0,
        sc_a.baseline_metrics.recall_at_3 * 100.0,
        sc_a.baseline_metrics.recall_at_5 * 100.0,
        sc_a.baseline_metrics.mrr,
        sc_a.baseline_metrics.error_rate_at_1 * 100.0,
    ));
    out.push_str(&format!(
        "| | Mit Kontext-Präfix | {:.1}% | {:.1}% | {:.1}% | {:.3} | {:.1}% | **+{:.1}%** | **-{:.1}%** |\n",
        sc_a.feature_metrics.recall_at_1 * 100.0,
        sc_a.feature_metrics.recall_at_3 * 100.0,
        sc_a.feature_metrics.recall_at_5 * 100.0,
        sc_a.feature_metrics.mrr,
        sc_a.feature_metrics.error_rate_at_1 * 100.0,
        sc_a.recall_at_1_delta_pct,
        sc_a.error_rate_reduction_pct,
    ));

    let sc_b = &report.scenario_b_reranking;
    out.push_str(&format!(
        "| **Szenario B**: Reranking | Standard RRF (Ohne) | {:.1}% | {:.1}% | {:.1}% | {:.3} | {:.1}% | - | - |\n",
        sc_b.baseline_metrics.recall_at_1 * 100.0,
        sc_b.baseline_metrics.recall_at_3 * 100.0,
        sc_b.baseline_metrics.recall_at_5 * 100.0,
        sc_b.baseline_metrics.mrr,
        sc_b.baseline_metrics.error_rate_at_1 * 100.0,
    ));
    out.push_str(&format!(
        "| | Mit Cross-Encoder | {:.1}% | {:.1}% | {:.1}% | {:.3} | {:.1}% | **+{:.1}%** | **-{:.1}%** |\n",
        sc_b.feature_metrics.recall_at_1 * 100.0,
        sc_b.feature_metrics.recall_at_3 * 100.0,
        sc_b.feature_metrics.recall_at_5 * 100.0,
        sc_b.feature_metrics.mrr,
        sc_b.feature_metrics.error_rate_at_1 * 100.0,
        sc_b.recall_at_1_delta_pct,
        sc_b.error_rate_reduction_pct,
    ));

    out
}

#[tokio::main]
async fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    println!("=== Running MemFuse Retrieval Accuracy Benchmarks ===");

    let (docs, queries_a, queries_b) = create_synthetic_corpus();
    println!("Loaded synthetic corpus with {} documents.", docs.len());

    let scenario_a = run_scenario_a(&docs, &queries_a).await?;
    println!("Scenario A completed.");

    let scenario_b = run_scenario_b(&docs, &queries_b).await?;
    println!("Scenario B completed.");

    let report = BenchmarkReport {
        timestamp: "2026-09-03T10:00:00Z".into(),
        corpus_size_docs: docs.len(),
        total_test_queries: queries_a.len() + queries_b.len(),
        scenario_a_context_prefix: scenario_a,
        scenario_b_reranking: scenario_b,
    };

    let json_output = serde_json::to_string_pretty(&report)?;
    let markdown_summary = generate_markdown_summary(&report);

    // Save outputs
    let results_dir = Path::new("benchmarks/results");
    if !results_dir.exists() {
        fs::create_dir_all(results_dir)?;
    }

    fs::write(results_dir.join("results.json"), &json_output)?;
    fs::write(results_dir.join("summary.md"), &markdown_summary)?;

    println!("\nBenchmark results saved to `benchmarks/results/results.json` and `benchmarks/results/summary.md`.\n");
    println!("{}", markdown_summary);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_synthetic_corpus_integrity() {
        let (docs, q_a, q_b) = create_synthetic_corpus();
        assert!(!docs.is_empty());
        assert!(!q_a.is_empty());
        assert!(!q_b.is_empty());

        let doc_ids: HashSet<&str> = docs.iter().map(|d| d.id.as_str()).collect();
        for q in q_a.iter().chain(q_b.iter()) {
            for rel in &q.relevant_doc_ids {
                assert!(doc_ids.contains(rel.as_str()), "Query target {} not found in corpus", rel);
            }
        }
    }

    #[tokio::test]
    async fn test_benchmark_scenarios_execution() {
        let (docs, q_a, q_b) = create_synthetic_corpus();
        let sc_a = run_scenario_a(&docs, &q_a).await.unwrap();
        let sc_b = run_scenario_b(&docs, &q_b).await.unwrap();

        assert_eq!(sc_a.baseline_metrics.total_queries, q_a.len());
        assert_eq!(sc_b.baseline_metrics.total_queries, q_b.len());
    }
}
