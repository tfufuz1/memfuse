//! SSTable (Sorted String Table) implementation.
//!
//! SSTables are persistent, immutable files containing sorted key-value pairs.
//!
//! ## Architecture
//! - **Data Blocks**: Keys and values are grouped into 4KB blocks. Each entry consists of
//!   key length, key, sequence number, value length, and value.
//! - **Index Block**: Located at the end of the file, it maps the last key of each data
//!   block to its byte offset, enabling efficient binary search.
//! - **Trailer**: Contains the index offset (8 bytes) and a magic number `0x4D465354` ("MFST").
//!
//! ## Invariants
//! - **Immutability**: Once written, SSTables are never modified. Compaction creates new ones.
//! - **Sorted Order**: Entries within blocks and blocks within the file are sorted lexicographically by key.
//! - **Async I/O**: All disk operations use `tokio::fs` or `memmap2` with `spawn_blocking`.
//! - **Zero Panic**: Production code paths avoid `unwrap()` and `expect()`, favoring explicit error handling.

use bytes::{BufMut, Bytes, BytesMut};
use lru::LruCache;
use memfuse_core::{MemFuseError, Result};
use memfuse_crypto::crypto::KeyManager;
use parking_lot::RwLock;
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;

/// Cache for SSTable blocks. Key is (file_id, block_offset).
pub type BlockCache = RwLock<LruCache<(u64, u64), Bytes>>;

/// Creates a new block cache instance. Capacity is in MB (assuming 4KB blocks).
pub fn create_block_cache(capacity_mb: usize) -> Arc<BlockCache> {
    let capacity = capacity_mb * 256;
    let capacity = capacity.max(256); // minimum 1MB
    Arc::new(RwLock::new(LruCache::new(
        NonZeroUsize::new(capacity).unwrap_or(NonZeroUsize::MIN),
    )))
}

/// Block size for SSTable data blocks (4KB).
const BLOCK_SIZE: usize = 4096;

/// A builder for SSTable data blocks.
pub struct BlockBuilder {
    data: BytesMut,
    offsets: Vec<u16>,
    block_size: usize,
    bloom: u64,
}

impl BlockBuilder {
    pub fn new(block_size: usize) -> Self {
        Self {
            data: BytesMut::new(),
            offsets: Vec::new(),
            block_size,
            bloom: 0,
        }
    }

    fn update_bloom(&mut self, key: &[u8]) {
        let hash = blake3::hash(key);
        let bytes = hash.as_bytes();
        // Use 4 x 11-bit chunks from the 256-bit hash for Bloom filter bits (64-bit filter)
        // Safety: blake3 outputs 32 bytes, i * 2 + 1 is max 7.
        for i in 0..4 {
            let chunk = u16::from_le_bytes([
                *bytes.get(i * 2).unwrap_or(&0),
                *bytes.get(i * 2 + 1).unwrap_or(&0),
            ]);
            let bit = chunk % 64;
            self.bloom |= 1 << bit;
        }
    }

    pub fn add(&mut self, key: &[u8], value: &[u8], seq_no: u64) -> bool {
        // size: key_len(2) + key + seq_no(8) + val_len(2) + value + bloom(8) + offsets + offset count (2 bytes)
        if !self.data.is_empty()
            && self.current_size() + key.len() + value.len() + 12 > self.block_size
        {
            return false;
        }

        self.update_bloom(key);
        self.offsets.push(self.data.len() as u16);
        self.data.put_u16_le(key.len() as u16);
        self.data.put_slice(key);
        self.data.put_u64_le(seq_no);
        self.data.put_u16_le(value.len() as u16);
        self.data.put_slice(value);
        true
    }

    pub fn current_size(&self) -> usize {
        // data + bloom(8) + offsets + offset count (2 bytes)
        self.data.len() + 8 + self.offsets.len() * 2 + 2
    }

    pub fn is_empty(&self) -> bool {
        self.offsets.is_empty()
    }

    /// Finalizes the block and returns the bytes.
    pub fn build(mut self) -> Bytes {
        self.data.put_u64_le(self.bloom);
        for &offset in &self.offsets {
            self.data.put_u16_le(offset);
        }
        self.data.put_u16_le(self.offsets.len() as u16);
        self.data.freeze()
    }
}

/// Metadata for an SSTable.
pub struct SstableMetadata {
    pub first_key: Bytes,
    pub last_key: Bytes,
    pub file_size: u64,
}

/// A builder for creating new SSTables.
pub struct SstableBuilder {
    file: File,
    block_builder: BlockBuilder,
    index: Vec<(Bytes, u64)>, // (last_key, offset)
    first_key: Option<Bytes>,
    last_key: Option<Bytes>,
    offset: u64,
    key_manager: Option<Arc<KeyManager>>,
}

impl SstableBuilder {
    /// Creates a new SstableBuilder that writes to the given file path.
    pub async fn create(path: impl AsRef<Path>) -> Result<Self> {
        Self::create_with_key_manager(path, None).await
    }

