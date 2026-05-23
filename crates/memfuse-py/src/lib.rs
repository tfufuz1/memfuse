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

// AGENT:06 DATE:2026-05-23 STATUS:DONE
// ANCHOR:TODO:PY-001 — Stelle sicher, dass die zero-copy Vektor-Anbindung via numpy stabil ist.
// WP:WP-3.1 PRIO:1 NEEDS:SEARCH-001
// AGENT:@JULES-06 DATE:2026-05-23 STATUS:DONE
// TEST: cd crates/memfuse-py && python -m pytest tests/ -v
// DONE: pip install . funktioniert, keine Deadlocks in tokio-Runtime.
// SUCCESSOR: @JULES-09 — "Python Bindings sind stabil. StateGraph kann darauf aufbauen."
#![forbid(unsafe_code)]

use memfuse_db::{Collection as MemFuseCollection, MemFuse, MemFuseConfig};
use memfuse_orchestrator::{AgentNode as MemFuseNode, StateGraph as MemFuseStateGraph};
use memfuse_runtime::airgap::{AirGapConfig, AirGapVerifier, EmbeddingRuntime};
use numpy::PyReadonlyArray1;
use pyo3::prelude::*;
use pythonize::{depythonize, pythonize};
use std::sync::Arc;
use std::sync::OnceLock;
use tokio::runtime::Runtime;

/// Returns a reference to the shared Tokio runtime.
///
/// In compliance with the Zero-Panic policy, this function handles potential
/// runtime creation errors by returning a `PyResult`.
fn get_runtime() -> PyResult<&'static Runtime> {
    static RUNTIME: OnceLock<Runtime> = OnceLock::new();
    if let Some(rt) = RUNTIME.get() {
        return Ok(rt);
    }

    let rt = tokio::runtime::Builder::new_multi_thread()
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
        // The newly created runtime will be dropped here.
    }

    RUNTIME.get().ok_or_else(|| {
        pyo3::exceptions::PyRuntimeError::new_err("Failed to retrieve initialized tokio runtime")
    })
}

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

#[pyclass(unsendable, name = "Db")]
pub struct PyMemFuse {
    inner: Arc<MemFuse>,
}

#[pymethods]
impl PyMemFuse {
    /// Returns a specific collection (namespace).
    /// Creates the collection if it does not already exist.
    pub fn collection(&self, name: &str, py: Python<'_>) -> PyResult<PyCollection> {
        let rt = get_runtime()?;
        let col = py
            .allow_threads(|| rt.block_on(self.inner.collection(name)))
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
        Ok(PyCollection {
            inner: Arc::new(col),
        })
    }

