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

/// A document retrieved from MemFuse.
#[pyclass(get_all)]
pub struct PyDocument {
    /// The document ID.
    pub id: String,
    /// Metadata associated with the document.
    pub metadata: Option<PyObject>,
}

#[pyclass(unsendable, name = "Db")]
pub struct PyMemFuse {
    inner: Arc<MemFuse>,
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

    pub fn stats(&self, py: Python<'_>) -> PyResult<PyObject> {
        let rt = get_runtime()?;
        let stats = py
            .allow_threads(|| rt.block_on(self.inner.stats()))
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;

        Ok(pythonize(py, &stats)
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("Stats error: {}", e)))?
            .unbind())
    }
}

#[pyclass(unsendable, name = "Collection")]
pub struct PyCollection {
    inner: Arc<MemFuseCollection>,
}

#[pymethods]
impl PyCollection {
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
            depythonize(&d).map_err(|e| {
                pyo3::exceptions::PyValueError::new_err(format!("Metadata error: {}", e))
            })?
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
            depythonize(&d).map_err(|e| {
                pyo3::exceptions::PyValueError::new_err(format!("Metadata error: {}", e))
            })?
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

    pub fn relate(&self, py: Python<'_>, from: &str, to: &str, label: &str) -> PyResult<()> {
        let rt = get_runtime()?;
        py.allow_threads(|| rt.block_on(self.inner.relate(from, to, label)))
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
        Ok(())
    }

    pub fn scan_prefix(&self, py: Python<'_>, prefix: &str) -> PyResult<Vec<(String, PyObject)>> {
        let rt = get_runtime()?;
        let results = py
            .allow_threads(|| rt.block_on(self.inner.scan_prefix(prefix)))
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;

        let mut py_results = Vec::with_capacity(results.len());
        for (k, v) in results {
            let v_py = pythonize(py, &v).map_err(|e| {
                pyo3::exceptions::PyRuntimeError::new_err(format!("Metadata error: {}", e))
            })?;
            py_results.push((k, v_py.unbind()));
        }
        Ok(py_results)
    }

    pub fn len(&self, py: Python<'_>) -> PyResult<usize> {
        let rt = get_runtime()?;
        Ok(py.allow_threads(|| rt.block_on(self.inner.len())))
    }

    pub fn is_empty(&self, py: Python<'_>) -> PyResult<bool> {
        let rt = get_runtime()?;
        Ok(py.allow_threads(|| rt.block_on(self.inner.is_empty())))
    }

    pub fn stats(&self, py: Python<'_>) -> PyResult<PyObject> {
        let rt = get_runtime()?;
        let stats = py
            .allow_threads(|| rt.block_on(self.inner.stats()))
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;

        Ok(pythonize(py, &stats)
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("Stats error: {}", e)))?
            .unbind())
    }

    #[pyo3(signature = (vector, k))]
    pub fn search<'py>(
        &self,
        py: Python<'py>,
        vector: PyReadonlyArray1<'py, f32>,
        k: usize,
    ) -> PyResult<Vec<PyObject>> {
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
}

#[pyfunction]
#[pyo3(signature = (path, dimension=1536))]
fn open(py: Python<'_>, path: &str, dimension: usize) -> PyResult<PyMemFuse> {
    let rt = get_runtime()?;
    let config = MemFuseConfig {
        dimension,
        ..Default::default()
    };
    let path_string = path.to_string();
    let db = py
        .allow_threads(|| rt.block_on(MemFuse::open_with_config(path_string, config)))
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;

    Ok(PyMemFuse {
        inner: Arc::new(db),
    })
}

#[pymodule]
fn memfuse(_py: Python<'_>, m: &Bound<'_, pyo3::types::PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(open, m)?)?;
    m.add_class::<PyMemFuse>()?;
    m.add_class::<PyCollection>()?;
    m.add_class::<PySearchResult>()?;
    m.add_class::<PyDocument>()?;
    Ok(())
}