    pub async fn create_with_key_manager(
        path: impl AsRef<Path>,
        key_manager: Option<Arc<KeyManager>>,
    ) -> Result<Self> {
        let file = File::create(path)
            .await
            .map_err(|e| MemFuseError::Storage(format!("Failed to create SSTable: {}", e)))?;

        Ok(Self {
            file,
            block_builder: BlockBuilder::new(BLOCK_SIZE),
            index: Vec::new(),
            first_key: None,
            last_key: None,
            offset: 0,
            key_manager,
        })
    }

    /// Adds a key-value pair to the SSTable being built.
    pub async fn add(&mut self, key: &[u8], value: &[u8], seq_no: u64) -> Result<()> {
        if self.first_key.is_none() {
            self.first_key = Some(Bytes::copy_from_slice(key));
        }

        if !self.block_builder.add(key, value, seq_no) {
            self.flush_block().await?;
            let _ = self.block_builder.add(key, value, seq_no);
        }

        self.last_key = Some(Bytes::copy_from_slice(key));
        Ok(())
    }

    async fn flush_block(&mut self) -> Result<()> {
        if self.block_builder.is_empty() {
            return Ok(());
        }

        let last_key = self
            .last_key
            .clone()
            .ok_or_else(|| MemFuseError::Storage("Missing last_key".into()))?;
        let mut block =
            std::mem::replace(&mut self.block_builder, BlockBuilder::new(BLOCK_SIZE)).build();

        if let Some(km) = &self.key_manager {
            let encrypted = km.encrypt(&block, self.offset)?;
            block = Bytes::from(encrypted);
        }

        let block_len = block.len() as u64;

        self.file
            .write_all(&block)
            .await
            .map_err(|e| MemFuseError::Storage(format!("SSTable block write failed: {}", e)))?;

        self.index.push((last_key, self.offset));
        self.offset += block_len;
        Ok(())
    }

    /// Finalizes the SSTable and returns metadata.
    pub async fn finish(mut self) -> Result<SstableMetadata> {
        self.flush_block().await?;

        let index_offset = self.offset;
        let mut index_builder = BytesMut::new();

        for (key, offset) in &self.index {
            index_builder.put_u16_le(key.len() as u16);
            index_builder.put_slice(key);
            index_builder.put_u64_le(*offset);
        }

        let index_bytes = index_builder.freeze();
        self.file
            .write_all(&index_bytes)
            .await
            .map_err(|e| MemFuseError::Storage(format!("SSTable index write failed: {}", e)))?;

        // Write index offset and magic number
        self.file
            .write_u64_le(index_offset)
            .await
            .map_err(|e| MemFuseError::Storage(e.to_string()))?;
        self.file
            .write_u32_le(0x4D465354)
            .await // "MFST" in hex
            .map_err(|e| MemFuseError::Storage(e.to_string()))?;

        self.file
            .sync_all()
            .await
            .map_err(|e| MemFuseError::Storage(e.to_string()))?;

        let file_size = self
            .file
            .metadata()
            .await
            .map_err(|e| MemFuseError::Storage(e.to_string()))?
            .len();

        Ok(SstableMetadata {
            first_key: self.first_key.unwrap_or_default(),
            last_key: self.last_key.unwrap_or_default(),
            file_size,
        })
    }
}

/// A reader for existing SSTables.
pub struct SstableReader {
    mmap: Arc<memmap2::Mmap>,
    index: Vec<(Bytes, u64)>,
    metadata: SstableMetadata,
    /// Byte offset where the index data begins (= end of last block).
    index_offset: u64,
    /// File path of this SSTable (for compaction cleanup).
    file_path: PathBuf,
    /// Unique ID for this SSTable (for cache keys).
    file_id: u64,
    /// Shared block cache.
    block_cache: Arc<BlockCache>,
    key_manager: Option<Arc<KeyManager>>,
}

impl SstableReader {
    /// Opens an existing SSTable file for reading.
    pub async fn open(path: impl AsRef<Path>, block_cache: Arc<BlockCache>) -> Result<Self> {
        Self::open_with_key_manager(path, block_cache, None).await
    }

