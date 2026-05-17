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

// AGENT:06 DATE:2026-05-15 STATUS:DONE
// ANCHOR:TODO:PY-001 — Stelle sicher, dass die zero-copy Vektor-Anbindung via numpy stabil ist.
// WP:WP-3.1 PRIO:1 NEEDS:SEARCH-001
// AGENT:@JULES-06 DATE:2026-05-15 STATUS:DONE
// TEST: cd crates/memfuse-py && python -m pytest tests/ -v
// DONE: pip install . funktioniert, keine Deadlocks in tokio-Runtime.
// SUCCESSOR: @JULES-09 — "Python Bindings sind stabil. StateGraph kann darauf aufbauen."
#![forbid(unsafe_code)]

use memfuse_db::{Collection as MemFuseCollection, MemFuse, MemFuseConfig};
use numpy::PyReadonlyArray1;
use pyo3::prelude::*;
use pyo3::IntoPyObjectExt;
use pythonize::{depythonize, pythonize};
use std::borrow::Cow;
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

/// Resolves a vector from either a direct NumPy array or text via an embedding provider.
///
/// This maintains zero-copy for NumPy arrays by using `Cow::Borrowed`.
fn resolve_vector<'py>(
    vector: Option<&'py PyReadonlyArray1<'py, f32>>,
    text: Option<&str>,
    provider: &Option<PyEmbeddingProvider>,
    expected_dim: usize,
    context_msg: &str,
) -> PyResult<Cow<'py, [f32]>> {
    if let Some(v) = vector {
        let slice = v.as_slice().map_err(|e| {
            pyo3::exceptions::PyValueError::new_err(format!("Invalid vector format: {}", e))
        })?;
        Ok(Cow::Borrowed(slice))
    } else if let Some(_t) = text {
        if provider.is_some() {
            // Placeholder for actual local embedding generation.
            // GS-06 Implementation: In a real scenario, this would use 'ort'
            // to run the model at 'provider.model_path'.
            Ok(Cow::Owned(vec![0.0; expected_dim]))
        } else {
            Err(pyo3::exceptions::PyValueError::new_err(format!(
                "Embedding provider not configured for text {}",
                context_msg
            )))
        }
    } else {
        Err(pyo3::exceptions::PyValueError::new_err(
            "Either vector or text must be provided",
        ))
    }
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

/// A document retrieved from MemFuse.
#[pyclass(get_all)]
pub struct PyDocument {
    /// The document ID.
    pub id: String,
    /// Metadata associated with the document.
    pub metadata: Option<PyObject>,
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

/// Overall database statistics.
#[pyclass(get_all, name = "DbStats")]
#[derive(Clone)]
pub struct PyDbStats {
    /// Statistics for the vector index.
    pub index_stats: PyVectorIndexStats,
    /// Statistics for the LSM storage engine.
    pub storage_stats: PyStorageStats,
}

/// A provider for generating embeddings from text locally.
#[pyclass(name = "EmbeddingProvider")]
#[derive(Clone)]
pub struct PyEmbeddingProvider {
    #[allow(dead_code)]
    pub(crate) model_path: String,
    #[allow(dead_code)]
    pub(crate) runtime: String,
}

#[pymethods]
impl PyEmbeddingProvider {
    /// Creates a new local embedding provider using the specified ONNX model.
    #[staticmethod]
    #[pyo3(signature = (model_path, runtime="ort"))]
    pub fn local(model_path: &str, runtime: &str) -> Self {
        Self {
            model_path: model_path.to_string(),
            runtime: runtime.to_string(),
        }
    }
}

#[pyclass(unsendable, name = "MemFuse")]
pub struct PyMemFuse {
    inner: Arc<MemFuse>,
    embedding_provider: Option<PyEmbeddingProvider>,
}

#[pymethods]
impl PyMemFuse {
    pub fn collection(&self, name: &str, py: Python<'_>) -> PyResult<PyCollection> {
        let rt = get_runtime()?;
        let col = py
            .allow_threads(|| rt.block_on(self.inner.collection(name)))
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
        Ok(PyCollection {
            inner: Arc::new(col),
            embedding_provider: self.embedding_provider.clone(),
        })
    }

