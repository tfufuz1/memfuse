// FILE-CONTEXT:
// ZWECK: PyO3 FFI bindings bridging MemFuse embedded vector DB functionality to Python.
// INVARIANTEN: Zero Rust panics cross FFI boundary; GIL released during block_on async calls; Tokio Runtime bound per interpreter module state.
// NICHT-OFFENSICHTLICH: Per-interpreter Tokio runtime instance attached to Python module state (`PyRuntimeState`) to enforce sub-interpreter isolation (PEP 684).
// HOTSPOTS: [160-205] memfuse_err mapping, [270-650] CRUD & search methods FFI boundary validation.
// STAND: TS:2026-09-03T10:00:00Z (SESSION: 14a123bc)

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
//! - **Minimal Copying**: Zero-copy borrowing of input vector data from NumPy arrays into Rust; FlatBuffer responses returned as PyBytes.

#![forbid(unsafe_code)]

// FILE-CONTEXT
// STAND:       2026-09-03T10:00:00Z (SESSION: 14a123bc)
// ZWECK:       PyO3 FFI-Grenzschicht — Rust-Fehler müssen in Python-Exceptions konvertiert werden
// INVARIANTEN: Alle MemFuseError -> PyErr Konvertierung vollständig; kein Panic darf FFI-Grenze überschreiten
// HOTSPOTS:    PyMemFuse, PyCollection methods, error conversion
// SIEHE AUCH:  crates/memfuse-db/AGENTS.md

