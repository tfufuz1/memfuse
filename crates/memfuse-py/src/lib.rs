// ANCHOR:DOC:DOC-LIB-001 — Missing module documentation
// WP:WP-0.0 PRIO:3 NEEDS:NONE
// AGENT:06 DATE:2026-05-12 STATUS:DONE
// CREATED:2026-05-09 DEADLINE:NONE
// ANCHOR:TODO:PY-001 — Stelle sicher, dass die zero-copy Vektor-Anbindung via numpy stabil ist.
// WP:WP-3.1 PRIO:1 NEEDS:SEARCH-001
// AGENT:@JULES-06 DATE:2026-05-12 STATUS:DONE
// TEST: cd crates/memfuse-py && python -m pytest tests/ -v
// DONE: pip install . funktioniert, keine Deadlocks in tokio-Runtime. Zero-copy slices & direct metadata conversion implemented.
// SUCCESSOR: @JULES-09 — "Python Bindings sind stabil. StateGraph kann darauf aufbauen."
//! # MemFuse Python Bindings
//!
//! This crate provides Python bindings for the MemFuse embedded hybrid-search database.
//! It uses PyO3 and maturin to expose a high-performance, zero-copy interface
//! between Rust and Python, specifically optimized for NumPy arrays.
//!
//! ## Key components:
//! - `PyMemFuse`: Main entry point to manage collections.
//! - `PyCollection`: Handles document insertion and search operations.
//! - `PySearchResult`: Encapsulates search results with ID, score, and metadata.
#![forbid(unsafe_code)]
#![feature(once_cell_try)]

use memfuse_db::{Collection as MemFuseCollection, MemFuse, MemFuseConfig};
use numpy::PyReadonlyArray1;
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyDict, PyFloat, PyInt, PyList, PyString};
use std::sync::Arc;
use std::sync::OnceLock;
use tokio::runtime::Runtime;

fn runtime() -> PyResult<&'static Runtime> {
    static RUNTIME: OnceLock<Runtime> = OnceLock::new();
    RUNTIME.get_or_try_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .map_err(|e| {
                pyo3::exceptions::PyRuntimeError::new_err(format!(
                    "Failed to create tokio runtime for memfuse-py: {}",
                    e
                ))
            })
    })?;

    // ANCHOR:DEBT:DEBT-UNWRAP-LIB-25 — unwrap/expect in production code
    // WP:WP-0.0 PRIO:2 NEEDS:NONE
    // AGENT:06 DATE:2026-05-12 STATUS:DONE
    // CREATED:2026-05-09 DEADLINE:NONE
    RUNTIME.get().ok_or_else(|| {
        pyo3::exceptions::PyRuntimeError::new_err("Failed to initialize tokio runtime")
    })
}

#[pyclass(name = "Db", unsendable)]
pub struct PyMemFuse {
    inner: Arc<MemFuse>,
}

#[pymethods]
impl PyMemFuse {
    pub fn collection(&self, name: &str, py: Python<'_>) -> PyResult<PyCollection> {
        let rt = runtime()?;
        let col = py
            .allow_threads(|| rt.block_on(self.inner.collection(name)))
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
        Ok(PyCollection {
            inner: Arc::new(col),
        })
    }
}

#[pyclass(name = "SearchResult")]
pub struct PySearchResult {
    #[pyo3(get)]
    pub id: String,
    #[pyo3(get)]
    pub score: f32,
    #[pyo3(get)]
    pub metadata: PyObject,
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
        let rt = runtime()?;
        let vec_slice = vector.as_slice().map_err(|e| {
            pyo3::exceptions::PyValueError::new_err(format!("Invalid vector format: {}", e))
        })?;

