// FILE-CONTEXT:
// ZWECK: PyO3 FFI bindings bridging MemFuse embedded vector DB functionality to Python.
// INVARIANTEN: Zero Rust panics cross FFI boundary; GIL released during block_on async calls.
// NICHT-OFFENSICHTLICH: Uses OnceLock multi-thread Tokio runtime shared across Python worker threads.
// HOTSPOTS: [160-205] memfuse_err mapping, [270-650] CRUD & search methods FFI boundary validation.
// STAND: TS:2026-08-30T18:52:02Z (SESSION: 846802ab)

//! # MemFuse Python Bindings
//!
//! This crate provides the Python bridge for the MemFuse embedded hybrid-search database.
//! It utilizes PyO3 for the bridge and NumPy for efficient vector operations.
//!
//! ## Architecture Role
//!
//! - **Python Bridge (Layer 3)**: Exposes the core functionality of MemFuse to Python.
//! - **Async Orchestration**: Manages a shared Tokio runtime for executing async Rust code
//!   from synchronous Python calls.
//! - **Zero-Copy**: Aims for minimal copying of vector data between Python and Rust.

#![forbid(unsafe_code)]

use memfuse_db::{Collection as MemFuseCollection, MemFuse, MemFuseConfig};
use numpy::PyReadonlyArray1;
use pyo3::exceptions::{PyKeyError, PyPermissionError, PyRuntimeError};
use pyo3::prelude::*;
use pyo3::types::PyBytes;
use pythonize::{depythonize, pythonize};
use std::sync::Arc;
use std::sync::OnceLock;
use tokio::runtime::Runtime;

// ─── Custom Exceptions ──────────────────────────────────────────────────────

pyo3::create_exception!(_memfuse, MemFuseError, pyo3::exceptions::PyException);
pyo3::create_exception!(_memfuse, MemFuseIOError, MemFuseError);
pyo3::create_exception!(_memfuse, MemFuseIndexError, MemFuseError);
pyo3::create_exception!(_memfuse, MemFuseValueError, MemFuseError);
pyo3::create_exception!(_memfuse, MemFuseCryptoError, MemFuseError);
pyo3::create_exception!(_memfuse, MemFuseInternalError, MemFuseError);

// ─── Shared Tokio Runtime ───────────────────────────────────────────────────

/// Returns a reference to the shared Tokio runtime.
///
/// In compliance with the Zero-Panic policy, this function handles potential
/// runtime creation errors by returning a `PyResult`.
fn get_runtime() -> PyResult<&'static Runtime> {
    static RUNTIME: OnceLock<Runtime> = OnceLock::new();
    if let Some(rt) = RUNTIME.get() {
        return Ok(rt);
    }

    let worker_threads = std::env::var("MEMFUSE_WORKER_THREADS")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or_else(|| {
            (std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(4)
                / 2)
            .max(2)
        });

    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(worker_threads)
        .thread_name("memfuse-py-worker")
        .enable_all()
        .build()
        .map_err(|e| {
            pyo3::exceptions::PyRuntimeError::new_err(format!(
                "Failed to create tokio runtime for memfuse-py: {}",
                e
            ))
        })?;

    if let Err(_rt_existing) = RUNTIME.set(rt) {
        // Another thread already initialized it, just return the existing one.
    }

    RUNTIME.get().ok_or_else(|| {
        pyo3::exceptions::PyRuntimeError::new_err("Failed to retrieve initialized tokio runtime")
    })
}

// ─── Shared Helper Functions ────────────────────────────────────────────────

/// Converts a Python dict to a serde_json::Value.
fn dict_to_json(d: &pyo3::Bound<'_, pyo3::types::PyDict>) -> PyResult<serde_json::Value> {
    depythonize(d)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(format!("Metadata error: {}", e)))
}

/// Converts an optional Python dict to an optional serde_json::Value.
fn opt_dict_to_json(
    metadata: Option<&pyo3::Bound<'_, pyo3::types::PyDict>>,
) -> PyResult<Option<serde_json::Value>> {
    match metadata {
        Some(d) => Ok(Some(dict_to_json(d)?)),
        None => Ok(None),
    }
}

/// Maximum allowed length for document string IDs (1024 characters).
const MAX_ID_LENGTH: usize = 1024;
/// Maximum allowed length for relationship labels (256 characters).
const MAX_LABEL_LENGTH: usize = 256;
/// Maximum batch size for batch insertion/upsertion (10,000 items).
const MAX_BATCH_SIZE: usize = 10_000;

/// Validates that a string ID is non-empty and does not exceed maximum length.
fn validate_id(id: &str) -> PyResult<()> {
    if id.is_empty() {
        return Err(MemFuseValueError::new_err("Document ID cannot be empty"));
    }
    if id.len() > MAX_ID_LENGTH {
        return Err(MemFuseValueError::new_err(format!(
            "Document ID exceeds maximum length of {} bytes. Got: {}",
            MAX_ID_LENGTH,
            id.len()
        )));
    }
    Ok(())
}