use memfuse_db::{Collection as MemFuseCollection, MemFuse, MemFuseConfig};
use numpy::PyReadonlyArray1;
use pyo3::exceptions::{PyKeyError, PyPermissionError, PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyBytes;
use pythonize::{depythonize, pythonize};
use std::sync::Arc;
use tokio::runtime::Runtime;

// ─── Custom Exceptions ──────────────────────────────────────────────────────

pyo3::create_exception!(_memfuse, MemFuseError, pyo3::exceptions::PyException);
pyo3::create_exception!(_memfuse, MemFuseIOError, MemFuseError);
pyo3::create_exception!(_memfuse, MemFuseIndexError, MemFuseError);
pyo3::create_exception!(_memfuse, MemFuseValueError, MemFuseError);
pyo3::create_exception!(_memfuse, MemFuseCryptoError, MemFuseError);
pyo3::create_exception!(_memfuse, MemFuseInternalError, MemFuseError);

// ─── Per-Interpreter Tokio Runtime State ───────────────────────────────────

/// Holds the per-interpreter/per-module Tokio runtime state and worker thread configuration.
#[pyclass(name = "RuntimeState")]
#[derive(Clone)]
pub struct PyRuntimeState {
    pub runtime: Arc<Runtime>,
    pub worker_threads: usize,
}

/// Retrieves or initializes the per-interpreter Tokio runtime attached to the `_memfuse` module state.
///
/// Reads `MEMFUSE_WORKER_THREADS` on initialization for the current interpreter/module context.
fn get_runtime(py: Python<'_>) -> PyResult<Arc<Runtime>> {
    let module = py
        .import("memfuse._memfuse")
        .or_else(|_| py.import("_memfuse"))?;
    if let Ok(state_attr) = module.getattr("_runtime_state") {
        if let Ok(state) = state_attr.extract::<PyRef<'_, PyRuntimeState>>() {
            return Ok(state.runtime.clone());
        }
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

    let runtime = Arc::new(rt);
    let state = PyRuntimeState {
        runtime: runtime.clone(),
        worker_threads,
    };

    let py_state = Py::new(py, state)?;
    let _ = module.setattr("_runtime_state", py_state);

    Ok(runtime)
}

// ─── Shared Helper Functions ────────────────────────────────────────────────

/// Converts a Python dict to a serde_json::Value.
fn dict_to_json(d: &pyo3::Bound<'_, pyo3::types::PyDict>) -> PyResult<serde_json::Value> {
    depythonize(d).map_err(|e| PyValueError::new_err(format!("Metadata error: {}", e)))
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

/// Validates that a string ID is non-empty, contains no null bytes, and does not exceed maximum length.
fn validate_id(id: &str) -> PyResult<()> {
    if id.trim().is_empty() {
        return Err(MemFuseValueError::new_err(
            "Document ID cannot be empty or whitespace-only",
        ));
    }
    if id.contains('\0') {
        return Err(MemFuseValueError::new_err(
            "Document ID cannot contain null bytes",
        ));
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

/// Validates that a collection name is non-empty, contains no null bytes, and does not exceed maximum length.
fn validate_collection_name(name: &str) -> PyResult<()> {
    if name.trim().is_empty() {
        return Err(MemFuseValueError::new_err(
            "Collection name cannot be empty or whitespace-only",
        ));
    }
    if name.contains('\0') {
        return Err(MemFuseValueError::new_err(
            "Collection name cannot contain null bytes",
        ));
    }
    if name.len() > 64 {
        return Err(MemFuseValueError::new_err(format!(
            "Collection name exceeds maximum length of 64 bytes. Got: {}",
            name.len()
        )));
    }
    Ok(())
}

/// Validates that a database storage path is non-empty and contains no null bytes.
fn validate_db_path(path: &str) -> PyResult<()> {
    if path.trim().is_empty() {
        return Err(MemFuseValueError::new_err(
            "Database path cannot be empty or whitespace-only",
        ));
    }
    if path.contains('\0') {
        return Err(MemFuseValueError::new_err(
            "Database path cannot contain null bytes",
        ));
    }
    Ok(())
}

/// Validates search query text.
fn validate_query_text(text: &str) -> PyResult<()> {
    if text.trim().is_empty() {
        return Err(MemFuseValueError::new_err(
            "Search query text cannot be empty or whitespace-only",
        ));
    }
    if text.contains('\0') {
        return Err(MemFuseValueError::new_err(
            "Search query text cannot contain null bytes",
        ));
    }
    if text.len() > MAX_ID_LENGTH {
        return Err(MemFuseValueError::new_err(format!(
            "Query text exceeds maximum length of {} bytes. Got: {}",
            MAX_ID_LENGTH,
            text.len()
        )));
    }
    Ok(())
}

/// Validates batch size against maximum resource allocation limits.
fn validate_batch_size(size: usize) -> PyResult<()> {
    if size == 0 {
        return Err(MemFuseValueError::new_err("Batch cannot be empty"));
    }
    if size > MAX_BATCH_SIZE {
        return Err(MemFuseValueError::new_err(format!(
            "Batch size {} exceeds maximum allowed limit of {}",
            size, MAX_BATCH_SIZE
        )));
    }
    Ok(())
}

/// Validates that a vector slice is non-empty and contains no NaN or infinite values.
fn validate_vector(vector: &[f32]) -> PyResult<()> {
    if vector.is_empty() {
        return Err(MemFuseValueError::new_err("Vector cannot be empty"));
    }
    if vector.iter().any(|x| x.is_nan() || x.is_infinite()) {
        return Err(MemFuseValueError::new_err(
            "Vector contains NaN or infinite float values",
        ));
    }
    Ok(())
}

/// Validates a document ID provided as a string or numeric value.
fn validate_id_obj(id_obj: &pyo3::Bound<'_, pyo3::types::PyAny>) -> PyResult<String> {
    if let Ok(id_str) = id_obj.extract::<String>() {
        validate_id(&id_str)?;
        Ok(id_str)
    } else if let Ok(id_int) = id_obj.extract::<i128>() {
        if id_int < 0 {
            return Err(MemFuseValueError::new_err(
                "Document ID cannot be a negative integer",
            ));
        }
        if id_int > (u64::MAX as i128) {
            return Err(MemFuseValueError::new_err(
                "Document ID integer value exceeds maximum allowed bound (u64::MAX)",
            ));
        }
        Ok(id_int.to_string())
    } else if id_obj.is_instance_of::<pyo3::types::PyInt>() {
        Err(MemFuseValueError::new_err(
            "Document ID integer value exceeds maximum allowed bound",
        ))
    } else {
        Err(MemFuseValueError::new_err(
            "Document ID must be a string or non-negative integer",
        ))
    }
}

/// Explicitly checks whether the module is being imported inside a CPython sub-interpreter.
///
/// If running inside a sub-interpreter (interpreter ID != 0), returns `PyImportError` explaining
/// that sub-interpreters are not supported due to per-process Tokio runtime isolation.
fn check_subinterpreter_guard(py: Python<'_>) -> PyResult<()> {
    let current_id: Option<i64> = if let Ok(interp_mod) = py.import("_xxsubinterpreters") {
        interp_mod
            .call_method0("get_current")
            .ok()
            .and_then(|id| id.extract().ok())
    } else if let Ok(interp_mod) = py.import("_interpreters") {
        interp_mod
            .call_method0("get_current")
            .ok()
            .and_then(|id| id.extract().ok())
    } else {
        None
    };

    if let Some(id) = current_id {
        if id != 0 {
            return Err(pyo3::exceptions::PyImportError::new_err(
                "memfuse does not support loading in sub-interpreters due to per-process Tokio runtime isolation",
            ));
        }
    }
    Ok(())
}

// AI-TAG[SECURITY][MAJOR][RESOLVED] panic="abort" in workspace Cargo.toml release profile disables catch_unwind (ID: AGT-PY-d5d2be30) (TS: 2026-09-06T11:19:12Z) (SESSION: 831f9286)
// BEFUND: Resolved by decoupling `crates/memfuse-py` into an independent workspace with its own `[profile.release]` setting `panic = "unwind"`.
// BEHOBEN: `std::panic::catch_unwind` in `run_blocking_ffi` intercepts panics in release builds, converting them into catchable PyRuntimeError exceptions without aborting CPython via SIGABRT.
/// Safely executes a blocking closure across FFI boundaries with thread state release
/// and panic containment to guarantee no Rust panic propagates across FFI boundaries into Python.
fn run_blocking_ffi<F, R>(py: Python<'_>, f: F) -> PyResult<R>
where
    F: FnOnce() -> PyResult<R> + Send,
    R: Send,
{
    let panic_result =
        py.allow_threads(|| std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)));
    match panic_result {
        Ok(res) => res,
        Err(panic_payload) => {
            let panic_msg = if let Some(s) = panic_payload.downcast_ref::<&str>() {
                (*s).to_string()
            } else if let Some(s) = panic_payload.downcast_ref::<String>() {
                s.clone()
            } else {
                "Unknown panic inside Rust core".to_string()
            };
            Err(pyo3::exceptions::PyRuntimeError::new_err(format!(
                "Rust panic caught at FFI boundary: {}",
                panic_msg
            )))
        }
    }
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
        if value.setattr("kind", dto.kind).is_err() {
            // Ignore non-fatal attribute attachment error if py_err instance doesn't support setattr
        }
        if value.setattr("message", dto.message).is_err() {
            // Ignore non-fatal attribute attachment error
        }
        if let Some(ref details) = dto.details {
            if let Ok(details_py) = json_to_py(py, details) {
                if value.setattr("details", details_py).is_err() {
                    // Ignore non-fatal attribute attachment error
                }
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
        #[allow(deprecated)]
        impl $struct_type {
            /// Inserts a document with an embedding and optional metadata.
            #[pyo3(signature = (id, vector, metadata=None))]
            pub fn insert<'py>(
                &self,
                py: Python<'py>,
                id: &pyo3::Bound<'py, pyo3::types::PyAny>,
                vector: PyReadonlyArray1<'py, f32>,
                metadata: Option<pyo3::Bound<'py, pyo3::types::PyDict>>,
            ) -> PyResult<()> {
                let id_str = validate_id_obj(id)?;
                let rt = &self.runtime;
                let v = vector.as_slice().map_err(|e| {
                    pyo3::exceptions::PyValueError::new_err(format!("Invalid vector: {}", e))
                })?;
                validate_vector(v)?;
                let m = opt_dict_to_json(metadata.as_ref())?;
                let v_owned = v.to_vec();
                run_blocking_ffi(py, || rt.block_on(self.inner.insert(&id_str, &v_owned, m)).map_err(memfuse_err))
            }

            /// Retrieves a document by its user-provided string or numeric ID.
            pub fn get<'py>(
                &self,
                py: Python<'py>,
                id: &pyo3::Bound<'py, pyo3::types::PyAny>,
            ) -> PyResult<Option<PyDocument>> {
                let id_str = validate_id_obj(id)?;
                let rt = &self.runtime;
                let doc = run_blocking_ffi(py, || rt.block_on(self.inner.get(&id_str)).map_err(memfuse_err))?;
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
                id: &pyo3::Bound<'py, pyo3::types::PyAny>,
                vector: PyReadonlyArray1<'py, f32>,
                metadata: Option<pyo3::Bound<'py, pyo3::types::PyDict>>,
            ) -> PyResult<()> {
                let id_str = validate_id_obj(id)?;
                let rt = &self.runtime;
                let v = vector.as_slice().map_err(|e| {
                    pyo3::exceptions::PyValueError::new_err(format!("Invalid vector: {}", e))
                })?;
                validate_vector(v)?;
                let m = opt_dict_to_json(metadata.as_ref())?;
                let v_owned = v.to_vec();
                run_blocking_ffi(py, || rt.block_on(self.inner.update(&id_str, &v_owned, m)).map_err(memfuse_err))
            }

            /// Upserts a document (inserts if missing, updates if exists).
            #[pyo3(signature = (id, vector, metadata=None))]
            pub fn upsert<'py>(
                &self,
                py: Python<'py>,
                id: &pyo3::Bound<'py, pyo3::types::PyAny>,
                vector: PyReadonlyArray1<'py, f32>,
                metadata: Option<pyo3::Bound<'py, pyo3::types::PyDict>>,
            ) -> PyResult<()> {
                let id_str = validate_id_obj(id)?;
                let rt = &self.runtime;
                let v = vector.as_slice().map_err(|e| {
                    pyo3::exceptions::PyValueError::new_err(format!("Invalid vector: {}", e))
                })?;
                validate_vector(v)?;
                let m = opt_dict_to_json(metadata.as_ref())?;
                let v_owned = v.to_vec();
                run_blocking_ffi(py, || rt.block_on(self.inner.upsert(&id_str, &v_owned, m)).map_err(memfuse_err))
            }

            /// Deletes a document by its ID.
            pub fn delete<'py>(
                &self,
                py: Python<'py>,
                id: &pyo3::Bound<'py, pyo3::types::PyAny>,
            ) -> PyResult<()> {
                let id_str = validate_id_obj(id)?;
                let rt = &self.runtime;
                run_blocking_ffi(py, || rt.block_on(self.inner.delete(&id_str)).map_err(memfuse_err))
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
                    return Err(PyValueError::new_err(format!(
                        "Search k must be between 1 and 1000. Got: {}",
                        k
                    )));
                }
                let rt = &self.runtime;
                let v = vector.as_slice().map_err(|e| {
                    PyValueError::new_err(format!("Invalid vector: {}", e))
                })?;
                validate_vector(v)?;
                let v_owned = v.to_vec();
                let results = run_blocking_ffi(py, || rt.block_on(self.inner.search(&v_owned, k)).map_err(memfuse_err))?;
                results_to_py(py, results)
            }

            /// Performs semantic search and returns results as FlatBuffer-encoded bytes (PyBytes).
            ///
            /// Returns FlatBuffer binary IPC payload copied into Python PyBytes.
            #[pyo3(signature = (vector, k))]
            pub fn search_fb<'py>(
                &self,
                py: Python<'py>,
                vector: PyReadonlyArray1<'py, f32>,
                k: usize,
            ) -> PyResult<Bound<'py, PyBytes>> {
                // Note on Zero-Copy: True zero-copy return via Python Buffer Protocol is not safely
                // feasible here without `unsafe` code (`__getbuffer__`) and lifetime management risks,
                // because `FlatBufferBuilder` produces a temporary stack/heap buffer during search execution.
                // Returning `PyBytes::new(py, data)` copies the buffer into Python-managed memory safely,
                // preserving `#![forbid(unsafe_code)]` compliance and zero use-after-free risk.
                if k == 0 || k > 1000 {
                    return Err(PyValueError::new_err(format!(
                        "Search k must be between 1 and 1000. Got: {}",
                        k
                    )));
                }
                let rt = &self.runtime;
                let v = vector.as_slice().map_err(|e| {
                    PyValueError::new_err(format!("Invalid vector: {}", e))
                })?;
                validate_vector(v)?;
                let v_owned = v.to_vec();
                let results = run_blocking_ffi(py, || rt.block_on(self.inner.search(&v_owned, k)).map_err(memfuse_err))?;

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
                validate_query_text(text)?;
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
                let rt = &self.runtime;
                let v = vector.as_slice().map_err(|e| {
                    pyo3::exceptions::PyValueError::new_err(format!("Invalid vector: {}", e))
                })?;
                validate_vector(v)?;
                let text_owned = text.to_string();
                let v_owned = v.to_vec();
                let results = run_blocking_ffi(py, || {
                    rt.block_on(self.inner.hybrid_search_with_weights(&text_owned, &v_owned, k, None, weights.as_ref()))
                        .map_err(memfuse_err)
                })?;
                results_to_py(py, results)
            }

            /// Performs hybrid search and returns results as FlatBuffer-encoded bytes (PyBytes).
            ///
            /// Returns FlatBuffer binary IPC payload copied into Python PyBytes.
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
                // Note on Zero-Copy: True zero-copy return via Python Buffer Protocol is not safely
                // feasible here without `unsafe` code (`__getbuffer__`) and lifetime management risks,
                // because `FlatBufferBuilder` produces a temporary stack/heap buffer during search execution.
                // Returning `PyBytes::new(py, data)` copies the buffer into Python-managed memory safely,
                // preserving `#![forbid(unsafe_code)]` compliance and zero use-after-free risk.
                validate_query_text(text)?;
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
                let rt = &self.runtime;
                let v = vector.as_slice().map_err(|e| {
                    pyo3::exceptions::PyValueError::new_err(format!("Invalid vector: {}", e))
                })?;
                validate_vector(v)?;
                let text_owned = text.to_string();
                let v_owned = v.to_vec();
                let results = run_blocking_ffi(py, || {
                    rt.block_on(self.inner.hybrid_search_with_weights(&text_owned, &v_owned, k, None, weights.as_ref()))
                        .map_err(memfuse_err)
                })?;

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
            pub fn relate<'py>(
                &self,
                py: Python<'py>,
                from: &pyo3::Bound<'py, pyo3::types::PyAny>,
                to: &pyo3::Bound<'py, pyo3::types::PyAny>,
                label: &str,
            ) -> PyResult<()> {
                let from_str = validate_id_obj(from)?;
                let to_str = validate_id_obj(to)?;
                if label.trim().is_empty() {
                    return Err(MemFuseValueError::new_err(
                        "Relationship label cannot be empty or whitespace-only",
                    ));
                }
                if label.contains('\0') {
                    return Err(MemFuseValueError::new_err(
                        "Relationship label cannot contain null bytes",
                    ));
                }
                if label.len() > MAX_LABEL_LENGTH {
                    return Err(MemFuseValueError::new_err(format!(
                        "Relationship label exceeds maximum length of {} bytes. Got: {}",
                        MAX_LABEL_LENGTH,
                        label.len()
                    )));
                }
                let rt = &self.runtime;
                let label_owned = label.to_string();
                run_blocking_ffi(py, || {
                    rt.block_on(self.inner.relate(&from_str, &to_str, &label_owned))
                        .map_err(memfuse_err)
                })
            }

            /// Scans documents matching a given key prefix.
            #[pyo3(signature = (prefix=""))]
            pub fn scan_prefix(
                &self,
                py: Python<'_>,
                prefix: &str,
            ) -> PyResult<Vec<(String, PyObject)>> {
                let rt = &self.runtime;
                let prefix_owned = prefix.to_string();
                let results = run_blocking_ffi(py, || {
                    rt.block_on(self.inner.scan_prefix(&prefix_owned))
                        .map_err(memfuse_err)
                })?;
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
                let rt = &self.runtime;
                let start_bytes: Option<Vec<u8>> = start.map(|s| s.as_bytes().to_vec());
                let end_bytes: Option<Vec<u8>> = end.map(|s| s.as_bytes().to_vec());

                let results = run_blocking_ffi(py, || {
                    use std::ops::Bound;
                    let start_bound = match &start_bytes {
                        Some(b) => Bound::Included(b.as_slice()),
                        None => Bound::Unbounded,
                    };
                    let end_bound = match &end_bytes {
                        Some(b) => Bound::Included(b.as_slice()),
                        None => Bound::Unbounded,
                    };
                    rt.block_on(self.inner.scan(start_bound, end_bound))
                        .map_err(memfuse_err)
                })?;
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
                    pyo3::Bound<'py, pyo3::types::PyAny>,
                    PyReadonlyArray1<'py, f32>,
                    Option<pyo3::Bound<'py, pyo3::types::PyDict>>,
                )>,
            ) -> PyResult<()> {
                validate_batch_size(docs.len())?;
                let rt = &self.runtime;
                let mut batch: Vec<(String, Vec<f32>, Option<serde_json::Value>)> =
                    Vec::with_capacity(docs.len());
                for (id_obj, vector, metadata) in &docs {
                    let id_str = validate_id_obj(id_obj)?;
                    let v = vector.as_slice().map_err(|e| {
                        pyo3::exceptions::PyValueError::new_err(format!("Invalid vector: {}", e))
                    })?;
                    validate_vector(v)?;
                    let m = opt_dict_to_json(metadata.as_ref())?;
                    batch.push((id_str, v.to_vec(), m));
                }
                run_blocking_ffi(py, || {
                    rt.block_on(self.inner.insert_many(&batch))
                        .map_err(memfuse_err)
                })
            }

            /// Upserts multiple documents in a single transaction.
            ///
            /// Each doc is a tuple of `(id: str, vector: np.ndarray, metadata: dict | None)`.
            pub fn upsert_many<'py>(
                &self,
                py: Python<'py>,
                docs: Vec<(
                    pyo3::Bound<'py, pyo3::types::PyAny>,
                    PyReadonlyArray1<'py, f32>,
                    Option<pyo3::Bound<'py, pyo3::types::PyDict>>,
                )>,
            ) -> PyResult<()> {
                validate_batch_size(docs.len())?;
                let rt = &self.runtime;
                let mut batch: Vec<(String, Vec<f32>, Option<serde_json::Value>)> =
                    Vec::with_capacity(docs.len());
                for (id_obj, vector, metadata) in &docs {
                    let id_str = validate_id_obj(id_obj)?;
                    let v = vector.as_slice().map_err(|e| {
                        pyo3::exceptions::PyValueError::new_err(format!("Invalid vector: {}", e))
                    })?;
                    validate_vector(v)?;
                    let m = opt_dict_to_json(metadata.as_ref())?;
                    batch.push((id_str, v.to_vec(), m));
                }
                run_blocking_ffi(py, || {
                    rt.block_on(self.inner.upsert_many(&batch))
                        .map_err(memfuse_err)
                })
            }
        }
    };
}