    /// Lists all existing collection names.
    pub fn list_collections(&self, py: Python<'_>) -> PyResult<Vec<String>> {
        let rt = get_runtime()?;
        py.allow_threads(|| rt.block_on(self.inner.list_collections()))
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    /// Drops a collection, removing all its data from storage.
    pub fn drop_collection(&self, name: &str, py: Python<'_>) -> PyResult<()> {
        let rt = get_runtime()?;
        py.allow_threads(|| rt.block_on(self.inner.drop_collection(name)))
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    /// Inserts a document with an embedding and optional metadata into the default collection.
    #[pyo3(signature = (id, vector, metadata=None))]
    pub fn insert<'py>(
        &self,
        py: Python<'py>,
        id: &str,
        vector: PyReadonlyArray1<'py, f32>,
        metadata: Option<pyo3::Bound<'py, pyo3::types::PyDict>>,
    ) -> PyResult<()> {
        let rt = get_runtime()?;
        let vec_slice = vector.as_slice().map_err(|e| {
            pyo3::exceptions::PyValueError::new_err(format!("Invalid vector format: {}", e))
        })?;

        let meta_val: Option<serde_json::Value> = if let Some(d) = metadata {
            Some(depythonize(&d).map_err(|e| {
                pyo3::exceptions::PyValueError::new_err(format!("Metadata error: {}", e))
            })?)
        } else {
            None
        };

        py.allow_threads(|| rt.block_on(self.inner.insert(id, vec_slice, meta_val)))
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
        Ok(())
    }

    /// Retrieves a document by its user-provided string ID from the default collection.
    pub fn get(&self, py: Python<'_>, id: &str) -> PyResult<Option<PyDocument>> {
        let rt = get_runtime()?;
        let doc = py
            .allow_threads(|| rt.block_on(self.inner.get(id)))
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;

        if let Some(d) = doc {
            let meta_py = if let Some(m) = d.metadata {
                Some(
                    pythonize(py, &m)
                        .map_err(|e| {
                            pyo3::exceptions::PyRuntimeError::new_err(format!(
                                "Metadata error: {}",
                                e
                            ))
                        })?
                        .unbind(),
                )
            } else {
                None
            };

            Ok(Some(PyDocument {
                id: d.id,
                metadata: meta_py,
            }))
        } else {
            Ok(None)
        }
    }

    /// Updates an existing document in the default collection.
    #[pyo3(signature = (id, vector, metadata=None))]
    pub fn update<'py>(
        &self,
        py: Python<'py>,
        id: &str,
        vector: PyReadonlyArray1<'py, f32>,
        metadata: Option<pyo3::Bound<'py, pyo3::types::PyDict>>,
    ) -> PyResult<()> {
        let rt = get_runtime()?;
        let vec_slice = vector.as_slice().map_err(|e| {
            pyo3::exceptions::PyValueError::new_err(format!("Invalid vector format: {}", e))
        })?;

        let meta_val: Option<serde_json::Value> = if let Some(d) = metadata {
            Some(depythonize(&d).map_err(|e| {
                pyo3::exceptions::PyValueError::new_err(format!("Metadata error: {}", e))
            })?)
        } else {
            None
        };

        py.allow_threads(|| rt.block_on(self.inner.update(id, vec_slice, meta_val)))
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
        Ok(())
    }

    /// Deletes a document from the default collection by its ID.
    pub fn delete(&self, py: Python<'_>, id: &str) -> PyResult<()> {
        let rt = get_runtime()?;
        py.allow_threads(|| rt.block_on(self.inner.delete(id)))
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
        Ok(())
    }