/// Validates that a vector slice contains no NaN or infinite values.
fn validate_vector(vector: &[f32]) -> PyResult<()> {
    if vector.iter().any(|x| x.is_nan() || x.is_infinite()) {
        return Err(MemFuseValueError::new_err(
            "Vector contains NaN or infinite float values",
        ));
    }
    Ok(())
}

/// Validates both document ID and vector slice.
fn validate_id_and_vector(id: &str, vector: &[f32]) -> PyResult<()> {
    validate_id(id)?;
    validate_vector(vector)?;
    Ok(())
}

/// Converts a serde_json::Value to a Python object.
fn json_to_py(py: Python<'_>, val: &serde_json::Value) -> PyResult<PyObject> {
    pythonize(py, val)
        .map(|o| o.unbind())
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("Metadata error: {}", e)))
}

/// Converts a memfuse_db::Document to a PyDocument.
fn doc_to_py(py: Python<'_>, d: memfuse_db::Document) -> PyResult<PyDocument> {
    let meta_py = match d.metadata {
        Some(ref m) => Some(json_to_py(py, m)?),
        None => None,
    };
    Ok(PyDocument {
        id: d.id,
        metadata: meta_py,
    })
}

/// Converts a Vec of SearchResult to Vec of PySearchResult.
fn results_to_py(
    py: Python<'_>,
    results: Vec<memfuse_db::SearchResult>,
) -> PyResult<Vec<PySearchResult>> {
    let mut py_res = Vec::with_capacity(results.len());
    for r in results {
        let meta_py = match r.metadata {
            Some(ref m) => Some(json_to_py(py, m)?),
            None => None,
        };
        py_res.push(PySearchResult {
            id: r.id,
            score: r.score,
            metadata: meta_py,
        });
    }
    Ok(py_res)
}

/// Maps a MemFuseError into a structured Python PyErr with `kind`, `message`, and `details` attributes.
fn memfuse_err(e: memfuse_core::MemFuseError) -> PyErr {
    let dto = memfuse_core::MemFuseErrorDto::from(&e);
    Python::with_gil(|py| {
        let py_err = match dto.kind.as_str() {
            "NotFound" => PyKeyError::new_err(dto.message.clone()),
            "Conflict" | "Transaction" | "TransactionTimeout" => {
                PyRuntimeError::new_err(dto.message.clone())
            }
            "PolicyViolation"
            | "NamespaceViolation"
            | "Sandbox"
            | "MemoryLimitExceeded"
            | "SandboxTimeout" => PyPermissionError::new_err(dto.message.clone()),
            "InvalidInput"
            | "Serialization"
            | "Json"
            | "ParseError"
            | "Bincode"
            | "InvalidSequenceNumber"
            | "CheckpointNotFound" => MemFuseValueError::new_err(dto.message.clone()),
            "Storage" | "Io" | "WalCorruption" | "ChecksumMismatch" => {
                MemFuseIOError::new_err(dto.message.clone())
            }
            "Index" | "HnswConnectivityDegraded" | "Text" => {
                MemFuseIndexError::new_err(dto.message.clone())
            }
            "Crypto" => MemFuseCryptoError::new_err(dto.message.clone()),
            "MemoryBudgetExceeded" => pyo3::exceptions::PyMemoryError::new_err(dto.message.clone()),
            "CapabilityUnsupported" => {
                pyo3::exceptions::PyNotImplementedError::new_err(dto.message.clone())
            }
            "Internal" | "Cluster" => MemFuseInternalError::new_err(dto.message.clone()),
            _ => MemFuseError::new_err(dto.message.clone()),
        };
        let value = py_err.value(py);
        let _ = value.setattr("kind", dto.kind);
        let _ = value.setattr("message", dto.message);
        if let Some(ref details) = dto.details {
            if let Ok(details_py) = json_to_py(py, details) {
                let _ = value.setattr("details", details_py);
            }
        }
        py_err
    })
}

// ─── Python Types ───────────────────────────────────────────────────────────

/// A single search result from MemFuse.
#[pyclass(get_all)]
pub struct PySearchResult {
    /// The document ID.
    pub id: String,
    /// Similarity score (higher = more similar).
    pub score: f32,
    /// Metadata associated with the document.
    pub metadata: Option<PyObject>,
}

#[pymethods]
impl PySearchResult {
    fn __repr__(&self) -> String {
        format!("SearchResult(id='{}', score={:.4})", self.id, self.score)
    }
}

/// A document retrieved from MemFuse.
#[pyclass(get_all)]
pub struct PyDocument {
    /// The document ID.
    pub id: String,
    /// Metadata associated with the document.
    pub metadata: Option<PyObject>,
}

#[pymethods]
impl PyDocument {
    fn __repr__(&self) -> String {
        format!("Document(id='{}')", self.id)
    }
}

