//! Python bindings for MemFuse — Embedded Hybrid-Search for AI Agents.
//!
//! This crate provides a PyO3-based bridge to the MemFuse database,
//! allowing high-performance vector search and hybrid search from Python
//! with zero-copy NumPy integration.
//!
//! ## Example
//!
//! ```python
//! import memfuse
//! import numpy as np
//!
//! db = memfuse.open("./my_db", dimension=1536)
//! col = db.collection("docs")
//!
//! vec = np.random.rand(1536).astype(np.float32)
//! col.insert("doc-1", vec, metadata={"text": "Hello MemFuse"})
//!
//! results = col.search(vec, k=5)
//! for r in results:
//!     print(f"{r.id}: {r.score}")
//! ```

// ANCHOR:DOC:DOC-LIB-001 — Missing module documentation
// WP:WP-0.0 PRIO:3 NEEDS:NONE
// AGENT:06 DATE:2026-05-09 STATUS:DONE
// CREATED:2026-05-09 DEADLINE:NONE
// ANCHOR:TODO:PY-001 — Stelle sicher, dass die zero-copy Vektor-Anbindung via numpy stabil ist.
// WP:WP-3.1 PRIO:1 NEEDS:SEARCH-001
// AGENT:@JULES-06 DATE:2026-05-09 STATUS:READY
// TEST: cd crates/memfuse-py && python -m pytest tests/ -v
// DONE: pip install . funktioniert, keine Deadlocks in tokio-Runtime.
// SUCCESSOR: @JULES-09 — "Python Bindings sind stabil. StateGraph kann darauf aufbauen."
#![forbid(unsafe_code)]

use memfuse_db::{Collection as MemFuseCollection, MemFuse, MemFuseConfig};
use numpy::PyReadonlyArray1;
use pyo3::prelude::*;
use pyo3::types::PyDict;
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
            // REASON: Failure to initialize the core async runtime for the Python extension is a fatal configuration error.
            .expect("Failed to create tokio runtime for memfuse-py")
    })
}

/// Helper to convert `serde_json::Value` to `PyObject` using Python's `json.loads`.
fn value_to_py(py: Python<'_>, value: &serde_json::Value) -> PyResult<PyObject> {
    let json_module = py.import("json")?;
    let json_str = serde_json::to_string(value).map_err(|e| {
        pyo3::exceptions::PyValueError::new_err(format!("Failed to serialize metadata: {}", e))
    })?;
    json_module.call_method1("loads", (json_str,))?.extract()
}

/// Helper to convert a Python dictionary to `serde_json::Value` using Python's `json.dumps`.
fn py_dict_to_value(py: Python<'_>, dict: &Bound<'_, PyDict>) -> PyResult<serde_json::Value> {
    let json_module = py.import("json")?;
    let json_str: String = json_module.call_method1("dumps", (dict,))?.extract()?;
    serde_json::from_str(&json_str).map_err(|e| {
        pyo3::exceptions::PyValueError::new_err(format!(
            "Failed to parse metadata from JSON: {}",
            e
        ))
    })
}

#[pyclass(unsendable)]
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

#[pyclass]
pub struct SearchResult {
    #[pyo3(get)]
    pub id: String,
    #[pyo3(get)]
    pub score: f32,
    #[pyo3(get)]
    pub metadata: Option<PyObject>,
}

#[pyclass(unsendable)]
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
        let vec_slice = vector.as_slice().map_err(|e| {
            pyo3::exceptions::PyValueError::new_err(format!("Invalid vector format: {}", e))
        })?;

        let meta_val = if let Some(d) = metadata {
            Some(py_dict_to_value(py, &d)?)
        } else {
            None
        };
        let id_string = id.to_string();

        py.allow_threads(|| runtime().block_on(self.inner.insert(&id_string, vec_slice, meta_val)))
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
        Ok(())
    }

    #[pyo3(signature = (vector, k))]
    pub fn search<'py>(
        &self,
        py: Python<'py>,
        vector: PyReadonlyArray1<'py, f32>,
        k: usize,
    ) -> PyResult<Vec<SearchResult>> {
        let vec_slice = vector.as_slice().map_err(|e| {
            pyo3::exceptions::PyValueError::new_err(format!("Invalid vector format: {}", e))
        })?;

        let results = py
            .allow_threads(|| runtime().block_on(self.inner.search(vec_slice, k)))
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;

        let mut py_res = Vec::with_capacity(results.len());
        for r in results {
            let metadata = if let Some(m) = r.metadata {
                Some(value_to_py(py, &m)?)
            } else {
                None
            };
            py_res.push(SearchResult {
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
    ) -> PyResult<Vec<SearchResult>> {
        let vec_slice = vector.as_slice().map_err(|e| {
            pyo3::exceptions::PyValueError::new_err(format!("Invalid vector format: {}", e))
        })?;
        let text_owned = text.to_string();

        let results = py
            .allow_threads(|| {
                runtime().block_on(self.inner.hybrid_search(&text_owned, vec_slice, k))
            })
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;

        let mut py_res = Vec::with_capacity(results.len());
        for r in results {
            let metadata = if let Some(m) = r.metadata {
                Some(value_to_py(py, &m)?)
            } else {
                None
            };
            py_res.push(SearchResult {
                id: r.id,
                score: r.score,
                metadata,
            });
        }
        Ok(py_res)
    }
}

#[pyfunction]
#[pyo3(signature = (path, dimension=1536))]
fn open(py: Python<'_>, path: &str, dimension: usize) -> PyResult<Db> {
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
    m.add_function(wrap_pyfunction!(open, m)?)?;
    m.add_class::<Db>()?;
    m.add_class::<Collection>()?;
    m.add_class::<SearchResult>()?;
    Ok(())
}
