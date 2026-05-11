// AGENT:06 DATE:2026-05-09 STATUS:READY
// ANCHOR:TODO:PY-001 — Stelle sicher, dass die zero-copy Vektor-Anbindung via numpy stabil ist.
// WP:WP-3.1 PRIO:1 NEEDS:SEARCH-001
// AGENT:@JULES-06 DATE:2026-05-09 STATUS:READY
// TEST: cd crates/memfuse-py && python -m pytest tests/ -v
// DONE: pip install . funktioniert, keine Deadlocks in tokio-Runtime.
// SUCCESSOR: @JULES-09 — "Python Bindings sind stabil. StateGraph kann darauf aufbauen."
//! # MemFuse Python Bindings
//!
//! This crate provides Python bindings for the MemFuse embedded hybrid-search database.
//! It allows Python users to interact with MemFuse's vector search and document
//! storage capabilities with minimal overhead.
#![forbid(unsafe_code)]

use memfuse_db::{Collection as MemFuseCollection, MemFuse, MemFuseConfig};
use numpy::PyReadonlyArray1;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use std::sync::Arc;
use std::sync::OnceLock;
use tokio::runtime::Runtime;

fn runtime() -> PyResult<&'static Runtime> {
    static RUNTIME: OnceLock<Runtime> = OnceLock::new();
    if let Some(rt) = RUNTIME.get() {
        return Ok(rt);
    }

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|e| {
            pyo3::exceptions::PyRuntimeError::new_err(format!(
                "Failed to create tokio runtime: {}",
                e
            ))
        })?;

    Ok(RUNTIME.get_or_init(|| rt))
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

    pub fn get(&self, id: &str, py: Python<'_>) -> PyResult<Option<PyObject>> {
        let rt = runtime()?;
        let doc = py
            .allow_threads(|| rt.block_on(self.inner.get(id)))
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;

        if let Some(d) = doc {
            let dict = PyDict::new(py);
            dict.set_item("id", d.id)?;
            if let Some(meta) = d.metadata {
                let py_meta = pythonize::pythonize(py, &meta)
                    .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
                dict.set_item("metadata", py_meta)?;
            }
            Ok(Some(dict.into()))
        } else {
            Ok(None)
        }
    }

    pub fn delete(&self, id: &str, py: Python<'_>) -> PyResult<()> {
        let rt = runtime()?;
        py.allow_threads(|| rt.block_on(self.inner.delete(id)))
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
        Ok(())
    }

    pub fn list_collections(&self, py: Python<'_>) -> PyResult<Vec<String>> {
        let rt = runtime()?;
        py.allow_threads(|| rt.block_on(self.inner.list_collections()))
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    pub fn drop_collection(&self, name: &str, py: Python<'_>) -> PyResult<()> {
        let rt = runtime()?;
        py.allow_threads(|| rt.block_on(self.inner.drop_collection(name)))
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    #[pyo3(signature = (id, vector, metadata=None))]
    pub fn insert<'py>(
        &self,
        py: Python<'py>,
        id: &str,
        vector: PyReadonlyArray1<'py, f32>,
        metadata: Option<pyo3::Bound<'py, pyo3::types::PyDict>>,
    ) -> PyResult<()> {
        let rt = runtime()?;
        let col = py
            .allow_threads(|| rt.block_on(self.inner.collection("default")))
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;

        let vec_slice = vector.as_slice().map_err(|e| {
            pyo3::exceptions::PyValueError::new_err(format!("Invalid vector format: {}", e))
        })?;

        let meta_val = if let Some(d) = metadata {
            Some(pythonize::depythonize(&d).map_err(|e| {
                pyo3::exceptions::PyValueError::new_err(format!("Invalid metadata: {}", e))
            })?)
        } else {
            None
        };
        let id_string = id.to_string();

        py.allow_threads(|| rt.block_on(col.insert(&id_string, vec_slice, meta_val)))
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
        Ok(())
    }

    #[pyo3(signature = (id, vector, metadata=None))]
    pub fn update<'py>(
        &self,
        py: Python<'py>,
        id: &str,
        vector: PyReadonlyArray1<'py, f32>,
        metadata: Option<pyo3::Bound<'py, pyo3::types::PyDict>>,
    ) -> PyResult<()> {
        let rt = runtime()?;
        let col = py
            .allow_threads(|| rt.block_on(self.inner.collection("default")))
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;

        let vec_slice = vector.as_slice().map_err(|e| {
            pyo3::exceptions::PyValueError::new_err(format!("Invalid vector format: {}", e))
        })?;

        let meta_val = if let Some(d) = metadata {
            Some(pythonize::depythonize(&d).map_err(|e| {
                pyo3::exceptions::PyValueError::new_err(format!("Invalid metadata: {}", e))
            })?)
        } else {
            None
        };
        let id_string = id.to_string();

        py.allow_threads(|| rt.block_on(col.update(&id_string, vec_slice, meta_val)))
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
        Ok(())
    }

    pub fn len(&self, py: Python<'_>) -> PyResult<usize> {
        let rt = runtime()?;
        py.allow_threads(|| rt.block_on(self.inner.len()))
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    pub fn is_empty(&self, py: Python<'_>) -> PyResult<bool> {
        let rt = runtime()?;
        py.allow_threads(|| rt.block_on(self.inner.is_empty()))
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    pub fn relate(&self, from: &str, to: &str, label: &str, py: Python<'_>) -> PyResult<()> {
        let rt = runtime()?;
        py.allow_threads(|| rt.block_on(self.inner.relate(from, to, label)))
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
        Ok(())
    }

    pub fn scan_prefix(&self, prefix: &str, py: Python<'_>) -> PyResult<Vec<PyObject>> {
        let rt = runtime()?;
        let results = py
            .allow_threads(|| rt.block_on(self.inner.scan_prefix(prefix)))
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;

        let mut py_results = Vec::new();
        for (k, v) in results {
            let dict = PyDict::new(py);
            dict.set_item("key", k)?;
            let py_val = pythonize::pythonize(py, &v)
                .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
            dict.set_item("value", py_val)?;
            py_results.push(dict.into());
        }
        Ok(py_results)
    }

    pub fn stats(&self, py: Python<'_>) -> PyResult<PyObject> {
        let rt = runtime()?;
        let stats = py
            .allow_threads(|| rt.block_on(self.inner.stats()))
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;

        let dict = PyDict::new(py);
        let idx_dict = PyDict::new(py);
        idx_dict.set_item("num_vectors", stats.index_stats.num_vectors)?;
        idx_dict.set_item("num_layers", stats.index_stats.num_layers)?;
        idx_dict.set_item("memory_usage_bytes", stats.index_stats.memory_usage_bytes)?;

        let storage_dict = PyDict::new(py);
        storage_dict.set_item(
            "memtable_size_bytes",
            stats.storage_stats.memtable_size_bytes,
        )?;
        storage_dict.set_item("num_segments", stats.storage_stats.num_segments)?;
        storage_dict.set_item("total_size_bytes", stats.storage_stats.total_size_bytes)?;

        dict.set_item("index", idx_dict)?;
        dict.set_item("storage", storage_dict)?;

        Ok(dict.into())
    }
}

#[pyclass(name = "Collection", unsendable)]
pub struct PyCollection {
    inner: Arc<MemFuseCollection>,
}

#[pyclass(name = "SearchResult")]
pub struct PySearchResult {
    #[pyo3(get)]
    pub id: String,
    #[pyo3(get)]
    pub score: f32,
    #[pyo3(get)]
    pub metadata: Option<PyObject>,
}

#[pymethods]
impl PyCollection {
    pub fn get(&self, id: &str, py: Python<'_>) -> PyResult<Option<PyObject>> {
        let rt = runtime()?;
        let doc = py
            .allow_threads(|| rt.block_on(self.inner.get(id)))
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;

        if let Some(d) = doc {
            let dict = PyDict::new(py);
            dict.set_item("id", d.id)?;
            if let Some(meta) = d.metadata {
                let py_meta = pythonize::pythonize(py, &meta)
                    .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
                dict.set_item("metadata", py_meta)?;
            }
            Ok(Some(dict.into()))
        } else {
            Ok(None)
        }
    }

    pub fn delete(&self, id: &str, py: Python<'_>) -> PyResult<()> {
        let rt = runtime()?;
        py.allow_threads(|| rt.block_on(self.inner.delete(id)))
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
        Ok(())
    }

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
            Some(pythonize::depythonize(&d).map_err(|e| {
                pyo3::exceptions::PyValueError::new_err(format!("Invalid metadata: {}", e))
            })?)
        } else {
            None
        };
        let id_string = id.to_string();
        let rt = runtime()?;

        py.allow_threads(|| rt.block_on(self.inner.insert(&id_string, vec_slice, meta_val)))
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
        Ok(())
    }

    #[pyo3(signature = (id, vector, metadata=None))]
    pub fn update<'py>(
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
            Some(pythonize::depythonize(&d).map_err(|e| {
                pyo3::exceptions::PyValueError::new_err(format!("Invalid metadata: {}", e))
            })?)
        } else {
            None
        };
        let id_string = id.to_string();
        let rt = runtime()?;

        py.allow_threads(|| rt.block_on(self.inner.update(&id_string, vec_slice, meta_val)))
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
        let vec_slice = vector.as_slice().map_err(|e| {
            pyo3::exceptions::PyValueError::new_err(format!("Invalid vector format: {}", e))
        })?;
        let rt = runtime()?;

        let results = py
            .allow_threads(|| rt.block_on(self.inner.search(vec_slice, k)))
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;

        let mut py_res = Vec::new();
        for r in results {
            let metadata = if let Some(meta) = r.metadata {
                Some(
                    pythonize::pythonize(py, &meta)
                        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?
                        .unbind(),
                )
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
        let vec_slice = vector.as_slice().map_err(|e| {
            pyo3::exceptions::PyValueError::new_err(format!("Invalid vector format: {}", e))
        })?;
        let text_owned = text.to_string();

        let rt = runtime()?;
        let results = py
            .allow_threads(|| rt.block_on(self.inner.hybrid_search(&text_owned, vec_slice, k)))
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;

        let mut py_res = Vec::new();
        for r in results {
            let metadata = if let Some(meta) = r.metadata {
                Some(
                    pythonize::pythonize(py, &meta)
                        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?
                        .unbind(),
                )
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

    pub fn len(&self, py: Python<'_>) -> PyResult<usize> {
        let rt = runtime()?;
        Ok(py.allow_threads(|| rt.block_on(self.inner.len())))
    }

    pub fn is_empty(&self, py: Python<'_>) -> PyResult<bool> {
        let rt = runtime()?;
        Ok(py.allow_threads(|| rt.block_on(self.inner.is_empty())))
    }

    pub fn relate(&self, from: &str, to: &str, label: &str, py: Python<'_>) -> PyResult<()> {
        let rt = runtime()?;
        py.allow_threads(|| rt.block_on(self.inner.relate(from, to, label)))
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
        Ok(())
    }

    pub fn scan_prefix(&self, prefix: &str, py: Python<'_>) -> PyResult<Vec<PyObject>> {
        let rt = runtime()?;
        let results = py
            .allow_threads(|| rt.block_on(self.inner.scan_prefix(prefix)))
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;

        let mut py_results = Vec::new();
        for (k, v) in results {
            let dict = PyDict::new(py);
            dict.set_item("key", k)?;
            let py_val = pythonize::pythonize(py, &v)
                .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
            dict.set_item("value", py_val)?;
            py_results.push(dict.into());
        }
        Ok(py_results)
    }

    pub fn stats(&self, py: Python<'_>) -> PyResult<PyObject> {
        let rt = runtime()?;
        let stats = py
            .allow_threads(|| rt.block_on(self.inner.stats()))
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;

        let dict = PyDict::new(py);
        dict.set_item("num_vectors", stats.num_vectors)?;
        dict.set_item("num_layers", stats.num_layers)?;
        dict.set_item("memory_usage_bytes", stats.memory_usage_bytes)?;
        Ok(dict.into())
    }
}

#[pyfunction]
#[pyo3(signature = (path, dimension=1536))]
fn open(py: Python<'_>, path: &str, dimension: usize) -> PyResult<PyMemFuse> {
    let config = MemFuseConfig {
        dimension,
        ..Default::default()
    };
    let path_string = path.to_string();
    let rt = runtime()?;
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
    Ok(())
}