/// Statistics for a vector index.
#[pyclass(get_all, name = "VectorIndexStats")]
#[derive(Clone)]
pub struct PyVectorIndexStats {
    /// Number of active (non-deleted) vectors.
    pub num_vectors: usize,
    /// Estimated memory usage in bytes.
    pub memory_usage_bytes: usize,
    /// Number of HNSW layers.
    pub num_layers: usize,
}

#[pymethods]
impl PyVectorIndexStats {
    fn __repr__(&self) -> String {
        format!(
            "VectorIndexStats(num_vectors={}, memory_usage_bytes={}, num_layers={})",
            self.num_vectors, self.memory_usage_bytes, self.num_layers
        )
    }
}

/// Statistics for the storage engine.
#[pyclass(get_all, name = "StorageStats")]
#[derive(Clone)]
pub struct PyStorageStats {
    /// Number of SSTable segments.
    pub num_segments: usize,
    /// Total size of all SSTables in bytes.
    pub total_size_bytes: u64,
    /// Total size of memtables in bytes.
    pub memtable_size_bytes: u64,
}

#[pymethods]
impl PyStorageStats {
    fn __repr__(&self) -> String {
        format!(
            "StorageStats(num_segments={}, total_size_bytes={}, memtable_size_bytes={})",
            self.num_segments, self.total_size_bytes, self.memtable_size_bytes
        )
    }
}

/// Overall database statistics.
#[pyclass(get_all, name = "DbStats")]
#[derive(Clone)]
pub struct PyDbStats {
    /// Statistics for the vector index.
    pub index_stats: PyVectorIndexStats,
    /// Statistics for the LSM storage engine.
    pub storage_stats: PyStorageStats,
}

#[pymethods]
impl PyDbStats {
    fn __repr__(&self) -> String {
        format!(
            "DbStats(vectors={}, size_bytes={})",
            self.index_stats.num_vectors,
            self.storage_stats.total_size_bytes + self.storage_stats.memtable_size_bytes
        )
    }
}

// ─── Macro: Shared CRUD Methods ─────────────────────────────────────────────
//
// This macro generates the common CRUD, search, and scan methods that are
// shared between PyMemFuse (default collection facade) and PyCollection.
// It eliminates ~400 LoC of duplication.