    pub async fn open_with_key_manager(
        path: impl AsRef<Path>,
        block_cache: Arc<BlockCache>,
        key_manager: Option<Arc<KeyManager>>,
    ) -> Result<Self> {
        let path_buf = path.as_ref().to_path_buf();
        let (mmap, file_size) =
            tokio::task::spawn_blocking(move || -> std::io::Result<(memmap2::Mmap, u64)> {
                let file = std::fs::File::open(&path_buf)?; // std::fs justification: required for memmap2
                let metadata = file.metadata()?;
                let file_size = metadata.len();
                // ANCHOR:SAFETY:MMAP-001 — Memory Mapping of SSTable file
                // WP:WP-4.1 PRIO:1 NEEDS:NONE
                // AGENT:02 DATE:2026-05-16 STATUS:REVIEW
                // BEGRÜNDUNG: SSTables sind im LSM-Tree unveränderlich. Memory Mapping
                // ermöglicht effizienten Zugriff ohne explizite Syscalls.
                #[allow(unsafe_code)]
                let mmap = unsafe { memmap2::Mmap::map(&file)? };
                Ok((mmap, file_size))
            })
            .await
            .map_err(|e| MemFuseError::Storage(format!("Join error: {}", e)))?
            .map_err(|e| MemFuseError::Storage(format!("Mmap failed: {}", e)))?;

        if file_size < 12 {
            return Err(MemFuseError::Storage("SSTable file too small".into()));
        }

        let mmap = Arc::new(mmap);

        // Read index offset and magic from trailer (last 12 bytes)
        let trailer_pos = (file_size - 12) as usize;
        let index_offset = u64::from_le_bytes(
            mmap.get(trailer_pos..trailer_pos + 8)
                .ok_or_else(|| MemFuseError::Storage("invalid trailer offset".into()))?
                .try_into()
                .map_err(|_| MemFuseError::Storage("invalid trailer".into()))?,
        );
        let magic = u32::from_le_bytes(
            mmap.get(trailer_pos + 8..trailer_pos + 12)
                .ok_or_else(|| MemFuseError::Storage("invalid trailer magic".into()))?
                .try_into()
                .map_err(|_| MemFuseError::Storage("invalid trailer".into()))?,
        );

        if magic != 0x4D465354 {
            return Err(MemFuseError::Storage("Invalid SSTable magic number".into()));
        }

        if index_offset + 12 > file_size {
            return Err(MemFuseError::Storage(
                "corrupted SSTable: index_offset out of bounds".into(),
            ));
        }

        // Read index
        let mut index = Vec::new();
        let mut pos = index_offset as usize;
        let index_end = (file_size - 12) as usize;

        while pos + 10 <= index_end {
            let key_len = u16::from_le_bytes(
                mmap.get(pos..pos + 2)
                    .ok_or_else(|| MemFuseError::Storage("corrupted index k_len".into()))?
                    .try_into()
                    .map_err(|_| MemFuseError::Storage("invalid slice".into()))?,
            ) as usize;
            pos += 2;

            if pos + key_len > index_end {
                return Err(MemFuseError::Storage("corrupted SSTable index".into()));
            }
            let key = Bytes::copy_from_slice(
                mmap.get(pos..pos + key_len)
                    .ok_or_else(|| MemFuseError::Storage("corrupted index key".into()))?,
            );
            pos += key_len;

            if pos + 8 > index_end {
                return Err(MemFuseError::Storage("corrupted SSTable index".into()));
            }
            let offset = u64::from_le_bytes(
                mmap.get(pos..pos + 8)
                    .ok_or_else(|| MemFuseError::Storage("corrupted index offset".into()))?
                    .try_into()
                    .map_err(|_| MemFuseError::Storage("invalid slice".into()))?,
            );
            pos += 8;
            index.push((key, offset));
        }

        let last_key = index.last().map(|(k, _)| k.clone()).unwrap_or_default();

        let first_key = if !index.is_empty() {
            let offset = index
                .first()
                .ok_or_else(|| {
                    MemFuseError::Storage("corrupted index: first entry missing".into())
                })?
                .1;
            let next_offset = if index.len() > 1 {
                index
                    .get(1)
                    .ok_or_else(|| {
                        MemFuseError::Storage("corrupted index: second entry missing".into())
                    })?
                    .1
            } else {
                index_offset
            };

            let block_data = Self::read_block_at(&mmap, offset, next_offset, &key_manager)?;
            if block_data.len() < 2 {
                return Err(MemFuseError::Storage("corrupted SSTable block".into()));
            }
            let k_len = u16::from_le_bytes([
                *block_data
                    .first()
                    .ok_or_else(|| MemFuseError::Storage("block too small".into()))?,
                *block_data
                    .get(1)
                    .ok_or_else(|| MemFuseError::Storage("block too small".into()))?,
            ]) as usize;
            if block_data.len() < 2 + k_len {
                return Err(MemFuseError::Storage("corrupted SSTable block".into()));
            }
            Bytes::copy_from_slice(
                block_data
                    .get(2..2 + k_len)
                    .ok_or_else(|| MemFuseError::Storage("corrupted block key".into()))?,
            )
        } else {
            Bytes::new()
        };

        Ok(Self {
            mmap,
            index,
            metadata: SstableMetadata {
                first_key,
                last_key,
                file_size,
            },
            index_offset,
            file_path: path.as_ref().to_path_buf(),
            file_id: {
                static NEXT_FILE_ID: std::sync::atomic::AtomicU64 =
                    std::sync::atomic::AtomicU64::new(1);
                NEXT_FILE_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            },
            block_cache,
            key_manager,
        })
    }

