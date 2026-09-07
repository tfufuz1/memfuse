use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use tokio::sync::Mutex;

const KV_LOCK_SHARDS: usize = 16;

pub(crate) struct KvKeyLocks {
    shards: [Mutex<()>; KV_LOCK_SHARDS],
}

impl KvKeyLocks {
    pub const fn new() -> Self {
        Self {
            shards: [
                Mutex::const_new(()),
                Mutex::const_new(()),
                Mutex::const_new(()),
                Mutex::const_new(()),
                Mutex::const_new(()),
                Mutex::const_new(()),
                Mutex::const_new(()),
                Mutex::const_new(()),
                Mutex::const_new(()),
                Mutex::const_new(()),
                Mutex::const_new(()),
                Mutex::const_new(()),
                Mutex::const_new(()),
                Mutex::const_new(()),
                Mutex::const_new(()),
                Mutex::const_new(()),
            ],
        }
    }

    pub async fn lock_for<'a>(&'a self, key: &str) -> tokio::sync::MutexGuard<'a, ()> {
        let mut hasher = DefaultHasher::new();
        key.hash(&mut hasher);
        let idx = (hasher.finish() as usize) % KV_LOCK_SHARDS;
        self.shards[idx].lock().await
    }
}