macro_rules! memfuse_crud_methods {
    ($struct_type:ty) => {
        #[pymethods]
        impl $struct_type {
            /// Inserts a document with an embedding and optional metadata.
            #[pyo3(signature = (id, vector, metadata=None))]
            pub fn insert<'py>(
                &self,
                py: Python<'py>,
                id: &str,
                vector: PyReadonlyArray1<'py, f32>,
                metadata: Option<pyo3::Bound<'py, pyo3::types::PyDict>>,
            ) -> PyResult<()> {
                let rt = get_runtime()?;
                let v = vector.as_slice().map_err(|e| {
                    pyo3::exceptions::PyValueError::new_err(format!("Invalid vector: {}", e))
                })?;
                validate_id_and_vector(id, v)?;
                let m = opt_dict_to_json(metadata.as_ref())?;
                py.allow_threads(|| rt.block_on(self.inner.insert(id, v, m)))
                    .map_err(memfuse_err)
            }

            /// Retrieves a document by its user-provided string ID.
            pub fn get(&self, py: Python<'_>, id: &str) -> PyResult<Option<PyDocument>> {
                validate_id(id)?;
                let rt = get_runtime()?;
                let doc = py
                    .allow_threads(|| rt.block_on(self.inner.get(id)))
                    .map_err(memfuse_err)?;
                match doc {
                    Some(d) => Ok(Some(doc_to_py(py, d)?)),
                    None => Ok(None),
                }
            }

            /// Updates an existing document.
            #[pyo3(signature = (id, vector, metadata=None))]
            pub fn update<'py>(
                &self,
                py: Python<'py>,
                id: &str,
                vector: PyReadonlyArray1<'py, f32>,
                metadata: Option<pyo3::Bound<'py, pyo3::types::PyDict>>,
            ) -> PyResult<()> {
                let rt = get_runtime()?;
                let v = vector.as_slice().map_err(|e| {
                    pyo3::exceptions::PyValueError::new_err(format!("Invalid vector: {}", e))
                })?;
                validate_id_and_vector(id, v)?;
                let m = opt_dict_to_json(metadata.as_ref())?;
                py.allow_threads(|| rt.block_on(self.inner.update(id, v, m)))
                    .map_err(memfuse_err)
            }

            /// Upserts a document (inserts if missing, updates if exists).
            #[pyo3(signature = (id, vector, metadata=None))]
            pub fn upsert<'py>(
                &self,
                py: Python<'py>,
                id: &str,
                vector: PyReadonlyArray1<'py, f32>,
                metadata: Option<pyo3::Bound<'py, pyo3::types::PyDict>>,
            ) -> PyResult<()> {
                let rt = get_runtime()?;
                let v = vector.as_slice().map_err(|e| {
                    pyo3::exceptions::PyValueError::new_err(format!("Invalid vector: {}", e))
                })?;
                validate_id_and_vector(id, v)?;
                let m = opt_dict_to_json(metadata.as_ref())?;
                py.allow_threads(|| rt.block_on(self.inner.upsert(id, v, m)))
                    .map_err(memfuse_err)
            }

            /// Deletes a document by its ID.
            pub fn delete(&self, py: Python<'_>, id: &str) -> PyResult<()> {
                validate_id(id)?;
                let rt = get_runtime()?;
                py.allow_threads(|| rt.block_on(self.inner.delete(id)))
                    .map_err(memfuse_err)
            }

            /// Performs semantic k-NN search over the embeddings.
            #[pyo3(signature = (vector, k))]
            pub fn search<'py>(
                &self,
                py: Python<'py>,
                vector: PyReadonlyArray1<'py, f32>,
                k: usize,
            ) -> PyResult<Vec<PySearchResult>> {
                if k == 0 || k > 1000 {
                    return Err(pyo3::exceptions::PyValueError::new_err(format!(
                        "Search k must be between 1 and 1000. Got: {}",
                        k
                    )));
                }
                let rt = get_runtime()?;
                let v = vector.as_slice().map_err(|e| {
                    pyo3::exceptions::PyValueError::new_err(format!("Invalid vector: {}", e))
                })?;
                validate_vector(v)?;
                let results = py
                    .allow_threads(|| rt.block_on(self.inner.search(v, k)))
                    .map_err(memfuse_err)?;
                results_to_py(py, results)
            }

            /// Performs semantic search and returns results as FlatBuffer (zero-copy).
            #[pyo3(signature = (vector, k))]
            pub fn search_fb<'py>(
                &self,
                py: Python<'py>,
                vector: PyReadonlyArray1<'py, f32>,
                k: usize,
            ) -> PyResult<Bound<'py, PyBytes>> {
                if k == 0 || k > 1000 {
                    return Err(pyo3::exceptions::PyValueError::new_err(format!(
                        "Search k must be between 1 and 1000. Got: {}",
                        k
                    )));
                }
                let rt = get_runtime()?;
                let v = vector.as_slice().map_err(|e| {
                    pyo3::exceptions::PyValueError::new_err(format!("Invalid vector: {}", e))
                })?;
                validate_vector(v)?;
                let results = py
                    .allow_threads(|| rt.block_on(self.inner.search(v, k)))
                    .map_err(memfuse_err)?;

                let mut builder = flatbuffers::FlatBufferBuilder::with_capacity(1024);
                let mut res_offsets = Vec::with_capacity(results.len());

                for r in results {
                    let id_off = builder.create_string(&r.id);
                    let meta_str = r.metadata.map(|m| m.to_string()).unwrap_or_default();
                    let meta_off = builder.create_string(&meta_str);

                    let doc_res = memfuse_core::ipc::ScoredDocument::create(
                        &mut builder,
                        &memfuse_core::ipc::ScoredDocumentArgs {
                            id: Some(id_off),
                            score: r.score,
                            metadata: Some(meta_off),
                            embedding: None,
                        },
                    );
                    res_offsets.push(doc_res);
                }

                let results_vec_off = builder.create_vector(&res_offsets);
                let response = memfuse_core::ipc::SearchResponse::create(
                    &mut builder,
                    &memfuse_core::ipc::SearchResponseArgs {
                        results: Some(results_vec_off),
                        total_hits: res_offsets.len() as u32,
                        processing_time_ms: 0.0,
                    },
                );

                builder.finish(response, None);
                let data = builder.finished_data();
                Ok(PyBytes::new(py, data))
            }

            /// Performs hybrid search combining BM25, vector search, and graph traversal results.
            #[allow(clippy::too_many_arguments)]
            #[pyo3(signature = (text, vector, k, vector_weight=None, text_weight=None, graph_weight=None))]
            pub fn hybrid_search<'py>(
                &self,
                py: Python<'py>,
                text: &str,
                vector: PyReadonlyArray1<'py, f32>,
                k: usize,
                vector_weight: Option<f32>,
                text_weight: Option<f32>,
                graph_weight: Option<f32>,
            ) -> PyResult<Vec<PySearchResult>> {
                if k == 0 || k > 1000 {
                    return Err(pyo3::exceptions::PyValueError::new_err(format!(
                        "Search k must be between 1 and 1000. Got: {}",
                        k
                    )));
                }
                let weights = match (vector_weight, text_weight, graph_weight) {
                    (Some(v), Some(t), Some(g)) => {
                        Some(memfuse_core::FusionWeights::new(v, t, g)
                            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?)
                    }
                    (None, None, None) => None,
                    _ => return Err(pyo3::exceptions::PyValueError::new_err(
                        "Must specify either all three weights (vector_weight, text_weight, graph_weight) or none"
                    )),
                };
                let rt = get_runtime()?;
                let v = vector.as_slice().map_err(|e| {
                    pyo3::exceptions::PyValueError::new_err(format!("Invalid vector: {}", e))
                })?;
                validate_vector(v)?;
                let results = py
                    .allow_threads(|| rt.block_on(self.inner.hybrid_search_with_weights(text, v, k, None, weights.as_ref())))
                    .map_err(memfuse_err)?;
                results_to_py(py, results)
            }

            /// Performs hybrid search and returns results as FlatBuffer (zero-copy).
            #[allow(clippy::too_many_arguments)]
            #[pyo3(signature = (text, vector, k, vector_weight=None, text_weight=None, graph_weight=None))]
            pub fn hybrid_search_fb<'py>(
                &self,
                py: Python<'py>,
                text: &str,
                vector: PyReadonlyArray1<'py, f32>,
                k: usize,
                vector_weight: Option<f32>,
                text_weight: Option<f32>,
                graph_weight: Option<f32>,
            ) -> PyResult<Bound<'py, PyBytes>> {
                if k == 0 || k > 1000 {
                    return Err(pyo3::exceptions::PyValueError::new_err(format!(
                        "Search k must be between 1 and 1000. Got: {}",
                        k
                    )));
                }
                let weights = match (vector_weight, text_weight, graph_weight) {
                    (Some(v), Some(t), Some(g)) => {
                        Some(memfuse_core::FusionWeights::new(v, t, g)
                            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?)
                    }
                    (None, None, None) => None,
                    _ => return Err(pyo3::exceptions::PyValueError::new_err(
                        "Must specify either all three weights (vector_weight, text_weight, graph_weight) or none"
                    )),
                };
                let rt = get_runtime()?;
                let v = vector.as_slice().map_err(|e| {
                    pyo3::exceptions::PyValueError::new_err(format!("Invalid vector: {}", e))
                })?;
                let results = py
                    .allow_threads(|| rt.block_on(self.inner.hybrid_search_with_weights(text, v, k, None, weights.as_ref())))
                    .map_err(memfuse_err)?;

                let mut builder = flatbuffers::FlatBufferBuilder::with_capacity(1024);
                let mut res_offsets = Vec::with_capacity(results.len());

                for r in results {
                    let id_off = builder.create_string(&r.id);
                    let meta_str = r.metadata.map(|m| m.to_string()).unwrap_or_default();
                    let meta_off = builder.create_string(&meta_str);

                    let doc_res = memfuse_core::ipc::ScoredDocument::create(
                        &mut builder,
                        &memfuse_core::ipc::ScoredDocumentArgs {
                            id: Some(id_off),
                            score: r.score,
                            metadata: Some(meta_off),
                            embedding: None,
                        },
                    );
                    res_offsets.push(doc_res);
                }

                let results_vec_off = builder.create_vector(&res_offsets);
                let response = memfuse_core::ipc::SearchResponse::create(
                    &mut builder,
                    &memfuse_core::ipc::SearchResponseArgs {
                        results: Some(results_vec_off),
                        total_hits: res_offsets.len() as u32,
                        processing_time_ms: 0.0,
                    },
                );

                builder.finish(response, None);
                let data = builder.finished_data();
                Ok(PyBytes::new(py, data))
            }

            /// Creates a bidirectional relationship between two documents.
            pub fn relate(
                &self,
                py: Python<'_>,
                from: &str,
                to: &str,
                label: &str,
            ) -> PyResult<()> {
                validate_id(from)?;
                validate_id(to)?;
                if label.is_empty() {
                    return Err(MemFuseValueError::new_err(
                        "Relationship label cannot be empty",
                    ));
                }
                if label.len() > MAX_LABEL_LENGTH {
                    return Err(MemFuseValueError::new_err(format!(
                        "Relationship label exceeds maximum length of {} bytes. Got: {}",
                        MAX_LABEL_LENGTH,
                        label.len()
                    )));
                }
                let rt = get_runtime()?;
                py.allow_threads(|| rt.block_on(self.inner.relate(from, to, label)))
                    .map_err(memfuse_err)
            }

            /// Scans documents matching a given key prefix.
            #[pyo3(signature = (prefix=""))]
            pub fn scan_prefix(
                &self,
                py: Python<'_>,
                prefix: &str,
            ) -> PyResult<Vec<(String, PyObject)>> {
                let rt = get_runtime()?;
                let results = py
                    .allow_threads(|| rt.block_on(self.inner.scan_prefix(prefix)))
                    .map_err(memfuse_err)?;
                let mut py_res = Vec::with_capacity(results.len());
                for (k, v) in results {
                    py_res.push((k, json_to_py(py, &v)?));
                }
                Ok(py_res)
            }

            /// Performs a range scan of documents.
            ///
            /// Accepts optional string keys for start and end bounds (inclusive).
            /// Pass `None` for unbounded.
            #[pyo3(signature = (start=None, end=None))]
            pub fn scan(
                &self,
                py: Python<'_>,
                start: Option<&str>,
                end: Option<&str>,
            ) -> PyResult<Vec<(String, PyObject)>> {
                let rt = get_runtime()?;
                use std::ops::Bound;

                let start_bytes: Option<Vec<u8>> = start.map(|s| s.as_bytes().to_vec());
                let end_bytes: Option<Vec<u8>> = end.map(|s| s.as_bytes().to_vec());

                let start_bound = match &start_bytes {
                    Some(b) => Bound::Included(b.as_slice()),
                    None => Bound::Unbounded,
                };
                let end_bound = match &end_bytes {
                    Some(b) => Bound::Included(b.as_slice()),
                    None => Bound::Unbounded,
                };

                let results = py
                    .allow_threads(|| rt.block_on(self.inner.scan(start_bound, end_bound)))
                    .map_err(memfuse_err)?;
                let mut py_res = Vec::with_capacity(results.len());
                for (k, v) in results {
                    py_res.push((k, json_to_py(py, &v)?));
                }
                Ok(py_res)
            }
        }
    };
}

