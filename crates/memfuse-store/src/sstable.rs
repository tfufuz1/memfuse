// ANCHOR:DOC:DOC-SSTABLE-001 — Missing module documentation
// WP:WP-0.0 PRIO:3 NEEDS:NONE
// AGENT:02 DATE:2026-05-09 STATUS:READY
// CREATED:2026-05-09 DEADLINE:NONE
// ANCHOR:ARCH:SST-001 — Immutable persistente Datendateien.
// WP:WP-0.0 PRIO:1 NEEDS:NONE
// AGENT:01 DATE:2026-05-09 STATUS:DONE
// CREATED:2026-05-05 DEADLINE:NONE
// FORMAT: [DataBlock 0..N][IndexBlock][u64 index_offset][u32 MAGIC=0x4D465354 "MFST"]
// BLOCK-FORMAT: [entries...][u16 offsets...][u16 num_offsets]
// ENTRY-FORMAT: [u16 key_len][key][u64 seq_no][u16 val_len][value]
// LOOKUP: Binary Search über Index (last_key pro Block) → Block lesen → Linear Scan.
// VERWENDET IN: LsmStorage::get() (point lookup), CompactionEngine::merge_sstables() (full scan).
//
// ANCHOR:SPEC:WP-4.1-BLOOM-001 — Bloom Filter pro Block für schnellere Negative Lookups.
// WP:WP-4.1 PRIO:3 NEEDS:NONE
// AGENT:02 DATE:2026-05-09 STATUS:READY
// CREATED:2026-05-09 DEADLINE:NONE
//! SSTable (Sorted String Table) implementation.
//!
//! SSTables are persistent, immutable files containing sorted key-value pairs.
//! They consist of multiple data blocks and an index block at the end.

use bytes::{BufMut, Bytes, BytesMut};
use lru::LruCache;
use memfuse_core::{MemFuseError, Result};
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
        // ANCHOR:DEBT:DEBT-UNWRAP-SSTABLE-37 — unwrap/expect in production code
        // WP:WP-0.0 PRIO:2 NEEDS:NONE
        // AGENT:02 DATE:2026-05-09 STATUS:READY
        // CREATED:2026-05-09 DEADLINE:NONE
        NonZeroUsize::new(capacity).expect("capacity > 0"), // unwrap
    )))
}

/// Block size for SSTable data blocks (4KB).
const BLOCK_SIZE: usize = 4096;

/// A builder for SSTable data blocks.
pub struct BlockBuilder {
    data: BytesMut,
    offsets: Vec<u16>,
    block_size: usize,
}

impl BlockBuilder {
    pub fn new(block_size: usize) -> Self {
        Self {
            data: BytesMut::new(),
            offsets: Vec::new(),
            block_size,
        }
    }

    pub fn add(&mut self, key: &[u8], value: &[u8], seq_no: u64) -> bool {
        // size: key_len(2) + key + seq_no(8) + val_len(2) + value + offset metadata in builder
        if !self.data.is_empty()
            && self.current_size() + key.len() + value.len() + 12 > self.block_size
        {
            return false;
        }

        self.offsets.push(self.data.len() as u16);
        self.data.put_u16_le(key.len() as u16);
        self.data.put_slice(key);
        self.data.put_u64_le(seq_no);
        self.data.put_u16_le(value.len() as u16);
        self.data.put_slice(value);
        true
    }

    pub fn current_size(&self) -> usize {
        // data + offsets + offset count (2 bytes)
        self.data.len() + self.offsets.len() * 2 + 2
    }

    pub fn is_empty(&self) -> bool {
        self.offsets.is_empty()
    }

    /// Finalizes the block and returns the bytes.
    pub fn build(mut self) -> Bytes {
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
}

impl SstableBuilder {
    pub async fn create(path: impl AsRef<Path>) -> Result<Self> {
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
        })
    }

    /// Adds a key-value pair to the SSTable.
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
        let block =
            std::mem::replace(&mut self.block_builder, BlockBuilder::new(BLOCK_SIZE)).build();
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
            first_key: self.first_key.unwrap_or_default(), // unwrap
            last_key: self.last_key.unwrap_or_default(),   // unwrap
            file_size,
        })
    }
}

