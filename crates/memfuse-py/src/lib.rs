//! Zero-Copy Python Bridge for MemFuse (SAOS)

#![allow(unsafe_op_in_unsafe_fn)]
#![allow(non_local_definitions)]
use memfuse_db::{Collection as DbCollection, MemFuse};
use numpy::PyReadonlyArray1;
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use std::sync::Arc;
use tokio::runtime::Runtime;

#[pyclass]
pub struct Collection {
    inner: DbCollection,
    rt: Arc<Runtime>,
}

#[pymethods]
impl Collection {
    pub fn insert(
        &self,
        id: &str,
        embedding: PyReadonlyArray1<f32>,
        metadata: Option<String>,
    ) -> PyResult<()> {
        let vec = embedding
            .as_slice()
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
        let meta = metadata.and_then(|m| serde_json::from_str(&m).ok());

        self.rt
            .block_on(async { self.inner.insert(id, vec, meta).await })
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))
    }

    pub fn search<'py>(
        &self,
        _py: Python<'py>,
        query: PyReadonlyArray1<f32>,
        k: usize,
    ) -> PyResult<Vec<(String, f32)>> {
        let vec = query
            .as_slice()
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
        let results = self
            .rt
            .block_on(async { self.inner.search(vec, k).await })
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;

        Ok(results.into_iter().map(|r| (r.id, r.score)).collect())
    }
}

#[pyclass]
pub struct Agent {
    db: Arc<MemFuse>,
    rt: Arc<Runtime>,
}

#[pymethods]
impl Agent {
    #[new]
    pub fn new(path: &str) -> PyResult<Self> {
        let rt = Arc::new(Runtime::new().map_err(|e| PyRuntimeError::new_err(e.to_string()))?);
        let db = rt
            .block_on(async { MemFuse::open(path).await })
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;

        Ok(Self {
            db: Arc::new(db),
            rt,
        })
    }

    pub fn collection(&self, name: &str) -> PyResult<Collection> {
        let col = self
            .rt
            .block_on(async { self.db.collection(name).await })
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;

        Ok(Collection {
            inner: col,
            rt: Arc::clone(&self.rt),
        })
    }
}

#[pymodule]
fn memfuse(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Agent>()?;
    m.add_class::<Collection>()?;
    Ok(())
}