// ─── PyMemFuse (Database Facade) ────────────────────────────────────────────

#[pyclass(name = "Db")]
pub struct PyMemFuse {
    inner: Arc<MemFuse>,
    runtime: Arc<Runtime>,
    worker_threads: usize,
}

#[pymethods]
impl PyMemFuse {
    /// Returns the number of worker threads configured in this database's Tokio runtime.
    #[getter]
    pub fn worker_threads(&self) -> usize {
        self.worker_threads
    }

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
        let rt = &self.runtime;
        run_blocking_ffi(py, || rt.block_on(self.inner.flush()).map_err(memfuse_err))?;
        Ok(false) // Do not suppress exceptions
    }

    // ── Collection Management ──

    /// Returns a specific collection (namespace).
    /// Creates the collection if it does not already exist.
    pub fn collection(&self, name: &str, py: Python<'_>) -> PyResult<PyCollection> {
        validate_collection_name(name)?;
        let rt = &self.runtime;
        let name_owned = name.to_string();
        let col = run_blocking_ffi(py, || {
            rt.block_on(self.inner.collection(&name_owned))
                .map_err(memfuse_err)
        })?;
        Ok(PyCollection {
            inner: col,
            runtime: self.runtime.clone(),
        })
    }

    /// Lists all existing collection names.
    pub fn list_collections(&self, py: Python<'_>) -> PyResult<Vec<String>> {
        let rt = &self.runtime;
        run_blocking_ffi(py, || {
            rt.block_on(self.inner.list_collections())
                .map_err(memfuse_err)
        })
    }

    /// Drops a collection, removing all its data from storage.
    pub fn drop_collection(&self, name: &str, py: Python<'_>) -> PyResult<()> {
        validate_collection_name(name)?;
        let rt = &self.runtime;
        let name_owned = name.to_string();
        run_blocking_ffi(py, || {
            rt.block_on(self.inner.drop_collection(&name_owned))
                .map_err(memfuse_err)
        })
    }

    /// Flushes all pending writes to disk.
    pub fn flush(&self, py: Python<'_>) -> PyResult<()> {
        let rt = &self.runtime;
        run_blocking_ffi(py, || rt.block_on(self.inner.flush()).map_err(memfuse_err))
    }

    /// Returns combined statistics for the vector index and storage engine.
    pub fn stats(&self, py: Python<'_>) -> PyResult<PyDbStats> {
        let rt = &self.runtime;
        let stats = run_blocking_ffi(py, || rt.block_on(self.inner.stats()).map_err(memfuse_err))?;

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
        let rt = &self.runtime;
        run_blocking_ffi(py, || rt.block_on(self.inner.len()).map_err(memfuse_err))
    }

    /// Returns true if the collection/database is empty.
    pub fn is_empty(&self, py: Python<'_>) -> PyResult<bool> {
        let rt = &self.runtime;
        run_blocking_ffi(py, || {
            rt.block_on(self.inner.is_empty()).map_err(memfuse_err)
        })
    }
}