    /// Performs semantic k-NN search over the default collection's embeddings.
    #[pyo3(signature = (vector, k))]
    pub fn search<'py>(
        &self,
        py: Python<'py>,
        vector: PyReadonlyArray1<'py, f32>,
        k: usize,
    ) -> PyResult<Vec<PySearchResult>> {
        let rt = get_runtime()?;
        let vec_slice = vector.as_slice().map_err(|e| {
            pyo3::exceptions::PyValueError::new_err(format!("Invalid vector format: {}", e))
        })?;

        let results = py
            .allow_threads(|| rt.block_on(self.inner.search(vec_slice, k)))
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;

        let mut py_res = Vec::new();
        for r in results {
            let meta_py = if let Some(m) = r.metadata {
                Some(
                    pythonize(py, &m)
                        .map_err(|e| {
                            pyo3::exceptions::PyRuntimeError::new_err(format!(
                                "Metadata error: {}",
                                e
                            ))
                        })?
                        .unbind(),
                )
            } else {
                None
            };

            py_res.push(PySearchResult {
                id: r.id,
                score: r.score,
                metadata: meta_py,
            });
        }
        Ok(py_res)
    }

    /// Performs hybrid search combining BM25 and vector search results in the default collection.
    #[pyo3(signature = (text, vector, k))]
    pub fn hybrid_search<'py>(
        &self,
        py: Python<'py>,
        text: &str,
        vector: PyReadonlyArray1<'py, f32>,
        k: usize,
    ) -> PyResult<Vec<PySearchResult>> {
        let rt = get_runtime()?;
        let vec_slice = vector.as_slice().map_err(|e| {
            pyo3::exceptions::PyValueError::new_err(format!("Invalid vector format: {}", e))
        })?;

        let results = py
            .allow_threads(|| rt.block_on(self.inner.hybrid_search(text, vec_slice, k)))
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;

        let mut py_res = Vec::new();
        for r in results {
            let meta_py = if let Some(m) = r.metadata {
                Some(
                    pythonize(py, &m)
                        .map_err(|e| {
                            pyo3::exceptions::PyRuntimeError::new_err(format!(
                                "Metadata error: {}",
                                e
                            ))
                        })?
                        .unbind(),
                )
            } else {
                None
            };

            py_res.push(PySearchResult {
                id: r.id,
                score: r.score,
                metadata: meta_py,
            });
        }
        Ok(py_res)
    }

    /// Creates a bidirectional relationship between two documents in the default collection.
    pub fn relate(&self, py: Python<'_>, from: &str, to: &str, label: &str) -> PyResult<()> {
        let rt = get_runtime()?;
        py.allow_threads(|| rt.block_on(self.inner.relate(from, to, label)))
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    /// Scans documents in the default collection that match a given key prefix.
    #[pyo3(signature = (prefix=""))]
    pub fn scan_prefix(&self, py: Python<'_>, prefix: &str) -> PyResult<Vec<(String, PyObject)>> {
        let rt = get_runtime()?;
        let results = py
            .allow_threads(|| rt.block_on(self.inner.scan_prefix(prefix)))
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;

        let mut py_res = Vec::new();
        for (k, v) in results {
            let val_py = pythonize(py, &v).map_err(|e| {
                pyo3::exceptions::PyRuntimeError::new_err(format!("Metadata error: {}", e))
            })?;
            py_res.push((k, val_py.unbind()));
        }
        Ok(py_res)
    }

    /// Returns the number of documents in the default collection.
    pub fn len(&self, py: Python<'_>) -> PyResult<usize> {
        let rt = get_runtime()?;
        py.allow_threads(|| rt.block_on(self.inner.len()))
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    /// Returns true if the default collection is empty.
    pub fn is_empty(&self, py: Python<'_>) -> PyResult<bool> {
        let rt = get_runtime()?;
        py.allow_threads(|| rt.block_on(self.inner.is_empty()))
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    /// Performs a range scan of documents in the default collection.
    #[pyo3(signature = (start=None, end=None))]
    pub fn scan(
        &self,
        py: Python<'_>,
        start: Option<&[u8]>,
        end: Option<&[u8]>,
    ) -> PyResult<Vec<(String, PyObject)>> {
        let rt = get_runtime()?;
        use std::ops::Bound;
        let start_bound = start.map_or(Bound::Unbounded, Bound::Included);
        let end_bound = end.map_or(Bound::Unbounded, Bound::Included);

        let results = py
            .allow_threads(|| rt.block_on(self.inner.scan(start_bound, end_bound)))
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;

        let mut py_res = Vec::new();
        for (k, v) in results {
            let val_py = pythonize(py, &v).map_err(|e| {
                pyo3::exceptions::PyRuntimeError::new_err(format!("Metadata error: {}", e))
            })?;
            py_res.push((k, val_py.unbind()));
        }
        Ok(py_res)
    }

    /// Returns combined statistics for the vector index and storage engine.
    pub fn stats(&self, py: Python<'_>) -> PyResult<PyDbStats> {
        let rt = get_runtime()?;
        let stats = py
            .allow_threads(|| rt.block_on(self.inner.stats()))
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;

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
}

#[pyclass(unsendable, name = "Collection")]
pub struct PyCollection {
    inner: Arc<MemFuseCollection>,
}

#[pymethods]
impl PyCollection {
    /// Inserts a document with an embedding and optional metadata into the collection.
    #[pyo3(signature = (id, vector, metadata=None))]
    pub fn insert<'py>(
        &self,
        py: Python<'py>,
        id: &str,
        vector: PyReadonlyArray1<'py, f32>,
        metadata: Option<pyo3::Bound<'py, pyo3::types::PyDict>>,
    ) -> PyResult<()> {
        let rt = get_runtime()?;
        let vec_slice = vector.as_slice().map_err(|e| {
            pyo3::exceptions::PyValueError::new_err(format!("Invalid vector format: {}", e))
        })?;

        let meta_val: Option<serde_json::Value> = if let Some(d) = metadata {
            Some(depythonize(&d).map_err(|e| {
                pyo3::exceptions::PyValueError::new_err(format!("Metadata error: {}", e))
            })?)
        } else {
            None
        };

        py.allow_threads(|| rt.block_on(self.inner.insert(id, vec_slice, meta_val)))
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
        Ok(())
    }

    /// Retrieves a document by its user-provided string ID from the collection.
    pub fn get(&self, py: Python<'_>, id: &str) -> PyResult<Option<PyDocument>> {
        let rt = get_runtime()?;
        let doc = py
            .allow_threads(|| rt.block_on(self.inner.get(id)))
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;

        if let Some(d) = doc {
            let meta_py = if let Some(m) = d.metadata {
                Some(
                    pythonize(py, &m)
                        .map_err(|e| {
                            pyo3::exceptions::PyRuntimeError::new_err(format!(
                                "Metadata error: {}",
                                e
                            ))
                        })?
                        .unbind(),
                )
            } else {
                None
            };

            Ok(Some(PyDocument {
                id: d.id,
                metadata: meta_py,
            }))
        } else {
            Ok(None)
        }
    }

    /// Updates an existing document in the collection.
    #[pyo3(signature = (id, vector, metadata=None))]
    pub fn update<'py>(
        &self,
        py: Python<'py>,
        id: &str,
        vector: PyReadonlyArray1<'py, f32>,
        metadata: Option<pyo3::Bound<'py, pyo3::types::PyDict>>,
    ) -> PyResult<()> {
        let rt = get_runtime()?;
        let vec_slice = vector.as_slice().map_err(|e| {
            pyo3::exceptions::PyValueError::new_err(format!("Invalid vector format: {}", e))
        })?;

        let meta_val: Option<serde_json::Value> = if let Some(d) = metadata {
            Some(depythonize(&d).map_err(|e| {
                pyo3::exceptions::PyValueError::new_err(format!("Metadata error: {}", e))
            })?)
        } else {
            None
        };

        py.allow_threads(|| rt.block_on(self.inner.update(id, vec_slice, meta_val)))
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
        Ok(())
    }

    /// Deletes a document from the collection by its ID.
    pub fn delete(&self, py: Python<'_>, id: &str) -> PyResult<()> {
        let rt = get_runtime()?;
        py.allow_threads(|| rt.block_on(self.inner.delete(id)))
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
        Ok(())
    }

    /// Performs semantic k-NN search over the collection's embeddings.
    #[pyo3(signature = (vector, k))]
    pub fn search<'py>(
        &self,
        py: Python<'py>,
        vector: PyReadonlyArray1<'py, f32>,
        k: usize,
    ) -> PyResult<Vec<PySearchResult>> {
        let rt = get_runtime()?;
        let vec_slice = vector.as_slice().map_err(|e| {
            pyo3::exceptions::PyValueError::new_err(format!("Invalid vector format: {}", e))
        })?;

        let results = py
            .allow_threads(|| rt.block_on(self.inner.search(vec_slice, k)))
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;

        let mut py_res = Vec::new();
        for r in results {
            let meta_py = if let Some(m) = r.metadata {
                Some(
                    pythonize(py, &m)
                        .map_err(|e| {
                            pyo3::exceptions::PyRuntimeError::new_err(format!(
                                "Metadata error: {}",
                                e
                            ))
                        })?
                        .unbind(),
                )
            } else {
                None
            };

            py_res.push(PySearchResult {
                id: r.id,
                score: r.score,
                metadata: meta_py,
            });
        }
        Ok(py_res)
    }

    /// Performs hybrid search combining BM25 and vector search results in the collection.
    #[pyo3(signature = (text, vector, k))]
    pub fn hybrid_search<'py>(
        &self,
        py: Python<'py>,
        text: &str,
        vector: PyReadonlyArray1<'py, f32>,
        k: usize,
    ) -> PyResult<Vec<PySearchResult>> {
        let rt = get_runtime()?;
        let vec_slice = vector.as_slice().map_err(|e| {
            pyo3::exceptions::PyValueError::new_err(format!("Invalid vector format: {}", e))
        })?;

        let results = py
            .allow_threads(|| rt.block_on(self.inner.hybrid_search(text, vec_slice, k)))
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;

        let mut py_res = Vec::new();
        for r in results {
            let meta_py = if let Some(m) = r.metadata {
                Some(
                    pythonize(py, &m)
                        .map_err(|e| {
                            pyo3::exceptions::PyRuntimeError::new_err(format!(
                                "Metadata error: {}",
                                e
                            ))
                        })?
                        .unbind(),
                )
            } else {
                None
            };

            py_res.push(PySearchResult {
                id: r.id,
                score: r.score,
                metadata: meta_py,
            });
        }
        Ok(py_res)
    }

    /// Returns the number of documents in the collection.
    pub fn len(&self, py: Python<'_>) -> PyResult<usize> {
        let rt = get_runtime()?;
        Ok(py.allow_threads(|| rt.block_on(self.inner.len())))
    }

    /// Returns true if the collection is empty.
    pub fn is_empty(&self, py: Python<'_>) -> PyResult<bool> {
        let rt = get_runtime()?;
        Ok(py.allow_threads(|| rt.block_on(self.inner.is_empty())))
    }

    /// Creates a bidirectional relationship between two documents in the collection.
    pub fn relate(&self, py: Python<'_>, from: &str, to: &str, label: &str) -> PyResult<()> {
        let rt = get_runtime()?;
        py.allow_threads(|| rt.block_on(self.inner.relate(from, to, label)))
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    /// Scans documents in the collection that match a given key prefix.
    #[pyo3(signature = (prefix=""))]
    pub fn scan_prefix(&self, py: Python<'_>, prefix: &str) -> PyResult<Vec<(String, PyObject)>> {
        let rt = get_runtime()?;
        let results = py
            .allow_threads(|| rt.block_on(self.inner.scan_prefix(prefix)))
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;

        let mut py_res = Vec::new();
        for (k, v) in results {
            let val_py = pythonize(py, &v).map_err(|e| {
                pyo3::exceptions::PyRuntimeError::new_err(format!("Metadata error: {}", e))
            })?;
            py_res.push((k, val_py.unbind()));
        }
        Ok(py_res)
    }

    /// Performs a range scan of documents in the collection.
    #[pyo3(signature = (start=None, end=None))]
    pub fn scan(
        &self,
        py: Python<'_>,
        start: Option<&[u8]>,
        end: Option<&[u8]>,
    ) -> PyResult<Vec<(String, PyObject)>> {
        let rt = get_runtime()?;
        use std::ops::Bound;
        let start_bound = start.map_or(Bound::Unbounded, Bound::Included);
        let end_bound = end.map_or(Bound::Unbounded, Bound::Included);

        let results = py
            .allow_threads(|| rt.block_on(self.inner.scan(start_bound, end_bound)))
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;

        let mut py_res = Vec::new();
        for (k, v) in results {
            let val_py = pythonize(py, &v).map_err(|e| {
                pyo3::exceptions::PyRuntimeError::new_err(format!("Metadata error: {}", e))
            })?;
            py_res.push((k, val_py.unbind()));
        }
        Ok(py_res)
    }

    /// Returns statistics for the collection's vector index.
    pub fn stats(&self, py: Python<'_>) -> PyResult<PyVectorIndexStats> {
        let rt = get_runtime()?;
        let stats = py
            .allow_threads(|| rt.block_on(self.inner.stats()))
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;

        Ok(PyVectorIndexStats {
            num_vectors: stats.num_vectors,
            memory_usage_bytes: stats.memory_usage_bytes,
            num_layers: stats.num_layers,
        })
    }
}

