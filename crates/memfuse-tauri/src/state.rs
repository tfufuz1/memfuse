use memfuse_db::MemFuse;
use parking_lot::RwLock;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Semaphore;

/// Maximale Anzahl gleichzeitig laufender blockierender Regex-Operationen.
/// Verhindert Erschöpfung des tokio-Blocking-Thread-Pools bei Bulk-Transform-Szenarien.
///
/// AI-NOTE[CONCURRENCY]: Auch wenn die `regex`-Crate v1.13.1 lineare Laufzeit
/// garantiert und kein echtes ReDoS möglich ist, begrenzt das Semaphore die
/// Anzahl gleichzeitig belegter Blocking-Threads, falls ein pathologischer Input
/// (z.B. sehr langer Input × sehr komplexes Pattern) trotz linearer Ausführung
/// tatsächlich mehrere Sekunden läuft. Standard tokio-Pool-Limit: 512 Threads.
/// DECISION-REF: ADR-014
pub const MAX_CONCURRENT_REGEX_OPS: usize = 8;

/// Globaler App-Zustand: hält die aktuell geöffnete lokale Datenbank.
pub struct AppState {
    pub db: RwLock<Option<Arc<MemFuse>>>,
    pub db_path: RwLock<Option<PathBuf>>,
    /// Semaphore zur Begrenzung gleichzeitiger Regex-Blocking-Operationen.
    /// Schützt den tokio-Blocking-Thread-Pool bei Bulk-Transform-Szenarien.
    /// DECISION-REF: ADR-014
    pub regex_semaphore: Arc<Semaphore>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            db: RwLock::new(None),
            db_path: RwLock::new(None),
            regex_semaphore: Arc::new(Semaphore::new(MAX_CONCURRENT_REGEX_OPS)),
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
        assert_eq!(
            state.regex_semaphore.available_permits(),
            MAX_CONCURRENT_REGEX_OPS
        );

        let default_state = AppState::default();
        assert!(default_state.db.read().is_none());
        assert!(default_state.db_path.read().is_none());
    }

    #[test]
    fn test_semaphore_permit_capacity() {
        let state = AppState::new();
        assert_eq!(
            state.regex_semaphore.available_permits(),
            MAX_CONCURRENT_REGEX_OPS,
            "Semaphore muss mit MAX_CONCURRENT_REGEX_OPS Permits initialisiert sein"
        );
    }

    #[test]
    fn test_semaphore_limit_not_exceeded() {
        let state = AppState::new();
        let mut permits = Vec::new();

        for _ in 0..MAX_CONCURRENT_REGEX_OPS {
            let permit = state.regex_semaphore.try_acquire();
            assert!(
                permit.is_ok(),
                "Permit allocation should succeed within limit"
            );
            permits.push(permit.expect("permit acquisition should succeed")); // expect
        }

        assert_eq!(state.regex_semaphore.available_permits(), 0);
        assert!(
            state.regex_semaphore.try_acquire().is_err(),
            "Acquiring permit beyond limit must fail"
        );

        permits.pop();
        assert_eq!(state.regex_semaphore.available_permits(), 1);
        assert!(
            state.regex_semaphore.try_acquire().is_ok(),
            "Acquiring permit after drop should succeed"
        );
    }
}