// ─── Macro: Batch Methods ───────────────────────────────────────────────────
//
// Batch insert/upsert operations. Separated because the inner types
// (MemFuse vs Collection) share identical batch signatures.

macro_rules! memfuse_batch_methods {
    ($struct_type:ty) => {
        #[pymethods]
        impl $struct_type {
            /// Inserts multiple documents in a single transaction.
            ///
            /// Each doc is a tuple of `(id: str, vector: np.ndarray, metadata: dict | None)`.
            pub fn insert_many<'py>(
                &self,
                py: Python<'py>,
                docs: Vec<(
                    String,
                    PyReadonlyArray1<'py, f32>,
                    Option<pyo3::Bound<'py, pyo3::types::PyDict>>,
                )>,
            ) -> PyResult<()> {
                if docs.len() > MAX_BATCH_SIZE {
                    return Err(MemFuseValueError::new_err(format!(
                        "Batch size exceeds maximum limit of {} items. Got: {}",
                        MAX_BATCH_SIZE,
                        docs.len()
                    )));
                }
                let rt = get_runtime()?;
                let mut batch: Vec<(String, Vec<f32>, Option<serde_json::Value>)> =
                    Vec::with_capacity(docs.len());
                for (id, vector, metadata) in &docs {
                    let v = vector.as_slice().map_err(|e| {
                        pyo3::exceptions::PyValueError::new_err(format!("Invalid vector: {}", e))
                    })?;
                    validate_id_and_vector(id, v)?;
                    let m = opt_dict_to_json(metadata.as_ref())?;
                    batch.push((id.clone(), v.to_vec(), m));
                }
                py.allow_threads(|| rt.block_on(self.inner.insert_many(&batch)))
                    .map_err(memfuse_err)
            }

            /// Upserts multiple documents in a single transaction.
            ///
            /// Each doc is a tuple of `(id: str, vector: np.ndarray, metadata: dict | None)`.
            pub fn upsert_many<'py>(
                &self,
                py: Python<'py>,
                docs: Vec<(
                    String,
                    PyReadonlyArray1<'py, f32>,
                    Option<pyo3::Bound<'py, pyo3::types::PyDict>>,
                )>,
            ) -> PyResult<()> {
                if docs.len() > MAX_BATCH_SIZE {
                    return Err(MemFuseValueError::new_err(format!(
                        "Batch size exceeds maximum limit of {} items. Got: {}",
                        MAX_BATCH_SIZE,
                        docs.len()
                    )));
                }
                let rt = get_runtime()?;
                let mut batch: Vec<(String, Vec<f32>, Option<serde_json::Value>)> =
                    Vec::with_capacity(docs.len());
                for (id, vector, metadata) in &docs {
                    let v = vector.as_slice().map_err(|e| {
                        pyo3::exceptions::PyValueError::new_err(format!("Invalid vector: {}", e))
                    })?;
                    validate_id_and_vector(id, v)?;
                    let m = opt_dict_to_json(metadata.as_ref())?;
                    batch.push((id.clone(), v.to_vec(), m));
                }
                py.allow_threads(|| rt.block_on(self.inner.upsert_many(&batch)))
                    .map_err(memfuse_err)
            }
        }
    };
}