    fn read_block_at(
        mmap: &[u8],
        offset: u64,
        next_offset: u64,
        key_manager: &Option<Arc<KeyManager>>,
    ) -> Result<Bytes> {
        if offset > next_offset || next_offset as usize > mmap.len() {
            return Err(MemFuseError::Storage("Inconsistent block offsets".into()));
        }
        let raw_block = mmap
            .get(offset as usize..next_offset as usize)
            .ok_or_else(|| MemFuseError::Storage("block offset out of bounds".into()))?;
        if let Some(km) = key_manager {
            let decrypted = km.decrypt(raw_block, offset)?;
            Ok(Bytes::from(decrypted))
        } else {
            Ok(Bytes::copy_from_slice(raw_block))
        }
    }

    async fn get_block(&self, offset: u64, next_offset: u64) -> Result<Bytes> {
        let cached = {
            let mut cache = self.block_cache.write();
            cache.get(&(self.file_id, offset)).cloned()
        };

        if let Some(block) = cached {
            Ok(block)
        } else {
            let block = Self::read_block_at(&self.mmap, offset, next_offset, &self.key_manager)?;
            self.block_cache
                .write()
                .put((self.file_id, offset), block.clone());
            Ok(block)
        }
    }

    /// Retrieves a value from the SSTable by key.
    pub async fn get(&self, key: &[u8]) -> Result<Option<(Bytes, u64)>> {
        if key < self.metadata.first_key || key > self.metadata.last_key {
            return Ok(None);
        }

        let idx = match self.index.binary_search_by(|(k, _)| k.as_ref().cmp(key)) {
            Ok(i) => i,
            Err(i) => {
                if i >= self.index.len() {
                    return Ok(None);
                }
                i
            }
        };

        let offset = self
            .index
            .get(idx)
            .ok_or_else(|| MemFuseError::Storage("index out of bounds".into()))?
            .1;
        let next_offset = if idx + 1 < self.index.len() {
            self.index
                .get(idx + 1)
                .ok_or_else(|| MemFuseError::Storage("index out of bounds".into()))?
                .1
        } else {
            self.index_offset
        };

        let block_data = self.get_block(offset, next_offset).await?;

        let n = block_data.len();
        if n < 10 {
            return Err(MemFuseError::Storage("block too small".into()));
        }

        let num_offsets = u16::from_le_bytes(
            block_data
                .get(n.saturating_sub(2)..n)
                .ok_or_else(|| {
                    MemFuseError::Storage("malformed block: missing num_offsets".into())
                })?
                .try_into()
                .map_err(|_| MemFuseError::Storage("invalid slice".into()))?,
        ) as usize;

        let offsets_len = num_offsets.saturating_mul(2);
        if n < offsets_len.saturating_add(10) {
            return Err(MemFuseError::Storage(
                "malformed block: num_offsets too large".into(),
            ));
        }
        let offsets_start = n.saturating_sub(2).saturating_sub(offsets_len);
        let bloom_offset = offsets_start.saturating_sub(8);
        let bloom = u64::from_le_bytes(
            block_data
                .get(bloom_offset..bloom_offset.saturating_add(8))
                .ok_or_else(|| {
                    MemFuseError::Storage("malformed block: missing bloom filter".into())
                })?
                .try_into()
                .map_err(|_| MemFuseError::Storage("invalid slice".into()))?,
        );

        // Bloom check
        let hash = blake3::hash(key);
        let hash_bytes = hash.as_bytes();
        let mut may_contain = true;
        // Safety: blake3 outputs 32 bytes, i * 2 + 1 is max 7.
        for i in 0..4 {
            let chunk = u16::from_le_bytes([
                *hash_bytes.get(i * 2).unwrap_or(&0),
                *hash_bytes.get(i * 2 + 1).unwrap_or(&0),
            ]);
            let bit = chunk % 64;
            if (bloom & (1 << bit)) == 0 {
                may_contain = false;
                break;
            }
        }

        if !may_contain {
            return Ok(None);
        }

        for i in 0..num_offsets {
            let off_pos = offsets_start + i * 2;
            let entry_off = u16::from_le_bytes(
                block_data
                    .get(off_pos..off_pos + 2)
                    .ok_or_else(|| MemFuseError::Storage("malformed block: off_pos".into()))?
                    .try_into()
                    .map_err(|_| MemFuseError::Storage("invalid slice".into()))?,
            ) as usize;

            let mut ep = entry_off;
            let k_len = u16::from_le_bytes(
                block_data
                    .get(ep..ep + 2)
                    .ok_or_else(|| MemFuseError::Storage("malformed block: k_len".into()))?
                    .try_into()
                    .map_err(|_| MemFuseError::Storage("invalid slice".into()))?,
            ) as usize;
            ep += 2;
            let entry_key = block_data
                .get(ep..ep + k_len)
                .ok_or_else(|| MemFuseError::Storage("malformed block: entry_key".into()))?;
            ep += k_len;

            if entry_key == key {
                let seq_no = u64::from_le_bytes(
                    block_data
                        .get(ep..ep + 8)
                        .ok_or_else(|| MemFuseError::Storage("malformed block: seq_no".into()))?
                        .try_into()
                        .map_err(|_| MemFuseError::Storage("invalid slice".into()))?,
                );
                ep += 8;
                let v_len = u16::from_le_bytes(
                    block_data
                        .get(ep..ep + 2)
                        .ok_or_else(|| MemFuseError::Storage("malformed block: v_len".into()))?
                        .try_into()
                        .map_err(|_| MemFuseError::Storage("invalid slice".into()))?,
                ) as usize;
                ep += 2;
                if ep + v_len > block_data.len() {
                    return Err(MemFuseError::Storage(
                        "malformed block: value length out of bounds".into(),
                    ));
                }
                let entry_val = block_data.slice(ep..ep + v_len);
                return Ok(Some((entry_val, seq_no)));
            }
        }
        Ok(None)
    }