/// A reader for existing SSTables.
pub struct SstableReader {
    file: tokio::sync::Mutex<tokio::fs::File>,
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
}

impl SstableReader {
    pub async fn open(path: impl AsRef<Path>, block_cache: Arc<BlockCache>) -> Result<Self> {
        let mut file = tokio::fs::File::open(&path)
            .await
            .map_err(|e| MemFuseError::Storage(format!("Failed to open SSTable: {}", e)))?;

        let metadata = file
            .metadata()
            .await
            .map_err(|e| MemFuseError::Storage(e.to_string()))?;
        let file_size = metadata.len();

        if file_size < 12 {
            return Err(MemFuseError::Storage("SSTable file too small".into()));
        }

        // Read index offset and magic
        use tokio::io::{AsyncReadExt, AsyncSeekExt};
        file.seek(std::io::SeekFrom::End(-12))
            .await
            .map_err(|e| MemFuseError::Storage(e.to_string()))?;

        let index_offset = file
            .read_u64_le()
            .await
            .map_err(|e| MemFuseError::Storage(e.to_string()))?;
        let magic = file
            .read_u32_le()
            .await
            .map_err(|e| MemFuseError::Storage(e.to_string()))?;

        if magic != 0x4D465354 {
            return Err(MemFuseError::Storage("Invalid SSTable magic number".into()));
        }

        // Read index
        file.seek(std::io::SeekFrom::Start(index_offset))
            .await
            .map_err(|e| MemFuseError::Storage(e.to_string()))?;

        let mut index_data = vec![0u8; (file_size - 12 - index_offset) as usize];
        file.read_exact(&mut index_data)
            .await
            .map_err(|e| MemFuseError::Storage(e.to_string()))?;

        let mut index = Vec::new();
        let mut pos = 0;

        while pos + 10 <= index_data.len() {
            let key_len = u16::from_le_bytes(
                index_data[pos..pos + 2]
                    .try_into()
                    .map_err(|_| MemFuseError::Storage("invalid slice".into()))?,
            ) as usize;
            pos += 2;
            let key = Bytes::copy_from_slice(&index_data[pos..pos + key_len]);
            pos += key_len;
            let offset = u64::from_le_bytes(
                index_data[pos..pos + 8]
                    .try_into()
                    .map_err(|_| MemFuseError::Storage("invalid slice".into()))?,
            );
            pos += 8;
            index.push((key, offset));
        }

        let last_key = index.last().map(|(k, _)| k.clone()).unwrap_or_default(); // unwrap

        // Read the actual first key from the first data block header
        // (index stores last_key per block, NOT first_key)
        let mut sync_file = tokio::fs::File::open(&path)
            .await
            .map_err(|e| MemFuseError::Storage(format!("Failed to open SSTable: {}", e)))?;
        let first_key = if !index.is_empty() {
            sync_file
                .seek(tokio::io::SeekFrom::Start(index[0].1))
                .await
                .map_err(|e| MemFuseError::Storage(format!("Seek failed: {}", e)))?;
            let mut hdr = [0u8; 2];
            sync_file
                .read_exact(&mut hdr)
                .await
                .map_err(|e| MemFuseError::Storage(format!("Read failed: {}", e)))?;
            let k_len = u16::from_le_bytes(hdr) as usize;
            let mut k_buf = vec![0u8; k_len];
            sync_file
                .read_exact(&mut k_buf)
                .await
                .map_err(|e| MemFuseError::Storage(format!("Read failed: {}", e)))?;
            Bytes::from(k_buf)
        } else {
            Bytes::new()
        };

        Ok(Self {
            file: tokio::sync::Mutex::new(sync_file),
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
        })
    }

    // ANCHOR:PERF:ALLOC-001 — Allokations-intensiver Scanner
    // WP:WP-4.1 PRIO:2 NEEDS:NONE
    // AGENT:08-perf DATE:2026-05-09 STATUS:READY
    // CREATED:2026-05-09 DEADLINE:NONE
    // TARGET: Zero-Allocation Lookup
    // AKTUELL: Vec::new() pro Block + read_exact
    // BOTTLENECK: Memory Allocator / Heap Churn
    // OPTIMIERUNGSIDEE: SmallVec oder Pool-Buffer
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