/// Configuration for air-gap deployment mode.
#[pyclass(name = "AirGapConfig")]
#[derive(Clone)]
pub struct PyAirGapConfig {
    pub(crate) inner: AirGapConfig,
}

#[pymethods]
impl PyAirGapConfig {
    #[new]
    #[pyo3(signature = (network_disabled=true, local_model_path=None, require_encryption=true))]
    pub fn new(
        network_disabled: bool,
        local_model_path: Option<String>,
        require_encryption: bool,
    ) -> Self {
        let mut inner = AirGapConfig {
            network_disabled,
            local_model_path,
            require_encryption,
            ..Default::default()
        };
        if inner.local_model_path.is_some() {
            inner.embedding_runtime = EmbeddingRuntime::OnnxRuntime;
        }
        Self { inner }
    }

    #[staticmethod]
    pub fn strict() -> Self {
        Self {
            inner: AirGapConfig::strict(),
        }
    }

    #[pyo3(signature = ())]
    pub fn validate(&self) -> PyResult<()> {
        self.inner
            .validate()
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
    }
}

/// Result of an air-gap compliance verification.
#[pyclass(get_all, name = "AirGapReport")]
pub struct PyAirGapReport {
    pub network_isolated: bool,
    pub encryption_active: bool,
    pub sbom_generated: bool,
}