    pub fn metadata(&self) -> &SstableMetadata {
        &self.metadata
    }

    /// Iterates over all entries in sorted key order.
    pub async fn iter(&self) -> Result<Vec<(Bytes, Bytes, u64)>> {
        let mut results = Vec::new();
        if self.index.is_empty() {
            return Ok(results);
        }

        for idx in 0..self.index.len() {
            let offset = self
                .index
                .get(idx)
                .ok_or_else(|| MemFuseError::Storage("index out of bounds".into()))?
                .1;
            let next_offset = if idx + 1 < self.index.len() {
                self.index
                    .get(idx + 1)
                    .ok_or_else(|| MemFuseError::Storage("index out of bounds".into()))?
                    .1
            } else {
                self.index_offset
            };

            let block_data = self.get_block(offset, next_offset).await?;

            let n = block_data.len();
            if n < 10 {
                continue;
            }

            let num_offsets = u16::from_le_bytes(
                block_data
                    .get(n.saturating_sub(2)..n)
                    .ok_or_else(|| {
                        MemFuseError::Storage("malformed block: missing num_offsets".into())
                    })?
                    .try_into()
                    .map_err(|_| MemFuseError::Storage("invalid slice".into()))?,
            ) as usize;

            let offsets_len = num_offsets * 2;
            if n < 10 + offsets_len {
                return Err(MemFuseError::Storage(
                    "malformed block: num_offsets too large".into(),
                ));
            }
            let offsets_start = n - 2 - offsets_len;

            for i in 0..num_offsets {
                let off_pos = offsets_start + i * 2;
                let entry_off = u16::from_le_bytes(
                    block_data
                        .get(off_pos..off_pos + 2)
                        .ok_or_else(|| MemFuseError::Storage("malformed block: off_pos".into()))?
                        .try_into()
                        .map_err(|_| MemFuseError::Storage("invalid slice".into()))?,
                ) as usize;

                let mut ep = entry_off;
                let k_len = u16::from_le_bytes(
                    block_data
                        .get(ep..ep + 2)
                        .ok_or_else(|| MemFuseError::Storage("malformed block: k_len".into()))?
                        .try_into()
                        .map_err(|_| MemFuseError::Storage("invalid slice".into()))?,
                ) as usize;
                ep += 2;
                let entry_key = block_data
                    .get(ep..ep + k_len)
                    .ok_or_else(|| MemFuseError::Storage("malformed block: entry_key".into()))?;
                ep += k_len;

                let seq_no = u64::from_le_bytes(
                    block_data
                        .get(ep..ep + 8)
                        .ok_or_else(|| MemFuseError::Storage("malformed block: seq_no".into()))?
                        .try_into()
                        .map_err(|_| MemFuseError::Storage("invalid slice".into()))?,
                );
                ep += 8;
                let v_len = u16::from_le_bytes(
                    block_data
                        .get(ep..ep + 2)
                        .ok_or_else(|| MemFuseError::Storage("malformed block: v_len".into()))?
                        .try_into()
                        .map_err(|_| MemFuseError::Storage("invalid slice".into()))?,
                ) as usize;
                ep += 2;
                let entry_val = block_data
                    .get(ep..ep + v_len)
                    .ok_or_else(|| MemFuseError::Storage("malformed block: value".into()))?;

                results.push((
                    Bytes::copy_from_slice(entry_key),
                    Bytes::copy_from_slice(entry_val),
                    seq_no,
                ));
            }
        }

        Ok(results)
    }

    /// Returns the file path of this SSTable.
    pub fn file_path(&self) -> &std::path::Path {
        &self.file_path
    }