// ── Generated CRUD + Batch Methods ──
memfuse_crud_methods!(PyMemFuse);
memfuse_batch_methods!(PyMemFuse);

// ─── PyCollection ───────────────────────────────────────────────────────────

#[pyclass(name = "Collection")]
pub struct PyCollection {
    inner: Arc<MemFuseCollection>,
    runtime: Arc<Runtime>,
}

#[pymethods]
impl PyCollection {
    /// Returns statistics for the collection's vector index.
    pub fn stats(&self, py: Python<'_>) -> PyResult<PyVectorIndexStats> {
        let rt = &self.runtime;
        let stats = run_blocking_ffi(py, || rt.block_on(self.inner.stats()).map_err(memfuse_err))?;

        Ok(PyVectorIndexStats {
            num_vectors: stats.num_vectors,
            memory_usage_bytes: stats.memory_usage_bytes,
            num_layers: stats.num_layers,
        })
    }

    /// Returns the number of documents.
    pub fn len(&self, py: Python<'_>) -> PyResult<usize> {
        let rt = &self.runtime;
        run_blocking_ffi(py, || Ok(rt.block_on(self.inner.len())))
    }

    /// Returns true if the collection is empty.
    pub fn is_empty(&self, py: Python<'_>) -> PyResult<bool> {
        let rt = &self.runtime;
        run_blocking_ffi(py, || Ok(rt.block_on(self.inner.is_empty())))
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
    validate_db_path(path)?;
    if dimension == 0 || dimension > 10_000 {
        return Err(pyo3::exceptions::PyValueError::new_err(format!(
            "Dimension must be between 1 and 10000. Got: {}",
            dimension
        )));
    }
    let rt = get_runtime(py)?;
    let worker_threads = rt.metrics().num_workers();
    let mut config = MemFuseConfig {
        dimension,
        encryption_passphrase,
        ..Default::default()
    };

    if let Some(me) = max_elements {
        if me == 0 {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "max_elements must be greater than 0",
            ));
        }
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
    let db = run_blocking_ffi(py, || {
        rt.block_on(MemFuse::open_with_config(path_string, config))
            .map_err(memfuse_err)
    })?;

