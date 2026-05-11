// AGENT:06 DATE:2026-05-11 STATUS:READY
// ANCHOR:DOC:DOC-LIB-001 — Python Bindings for MemFuse
// WP:WP-3.1 PRIO:1 NEEDS:SEARCH-001
#![forbid(unsafe_code)]

use memfuse_db::{Collection as MemFuseCollection, MemFuse, MemFuseConfig};
use memfuse_core::DistanceMetric;
use numpy::PyReadonlyArray1;
use pyo3::prelude::*;
use pythonize::{pythonize, depythonize};
use std::sync::Arc;
use std::sync::OnceLock;
use tokio::runtime::Runtime;

static RUNTIME: OnceLock<Result<Runtime, String>> = OnceLock::new();

fn get_runtime() -> PyResult<&'static Runtime> {
    let res = RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .map_err(|e| e.to_string())
    });
    match res {
        Ok(rt) => Ok(rt),
        Err(e) => Err(pyo3::exceptions::PyRuntimeError::new_err(format!("Failed to create tokio runtime: {}", e))),
    }
}

#[pyclass(eq, eq_int, name = "DistanceMetric")]
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum PyDistanceMetric {
    Cosine,
    Euclidean,
    DotProduct,
}

impl From<PyDistanceMetric> for DistanceMetric {
    fn from(m: PyDistanceMetric) -> Self {
        match m {
            PyDistanceMetric::Cosine => DistanceMetric::Cosine,
            PyDistanceMetric::Euclidean => DistanceMetric::Euclidean,
            PyDistanceMetric::DotProduct => DistanceMetric::DotProduct,
        }
    }
}

impl From<DistanceMetric> for PyDistanceMetric {
    fn from(m: DistanceMetric) -> Self {
        match m {
            DistanceMetric::Cosine => PyDistanceMetric::Cosine,
            DistanceMetric::Euclidean => PyDistanceMetric::Euclidean,
            DistanceMetric::DotProduct => PyDistanceMetric::DotProduct,
        }
    }
}

#[pyclass(name = "MemFuseConfig")]
#[derive(Clone)]
pub struct PyMemFuseConfig {
    #[pyo3(get, set)]
    pub dimension: usize,
    #[pyo3(get, set)]
    pub max_elements: usize,
    #[pyo3(get, set)]
    pub distance_metric: PyDistanceMetric,
}

#[pymethods]
impl PyMemFuseConfig {
    #[new]
    #[pyo3(signature = (dimension=1536, max_elements=1000000, distance_metric=PyDistanceMetric::Cosine))]
    fn new(dimension: usize, max_elements: usize, distance_metric: PyDistanceMetric) -> Self {
        Self {
            dimension,
            max_elements,
            distance_metric,
        }
    }

    fn __repr__(&self) -> String {
        format!(
            "MemFuseConfig(dimension={}, max_elements={}, distance_metric={:?})",
            self.dimension, self.max_elements, self.distance_metric
        )
    }
}

#[pyclass(name = "SearchResult")]
pub struct PySearchResult {
    #[pyo3(get)]
    pub id: String,
    #[pyo3(get)]
    pub score: f32,
    pub metadata: Option<serde_json::Value>,
}

#[pymethods]
impl PySearchResult {
    #[getter]
    fn get_metadata(&self, py: Python<'_>) -> PyObject {
        if let Some(ref meta) = self.metadata {
            pythonize(py, meta).map(|b| b.into()).unwrap_or_else(|_| py.None())
        } else {
            py.None()
        }
    }

    fn __repr__(&self) -> String {
        format!("SearchResult(id='{}', score={:.4})", self.id, self.score)
    }
}

#[pyclass(name = "MemFuse")]
pub struct PyMemFuse {
    inner: Arc<MemFuse>,
}

#[pymethods]
impl PyMemFuse {
    #[staticmethod]
    #[pyo3(signature = (path, dimension=1536))]
    pub fn open(py: Python<'_>, path: &str, dimension: usize) -> PyResult<Self> {
        let config = MemFuseConfig {
            dimension,
            ..Default::default()
        };
        let path_owned = path.to_string();
        let rt = get_runtime()?;
        let db = py.allow_threads(|| {
            rt.block_on(MemFuse::open_with_config(path_owned, config))
        }).map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;

        Ok(Self { inner: Arc::new(db) })
    }