    /// Scans the SSTable for keys starting with the given prefix.
    pub async fn scan_prefix(&self, prefix: &[u8]) -> Result<Vec<(Bytes, Bytes, u64)>> {
        let mut results = Vec::with_capacity(16);

        let start_idx = match self.index.binary_search_by(|(k, _)| k.as_ref().cmp(prefix)) {
            Ok(i) => i,
            Err(i) => {
                if i > 0 {
                    i - 1
                } else {
                    0
                }
            }
        };

        for idx in start_idx..self.index.len() {
            let offset = self
                .index
                .get(idx)
                .ok_or_else(|| MemFuseError::Storage("index out of bounds".into()))?
                .1;
            let next_offset = if idx + 1 < self.index.len() {
                self.index
                    .get(idx + 1)
                    .ok_or_else(|| MemFuseError::Storage("index out of bounds".into()))?
                    .1
            } else {
                self.index_offset
            };

            let block_data = self.get_block(offset, next_offset).await?;

            let n = block_data.len();
            if n < 10 {
                continue;
            }

            let num_offsets = u16::from_le_bytes(
                block_data
                    .get(n.saturating_sub(2)..n)
                    .ok_or_else(|| {
                        MemFuseError::Storage("malformed block: missing num_offsets".into())
                    })?
                    .try_into()
                    .map_err(|_| MemFuseError::Storage("invalid slice".into()))?,
            ) as usize;

            let offsets_len = num_offsets * 2;
            if n < 10 + offsets_len {
                return Err(MemFuseError::Storage(
                    "malformed block: num_offsets too large".into(),
                ));
            }
            let offsets_start = n - 2 - offsets_len;

            let mut broke = false;
            for i in 0..num_offsets {
                let off_pos = offsets_start + i * 2;
                let entry_off = u16::from_le_bytes(
                    block_data
                        .get(off_pos..off_pos + 2)
                        .ok_or_else(|| MemFuseError::Storage("malformed block: off_pos".into()))?
                        .try_into()
                        .map_err(|_| MemFuseError::Storage("invalid slice".into()))?,
                ) as usize;

                let mut ep = entry_off;
                let k_len = u16::from_le_bytes(
                    block_data
                        .get(ep..ep + 2)
                        .ok_or_else(|| MemFuseError::Storage("malformed block: k_len".into()))?
                        .try_into()
                        .map_err(|_| MemFuseError::Storage("invalid slice".into()))?,
                ) as usize;
                ep += 2;
                let entry_key = block_data
                    .get(ep..ep + k_len)
                    .ok_or_else(|| MemFuseError::Storage("malformed block: entry_key".into()))?;
                ep += k_len;

                if !entry_key.starts_with(prefix) && entry_key > prefix {
                    broke = true;
                    break; // Passed prefix lexicographically
                }

                if entry_key.starts_with(prefix) {
                    let seq_no = u64::from_le_bytes(
                        block_data
                            .get(ep..ep + 8)
                            .ok_or_else(|| MemFuseError::Storage("malformed block: seq_no".into()))?
                            .try_into()
                            .map_err(|_| MemFuseError::Storage("invalid slice".into()))?,
                    );
                    ep += 8;
                    let v_len = u16::from_le_bytes(
                        block_data
                            .get(ep..ep + 2)
                            .ok_or_else(|| MemFuseError::Storage("malformed block: v_len".into()))?
                            .try_into()
                            .map_err(|_| MemFuseError::Storage("invalid slice".into()))?,
                    ) as usize;
                    ep += 2;
                    let entry_val = block_data
                        .get(ep..ep + v_len)
                        .ok_or_else(|| MemFuseError::Storage("malformed block: value".into()))?;
                    results.push((
                        Bytes::copy_from_slice(entry_key),
                        Bytes::copy_from_slice(entry_val),
                        seq_no,
                    ));
                }
            }
            if broke {
                break;
            }
        }

        Ok(results)
    }

