//! Persistence layer for router calibration state.

use crate::profile::ProfileCalibrationState;
use memfuse_core::traits::{StorageEngine, VectorIndex};
use memfuse_core::Result;
use memfuse_db::Collection;
use std::collections::HashMap;

/// Key used to store router calibration state in collection KV store.
pub const CALIBRATION_STATE_KV_KEY: &str = "router:calibration_state:v1";

/// Persists calibration state into the collection's KV store.
pub async fn persist_calibration_state<S, V>(
    collection: &Collection<S, V>,
    state: &HashMap<String, ProfileCalibrationState>,
) -> Result<()>
where
    S: StorageEngine,
    V: VectorIndex,
{
    let value = serde_json::to_value(state)?;
    // put_kv (not put_kv_if_absent) — calibration is an overwritable snapshot.
    collection.put_kv(CALIBRATION_STATE_KV_KEY, &value).await
}

/// Loads calibration state from the collection's KV store.
pub async fn load_calibration_state<S, V>(
    collection: &Collection<S, V>,
) -> Result<HashMap<String, ProfileCalibrationState>>
where
    S: StorageEngine,
    V: VectorIndex,
{
    match collection.get_kv(CALIBRATION_STATE_KV_KEY).await? {
        Some(value) => Ok(serde_json::from_value(value)?),
        None => Ok(HashMap::new()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_persistence_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let config = memfuse_db::MemFuseConfig {
            dimension: 4,
            ..Default::default()
        };
        let db = memfuse_db::MemFuse::open_with_config(dir.path(), config)
            .await
            .unwrap();
        let collection = db.collection("default").await.unwrap();

        let mut initial_state = HashMap::new();
        let mut p1_state = ProfileCalibrationState::new(0.5);
        p1_state.times_selected = 10;
        p1_state.cumulative_confidence = 15.5;
        p1_state.calibrated_min_score = 0.62;
        initial_state.insert("p1".to_string(), p1_state.clone());

        persist_calibration_state(&collection, &initial_state)
            .await
            .unwrap();

        let loaded = load_calibration_state(&collection).await.unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded.get("p1"), Some(&p1_state));
    }
}
