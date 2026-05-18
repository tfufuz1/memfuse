import re

with open("crates/memfuse-store/src/compaction.rs", "r") as f:
    code = f.read()
code = code.replace(".iter().expect(\"iter\")", ".iter().await.expect(\"iter\")")
with open("crates/memfuse-store/src/compaction.rs", "w") as f:
    f.write(code)

with open("crates/memfuse-db/src/collection.rs", "r") as f:
    code = f.read()

import re
code = re.sub(
    r'pub fn new\(.*?\) -> Self \{.*?(?=pub async fn insert)',
    '''pub fn new(
        name: String,
        storage: Arc<LsmStorage>,
        index: Arc<HnswIndex>,
        _next_tx: std::sync::Arc<std::sync::atomic::AtomicU64>,
        dimension: usize,
    ) -> Self {
        let prefix = format!("__col:{}:", name).into_bytes();
        Self { name, prefix, index, storage, dimension }
    }

    ''',
    code,
    flags=re.DOTALL
)

with open("crates/memfuse-db/src/collection.rs", "w") as f:
    f.write(code)