    /// Scans all entries within a key range.
    pub async fn scan_range(
        &self,
        start: std::ops::Bound<&[u8]>,
        end: std::ops::Bound<&[u8]>,
    ) -> Result<Vec<(Bytes, Bytes, u64)>> {
        use std::ops::Bound;

        let mut results = Vec::with_capacity(16);
        if self.index.is_empty() {
            return Ok(results);
        }

        for idx in 0..self.index.len() {
            let offset = self
                .index
                .get(idx)
                .ok_or_else(|| MemFuseError::Storage("index out of bounds".into()))?
                .1;
            let next_offset = if idx + 1 < self.index.len() {
                self.index
                    .get(idx + 1)
                    .ok_or_else(|| MemFuseError::Storage("index out of bounds".into()))?
                    .1
            } else {
                self.index_offset
            };

            let block_data = self.get_block(offset, next_offset).await?;

            let n = block_data.len();
            if n < 10 {
                continue;
            }

            let num_offsets = u16::from_le_bytes(
                block_data
                    .get(n.saturating_sub(2)..n)
                    .ok_or_else(|| {
                        MemFuseError::Storage("malformed block: missing num_offsets".into())
                    })?
                    .try_into()
                    .map_err(|_| MemFuseError::Storage("invalid slice".into()))?,
            ) as usize;

            let offsets_len = num_offsets * 2;
            if n < 10 + offsets_len {
                return Err(MemFuseError::Storage(
                    "malformed block: num_offsets too large".into(),
                ));
            }
            let offsets_start = n - 2 - offsets_len;

            for i in 0..num_offsets {
                let off_pos = offsets_start + i * 2;
                let entry_off = u16::from_le_bytes(
                    block_data
                        .get(off_pos..off_pos + 2)
                        .ok_or_else(|| MemFuseError::Storage("malformed block: off_pos".into()))?
                        .try_into()
                        .map_err(|_| MemFuseError::Storage("invalid slice".into()))?,
                ) as usize;

                let mut ep = entry_off;
                let k_len = u16::from_le_bytes(
                    block_data
                        .get(ep..ep + 2)
                        .ok_or_else(|| MemFuseError::Storage("malformed block: k_len".into()))?
                        .try_into()
                        .map_err(|_| MemFuseError::Storage("invalid slice".into()))?,
                ) as usize;
                ep += 2;
                let entry_key = block_data
                    .get(ep..ep + k_len)
                    .ok_or_else(|| MemFuseError::Storage("malformed block: entry_key".into()))?;
                ep += k_len;

                // Check start bound
                let after_start = match start {
                    Bound::Included(s) => entry_key >= s,
                    Bound::Excluded(s) => entry_key > s,
                    Bound::Unbounded => true,
                };
                if !after_start {
                    continue;
                }

                // Check end bound
                let before_end = match end {
                    Bound::Included(e) => entry_key <= e,
                    Bound::Excluded(e) => entry_key < e,
                    Bound::Unbounded => true,
                };
                if !before_end {
                    return Ok(results); // Past the range, done
                }

                let seq_no = u64::from_le_bytes(
                    block_data
                        .get(ep..ep + 8)
                        .ok_or_else(|| MemFuseError::Storage("malformed block: seq_no".into()))?
                        .try_into()
                        .map_err(|_| MemFuseError::Storage("invalid slice".into()))?,
                );
                ep += 8;
                let v_len = u16::from_le_bytes(
                    block_data
                        .get(ep..ep + 2)
                        .ok_or_else(|| MemFuseError::Storage("malformed block: v_len".into()))?
                        .try_into()
                        .map_err(|_| MemFuseError::Storage("invalid slice".into()))?,
                ) as usize;
                ep += 2;
                let entry_val = block_data
                    .get(ep..ep + v_len)
                    .ok_or_else(|| MemFuseError::Storage("malformed block: value".into()))?;
                results.push((
                    Bytes::copy_from_slice(entry_key),
                    Bytes::copy_from_slice(entry_val),
                    seq_no,
                ));
            }
        }

        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_block_bloom_filter() {
        let mut builder = BlockBuilder::new(4096);
        builder.add(b"apple", b"red", 1);
        builder.add(b"banana", b"yellow", 2);
        let block = builder.build();

        // 1. Verify format: [entries][u64 bloom][u16 offset1][u16 offset2][u16 num_offsets]
        let n = block.len();
        let num_offsets = u16::from_le_bytes(
            block
                .get(n.saturating_sub(2)..n)
                .expect("test")
                .try_into()
                .expect("test"),
        );
        assert_eq!(num_offsets, 2);

        let bloom_pos = n - 2 - (num_offsets as usize * 2) - 8;
        let bloom = u64::from_le_bytes(
            block
                .get(bloom_pos..bloom_pos + 8)
                .expect("test")
                .try_into()
                .expect("correct length"),
        );
        assert!(bloom > 0);

        // 2. Helper to check bloom
        let check_bloom = |key: &[u8], filter: u64| {
            let hash = blake3::hash(key);
            let bytes = hash.as_bytes();
            // Safety: blake3 outputs 32 bytes.
            for i in 0..4 {
                let chunk = u16::from_le_bytes([
                    *bytes.get(i * 2).unwrap_or(&0),
                    *bytes.get(i * 2 + 1).unwrap_or(&0),
                ]);
                let bit = chunk % 64;
                if (filter & (1 << bit)) == 0 {
                    return false;
                }
            }
            true
        };

        assert!(check_bloom(b"apple", bloom));
        assert!(check_bloom(b"banana", bloom));
        // Might have false positive, but definitely shouldn't have many false positives for random strings
        assert!(!check_bloom(b"cherry", bloom));
    }

    #[tokio::test]
    async fn test_sstable_bloom_integration() {
        let tmp = TempDir::new().expect("temp dir");
        let path = tmp.path().join("test.sst");
        let bc = create_block_cache(1);

        let mut builder = SstableBuilder::create(&path).await.expect("create builder");
        builder.add(b"key1", b"val1", 1).await.expect("add key1");
        builder.add(b"key2", b"val2", 2).await.expect("add key2");
        builder.finish().await.expect("finish builder");

        let reader = SstableReader::open(&path, bc).await.expect("open reader");

        // Positive lookup
        let res = reader.get(b"key1").await.expect("get key1");
        assert_eq!(res.expect("exists").0.as_ref(), b"val1");

        // Negative lookup (should be caught by bloom or range check)
        let res = reader.get(b"nonexistent").await.expect("get nonexistent");
        assert!(res.is_none());
    }

    #[tokio::test]
    async fn test_mmap_read_correct_values() {
        let tmp = TempDir::new().expect("temp dir");
        let path = tmp.path().join("mmap_test.sst");
        let bc = create_block_cache(1);

        let mut builder = SstableBuilder::create(&path).await.expect("create builder");
        for i in 0..100 {
            let key = format!("key-{:03}", i);
            let val = format!("val-{:03}", i);
            builder
                .add(key.as_bytes(), val.as_bytes(), i as u64)
                .await
                .expect("add");
        }
        builder.finish().await.expect("finish");

        let reader = SstableReader::open(&path, bc).await.expect("open");
        for i in 0..100 {
            let key = format!("key-{:03}", i);
            let expected = format!("val-{:03}", i);
            let res = reader
                .get(key.as_bytes())
                .await
                .expect("get")
                .expect("exists");
            assert_eq!(res.0.as_ref(), expected.as_bytes());
            assert_eq!(res.1, i as u64);
        }
    }

    #[tokio::test]
    async fn test_mmap_concurrent_readers() {
        use std::sync::Arc;
        let tmp = TempDir::new().expect("temp dir");
        let path = tmp.path().join("mmap_concurrent.sst");
        let bc = create_block_cache(1);

        let mut builder = SstableBuilder::create(&path).await.expect("create builder");
        for i in 0..100 {
            let key = format!("key-{:03}", i);
            let val = format!("val-{:03}", i);
            builder
                .add(key.as_bytes(), val.as_bytes(), i as u64)
                .await
                .expect("add");
        }
        builder.finish().await.expect("finish");

        let reader = Arc::new(SstableReader::open(&path, bc).await.expect("open"));
        let mut handles = Vec::new();

        for _ in 0..16 {
            let r = Arc::clone(&reader);
            handles.push(tokio::spawn(async move {
                for i in 0..100 {
                    let key = format!("key-{:03}", i);
                    let expected = format!("val-{:03}", i);
                    let res = r.get(key.as_bytes()).await.expect("get").expect("exists");
                    assert_eq!(res.0.as_ref(), expected.as_bytes());
                }
            }));
        }

        for h in handles {
            h.await.expect("task failed");
        }
    }

    #[tokio::test]
    async fn test_sstable_scan_prefix() {
        let tmp = TempDir::new().expect("temp dir");
        let path = tmp.path().join("scan_prefix.sst");
        let bc = create_block_cache(1);

        let mut builder = SstableBuilder::create(&path).await.expect("create builder");
        builder.add(b"apple/1", b"a1", 1).await.expect("add");
        builder.add(b"apple/2", b"a2", 2).await.expect("add");
        builder.add(b"banana/1", b"b1", 3).await.expect("add");
        builder.add(b"cherry/1", b"c1", 4).await.expect("add");
        builder.finish().await.expect("finish");

        let reader = SstableReader::open(&path, bc).await.expect("open");

        let apples = reader.scan_prefix(b"apple/").await.expect("scan");
        assert_eq!(apples.len(), 2);
        assert_eq!(apples[0].0.as_ref(), b"apple/1");
        assert_eq!(apples[1].0.as_ref(), b"apple/2");

        let bananas = reader.scan_prefix(b"banana/").await.expect("scan");
        assert_eq!(bananas.len(), 1);
        assert_eq!(bananas[0].0.as_ref(), b"banana/1");

        let non = reader.scan_prefix(b"zebra").await.expect("scan");
        assert!(non.is_empty());
    }

    #[tokio::test]
    async fn test_sstable_scan_range() {
        use std::ops::Bound;
        let tmp = TempDir::new().expect("temp dir");
        let path = tmp.path().join("scan_range.sst");
        let bc = create_block_cache(1);

        let mut builder = SstableBuilder::create(&path).await.expect("create builder");
        builder.add(b"a", b"1", 1).await.expect("add");
        builder.add(b"b", b"2", 2).await.expect("add");
        builder.add(b"c", b"3", 3).await.expect("add");
        builder.add(b"d", b"4", 4).await.expect("add");
        builder.finish().await.expect("finish");

        let reader = SstableReader::open(&path, bc).await.expect("open");

        // Included Range [b, c]
        let res = reader
            .scan_range(Bound::Included(b"b"), Bound::Included(b"c"))
            .await
            .expect("scan");
        assert_eq!(res.len(), 2);
        assert_eq!(res[0].0.as_ref(), b"b");
        assert_eq!(res[1].0.as_ref(), b"c");

        // Excluded Range (b, d) -> only c
        let res = reader
            .scan_range(Bound::Excluded(b"b"), Bound::Excluded(b"d"))
            .await
            .expect("scan");
        assert_eq!(res.len(), 1);
        assert_eq!(res[0].0.as_ref(), b"c");

        // Unbounded
        let res = reader
            .scan_range(Bound::Unbounded, Bound::Included(b"b"))
            .await
            .expect("scan");
        assert_eq!(res.len(), 2);
        assert_eq!(res[0].0.as_ref(), b"a");
    }
}