        let offset = self.index[idx].1;
        let next_offset = if idx + 1 < self.index.len() {
            self.index[idx + 1].1
        } else {
            self.index_offset
        };

        let mut block_data = Vec::new();
        let mut cache_miss = false;

        {
            let mut cache = self.block_cache.write();
            if let Some(cached) = cache.get(&(self.file_id, offset)) {
                block_data.extend_from_slice(cached);
            } else {
                cache_miss = true;
            }
        }

        if cache_miss {
            let mut raw_block = vec![0u8; (next_offset - offset) as usize];
            use tokio::io::{AsyncReadExt, AsyncSeekExt};
            let mut file = self.file.lock().await;
            file.seek(std::io::SeekFrom::Start(offset))
                .await
                .map_err(|e| MemFuseError::Storage(format!("Seek failed: {}", e)))?;
            file.read_exact(&mut raw_block)
                .await
                .map_err(|e| MemFuseError::Storage(format!("SSTable read failed: {}", e)))?;

            block_data.extend_from_slice(&raw_block);
            self.block_cache
                .write()
                .put((self.file_id, offset), Bytes::from(raw_block));
        }

        let n = block_data.len();
        if n < 2 {
            return Ok(None);
        }

        let num_offsets = u16::from_le_bytes(
            block_data[n - 2..n]
                .try_into()
                .map_err(|_| MemFuseError::Storage("invalid slice".into()))?,
        ) as usize;
        let offsets_start = n - 2 - num_offsets * 2;