        let meta_val = metadata.map(|d| py_to_json(d.into_any())).transpose()?;

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
    ) -> PyResult<Vec<PySearchResult>> {
        let rt = runtime()?;
        let vec_slice = vector.as_slice().map_err(|e| {
            pyo3::exceptions::PyValueError::new_err(format!("Invalid vector format: {}", e))
        })?;

        let results = py
            .allow_threads(|| rt.block_on(self.inner.search(vec_slice, k)))
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;

        let mut py_res = Vec::new();
        for r in results {
            let py_meta = if let Some(meta) = r.metadata {
                json_to_py(py, meta)?
            } else {
                py.None()
            };

            py_res.push(PySearchResult {
                id: r.id,
                score: r.score,
                metadata: py_meta,
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
        let rt = runtime()?;
        let vec_slice = vector.as_slice().map_err(|e| {
            pyo3::exceptions::PyValueError::new_err(format!("Invalid vector format: {}", e))
        })?;

        let results = py
            .allow_threads(|| rt.block_on(self.inner.hybrid_search(text, vec_slice, k)))
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;

        let mut py_res = Vec::new();
        for r in results {
            let py_meta = if let Some(meta) = r.metadata {
                json_to_py(py, meta)?
            } else {
                py.None()
            };

            py_res.push(PySearchResult {
                id: r.id,
                score: r.score,
                metadata: py_meta,
            });
        }
        Ok(py_res)
    }
}

#[pyfunction]
#[pyo3(signature = (path, dimension=1536))]
fn open(py: Python<'_>, path: &str, dimension: usize) -> PyResult<PyMemFuse> {
    let rt = runtime()?;
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

fn json_to_py(py: Python<'_>, value: serde_json::Value) -> PyResult<PyObject> {
    match value {
        serde_json::Value::Null => Ok(py.None()),
        serde_json::Value::Bool(b) => Ok(PyBool::new(py, b).to_owned().into()),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(PyInt::new(py, i).to_owned().into())
            } else if let Some(f) = n.as_f64() {
                Ok(PyFloat::new(py, f).to_owned().into())
            } else {
                Err(pyo3::exceptions::PyValueError::new_err("Invalid number"))
            }
        }
        serde_json::Value::String(s) => Ok(PyString::new(py, &s).to_owned().into()),
        serde_json::Value::Array(v) => {
            let list = PyList::empty(py);
            for val in v {
                list.append(json_to_py(py, val)?)?;
            }
            Ok(list.to_owned().into())
        }
        serde_json::Value::Object(m) => {
            let dict = PyDict::new(py);
            for (k, v) in m {
                dict.set_item(k, json_to_py(py, v)?)?;
            }
            Ok(dict.to_owned().into())
        }
    }
}

fn py_to_json(obj: Bound<'_, PyAny>) -> PyResult<serde_json::Value> {
    if obj.is_none() {
        Ok(serde_json::Value::Null)
    } else if let Ok(b) = obj.extract::<bool>() {
        Ok(serde_json::Value::Bool(b))
    } else if let Ok(i) = obj.extract::<i64>() {
        Ok(serde_json::Value::Number(i.into()))
    } else if let Ok(f) = obj.extract::<f64>() {
        Ok(serde_json::Value::from(f))
    } else if let Ok(s) = obj.extract::<String>() {
        Ok(serde_json::Value::String(s))
    } else if let Ok(dict) = obj.downcast::<PyDict>() {
        let mut map = serde_json::Map::new();
        for (k, v) in dict.iter() {
            let key = k.extract::<String>()?;
            map.insert(key, py_to_json(v)?);
        }
        Ok(serde_json::Value::Object(map))
    } else if let Ok(list) = obj.downcast::<PyList>() {
        let mut vec = Vec::new();
        for v in list.iter() {
            vec.push(py_to_json(v)?);
        }
        Ok(serde_json::Value::Array(vec))
    } else {
        // Fallback to json.dumps if complex type
        static JSON_DUMPS: OnceLock<PyObject> = OnceLock::new();
        let py = obj.py();
        let dumps = JSON_DUMPS.get_or_try_init(|| {
            let json = py.import("json")?;
            let dumps = json.getattr("dumps")?;
            Ok::<PyObject, PyErr>(dumps.into())
        })?;

        let s: String = dumps.bind(py).call1((obj,))?.extract()?;
        serde_json::from_str(&s).map_err(|e| {
            pyo3::exceptions::PyValueError::new_err(format!("Failed to convert to JSON: {}", e))
        })
    }
}

#[pymodule]
fn memfuse(_py: Python<'_>, m: &Bound<'_, pyo3::types::PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(open, m)?)?;
    m.add_class::<PyMemFuse>()?;
    m.add_class::<PyCollection>()?;
    m.add_class::<PySearchResult>()?;
    Ok(())
}