// ─── PyMemFuse (Database Facade) ────────────────────────────────────────────

#[pyclass(name = "Db")]
pub struct PyMemFuse {
    inner: Arc<MemFuse>,
}

#[pymethods]
impl PyMemFuse {
    // ── Context Manager Protocol ──

    /// Enters the context manager. Returns `self`.
    fn __enter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    /// Exits the context manager, flushing all pending writes.
    #[pyo3(signature = (_exc_type=None, _exc_val=None, _exc_tb=None))]
    fn __exit__(
        &self,
        py: Python<'_>,
        _exc_type: Option<&Bound<'_, PyAny>>,
        _exc_val: Option<&Bound<'_, PyAny>>,
        _exc_tb: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<bool> {
        let rt = get_runtime()?;
        py.allow_threads(|| rt.block_on(self.inner.flush()))
            .map_err(memfuse_err)?;
        Ok(false) // Do not suppress exceptions
    }

    // ── Collection Management ──

    /// Returns a specific collection (namespace).
    /// Creates the collection if it does not already exist.
    pub fn collection(&self, name: &str, py: Python<'_>) -> PyResult<PyCollection> {
        let rt = get_runtime()?;
        let col = py
            .allow_threads(|| rt.block_on(self.inner.collection(name)))
            .map_err(memfuse_err)?;
        Ok(PyCollection { inner: col })
    }

    /// Lists all existing collection names.
    pub fn list_collections(&self, py: Python<'_>) -> PyResult<Vec<String>> {
        let rt = get_runtime()?;
        py.allow_threads(|| rt.block_on(self.inner.list_collections()))
            .map_err(memfuse_err)
    }

    /// Drops a collection, removing all its data from storage.
    pub fn drop_collection(&self, name: &str, py: Python<'_>) -> PyResult<()> {
        let rt = get_runtime()?;
        py.allow_threads(|| rt.block_on(self.inner.drop_collection(name)))
            .map_err(memfuse_err)
    }

    /// Flushes all pending writes to disk.
    pub fn flush(&self, py: Python<'_>) -> PyResult<()> {
        let rt = get_runtime()?;
        py.allow_threads(|| rt.block_on(self.inner.flush()))
            .map_err(memfuse_err)
    }

    /// Returns combined statistics for the vector index and storage engine.
    pub fn stats(&self, py: Python<'_>) -> PyResult<PyDbStats> {
        let rt = get_runtime()?;
        let stats = py
            .allow_threads(|| rt.block_on(self.inner.stats()))
            .map_err(memfuse_err)?;

        Ok(PyDbStats {
            index_stats: PyVectorIndexStats {
                num_vectors: stats.index_stats.num_vectors,
                memory_usage_bytes: stats.index_stats.memory_usage_bytes,
                num_layers: stats.index_stats.num_layers,
            },
            storage_stats: PyStorageStats {
                num_segments: stats.storage_stats.num_segments,
                total_size_bytes: stats.storage_stats.total_size_bytes,
                memtable_size_bytes: stats.storage_stats.memtable_size_bytes,
            },
        })
    }

    /// Returns the number of documents.
    pub fn len(&self, py: Python<'_>) -> PyResult<usize> {
        let rt = get_runtime()?;
        py.allow_threads(|| rt.block_on(self.inner.len()))
            .map_err(memfuse_err)
    }

    /// Returns true if the collection/database is empty.
    pub fn is_empty(&self, py: Python<'_>) -> PyResult<bool> {
        let rt = get_runtime()?;
        py.allow_threads(|| rt.block_on(self.inner.is_empty()))
            .map_err(memfuse_err)
    }
}

// ── Generated CRUD + Batch Methods ──
memfuse_crud_methods!(PyMemFuse);
memfuse_batch_methods!(PyMemFuse);

// ─── PyCollection ───────────────────────────────────────────────────────────

#[pyclass(name = "Collection")]
pub struct PyCollection {
    inner: Arc<MemFuseCollection>,
}

#[pymethods]
impl PyCollection {
    /// Returns statistics for the collection's vector index.
    pub fn stats(&self, py: Python<'_>) -> PyResult<PyVectorIndexStats> {
        let rt = get_runtime()?;
        let stats = py
            .allow_threads(|| rt.block_on(self.inner.stats()))
            .map_err(memfuse_err)?;

        Ok(PyVectorIndexStats {
            num_vectors: stats.num_vectors,
            memory_usage_bytes: stats.memory_usage_bytes,
            num_layers: stats.num_layers,
        })
    }