        for i in 0..num_offsets {
            let off_pos = offsets_start + i * 2;
            let entry_off = u16::from_le_bytes(
                block_data[off_pos..off_pos + 2]
                    .try_into()
                    .map_err(|_| MemFuseError::Storage("invalid slice".into()))?,
            ) as usize;

            let mut ep = entry_off;
            let k_len = u16::from_le_bytes(
                block_data[ep..ep + 2]
                    .try_into()
                    .map_err(|_| MemFuseError::Storage("invalid slice".into()))?,
            ) as usize;
            ep += 2;
            // ANCHOR:SEC:SLICE-002 — Slice-Indexing ohne Bounds-Check
            // WP:WP-0.0 PRIO:1 NEEDS:NONE
            // AGENT:09-security DATE:2026-05-09 STATUS:READY
            // CREATED:2026-05-09 DEADLINE:NONE
            // FUNDORT: memfuse-store/src/sstable.rs:416
            // RISIKO: Panic bei Runtime durch unzureichende Datei-Länge
            // BEHEBUNG: bounds check vor indexing implementieren
            let entry_key = &block_data[ep..ep + k_len];
            ep += k_len;

            if entry_key == key {
                let seq_no = u64::from_le_bytes(
                    block_data[ep..ep + 8]
                        .try_into()
                        .map_err(|_| MemFuseError::Storage("invalid slice".into()))?,
                );
                ep += 8;
                let v_len = u16::from_le_bytes(
                    block_data[ep..ep + 2]
                        .try_into()
                        .map_err(|_| MemFuseError::Storage("invalid slice".into()))?,
                ) as usize;
                ep += 2;
                return Ok(Some((
                    Bytes::copy_from_slice(&block_data[ep..ep + v_len]),
                    seq_no,
                )));
            }
        }
        Ok(None)
    }

    pub fn metadata(&self) -> &SstableMetadata {
        &self.metadata
    }

    /// Iterates over all entries in sorted key order.
    ///
    /// Returns `(key, value, seq_no)` triples for every entry in the SSTable,
    /// including tombstones. Used by compaction for multi-way merge.
    pub async fn iter(&self) -> Result<Vec<(Bytes, Bytes, u64)>> {
        let mut results = Vec::new();
        if self.index.is_empty() {
            return Ok(results);
        }

        for idx in 0..self.index.len() {
            let offset = self.index[idx].1;
            let next_offset = if idx + 1 < self.index.len() {
                self.index[idx + 1].1
            } else {
                self.index_offset
            };

            let mut block_data = Vec::new();
            let mut cache_miss = false;

            {
                let mut cache = self.block_cache.write();
                if let Some(cached) = cache.get(&(self.file_id, offset)) {
                    block_data.extend_from_slice(cached);
                } else {
                    cache_miss = true;
                }
            }

            if cache_miss {
                let mut raw_block = vec![0u8; (next_offset - offset) as usize];
                use tokio::io::{AsyncReadExt, AsyncSeekExt};
                let mut file = self.file.lock().await;
                file.seek(std::io::SeekFrom::Start(offset))
                    .await
                    .map_err(|e| MemFuseError::Storage(format!("Seek failed: {}", e)))?;
                file.read_exact(&mut raw_block).await.map_err(|e| {
                    MemFuseError::Storage(format!("SSTable iter read failed: {}", e))
                })?;
                block_data.extend_from_slice(&raw_block);
                self.block_cache
                    .write()
                    .put((self.file_id, offset), Bytes::from(raw_block));
            }

            let n = block_data.len();
            if n < 2 {
                continue;
            }

            let num_offsets = u16::from_le_bytes(
                block_data[n - 2..n]
                    .try_into()
                    .map_err(|_| MemFuseError::Storage("invalid slice".into()))?,
            ) as usize;
            let offsets_start = n - 2 - num_offsets * 2;

            for i in 0..num_offsets {
                let off_pos = offsets_start + i * 2;
                let entry_off = u16::from_le_bytes(
                    block_data[off_pos..off_pos + 2]
                        .try_into()
                        .map_err(|_| MemFuseError::Storage("invalid slice".into()))?,
                ) as usize;

                let mut ep = entry_off;
                let k_len = u16::from_le_bytes(
                    block_data[ep..ep + 2]
                        .try_into()
                        .map_err(|_| MemFuseError::Storage("invalid slice".into()))?,
                ) as usize;
                ep += 2;
                let entry_key = &block_data[ep..ep + k_len];
                ep += k_len;

                let seq_no = u64::from_le_bytes(
                    block_data[ep..ep + 8]
                        .try_into()
                        .map_err(|_| MemFuseError::Storage("invalid slice".into()))?,
                );
                ep += 8;
                let v_len = u16::from_le_bytes(
                    block_data[ep..ep + 2]
                        .try_into()
                        .map_err(|_| MemFuseError::Storage("invalid slice".into()))?,
                ) as usize;
                ep += 2;
                let entry_val = &block_data[ep..ep + v_len];

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

    pub async fn scan_prefix(&self, prefix: &[u8]) -> Result<Vec<(Bytes, Bytes, u64)>> {
        let mut results = Vec::new();

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
            let offset = self.index[idx].1;
            let next_offset = if idx + 1 < self.index.len() {
                self.index[idx + 1].1
            } else {
                self.index_offset
            };

            let mut block_data = Vec::new();
            let mut cache_miss = false;

            {
                let mut cache = self.block_cache.write();
                if let Some(cached) = cache.get(&(self.file_id, offset)) {
                    block_data.extend_from_slice(cached);
                } else {
                    cache_miss = true;
                }
            }

            if cache_miss {
                let mut raw_block = vec![0u8; (next_offset - offset) as usize];
                use tokio::io::{AsyncReadExt, AsyncSeekExt};
                let mut file = self.file.lock().await;
                file.seek(std::io::SeekFrom::Start(offset))
                    .await
                    .map_err(|e| MemFuseError::Storage(format!("Seek failed: {}", e)))?;
                file.read_exact(&mut raw_block).await.map_err(|e| {
                    MemFuseError::Storage(format!("SSTable scan read failed: {}", e))
                })?;
                block_data.extend_from_slice(&raw_block);
                self.block_cache
                    .write()
                    .put((self.file_id, offset), Bytes::from(raw_block));
            }

            let n = block_data.len();
            if n < 2 {
                continue;
            }

            let num_offsets = u16::from_le_bytes(
                block_data[n - 2..n]
                    .try_into()
                    .map_err(|_| MemFuseError::Storage("invalid slice".into()))?,
            ) as usize;
            let offsets_start = n - 2 - num_offsets * 2;

            let mut broke = false;
            for i in 0..num_offsets {
                let off_pos = offsets_start + i * 2;
                let entry_off = u16::from_le_bytes(
                    block_data[off_pos..off_pos + 2]
                        .try_into()
                        .map_err(|_| MemFuseError::Storage("invalid slice".into()))?,
                ) as usize;

                let mut ep = entry_off;
                let k_len = u16::from_le_bytes(
                    block_data[ep..ep + 2]
                        .try_into()
                        .map_err(|_| MemFuseError::Storage("invalid slice".into()))?,
                ) as usize;
                ep += 2;
                let entry_key = &block_data[ep..ep + k_len];
                ep += k_len;

                if !entry_key.starts_with(prefix) && entry_key > prefix {
                    broke = true;
                    break; // Passed prefix lexicographically
                }

                if entry_key.starts_with(prefix) {
                    let seq_no = u64::from_le_bytes(
                        block_data[ep..ep + 8]
                            .try_into()
                            .map_err(|_| MemFuseError::Storage("invalid slice".into()))?,
                    );
                    ep += 8;
                    let v_len = u16::from_le_bytes(
                        block_data[ep..ep + 2]
                            .try_into()
                            .map_err(|_| MemFuseError::Storage("invalid slice".into()))?,
                    ) as usize;
                    ep += 2;
                    let entry_val = &block_data[ep..ep + v_len];
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

        let mut results = Vec::new();
        if self.index.is_empty() {
            return Ok(results);
        }

        for idx in 0..self.index.len() {
            let offset = self.index[idx].1;
            let next_offset = if idx + 1 < self.index.len() {
                self.index[idx + 1].1
            } else {
                self.index_offset
            };

            let mut block_data = Vec::new();
            let mut cache_miss = false;

            {
                let mut cache = self.block_cache.write();
                if let Some(cached) = cache.get(&(self.file_id, offset)) {
                    block_data.extend_from_slice(cached);
                } else {
                    cache_miss = true;
                }
            }

            if cache_miss {
                let mut raw_block = vec![0u8; (next_offset - offset) as usize];
                use tokio::io::{AsyncReadExt, AsyncSeekExt};
                let mut file = self.file.lock().await;
                file.seek(std::io::SeekFrom::Start(offset))
                    .await
                    .map_err(|e| MemFuseError::Storage(format!("Seek failed: {}", e)))?;
                file.read_exact(&mut raw_block).await.map_err(|e| {
                    MemFuseError::Storage(format!("SSTable range read failed: {}", e))
                })?;
                block_data.extend_from_slice(&raw_block);
                self.block_cache
                    .write()
                    .put((self.file_id, offset), Bytes::from(raw_block));
            }

            let n = block_data.len();
            if n < 2 {
                continue;
            }

            let num_offsets = u16::from_le_bytes(
                block_data[n - 2..n]
                    .try_into()
                    .map_err(|_| MemFuseError::Storage("invalid slice".into()))?,
            ) as usize;
            let offsets_start = n - 2 - num_offsets * 2;

            for i in 0..num_offsets {
                let off_pos = offsets_start + i * 2;
                let entry_off = u16::from_le_bytes(
                    block_data[off_pos..off_pos + 2]
                        .try_into()
                        .map_err(|_| MemFuseError::Storage("invalid slice".into()))?,
                ) as usize;

                let mut ep = entry_off;
                let k_len = u16::from_le_bytes(
                    block_data[ep..ep + 2]
                        .try_into()
                        .map_err(|_| MemFuseError::Storage("invalid slice".into()))?,
                ) as usize;
                ep += 2;
                let entry_key = &block_data[ep..ep + k_len];
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
                    block_data[ep..ep + 8]
                        .try_into()
                        .map_err(|_| MemFuseError::Storage("invalid slice".into()))?,
                );
                ep += 8;
                let v_len = u16::from_le_bytes(
                    block_data[ep..ep + 2]
                        .try_into()
                        .map_err(|_| MemFuseError::Storage("invalid slice".into()))?,
                ) as usize;
                ep += 2;
                let entry_val = &block_data[ep..ep + v_len];
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