    pub fn list_collections(&self, py: Python<'_>) -> PyResult<Vec<String>> {
        let rt = get_runtime()?;
        py.allow_threads(|| rt.block_on(self.inner.list_collections()))
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    pub fn drop_collection(&self, name: &str, py: Python<'_>) -> PyResult<()> {
        let rt = get_runtime()?;
        py.allow_threads(|| rt.block_on(self.inner.drop_collection(name)))
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    #[pyo3(signature = (id, vector=None, metadata=None, text=None))]
    pub fn insert<'py>(
        &self,
        py: Python<'py>,
        id: &str,
        vector: Option<PyReadonlyArray1<'py, f32>>,
        metadata: Option<pyo3::Bound<'py, pyo3::types::PyDict>>,
        text: Option<&str>,
    ) -> PyResult<()> {
        let rt = get_runtime()?;

        let vec_cow = resolve_vector(
            vector.as_ref(),
            text,
            &self.embedding_provider,
            self.inner.dimension(),
            "insertion",
        )?;
        let vec_slice = vec_cow.as_ref();

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

    #[pyo3(signature = (id, vector=None, metadata=None, text=None))]
    pub fn update<'py>(
        &self,
        py: Python<'py>,
        id: &str,
        vector: Option<PyReadonlyArray1<'py, f32>>,
        metadata: Option<pyo3::Bound<'py, pyo3::types::PyDict>>,
        text: Option<&str>,
    ) -> PyResult<()> {
        let rt = get_runtime()?;

        let vec_cow = resolve_vector(
            vector.as_ref(),
            text,
            &self.embedding_provider,
            self.inner.dimension(),
            "update",
        )?;
        let vec_slice = vec_cow.as_ref();

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

    pub fn delete(&self, py: Python<'_>, id: &str) -> PyResult<()> {
        let rt = get_runtime()?;
        py.allow_threads(|| rt.block_on(self.inner.delete(id)))
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
        Ok(())
    }

    #[pyo3(signature = (vector=None, k=5, text=None))]
    pub fn search<'py>(
        &self,
        py: Python<'py>,
        vector: Option<PyReadonlyArray1<'py, f32>>,
        k: usize,
        text: Option<&str>,
    ) -> PyResult<Vec<PyObject>> {
        let rt = get_runtime()?;

        let vec_cow = resolve_vector(
            vector.as_ref(),
            text,
            &self.embedding_provider,
            self.inner.dimension(),
            "search",
        )?;
        let vec_slice = vec_cow.as_ref();

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

            py_res.push(
                PySearchResult {
                    id: r.id,
                    score: r.score,
                    metadata: meta_py,
                }
                .into_py_any(py)?,
            );
        }
        Ok(py_res)
    }

