# Dependency Rules

## Before Adding Any Dependency

1. **Does it exist?** Check crates.io — LLMs hallucinate crate names ("slopsquatting")
2. **Is it maintained?** Last release within 12 months, >1 maintainer preferred
3. **License compatible?** Must be MIT OR Apache-2.0 (see workspace Cargo.toml)
4. **Is it necessary?** If the needed functionality is <20 lines of std code, write it inline
5. **`cargo audit` clean?** No known advisories for the version being added

## Before Using Any API from an Existing Dependency

**Verify the function/trait/method exists in the PINNED version from Cargo.lock.**

LLMs routinely generate calls to functions that existed in a different version or never existed.
Check docs.rs/[crate]/[exact-version] — not from memory.

## Current Workspace Dependencies (2026-07)

```toml
# Verified essential — used extensively
thiserror = "2"          # MemFuseError derive
tokio = "1"              # async runtime (full features)
bytes = "1"              # zero-copy buffer management
blake3 = "1"             # hashing (keys, bloom, HMAC)
serde = "1"              # serialization
serde_json = "1"         # JSON for metadata

# Verified justified — specific use cases
crc32fast = "1.3"        # SSTable/WAL integrity checks
memmap2 = "0.9"          # memory-mapped SSTable reads
ahash = "0.8"            # fast HashMap hashing
roaring = "0.10"         # compressed bitmaps (HNSW delete tracking)
parking_lot = "0.12"     # faster Mutex/RwLock (index hot path)
flatbuffers = "24.3"     # IPC serialization

# Review needed — possibly over-specified
bincode = "1.3.3"        # used for WAL entry serialization
rand = "0.8"             # salt generation, HNSW random levels
```

## Slopsquatting Defense

If a dependency name looks unusual or you haven't seen it before:
1. Search crates.io manually
2. Verify the GitHub/repo link in Cargo.toml matches the crate
3. Check download count — very low downloads on a "utility" crate is a red flag
