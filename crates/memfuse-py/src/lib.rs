// ANCHOR:DOC:DOC-LIB-001 — Missing module documentation
// WP:WP-0.0 PRIO:3 NEEDS:NONE
// AGENT:06 DATE:2026-05-09 STATUS:READY
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
use pyo3::exceptions::PyRuntimeError;
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
        .map_err(|e| PyRuntimeError::new_err(format!("Failed to create tokio runtime: {}", e)))?;

    // ANCHOR:DEBT:DEBT-UNWRAP-LIB-25 — unwrap/expect in production code
    // WP:WP-0.0 PRIO:2 NEEDS:NONE
    // AGENT:06 DATE:2026-05-09 STATUS:DONE
    // CREATED:2026-05-09 DEADLINE:NONE
    Ok(RUNTIME.get_or_init(|| rt))
}

#[pyclass(unsendable)]
pub struct Db {
    inner: Arc<MemFuse>,
}

#[pymethods]
impl Db {
    pub fn collection(&self, name: &str, py: Python<'_>) -> PyResult<Collection> {
        let rt = runtime()?;
        let col = py
            .allow_threads(|| rt.block_on(self.inner.collection(name)))
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
        Ok(Collection {
            inner: Arc::new(col),
        })
    }
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
        let vec_owned = vector
            .as_slice()
            .map_err(|e| {
                pyo3::exceptions::PyValueError::new_err(format!("Invalid vector format: {}", e))
            })?
            .to_vec();

        let meta_val = if let Some(d) = metadata {
            let json_str = py.import("json")?.call_method1("dumps", (d,))?;
            let s: String = json_str.extract()?;
            serde_json::from_str(&s).ok()
        } else {
            None
        };
        let id_string = id.to_string();

        let rt = runtime()?;
        py.allow_threads(move || rt.block_on(self.inner.insert(&id_string, &vec_owned, meta_val)))
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
        let vec_owned = vector
            .as_slice()
            .map_err(|e| {
                pyo3::exceptions::PyValueError::new_err(format!("Invalid vector format: {}", e))
            })?
            .to_vec();

        let rt = runtime()?;
        let results = py
            .allow_threads(move || rt.block_on(self.inner.search(&vec_owned, k)))
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;

        let mut py_res = Vec::new();
        for r in results {
            let dict = PyDict::new(py);
            dict.set_item("id", r.id)?;
            dict.set_item("score", r.score)?;
            py_res.push(dict.into());
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
        let vec_owned = vector
            .as_slice()
            .map_err(|e| {
                pyo3::exceptions::PyValueError::new_err(format!("Invalid vector format: {}", e))
            })?
            .to_vec();
        let text_owned = text.to_string();

        let rt = runtime()?;
        let results = py
            .allow_threads(move || rt.block_on(self.inner.hybrid_search(&text_owned, &vec_owned, k)))
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;

        let mut py_res = Vec::new();
        for r in results {
            let dict = PyDict::new(py);
            dict.set_item("id", r.id)?;
            dict.set_item("score", r.score)?;
            py_res.push(dict.into());
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
    let rt = runtime()?;
    let db = py
        .allow_threads(|| rt.block_on(MemFuse::open_with_config(path_string, config)))
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
    Ok(())
}
