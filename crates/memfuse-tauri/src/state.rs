use memfuse_db::MemFuse;
use parking_lot::RwLock;
use std::path::PathBuf;
use std::sync::Arc;

/// Globaler App-Zustand: hält die aktuell geöffnete lokale Datenbank.
pub struct AppState {
    pub db: RwLock<Option<Arc<MemFuse>>>,
    pub db_path: RwLock<Option<PathBuf>>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            db: RwLock::new(None),
            db_path: RwLock::new(None),
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_state_initialization() {
        let state = AppState::new();
        assert!(state.db.read().is_none());
        assert!(state.db_path.read().is_none());

        let default_state = AppState::default();
        assert!(default_state.db.read().is_none());
        assert!(default_state.db_path.read().is_none());
    }
}