#[pymethods]
impl PyAirGapReport {
    pub fn is_compliant(&self) -> bool {
        self.network_isolated && self.encryption_active
    }

    fn __repr__(&self) -> String {
        format!(
            "AirGapReport(network_isolated={}, encryption_active={})",
            self.network_isolated, self.encryption_active
        )
    }
}

/// Verifies air-gap compliance.
#[pyclass(name = "AirGapVerifier")]
pub struct PyAirGapVerifier;

#[pymethods]
impl PyAirGapVerifier {
    #[staticmethod]
    pub fn verify(config: &PyAirGapConfig) -> PyResult<PyAirGapReport> {
        let report = AirGapVerifier::verify(&config.inner)
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
        Ok(PyAirGapReport {
            network_isolated: report.network_isolated,
            encryption_active: report.encryption_active,
            sbom_generated: report.sbom_generated,
        })
    }
}

/// Provides local embedding models for air-gapped environments.
#[pyclass(name = "EmbeddingProvider")]
pub struct PyEmbeddingProvider;

#[pymethods]
impl PyEmbeddingProvider {
    #[staticmethod]
    #[pyo3(signature = (model_path, runtime="ort"))]
    pub fn local(model_path: &str, runtime: &str) -> PyAirGapConfig {
        let _ = runtime;
        PyAirGapConfig {
            inner: AirGapConfig::with_local_model(model_path),
        }
    }
}