    #[staticmethod]
    pub fn open_with_config(py: Python<'_>, path: &str, config: PyMemFuseConfig) -> PyResult<Self> {
        let db_config = MemFuseConfig {
            dimension: config.dimension,
            max_elements: config.max_elements,
            distance_metric: config.distance_metric.into(),
        };
        let path_owned = path.to_string();
        let rt = get_runtime()?;
        let db = py.allow_threads(|| {
            rt.block_on(MemFuse::open_with_config(path_owned, db_config))
        }).map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;

        Ok(Self { inner: Arc::new(db) })
    }

    pub fn collection(&self, py: Python<'_>, name: &str) -> PyResult<PyCollection> {
        let rt = get_runtime()?;
        let col = py.allow_threads(|| {
            rt.block_on(self.inner.collection(name))
        }).map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;

        Ok(PyCollection { inner: Arc::new(col) })
    }

    pub fn list_collections(&self, py: Python<'_>) -> PyResult<Vec<String>> {
        let rt = get_runtime()?;
        py.allow_threads(|| {
            rt.block_on(self.inner.list_collections())
        }).map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    pub fn drop_collection(&self, py: Python<'_>, name: &str) -> PyResult<()> {
        let rt = get_runtime()?;
        let name_owned = name.to_string();
        py.allow_threads(|| {
            rt.block_on(self.inner.drop_collection(&name_owned))
        }).map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    fn __repr__(&self) -> String {
        "MemFuse()".to_string()
    }
}

#[pyclass(name = "Collection")]
pub struct PyCollection {
    inner: Arc<MemFuseCollection>,
}

#[pymethods]
impl PyCollection {
    #[pyo3(signature = (id, vector, metadata=None))]
    pub fn insert(
        &self,
        py: Python<'_>,
        id: &str,
        vector: PyReadonlyArray1<'_, f32>,
        metadata: Option<PyObject>,
    ) -> PyResult<()> {
        let vec_slice = vector.as_slice().map_err(|e| {
            pyo3::exceptions::PyValueError::new_err(format!("Invalid vector format: {}", e))
        })?;

        let meta_val: Option<serde_json::Value> = if let Some(m) = metadata {
            Some(depythonize(m.bind(py))?)
        } else {
            None
        };

        let id_owned = id.to_string();
        let rt = get_runtime()?;
        py.allow_threads(|| {
            rt.block_on(self.inner.insert(&id_owned, vec_slice, meta_val))
        }).map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    pub fn get(&self, py: Python<'_>, id: &str) -> PyResult<Option<PyObject>> {
        let rt = get_runtime()?;
        let id_owned = id.to_string();
        let doc = py.allow_threads(|| {
            rt.block_on(self.inner.get(&id_owned))
        }).map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;

        if let Some(d) = doc {
            let dict = pyo3::types::PyDict::new(py);
            dict.set_item("id", d.id)?;
            if let Some(meta) = d.metadata {
                dict.set_item("metadata", pythonize(py, &meta)?)?;
            } else {
                dict.set_item("metadata", py.None())?;
            }
            Ok(Some(dict.into()))
        } else {
            Ok(None)
        }
    }

    #[pyo3(signature = (id, vector, metadata=None))]
    pub fn update(
        &self,
        py: Python<'_>,
        id: &str,
        vector: PyReadonlyArray1<'_, f32>,
        metadata: Option<PyObject>,
    ) -> PyResult<()> {
        let vec_slice = vector.as_slice().map_err(|e| {
            pyo3::exceptions::PyValueError::new_err(format!("Invalid vector format: {}", e))
        })?;

        let meta_val: Option<serde_json::Value> = if let Some(m) = metadata {
            Some(depythonize(m.bind(py))?)
        } else {
            None
        };

        let id_owned = id.to_string();
        let rt = get_runtime()?;
        py.allow_threads(|| {
            rt.block_on(self.inner.update(&id_owned, vec_slice, meta_val))
        }).map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    pub fn delete(&self, py: Python<'_>, id: &str) -> PyResult<()> {
        let rt = get_runtime()?;
        let id_owned = id.to_string();
        py.allow_threads(|| {
            rt.block_on(self.inner.delete(&id_owned))
        }).map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    #[pyo3(signature = (vector, k))]
    pub fn search(
        &self,
        py: Python<'_>,
        vector: PyReadonlyArray1<'_, f32>,
        k: usize,
    ) -> PyResult<Vec<PySearchResult>> {
        let vec_slice = vector.as_slice().map_err(|e| {
            pyo3::exceptions::PyValueError::new_err(format!("Invalid vector format: {}", e))
        })?;

        let rt = get_runtime()?;
        let results = py.allow_threads(|| {
            rt.block_on(self.inner.search(vec_slice, k))
        }).map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;

        Ok(results.into_iter().map(|r| PySearchResult {
            id: r.id,
            score: r.score,
            metadata: r.metadata,
        }).collect())
    }

    #[pyo3(signature = (text, vector, k))]
    pub fn hybrid_search(
        &self,
        py: Python<'_>,
        text: &str,
        vector: PyReadonlyArray1<'_, f32>,
        k: usize,
    ) -> PyResult<Vec<PySearchResult>> {
        let vec_slice = vector.as_slice().map_err(|e| {
            pyo3::exceptions::PyValueError::new_err(format!("Invalid vector format: {}", e))
        })?;

        let text_owned = text.to_string();
        let rt = get_runtime()?;
        let results = py.allow_threads(|| {
            rt.block_on(self.inner.hybrid_search(&text_owned, vec_slice, k))
        }).map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;

        Ok(results.into_iter().map(|r| PySearchResult {
            id: r.id,
            score: r.score,
            metadata: r.metadata,
        }).collect())
    }

    pub fn relate(&self, py: Python<'_>, from: &str, to: &str, label: &str) -> PyResult<()> {
        let rt = get_runtime()?;
        let from_owned = from.to_string();
        let to_owned = to.to_string();
        let label_owned = label.to_string();
        py.allow_threads(|| {
            rt.block_on(self.inner.relate(&from_owned, &to_owned, &label_owned))
        }).map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    pub fn len(&self, py: Python<'_>) -> PyResult<usize> {
        let rt = get_runtime()?;
        Ok(py.allow_threads(|| rt.block_on(self.inner.len())))
    }

    pub fn stats(&self, py: Python<'_>) -> PyResult<PyObject> {
        let rt = get_runtime()?;
        let stats = py.allow_threads(|| {
            rt.block_on(self.inner.stats())
        }).map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;

        // Simplified stats exposure
        let dict = pyo3::types::PyDict::new(py);
        dict.set_item("num_vectors", stats.num_vectors)?;
        dict.set_item("memory_usage_bytes", stats.memory_usage_bytes)?;
        dict.set_item("num_layers", stats.num_layers)?;
        Ok(dict.into())
    }

    pub fn scan_prefix(&self, py: Python<'_>, prefix: &str) -> PyResult<Vec<(String, PyObject)>> {
        let rt = get_runtime()?;
        let prefix_owned = prefix.to_string();
        let results = py.allow_threads(|| {
            rt.block_on(self.inner.scan_prefix(&prefix_owned))
        }).map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;

        let mut py_res = Vec::with_capacity(results.len());
        for (k, v) in results {
            py_res.push((k, pythonize(py, &v)?.into()));
        }
        Ok(py_res)
    }

    pub fn is_empty(&self, py: Python<'_>) -> PyResult<bool> {
        let rt = get_runtime()?;
        Ok(py.allow_threads(|| rt.block_on(self.inner.is_empty())))
    }

    fn __repr__(&self) -> String {
        format!("Collection(name='{}')", self.inner.name())
    }
}

#[pyfunction]
#[pyo3(signature = (path, dimension=1536))]
fn open(py: Python<'_>, path: &str, dimension: usize) -> PyResult<PyMemFuse> {
    PyMemFuse::open(py, path, dimension)
}

#[pymodule]
fn memfuse(_py: Python<'_>, m: &Bound<'_, pyo3::types::PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(open, m)?)?;
    m.add_class::<PyMemFuse>()?;
    m.add_class::<PyCollection>()?;
    m.add_class::<PyMemFuseConfig>()?;
    m.add_class::<PyDistanceMetric>()?;
    m.add_class::<PySearchResult>()?;
    Ok(())
}
