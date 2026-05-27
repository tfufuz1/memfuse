import re

# 1. compaction.rs
with open("crates/memfuse-store/src/compaction.rs", "r") as f:
    code = f.read()
code = code.replace("last_sst.iter().expect", "last_sst.iter().await.expect")
with open("crates/memfuse-store/src/compaction.rs", "w") as f:
    f.write(code)

# 2. collection.rs
with open("crates/memfuse-db/src/collection.rs", "r") as f:
    code = f.read()

code = code.replace("pub struct Collection", "#[derive(Clone)]\npub struct Collection")

old_new = """    pub fn new(name: &str, storage: Arc<LsmStorage>, dimension: usize) -> Self {
        // The prefix perfectly fences all documents of this collection in the LSM-Tree
        let prefix = format!("__col:{name}:").into_bytes();
        let index = Arc::new(HnswIndex::new(memfuse_index::HnswConfig::default()));

        Self {
            name: name.to_string(),
            prefix,
            index,
            storage,
            dimension,
        }
    }"""
new_new = """    pub fn new(
        name: String,
        storage: Arc<LsmStorage>,
        index: Arc<HnswIndex>,
        _next_tx: std::sync::Arc<std::sync::atomic::AtomicU64>,
        dimension: usize,
    ) -> Self {
        let prefix = format!("__col:{}:", name).into_bytes();
        Self { name, prefix, index, storage, dimension }
    }"""
code = code.replace(old_new, new_new)

code = code.replace("index_size_bytes: 0", "memory_usage_bytes: 0, num_layers: 0")

with open("crates/memfuse-db/src/collection.rs", "w") as f:
    f.write(code)

# 3. lib.rs
with open("crates/memfuse-db/src/lib.rs", "r") as f:
    code = f.read()

code = code.replace("db.len().await, 1", "db.len().await.expect(\"len\"), 1")
code = code.replace("db.len().await, 0", "db.len().await.expect(\"len\"), 0")
code = code.replace("db.len().await, 2", "db.len().await.expect(\"len\"), 2")

with open("crates/memfuse-db/src/lib.rs", "w") as f:
    f.write(code)
