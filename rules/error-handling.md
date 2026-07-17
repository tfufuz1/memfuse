# Error Handling Rules

## Single Error Type

All crates use `memfuse_core::MemFuseError`. No crate-local error enums.

## Variant Policy

- **Append-only**: new variants go at the end of the enum (binary compat)
- **Structured over String**: prefer `WalCorruption { offset, reason }` over `Storage(String)` for machine-parseable errors
- **`From` impls**: only in `error.rs` itself — no wildcard `From<E>` in other modules

## Real Examples (from this repo)

```rust
// ✅ Storage error with context (crates/memfuse-store/src/lsm.rs:289)
let file = File::create(path_ref).await
    .map_err(|e| MemFuseError::Storage(format!("Failed to create SSTable: {}", e)))?;

// ✅ Structured error (crates/memfuse-store/src/sstable.rs:712)
return Err(MemFuseError::ChecksumMismatch {
    path: path_buf.to_string_lossy().to_string(),
    block_id: bloom_offset,
});

// ❌ Wrong: swallowing error
let _ = file.sync_all().await;  // ONLY acceptable for best-effort cleanup

// ❌ Wrong: panic on missing value
let val = map[&key];  // Use map.get(&key).ok_or_else(|| ...)?
```

## Available Variants (as of 2026-07)

| Variant | When to use |
|---|---|
| `Internal(String)` | Logic bugs that should be unreachable |
| `InvalidInput(String)` | Caller-provided data fails validation |
| `NotFound(String)` | Key/doc/entity lookup miss |
| `Storage(String)` | Disk I/O, SSTable, WAL generic failures |
| `Io(io::Error)` | Raw I/O (auto-converted via `From`) |
| `WalCorruption { offset, reason }` | WAL integrity check failure |
| `ChecksumMismatch { path, block_id }` | CRC/hash verification failure |
| `Transaction(String)` | Tx lifecycle errors |
| `TransactionTimeout { tx_id, elapsed_ms }` | Tx exceeded TTL |
| `Index(String)` | HNSW/vector index errors |
| `Text(String)` | BM25/text index errors |
| `Crypto(String)` | Encryption/decryption failures |
| `ParseError(String)` | Deserialization failures |