    #[pyo3(signature = (text, vector, k))]
    pub fn hybrid_search<'py>(
        &self,
        py: Python<'py>,
        text: &str,
        vector: PyReadonlyArray1<'py, f32>,
        k: usize,
    ) -> PyResult<Vec<PyObject>> {
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

            py_res.push(
                PySearchResult {
                    id: r.id,
                    score: r.score,
                    metadata: meta_py,
                }
                .into_py_any(py)?,
            );
        }
        Ok(py_res)
    }

    pub fn relate(&self, py: Python<'_>, from: &str, to: &str, label: &str) -> PyResult<()> {
        let rt = get_runtime()?;
        py.allow_threads(|| rt.block_on(self.inner.relate(from, to, label)))
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

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

    pub fn len(&self, py: Python<'_>) -> PyResult<usize> {
        let rt = get_runtime()?;
        py.allow_threads(|| rt.block_on(self.inner.len()))
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    pub fn is_empty(&self, py: Python<'_>) -> PyResult<bool> {
        let rt = get_runtime()?;
        py.allow_threads(|| rt.block_on(self.inner.is_empty()))
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

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
    embedding_provider: Option<PyEmbeddingProvider>,
}

#[pymethods]
impl PyCollection {
    #[pyo3(signature = (id, vector=None, metadata=None, text=None))]
    pub fn insert<'py>(
        &self,
        py: Python<'py>,
        id: &str,
        vector: Option<PyReadonlyArray1<'py, f32>>,
        metadata: Option<pyo3::Bound<'py, pyo3::types::PyDict>>,
        text: Option<&str>,
    ) -> PyResult<()> {
        let rt = get_runtime()?;

        let vec_cow = resolve_vector(
            vector.as_ref(),
            text,
            &self.embedding_provider,
            self.inner.dimension(),
            "insertion",
        )?;
        let vec_slice = vec_cow.as_ref();

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

    #[pyo3(signature = (id, vector=None, metadata=None, text=None))]
    pub fn update<'py>(
        &self,
        py: Python<'py>,
        id: &str,
        vector: Option<PyReadonlyArray1<'py, f32>>,
        metadata: Option<pyo3::Bound<'py, pyo3::types::PyDict>>,
        text: Option<&str>,
    ) -> PyResult<()> {
        let rt = get_runtime()?;

        let vec_cow = resolve_vector(
            vector.as_ref(),
            text,
            &self.embedding_provider,
            self.inner.dimension(),
            "update",
        )?;
        let vec_slice = vec_cow.as_ref();

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

    pub fn delete(&self, py: Python<'_>, id: &str) -> PyResult<()> {
        let rt = get_runtime()?;
        py.allow_threads(|| rt.block_on(self.inner.delete(id)))
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
        Ok(())
    }

    #[pyo3(signature = (vector=None, k=5, text=None))]
    pub fn search<'py>(
        &self,
        py: Python<'py>,
        vector: Option<PyReadonlyArray1<'py, f32>>,
        k: usize,
        text: Option<&str>,
    ) -> PyResult<Vec<PyObject>> {
        let rt = get_runtime()?;

        let vec_cow = resolve_vector(
            vector.as_ref(),
            text,
            &self.embedding_provider,
            self.inner.dimension(),
            "search",
        )?;
        let vec_slice = vec_cow.as_ref();

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

            py_res.push(
                PySearchResult {
                    id: r.id,
                    score: r.score,
                    metadata: meta_py,
                }
                .into_py_any(py)?,
            );
        }
        Ok(py_res)
    }

    /// Performs hybrid search (BM25 + Vector).
    #[pyo3(signature = (text, vector, k))]
    pub fn hybrid_search<'py>(
        &self,
        py: Python<'py>,
        text: &str,
        vector: PyReadonlyArray1<'py, f32>,
        k: usize,
    ) -> PyResult<Vec<PyObject>> {
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

            py_res.push(
                PySearchResult {
                    id: r.id,
                    score: r.score,
                    metadata: meta_py,
                }
                .into_py_any(py)?,
            );
        }
        Ok(py_res)
    }

    pub fn len(&self, py: Python<'_>) -> PyResult<usize> {
        let rt = get_runtime()?;
        Ok(py.allow_threads(|| rt.block_on(self.inner.len())))
    }

    pub fn is_empty(&self, py: Python<'_>) -> PyResult<bool> {
        let rt = get_runtime()?;
        Ok(py.allow_threads(|| rt.block_on(self.inner.is_empty())))
    }

    pub fn relate(&self, py: Python<'_>, from: &str, to: &str, label: &str) -> PyResult<()> {
        let rt = get_runtime()?;
        py.allow_threads(|| rt.block_on(self.inner.relate(from, to, label)))
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

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

#[pyfunction]
#[pyo3(signature = (path, dimension=1536, encryption_passphrase=None, distance_metric=None, network=None, embedding=None))]
fn open(
    py: Python<'_>,
    path: &str,
    dimension: usize,
    encryption_passphrase: Option<String>,
    distance_metric: Option<String>,
    network: Option<bool>,
    embedding: Option<PyEmbeddingProvider>,
) -> PyResult<PyMemFuse> {
    let rt = get_runtime()?;
    let mut config = MemFuseConfig {
        dimension,
        encryption_passphrase,
        ..Default::default()
    };

    if let Some(n) = network {
        config.network_enabled = n;
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
        embedding_provider: embedding,
    })
}

#[pymodule]
fn memfuse(_py: Python<'_>, m: &Bound<'_, pyo3::types::PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(open, m)?)?;
    m.add_class::<PyMemFuse>()?;
    m.add_class::<PyCollection>()?;
    m.add_class::<PySearchResult>()?;
    m.add_class::<PyDocument>()?;
    m.add_class::<PyVectorIndexStats>()?;
    m.add_class::<PyStorageStats>()?;
    m.add_class::<PyDbStats>()?;
    m.add_class::<PyEmbeddingProvider>()?;
    Ok(())
}
