path = 'crates/memfuse-store/src/checkpoint.rs'
with open(path, 'r') as f:
    content = f.read()

bad = 'assert_eq!(storage.get(b"key3").await.unwrap(), Some(b"val3".to_vec())); // unwrap allowed'
good = 'let val = storage.get(b"key3").await.unwrap(); // unwrap allowed\n        assert_eq!(val, Some(b"val3".to_vec()));'
new_content = content.replace(bad, good)
with open(path, 'w') as f:
    f.write(new_content)