    Ok(PyMemFuse {
        inner: Arc::new(db),
        runtime: rt,
        worker_threads,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use memfuse_core::MemFuseError;
    use pyo3::exceptions::*;

    #[test]
    fn test_py_runtime_state_initialization() {
        pyo3::prepare_freethreaded_python();
        Python::with_gil(|py| {
            let rt_res = get_runtime(py);
            assert!(rt_res.is_ok(), "get_runtime failed: {:?}", rt_res.err());
            let rt = rt_res.unwrap();
            assert!(rt.metrics().num_workers() >= 1);
        });
    }

    #[test]
    fn test_validate_id_guards() {
        pyo3::prepare_freethreaded_python();
        assert!(validate_id("").is_err());
        assert!(validate_id("   ").is_err());
        assert!(validate_id("\t\n").is_err());
        assert!(validate_id("doc\x00123").is_err());
        assert!(validate_id("doc123").is_ok());
    }

    #[test]
    fn test_validate_collection_name_guards() {
        pyo3::prepare_freethreaded_python();
        assert!(validate_collection_name("").is_err());
        assert!(validate_collection_name("   ").is_err());
        assert!(validate_collection_name("col\0name").is_err());
        assert!(validate_collection_name("my_collection").is_ok());
    }

    #[test]
    fn test_validate_db_path_guards() {
        pyo3::prepare_freethreaded_python();
        assert!(validate_db_path("").is_err());
        assert!(validate_db_path("   ").is_err());
        assert!(validate_db_path("./data/\0db").is_err());
        assert!(validate_db_path("./data/db").is_ok());
    }

    #[test]
    fn test_validate_query_text_guards() {
        pyo3::prepare_freethreaded_python();
        assert!(validate_query_text("").is_err());
        assert!(validate_query_text("   ").is_err());
        assert!(validate_query_text("query\0text").is_err());
        assert!(validate_query_text("search query").is_ok());
    }

    #[test]
    fn test_validate_batch_size_guards() {
        pyo3::prepare_freethreaded_python();
        assert!(validate_batch_size(0).is_err());
        assert!(validate_batch_size(MAX_BATCH_SIZE + 1).is_err());
        assert!(validate_batch_size(1).is_ok());
        assert!(validate_batch_size(100).is_ok());
        assert!(validate_batch_size(MAX_BATCH_SIZE).is_ok());
    }

    #[test]
    fn test_validate_vector_guards() {
        pyo3::prepare_freethreaded_python();
        assert!(validate_vector(&[]).is_err());
        assert!(validate_vector(&[1.0, f32::NAN, 0.5]).is_err());
        assert!(validate_vector(&[1.0, f32::INFINITY, 0.5]).is_err());
        assert!(validate_vector(&[1.0, f32::NEG_INFINITY, 0.5]).is_err());
        assert!(validate_vector(&[0.1, 0.2, -0.5]).is_ok());
    }

    #[test]
    fn test_py_err_mapping_all_error_kinds() {
        pyo3::prepare_freethreaded_python();
        Python::with_gil(|py| {
            let err_not_found = memfuse_err(MemFuseError::NotFound("doc1".into()));
            assert!(err_not_found.is_instance_of::<PyKeyError>(py));

            let err_invalid = memfuse_err(MemFuseError::InvalidInput("bad input".into()));
            assert!(err_invalid.is_instance_of::<PyValueError>(py));

            let err_policy = memfuse_err(MemFuseError::PolicyViolation("access denied".into()));
            assert!(err_policy.is_instance_of::<PyPermissionError>(py));

            let err_storage = memfuse_err(MemFuseError::Storage("io error".into()));
            assert!(err_storage.is_instance_of::<MemFuseIOError>(py));

            let err_index = memfuse_err(MemFuseError::Index("hnsw error".into()));
            assert!(err_index.is_instance_of::<MemFuseIndexError>(py));

            let err_crypto = memfuse_err(MemFuseError::Crypto("key error".into()));
            assert!(err_crypto.is_instance_of::<MemFuseCryptoError>(py));

            let err_mem = memfuse_err(MemFuseError::MemoryBudgetExceeded {
                used_mb: 200,
                limit_mb: 100,
            });
            assert!(err_mem.is_instance_of::<PyMemoryError>(py));

            let err_not_impl = memfuse_err(MemFuseError::CapabilityUnsupported {
                capability: "cap".into(),
                reason: "not supported".into(),
            });
            assert!(err_not_impl.is_instance_of::<PyNotImplementedError>(py));

            let err_internal = memfuse_err(MemFuseError::Internal("crash".into()));
            assert!(err_internal.is_instance_of::<MemFuseInternalError>(py));
        });
    }

    #[test]
    fn test_memfuse_err_attributes_set() -> PyResult<()> {
        pyo3::prepare_freethreaded_python();
        Python::with_gil(|py| {
            let py_err = memfuse_err(MemFuseError::NotFound("key123".into()));
            let bound_val = py_err.value(py);
            if let Ok(kind_obj) = bound_val.getattr("kind") {
                let kind: String = kind_obj.extract().unwrap_or_default();
                assert_eq!(kind, "NotFound");
            } else {
                return Err(PyValueError::new_err(
                    "kind attribute missing on MemFuse error object",
                ));
            }
            if let Ok(msg_obj) = bound_val.getattr("message") {
                let msg: String = msg_obj.extract().unwrap_or_default();
                assert!(msg.contains("key123"));
            } else {
                return Err(PyValueError::new_err(
                    "message attribute missing on MemFuse error object",
                ));
            }
            Ok(())
        })
    }

    #[cfg(test)]
    fn _simulate_panic_for_test() -> ! {
        #[cfg(test)]
        panic!("Simulated Rust core panic");
    }

    #[test]
    fn test_run_blocking_ffi_panic_containment() {
        pyo3::prepare_freethreaded_python();
        Python::with_gil(|py| {
            let res: PyResult<i32> = run_blocking_ffi(py, || {
                _simulate_panic_for_test();
            });
            assert!(res.is_err());
            if let Err(py_err) = res {
                assert!(py_err.is_instance_of::<PyRuntimeError>(py));
                let bound_val = py_err.value(py);
                let msg: String = bound_val.to_string();
                assert!(msg.contains("Rust panic caught at FFI boundary"));
                assert!(msg.contains("Simulated Rust core panic"));
            }
        });
    }

    #[test]
    fn test_run_blocking_ffi_success() {
        pyo3::prepare_freethreaded_python();
        Python::with_gil(|py| {
            let res: PyResult<i32> = run_blocking_ffi(py, || Ok(42));
            assert!(matches!(res, Ok(42)));
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

// AI-TAG[BUG][MAJOR][RESOLVED] _trigger_panic_for_test directly returns PyRuntimeError instead of invoking panic inside run_blocking_ffi (ID: AGT-PY-ff475c8e) (TS: 2026-09-03T19:29:58Z) (SESSION: 94a6a82c)
// RESOLVED: _trigger_panic_for_test now calls run_blocking_ffi internally triggering a panic in the closure to test FFI panic containment.
/// Internal helper function for testing FFI panic isolation.
#[pyfunction]
fn _trigger_panic_for_test(py: Python<'_>, message: Option<String>) -> PyResult<()> {
    let msg = message.unwrap_or_else(|| "Test panic for FFI isolation".to_string());
    run_blocking_ffi(py, move || -> PyResult<()> {
        panic!("{}", msg);
    })
}

#[pymodule]
fn _memfuse(_py: Python<'_>, m: &Bound<'_, pyo3::types::PyModule>) -> PyResult<()> {
    check_subinterpreter_guard(_py)?;
    m.add("__version__", "0.2.0")?;

    // Initialize per-interpreter Tokio runtime state
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

    let runtime = Arc::new(rt);
    let state = PyRuntimeState {
        runtime,
        worker_threads,
    };
    m.add("_runtime_state", Py::new(_py, state)?)?;

    m.add_function(wrap_pyfunction!(open, m)?)?;
    m.add_function(wrap_pyfunction!(_trigger_panic_for_test, m)?)?;
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
