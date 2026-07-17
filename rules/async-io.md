# Async I/O Rules

## Decision Tree

```
Is it WAL append / MemTable flush / directory create?
  → tokio::fs (sequential async writes)

Is it SSTable random-access read (pread at offset)?
  → std::fs::File inside tokio::task::spawn_blocking
  → Reason: tokio::fs has no equivalent to FileExt::read_exact_at

Is it file delete / rename / metadata?
  → tokio::fs::remove_file / tokio::fs::rename
```

## The spawn_blocking Pattern (SSTable reads)

```rust
// From crates/memfuse-store/src/sstable.rs:542-551
let (file, file_size) =
    tokio::task::spawn_blocking(move || -> std::io::Result<(std::fs::File, u64)> {
        let file = std::fs::File::open(&path)?;
        let metadata = file.metadata()?;
        Ok((file, metadata.len()))
    })
    .await
    .map_err(|e| MemFuseError::Storage(format!("Join error: {}", e)))?
    .map_err(|e| MemFuseError::Storage(format!("File open failed: {}", e)))?;
```

Note the double `?` — first for `JoinError` (task panic), then for the inner `io::Error`.

## Invariant

`lib.rs` states: "Alle Disk-I/O via tokio::fs (zero std::fs imports)."
This invariant is **documented but intentionally violated** for SSTable random reads.
The violation is tracked as `TODO[STABILIZE]` in `lib.rs`.
