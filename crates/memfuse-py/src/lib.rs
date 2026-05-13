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

// AGENT:06 DATE:2026-05-13 STATUS:DONE
// ANCHOR:DONE:PY-001 — Stelle sicher, dass die zero-copy Vektor-Anbindung via numpy stabil ist.
// WP:WP-3.1 PRIO:1 NEEDS:SEARCH-001
// AGENT:@JULES-06 DATE:2026-05-09 STATUS:DONE
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

    // AGENT:06 DATE:2026-05-13 STATUS:DONE — Fixed DEBT-UNWRAP-LIB-25 and Zero-Panic compliant
    let _ = RUNTIME.set(rt);
    RUNTIME.get().ok_or_else(|| {
        pyo3::exceptions::PyRuntimeError::new_err("Failed to retrieve initialized runtime")
    })
}

/// A single search result from MemFuse.
#[pyclass(name = "SearchResult", get_all)]
pub struct PySearchResult {
    /// The document ID.
    pub id: String,
    /// Similarity score (higher = more similar).
    pub score: f32,
    /// Metadata associated with the document.
    pub metadata: Option<PyObject>,
}

/// A document retrieved from MemFuse.
#[pyclass(name = "Document", get_all)]
pub struct PyDocument {
    /// The document ID.
    pub id: String,
    /// Metadata associated with the document.
    pub metadata: Option<PyObject>,
}

#[pyclass(name = "Db", unsendable)]
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
}

#[pyclass(name = "Collection", unsendable)]
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

        // AGENT:06 DATE:2026-05-13 STATUS:DONE — Zero-copy slice passed to backend
        py.allow_threads(|| rt.block_on(self.inner.insert(id, vec_slice, meta_val)))
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
        Ok(())
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

        // AGENT:06 DATE:2026-05-13 STATUS:DONE — Zero-copy slice passed to backend
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

        // AGENT:06 DATE:2026-05-13 STATUS:DONE — Zero-copy slice passed to backend
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
