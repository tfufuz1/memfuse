//! # MemFuse Python Bindings
//!
//! This crate provides Python bindings for MemFuse, a high-performance,
//! embedded hybrid-search database for AI agents.
//!
//! ## Features
//! - **Zero-copy vector support**: Direct integration with NumPy arrays.
//! - **Async core**: High-performance Rust core exposed via a blocking Python API.
//! - **Hybrid search**: Combines semantic vector search with keyword-based search.
//! - **Collection isolation**: Logical separation of data within the same database.

// ANCHOR:DOC:DOC-LIB-001 — Missing module documentation
// WP:WP-0.0 PRIO:3 NEEDS:NONE
// AGENT:06 DATE:2026-05-09 STATUS:DONE
// CREATED:2026-05-09 DEADLINE:NONE
// ANCHOR:TODO:PY-001 — Stelle sicher, dass die zero-copy Vektor-Anbindung via numpy stabil ist.
// WP:WP-3.1 PRIO:1 NEEDS:SEARCH-001
// AGENT:@JULES-06 DATE:2026-05-09 STATUS:DONE
// TEST: cd crates/memfuse-py && python -m pytest tests/ -v
// DONE: pip install . funktioniert, keine Deadlocks in tokio-Runtime. Zero-copy NumPy integration implemented.
// SUCCESSOR: @JULES-09 — "Python Bindings sind stabil. StateGraph kann darauf aufbauen."
#![forbid(unsafe_code)]

use memfuse_db::{Collection as MemFuseCollection, MemFuse, MemFuseConfig};
use numpy::PyReadonlyArray1;
use pyo3::exceptions::PyKeyError;
use pyo3::prelude::*;
use pyo3::IntoPyObjectExt;
use std::sync::Arc;
use std::sync::OnceLock;
use tokio::runtime::Runtime;

fn runtime() -> &'static Runtime {
    static RUNTIME: OnceLock<Runtime> = OnceLock::new();
    RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            // ANCHOR:DEBT:DEBT-UNWRAP-LIB-25 — unwrap/expect in production code
            // WP:WP-0.0 PRIO:2 NEEDS:NONE
            // AGENT:06 DATE:2026-05-09 STATUS:DONE
            // CREATED:2026-05-09 DEADLINE:NONE
            .expect("CRITICAL: Failed to create tokio runtime for memfuse-py. This usually indicates system resource exhaustion.")
    })
}

#[pyclass(name = "PyMemFuse", unsendable)]
pub struct Db {
    inner: Arc<MemFuse>,
}

#[pymethods]
impl Db {
    pub fn collection(&self, name: &str, py: Python<'_>) -> PyResult<Collection> {
        let col = py
            .allow_threads(|| runtime().block_on(self.inner.collection(name)))
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
        Ok(Collection {
            inner: Arc::new(col),
        })
    }
}

#[pyclass(get_all)]
pub struct PySearchResult {
    pub id: String,
    pub score: f32,
    pub metadata: Option<PyObject>,
}

#[pymethods]
impl PySearchResult {
    fn __getitem__(&self, key: &str, py: Python<'_>) -> PyResult<PyObject> {
        match key {
            "id" => Ok(self.id.clone().into_py_any(py)?),
            "score" => Ok(self.score.into_py_any(py)?),
            "metadata" => Ok(self.metadata.as_ref().map(|m| m.clone_ref(py)).unwrap_or_else(|| py.None())),
            _ => Err(PyKeyError::new_err(key.to_string())),
        }
    }

    fn __repr__(&self) -> String {
        format!(
            "PySearchResult(id='{}', score={:.4})",
            self.id, self.score
        )
    }
}

#[pyclass(name = "PyCollection", unsendable)]
pub struct Collection {
    inner: Arc<MemFuseCollection>,
}

#[pymethods]
impl Collection {
    #[pyo3(signature = (id, vector, metadata=None))]
    pub fn insert<'py>(
        &self,
        py: Python<'py>,
        id: &str,
        vector: PyReadonlyArray1<'py, f32>,
        metadata: Option<pyo3::Bound<'py, pyo3::types::PyDict>>,
    ) -> PyResult<()> {
        let slice = vector.as_slice().map_err(|e| {
            pyo3::exceptions::PyValueError::new_err(format!("Invalid vector format: {}", e))
        })?;

        let meta_val = if let Some(d) = metadata {
            let json_str = py.import("json")?.call_method1("dumps", (d,))?;
            let s: String = json_str.extract()?;
            let val: serde_json::Value = serde_json::from_str(&s).map_err(|e| {
                pyo3::exceptions::PyValueError::new_err(format!("Metadata JSON error: {}", e))
            })?;
            Some(val)
        } else {
            None
        };

        py.allow_threads(|| runtime().block_on(self.inner.insert(id, slice, meta_val)))
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
        Ok(())
    }

    #[pyo3(signature = (vector, k))]
    pub fn search<'py>(
        &self,
        py: Python<'py>,
        vector: PyReadonlyArray1<'py, f32>,
        k: usize,
    ) -> PyResult<Vec<PySearchResult>> {
        let slice = vector.as_slice().map_err(|e| {
            pyo3::exceptions::PyValueError::new_err(format!("Invalid vector format: {}", e))
        })?;

        let results = py
            .allow_threads(|| runtime().block_on(self.inner.search(slice, k)))
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;

        let mut py_res = Vec::with_capacity(results.len());
        for r in results {
            let metadata = if let Some(m) = r.metadata {
                Some(json_to_py(py, &m)?)
            } else {
                None
            };
            py_res.push(PySearchResult {
                id: r.id,
                score: r.score,
                metadata,
            });
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
    ) -> PyResult<Vec<PySearchResult>> {
        let slice = vector.as_slice().map_err(|e| {
            pyo3::exceptions::PyValueError::new_err(format!("Invalid vector format: {}", e))
        })?;

        let results = py
            .allow_threads(|| runtime().block_on(self.inner.hybrid_search(text, slice, k)))
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;

        let mut py_res = Vec::with_capacity(results.len());
        for r in results {
            let metadata = if let Some(m) = r.metadata {
                Some(json_to_py(py, &m)?)
            } else {
                None
            };
            py_res.push(PySearchResult {
                id: r.id,
                score: r.score,
                metadata,
            });
        }
        Ok(py_res)
    }
}

fn json_to_py(py: Python<'_>, value: &serde_json::Value) -> PyResult<PyObject> {
    let json_module = py.import("json")?;
    let json_str = serde_json::to_string(value).map_err(|e| {
        pyo3::exceptions::PyRuntimeError::new_err(format!("JSON serialization failed: {}", e))
    })?;
    let res = json_module.call_method1("loads", (json_str,))?;
    Ok(res.into())
}

#[pyfunction]
#[pyo3(name = "open", signature = (path, dimension=1536))]
fn py_open(py: Python<'_>, path: &str, dimension: usize) -> PyResult<Db> {
    let config = MemFuseConfig {
        dimension,
        ..Default::default()
    };
    let path_string = path.to_string();
    let db = py
        .allow_threads(|| runtime().block_on(MemFuse::open_with_config(path_string, config)))
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;

    Ok(Db {
        inner: Arc::new(db),
    })
}

#[pymodule]
fn memfuse(_py: Python<'_>, m: &Bound<'_, pyo3::types::PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(py_open, m)?)?;
    m.add_class::<Db>()?;
    m.add_class::<Collection>()?;
    m.add_class::<PySearchResult>()?;
    Ok(())
}
