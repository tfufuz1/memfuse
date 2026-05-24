import sys
path = 'crates/memfuse-db/src/collection.rs'
with open(path, 'r') as f:
    content = f.read()

old = """                3 => {
                    let mut k = b"__tx_intent:".to_vec();
                    k.extend_from_slice(key);
                    k
                }"""
new = """                3 => {
                    let prefix = b"__tx_intent:";
                    let mut k = Vec::with_capacity(prefix.len() + key.len());
                    k.extend_from_slice(prefix);
                    k.extend_from_slice(key);
                    k
                }"""
if old in content:
    content = content.replace(old, new)
    with open(path, 'w') as f:
        f.write(content)