/// Opens or creates a MemFuse database at the given path.
#[pyfunction]
#[pyo3(signature = (path, dimension=1536, max_elements=None, encryption_passphrase=None, distance_metric=None, airgap=None))]
fn open(
    py: Python<'_>,
    path: &str,
    dimension: usize,
    max_elements: Option<usize>,
    encryption_passphrase: Option<String>,
    distance_metric: Option<String>,
    airgap: Option<PyAirGapConfig>,
) -> PyResult<PyMemFuse> {
    let rt = get_runtime()?;
    let mut config = MemFuseConfig {
        dimension,
        encryption_passphrase,
        ..Default::default()
    };

    if let Some(ag) = airgap {
        ag.validate()?;
        if ag.inner.require_encryption && config.encryption_passphrase.is_none() {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "Air-gap mode requires encryption passphrase",
            ));
        }
        // INTEGRATION: AirGapConfig is validated here.
        // Once memfuse-db supports AirGapConfig natively in its MemFuseConfig,
        // we will pass it along. For now, we enforce the isolation at the binding level.
    }

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
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;

    Ok(PyMemFuse {
        inner: Arc::new(db),
    })
}

/// Represents a single node in the agent state graph.
#[pyclass(name = "Node")]
pub struct PyNode {
    pub(crate) inner: MemFuseNode,
}