    /// Returns the number of documents.
    pub fn len(&self, py: Python<'_>) -> PyResult<usize> {
        let rt = get_runtime()?;
        Ok(py.allow_threads(|| rt.block_on(self.inner.len())))
    }

    /// Returns true if the collection is empty.
    pub fn is_empty(&self, py: Python<'_>) -> PyResult<bool> {
        let rt = get_runtime()?;
        Ok(py.allow_threads(|| rt.block_on(self.inner.is_empty())))
    }
}

// ── Generated CRUD + Batch Methods ──
memfuse_crud_methods!(PyCollection);
memfuse_batch_methods!(PyCollection);

// ─── Module Entry Point ─────────────────────────────────────────────────────

/// Opens or creates a MemFuse database at the given path.
///
/// Supports Python context manager protocol:
/// ```python
/// with memfuse.open("./data") as db:
///     db.insert("doc1", vector, {"key": "value"})
/// ```
#[pyfunction]
#[pyo3(signature = (path, dimension=1536, max_elements=None, encryption_passphrase=None, distance_metric=None))]
fn open(
    py: Python<'_>,
    path: &str,
    dimension: usize,
    max_elements: Option<usize>,
    encryption_passphrase: Option<String>,
    distance_metric: Option<String>,
) -> PyResult<PyMemFuse> {
    if dimension == 0 || dimension > 10_000 {
        return Err(pyo3::exceptions::PyValueError::new_err(format!(
            "Dimension must be between 1 and 10000. Got: {}",
            dimension
        )));
    }
    let rt = get_runtime()?;
    let mut config = MemFuseConfig {
        dimension,
        encryption_passphrase,
        ..Default::default()
    };

    if let Some(me) = max_elements {
        config.max_elements = me;
    }

    if let Some(dm) = distance_metric {
        config.distance_metric = match dm.to_lowercase().as_str() {
            "cosine" => memfuse_db::DistanceMetric::Cosine,
            "euclidean" | "l2" => memfuse_db::DistanceMetric::Euclidean,
            "dot" | "dotproduct" => memfuse_db::DistanceMetric::DotProduct,
            _ => {
                return Err(pyo3::exceptions::PyValueError::new_err(format!(
                    "Unsupported distance metric: {}",
                    dm
                )))
            }
        };
    }

    let path_string = path.to_string();
    let db = py
        .allow_threads(|| rt.block_on(MemFuse::open_with_config(path_string, config)))
        .map_err(memfuse_err)?;

    Ok(PyMemFuse {
        inner: Arc::new(db),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use memfuse_core::MemFuseError;
    use pyo3::exceptions::*;

    #[test]
    fn test_py_err_not_found_maps_to_key_error() {
        pyo3::prepare_freethreaded_python();
        let err = MemFuseError::NotFound("doc1".into());
        let py_err: PyErr = memfuse_err(err);
        Python::with_gil(|py| {
            assert!(py_err.is_instance_of::<PyKeyError>(py));
        });
    }

    #[test]
    fn test_py_err_invalid_input_maps_to_value_error() {
        pyo3::prepare_freethreaded_python();
        let err = MemFuseError::InvalidInput("invalid key".into());
        let py_err: PyErr = memfuse_err(err);
        Python::with_gil(|py| {
            assert!(py_err.is_instance_of::<PyValueError>(py));
        });
    }

    #[test]
    fn test_py_err_policy_violation_maps_to_permission_error() {
        pyo3::prepare_freethreaded_python();
        let err = MemFuseError::PolicyViolation("access denied".into());
        let py_err: PyErr = memfuse_err(err);
        Python::with_gil(|py| {
            assert!(py_err.is_instance_of::<PyPermissionError>(py));
        });
    }

    #[test]
    fn test_validate_id_length_and_empty() {
        assert!(validate_id("").is_err());
        assert!(validate_id("valid_id").is_ok());

        let long_id = "a".repeat(MAX_ID_LENGTH + 1);
        assert!(validate_id(&long_id).is_err());

        let max_id = "a".repeat(MAX_ID_LENGTH);
        assert!(validate_id(&max_id).is_ok());
    }

    #[test]
    fn test_validate_vector_nan_inf() {
        assert!(validate_vector(&[1.0, 2.0, 3.0]).is_ok());
        assert!(validate_vector(&[1.0, f32::NAN, 3.0]).is_err());
        assert!(validate_vector(&[1.0, f32::INFINITY, 3.0]).is_err());
        assert!(validate_vector(&[1.0, f32::NEG_INFINITY, 3.0]).is_err());
    }

    #[test]
    fn test_py_err_io_and_index_mappings() {
        pyo3::prepare_freethreaded_python();
        let io_err = MemFuseError::Io(std::io::Error::other("disk error"));
        let py_io_err: PyErr = memfuse_err(io_err);
        Python::with_gil(|py| {
            assert!(py_io_err.is_instance_of::<MemFuseIOError>(py));
        });

        let idx_err = MemFuseError::Index("hnsw broken".into());
        let py_idx_err: PyErr = memfuse_err(idx_err);
        Python::with_gil(|py| {
            assert!(py_idx_err.is_instance_of::<MemFuseIndexError>(py));
        });
    }
}

#[pymodule]
fn _memfuse(_py: Python<'_>, m: &Bound<'_, pyo3::types::PyModule>) -> PyResult<()> {
    m.add("__version__", "0.2.0")?;
    m.add_function(wrap_pyfunction!(open, m)?)?;
    m.add_class::<PyMemFuse>()?;
    m.add_class::<PyCollection>()?;
    m.add_class::<PySearchResult>()?;
    m.add_class::<PyDocument>()?;
    m.add_class::<PyVectorIndexStats>()?;
    m.add_class::<PyStorageStats>()?;
    m.add_class::<PyDbStats>()?;

    // Exceptions
    m.add("MemFuseError", _py.get_type::<MemFuseError>())?;
    m.add("MemFuseIOError", _py.get_type::<MemFuseIOError>())?;
    m.add("MemFuseIndexError", _py.get_type::<MemFuseIndexError>())?;
    m.add("MemFuseValueError", _py.get_type::<MemFuseValueError>())?;
    m.add("MemFuseCryptoError", _py.get_type::<MemFuseCryptoError>())?;
    m.add(
        "MemFuseInternalError",
        _py.get_type::<MemFuseInternalError>(),
    )?;

    Ok(())
}