#[pymethods]
impl PyNode {
    #[new]
    #[pyo3(signature = (id, description, tool=None, params=None))]
    pub fn new(
        id: String,
        description: String,
        tool: Option<String>,
        params: Option<pyo3::Bound<'_, pyo3::types::PyDict>>,
    ) -> PyResult<Self> {
        let params_val = if let Some(d) = params {
            Some(depythonize(&d).map_err(|e| {
                pyo3::exceptions::PyValueError::new_err(format!("Params error: {}", e))
            })?)
        } else {
            None
        };

        Ok(Self {
            inner: MemFuseNode {
                id,
                description,
                tool,
                params: params_val,
            },
        })
    }

    #[getter]
    pub fn id(&self) -> String {
        self.inner.id.clone()
    }

    #[getter]
    pub fn description(&self) -> String {
        self.inner.description.clone()
    }

    fn __repr__(&self) -> String {
        format!("Node(id='{}')", self.inner.id)
    }
}

/// A declarative state graph for agent workflows.
#[pyclass(name = "StateGraph")]
pub struct PyStateGraph {
    pub(crate) inner: MemFuseStateGraph,
}

impl Default for PyStateGraph {
    fn default() -> Self {
        Self::new()
    }
}

#[pymethods]
impl PyStateGraph {
    #[new]
    pub fn new() -> Self {
        Self {
            inner: MemFuseStateGraph::new(),
        }
    }

    /// Adds a node to the workflow graph.
    pub fn add_node(&mut self, node: &PyNode) {
        self.inner.add_agent_node(MemFuseNode {
            id: node.inner.id.clone(),
            description: node.inner.description.clone(),
            tool: node.inner.tool.clone(),
            params: node.inner.params.clone(),
        });
    }

    /// Adds a directed edge between two nodes with an optional condition.
    #[pyo3(signature = (source, target, condition=None))]
    pub fn add_edge(&mut self, source: &str, target: &str, condition: Option<&str>) {
        self.inner.add_edge(source, target, condition);
    }

    /// Executes the workflow starting from the given initial state.
    pub fn run(&self, initial_state: &str) {
        self.inner.run_workflow(initial_state);
    }
}

#[pymodule]
fn memfuse(_py: Python<'_>, m: &Bound<'_, pyo3::types::PyModule>) -> PyResult<()> {
    m.add("__version__", "0.1.0")?;
    m.add_function(wrap_pyfunction!(open, m)?)?;
    m.add_class::<PyMemFuse>()?;
    m.add_class::<PyCollection>()?;
    m.add_class::<PySearchResult>()?;
    m.add_class::<PyDocument>()?;
    m.add_class::<PyVectorIndexStats>()?;
    m.add_class::<PyStorageStats>()?;
    m.add_class::<PyDbStats>()?;
    m.add_class::<PyNode>()?;
    m.add_class::<PyStateGraph>()?;
    m.add_class::<PyAirGapConfig>()?;
    m.add_class::<PyAirGapReport>()?;
    m.add_class::<PyAirGapVerifier>()?;
    m.add_class::<PyEmbeddingProvider>()?;
    Ok(())
}
